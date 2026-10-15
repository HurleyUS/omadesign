//! Native document samples. RGBA8 is a display cache; authored samples remain authoritative.
mod composite;
pub use composite::{blend_pixel, composite};

use crate::document::Pixels;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Depth {
    #[default]
    U8,
    U16,
    F32,
}
impl Depth {
    /// Photoshop sample width.
    /// Args: self. Returns bits per component.
    pub fn bits(self) -> u16 {
        match self {
            Self::U8 => 8,
            Self::U16 => 16,
            Self::F32 => 32,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NativePixels {
    pub depth: Depth,
    #[serde(with = "sample_bytes")]
    pub samples: Vec<f32>,
}

impl Pixels {
    /// Construct authoritative RGBA samples and their display cache.
    /// Args: dimensions, depth and straight RGBA samples. Returns validated pixels or an error.
    pub fn from_samples(w: u32, h: u32, depth: Depth, samples: Vec<f32>) -> Result<Self, String> {
        let count = (w as usize)
            .checked_mul(h as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or("Native pixel dimensions overflow")?;
        if w == 0
            || h == 0
            || count > 128 * 1024 * 1024
            || samples.len() != count
            || samples.iter().any(|s| !s.is_finite())
        {
            return Err("Invalid or excessive native pixel samples".into());
        }
        let data = samples.iter().map(|&s| byte(s)).collect();
        let mut out = Self::from_rgba(w, h, data).ok_or("Invalid native pixel dimensions")?;
        out.native = Some(NativePixels { depth, samples });
        Ok(out)
    }

    /// Read straight samples without quantizing them.
    /// Args: self. Returns a copy in RGBA order, including out-of-range float color values.
    pub fn samples(&self) -> Vec<f32> {
        self.native
            .as_ref()
            .map(|n| n.samples.clone())
            .unwrap_or_else(|| self.data.iter().map(|&s| s as f32 / 255.).collect())
    }

    /// Read one authored pixel.
    /// Args: zero-based pixel index. Returns straight RGBA or transparent outside the image.
    pub fn sample(&self, i: usize) -> [f32; 4] {
        if let Some(n) = &self.native {
            n.samples
                .get(i * 4..i * 4 + 4)
                .and_then(|v| v.try_into().ok())
                .unwrap_or([0.; 4])
        } else {
            self.data
                .get(i * 4..i * 4 + 4)
                .map(|v| {
                    [
                        v[0] as f32 / 255.,
                        v[1] as f32 / 255.,
                        v[2] as f32 / 255.,
                        v[3] as f32 / 255.,
                    ]
                })
                .unwrap_or([0.; 4])
        }
    }

    /// Replace an edited pixel buffer while retaining its precision.
    /// Args: straight samples. Returns an error for malformed or nonfinite samples.
    pub fn set_samples(&mut self, samples: Vec<f32>) -> Result<(), String> {
        let depth = self.native.as_ref().map_or(Depth::F32, |n| n.depth);
        let mut out = Self::from_samples(self.w, self.h, depth, samples)?;
        out.version = self.version.wrapping_add(1);
        *self = out;
        Ok(())
    }

    /// Apply a mask in authored precision.
    /// Args: a matching luminance mask. Returns a new image with multiplied alpha.
    pub fn masked(&self, mask: &Pixels) -> Result<Self, String> {
        if (self.w, self.h) != (mask.w, mask.h) {
            return Err("Mask dimensions do not match pixels".into());
        }
        let mut samples = self.samples();
        for (i, p) in samples.chunks_exact_mut(4).enumerate() {
            let m = mask.sample(i);
            p[3] *= ((m[0] * 0.2126 + m[1] * 0.7152 + m[2] * 0.0722) * m[3]).clamp(0., 1.);
        }
        Self::from_samples(
            self.w,
            self.h,
            self.native.as_ref().map_or(Depth::F32, |n| n.depth),
            samples,
        )
    }

    /// Apply a native raster filter.
    /// Args: filter settings and optional byte coverage selection. Returns a full-precision edited buffer.
    pub fn filtered(
        &self,
        settings: &crate::raster::Settings,
        selection: Option<&[u8]>,
    ) -> Result<Self, String> {
        let src: Vec<[f32; 4]> = self
            .samples()
            .chunks_exact(4)
            .map(|p| p.try_into().unwrap())
            .collect();
        let samples = crate::raster::apply_float(&src, self.w, self.h, settings, selection)?
            .into_iter()
            .flatten()
            .collect();
        Self::from_samples(
            self.w,
            self.h,
            self.native.as_ref().map_or(Depth::F32, |n| n.depth),
            samples,
        )
    }
}

pub(crate) fn byte(v: f32) -> u8 {
    (v.clamp(0., 1.) * 255.).round() as u8
}

mod sample_bytes {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};
    use std::io::Read;
    pub fn serialize<S: Serializer>(samples: &[f32], s: S) -> Result<S::Ok, S::Error> {
        let bytes: Vec<u8> = samples.iter().flat_map(|v| v.to_le_bytes()).collect();
        let packed =
            zstd::stream::encode_all(bytes.as_slice(), 3).map_err(serde::ser::Error::custom)?;
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(packed))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<f32>, D::Error> {
        let text = String::deserialize(d)?;
        if text.len() > 720 * 1024 * 1024 {
            return Err(serde::de::Error::custom("Native sample data exceeds limit"));
        }
        let packed = base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(serde::de::Error::custom)?;
        let mut raw = Vec::new();
        zstd::stream::read::Decoder::new(packed.as_slice())
            .map_err(serde::de::Error::custom)?
            .take(512 * 1024 * 1024 + 1)
            .read_to_end(&mut raw)
            .map_err(serde::de::Error::custom)?;
        if raw.len() > 512 * 1024 * 1024 || raw.len() % 4 != 0 {
            return Err(serde::de::Error::custom("Invalid native sample length"));
        }
        let samples: Vec<f32> = raw
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        if samples.iter().any(|s| !s.is_finite()) {
            return Err(serde::de::Error::custom("Nonfinite native sample"));
        }
        Ok(samples)
    }
}

#[cfg(test)]
mod tests;
