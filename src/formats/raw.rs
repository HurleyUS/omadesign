//! Full sensor RAW decoding into oriented, camera-white-balanced linear sRGB.
//!
//! LibRaw performs black-level subtraction, demosaicing and the camera matrix.
//! Its 16-bit output deliberately has neither a display transfer function nor
//! automatic brightness. DNG BaselineExposure is reported separately; the Photo
//! pipeline applies it before display conversion. Neither thumbnails nor camera
//! JPEG previews are used. The source buffer is borrowed and never modified.

use omadesign_libraw::Image as NativeImage;
use std::os::raw::c_char;
mod cancellation;
pub use cancellation::Cancellation;
pub mod resources;

use serde::{Deserialize, Serialize};

pub const VERSION: &str = "LibRaw 0.22.2";

/// Legacy general-document bitmap budget; RAW admission uses resource demand.
pub const MAX_PIXELS: u64 = 64_000_000;
pub const EXTENSIONS: &[&str] = &[
    "3fr", "arw", "bay", "cap", "cr2", "cr3", "crw", "dcr", "dcs", "dng", "drf", "erf", "fff",
    "iiq", "k25", "kdc", "mdc", "mef", "mos", "mrw", "nef", "nrw", "orf", "pef", "ptx", "pxn",
    "raf", "raw", "rw2", "rwl", "rwz", "sr2", "srf", "srw", "sti", "x3f",
];

pub fn is_extension(extension: &str) -> bool {
    EXTENSIONS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(extension))
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RawMetadata {
    pub make: String,
    pub model: String,
    pub lens: String,
    pub iso: f32,
    /// Exposure time in seconds.
    pub shutter: f32,
    pub aperture: f32,
    /// Physical focal length in millimeters, when supplied by the camera.
    pub focal_length: f32,
    /// Original LibRaw flip code; the returned pixels are already oriented.
    pub orientation: i32,
    pub camera_wb: [f32; 4],
    /// DNG baseline exposure in stops, not yet applied to `pixels`.
    pub baseline_exposure: f32,
    /// Camera timestamp as parsed by LibRaw; no timezone is inferred.
    pub timestamp: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawImage {
    pub width: u32,
    pub height: u32,
    /// Interleaved, oriented, linear sRGB. Every pixel has exactly 3 channels.
    #[serde(deserialize_with = "restore_samples")]
    pub pixels: std::sync::Arc<Vec<u16>>,
    #[serde(skip)]
    pub(crate) memory: Option<std::sync::Arc<resources::Lease>>,
    pub metadata: RawMetadata,
    pub warnings: Vec<String>,
}

impl RawImage {
    /// Admit restored or constructed linear samples to the shared memory pool.
    /// Args: owning job token. Returns after accounting for samples and bounded previews.
    pub(crate) fn admit_memory(&mut self, cancel: &Cancellation) -> Result<(), String> {
        if self.memory.is_none() {
            let count = u64::from(self.width)
                .checked_mul(u64::from(self.height))
                .ok_or("RAW dimensions overflow")?;
            if count == 0 || count.checked_mul(3) != Some(self.pixels.len() as u64) {
                return Err("Invalid linear RAW samples".into());
            }
            if let Some(lease) = resources::global().retained(&self.pixels) {
                self.memory = Some(lease);
                return Ok(());
            }
            let lease = resources::global().reserve(
                resources::retained_demand(count).ok_or("RAW memory demand overflow")?,
                cancel,
            )?;
            resources::global().retain(&self.pixels, lease.clone());
            self.memory = Some(lease);
        }
        Ok(())
    }
}

/// Restore the existing JSON sample array with admission before each allocation.
/// Args: serde array deserializer. Returns shared samples whose lease follows their last clone.
fn restore_samples<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<std::sync::Arc<Vec<u16>>, D::Error> {
    struct Samples;
    impl<'de> serde::de::Visitor<'de> for Samples {
        type Value = std::sync::Arc<Vec<u16>>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("an admitted array of linear RGB16 samples")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            let cancel = Cancellation::default();
            let lease = resources::global()
                .reserve(40 * 1024 * 1024, &cancel)
                .map_err(A::Error::custom)?;
            let mut values = Vec::new();
            while let Some(sample) = seq.next_element::<u16>()? {
                if values.len() == values.capacity() {
                    let capacity = values
                        .len()
                        .checked_add(65536)
                        .ok_or_else(|| A::Error::custom("RAW sample count overflow"))?;
                    let bytes = (capacity as u64)
                        .checked_mul(2)
                        .and_then(|bytes| bytes.checked_add(40 * 1024 * 1024))
                        .ok_or_else(|| A::Error::custom("RAW memory demand overflow"))?;
                    lease.resize(bytes, &cancel).map_err(A::Error::custom)?;
                    values
                        .try_reserve_exact(capacity - values.len())
                        .map_err(A::Error::custom)?;
                }
                values.push(sample);
            }
            let values = std::sync::Arc::new(values);
            resources::global().retain(&values, lease);
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Samples)
}

/// Release native pixels before their reservation and decoder lock.
/// Args: completed native image, admitted memory and exclusive decoder guard.
/// Returns an owner that preserves this cleanup order on every error path.
struct NativeDecoded {
    image: NativeImage,
    lease: std::sync::Arc<resources::Lease>,
    _decoder: std::sync::MutexGuard<'static, ()>,
}

pub fn decode(bytes: &[u8]) -> Result<RawImage, String> {
    decode_cancellable(bytes, &Cancellation::default())
}

/// Decode untouched camera bytes with active cancellation.
/// Args: encoded source and a job-owned token. Returns oriented linear RGB16 or a decoding/cancellation error.
pub fn decode_cancellable(bytes: &[u8], cancel: &Cancellation) -> Result<RawImage, String> {
    let input = resources::global().reserve(bytes.len() as u64, cancel)?;
    decode_reserved(bytes, cancel, &input)
}

fn native_decode(
    bytes: &[u8],
    cancel: &Cancellation,
    sensor_only: bool,
) -> Result<NativeDecoded, String> {
    cancel.check()?;
    if bytes.is_empty() {
        return Err("RAW input must not be empty".into());
    }
    let _decoder = resources::decoder(cancel)?;
    let admission = Admission {
        cancel,
        lease: resources::global().reserve(64 * 1024 * 1024, cancel)?,
        error: std::sync::Mutex::new(None),
    };
    let active = cancel.native_activity();
    let (result, native) = omadesign_libraw::decode(
        bytes,
        &admission,
        resources::global().usage().1,
        sensor_only,
    );
    drop(active);
    let decoded = NativeDecoded {
        image: native,
        lease: admission.lease,
        _decoder,
    };
    cancel.check()?;
    if let Some(error) = admission
        .error
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
    {
        return Err(error);
    }
    if result != 0 {
        let error = text(&decoded.image.error);
        return Err(format!("Could not decode camera RAW: {error}"));
    }
    Ok(decoded)
}

pub(crate) fn decode_reserved(
    bytes: &[u8],
    cancel: &Cancellation,
    input: &resources::Lease,
) -> Result<RawImage, String> {
    cancel.check()?;
    if input.bytes() < bytes.len() as u64 {
        return Err("RAW input is not covered by its memory reservation".into());
    }
    if bytes.is_empty() {
        return Err("RAW input must not be empty".into());
    }
    let decoded = native_decode(bytes, cancel, false)?;
    let native = &decoded.image;
    let lease = decoded.lease.clone();
    let count = u64::from(native.width) * u64::from(native.height);
    if count == 0
        || count
            .checked_mul(6)
            .is_none_or(|bytes| bytes > isize::MAX as u64)
        || native.bits != 16
        || !matches!(native.colors, 1 | 3)
        || native.data_size != count * u64::from(native.colors) * 2
    {
        return Err("The RAW decoder returned an invalid pixel buffer".into());
    }
    // The bridge allocation stays alive until `decoded` is dropped after this copy.
    let samples = native
        .data()
        .ok_or("The RAW decoder returned an invalid pixel buffer")?;
    let mut pixels: Vec<u16> = Vec::new();
    pixels
        .try_reserve_exact(count as usize * 3)
        .map_err(|_| "Not enough memory to retain the 16-bit RAW image".to_owned())?;
    if native.colors == 3 {
        // Every u16 bit pattern is valid and LibRaw memory images use host
        // byte order, so copy the validated bytes straight into the samples.
        pixels.resize(count as usize * 3, 0);
        copy_bytes_cancellable(samples, bytemuck::cast_slice_mut(&mut pixels), cancel)?;
    } else {
        for (index, sample) in samples.as_chunks::<2>().0.iter().enumerate() {
            if index % 65536 == 0 {
                cancel.check()?;
            }
            let value = u16::from_ne_bytes([sample[0], sample[1]]);
            pixels.extend([value; 3]);
        }
    }
    let mut warnings = Vec::new();
    if native.warnings & (1 << 2) != 0 {
        warnings
            .push("Camera white balance is missing or invalid; daylight balance was used.".into());
    }
    if native.warnings & (1 << 3) != 0 {
        warnings.push("This RAW file does not include complete camera metadata.".into());
    }
    if native.warnings & ((1 << 26) | (1 << 27)) != 0 {
        warnings.push("Some DNG processing instructions were not applied by the decoder.".into());
    } else if native.dng_opcodes != 0 {
        warnings.push("This DNG includes processing instructions; lens corrections and some DNG effects may differ from the camera preview.".into());
    }
    if native.raw_count > 1 {
        warnings.push(format!(
            "This file contains {} RAW frames; the first frame was opened.",
            native.raw_count
        ));
    }
    cancel.check()?;
    let pixels = std::sync::Arc::new(pixels);
    let retained_bytes = resources::retained_demand(count).ok_or("RAW memory demand overflow")?;
    let image = RawImage {
        width: native.width,
        height: native.height,
        pixels: pixels.clone(),
        memory: Some(lease.clone()),
        metadata: RawMetadata {
            make: text(&native.make),
            model: text(&native.model),
            lens: text(&native.lens),
            iso: positive(native.iso),
            shutter: positive(native.shutter),
            aperture: positive(native.aperture),
            focal_length: positive(native.focal_length),
            orientation: native.orientation,
            camera_wb: native.camera_wb.map(positive),
            baseline_exposure: if native.baseline_exposure.is_finite()
                && (-16.0..=16.0).contains(&native.baseline_exposure)
            {
                native.baseline_exposure
            } else {
                0.0
            },
            timestamp: native.timestamp,
        },
        warnings,
    };
    drop(decoded);
    lease.resize(retained_bytes, cancel)?;
    resources::global().retain(&pixels, lease.clone());
    Ok(image)
}

struct Admission<'a> {
    cancel: &'a Cancellation,
    lease: std::sync::Arc<resources::Lease>,
    error: std::sync::Mutex<Option<String>>,
}

impl omadesign_libraw::Host for Admission<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    fn progress(&self, stage: u32) {
        self.cancel.record_stage(stage);
    }

    fn reserve(&self, bytes: u64) -> bool {
        self.cancel.record_native_memory_request(bytes);
        match self.lease.resize(bytes, self.cancel) {
            Ok(()) => true,
            Err(error) => {
                *self.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(error);
                false
            }
        }
    }
}

/// Copy native bytes in 4 MiB chunks, checking cancellation between chunks.
/// Args: source bytes and an equally long destination. Returns a cancellation error.
fn copy_bytes_cancellable(source: &[u8], target: &mut [u8], cancel: &Cancellation) -> Result<(), String> {
    const CHUNK: usize = 4 * 1024 * 1024;
    for (from, to) in source.chunks(CHUNK).zip(target.chunks_mut(CHUNK)) {
        cancel.check()?;
        to.copy_from_slice(from);
    }
    Ok(())
}

fn positive(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

fn text<const N: usize>(field: &[c_char; N]) -> String {
    let bytes: Vec<u8> = field
        .iter()
        .take_while(|&&byte| byte != 0)
        .map(|&byte| byte.to_ne_bytes()[0])
        .collect();
    String::from_utf8_lossy(&bytes)
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
#[path = "raw/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "raw/qualification.rs"]
mod qualification;

#[cfg(test)]
struct SensorImage {
    width: u32,
    height: u32,
    samples: std::sync::Arc<resources::Allocation<u16>>,
}

#[cfg(test)]
fn decode_sensor(bytes: &[u8], cancel: &Cancellation) -> Result<SensorImage, String> {
    let _input = resources::global().reserve(bytes.len() as u64, cancel)?;
    let decoded = native_decode(bytes, cancel, true)?;
    let native = &decoded.image;
    let lease = decoded.lease.clone();
    let count = u64::from(native.width) * u64::from(native.height);
    if count == 0
        || count.checked_mul(2) != Some(native.data_size)
        || native.colors != 1
        || native.bits != 16
    {
        return Err("Invalid sensor reference plane".into());
    }
    let mut values: Vec<u16> = Vec::new();
    values
        .try_reserve_exact(count as usize)
        .map_err(|_| "Not enough memory for the sensor reference")?;
    // The native allocation stays alive until `decoded` is dropped below.
    let bytes = native.data().ok_or("Invalid sensor reference plane")?;
    values.resize(count as usize, 0);
    copy_bytes_cancellable(bytes, bytemuck::cast_slice_mut(&mut values), cancel)?;
    let (width, height) = (native.width, native.height);
    drop(decoded);
    lease.resize(values.capacity() as u64 * 2, cancel)?;
    let samples = resources::global().adopt(values, lease)?;
    Ok(SensorImage {
        width,
        height,
        samples,
    })
}
