//! Profile conversion through Little CMS 2. Authored samples are never replaced by display bytes.
use super::Depth;
use serde::{Deserialize, Serialize};
use std::ffi::c_void;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Model {
    #[default]
    Rgb,
    Cmyk,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentColor {
    pub model: Model,
    pub depth: Depth,
    pub icc_profile: Vec<u8>,
    pub intent: u32,
}

/// Check an embedded profile before native transformation.
/// Args: ICC bytes and expected authoring model. Returns an error for mismatched or malformed data.
pub fn validate_profile(profile: &[u8], model: Model) -> Result<(), String> {
    if profile.is_empty() {
        return Ok(());
    }
    if profile.len() < 128
        || profile.len() > 16 * 1024 * 1024
        || profile.get(36..40) != Some(b"acsp")
        || u32::from_be_bytes(profile[..4].try_into().unwrap()) as usize != profile.len()
        || profile.get(16..20)
            != Some(match model {
                Model::Rgb => b"RGB ",
                Model::Cmyk => b"CMYK",
            })
    {
        return Err(
            "Embedded ICC profile is invalid or does not match the document color model".into(),
        );
    }
    Ok(())
}

/// Convert authored RGB/CMYK color components using an embedded ICC profile.
/// Args: interleaved color samples without alpha, source model, profile, direction and intent.
/// Returns transformed color components; untagged CMYK uses an explicitly uncalibrated device formula.
pub fn transform(
    samples: &[f32],
    model: Model,
    profile: &[u8],
    to_display: bool,
    intent: u32,
) -> Result<Vec<f32>, String> {
    validate_profile(profile, model)?;
    let native = if model == Model::Cmyk { 4 } else { 3 };
    let input = if to_display { native } else { 3 };
    let output = if to_display { 3 } else { native };
    if samples.len() % input != 0 || samples.iter().any(|v| !v.is_finite()) || intent > 3 {
        return Err("Invalid ICC transform samples or rendering intent".into());
    }
    if profile.is_empty() {
        return Ok(if model == Model::Rgb {
            samples.to_vec()
        } else if to_display {
            samples
                .chunks_exact(4)
                .flat_map(|p| {
                    [
                        (1. - p[0]) * (1. - p[3]),
                        (1. - p[1]) * (1. - p[3]),
                        (1. - p[2]) * (1. - p[3]),
                    ]
                })
                .collect()
        } else {
            samples
                .chunks_exact(3)
                .flat_map(|p| {
                    let k = 1. - p[0].max(p[1]).max(p[2]).clamp(0., 1.);
                    let d = 1. - k;
                    if d < 1e-8 {
                        [0., 0., 0., 1.]
                    } else {
                        [
                            (1. - p[0] - k) / d,
                            (1. - p[1] - k) / d,
                            (1. - p[2] - k) / d,
                            k,
                        ]
                    }
                })
                .collect()
        });
    }
    let mut converted = vec![0f32; samples.len() / input * output];
    let api = Api::load()?;
    unsafe {
        let original = (api.open)(profile.as_ptr().cast(), profile.len() as u32);
        let srgb = (api.srgb)();
        if original.is_null() || srgb.is_null() {
            if !original.is_null() {
                (api.close)(original);
            }
            if !srgb.is_null() {
                (api.close)(srgb);
            }
            return Err("Little CMS rejected the embedded ICC profile".into());
        }
        let fmt = |channels: usize| {
            (1u32 << 22)
                | ((if channels == 4 { 6 } else { 4 }) << 16)
                | ((channels as u32) << 3)
                | 4
        };
        let (src, dst) = if to_display {
            (original, srgb)
        } else {
            (srgb, original)
        };
        let transform = (api.create)(src, fmt(input), dst, fmt(output), intent, 0x0100);
        if transform.is_null() {
            (api.close)(original);
            (api.close)(srgb);
            return Err("The ICC profile lacks the requested color transform".into());
        }
        let scaled: Vec<f32> = samples
            .iter()
            .map(|&v| if input == 4 { v * 100. } else { v })
            .collect();
        (api.apply)(
            transform,
            scaled.as_ptr().cast(),
            converted.as_mut_ptr().cast(),
            (samples.len() / input) as u32,
        );
        (api.delete)(transform);
        (api.close)(original);
        (api.close)(srgb);
        if output == 4 {
            for value in &mut converted {
                *value /= 100.;
            }
        }
    }
    if converted.iter().any(|v| !v.is_finite()) {
        return Err("ICC transform produced nonfinite samples".into());
    }
    Ok(converted)
}

type Handle = *mut c_void;
struct Api {
    _library: libloading::Library,
    open: unsafe extern "C" fn(*const c_void, u32) -> Handle,
    srgb: unsafe extern "C" fn() -> Handle,
    close: unsafe extern "C" fn(Handle) -> i32,
    create: unsafe extern "C" fn(Handle, u32, Handle, u32, u32, u32) -> Handle,
    apply: unsafe extern "C" fn(Handle, *const c_void, *mut c_void, u32),
    delete: unsafe extern "C" fn(Handle),
}
impl Api {
    fn load() -> Result<Self, String> {
        let mut loaded = None;
        for name in [
            "liblcms2.so.2",
            "liblcms2.so",
            "liblcms2.2.dylib",
            "lcms2.dll",
        ] {
            if let Ok(library) = unsafe { libloading::Library::new(name) } {
                loaded = Some(library);
                break;
            }
        }
        let library=loaded.ok_or("Color conversion needs Little CMS 2 (lcms2). Original samples and profile remain preserved in the source file.")?;
        unsafe {
            Ok(Self {
                open: *library
                    .get(b"cmsOpenProfileFromMem\0")
                    .map_err(|e| e.to_string())?,
                srgb: *library
                    .get(b"cmsCreate_sRGBProfile\0")
                    .map_err(|e| e.to_string())?,
                close: *library
                    .get(b"cmsCloseProfile\0")
                    .map_err(|e| e.to_string())?,
                create: *library
                    .get(b"cmsCreateTransform\0")
                    .map_err(|e| e.to_string())?,
                apply: *library
                    .get(b"cmsDoTransform\0")
                    .map_err(|e| e.to_string())?,
                delete: *library
                    .get(b"cmsDeleteTransform\0")
                    .map_err(|e| e.to_string())?,
                _library: library,
            })
        }
    }
}

impl crate::document::Document {
    /// Prepare one undoable document color conversion.
    /// Args: target authoring mode, depth, ICC profile and intent. Returns commands for every raster and the document settings.
    pub fn color_conversion(&self, color: DocumentColor) -> Result<crate::document::Cmd, String> {
        validate_profile(&color.icc_profile, color.model)?;
        let mut commands = Vec::new();
        for (index, layer) in self.layers.iter().enumerate() {
            if let Some(before) = layer.kind.pixels() {
                let mut after = before.in_document_color(&color)?;
                if let Some(native) = &mut after.native {
                    native.depth = color.depth;
                }
                if color.depth == Depth::U8 {
                    if let Some(inks) = after.native.as_ref().and_then(|n| n.cmyk.as_ref()) {
                        after = crate::document::Pixels::from_cmyk(
                            after.w,
                            after.h,
                            Depth::U8,
                            inks.iter().map(|v| super::byte(*v) as f32 / 255.).collect(),
                            color.icc_profile.clone(),
                            color.intent,
                        )?;
                    } else {
                        after.set_samples(
                            after
                                .samples()
                                .into_iter()
                                .map(|v| super::byte(v) as f32 / 255.)
                                .collect(),
                        )?;
                    }
                }
                commands.push(crate::document::Cmd::NativePixels {
                    layer: index,
                    mask: false,
                    before: before.clone(),
                    after,
                });
            }
        }
        commands.push(crate::document::Cmd::SetDocumentColor {
            before: self.color.clone(),
            after: color,
        });
        Ok(crate::document::Cmd::Batch(commands))
    }
}
