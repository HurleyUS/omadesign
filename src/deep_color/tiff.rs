//! CMYK TIFF interchange without an RGB conversion.
use super::Depth;
use crate::document::Pixels;
use std::io::Cursor;
use tiff::{
    decoder::DecodingResult,
    encoder::colortype,
    tags::{ExtraSamples, Tag},
};

/// Encode the document's CMYKA samples in TIFF.
/// Args: authored pixels with retained CMYK and ICC data. Returns TIFF bytes preserving alpha and sample depth.
pub fn encode(pixels: &Pixels) -> Result<Vec<u8>, String> {
    let native = pixels
        .native
        .as_ref()
        .ok_or("TIFF needs native CMYK pixels")?;
    let inks = native
        .cmyk
        .as_ref()
        .ok_or("TIFF needs retained CMYK ink samples")?;
    let mut out = Cursor::new(Vec::new());
    {
        let mut encoder = tiff::encoder::TiffEncoder::new(&mut out).map_err(|e| e.to_string())?;
        macro_rules! write {
            ($type:ty,$data:expr) => {{
                let mut image = encoder
                    .new_image::<$type>(pixels.w, pixels.h)
                    .map_err(|e| e.to_string())?;
                image
                    .extra_samples(&[ExtraSamples::UnassociatedAlpha])
                    .map_err(|e| e.to_string())?;
                if !native.icc_profile.is_empty() {
                    image
                        .encoder()
                        .write_tag(Tag::IccProfile, native.icc_profile.as_slice())
                        .map_err(|e| e.to_string())?;
                }
                image.write_data(&$data).map_err(|e| e.to_string())?;
            }};
        }
        match native.depth {
            Depth::U8 => write!(
                colortype::CMYK8,
                inks.iter().map(|v| super::byte(*v)).collect::<Vec<_>>()
            ),
            Depth::U16 => write!(
                colortype::CMYK16,
                inks.iter()
                    .map(|v| (v.clamp(0., 1.) * 65535.).round() as u16)
                    .collect::<Vec<_>>()
            ),
            Depth::F32 => write!(colortype::CMYK32Float, inks.as_slice()),
        }
    }
    Ok(out.into_inner())
}

/// Read native CMYK TIFF, leaving other image formats to the normal decoder.
/// Args: file bytes. Returns a native CMYK image when the TIFF photometric model is CMYK.
pub fn decode(bytes: &[u8]) -> Result<Option<Pixels>, String> {
    if !bytes.starts_with(b"II\x2a\0") && !bytes.starts_with(b"MM\0\x2a") {
        return Ok(None);
    }
    let mut decoder = tiff::decoder::Decoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let color = decoder.colortype().map_err(|e| e.to_string())?;
    let channels = match color {
        tiff::ColorType::CMYK(_) => 4,
        tiff::ColorType::CMYKA(_) => 5,
        _ => return Ok(None),
    };
    let (w, h) = decoder.dimensions().map_err(|e| e.to_string())?;
    if u64::from(w) * u64::from(h) > 16 * 1024 * 1024 {
        return Err("CMYK TIFF exceeds the native 16 megapixel working limit".into());
    }
    let profile = decoder.get_tag_u8_vec(Tag::IccProfile).unwrap_or_default();
    let (depth, samples) = match decoder.read_image().map_err(|e| e.to_string())? {
        DecodingResult::U8(v) => (
            Depth::U8,
            v.into_iter().map(|v| v as f32 / 255.).collect::<Vec<_>>(),
        ),
        DecodingResult::U16(v) => (
            Depth::U16,
            v.into_iter().map(|v| v as f32 / 65535.).collect(),
        ),
        DecodingResult::F32(v) => (Depth::F32, v),
        _ => {
            return Err(
                "CMYK TIFF sample type is unsupported; no precision conversion was performed"
                    .into(),
            );
        }
    };
    if samples.len() != w as usize * h as usize * channels {
        return Err("Invalid CMYK TIFF sample dimensions".into());
    }
    let inks = samples
        .chunks_exact(channels)
        .flat_map(|p| {
            [
                p[0],
                p[1],
                p[2],
                p[3],
                if channels == 5 { p[4] } else { 1. },
            ]
        })
        .collect();
    Pixels::from_cmyk(w, h, depth, inks, profile, 1).map(Some)
}
