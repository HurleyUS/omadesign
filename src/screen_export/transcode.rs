//! Screen-export variants encoded from the shared rendered PNG: PNG-8, WebP, GIF and quality JPEG.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Variant {
    Png8,
    WebP,
    Gif,
}
impl Variant {
    pub const ALL: [Self; 3] = [Self::Png8, Self::WebP, Self::Gif];
    pub fn label(self) -> &'static str {
        match self {
            Self::Png8 => "PNG-8",
            Self::WebP => "WebP (lossless)",
            Self::Gif => "GIF",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png8 => "png",
            Self::WebP => "webp",
            Self::Gif => "gif",
        }
    }
}

fn decode(png: &[u8]) -> Result<image::RgbaImage, String> {
    Ok(image::load_from_memory_with_format(png, image::ImageFormat::Png).map_err(|e| e.to_string())?.into_rgba8())
}

/// Encode a rendered PNG as a variant. Args: PNG bytes and variant. Returns the new file bytes.
pub fn variant(png: &[u8], variant: Variant) -> Result<Vec<u8>, String> {
    let rgba = decode(png)?;
    let mut out = Vec::new();
    match variant {
        Variant::Png8 => png8(&rgba, &mut out)?,
        Variant::WebP => image::codecs::webp::WebPEncoder::new_lossless(&mut out)
            .encode(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8)
            .map_err(|e| e.to_string())?,
        Variant::Gif => {
            let mut encoder = image::codecs::gif::GifEncoder::new_with_speed(&mut out, 10);
            encoder.encode(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
        }
    }
    Ok(out)
}

/// Encode a rendered PNG as JPEG at a chosen quality, matted on white like the shared JPEG writer.
pub fn jpeg(png: &[u8], quality: u8, dpi: Option<f64>) -> Result<Vec<u8>, String> {
    let rgba = decode(png)?;
    let rgb = image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let a = u32::from(p[3]);
        image::Rgb([0, 1, 2].map(|c| ((u32::from(p[c]) * a + 127) / 255 + 255 - a) as u8))
    });
    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100));
    if let Some(dpi) = dpi {
        encoder.set_pixel_density(image::codecs::jpeg::PixelDensity::dpi(dpi.round().clamp(1., 65535.) as u16));
    }
    encoder.encode_image(&rgb).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Median-cut quantization to at most 256 RGBA colors, written as an indexed PNG with tRNS.
fn png8(rgba: &image::RgbaImage, out: &mut Vec<u8>) -> Result<(), String> {
    let pixels: Vec<[u8; 4]> = rgba.pixels().map(|p| p.0).collect();
    let mut unique: Vec<[u8; 4]> = pixels.clone();
    unique.sort_unstable();
    unique.dedup();
    let palette: Vec<[u8; 4]> = if unique.len() <= 256 {
        unique
    } else {
        let mut boxes = vec![pixels.clone()];
        while boxes.len() < 256 {
            let (index, channel, range) = boxes
                .iter()
                .enumerate()
                .filter(|(_, b)| b.len() > 1)
                .flat_map(|(i, b)| (0..4).map(move |c| {
                    let (lo, hi) = b.iter().fold((255u8, 0u8), |(lo, hi), p| (lo.min(p[c]), hi.max(p[c])));
                    (i, c, hi - lo)
                }))
                .max_by_key(|&(_, _, r)| r)
                .unwrap_or((0, 0, 0));
            if range == 0 {
                break;
            }
            let mut b = boxes.swap_remove(index);
            b.sort_unstable_by_key(|p| p[channel]);
            let upper = b.split_off(b.len() / 2);
            boxes.push(b);
            boxes.push(upper);
        }
        boxes
            .iter()
            .map(|b| {
                let n = b.len() as u64;
                let mut sum = [0u64; 4];
                b.iter().for_each(|p| (0..4).for_each(|c| sum[c] += u64::from(p[c])));
                sum.map(|s| ((s + n / 2) / n) as u8)
            })
            .collect()
    };
    let nearest = |p: &[u8; 4]| {
        palette
            .iter()
            .enumerate()
            .min_by_key(|(_, q)| (0..4).map(|c| (i32::from(p[c]) - i32::from(q[c])).pow(2)).sum::<i32>())
            .map(|(i, _)| i as u8)
            .unwrap_or(0)
    };
    let mut cache = std::collections::HashMap::new();
    let indices: Vec<u8> = pixels.iter().map(|p| *cache.entry(*p).or_insert_with(|| nearest(p))).collect();
    let mut encoder = png::Encoder::new(&mut *out, rgba.width(), rgba.height());
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(palette.iter().flat_map(|p| [p[0], p[1], p[2]]).collect::<Vec<_>>());
    encoder.set_trns(palette.iter().map(|p| p[3]).collect::<Vec<_>>());
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&indices).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())
}
