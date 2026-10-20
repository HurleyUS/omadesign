//! Recolor presets that compute replacement colors for a draft: color
//! reduction by median cut, grayscale, and invert. Alpha is always kept.
use crate::color::Rgba;

/// Map every original color to one of at most `count` representative colors.
/// Args: distinct originals and target count (1..=64). Returns one replacement per original.
pub(crate) fn reduce(colors: &[Rgba], count: usize) -> Vec<Rgba> {
    let count = count.clamp(1, 64);
    let mut boxes: Vec<Vec<usize>> = vec![(0..colors.len()).collect()];
    while boxes.len() < count {
        let channel = |c: Rgba, k: usize| [c.r, c.g, c.b][k];
        let span = |b: &Vec<usize>| {
            (0..3)
                .map(|k| {
                    let v = b.iter().map(|&i| channel(colors[i], k));
                    (v.clone().max().unwrap_or(0) - v.min().unwrap_or(0), k)
                })
                .max()
                .unwrap()
        };
        let Some((index, (range, axis))) = boxes.iter().enumerate().filter(|(_, b)| b.len() > 1).map(|(i, b)| (i, span(b))).max_by_key(|(_, s)| s.0) else { break };
        if range == 0 {
            break;
        }
        let mut b = boxes.swap_remove(index);
        b.sort_by_key(|&i| (channel(colors[i], axis), i));
        let rest = b.split_off(b.len() / 2);
        boxes.push(b);
        boxes.push(rest);
    }
    let mut out = colors.to_vec();
    for b in boxes {
        let n = b.len().max(1) as u32;
        let sum = b.iter().fold([0u32; 3], |s, &i| [s[0] + colors[i].r as u32, s[1] + colors[i].g as u32, s[2] + colors[i].b as u32]);
        for &i in &b {
            out[i] = Rgba::new(((sum[0] + n / 2) / n) as u8, ((sum[1] + n / 2) / n) as u8, ((sum[2] + n / 2) / n) as u8, colors[i].a);
        }
    }
    out
}

/// Rec. 709 luminance grayscale. Args: originals. Returns replacements.
pub(crate) fn grayscale(colors: &[Rgba]) -> Vec<Rgba> {
    colors
        .iter()
        .map(|c| {
            let y = (0.2126 * c.r as f32 + 0.7152 * c.g as f32 + 0.0722 * c.b as f32).round() as u8;
            Rgba::new(y, y, y, c.a)
        })
        .collect()
}

/// RGB inversion. Args: originals. Returns replacements.
pub(crate) fn invert(colors: &[Rgba]) -> Vec<Rgba> {
    colors.iter().map(|c| Rgba::new(255 - c.r, 255 - c.g, 255 - c.b, c.a)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn craft_recolor_reduce_groups_near_colors_and_keeps_alpha() {
        let colors = [Rgba::new(250, 0, 0, 255), Rgba::new(240, 10, 0, 128), Rgba::new(0, 0, 250, 255), Rgba::new(0, 10, 240, 255)];
        let out = reduce(&colors, 2);
        assert_eq!(out[0].r, out[1].r);
        assert_eq!(out[1].a, 128);
        assert_eq!(out[2], out[3]);
        assert_ne!(out[0].r, out[2].r);
        let distinct: std::collections::HashSet<_> = reduce(&colors, 1).iter().map(|c| (c.r, c.g, c.b)).collect();
        assert_eq!(distinct.len(), 1);
        assert_eq!(reduce(&colors, 64).iter().map(|c| (c.r, c.g, c.b)).collect::<Vec<_>>(), colors.iter().map(|c| (c.r, c.g, c.b)).collect::<Vec<_>>());
        assert!(reduce(&[], 3).is_empty());
        assert_eq!(grayscale(&[Rgba::new(255, 255, 255, 9)]), vec![Rgba::new(255, 255, 255, 9)]);
        assert_eq!(invert(&[Rgba::new(0, 55, 255, 1)]), vec![Rgba::new(255, 200, 0, 1)]);
    }

    #[test]
    fn craft_recolor_reduce_applies_through_recolor_artwork_in_one_undo() {
        let mut s = crate::app::symbols::tests::fixture();
        let target = s.recolor_target().unwrap();
        let replacements = reduce(&target.colors, 1);
        let before = crate::project::encode(&s.doc).unwrap();
        let history = s.history.len();
        let changed = s.recolor_artwork(&target, &replacements).unwrap();
        assert_eq!(changed, replacements != target.colors);
        if changed {
            assert_eq!(s.history.len(), history + 1);
            s.undo();
        }
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    }
}
