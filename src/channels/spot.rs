//! Spot ink channels: a saved channel tagged with an ink color and solidity.
//! The editor and flattened exports show the ink as a tinted overlay at its color.
use super::Channel;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spot {
    /// Display ink in sRGB, each 0..=1.
    pub ink: [f32; 3],
    /// Ink opacity over the artwork, 0..=1 (Photoshop "solidity").
    pub solidity: f32,
}
impl Default for Spot {
    fn default() -> Self {
        Self { ink: [1., 0., 0.55], solidity: 1. }
    }
}
impl Spot {
    pub fn validate(&self) -> Result<(), String> {
        if self.ink.iter().chain([&self.solidity]).all(|v| v.is_finite() && (0. ..=1.).contains(v)) {
            Ok(())
        } else {
            Err("Spot ink color and solidity must be between 0 and 1".into())
        }
    }
}

pub fn any(channels: &[Channel]) -> bool {
    channels.iter().any(|c| c.spot.is_some())
}

/// Tint a premultiplied pixmap with every spot channel, in channel order.
/// Args: target, channels and the world-to-pixmap transform. Coverage uses nearest samples.
pub fn overlay(pm: &mut tiny_skia::Pixmap, channels: &[Channel], view: tiny_skia::Transform) {
    let (w, h) = (pm.width(), pm.height());
    for channel in channels {
        let Some(spot) = channel.spot else { continue };
        let plane = &channel.plane;
        let Some(inverse) = view.pre_concat(plane.placement()).invert() else { continue };
        let ink = spot.ink.map(|v| v * 255.);
        let pixels = pm.pixels_mut();
        for y in 0..h {
            for x in 0..w {
                let mut p = tiny_skia::Point::from_xy(x as f32 + 0.5, y as f32 + 0.5);
                inverse.map_point(&mut p);
                if !(p.x >= 0. && p.y >= 0. && p.x < plane.width as f32 && p.y < plane.height as f32) {
                    continue;
                }
                let v = plane.samples[p.y as usize * plane.width as usize + p.x as usize];
                let a = v.clamp(0., 1.) * spot.solidity;
                if a <= 0. {
                    continue;
                }
                let px = &mut pixels[(y * w + x) as usize];
                let mix = |c: u8, ink: f32| (f32::from(c) * (1. - a) + ink * a).round().clamp(0., 255.) as u8;
                let alpha = mix(px.alpha(), 255.);
                let (r, g, b) = (mix(px.red(), ink[0]), mix(px.green(), ink[1]), mix(px.blue(), ink[2]));
                if let Some(next) = tiny_skia::PremultipliedColorU8::from_rgba(r.min(alpha), g.min(alpha), b.min(alpha), alpha) {
                    *px = next;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "spot_tests.rs"]
mod tests;
