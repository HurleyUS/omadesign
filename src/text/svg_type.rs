//! Editable SVG type keeps text, styles and character positioning together.
use crate::geom::{Pt, TypeRun};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, OnceLock},
};
use tiny_skia::Transform;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TextNode {
    Element {
        name: String,
        attributes: BTreeMap<String, String>,
        children: Vec<TextNode>,
    },
    Characters {
        count: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SvgText {
    pub root: TextNode,
    pub definitions: String,
    pub base_font: String,
    pub base_px: f32,
    #[serde(default)]
    pub flow: Option<Flow>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flow {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub leading: f32,
}

#[derive(Serialize, Deserialize)]
struct FlowSource {
    root: TextNode,
    content: String,
}

pub(crate) struct Layout {
    pub tree: usvg::Tree,
    pub contours: Vec<Vec<Pt>>,
    pub carets: Vec<(usize, Pt, Pt, Pt)>,
    pub styles: Vec<(usize, crate::geom::CharSpan)>,
    pub bounds: crate::geom::Bounds,
    pub stroke_width: f32,
}

pub(crate) fn paint_tree(run: &TypeRun, fill: bool) -> Option<Arc<usvg::Tree>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<usvg::Tree>>>> = OnceLock::new();
    let mut source = source(run)?;
    let xml = roxmltree::Document::parse(&source).ok()?;
    let mut edits = Vec::new();
    let property = if fill { "stroke" } else { "fill" };
    for node in xml
        .descendants()
        .filter(|n| n.is_element() && matches!(n.tag_name().name(), "text" | "tspan" | "textPath"))
    {
        if let Some(attr) = node.attributes().find(|a| a.name() == property) {
            edits.push((attr.range_value(), "none".to_owned()));
        } else if let Some(end) = source[node.range()].find('>') {
            let at = node.range().start + end;
            edits.push((at..at, format!(" {property}=\"none\"")));
        }
    }
    for (range, value) in edits.into_iter().rev() {
        source.replace_range(range, &value);
    }
    let cache = CACHE.get_or_init(Default::default);
    if let Some(tree) = cache.lock().unwrap().get(&source).cloned() {
        return Some(tree);
    }
    let tree = Arc::new(usvg::Tree::from_str(&renderer_source(&source), &options_for(run)).ok()?);
    let mut cache = cache.lock().unwrap();
    if cache.len() >= 64 {
        cache.clear();
    }
    cache.insert(source, tree.clone());
    Some(tree)
}

pub(crate) fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn attributes(
    node: roxmltree::Node<'_, '_>,
    sheet: &simplecss::StyleSheet<'_>,
) -> BTreeMap<String, String> {
    let mut attrs = BTreeMap::new();
    for a in node.attributes() {
        let name = if a.namespace() == Some("http://www.w3.org/1999/xlink") {
            "href"
        } else if a.namespace() == Some("http://www.w3.org/XML/1998/namespace") {
            "xml:space"
        } else {
            a.name()
        };
        if name.to_ascii_lowercase().starts_with("on") || matches!(name, "id" | "class" | "style") {
            continue;
        }
        if name == "href" && !a.value().starts_with('#') {
            continue;
        }
        attrs.insert(name.into(), a.value().into());
    }
    for property in PROPERTIES {
        if let Some(value) = crate::formats::svg::cascaded_property(node, sheet, property) {
            attrs.insert((*property).into(), value);
        }
    }
    attrs
}

const PROPERTIES: &[&str] = &[
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "font-stretch",
    "font-variant",
    "font-variation-settings",
    "font-optical-sizing",
    "font-kerning",
    "font-feature-settings",
    "fill",
    "fill-opacity",
    "stroke",
    "stroke-width",
    "stroke-opacity",
    "stroke-linecap",
    "stroke-linejoin",
    "paint-order",
    "letter-spacing",
    "word-spacing",
    "text-anchor",
    "direction",
    "unicode-bidi",
    "writing-mode",
    "text-orientation",
    "dominant-baseline",
    "alignment-baseline",
    "baseline-shift",
    "text-decoration",
    "visibility",
    "color",
    "white-space",
    "inline-size",
    "shape-inside",
    "line-height",
    "stop-color",
    "stop-opacity",
];

const CSS_PROPERTIES: &[&str] = &[
    "font-variation-settings",
    "font-optical-sizing",
    "font-kerning",
    "font-feature-settings",
    "white-space",
    "text-orientation",
];

fn safe_value(value: &str) -> bool {
    !value.contains([';', '{', '}'])
        && (!value.contains("url(")
            || value
                .trim()
                .strip_prefix("url(#")
                .and_then(|v| v.strip_suffix(')'))
                .is_some_and(|id| {
                    !id.is_empty()
                        && !id.contains(['(', ')', '\'', '"'])
                        && !id.chars().any(char::is_whitespace)
                }))
}

fn collect(
    node: roxmltree::Node<'_, '_>,
    sheet: &simplecss::StyleSheet<'_>,
    content: &mut String,
    preserve: bool,
) -> Option<TextNode> {
    if node.is_text() {
        let raw = node.text().unwrap_or("").replace(['\n', '\r', '\t'], " ");
        let mut text = String::new();
        for c in raw.chars() {
            let previous = text
                .chars()
                .last()
                .or_else(|| content.chars().last())
                .unwrap_or(' ');
            if preserve || c != ' ' || previous != ' ' {
                text.push(c);
            }
        }
        let count = text.chars().count();
        content.push_str(&text);
        return Some(TextNode::Characters { count });
    }
    if !node.is_element()
        || !matches!(
            node.tag_name().name(),
            "text" | "tspan" | "textPath" | "a" | "flowRoot" | "flowPara" | "flowSpan"
        )
    {
        return None;
    }
    let attrs = attributes(node, sheet);
    let preserve = attrs.get("xml:space").map_or(preserve, |v| v == "preserve")
        || attrs
            .get("white-space")
            .is_some_and(|v| v.starts_with("pre"));
    let mut children = Vec::new();
    let mut paragraph = false;
    for child in node.children() {
        if child.has_tag_name("flowPara") {
            if paragraph {
                content.push('\n');
                children.push(TextNode::Characters { count: 1 });
            }
            paragraph = true;
        }
        if let Some(child) = collect(child, sheet, content, preserve) {
            children.push(child);
        }
    }
    Some(TextNode::Element {
        name: match node.tag_name().name() {
            "flowRoot" => "text",
            "flowPara" | "flowSpan" | "a" => "tspan",
            other => other,
        }
        .into(),
        attributes: attrs,
        children,
    })
}

fn sheet<'a>(xml: &'a roxmltree::Document<'_>) -> simplecss::StyleSheet<'a> {
    let mut sheet = simplecss::StyleSheet::new();
    for node in xml.descendants().filter(|n| n.has_tag_name("style")) {
        if let Some(text) = node.text() {
            sheet.parse_more(text);
        }
    }
    sheet
}

fn definition(node: roxmltree::Node<'_, '_>, sheet: &simplecss::StyleSheet<'_>, out: &mut String) {
    if !node.is_element()
        || !matches!(
            node.tag_name().name(),
            "defs"
                | "g"
                | "path"
                | "rect"
                | "circle"
                | "ellipse"
                | "line"
                | "polyline"
                | "polygon"
                | "linearGradient"
                | "radialGradient"
                | "stop"
                | "pattern"
                | "clipPath"
                | "mask"
        )
    {
        return;
    }
    let tag = node.tag_name().name();
    out.push_str(&format!("<{tag}"));
    if let Some(id) = node.attribute("id") {
        out.push_str(&format!(" id=\"{}\"", escape(id)));
    }
    for (key, value) in attributes(node, sheet) {
        if !safe_value(&value) {
            continue;
        }
        out.push_str(&format!(" {key}=\"{}\"", escape(&value)));
    }
    out.push('>');
    for child in node.children() {
        definition(child, sheet, out);
    }
    out.push_str(&format!("</{tag}>"));
}

fn safe_definitions(source: &str) -> String {
    let source = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">{source}</svg>"
    );
    let Ok(xml) = roxmltree::Document::parse_with_options(
        &source,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 400000,
            entity_resolver: None,
        },
    ) else {
        return String::new();
    };
    let sheet = sheet(&xml);
    let mut out = String::new();
    for node in xml.root_element().children() {
        definition(node, &sheet, &mut out);
    }
    out
}

/// Capture SVG type without discarding its nested attributes.
/// Args: source XML, text node, normalized text and selected font. Returns editable type metadata.
pub(crate) fn capture(
    xml: &roxmltree::Document<'_>,
    node: roxmltree::Node<'_, '_>,
    font: String,
    px: f32,
) -> (SvgText, String) {
    let sheet = sheet(xml);
    let mut content = String::new();
    let preserve = node
        .ancestors()
        .find_map(|n| n.attribute((roxmltree::NS_XML_URI, "space")))
        .is_some_and(|v| v == "preserve");
    let mut root = collect(node, &sheet, &mut content, preserve).unwrap();
    fn trim_last(node: &mut TextNode, preserve: bool) -> Option<bool> {
        match node {
            TextNode::Characters { count } if *count > 0 => {
                if !preserve {
                    *count -= 1;
                }
                Some(!preserve)
            }
            TextNode::Characters { .. } => None,
            TextNode::Element {
                attributes,
                children,
                ..
            } => {
                let preserve = attributes
                    .get("xml:space")
                    .map_or(preserve, |v| v == "preserve");
                children
                    .iter_mut()
                    .rev()
                    .find_map(|child| trim_last(child, preserve))
            }
        }
    }
    if content.ends_with(' ') && trim_last(&mut root, preserve) == Some(true) {
        content.pop();
    }
    if let Some(saved) = node
        .attribute("data-oma-flow-source")
        .and_then(|value| serde_json::from_str::<FlowSource>(value).ok())
    {
        root = saved.root;
        content = saved.content;
    }
    if let TextNode::Element { attributes, .. } = &mut root {
        for property in PROPERTIES {
            if attributes
                .get(*property)
                .is_some_and(|value| !matches!(value.as_str(), "inherit" | "unset"))
            {
                continue;
            }
            for ancestor in node.ancestors().skip(1).filter(|n| n.is_element()) {
                if let Some(value) =
                    crate::formats::svg::cascaded_property(ancestor, &sheet, property)
                {
                    if !matches!(value.as_str(), "inherit" | "unset") {
                        attributes.insert((*property).into(), value);
                        break;
                    }
                }
            }
        }
        for key in [
            "transform",
            "opacity",
            "filter",
            "clip-path",
            "mask",
            "mix-blend-mode",
        ] {
            attributes.remove(key);
        }
        attributes.insert("xml:space".into(), "preserve".into());
    }
    let mut definitions = String::new();
    for defs in xml
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("defs"))
    {
        definition(defs, &sheet, &mut definitions);
    }
    for path in xml.descendants().filter(|n| {
        n.has_tag_name("path")
            && n.attribute("id").is_some()
            && !n.ancestors().any(|n| n.has_tag_name("defs"))
    }) {
        definition(path, &sheet, &mut definitions);
    }
    let flow = node
        .attribute("data-oma-flow")
        .and_then(|v| serde_json::from_str(v).ok());
    (
        SvgText {
            root,
            definitions,
            base_font: font,
            base_px: px,
            flow,
        },
        content,
    )
}

impl SvgText {
    /// Preserve each source text leaf across a character edit.
    /// Args: replaced range and inserted character count. Returns nothing; source ranges are updated.
    pub fn replace_range(&mut self, start: usize, end: usize, inserted: usize) {
        fn visit(
            node: &mut TextNode,
            cursor: &mut usize,
            start: usize,
            end: usize,
            inserted: usize,
            done: &mut bool,
        ) {
            match node {
                TextNode::Characters { count } => {
                    let a = *cursor;
                    let b = a + *count;
                    *cursor = b;
                    let removed = b.min(end).saturating_sub(a.max(start));
                    let add = if !*done && start <= b {
                        *done = true;
                        inserted
                    } else {
                        0
                    };
                    *count = *count - removed + add;
                }
                TextNode::Element { children, .. } => {
                    for child in children {
                        visit(child, cursor, start, end, inserted, done)
                    }
                }
            }
        }
        visit(&mut self.root, &mut 0, start, end, inserted, &mut false);
    }
}

fn family(path: &str) -> String {
    if path.starts_with("omatype:") {
        return path.into();
    }
    let probe = TypeRun {
        font: path.into(),
        ..Default::default()
    };
    super::font_bytes_for(&probe)
        .and_then(|bytes| {
            let face = rustybuzz::ttf_parser::Face::parse(&bytes, 0).ok()?;
            face.names()
                .into_iter()
                .filter(|n| n.name_id == 1)
                .find_map(|n| n.to_string())
        })
        .unwrap_or_else(|| super::label_for(path))
}

fn font_attributes(path: &str) -> String {
    let probe = TypeRun {
        font: path.into(),
        ..Default::default()
    };
    let mut attrs = format!(" font-family=\"{}\"", escape(&css_family(path)));
    if let Some(bytes) = super::font_bytes_for(&probe)
        && let Ok(face) = rustybuzz::ttf_parser::Face::parse(&bytes, 0)
    {
        attrs.push_str(&format!(
            " font-weight=\"{}\" font-style=\"{}\"",
            face.weight().to_number(),
            if face.is_italic() { "italic" } else { "normal" }
        ));
    }
    attrs
}

fn css_family(path: &str) -> String {
    format!(
        "'{}'",
        family(path).replace('\\', "\\\\").replace('\'', "\\'")
    )
}

fn write_nodes(
    node: &TextNode,
    run: &TypeRun,
    cursor: &mut usize,
    range: std::ops::Range<usize>,
    out: &mut String,
    root: bool,
) {
    match node {
        TextNode::Characters { count } => {
            let start = *cursor;
            let end = (start + *count).min(run.content.chars().count());
            *cursor = start + *count;
            let a = start.max(range.start);
            let b = end.min(range.end);
            if a >= b {
                return;
            }
            for span in run.character_style_runs(a, b) {
                let text = &run.content[super::char_to_byte(&run.content, span.start)
                    ..super::char_to_byte(&run.content, span.end)];
                let mut attrs = String::new();
                if let Some(font) = &span.font {
                    attrs.push_str(&font_attributes(font));
                }
                if let Some(px) = span.px {
                    attrs.push_str(&format!(" font-size=\"{px}\""));
                }
                if let Some(fill) = &span.fill {
                    match fill {
                        crate::document::Fill::Solid(c) => {
                            attrs.push_str(&format!(" fill=\"{}\"", c.css()))
                        }
                        crate::document::Fill::None => attrs.push_str(" fill=\"none\""),
                        _ => {}
                    }
                }
                if let Some(shift) = span.baseline_shift {
                    attrs.push_str(&format!(" baseline-shift=\"{shift}\""));
                }
                if let Some(tracking) = span.tracking {
                    attrs.push_str(&format!(
                        " letter-spacing=\"{}\"",
                        tracking * span.px.unwrap_or(run.px) / 1000.
                    ));
                }
                if span.kerning == Some(crate::geom::KernMode::None) {
                    attrs.push_str(" style=\"font-kerning:none\"");
                }
                if attrs.is_empty() {
                    out.push_str(&escape(text));
                } else {
                    out.push_str(&format!("<tspan{attrs}>{}</tspan>", escape(text)));
                }
            }
        }
        TextNode::Element {
            name,
            attributes,
            children,
        } => {
            let name = if matches!(name.as_str(), "text" | "tspan" | "textPath") {
                name.as_str()
            } else {
                "tspan"
            };
            out.push_str(&format!("<{name}"));
            let mut css = String::new();
            for (key, value) in attributes {
                if !(PROPERTIES.contains(&key.as_str())
                    || matches!(
                        key.as_str(),
                        "x" | "y"
                            | "dx"
                            | "dy"
                            | "rotate"
                            | "textLength"
                            | "lengthAdjust"
                            | "href"
                            | "method"
                            | "spacing"
                            | "side"
                            | "xml:space"
                            | "transform"
                    ))
                    || !safe_value(value)
                    || key == "href" && !value.starts_with('#')
                {
                    continue;
                }
                if matches!(
                    key.as_str(),
                    "data-oma-flow" | "data-oma-flow-source" | "inline-size" | "shape-inside"
                ) || root && key == "transform"
                    || run.svg_text.as_ref().is_some_and(|svg| svg.flow.is_some())
                        && matches!(key.as_str(), "x" | "y" | "dx" | "dy" | "visibility")
                {
                    continue;
                }
                let value = if key == "xml:space" {
                    "preserve".into()
                } else if key == "font-family"
                    && root
                    && run
                        .svg_text
                        .as_ref()
                        .is_some_and(|svg| run.font != svg.base_font)
                {
                    css_family(&run.font)
                } else {
                    value.clone()
                };
                if CSS_PROPERTIES.contains(&key.as_str()) {
                    css.push_str(&format!("{key}:{value};"));
                    continue;
                }
                out.push_str(&format!(" {key}=\"{}\"", escape(&value)));
            }
            if !css.is_empty() {
                out.push_str(&format!(" style=\"{}\"", escape(&css)));
            }
            out.push('>');
            for child in children {
                write_nodes(child, run, cursor, range.clone(), out, false);
            }
            out.push_str(&format!("</{name}>"));
        }
    }
}

fn options() -> usvg::Options<'static> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let mut options = usvg::Options::default();
    options.fontdb = DB
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone();
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    options
}

fn options_for(run: &TypeRun) -> usvg::Options<'static> {
    let mut options = options();
    let fonts: std::collections::BTreeSet<_> = std::iter::once(&run.font)
        .chain(run.spans.iter().filter_map(|s| s.font.as_ref()))
        .filter(|font| font.starts_with("omatype:"))
        .collect();
    for font in fonts {
        let probe = TypeRun {
            font: font.clone(),
            ..Default::default()
        };
        if let Some(bytes) = super::font_bytes_for(&probe) {
            let db = options.fontdb_mut();
            let ids = db.load_font_source(usvg::fontdb::Source::Binary(bytes));
            for id in ids {
                if let Some(mut face) = db.face(id).cloned() {
                    for (family, _) in &mut face.families {
                        *family = font.clone();
                    }
                    db.remove_face(id);
                    db.push_face_info(face);
                }
            }
        }
    }
    options
}

fn plain_fragment(run: &TypeRun, range: std::ops::Range<usize>) -> String {
    let mut out = String::new();
    write_nodes(
        &run.svg_text.as_ref().unwrap().root,
        run,
        &mut 0,
        range,
        &mut out,
        true,
    );
    out
}

fn wrap(fragment: &str, definitions: &str, matrix: [f32; 6]) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"100000\" height=\"100000\"><defs>{definitions}</defs><g transform=\"matrix({} {} {} {} {} {})\">{fragment}</g></svg>",
        matrix[0], matrix[1], matrix[2], matrix[3], matrix[4], matrix[5]
    )
}

/// Write editable standard SVG for a native SVG type object.
/// Args: authoritative text run. Returns a self-contained SVG, or None for ordinary native type.
pub fn source(run: &TypeRun) -> Option<String> {
    let svg = run.svg_text.as_ref()?;
    let fragment = if let Some(flow) = &svg.flow {
        flow_fragment(run, flow)
    } else {
        plain_fragment(run, 0..run.content.chars().count())
    };
    let mut matrix = Transform::from_row(1., 0., 0., 1., 0., 0.);
    if let Some(m) = run.transform {
        matrix = Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5]);
    }
    let scale = run.px / svg.base_px.max(0.001);
    matrix = matrix
        .pre_translate(run.origin.x, run.origin.y)
        .pre_scale(scale, scale);
    Some(wrap(
        &fragment,
        &safe_definitions(&svg.definitions),
        [
            matrix.sx, matrix.ky, matrix.kx, matrix.sy, matrix.tx, matrix.ty,
        ],
    ))
}

pub(crate) fn export_fragment(run: &TypeRun, id: u64, outlines: bool) -> Option<String> {
    let original = source(run)?;
    let portable = run.font.starts_with("omatype:")
        || run.spans.iter().any(|s| {
            s.font
                .as_ref()
                .is_some_and(|font| font.starts_with("omatype:"))
        });
    let source = if outlines || portable {
        layout(run)?.tree.to_string(&usvg::WriteOptions::default())
    } else {
        original
    };
    let xml = roxmltree::Document::parse(&source).ok()?;
    let prefix = format!("oma-type-{id}-");
    let mut edits = Vec::new();
    let ids: Vec<_> = xml
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .collect();
    for node in xml.descendants().filter(|n| n.is_element()) {
        for attr in node.attributes() {
            let mut value = attr.value().to_owned();
            if attr.name() == "id" {
                value = format!("{prefix}{value}");
            } else {
                for id in &ids {
                    value = value.replace(&format!("url(#{id})"), &format!("url(#{prefix}{id})"));
                    if value == format!("#{id}") {
                        value = format!("#{prefix}{id}");
                    }
                }
            }
            if value != attr.value() {
                edits.push((attr.range_value(), escape(&value)));
            }
        }
    }
    let root = xml.root_element();
    let range = root.children().next()?.range().start..root.children().last()?.range().end;
    let mut fragment = source[range.clone()].to_owned();
    for (r, value) in edits.into_iter().rev() {
        if r.start >= range.start && r.end <= range.end {
            fragment.replace_range(r.start - range.start..r.end - range.start, &value);
        }
    }
    Some(fragment)
}

pub(crate) fn caret_segment(run: &TypeRun, index: usize) -> (Pt, Pt) {
    let Some(layout) = layout(run) else {
        return (run.origin, run.origin);
    };
    let Some(caret) = layout.carets.iter().min_by_key(|c| c.0.abs_diff(index)) else {
        return (run.origin, run.origin);
    };
    (caret.1 + caret.3 * 0.9, caret.1 - caret.3 * 0.2)
}

pub(crate) fn selection_quads(run: &TypeRun, a: usize, b: usize) -> Vec<[Pt; 4]> {
    let Some(layout) = layout(run) else {
        return vec![];
    };
    let mut result = Vec::new();
    for index in a.min(b)..a.max(b) {
        let Some(start) = layout.carets.iter().find(|c| c.0 == index) else {
            continue;
        };
        let end = layout
            .carets
            .iter()
            .find(|c| c.0 == index + 1)
            .map_or(start.1 + start.2 * start.3.length() * 0.5, |c| c.1);
        result.push([
            start.1 + start.3 * 0.9,
            end + start.3 * 0.9,
            end - start.3 * 0.2,
            start.1 - start.3 * 0.2,
        ]);
    }
    result
}

fn texts(group: &usvg::Group, out: &mut Vec<usvg::Text>) {
    for node in group.children() {
        match node {
            usvg::Node::Text(t) => out.push((**t).clone()),
            usvg::Node::Group(g) => texts(g, out),
            _ => {}
        }
    }
}

struct VisualCharacter {
    index: usize,
    rtl: bool,
    px: f32,
    color: Option<crate::color::Rgba>,
}

fn solid(fill: Option<&usvg::Fill>) -> Option<crate::color::Rgba> {
    let fill = fill?;
    let usvg::Paint::Color(c) = fill.paint() else {
        return None;
    };
    Some(crate::color::Rgba::new(
        c.red,
        c.green,
        c.blue,
        (fill.opacity().get() * 255.).round() as u8,
    ))
}

fn visual_characters(nodes: &[usvg::Text], content: &str) -> Vec<VisualCharacter> {
    let mut result = Vec::new();
    let mut cursor = 0;
    for node in nodes {
        for chunk in node.chunks() {
            let text = chunk.text();
            if text.is_empty() {
                continue;
            }
            let clean: String = text.chars().filter(|c| !bidi_control(*c)).collect();
            let start = content[cursor..]
                .find(&clean)
                .map_or(cursor, |offset| cursor + offset);
            let base = content[..start].chars().count();
            let bidi = unicode_bidi::BidiInfo::new(text, Some(unicode_bidi::Level::ltr()));
            for paragraph in &bidi.paragraphs {
                let (levels, runs) = bidi.visual_runs(paragraph, paragraph.range.clone());
                for range in runs {
                    let rtl = levels[range.start].is_rtl();
                    let mut characters: Vec<_> = text[range.clone()]
                        .char_indices()
                        .map(|(offset, _)| range.start + offset)
                        .collect();
                    if rtl {
                        characters.reverse();
                    }
                    for byte in characters {
                        if text[byte..].chars().next().is_some_and(bidi_control) {
                            continue;
                        }
                        let span = chunk
                            .spans()
                            .iter()
                            .find(|s| s.start() <= byte && byte < s.end());
                        result.push(VisualCharacter {
                            index: base
                                + text[..byte].chars().filter(|c| !bidi_control(*c)).count(),
                            rtl,
                            px: span.map_or(16., |s| s.font_size().get()),
                            color: solid(span.and_then(|s| s.fill())),
                        });
                    }
                }
            }
            cursor = (start + clean.len()).min(content.len());
        }
    }
    result
}

fn bidi_control(c: char) -> bool {
    matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
}

/// Normalize SVG direction for the shaper while preserving semantic export.
/// Args: canonical SVG type source. Returns runtime SVG with bidi controls and equivalent anchors.
pub(crate) fn renderer_source(source: &str) -> String {
    let Ok(xml) = roxmltree::Document::parse(source) else {
        return source.into();
    };
    let nodes: Vec<_> = xml
        .descendants()
        .filter(|n| n.is_element() && matches!(n.tag_name().name(), "text" | "tspan" | "textPath"))
        .collect();
    let inherited = |node: roxmltree::Node<'_, '_>, key: &str, initial: &str| {
        node.ancestors()
            .filter_map(|n| n.attribute(key))
            .find(|v| !matches!(*v, "inherit" | "unset"))
            .unwrap_or(initial)
            .to_string()
    };
    let mut controls = Vec::new();
    let mut edits = Vec::new();
    for node in &nodes {
        let rtl = inherited(*node, "direction", "ltr") == "rtl";
        if rtl {
            let anchor = inherited(*node, "text-anchor", "start");
            let anchor = match anchor.as_str() {
                "start" => "end",
                "end" => "start",
                other => other,
            };
            if let Some(attr) = node.attributes().find(|a| a.name() == "text-anchor") {
                edits.push((attr.range_value(), anchor.into()));
            } else if let Some(end) = source[node.range()].find('>') {
                let at = node.range().start + end;
                edits.push((at..at, format!(" text-anchor=\"{anchor}\"")));
            }
        }
        let mode = inherited(*node, "unicode-bidi", "normal");
        if (node.has_tag_name("text")
            || node.attribute("unicode-bidi").is_some()
            || node.attribute("x").is_some()
            || node.attribute("y").is_some())
            && (rtl || mode != "normal")
            && node.children().next().is_some()
        {
            let isolate = mode.contains("isolate");
            let prefix = if isolate {
                if rtl { '\u{2067}' } else { '\u{2066}' }
            } else if mode.contains("override") {
                if rtl { '\u{202e}' } else { '\u{202d}' }
            } else if rtl {
                '\u{202b}'
            } else {
                '\u{202a}'
            };
            if let Some(open) = source[node.range()].find('>')
                && let Some(close) = source[node.range()].rfind("</")
            {
                controls.push((node.range().start + open + 1, prefix));
                controls.push((
                    node.range().start + close,
                    if isolate { '\u{2069}' } else { '\u{202c}' },
                ));
            }
        }
    }
    controls.sort_by_key(|(at, _)| *at);
    for node in nodes {
        let positions: Vec<_> = controls
            .iter()
            .filter(|(at, _)| node.range().contains(at))
            .map(|(at, _)| {
                node.descendants()
                    .filter(|n| n.is_text() && n.range().end <= *at)
                    .map(|n| n.text().unwrap_or("").chars().count())
                    .sum::<usize>()
            })
            .collect();
        for attr in node
            .attributes()
            .filter(|a| matches!(a.name(), "x" | "y" | "dx" | "dy" | "rotate"))
        {
            let mut values: Vec<_> = attr
                .value()
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .collect();
            let mut added = 0;
            for position in &positions {
                let at = position + added;
                if at < values.len() {
                    let dummy = if matches!(attr.name(), "x" | "y") {
                        values[at].clone()
                    } else {
                        "0".into()
                    };
                    values.insert(at, dummy);
                    added += 1;
                }
            }
            if added > 0 {
                edits.push((attr.range_value(), values.join(" ")));
            }
        }
    }
    edits.extend(controls.into_iter().map(|(at, c)| (at..at, c.to_string())));
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut result = source.to_owned();
    for (range, value) in edits {
        result.replace_range(range, &value);
    }
    result
}

pub(crate) fn layout(run: &TypeRun) -> Option<Arc<Layout>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Layout>>>> = OnceLock::new();
    let source = source(run)?;
    let cache = CACHE.get_or_init(Default::default);
    if let Some(found) = cache.lock().unwrap().get(&source).cloned() {
        return Some(found);
    }
    let tree = usvg::Tree::from_str(&renderer_source(&source), &options_for(run)).ok()?;
    let mut text_nodes = Vec::new();
    texts(tree.root(), &mut text_nodes);
    let order = visual_characters(&text_nodes, &run.content);
    let all: Vec<_> = run.content.chars().collect();
    let mut contours = Vec::new();
    let mut carets = Vec::new();
    let mut styles = Vec::new();
    let mut used = vec![false; run.content.chars().count()];
    let mut bounds: Option<crate::geom::Bounds> = None;
    let mut stroke_width = 0_f32;
    fn paths(group: &usvg::Group, contours: &mut Vec<Vec<Pt>>) {
        for node in group.children() {
            match node {
                usvg::Node::Path(p) => {
                    if let Some(path) = p.data().clone().transform(p.abs_transform())
                        && let Some(geom) = crate::formats::svg::path_geom(&path, true)
                    {
                        contours.extend(geom.contours(96));
                    }
                }
                usvg::Node::Group(g) => paths(g, contours),
                _ => {}
            }
        }
    }
    for text in &text_nodes {
        let b = text.abs_stroke_bounding_box();
        let b = crate::geom::Bounds::from_min_size(
            Pt::new(b.x(), b.y()),
            Pt::new(b.width(), b.height()),
        );
        bounds = Some(bounds.map_or(b, |old| old.union(b)));
        paths(text.flattened(), &mut contours);
        for span in text.layouted() {
            if let Some(stroke) = &span.stroke {
                let scale = text.abs_transform().get_scale();
                stroke_width = stroke_width.max(stroke.width().get() * scale.0.max(scale.1));
            }
            for glyph in &span.positioned_glyphs {
                let chars: Vec<_> = glyph.text.chars().filter(|c| !bidi_control(*c)).collect();
                if chars.is_empty() {
                    continue;
                }
                let matches =
                    |at: usize| at < all.len() && !used[at] && all[at..].starts_with(&chars);
                let character = order
                    .iter()
                    .find(|c| {
                        matches(c.index)
                            && (c.px - glyph.font_size()).abs() < 0.001
                            && c.color == solid(span.fill.as_ref())
                    })
                    .or_else(|| order.iter().find(|c| matches(c.index)));
                let at = character.map_or_else(
                    || (0..all.len()).find(|&at| matches(at)).unwrap_or(0),
                    |c| c.index,
                );
                let rtl = character.is_some_and(|c| c.rtl);
                for value in used.iter_mut().skip(at).take(chars.len()) {
                    *value = true;
                }
                let transform = text.abs_transform().pre_concat(glyph.transform());
                let font = tree
                    .fontdb()
                    .face(glyph.font)
                    .and_then(|f| match &f.source {
                        usvg::fontdb::Source::File(p) | usvg::fontdb::Source::SharedFile(p, _) => {
                            Some(p.to_string_lossy().into_owned())
                        }
                        _ => None,
                    });
                let fill = span.fill.as_ref().and_then(|fill| {
                    if let usvg::Paint::Color(c) = fill.paint() {
                        Some(crate::document::Fill::Solid(crate::color::Rgba::new(
                            c.red,
                            c.green,
                            c.blue,
                            (fill.opacity().get() * 255.).round() as u8,
                        )))
                    } else {
                        None
                    }
                });
                for i in 0..chars.len() {
                    styles.push((
                        at + i,
                        crate::geom::CharSpan {
                            font: font.clone(),
                            px: Some(glyph.font_size()),
                            fill: fill.clone(),
                            ..Default::default()
                        },
                    ));
                }
                let point = Pt::new(transform.tx, transform.ty);
                let tangent = Pt::new(transform.sx, transform.ky).normalized();
                let units = tree
                    .fontdb()
                    .with_face_data(glyph.font, |bytes, index| {
                        rustybuzz::ttf_parser::Face::parse(bytes, index)
                            .map_or(1000., |f| f.units_per_em() as f32)
                    })
                    .unwrap_or(1000.);
                let advance = tree
                    .fontdb()
                    .with_face_data(glyph.font, |bytes, index| {
                        rustybuzz::ttf_parser::Face::parse(bytes, index)
                            .ok()
                            .map(|font| {
                                font.glyph_hor_advance(glyph.id)
                                    .unwrap_or(font.units_per_em())
                                    as f32
                            })
                    })
                    .flatten()
                    .unwrap_or(units * 0.5);
                let advance = Pt::new(transform.sx, transform.ky) * advance;
                let origin = if rtl { point + advance } else { point };
                let advance = if rtl { -advance } else { advance };
                for i in 0..chars.len().max(1) {
                    carets.push((
                        at + i,
                        origin + advance * i as f32 / chars.len().max(1) as f32,
                        tangent,
                        Pt::new(-transform.kx, -transform.sy)
                            * tree
                                .fontdb()
                                .with_face_data(glyph.font, |bytes, index| {
                                    rustybuzz::ttf_parser::Face::parse(bytes, index)
                                        .map_or(1000., |f| f.units_per_em() as f32)
                                })
                                .unwrap_or(1000.),
                    ));
                }
                carets.push((
                    at + chars.len(),
                    origin + advance,
                    tangent,
                    Pt::new(-transform.kx, -transform.sy) * units,
                ));
            }
        }
    }
    let result = Arc::new(Layout {
        tree,
        contours,
        carets,
        styles,
        bounds: bounds.unwrap_or_else(|| crate::geom::Bounds::from_pt(run.map_point(run.origin))),
        stroke_width,
    });
    let mut cache = cache.lock().unwrap();
    if cache.len() >= 64 {
        cache.clear();
    }
    cache.insert(source, result.clone());
    Some(result)
}

fn flow_fragment(run: &TypeRun, flow: &Flow) -> String {
    let mut out = String::new();
    let mut start = 0;
    let count = run.content.chars().count();
    let mut row = 0;
    let svg = run.svg_text.as_ref().unwrap();
    let TextNode::Element { attributes, .. } = &svg.root else {
        return out;
    };
    out.push_str("<text");
    let mut css = String::new();
    for (key, value) in attributes {
        if !(PROPERTIES.contains(&key.as_str()) || key == "xml:space") || !safe_value(value) {
            continue;
        }
        if !matches!(
            key.as_str(),
            "x" | "y"
                | "dx"
                | "dy"
                | "data-oma-flow"
                | "data-oma-flow-source"
                | "inline-size"
                | "shape-inside"
                | "transform"
        ) {
            if CSS_PROPERTIES.contains(&key.as_str()) {
                css.push_str(&format!("{key}:{value};"));
                continue;
            }
            out.push_str(&format!(" {key}=\"{}\"", escape(value)));
        }
    }
    if !css.is_empty() {
        out.push_str(&format!(" style=\"{}\"", escape(&css)));
    }
    out.push_str(&format!(
        " data-oma-flow=\"{}\" data-oma-flow-source=\"{}\">",
        escape(&serde_json::to_string(flow).unwrap()),
        escape(
            &serde_json::to_string(&FlowSource {
                root: svg.root.clone(),
                content: run.content.clone()
            })
            .unwrap()
        )
    ));
    while start < count {
        let remaining = &run.content[super::char_to_byte(&run.content, start)..];
        let paragraph = remaining.split('\n').next().unwrap_or("");
        let mut ends: Vec<_> = unicode_linebreak::linebreaks(paragraph)
            .map(|(byte, _)| start + paragraph[..byte].chars().count())
            .collect();
        if ends.is_empty() {
            ends.push(start + paragraph.chars().count());
        }
        let measure = |end| {
            let fragment = plain_fragment(run, start..end);
            usvg::Tree::from_str(
                &wrap(&fragment, &svg.definitions, [1., 0., 0., 1., 0., 0.]),
                &options_for(run),
            )
            .ok()
            .and_then(|tree| {
                let mut nodes = Vec::new();
                texts(tree.root(), &mut nodes);
                nodes.first().map(|t| t.bounding_box().width())
            })
            .unwrap_or(0.)
        };
        let mut lo = 0;
        let mut hi = ends.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if measure(ends[mid]) <= flow.width {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let end = ends[lo.saturating_sub(1)];
        let fragment = plain_fragment(run, start..end);
        if let Ok(xml) = roxmltree::Document::parse(&fragment) {
            let node = xml.root_element();
            let inner = node
                .children()
                .next()
                .zip(node.children().last())
                .map_or("", |(first, last)| {
                    &fragment[first.range().start..last.range().end]
                });
            out.push_str(&format!(
                "<tspan x=\"{}\" y=\"{}\"{}>{inner}</tspan>",
                flow.x,
                flow.y + run.px + row as f32 * flow.leading,
                if run.px + row as f32 * flow.leading > flow.height {
                    " visibility=\"hidden\""
                } else {
                    ""
                }
            ));
        }
        row += 1;
        let previous = start;
        start = end;
        if run.content.chars().nth(start) == Some('\n') || start == previous {
            start += 1;
        }
    }
    out.push_str("</text>");
    out
}

/// Normalize rectangular flowed SVG into ordinary positioned SVG text.
/// Args: source SVG. Returns a rewritten source when flow is present, otherwise None.
pub(crate) fn prepare_flow(source: &str) -> Result<Option<String>, String> {
    let xml = roxmltree::Document::parse_with_options(
        source,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 400000,
            entity_resolver: None,
        },
    )
    .map_err(|e| e.to_string())?;
    let sheet = sheet(&xml);
    let mut changes = Vec::new();
    for node in xml
        .descendants()
        .filter(|n| n.has_tag_name("flowRoot") || n.has_tag_name("text"))
    {
        let attrs = attributes(node, &sheet);
        let rect = if node.has_tag_name("flowRoot") {
            node.descendants().find(|n| n.has_tag_name("rect"))
        } else {
            attrs
                .get("shape-inside")
                .and_then(|v| {
                    v.trim()
                        .strip_prefix("url(#")
                        .and_then(|v| v.strip_suffix(')'))
                })
                .and_then(|id| {
                    xml.descendants()
                        .find(|n| n.attribute("id") == Some(id) && n.has_tag_name("rect"))
                })
        };
        let number = |name: &str, default: f32| {
            attrs
                .get(name)
                .and_then(|v| v.trim_end_matches("px").parse::<f32>().ok())
                .unwrap_or(default)
        };
        let width = rect
            .and_then(|r| r.attribute("width").and_then(|v| v.parse::<f32>().ok()))
            .unwrap_or_else(|| number("inline-size", 0.));
        if width <= 0. {
            continue;
        }
        let px = number("font-size", 16.);
        let read = |name, default| {
            rect.and_then(|r| r.attribute(name).and_then(|v| v.parse::<f32>().ok()))
                .unwrap_or(default)
        };
        let flow = Flow {
            x: read("x", number("x", 0.)),
            y: read("y", number("y", 0.) - px),
            width,
            height: read("height", 100000.),
            leading: number("line-height", px * 1.2),
        };
        let (mut svg, content) = capture(&xml, node, String::new(), px);
        svg.flow = Some(flow.clone());
        let mut run = TypeRun {
            content,
            px,
            svg_text: Some(svg),
            ..Default::default()
        };
        let mut fragment = flow_fragment(&run, &flow);

        if let Some(transform) = node.attribute("transform") {
            fragment = format!("<g transform=\"{}\">{fragment}</g>", escape(transform));
        }
        changes.push((node.range(), fragment));
        run.content.clear();
    }
    if changes.is_empty() {
        return Ok(None);
    }
    let mut result = source.to_owned();
    for (range, text) in changes.into_iter().rev() {
        result.replace_range(range, &text);
    }
    Ok(Some(result))
}
