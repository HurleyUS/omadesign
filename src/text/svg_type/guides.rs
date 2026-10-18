use super::*;

pub(in crate::text) const META: &str = "data-oma-glyph-guides";
const MAX_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Position {
    pub index: usize,
    pub dx: f32,
    pub dy: f32,
    pub vertical: bool,
    pub line_start: bool,
    #[serde(default)]
    pub px: Option<f32>,
    #[serde(default)]
    pub tracking: Option<f32>,
    #[serde(default)]
    pub baseline_shift: Option<f32>,
    #[serde(default)]
    pub upright: Option<bool>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Guides {
    version: u8,
    content: String,
    root: TextNode,
    positions: Vec<Position>,
}

fn characters(node: &TextNode, nodes: &mut usize) -> Option<usize> {
    *nodes = nodes.checked_add(1)?;
    if *nodes > 400000 {
        return None;
    }
    match node {
        TextNode::Characters { count } => Some(*count),
        TextNode::Element { children, .. } => children
            .iter()
            .try_fold(0usize, |n, child| n.checked_add(characters(child, nodes)?)),
    }
}

fn decode(value: &str) -> Result<Guides, String> {
    if value.len() > MAX_BYTES {
        return Err("SVG glyph guide metadata exceeds source bounds".into());
    }
    let saved: Guides =
        serde_json::from_str(value).map_err(|e| format!("Invalid SVG glyph guides: {e}"))?;
    let count = saved.content.chars().count();
    let native_count = if let TextNode::Element { attributes, .. } = &saved.root {
        attributes
            .get("data-oma-flow-source")
            .and_then(|v| serde_json::from_str::<FlowSource>(v).ok())
            .map_or(count, |flow| flow.content.chars().count())
    } else {
        count
    };
    if saved.version != 1
        || characters(&saved.root, &mut 0) != Some(count)
        || saved.positions.len() > native_count
        || saved.positions.iter().any(|p| {
            p.index >= native_count
                || !p.dx.is_finite()
                || !p.dy.is_finite()
                || p.px.is_some_and(|px| !px.is_finite() || px <= 0.)
                || p.tracking.is_some_and(|v| !v.is_finite())
                || p.baseline_shift.is_some_and(|v| !v.is_finite())
        })
        || saved.positions.windows(2).any(|p| p[0].index >= p[1].index)
    {
        return Err("Invalid SVG glyph guide ownership".into());
    }
    Ok(saved)
}

/// Restore the original source before normal scale, pair and flow capture.
/// Args: rendered text element and its actual Unicode. Returns a validated editable source, or no annotation.
pub(super) fn restore(
    node: roxmltree::Node<'_, '_>,
    content: &str,
) -> Result<Option<(TextNode, String)>, String> {
    let Some(value) = node.attribute(META) else {
        return Ok(None);
    };
    let saved = decode(value)?;
    if saved.content != content {
        return Err("SVG glyph guide text changed without updating its editable source".into());
    }
    Ok(Some((saved.root, saved.content)))
}

/// Recover actual editable offsets instead of exposing synthetic path-distance jumps.
/// Args: temporary rendered SVG. Returns original logical character positions.
pub(super) fn positions(source: &str) -> Vec<Position> {
    if !source.contains(META) {
        return Vec::new();
    }
    roxmltree::Document::parse(source)
        .ok()
        .and_then(|xml| {
            xml.descendants()
                .find_map(|n| n.attribute(META))
                .and_then(|s| decode(s).ok())
        })
        .map_or_else(Vec::new, |s| s.positions)
}

/// Express native glyph-local scale after the source character's supplemental rotation.
/// Args: actual shaped glyph. Returns its intended matrix for guide planning and optical measurement.
pub(super) fn desired(glyph: &GlyphMetric) -> Transform {
    let Some(factor) = glyph
        .native_factor
        .filter(|_| glyph.linear && glyph.rotation != 0.)
    else {
        return glyph.transform;
    };
    let axis = Transform::from_row(
        glyph.inline.x,
        glyph.inline.y,
        -glyph.inline.y,
        glyph.inline.x,
        0.,
        0.,
    );
    let stretch = axis
        .pre_scale(factor, 1.)
        .pre_concat(axis.invert().unwrap_or_default());
    let correction = Transform::from_rotate(glyph.rotation)
        .pre_concat(stretch)
        .pre_rotate(-glyph.rotation)
        .pre_concat(stretch.invert().unwrap_or_default());
    Transform::from_translate(glyph.origin.x, glyph.origin.y)
        .pre_concat(correction)
        .pre_translate(-glyph.origin.x, -glyph.origin.y)
        .pre_concat(glyph.transform)
}

fn horizontal_width(g: &GlyphMetric) -> f32 {
    let t = desired(g);
    g.width / g.length_scale * t.sx.hypot(t.ky) / g.font_px
}

fn normalize(node: &mut TextNode, glyphs: &[(usize, &GlyphMetric)], cursor: &mut usize) {
    if let TextNode::Characters { count } = node {
        *cursor += *count;
        return;
    }
    if let TextNode::Element {
        name,
        attributes,
        children,
    } = node
    {
        let start = *cursor;
        for child in children.iter_mut() {
            normalize(child, glyphs, cursor);
        }
        let mut selected = glyphs
            .iter()
            .filter(|(ordinal, _)| start <= *ordinal && *ordinal < *cursor)
            .map(|(_, g)| *g);
        if let Some(first) = selected.next() {
            let t = desired(first);
            let px = t.kx.hypot(t.sy);
            let mut seen = std::collections::HashSet::new();
            let width: f32 = std::iter::once(first)
                .chain(selected)
                .filter(|g| seen.insert(g.index))
                .map(horizontal_width)
                .sum();
            attributes.insert("font-size".into(), px.to_string());
            attributes.insert("letter-spacing".into(), "0".into());
            attributes.insert("word-spacing".into(), "0".into());
            attributes.insert("textLength".into(), width.to_string());
            attributes.insert("lengthAdjust".into(), "spacingAndGlyphs".into());
            attributes
                .entry("style".into())
                .or_default()
                .push_str(&format!(
                    ";font-size:{px}px;letter-spacing:0;word-spacing:0;writing-mode:horizontal-tb;"
                ));
        }
        if name == "text" || name == "textPath" {
            *name = "tspan".into();
        }
        for key in [
            "x",
            "y",
            "dx",
            "dy",
            "rotate",
            "href",
            "startOffset",
            "method",
            "spacing",
            "side",
            "text-anchor",
            "baseline-shift",
            "dominant-baseline",
            "alignment-baseline",
            "writing-mode",
            scaling::META,
            scaling::MARKER,
            positioning::META,
            "data-oma-flow",
            "data-oma-flow-source",
            META,
        ] {
            attributes.remove(key);
        }
    }
}

struct Indexed<'a> {
    start: usize,
    end: usize,
    node: &'a TextNode,
    children: Vec<Indexed<'a>>,
}
impl<'a> Indexed<'a> {
    fn new(node: &'a TextNode, cursor: &mut usize) -> Self {
        let start = *cursor;
        let children = match node {
            TextNode::Characters { count } => {
                *cursor += count;
                Vec::new()
            }
            TextNode::Element { children, .. } => children
                .iter()
                .map(|child| Self::new(child, cursor))
                .collect(),
        };
        Self {
            start,
            end: *cursor,
            node,
            children,
        }
    }
    fn slice(&self, range: &std::ops::Range<usize>) -> Option<TextNode> {
        if self.end <= range.start || self.start >= range.end {
            return None;
        }
        Some(match self.node {
            TextNode::Characters { .. } => TextNode::Characters {
                count: self.end.min(range.end) - self.start.max(range.start),
            },
            TextNode::Element {
                name, attributes, ..
            } => {
                let first = self.children.partition_point(|c| c.end <= range.start);
                let children = self.children[first..]
                    .iter()
                    .take_while(|c| c.start < range.end)
                    .filter_map(|c| c.slice(range))
                    .collect();
                TextNode::Element {
                    name: name.clone(),
                    attributes: attributes.clone(),
                    children,
                }
            }
        })
    }
}
fn piece(
    run: &TypeRun,
    root: &Indexed<'_>,
    content: &str,
    range: std::ops::Range<usize>,
    bytes: &[usize],
    glyphs: &[(usize, &GlyphMetric)],
) -> Option<String> {
    let mut root = root.slice(&range)?;
    let mut cursor = range.start;
    normalize(&mut root, glyphs, &mut cursor);
    let content = content[bytes[range.start]..bytes[range.end]].to_owned();
    let count = content.chars().count();
    let plain = TypeRun {
        content,
        font: run.font.clone(),
        px: run.px,
        svg_text: Some(SvgText {
            root,
            definitions: String::new(),
            base_font: run.font.clone(),
            base_px: run.px,
            viewport: run.svg_text.as_ref()?.viewport,
            base_fill: crate::document::Fill::None,
            flow: None,
        }),
        ..Default::default()
    };
    let mut out = String::new();
    write_nodes(
        &plain.svg_text.as_ref()?.root,
        &plain,
        &mut 0,
        0..count,
        &mut out,
        false,
        false,
        None,
    );
    Some(out)
}

/// Compile rotated scaled chunks into standard text paths without splitting their shaping context.
/// Args: native run, positioned standard source and full logical story. Returns equivalent editable SVG.
pub(super) fn compile(run: &TypeRun, source: String, content: &str) -> Option<String> {
    if !source.contains(scaling::META) {
        return Some(source);
    }
    let layout = layout_source(run, source.clone(), content)?;
    if !layout.glyphs.iter().any(|g| {
        g.linear
            && (g.rotation != 0.
                || g.upright
                || g.inline.y.abs() > 0.001
                || g.letter_spacing != 0.
                || g.word_spacing != 0.)
    }) {
        return Some(source);
    }
    for g in &layout.glyphs {
        let t = desired(g);
        let dot = t.sx * t.kx + t.ky * t.sy;
        let area = t.sx.hypot(t.ky) * t.kx.hypot(t.sy);
        if dot.abs() > area * 0.0001 {
            return Some(source);
        }
    }
    let xml = roxmltree::Document::parse(&source).ok()?;
    let text = xml
        .descendants()
        .find(|n| n.has_tag_name("text") && !n.ancestors().any(|a| a.has_tag_name("defs")))?;
    let mut visible = String::new();
    let original = collect(text, &sheet(&xml), &mut visible, true)?;
    let count = visible.chars().count();
    let mut groups: BTreeMap<(usize, usize), Vec<&VisualCharacter>> = BTreeMap::new();
    for character in &layout.characters {
        groups
            .entry((character.node, character.chunk))
            .or_default()
            .push(character);
    }
    let mut chunks: Vec<_> = groups.into_iter().collect();
    chunks.sort_by_key(|(_, c)| c.iter().map(|c| c.ordinal).min().unwrap_or(usize::MAX));
    let starts: Vec<_> = chunks
        .iter()
        .map(|(_, c)| c.iter().map(|c| c.ordinal).min().unwrap_or(0))
        .collect();
    let mut root = original.clone();
    if let TextNode::Element { attributes, .. } = &mut root {
        attributes.remove("opacity");
    }
    let indexed = Indexed::new(&root, &mut 0);
    let ordinals: BTreeMap<_, _> = layout
        .characters
        .iter()
        .map(|c| ((c.node, c.chunk, c.index), c.ordinal))
        .collect();
    let measured: Vec<_> = layout
        .glyphs
        .iter()
        .filter_map(|g| {
            ordinals
                .get(&(g.node, g.chunk, g.index))
                .map(|ordinal| (*ordinal, g))
        })
        .collect();
    let bytes: Vec<_> = visible
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(visible.len()))
        .collect();
    let mut definitions = String::new();
    let mut body = String::new();
    for (chunk_index, ((node, chunk), chars)) in chunks.iter().enumerate() {
        let a = if chunk_index == 0 {
            0
        } else {
            starts[chunk_index]
        };
        let b = starts.get(chunk_index + 1).copied().unwrap_or(count);
        if a >= b || b > count {
            return None;
        }
        let mut path = String::new();
        let mut cursor = 0.;
        let mut seen = std::collections::HashSet::new();
        for c in chars {
            let Some(g) = layout
                .glyphs
                .iter()
                .find(|g| g.node == *node && g.chunk == *chunk && g.index == c.index)
            else {
                continue;
            };
            if !seen.insert(g.index) {
                continue;
            }
            let target = desired(g);
            let angle = target.ky.atan2(target.sx);
            let axis = Pt::new(angle.cos(), angle.sin());
            let correction = target.pre_concat(g.transform.invert()?);
            let start = Pt::new(
                correction.sx * g.baseline.x + correction.kx * g.baseline.y + correction.tx,
                correction.ky * g.baseline.x + correction.sy * g.baseline.y + correction.ty,
            );
            let len = horizontal_width(g);
            if len <= 0. {
                return None;
            }
            let end = start + axis * len;
            if [start.x, start.y, end.x, end.y, len]
                .iter()
                .any(|v| !v.is_finite())
            {
                return None;
            }
            path.push_str(&format!("M{} {} L{} {} ", start.x, start.y, end.x, end.y));
            cursor += len;
        }
        if path.is_empty() {
            return None;
        }
        let existing = xml
            .descendants()
            .find(|n| {
                n.has_tag_name("path")
                    && n.attribute("d") == Some(path.as_str())
                    && n.attribute("transform").is_none()
            })
            .and_then(|n| n.attribute("id"));
        let id = if let Some(id) = existing {
            id.to_owned()
        } else {
            let digest = crate::typography::fingerprint(path.as_bytes());
            let base = format!("oma-glyph-guide-{digest:032x}");
            let mut id = base.clone();
            let mut suffix = 0;
            while xml
                .descendants()
                .any(|n| n.attribute("id") == Some(id.as_str()))
            {
                suffix += 1;
                id = format!("{base}-{suffix}");
            }
            definitions.push_str(&format!("<path id=\"{id}\" d=\"{path}\"/>"));
            id
        };
        let inner = piece(run, &indexed, &visible, a..b, &bytes, &measured)?;
        body.push_str(&format!("<textPath href=\"#{}\" startOffset=\"{}\" text-anchor=\"middle\" dominant-baseline=\"alphabetic\" alignment-baseline=\"alphabetic\" baseline-shift=\"0\"><tspan x=\"0\" y=\"0\" rotate=\"0\" dx=\"0\" dy=\"0\">{inner}</tspan></textPath>",escape(&id),cursor*0.5));
    }
    let mut positions: Vec<_> = layout
        .characters
        .iter()
        .map(|c| Position {
            index: c.index,
            dx: c.dx,
            dy: c.dy,
            vertical: c.vertical,
            line_start: c.line_start,
            px: Some(c.px),
            tracking: layout
                .styles
                .iter()
                .find(|(i, _)| *i == c.index)
                .and_then(|(_, s)| s.tracking),
            baseline_shift: layout
                .styles
                .iter()
                .find(|(i, _)| *i == c.index)
                .and_then(|(_, s)| s.baseline_shift),
            upright: layout
                .glyphs
                .iter()
                .find(|g| g.node == c.node && g.chunk == c.chunk && g.index == c.index)
                .map(|g| g.upright),
        })
        .collect();
    positions.sort_by_key(|p| p.index);
    let saved = Guides {
        version: 1,
        content: visible,
        root: original,
        positions,
    };
    let meta = serde_json::to_string(&saved).ok()?;
    if meta.len() > MAX_BYTES {
        return None;
    }
    let mut outer = attributes(text, &sheet(&xml));
    for key in [
        "x",
        "y",
        "dx",
        "dy",
        "rotate",
        "text-anchor",
        "baseline-shift",
        "dominant-baseline",
        "alignment-baseline",
        "textLength",
        "lengthAdjust",
        "writing-mode",
        scaling::MARKER,
    ] {
        outer.remove(key);
    }
    outer.insert("text-anchor".into(), "middle".into());
    outer.insert("baseline-shift".into(), "0".into());
    outer.insert("dominant-baseline".into(), "alphabetic".into());
    outer.insert("alignment-baseline".into(), "alphabetic".into());
    outer.insert("letter-spacing".into(), "0".into());
    outer.insert("word-spacing".into(), "0".into());
    outer
        .entry("style".into())
        .or_default()
        .push_str(";writing-mode:horizontal-tb;letter-spacing:0;word-spacing:0;");
    outer.insert(META.into(), meta);
    let mut fragment = "<text".to_owned();
    for (key, value) in outer {
        fragment.push_str(&format!(" {key}=\"{}\"", escape(&value)));
    }
    fragment.push('>');
    fragment.push_str(&body);
    fragment.push_str("</text>");
    let mut result = source.clone();
    result.replace_range(text.range(), &fragment);
    let at = result.find("</defs>")?;
    result.insert_str(at, &definitions);
    Some(result)
}

/// Rebind original snapshot font families through the existing export resolver.
/// Args: guide metadata and the existing family mapper. Returns the same provenance with portable aliases.
pub(in crate::text) fn map_families(
    value: &str,
    mut map: impl FnMut(&BTreeMap<String, String>) -> Result<String, String>,
) -> Result<String, String> {
    let mut saved = decode(value)?;
    super::fonts::map_families(&mut saved.root, |style, _| map(style))?;
    fn flow(
        node: &mut TextNode,
        map: &mut impl FnMut(&BTreeMap<String, String>) -> Result<String, String>,
    ) -> Result<(), String> {
        if let TextNode::Element {
            attributes,
            children,
            ..
        } = node
        {
            if let Some(value) = attributes.get_mut("data-oma-flow-source") {
                *value = super::fonts::map_flow_families(value, |style| map(style))?;
            }
            for child in children {
                flow(child, map)?;
            }
        }
        Ok(())
    }
    flow(&mut saved.root, &mut map)?;
    serde_json::to_string(&saved).map_err(|e| e.to_string())
}

/// Keep original path and paint references aligned with the normal SVG object prefix.
/// Args: source snapshot, existing definition IDs and object prefix. Returns rebound snapshot metadata.
pub(super) fn map_ids(value: &str, ids: &[&str], prefix: &str) -> Result<String, String> {
    let mut saved = decode(value)?;
    fn visit(node: &mut TextNode, ids: &[&str], prefix: &str) -> Result<(), String> {
        if let TextNode::Element {
            attributes,
            children,
            ..
        } = node
        {
            for value in attributes.values_mut() {
                for id in ids {
                    if *value == format!("#{id}") {
                        *value = format!("#{prefix}{id}");
                    }
                    *value = value.replace(&format!("url(#{id})"), &format!("url(#{prefix}{id})"));
                }
            }
            if let Some(value) = attributes.get_mut("data-oma-flow-source") {
                let mut flow: FlowSource =
                    serde_json::from_str(value).map_err(|e| e.to_string())?;
                visit(&mut flow.root, ids, prefix)?;
                *value = serde_json::to_string(&flow).map_err(|e| e.to_string())?;
            }
            for child in children {
                visit(child, ids, prefix)?;
            }
        }
        Ok(())
    }
    visit(&mut saved.root, ids, prefix)?;
    serde_json::to_string(&saved).map_err(|e| e.to_string())
}
