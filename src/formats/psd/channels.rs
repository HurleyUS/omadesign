//! Photoshop alpha and spot channel interchange: merged-image extra channels,
//! Unicode names (resource 1045) and DisplayInfo (resource 1077).
use crate::channels::{Channel, Plane, spot::Spot};
use crate::document::Document;


const KIND_ALPHA: u8 = 0;
const KIND_SPOT: u8 = 2;

fn coverage_at(channel: &Channel, x: u32, y: u32) -> f32 {
    let plane = &channel.plane;
    let Some(inverse) = plane.placement().invert() else { return 0. };
    let mut p = tiny_skia::Point::from_xy(x as f32 + 0.5, y as f32 + 0.5);
    inverse.map_point(&mut p);
    if p.x >= 0. && p.y >= 0. && p.x < plane.width as f32 && p.y < plane.height as f32 {
        plane.samples[p.y as usize * plane.width as usize + p.x as usize].clamp(0., 1.)
    } else {
        0.
    }
}

/// Resample each root saved channel onto the canvas grid as raw merged-image planes.
/// Alpha stores coverage (white = selected); spot stores ink as Photoshop does (black = full ink).
pub(super) fn extra_planes(doc: &Document, w: u32, h: u32, depth: u16) -> Result<Vec<Vec<u8>>, String> {
    let bytes = depth as usize / 8;
    doc.channels
        .iter()
        .map(|channel| {
            let mut out = Vec::new();
            out.try_reserve_exact(w as usize * h as usize * bytes).map_err(|_| "Photoshop channel is too large")?;
            for y in 0..h {
                for x in 0..w {
                    let c = coverage_at(channel, x, y);
                    let v = if channel.spot.is_some() { 1. - c } else { c };
                    match depth {
                        8 => out.push(crate::deep_color::byte(v)),
                        16 => out.extend_from_slice(&((v * 65535.).round() as u16).to_be_bytes()),
                        _ => out.extend_from_slice(&v.to_be_bytes()),
                    }
                }
            }
            Ok(out)
        })
        .collect()
}

fn resource(out: &mut Vec<u8>, id: u16, data: &[u8]) {
    out.extend_from_slice(b"8BIM");
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 != 0 {
        out.push(0);
    }
}

pub(super) fn write_resources(out: &mut Vec<u8>, channels: &[Channel]) -> Result<(), String> {
    if channels.is_empty() {
        return Ok(());
    }
    let mut names = Vec::new();
    let mut display = 1u32.to_be_bytes().to_vec();
    for channel in channels {
        let units: Vec<u16> = channel.name.encode_utf16().chain([0]).collect();
        names.extend_from_slice(&(units.len() as u32).to_be_bytes());
        units.iter().for_each(|u| names.extend_from_slice(&u.to_be_bytes()));
        let (ink, opacity, kind) = match channel.spot {
            Some(spot) => (spot.ink, spot.solidity, KIND_SPOT),
            None => ([1., 0., 0.], 0.5, KIND_ALPHA),
        };
        display.extend_from_slice(&0u16.to_be_bytes()); // RGB color space
        for c in ink {
            display.extend_from_slice(&((c * 65535.).round() as u16).to_be_bytes());
        }
        display.extend_from_slice(&0u16.to_be_bytes());
        display.extend_from_slice(&((opacity * 100.).round() as u16).to_be_bytes());
        display.push(kind);
    }
    resource(out, 1045, &names);
    resource(out, 1077, &display);
    Ok(())
}

#[derive(Default)]
pub(super) struct Info {
    pub names: Vec<String>,
    display: Vec<(u16, [u16; 3], u16, u8)>,
}
impl Info {
    pub(super) fn read(&mut self, id: u16, data: &[u8], warnings: &mut Vec<String>) {
        let parsed = if id == 1045 { self.read_names(data) } else { self.read_display(data) };
        if parsed.is_none() {
            super::warn(warnings, "Photoshop channel names or display settings were damaged and were ignored.");
        }
    }
    fn read_names(&mut self, data: &[u8]) -> Option<()> {
        let mut at = 0;
        let mut names = Vec::new();
        while at + 4 <= data.len() {
            let n = u32::from_be_bytes(data[at..at + 4].try_into().ok()?) as usize;
            at += 4;
            let raw = data.get(at..at.checked_add(n.checked_mul(2)?)?)?;
            at += n * 2;
            let units: Vec<u16> = raw.chunks_exact(2).map(|b| u16::from_be_bytes([b[0], b[1]])).take_while(|u| *u != 0).collect();
            names.push(String::from_utf16(&units).ok()?);
        }
        self.names = names;
        Some(())
    }
    fn read_display(&mut self, data: &[u8]) -> Option<()> {
        let body = data.get(4..)?;
        self.display = body
            .chunks_exact(13)
            .map(|c| {
                let u = |i: usize| u16::from_be_bytes([c[i], c[i + 1]]);
                (u(0), [u(2), u(4), u(6)], u(10), c[12])
            })
            .collect();
        Some(())
    }
    /// Build saved channels from the merged image's extra planes.
    pub(super) fn channels(
        &self,
        samples: &[u8],
        w: u32,
        h: u32,
        depth: u16,
        first: usize,
        count: usize,
        warnings: &mut Vec<String>,
    ) -> Result<Vec<Channel>, String> {
        let bytes = depth as usize / 8;
        let n = w as usize * h as usize;
        let mut out = Vec::new();
        for k in 0..count {
            let Some(raw) = samples.get((first + k) * n * bytes..(first + k + 1) * n * bytes) else {
                super::warn(warnings, "Photoshop extra channels were truncated and were not imported.");
                break;
            };
            let display = self.display.get(k).copied();
            let spot = display.filter(|d| d.3 == KIND_SPOT).map(|(space, color, opacity, _)| {
                if space != 0 {
                    super::warn(warnings, "A Photoshop spot ink in a non-RGB color space was imported with a neutral display color.");
                }
                Spot {
                    ink: if space == 0 { color.map(|c| f32::from(c) / 65535.) } else { [0.5; 3] },
                    solidity: (f32::from(opacity) / 100.).clamp(0., 1.),
                }
            });
            let inverted = spot.is_some();
            let values = raw.chunks_exact(bytes).map(|b| {
                let v = match depth {
                    8 => f32::from(b[0]) / 255.,
                    16 => f32::from(u16::from_be_bytes([b[0], b[1]])) / 65535.,
                    _ => f32::from_be_bytes([b[0], b[1], b[2], b[3]]),
                };
                let v = if v.is_finite() { v.clamp(0., 1.) } else { 0. };
                if inverted { 1. - v } else { v }
            });
            let samples = crate::deep_color::storage::collect(values)?;
            let name = self.names.get(k).filter(|s| !s.trim().is_empty() && s.chars().count() <= 64 && !s.chars().any(char::is_control)).cloned().unwrap_or_else(|| format!("Alpha {}", k + 1));
            out.push(Channel { id: crate::document::next_id(), name, plane: Plane { width: w, height: h, transform: [1., 0., 0., 1., 0., 0.], samples }, spot });
        }
        crate::channels::validate(&out)?;
        Ok(out)
    }
}

#[cfg(test)]
#[path = "channels_tests.rs"]
mod tests;
