//! Admitted RGB/CMYK TIFF interchange retaining native samples, alpha and embedded profiles.
use super::{Depth, storage};
use crate::resources;
use std::sync::Arc;
use crate::document::Pixels;
use std::io::Cursor;
use tiff::{
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
    let mut out = storage::Encoded::new()?;
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
                storage::collect(inks.iter().map(|v| super::byte(*v)))?.as_slice()
            ),
            Depth::U16 => write!(
                colortype::CMYK16,
                storage::collect(inks.iter().map(|v| (v.clamp(0., 1.) * 65535.).round() as u16))?.as_slice()
            ),
            Depth::F32 => write!(colortype::CMYK32Float, inks.as_slice()),
        }
    }
    let output = Arc::try_unwrap(out.finish()?).map_err(|_| "Shared TIFF output")?;
    let (bytes, _memory) = output.into_parts();
    Ok(bytes)
}

/// Read RGB/CMYK TIFF directly into admitted native storage.
/// Args: encoded bytes. Returns a matching TIFF image or None for a different format/model.
pub fn decode(bytes: &[u8]) -> Result<Option<Pixels>, String> {
    if ![b"II\x2a\0",b"MM\0\x2a",b"II\x2b\0",b"MM\0\x2b"].iter().any(|magic| bytes.starts_with(*magic)) {return Ok(None);}
    let _metadata = resources::global().reserve_now(16 * 1024 * 1024,&storage::check)?;
    let mut decoder = tiff::decoder::Decoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?.with_limits(tiff::decoder::Limits {ifd_value_size:16 * 1024 * 1024,..Default::default()});
    let color = decoder.colortype().map_err(|e| e.to_string())?;
    let (channels,cmyk) = match color {tiff::ColorType::CMYK(_) => (4,true),tiff::ColorType::CMYKA(_) => (5,true),tiff::ColorType::RGB(_) => (3,false),tiff::ColorType::RGBA(_) => (4,false),_ => return Ok(None)};
    let (w,h) = decoder.dimensions().map_err(|e| e.to_string())?;
    let count = storage::count(w,h,channels)?;
    let layout = decoder.image_buffer_layout().map_err(|e| e.to_string())?;
    let (depth,bps) = match (layout.sample_format,color.bit_depth()) {
        (tiff::tags::SampleFormat::Uint,8) => (Depth::U8,1),
        (tiff::tags::SampleFormat::Uint,16) => (Depth::U16,2),
        (tiff::tags::SampleFormat::IEEEFP,32) => (Depth::F32,4),
        _ => return Err("TIFF sample type cannot be represented without precision loss".into()),
    };
    let raw_bytes = count.checked_mul(bps).ok_or("TIFF sample bytes overflow")?;
    if layout.complete_len < raw_bytes || !(layout.planes == 1 || layout.planes == channels) {return Err("Invalid TIFF sample layout".into());}
    let row_stride = layout.row_stride.ok_or("Missing TIFF row layout")?.get();
    let plane_stride = layout.plane_stride.map_or(layout.complete_len,|s| s.get());
    let rows = decoder.get_tag_u32(Tag::RowsPerStrip).unwrap_or(h).min(h);
    let tile = decoder.get_tag_u32(Tag::TileWidth).ok().zip(decoder.get_tag_u32(Tag::TileLength).ok());
    let chunk = if let Some((tw,th)) = tile {storage::count(tw,th,channels)?} else {storage::count(w,rows,channels)?};
    let compression = decoder.get_tag_u32(Tag::Compression).unwrap_or(1);
    let scratch = if compression == 1 {64 * 1024} else {chunk.checked_mul(bps).and_then(|n| n.checked_add(bytes.len())).ok_or("TIFF codec working bytes overflow")?};
    let _codec = resources::global().reserve_now(scratch as u64,&storage::check)?;
    let limits = tiff::decoder::Limits {decoding_buffer_size:layout.complete_len,intermediate_buffer_size:scratch.max(64 * 1024),ifd_value_size:16 * 1024 * 1024};
    decoder = decoder.with_limits(limits);
    let profile = decoder.find_tag(Tag::IccProfile).map_err(|e| e.to_string())?.map(|v| v.into_u8_vec()).transpose().map_err(|e| e.to_string())?.unwrap_or_default();
    let associated = decoder.find_tag_unsigned_vec::<u16>(Tag::ExtraSamples).map_err(|e| e.to_string())?.is_some_and(|v| v.first() == Some(&1));
    let mut raw = resources::global().allocate_now::<u8,_>(layout.complete_len,&storage::check)?;
    decoder.read_image_bytes(raw.as_mut_slice()).map_err(|e| e.to_string())?;
    let out_channels = if cmyk {5} else {4};
    let mut samples = resources::global().allocate_now::<f32,_>(storage::count(w,h,out_channels)?,&storage::check)?;
    for (i,dst) in samples.as_mut_slice().chunks_exact_mut(out_channels).enumerate() {
        let (x,y) = (i % w as usize,i / w as usize);
        for c in 0..channels {
            let row = y.checked_mul(row_stride).ok_or("TIFF row offset overflows")?;
            let offset = if layout.planes == 1 {x.checked_mul(channels).and_then(|x| x.checked_add(c)).and_then(|x| x.checked_mul(bps)).and_then(|x| x.checked_add(row))} else {c.checked_mul(plane_stride).and_then(|n| n.checked_add(row)).and_then(|n| x.checked_mul(bps).and_then(|x| n.checked_add(x)))}.ok_or("TIFF sample offset overflows")?;
            let src = raw.get(offset..offset+bps).ok_or("TIFF pixel lies outside decoded layout")?;
            dst[c] = match depth {Depth::U8 => src[0] as f32 / 255.,Depth::U16 => u16::from_ne_bytes(src.try_into().unwrap()) as f32 / 65535.,Depth::F32 => f32::from_ne_bytes(src.try_into().unwrap())};
        }
        if channels < out_channels {dst[out_channels-1] = 1.;}
        if associated {let alpha = dst[out_channels-1];for c in &mut dst[..out_channels-1] {*c = if alpha > 0. {*c / alpha} else {0.};}}
    }
    drop(raw);drop(_codec);drop(decoder);
    if cmyk {Pixels::from_cmyk_allocation(w,h,depth,Arc::new(samples),profile,1).map(Some)}
    else {
        let mut pixels = if depth == Depth::U8 && profile.is_empty() {Pixels::from_byte_allocation(w,h,storage::collect(samples.iter().map(|&v| super::byte(v)))?)?} else {Pixels::from_sample_allocation(w,h,depth,Arc::new(samples))?};
        if !profile.is_empty() {pixels.assign_rgb_profile(profile,1)?;}
        Ok(Some(pixels))
    }
}
