use super::*;
use crate::geom::KernMode;

fn point(transform: Transform, p: Pt) -> Pt {
    Pt::new(
        transform.sx * p.x + transform.kx * p.y + transform.tx,
        transform.ky * p.x + transform.sy * p.y + transform.ty,
    )
}
fn dot(a: Pt, b: Pt) -> f32 {
    a.x * b.x + a.y * b.y
}

fn band(glyphs: &[&GlyphMetric], axis: Pt, normal: Pt, height: f32) -> Option<(f32, f32)> {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for glyph in glyphs {
        let font = TypeRun {
            font: glyph.font.clone(),
            font_variations: glyph.point.clone(),
            ..Default::default()
        };
        let contours = super::super::metrics::glyph_contours(&font, glyph.id);
        for contour in contours.iter() {
            for (a, b) in contour
                .iter()
                .zip(contour.iter().cycle().skip(1))
                .take(contour.len())
            {
                let a = point(super::guides::desired(glyph), *a);
                let b = point(super::guides::desired(glyph), *b);
                let ay = dot(a, normal);
                let by = dot(b, normal);
                if ay <= height && by > height || by <= height && ay > height {
                    let x =
                        dot(a, axis) + (dot(b, axis) - dot(a, axis)) * (height - ay) / (by - ay);
                    min = min.min(x);
                    max = max.max(x);
                }
            }
        }
    }
    min.is_finite().then_some((min, max))
}

/// Resolve optical spacing from the actual shaped outlines without changing Unicode clusters.
/// Args: native character intent, source layout and logical range offset. Returns per-character inline shifts.
pub(super) fn adjustments(run: &TypeRun, layout: &Layout, from: usize) -> BTreeMap<usize, f32> {
    let mut groups: BTreeMap<(usize, usize, usize), Vec<&GlyphMetric>> = BTreeMap::new();
    for glyph in &layout.glyphs {
        groups
            .entry((glyph.node, glyph.chunk, glyph.index))
            .or_default()
            .push(glyph);
    }
    let mut previous: Option<(&Vec<&GlyphMetric>, &VisualCharacter)> = None;
    let mut result = BTreeMap::new();
    for ((node, chunk, index), glyphs) in &groups {
        let Some(character) = layout
            .characters
            .iter()
            .find(|c| c.node == *node && c.chunk == *chunk && c.index == *index)
        else {
            previous = None;
            continue;
        };
        let whitespace = run
            .content
            .chars()
            .nth(from + index)
            .is_none_or(char::is_whitespace);
        if whitespace || glyphs.iter().any(|g| !g.allowed) {
            previous = None;
            continue;
        }
        let optical = run
            .spans
            .iter()
            .find(|s| s.start <= from + index && from + index < s.end)
            .and_then(|s| s.kerning)
            == Some(KernMode::Optical);
        if optical
            && !character.line_start
            && let Some((left, left_character)) = previous
        {
            if left_character.node == *node
                && left_character.chunk == *chunk
                && left_character.rtl == character.rtl
            {
                let right = glyphs[0];
                let sign = if character.rtl { -1. } else { 1. };
                let axis = right.inline * sign;
                let normal = Pt::new(-right.inline.y, right.inline.x);
                let baseline = dot(right.baseline, normal);
                let mut gaps = Vec::new();
                let relative = if character.vertical {
                    character.dy
                } else {
                    character.dx
                } * sign;
                let authored = left[0].advance - left[0].width;
                for i in 0..16 {
                    let height = baseline + (-0.95 + i as f32 * 0.075) * right.px;
                    if let (Some((_, a)), Some((b, _))) = (
                        band(left, axis, normal, height),
                        band(glyphs, axis, normal, height),
                    ) {
                        gaps.push(b - a - authored - relative);
                    }
                }
                if !gaps.is_empty() {
                    gaps.sort_by(f32::total_cmp);
                    let amount = (right.px * 0.075 - gaps[gaps.len() / 4])
                        .clamp(-right.px * 0.15, right.px * 0.15)
                        * sign;
                    if amount.is_finite() {
                        result.insert(character.ordinal, amount);
                    }
                }
            }
        }
        previous = Some((glyphs, character));
    }
    result
}
