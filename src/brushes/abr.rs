//! Bounded Photoshop sampled-brush decoding.
use super::{MAX_BRUSHES, MAX_PIXELS, Pack, Preset, Tip};

pub(super) struct Reader<'a> {
    pub bytes: &'a [u8],
    pub at: usize,
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).ok_or("Brush length overflow")?;
        let bytes = self.bytes.get(self.at..end).ok_or("Truncated brush file")?;
        self.at = end;
        Ok(bytes)
    }
    pub fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }
    pub(super) fn unicode(&mut self) -> Result<String, String> {
        let count = self.u32()? as usize;
        if count > 4096 {
            return Err("Brush name is too long".into());
        }
        let units = self
            .take(count * 2)?
            .chunks_exact(2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect::<Vec<_>>();
        Ok(String::from_utf16_lossy(&units)
            .trim_end_matches('\0')
            .into())
    }
}

pub(super) fn decode(bytes: &[u8], name: &str) -> Result<Pack, String> {
    let mut r = Reader::new(bytes);
    let version = r.u16()?;
    let count = r.u16()?;
    let mut pack = Pack::new(name);
    pack.notes.push("Photoshop sampled tips are imported. Photoshop tool presets, texture/dual-brush dynamics, pressure, and scattering are not reproduced.".into());
    match version {
        1 | 2 => {
            for index in 0..count {
                let kind = r.u16()?;
                let len = r.u32()? as usize;
                let mut entry = Reader::new(r.take(len)?);
                if kind != 2 {
                    pack.notes
                        .push(format!("Skipped computed brush {}", index + 1));
                    continue;
                }
                entry.take(4)?;
                let spacing = entry.u16()? as f32 / 100.0;
                let brush_name = if version == 2 {
                    entry.unicode()?
                } else {
                    format!("{name} {}", index + 1)
                };
                entry.take(9)?;
                let tip = sample(&mut entry)?;
                pack.brushes.push(Preset::new(
                    if brush_name.is_empty() {
                        format!("{name} {}", index + 1)
                    } else {
                        brush_name
                    },
                    tip,
                    spacing,
                ));
                limit(&pack)?;
            }
        }
        6 | 7 | 10 => {
            let skip = match count {
                1 => 47,
                2 => 301,
                _ => {
                    return Err(format!(
                        "Unsupported ABR {version}.{count}; export sampled brushes from Photoshop"
                    ));
                }
            };
            let mut samples_by_id = std::collections::BTreeMap::new();
            let mut descriptors = None;
            while r.at < bytes.len() {
                if r.take(4)? != b"8BIM" {
                    return Err("Invalid ABR section signature".into());
                }
                let tag = r.take(4)?;
                let len = r.u32()? as usize;
                let block = r.take(len)?;
                if tag == b"desc" {
                    descriptors = Some(super::descriptor::decode(block));
                }
                if tag == b"samp" {
                    let mut samples = Reader::new(block);
                    while samples.at < block.len() {
                        let len = samples.u32()? as usize;
                        let mut entry = Reader::new(samples.take(len)?);
                        let header = entry.take(skip)?;
                        let key_len = header.first().copied().unwrap_or(0) as usize;
                        let key =
                            String::from_utf8_lossy(header.get(1..1 + key_len).unwrap_or(&[]))
                                .into_owned();
                        let tip = sample(&mut entry)?;
                        samples_by_id.insert(key, std::sync::Arc::new(tip.clone()));
                        pack.brushes.push(Preset::new(
                            format!("{name} {}", pack.brushes.len() + 1),
                            tip,
                            0.15,
                        ));
                        limit(&pack)?;
                        let padding = (4 - len % 4) % 4;
                        samples.take(padding)?;
                    }
                }
                let padding = (4 - len % 4) % 4;
                if padding > 0 && r.at < bytes.len() && !bytes[r.at..].starts_with(b"8BIM") {
                    r.take(padding)?;
                }
            }
            match descriptors {
                Some(Ok(data)) => apply_descriptors(&mut pack, &data, &samples_by_id)?,
                Some(Err(error)) => pack.notes.push(format!(
                    "Sample names and spacing use fallback values: {error}"
                )),
                None => pack.notes.push(
                    "Sample names and spacing use fallback values because descriptors are absent."
                        .into(),
                ),
            }
        }
        _ => {
            return Err(format!(
                "Unsupported Photoshop ABR version {version}; use a sampled ABR version 1, 2, 6, 7, or 10"
            ));
        }
    }
    if pack.brushes.is_empty() {
        return Err("This ABR contains no supported sampled brush tips".into());
    }
    Ok(pack)
}

fn limit(pack: &Pack) -> Result<(), String> {
    if pack.brushes.len() > MAX_BRUSHES
        || pack
            .brushes
            .iter()
            .map(|b| b.tip.alpha.len())
            .sum::<usize>()
            > super::MAX_FILE
    {
        return Err("ABR exceeds brush count or decoded memory limits".into());
    }
    Ok(())
}

fn sample(r: &mut Reader<'_>) -> Result<Tip, String> {
    let top = r.i32()? as i64;
    let left = r.i32()? as i64;
    let bottom = r.i32()? as i64;
    let right = r.i32()? as i64;
    let (w, h) = (right - left, bottom - top);
    let depth = r.u16()?;
    let compression = r.u8()?;
    if w <= 0
        || h <= 0
        || w > 8192
        || h > 8192
        || w * h > MAX_PIXELS as i64
        || !matches!(depth, 8 | 16)
    {
        return Err("Invalid or excessive ABR tip dimensions/depth".into());
    }
    let (w, h) = (w as usize, h as usize);
    let row_bytes = w * (depth / 8) as usize;
    let data = match compression {
        0 => r.take(row_bytes * h)?.to_vec(),
        1 => {
            let lengths = (0..h)
                .map(|_| r.u16().map(usize::from))
                .collect::<Result<Vec<_>, _>>()?;
            let mut data = Vec::with_capacity(row_bytes * h);
            for length in lengths {
                unpack_row(r.take(length)?, row_bytes, &mut data)?;
            }
            data
        }
        _ => return Err("Unsupported ABR sample compression".into()),
    };
    let alpha = if depth == 16 {
        data.chunks_exact(2)
            .map(|v| ((u16::from_be_bytes([v[0], v[1]]) as u32 + 128) / 257) as u8)
            .collect()
    } else {
        data
    };
    super::crop(Tip {
        width: w as u32,
        height: h as u32,
        alpha,
    })
}

fn unpack_row(bytes: &[u8], expected: usize, output: &mut Vec<u8>) -> Result<(), String> {
    let start = output.len();
    let mut r = Reader::new(bytes);
    while r.at < bytes.len() {
        let control = r.u8()? as i8;
        let count = match control {
            -128 => continue,
            0..=127 => control as usize + 1,
            _ => (1i16 - control as i16) as usize,
        };
        if output.len() - start + count > expected {
            return Err("ABR RLE row exceeds its width".into());
        }
        if control >= 0 {
            output.extend_from_slice(r.take(count)?);
        } else {
            let value = r.u8()?;
            output.resize(output.len() + count, value);
        }
    }
    if output.len() - start != expected {
        return Err("Truncated ABR RLE row".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packbits_rows_reject_overflow_truncation_and_accept_noops() {
        let mut out = vec![];
        unpack_row(&[1, 20, 40, 254, 90, 128], 5, &mut out).unwrap();
        assert_eq!(out, [20, 40, 90, 90, 90]);
        assert!(unpack_row(&[252, 255], 4, &mut vec![]).is_err());
        assert!(unpack_row(&[1, 10], 2, &mut vec![]).is_err());
        assert!(unpack_row(&[0, 10], 2, &mut vec![]).is_err());
    }
    #[test]
    fn rejects_malformed_headers_without_panicking() {
        for len in 0..400 {
            let bytes = vec![255; len];
            assert!(decode(&bytes, "broken").is_err());
        }
    }
}

fn apply_descriptors(
    pack: &mut Pack,
    data: &serde_json::Value,
    samples: &std::collections::BTreeMap<String, std::sync::Arc<Tip>>,
) -> Result<(), String> {
    let Some(list) = data.get("Brsh").and_then(|v| v.as_array()) else {
        return Ok(());
    };
    let mut presets = vec![];
    for item in list {
        let Some(settings) = item.get("Brsh") else {
            continue;
        };
        let name = item
            .get("Nm  ")
            .and_then(|v| v.as_str())
            .unwrap_or("Imported brush");
        let name = name
            .strip_prefix("$$$/")
            .and_then(|_| name.split_once('=').map(|(_, label)| label))
            .unwrap_or(name)
            .to_owned();
        let num = |key: &str, default: f32| {
            settings
                .get(key)
                .and_then(|v| v.as_f64())
                .map_or(default, |n| n as f32)
        };
        let size = num("Dmtr", 48.0).clamp(1.0, 2048.0);
        let spacing = (num("Spcn", 15.0) / 100.0).clamp(0.01, 100.0);
        let tip = match settings.get("$class").and_then(|v| v.as_str()) {
            Some("sampledBrush") => settings
                .get("sampledData")
                .and_then(|v| v.as_str())
                .and_then(|id| samples.get(id))
                .cloned(),
            Some("computedBrush") => {
                let n = 128u32;
                let mut alpha = vec![0; n as usize * n as usize];
                let hardness = (num("Hrdn", 100.0) / 100.0).clamp(0.0, 1.0);
                let roundness = (num("Rndn", 100.0) / 100.0).clamp(0.01, 1.0);
                let angle = num("Angl", 0.0).to_radians();
                let (sin, cos) = angle.sin_cos();
                for y in 0..n {
                    for x in 0..n {
                        let a = (x as f32 + 0.5 - n as f32 / 2.0) / (n as f32 / 2.0);
                        let b = (y as f32 + 0.5 - n as f32 / 2.0) / (n as f32 / 2.0);
                        let radius = (a * cos + b * sin).hypot((-a * sin + b * cos) / roundness);
                        let v = if radius <= hardness {
                            1.0
                        } else {
                            ((1.0 - radius) / (1.0 - hardness).max(0.001)).clamp(0.0, 1.0)
                        };
                        alpha[(y * n + x) as usize] = (v * 255.0).round() as u8;
                    }
                }
                Some(std::sync::Arc::new(super::crop(Tip {
                    width: n,
                    height: n,
                    alpha,
                })?))
            }
            _ => None,
        };
        if let Some(tip) = tip {
            presets.push(Preset {
                name,
                size,
                spacing,
                hardness: 1.0,
                flow: 1.0,
                opacity: 1.0,
                tip,
                angle: 0.0,
                scatter: 0.0,
                angle_jitter: 0.0,
                size_jitter: 0.0,
                opacity_jitter: 0.0,
                follow_direction: false,
            });
        }
    }
    let skipped = list.len().saturating_sub(presets.len());
    if skipped > 0 {
        pack.notes.push(format!(
            "Skipped {skipped} unsupported tool or bristle presets."
        ));
    }
    if !presets.is_empty() {
        pack.brushes = presets;
        limit(pack)?;
    }
    Ok(())
}
