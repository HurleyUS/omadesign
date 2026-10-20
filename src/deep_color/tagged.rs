//! Gray and Lab tagged documents. The working space stays RGB; the tag
//! converts at the edges: display/export (Gray), import (PSD Gray/Lab) and
//! the color picker (Gray value or L*a*b* fields). A later true working mode
//! can replace these edge conversions without changing the saved tag.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorTag {
    Gray,
    Lab,
}

impl ColorTag {
    pub fn name(self) -> &'static str {
        match self {
            Self::Gray => "Gray",
            Self::Lab => "Lab",
        }
    }
}

fn to_linear(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}
fn to_srgb(v: f32) -> f32 {
    let v = v.clamp(0., 1.);
    if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1. / 2.4) - 0.055 }
}
const WHITE: [f32; 3] = [0.95047, 1.0, 1.08883];

/// sRGB (0..1) to CIE L*a*b* (D65). Returns [L 0..100, a, b].
pub fn srgb_to_lab(rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb.map(to_linear);
    let xyz = [
        0.4124564 * r + 0.3575761 * g + 0.1804375 * b,
        0.2126729 * r + 0.7151522 * g + 0.0721750 * b,
        0.0193339 * r + 0.1191920 * g + 0.9503041 * b,
    ];
    let f = |t: f32| if t > 216. / 24389. { t.cbrt() } else { (24389. / 27. * t + 16.) / 116. };
    let [fx, fy, fz] = [f(xyz[0] / WHITE[0]), f(xyz[1] / WHITE[1]), f(xyz[2] / WHITE[2])];
    [116. * fy - 16., 500. * (fx - fy), 200. * (fy - fz)]
}

/// CIE L*a*b* (D65) to sRGB (0..1, clipped to gamut).
pub fn lab_to_srgb(lab: [f32; 3]) -> [f32; 3] {
    let fy = (lab[0] + 16.) / 116.;
    let fx = fy + lab[1] / 500.;
    let fz = fy - lab[2] / 200.;
    let inv = |f: f32| if f.powi(3) > 216. / 24389. { f.powi(3) } else { (116. * f - 16.) * 27. / 24389. };
    let [x, y, z] = [inv(fx) * WHITE[0], inv(fy) * WHITE[1], inv(fz) * WHITE[2]];
    [
        3.2404542 * x - 1.5371385 * y - 0.4985314 * z,
        -0.9692660 * x + 1.8760108 * y + 0.0415560 * z,
        0.0556434 * x - 0.2040259 * y + 1.0572252 * z,
    ]
    .map(to_srgb)
}

/// Gray value (0..1) of an sRGB color: L* lightness mapped back through sRGB so
/// neutral colors keep their value.
pub fn gray_of(rgb: [f32; 3]) -> f32 {
    let y = 0.2126729 * to_linear(rgb[0]) + 0.7151522 * to_linear(rgb[1]) + 0.0721750 * to_linear(rgb[2]);
    to_srgb(y)
}

/// Convert a premultiplied preview pixmap to gray in place for Gray documents.
pub fn finish(pm: &mut tiny_skia::Pixmap, tag: Option<ColorTag>) {
    if tag != Some(ColorTag::Gray) {
        return;
    }
    for p in pm.pixels_mut() {
        let c = p.demultiply();
        let v = (gray_of([c.red(), c.green(), c.blue()].map(|v| v as f32 / 255.)) * 255.).round() as u8;
        *p = tiny_skia::ColorU8::from_rgba(v, v, v, c.alpha()).premultiply();
    }
}

/// Encode a pixmap as an 8-bit gray+alpha PNG. Args: rendered pixmap. Returns PNG bytes.
pub fn encode_gray_png(pm: &tiny_skia::Pixmap) -> Result<Vec<u8>, String> {
    let mut data = Vec::with_capacity(pm.width() as usize * pm.height() as usize * 2);
    for p in pm.pixels() {
        let c = p.demultiply();
        data.extend_from_slice(&[c.red(), c.alpha()]);
    }
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(&data, pm.width(), pm.height(), image::ExtendedColorType::La8)
        .map_err(|e| e.to_string())?;
    Ok(out)
}
use image::ImageEncoder;

#[cfg(test)]
#[path = "tagged_tests.rs"]
mod tests;
