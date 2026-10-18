use super::*;
use crate::geom::CharSpan;

pub(super) const MARKER: &str = "data-oma-character-scale";
pub(super) const META: &str = "data-oma-character-scales";
const ATTRIBUTES: &[&str] = &[
    "font-size",
    "letter-spacing",
    "word-spacing",
    "textLength",
    "lengthAdjust",
    "style",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Scale {
    start: usize,
    end: usize,
    hscale: Option<f32>,
    vscale: Option<f32>,
    pub font_factor: f32,
    pub inline_factor: f32,
    before: BTreeMap<String, Option<String>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scales {
    version: u8,
    length: usize,
    edits: Vec<Scale>,
}

impl Scale {
    fn valid(&self, count: usize) -> bool {
        self.start < self.end
            && self.end <= count
            && [self.hscale, self.vscale]
                .into_iter()
                .flatten()
                .all(f32::is_finite)
            && (self.hscale.is_some() || self.vscale.is_some())
            && self.font_factor.is_finite()
            && self.font_factor > 0.
            && self.inline_factor.is_finite()
            && self.inline_factor > 0.
            && self
                .before
                .keys()
                .all(|key| ATTRIBUTES.contains(&key.as_str()))
    }
    pub(super) fn contains(&self, index: usize) -> bool {
        self.start <= index && index < self.end
    }
}

fn bound(count: usize) -> usize {
    count
        .saturating_mul(4096)
        .saturating_add(1024)
        .min(256 * 1024 * 1024)
}

fn decode(value: &str, count: usize) -> Result<Scales, String> {
    if value.len() > bound(count) {
        return Err("SVG character scale metadata exceeds source bounds".into());
    }
    let scales: Scales =
        serde_json::from_str(value).map_err(|e| format!("Invalid SVG character scales: {e}"))?;
    if scales.version != 1
        || scales.length != count
        || scales.edits.len() > count
        || scales.edits.iter().any(|s| !s.valid(count))
        || scales.edits.windows(2).any(|p| p[0].end > p[1].start)
    {
        return Err("Invalid SVG character scale bounds".into());
    }
    Ok(scales)
}

/// Divide scale edits only at real shaped chunk or writing-orientation boundaries.
/// Args: accepted native interval and original glyph layout. Returns intervals retaining every native style.
pub(super) fn spans(span: CharSpan, layout: Option<&Layout>) -> Vec<CharSpan> {
    if span.hscale.is_none() && span.vscale.is_none() {
        return vec![span];
    }
    let mut ends = vec![span.start, span.end];
    if let Some(layout) = layout {
        let mut glyphs: Vec<_> = layout
            .glyphs
            .iter()
            .filter(|g| span.start <= g.index && g.index < span.end)
            .collect();
        glyphs.sort_by_key(|g| g.index);
        for pair in glyphs.windows(2) {
            if pair[0].index != pair[1].index
                && (pair[0].node, pair[0].chunk, pair[0].upright)
                    != (pair[1].node, pair[1].chunk, pair[1].upright)
            {
                ends.push(pair[1].index);
            }
        }
    }
    ends.sort_unstable();
    ends.dedup();
    ends.windows(2)
        .map(|range| CharSpan {
            start: range[0],
            end: range[1],
            ..span.clone()
        })
        .collect()
}

/// Mark accepted scale intent on a temporary ordinary SVG text span.
/// Args: native style interval. Returns bounded scalar metadata without changing the retained source.
pub(super) fn marker(span: &CharSpan) -> String {
    serde_json::to_string(&Scale {
        start: span.start,
        end: span.end,
        hscale: span.hscale,
        vscale: span.vscale,
        font_factor: 1.,
        inline_factor: 1.,
        before: Default::default(),
    })
    .unwrap()
}

/// Apply glyph scales through standard font sizing and inline length adjustment.
/// Args: native intent, unscaled SVG and logical source range. Returns editable SVG using the same glyph layout.
pub(super) fn apply(run: &TypeRun, source: String, from: usize, content: &str) -> Option<String> {
    if !source.contains(MARKER) {
        return Some(source);
    }
    let xml = roxmltree::Document::parse(&source).ok()?;
    let nodes: Vec<_> = xml
        .descendants()
        .filter(|n| n.is_element() && n.attribute(MARKER).is_some())
        .collect();
    if nodes.is_empty() {
        return Some(source);
    }
    let layout = layout_source(run, source.clone(), content)?;
    let mut edits = Vec::new();
    let mut applied = Vec::new();
    for node in nodes {
        let mut scale: Scale = serde_json::from_str(node.attribute(MARKER)?).ok()?;
        let start = scale.start.checked_sub(from)?;
        let end = scale.end.checked_sub(from)?;
        if end > content.chars().count() || start >= end {
            return None;
        }
        let mut clusters = BTreeMap::new();
        for glyph in layout
            .glyphs
            .iter()
            .filter(|g| start <= g.index && g.index < end)
        {
            clusters
                .entry((glyph.node, glyph.chunk, glyph.index))
                .or_insert(glyph);
        }
        let first = *clusters.values().next()?;
        let hscale = scale
            .hscale
            .map(|v| v.clamp(1., 1000.) / 100.)
            .unwrap_or(if first.upright {
                1.
            } else {
                first.length_scale
            });
        let vscale = scale
            .vscale
            .map(|v| v.clamp(1., 1000.) / 100.)
            .unwrap_or(if first.upright {
                first.length_scale
            } else {
                1.
            });
        let font_factor = if first.upright { hscale } else { vscale };
        let inline_scale = if first.upright { vscale } else { hscale };
        let factor = inline_scale / font_factor;
        let width: f32 = clusters
            .values()
            .map(|g| {
                let natural = g.width / g.length_scale;
                natural * inline_scale + g.unadjusted_advance - natural
            })
            .sum();
        if !width.is_finite()
            || width < 0.
            || !font_factor.is_finite()
            || font_factor <= 0.
            || !factor.is_finite()
            || factor <= 0.
        {
            return None;
        }
        scale.start = start;
        scale.end = end;
        scale.font_factor = font_factor;
        scale.inline_factor = factor;
        scale.before = ATTRIBUTES
            .iter()
            .map(|&key| (key.into(), node.attribute(key).map(str::to_owned)))
            .collect();
        let mut attrs = BTreeMap::from([
            ("font-size", (first.px * font_factor).to_string()),
            (
                "letter-spacing",
                (first.letter_spacing / factor).to_string(),
            ),
            ("word-spacing", (first.word_spacing / factor).to_string()),
            ("textLength", width.to_string()),
            ("lengthAdjust", "spacingAndGlyphs".into()),
        ]);
        if font_factor != 1. {
            let mut variations = first.variations.clone();
            if first.auto_opsz && !variations.iter().any(|(tag, _)| tag == b"opsz") {
                variations.push((*b"opsz", first.px));
            }
            let settings = super::super::variations::FontVariations::new(variations).ok()?;
            let mut style = node.attribute("style").unwrap_or_default().to_string();
            style.push_str(&format!(
                ";font-optical-sizing:none;font-variation-settings:{};",
                if settings.is_empty() {
                    "normal".into()
                } else {
                    super::super::variations::css(&settings)
                }
            ));
            attrs.insert("style", style);
        }
        attrs.insert(MARKER, serde_json::to_string(&scale).ok()?);
        replace_attributes(&source, node, attrs, &mut edits)?;
        applied.push(scale);
    }
    applied.sort_by_key(|s| s.start);
    let record = Scales {
        version: 1,
        length: content.chars().count(),
        edits: applied,
    };
    let text = xml
        .descendants()
        .find(|n| n.has_tag_name("text") && !n.ancestors().any(|a| a.has_tag_name("defs")))?;
    replace_attributes(
        &source,
        text,
        BTreeMap::from([(META, serde_json::to_string(&record).ok()?)]),
        &mut edits,
    )?;
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut result = source;
    for (range, value) in edits {
        result.replace_range(range, &value);
    }
    Some(result)
}

fn replace_attributes(
    source: &str,
    node: roxmltree::Node<'_, '_>,
    attrs: BTreeMap<&str, String>,
    edits: &mut Vec<(std::ops::Range<usize>, String)>,
) -> Option<()> {
    let at = node.range().start + source[node.range()].find('>')?;
    let mut added = String::new();
    for (key, value) in attrs {
        let attribute = format!("{key}=\"{}\"", escape(&value));
        if let Some(old) = node.attributes().find(|a| a.name() == key) {
            edits.push((old.range(), attribute));
        } else {
            added.push(' ');
            added.push_str(&attribute);
        }
    }
    if !added.is_empty() {
        edits.push((at..at, added));
    }
    Some(())
}

/// Read applied runtime compensation without changing glyph geometry or authored values.
/// Args: standard temporary SVG. Returns validated logical scale intervals.
pub(super) fn resolved(source: &str) -> Vec<Scale> {
    if !source.contains(META) {
        return Vec::new();
    }
    let Some(xml) = roxmltree::Document::parse(source).ok() else {
        return Vec::new();
    };
    let Some(text) = xml
        .descendants()
        .find(|n| n.has_tag_name("text") && !n.ancestors().any(|a| a.has_tag_name("defs")))
    else {
        return Vec::new();
    };
    let Some(value) = text.attribute(META) else {
        return Vec::new();
    };
    let count = text
        .attribute("data-oma-flow-source")
        .and_then(|value| serde_json::from_str::<FlowSource>(value).ok())
        .map(|flow| flow.content.chars().count())
        .unwrap_or_else(|| {
            text.descendants()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .map(|s| s.chars().count())
                .sum()
        });
    decode(value, count).map_or_else(|_| Vec::new(), |scales| scales.edits)
}

/// Restore native scale intent and the original span attributes before another editable render.
/// Args: imported native SVG text. Returns an error before accepting malformed scale metadata.
pub(crate) fn restore(run: &mut TypeRun) -> Result<(), String> {
    let Some(svg) = &run.svg_text else {
        return Ok(());
    };
    let TextNode::Element { attributes, .. } = &svg.root else {
        return Ok(());
    };
    let Some(value) = attributes.get(META) else {
        return Ok(());
    };
    let count = run.content.chars().count();
    let scales = decode(value, count)?;
    fn restore_node(
        node: &mut TextNode,
        cursor: &mut usize,
        count: usize,
        scales: &[Scale],
        seen: &mut Vec<usize>,
    ) -> Result<bool, String> {
        if let TextNode::Characters { count: n } = node {
            *cursor = cursor
                .checked_add(*n)
                .ok_or("SVG character scale count overflow")?;
            return Ok(false);
        }
        let TextNode::Element {
            name,
            attributes,
            children,
            ..
        } = node
        else {
            unreachable!()
        };
        let mut expected_end = None;
        if let Some(value) = attributes.remove(MARKER) {
            if value.len() > bound(count) {
                return Err("SVG character scale metadata exceeds source bounds".into());
            }
            let scale: Scale = serde_json::from_str(&value)
                .map_err(|e| format!("Invalid SVG character scale: {e}"))?;
            if !scale.valid(count)
                || scale.start != *cursor
                || !scales.contains(&scale)
                || seen.contains(&scale.start)
            {
                return Err("Invalid SVG character scale owner".into());
            }
            seen.push(scale.start);
            expected_end = Some(scale.end);
            for (key, value) in scale.before {
                if let Some(value) = value {
                    attributes.insert(key, value);
                } else {
                    attributes.remove(&key);
                }
            }
        }
        let mut normalized = Vec::new();
        for mut child in std::mem::take(children) {
            if restore_node(&mut child, cursor, count, scales, seen)? {
                let TextNode::Element { children, .. } = child else {
                    unreachable!()
                };
                normalized.extend(children);
            } else {
                normalized.push(child);
            }
        }
        *children = normalized;
        if expected_end.is_some_and(|end| end != *cursor) {
            return Err("Invalid SVG character scale range".into());
        }
        Ok(expected_end.is_some() && name == "tspan" && attributes.is_empty())
    }
    let mut root = svg.root.clone();
    let mut seen = Vec::new();
    if svg.flow.is_none() {
        restore_node(&mut root, &mut 0, count, &scales.edits, &mut seen)?;
        if seen.len() != scales.edits.len() {
            return Err("SVG character scale owner is missing".into());
        }
    }
    if let TextNode::Element { attributes, .. } = &mut root {
        attributes.remove(META);
    }
    run.svg_text.as_mut().unwrap().root = root;
    for scale in scales.edits {
        run.set_character_style(scale.start, scale.end, |s| {
            s.hscale = scale.hscale;
            s.vscale = scale.vscale;
        });
    }
    Ok(())
}
