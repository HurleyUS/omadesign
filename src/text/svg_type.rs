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
#[cfg(test)]
pub(crate) mod metric_tests;
#[cfg(test)]
pub(crate) mod pair_tests;
#[cfg(test)]
mod fallback_tests;
#[cfg(test)]
pub(crate) mod optical_tests;
#[cfg(test)]
pub(crate) mod scale_tests;
mod positioning;
mod optical;
mod scaling;
mod fonts;
pub(crate) use fonts::family_css;
pub(super) use fonts::{family_faces,map_families,map_flow_families};
pub(crate) use fonts::{references,project_families};
pub(crate) use positioning::{pair_kerning,restore_offsets};
pub(crate) use scaling::restore as restore_scales;
pub(crate) use positioning::leading;

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
    #[serde(default = "default_viewport")]
    pub viewport: [f32; 2],
    #[serde(default = "empty_fill")]
    pub base_fill: crate::document::Fill,
    #[serde(default)]
    pub flow: Option<Flow>,
}

fn empty_fill() -> crate::document::Fill {
    crate::document::Fill::None
}

fn default_viewport() -> [f32; 2] {
    [100000., 100000.]
}

fn length(value: &str, reference: f32) -> Option<f32> {
    let value = value.trim();
    for (unit, scale) in [
        ("px", 1.),
        ("in", 96.),
        ("cm", 96. / 2.54),
        ("mm", 96. / 25.4),
        ("pt", 96. / 72.),
        ("pc", 16.),
        ("Q", 96. / 101.6),
        ("%", reference / 100.),
        ("em", reference),
    ] {
        if let Some(number) = value.strip_suffix(unit) {
            return number.trim().parse::<f32>().ok().map(|v| v * scale);
        }
    }
    value.parse().ok()
}

fn viewport(node: roxmltree::Node<'_, '_>) -> [f32; 2] {
    let mut size = [300., 150.];
    let mut ancestors: Vec<_> = node.ancestors().filter(|n| n.has_tag_name("svg")).collect();
    ancestors.reverse();
    for svg in ancestors {
        for (i, key) in ["width", "height"].into_iter().enumerate() {
            if let Some(value) = svg.attribute(key) {
                if let Some(percent) = value.strip_suffix('%').and_then(|v| v.parse::<f32>().ok()) {
                    size[i] *= percent / 100.;
                } else if let Some(value) = length(value, size[i]) {
                    size[i] = value;
                }
            }
        }
        if let Some(v) = svg.attribute("viewBox") {
            let values: Vec<f32> = v
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter_map(|s| s.parse().ok())
                .collect();
            if values.len() == 4 {
                size = [values[2], values[3]];
            }
        }
    }
    size
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    manual_kern: BTreeMap<usize, f32>,
}

struct GlyphMetric {
    index:usize,node:usize,chunk:usize,font:String,id:u16,px:f32,point:super::variations::FontVariations,
    transform:Transform,advance:f32,width:f32,allowed:bool,inline:Pt,baseline:Pt,
    length_scale:f32,unadjusted_advance:f32,upright:bool,letter_spacing:f32,word_spacing:f32,
    variations:Vec<([u8;4],f32)>,auto_opsz:bool,
}

pub(crate) struct Layout {
    pub tree: usvg::Tree,
    pub contours: Vec<Vec<Pt>>,
    pub carets: Vec<(usize, Pt, Pt, Pt)>,
    pub styles: Vec<(usize, crate::geom::CharSpan)>,
    pub bounds: crate::geom::Bounds,
    pub stroke_width: f32,
    characters: Vec<VisualCharacter>,
    glyphs:Vec<GlyphMetric>,
    clusters: Vec<std::ops::Range<usize>>,
}

impl Layout {
    /// Read actual font choices, including glyphs without their own Unicode cluster label.
    /// Args: none. Returns logical character indices and resolved native references.
    pub(crate) fn font_choices(&self)->impl Iterator<Item=(usize,&str)> {self.glyphs.iter().map(|g|(g.index,g.font.as_str()))}
}

pub(crate) fn paint_tree(run: &TypeRun, fill: bool) -> Option<Arc<usvg::Tree>> {
    static CACHE: OnceLock<Mutex<HashMap<(String, u64), Arc<usvg::Tree>>>> = OnceLock::new();
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
    let key = (source.clone(), super::FONT_REVISION.load(std::sync::atomic::Ordering::Relaxed));
    let cache = CACHE.get_or_init(Default::default);
    if let Some(tree) = cache.lock().unwrap().get(&key).cloned() {
        return Some(tree);
    }
    let tree = Arc::new(usvg::Tree::from_str(&renderer_source(&source), &options_for(run)).ok()?);
    let mut cache = cache.lock().unwrap();
    if cache.len() >= 64 {
        cache.clear();
    }
    cache.insert(key, tree.clone());
    Some(tree)
}

pub(crate) fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn inline_png(value: &str) -> bool {
    let Some(data) = value.strip_prefix("data:image/png;base64,") else {
        return false;
    };
    if data.len() > 90 * 1024 * 1024 {
        return false;
    }
    let Ok(bytes) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data) else {
        return false;
    };
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return false;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 64 * 1024 * 1024
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
        if name == "data-oma-object-paint" {
            continue;
        }
        if name.to_ascii_lowercase().starts_with("on") || matches!(name, "id" | "class" | "style") {
            continue;
        }
        if name == "href"
            && !a.value().starts_with('#')
            && !(node.has_tag_name("image") && inline_png(a.value()))
        {
            continue;
        }
        attrs.insert(name.into(), a.value().into());
    }
    if let Some(value) = crate::formats::svg::cascaded_property(node, sheet, "opacity") {
        attrs.insert("opacity".into(), value);
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
    "text-rendering",
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
                | "image"
        )
    {
        return;
    }
    let tag = node.tag_name().name();
    if tag == "image"
        && !node
            .attributes()
            .any(|a| a.name() == "href" && inline_png(a.value()))
    {
        return;
    }
    out.push_str(&format!("<{tag}"));
    if let Some(id) = node.attribute("id") {
        out.push_str(&format!(" id=\"{}\"", escape(id)));
    }
    for (key, value) in attributes(node, sheet) {
        if !safe_value(&value) && !(tag == "image" && key == "href" && inline_png(&value)) {
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
) -> (SvgText, String, BTreeMap<usize, f32>) {
    let sheet = sheet(xml);
    let mut content = String::new();
    let mut manual_kern = BTreeMap::new();
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
        if let TextNode::Element{attributes,..}=&mut root {for key in [positioning::META,scaling::META] {if let Some(value)=node.attribute(key){attributes.insert(key.into(),value.into());}}}
        content = saved.content;
        let count = content.chars().count();
        manual_kern = saved.manual_kern.into_iter().filter(|(at,value)| *at > 0 && *at < count && value.is_finite()).collect();
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
    for defs in xml.descendants().filter(|n| {
        n.has_tag_name("defs") && !n.ancestors().skip(1).any(|a| a.has_tag_name("defs"))
    }) {
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
            viewport: viewport(node),
            base_fill: crate::document::Fill::None,
            flow,
        },
        content,
        manual_kern,
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

fn font_attributes(path: &str) -> String {
    let probe = TypeRun {
        font: path.into(),
        ..Default::default()
    };
    let mut attrs = format!(
        " font-family=\"{}\"",
        escape(&format!(
            "'{}'",
            path.replace('\\', "\\\\").replace('\'', "\\'")
        ))
    );
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

/// Find the installed family of an exact font program.
/// Args: native font reference. Returns a family only for an identical installed face.
fn installed_family(path: &str) -> Option<String> {
    static CACHE: OnceLock<Mutex<HashMap<(u64,u64,String),Option<String>>>>=OnceLock::new();
    let key=(super::INSTALLED_REVISION.load(std::sync::atomic::Ordering::Relaxed),super::FONT_REVISION.load(std::sync::atomic::Ordering::Relaxed),path.to_owned());
    let cache=CACHE.get_or_init(Default::default);
    if let Some(value)=cache.lock().unwrap().get(&key){return value.clone();}
    let options=options();
    let bytes=super::project_font_bytes(path);
    let names:Vec<_>=bytes.as_ref().and_then(|b|rustybuzz::ttf_parser::Face::parse(b,0).ok()).map(|face|face.names().into_iter().filter(|n|matches!(n.name_id,1|16)).filter_map(|n|n.to_string()).collect()).unwrap_or_default();
    let family=options.fontdb.faces().find_map(|face| {
        let (usvg::fontdb::Source::File(file) | usvg::fontdb::Source::SharedFile(file,_)) = &face.source else {return None;};
        let exact=file.to_string_lossy()==path || bytes.as_ref().is_some_and(|bytes| {
            face.families.iter().any(|(name,_)|names.iter().any(|n|n.eq_ignore_ascii_case(name))) && options.fontdb.with_face_data(face.id,super::font_catalog::face_bytes).is_some_and(|result|result.is_ok_and(|candidate|candidate.as_slice()==bytes.as_slice()))
        });
        exact.then(||face.families.first().map(|(name,_)|name.clone())).flatten()
    });
    let mut cache=cache.lock().unwrap();if cache.len()>=256{cache.clear();}cache.insert(key,family.clone());family
}

fn quoted_family(name: &str) -> String {
    format!("'{}'", name.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn overrides_font(run: &TypeRun) -> bool {
    run.svg_text
        .as_ref()
        .is_some_and(|svg| run.font != svg.base_font || !run.font_variations.is_empty())
}

fn paint_id(fill: &crate::document::Fill) -> String {
    format!(
        "oma-range-paint-{:032x}",
        crate::typography::fingerprint(serde_json::to_string(fill).unwrap().as_bytes())
    )
}

/// Isolate source text paint from the surrounding object wrapper.
/// Args: captured root attributes and SVG output. Returns no value; emits only absent SVG defaults.
fn paint_defaults(attributes:&BTreeMap<String,String>, out:&mut String) {
    for (name,value) in [("fill","#000000"),("fill-opacity","1")] {
        if !attributes.contains_key(name){out.push_str(&format!(" {name}=\"{value}\""));}
    }
}

fn write_nodes(
    node: &TextNode,
    run: &TypeRun,
    cursor: &mut usize,
    range: std::ops::Range<usize>,
    out: &mut String,
    root: bool,
    leaf_bound: bool,
    metrics: Option<&Layout>,
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
            if leaf_bound {out.push_str(&escape(&run.content[super::char_to_byte(&run.content,a)..super::char_to_byte(&run.content,b)]));return;}
            for (span, split_pair) in positioning::spans(run, a, b, metrics).into_iter().flat_map(|(span,split)|scaling::spans(span,metrics).into_iter().map(move |span|(span,split))) {
                let inherited = metrics.and_then(|layout|layout.styles.iter().find(|(at,_)|*at==span.start)).map(|(_,style)|style);
                let text = &run.content[super::char_to_byte(&run.content, span.start)
                    ..super::char_to_byte(&run.content, span.end)];
                let mut attrs = String::new();
                let mut css = String::new();
                let mut features = if span.kerning.is_some() || split_pair {inherited.map(|s|s.features.iter().copied().collect::<BTreeMap<_,_>>()).unwrap_or_default()}else{BTreeMap::new()};
                features.extend(span.features.iter().copied());
                if split_pair {for tag in [*b"liga",*b"clig",*b"dlig",*b"hlig"] {features.insert(tag,0);}}
                if let Some(mode) = span.kerning {
                    features.insert(*b"kern",u32::from(mode==crate::geom::KernMode::Metrics));
                    css.push_str(if mode==crate::geom::KernMode::Metrics {"font-kerning:normal;"} else {"font-kerning:none;"});
                }
                if !features.is_empty(){css.push_str(&format!("font-feature-settings:{};",super::feature_css(&features.into_iter().collect::<Vec<_>>())));}
                if span.font_variations.is_some() {
                    let axes = super::variations::at(run, span.start);
                    css.push_str(&format!("font-variation-settings:{};", if axes.is_empty() { "normal".into() } else { super::variations::css(&axes) }));
                }
                if let Some(font) = &span.font {
                    attrs.push_str(&font_attributes(font));
                }
                if let Some(px) = span.px {
                    attrs.push_str(&format!(" font-size=\"{px}\""));
                }
                if let Some(fill) = &span.fill {
                    match fill {
                        crate::document::Fill::Solid(c) => {
                            attrs.push_str(&format!(" fill=\"{}\" fill-opacity=\"1\"", c.css()))
                        }
                        crate::document::Fill::None => attrs.push_str(" fill=\"none\""),
                        _ => attrs.push_str(&format!(
                            " fill=\"url(#{})\" fill-opacity=\"{}\"",
                            paint_id(fill),
                            crate::svg::paint_opacity(fill)
                        )),
                    }
                }
                if let Some(shift) = span.baseline_shift {
                    let shift=shift-inherited.and_then(|s|s.baseline_shift).unwrap_or(0.);
                    attrs.push_str(&format!(" baseline-shift=\"{shift}\""));
                }
                if let Some(tracking) = span.tracking {
                    attrs.push_str(&format!(
                        " letter-spacing=\"{}\"",
                        tracking * span.px.or_else(||inherited.and_then(|s|s.px)).unwrap_or(run.px) / 1000.
                    ));
                }
                if let Some(leading) = span.leading {
                    css.push_str(&format!("line-height:{};",match leading {
                        crate::geom::Leading::Auto(value) => format!("{value}%"),
                        crate::geom::Leading::Fixed(value) => format!("{value}px"),
                    }));
                }
                if span.hscale.is_some() || span.vscale.is_some() {
                    attrs.push_str(&format!(" {}=\"{}\"",scaling::MARKER,escape(&scaling::marker(&span))));
                }
                if !css.is_empty() { attrs.push_str(&format!(" style=\"{}\"", escape(&css))); }
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
            let count=children.iter().map(|n|if let TextNode::Characters{count}=n {Some(*count)} else {None}).collect::<Option<Vec<_>>>().and_then(|counts|counts.into_iter().try_fold(0usize,usize::checked_add));
            let leaf_override=count.filter(|count|*count>0 && *cursor>=range.start && cursor.checked_add(*count).is_some_and(|end|end<=range.end)).and_then(|count|{
                if run.manual_kern.values().any(|v|*v!=0.) {return None;}
                let spans=run.character_style_runs(*cursor,*cursor+count);
                if spans.len()!=1{return None;}let style=spans[0].clone();
                if (style.hscale.is_some()||style.vscale.is_some()) && scaling::spans(style.clone(),metrics).len()!=1 {return None;}
                if style.font.is_none() && style.font_variations.is_none() && style.features.is_empty() && style.hscale.is_none() && style.vscale.is_none() && style.kerning.is_none(){return None;}
                let mut rest=style.clone();rest.start=0;rest.end=0;rest.font=None;rest.font_variations=None;rest.features.clear();rest.hscale=None;rest.vscale=None;rest.kerning=None;
                (rest==crate::geom::CharSpan::default()).then_some(style)
            });
            out.push_str(&format!("<{name}"));
            if root {paint_defaults(attributes,out);}
            let mut css = String::new();
            for (key, value) in attributes {
                if leaf_override.as_ref().is_some_and(|s| s.font.is_some()) && matches!(key.as_str(),"font-family"|"font-weight"|"font-style") {continue;}
                if leaf_override.as_ref().is_some_and(|s| s.font_variations.is_some()) && key=="font-variation-settings" {continue;}
                if leaf_override.as_ref().is_some_and(|s|!s.features.is_empty()||s.kerning.is_some()) && key=="font-feature-settings" {continue;}
                if root
                    && overrides_font(run)
                    && matches!(key.as_str(), "font-family" | "font-weight" | "font-style")
                    || root && !run.font_variations.is_empty() && key == "font-variation-settings"
                {
                    continue;
                }
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
                            | "startOffset"
                            | "method"
                            | "spacing"
                            | "side"
                            | "xml:space"
                            | "opacity"
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
                } else {
                    value.clone()
                };
                if CSS_PROPERTIES.contains(&key.as_str()) {
                    css.push_str(&format!("{key}:{value};"));
                    continue;
                }
                out.push_str(&format!(" {key}=\"{}\"", escape(&value)));
            }
            if root && overrides_font(run) && !leaf_override.as_ref().is_some_and(|s|s.font.is_some()) {
                out.push_str(&font_attributes(&run.font));
                if !run.font_variations.is_empty() { css.push_str(&format!("font-variation-settings:{};", super::variations::css(&run.font_variations))); }
            }
            if let Some(style)=&leaf_override {
                if style.hscale.is_some() || style.vscale.is_some() {out.push_str(&format!(" {}=\"{}\"",scaling::MARKER,escape(&scaling::marker(style))));}
                let mut features=if style.kerning.is_some(){metrics.and_then(|m|m.styles.iter().find(|(at,_)|*at==*cursor)).map(|(_,s)|s.features.iter().copied().collect::<BTreeMap<_,_>>()).unwrap_or_default()}else{BTreeMap::new()};
                features.extend(style.features.iter().copied());
                if let Some(mode)=style.kerning {features.insert(*b"kern",u32::from(mode==crate::geom::KernMode::Metrics));css.push_str(if mode==crate::geom::KernMode::Metrics{"font-kerning:normal;"}else{"font-kerning:none;"});}
                if !features.is_empty(){css.push_str(&format!("font-feature-settings:{};",super::feature_css(&features.into_iter().collect::<Vec<_>>())));}
                if let Some(font)=&style.font {out.push_str(&font_attributes(font));}
                if style.font_variations.is_some(){let axes=super::variations::at(run,*cursor);css.push_str(&format!("font-variation-settings:{};",if axes.is_empty(){"normal".into()}else{super::variations::css(&axes)}));}
            }
            if !css.is_empty() {
                out.push_str(&format!(" style=\"{}\"", escape(&css)));
            }
            out.push('>');
            for child in children {
                write_nodes(child, run, cursor, range.clone(), out, false, leaf_override.is_some(), metrics);
            }
            out.push_str(&format!("</{name}>"));
        }
    }
}

/// Share SVG font selection and external resource restrictions.
/// Args: none. Returns options using one installed-font database and cached native font bytes.
pub(crate) fn options() -> usvg::Options<'static> {
    static DB: OnceLock<std::sync::Mutex<Option<(u64, Arc<usvg::fontdb::Database>)>>> = OnceLock::new();
    let mut options = usvg::Options::default();
    let revision = super::INSTALLED_REVISION.load(std::sync::atomic::Ordering::Relaxed);
    let mut cached = DB.get_or_init(|| std::sync::Mutex::new(None)).lock().unwrap_or_else(|e| e.into_inner());
    if cached.as_ref().is_none_or(|(r, _)| *r != revision) {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            db.load_fonts_dir(crate::project::data_dir().join("fonts"));
            if let Some(fonts) = super::DYNAMIC_FONTS.get() && let Ok(fonts) = fonts.lock() {
                for font in fonts.iter() { let _ = db.load_font_file(&font.path); }
            }
            for generic in ["serif", "sans-serif", "monospace", "cursive", "fantasy"] {
                if let Ok(output) = std::process::Command::new("fc-match")
                    .args(["-f", "%{family[0]}", generic])
                    .output()
                    && output.status.success()
                {
                    let family = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                    if !db
                        .faces()
                        .any(|f| f.families.iter().any(|(n, _)| n == &family))
                    {
                        continue;
                    }
                    match generic {
                        "serif" => db.set_serif_family(family),
                        "sans-serif" => db.set_sans_serif_family(family),
                        "monospace" => db.set_monospace_family(family),
                        "cursive" => db.set_cursive_family(family),
                        _ => db.set_fantasy_family(family),
                    }
                }
            }
            *cached = Some((revision, Arc::new(db)));
    }
    options.fontdb = cached.as_ref().unwrap().1.clone();
    drop(cached);
    let selector = usvg::FontResolver::default_font_selector();
    options.font_resolver.select_font = Box::new(move |font, db| {
        let requested = font.families().iter().any(|family| match family {
            usvg::FontFamily::Named(name) => db.faces().any(|face| {
                face.families
                    .iter()
                    .any(|(n, _)| n.eq_ignore_ascii_case(name))
            }),
            _ => true,
        });
        if !requested
            && let Some(path) = substituted_font(font).or_else(super::default_path)
            && let Some(face) = db.faces().find(|face| match &face.source {
                usvg::fontdb::Source::File(p) | usvg::fontdb::Source::SharedFile(p, _) => {
                    *p == path
                }
                _ => false,
            })
            && let Some((family, _)) = face.families.first()
        {
            return db.query(&usvg::fontdb::Query {
                families: &[usvg::fontdb::Family::Name(family)],
                weight: usvg::fontdb::Weight(font.weight()),
                style: font.style().into(),
                stretch: font.stretch().into(),
                ..Default::default()
            });
        }
        selector(font, db)
    });
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    options
}

fn substituted_font(font: &usvg::Font) -> Option<std::path::PathBuf> {
    substituted_path(font.families(),font.weight(),font.style())
}

/// Reuse the shared fontconfig substitution for unavailable families.
/// Args: ordered families, weight and style. Returns the cached actual font path.
fn substituted_path(families:&[usvg::FontFamily],weight:u16,style:usvg::FontStyle)->Option<std::path::PathBuf> {
    static MATCHES: OnceLock<Mutex<HashMap<String, Option<std::path::PathBuf>>>> = OnceLock::new();
    let family = families.iter()
        .map(|family| match family {
            usvg::FontFamily::Named(name) => name
                .replace('\\', "\\\\")
                .replace(':', "\\:")
                .replace(',', "\\,")
                .replace('=', "\\=")
                .replace('-', "\\-"),
            generic => generic.to_string(),
        })
        .collect::<Vec<_>>()
        .join(",");
    let weight = match weight {
        0..150 => "thin",
        150..250 => "extralight",
        250..350 => "light",
        350..450 => "regular",
        450..550 => "medium",
        550..650 => "demibold",
        650..750 => "bold",
        750..850 => "extrabold",
        _ => "black",
    };
    let slant = match style {
        usvg::FontStyle::Normal => "roman",
        usvg::FontStyle::Italic => "italic",
        usvg::FontStyle::Oblique => "oblique",
    };
    let pattern = format!("{family}:weight={weight}:slant={slant}");
    let matches = MATCHES.get_or_init(Default::default);
    if let Some(path) = matches.lock().unwrap().get(&pattern) {
        return path.clone();
    }
    let path = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", &pattern])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from);
    let mut matches = matches.lock().unwrap();
    if matches.len() >= 64 {
        matches.clear();
    }
    matches.insert(pattern, path.clone());
    path
}

fn options_for(run: &TypeRun) -> usvg::Options<'static> {
    let mut options = options();
    bind_fonts(run, &mut options);
    options
}

fn bind_fonts(run: &TypeRun, options: &mut usvg::Options<'_>) {
    let mut fonts: std::collections::BTreeSet<String> = std::iter::once(&run.font)
        .chain(run.spans.iter().filter_map(|s| s.font.as_ref()))
        .cloned()
        .collect();
    fn retained(node: &TextNode, fonts: &mut std::collections::BTreeSet<String>) {
        if let TextNode::Element { attributes, children, .. } = node {
            if let Some(value) = attributes.get("font-family") {
                for family in usvg::parse_font_families(value).unwrap_or_default() {
                    let usvg::FontFamily::Named(family)=family else {continue;};
                    let cached=super::BYTES.get().and_then(|c|c.lock().ok().map(|c|c.contains_key(std::path::Path::new(&family)))).unwrap_or(false);
                    if family.starts_with("omatype:") || cached {fonts.insert(family);}
                }
            }
            for child in children {retained(child,fonts);}
        }
    }
    if let Some(svg) = &run.svg_text {retained(&svg.root,&mut fonts);}
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
}

fn plain_fragment(run: &TypeRun, range: std::ops::Range<usize>) -> String {
    let metrics=(run.manual_kern.values().any(|v|*v!=0.) || run.spans.iter().any(|s|s.tracking.is_some()||s.baseline_shift.is_some()||s.kerning.is_some()||s.hscale.is_some()||s.vscale.is_some())).then(||{
        let mut original=run.clone();
        original.manual_kern.clear();
        for style in &mut original.spans {style.tracking=None;style.baseline_shift=None;style.kerning=None;style.hscale=None;style.vscale=None;}
        layout(&original)
    }).flatten();
    let mut out = String::new();
    write_nodes(
        &run.svg_text.as_ref().unwrap().root,
        run,
        &mut 0,
        range,
        &mut out,
        true,
        false,
        metrics.as_deref(),
    );
    out
}

fn wrap(fragment: &str, definitions: &str, matrix: [f32; 6], viewport: [f32; 2]) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{}\" height=\"{}\" overflow=\"visible\" text-rendering=\"geometricPrecision\"><defs>{definitions}</defs><g transform=\"matrix({} {} {} {} {} {})\">{fragment}</g></svg>",
        viewport[0], viewport[1], matrix[0], matrix[1], matrix[2], matrix[3], matrix[4], matrix[5]
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
    let mut definitions = safe_definitions(&svg.definitions);
    let mut bounds = run.contours.iter().flatten().copied();
    let mut b =
        crate::geom::Bounds::from_pt(bounds.next().unwrap_or_else(|| run.map_point(run.origin)));
    for point in bounds {
        b.union_pt(point);
    }
    let mut seen = std::collections::HashSet::new();
    let object_fill = if let TextNode::Element { attributes, .. } = &svg.root {
        attributes
            .get("data-oma-object-paint")
            .and_then(|v| serde_json::from_str::<crate::document::Fill>(v).ok())
    } else {
        None
    };
    for fill in run
        .spans
        .iter()
        .filter_map(|s| s.fill.as_ref())
        .chain(object_fill.as_ref())
    {
        let id = paint_id(fill);
        if seen.insert(id.clone()) {
            definitions.push_str(&crate::svg::type_paint_definition(
                fill,
                b,
                &id,
                matrix.invert().unwrap_or_default(),
                &run.contours,
            ));
        }
    }
    let source = wrap(
        &fragment,
        &definitions,
        [
            matrix.sx, matrix.ky, matrix.kx, matrix.sy, matrix.tx, matrix.ty,
        ],
        svg.viewport,
    );
    let source=scaling::apply(run,source,0,&run.content)?;
    positioning::pairs(run, source, 0, &run.content)
}

/// Apply inherited object paint to a temporary SVG source view.
/// Args: native run and object fill. Returns a clone retaining explicit source and native range paints.
pub(crate) fn object_paint(run: &TypeRun, fill: &crate::document::Fill) -> TypeRun {
    let mut run = run.clone();
    if let Some(SvgText {
        root: TextNode::Element { attributes, .. },
        ..
    }) = &mut run.svg_text
    {
        let value = match fill {
            crate::document::Fill::None => "none".into(),
            crate::document::Fill::Solid(c) => c.css(),
            _ => format!("url(#{})", paint_id(fill)),
        };
        attributes.insert("fill".into(), value);
        attributes.insert("fill-opacity".into(), crate::svg::paint_opacity(fill).to_string());
        attributes.insert(
            "data-oma-object-paint".into(),
            serde_json::to_string(fill).unwrap(),
        );
    }
    run
}

pub(crate) fn export_fragment(run: &TypeRun, id: u64, outlines: bool) -> Option<String> {
    export_fragment_inner(run, id, outlines, false)
}

pub(crate) fn export_fragment_embedded(run: &TypeRun, id: u64) -> Option<String> {
    export_fragment_inner(run, id, false, true)
}

fn export_fragment_inner(run: &TypeRun, id: u64, outlines: bool, embedded: bool) -> Option<String> {
    let mut delivery;
    let run=if !embedded && !outlines {
        delivery=run.clone();
        for(at,style)in &layout(run)?.styles {if let Some(font)=&style.font {delivery.set_character_style(*at,at+1,|span|span.font=Some(font.clone()));}}
        &delivery
    } else {run};
    let original = source(run)?;
    let identities:std::collections::BTreeSet<_>=std::iter::once(run.font.clone()).chain(run.spans.iter().filter_map(|s|s.font.clone())).chain(references(run.svg_text.as_ref().unwrap())).collect();
    let fonts:std::collections::BTreeMap<_,_>=identities.into_iter().map(|font|{let family=installed_family(&font);(font,family)}).collect();
    let portable=fonts.iter().any(|(font,family)|family.is_none()&&(font.starts_with("omatype:") || font!=&run.svg_text.as_ref().unwrap().base_font));
    let family_list=|value:&str|usvg::parse_font_families(value).map(|list|list.into_iter().map(|family| match family {
        usvg::FontFamily::Named(name)=>quoted_family(fonts.get(&name).and_then(Option::as_deref).unwrap_or(&name)),
        other=>other.to_string(),
    }).collect::<Vec<_>>().join(", ")).unwrap_or_else(|_|value.to_owned());
    let source = if outlines || portable && !embedded {
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
            if attr.name()=="font-family" && !embedded {value=family_list(&value);}
            if attr.name()=="data-oma-flow-source" && !embedded {value=map_flow_families(&value,|style|Ok(family_list(style.get("font-family").map_or("serif",String::as_str)))).ok()?;}
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
    let range = 0..source.len();
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
    node:usize,chunk:usize,text_index:usize,
    index: usize,
    ordinal: usize,
    dx: f32,
    dy: f32,
    vertical: bool,
    line_start: bool,
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
    let mut ordinal = 0;
    for (node_index,node) in nodes.iter().enumerate() {
        let mut offset = 0;
        for (chunk_index,chunk) in node.chunks().iter().enumerate() {
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
                            node:node_index,chunk:chunk_index,text_index:offset+text[..byte].chars().count(),
                            index: base
                                + text[..byte].chars().filter(|c| !bidi_control(*c)).count(),
                            ordinal: ordinal + text[..byte].chars().filter(|c| !bidi_control(*c)).count(),
                            dx: node.dx().get(offset + text[..byte].chars().count()).copied().unwrap_or(0.),
                            dy: node.dy().get(offset + text[..byte].chars().count()).copied().unwrap_or(0.),
                            vertical: node.writing_mode() == usvg::WritingMode::TopToBottom && matches!(chunk.text_flow(),usvg::TextFlow::Linear),
                            line_start: text[..byte].chars().all(bidi_control),
                            rtl,
                            px: span.map_or(16., |s| s.font_size().get()),
                            color: solid(span.and_then(|s| s.fill())),
                        });
                    }
                }
            }
            cursor = (start + clean.len()).min(content.len());
            offset += text.chars().count();
            ordinal += clean.chars().count();
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
                if matches!(attr.name(), "x" | "y") && *position==0 && values.len()==1 {
                    continue;
                }
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
    layout_source(run, source(run)?, &run.content)
}

fn layout_source(run: &TypeRun, source: String, content: &str) -> Option<Arc<Layout>> {
    static CACHE: OnceLock<Mutex<HashMap<(String, String, u64), Arc<Layout>>>> = OnceLock::new();
    let key = (source.clone(), content.to_owned(), super::FONT_REVISION.load(std::sync::atomic::Ordering::Relaxed));
    let cache = CACHE.get_or_init(Default::default);
    if let Some(found) = cache.lock().unwrap().get(&key).cloned() {
        return Some(found);
    }
    let tree = usvg::Tree::from_str(&renderer_source(&source), &options_for(run)).ok()?;
    let scales=scaling::resolved(&source);
    let result = Arc::new(layout_tree(run, content, tree, false,&scales));
    let mut cache = cache.lock().unwrap();
    if cache.len() >= 64 { cache.clear(); }
    cache.insert(key, result.clone());
    Some(result)
}

/// Resolve imported text against its already loaded, bounded embedded font database.
/// Args: captured text and actual SVG parser options. Returns the same native layout and resolved font ranges.
pub(crate) fn layout_with_options(run: &TypeRun, options: &usvg::Options<'_>) -> Option<Layout> {
    let mut resolved=self::options();
    resolved.fontdb=options.fontdb.clone();
    bind_fonts(run,&mut resolved);
    let source = source(run)?;
    let scales=scaling::resolved(&source);
    let source=renderer_source(&source);
    Some(layout_tree(run, &run.content, usvg::Tree::from_str(&source, &resolved).ok()?, true,&scales))
}

fn layout_tree(run: &TypeRun, content: &str, tree: usvg::Tree, embedded: bool, scales:&[scaling::Scale]) -> Layout {
    let mut text_nodes = Vec::new();
    texts(tree.root(), &mut text_nodes);
    let order = visual_characters(&text_nodes, content);
    let all: Vec<_> = content.chars().collect();
    let mut contours = Vec::new();
    let mut glyph_metrics=Vec::new();
    let mut carets = Vec::new();
    let mut starts = std::collections::HashSet::new();
    let mut styles = Vec::new();
    let mut clusters = Vec::new();
    let mut resolved_fonts = HashMap::new();
    let mut used = vec![false; all.len()];
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
    for (node_index,text) in text_nodes.iter().enumerate() {
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
                let (font, axes) = resolved_fonts.entry(glyph.font).or_insert_with(|| {
                    let portable=embedded || tree.fontdb().face(glyph.font).is_some_and(|face|face.families.iter().any(|(name,_)|name.starts_with("omatype:")));
                    let font = if portable { super::webfonts::reference(tree.fontdb(), glyph.font) } else { tree.fontdb().face(glyph.font).and_then(|f|match &f.source {usvg::fontdb::Source::File(p)|usvg::fontdb::Source::SharedFile(p,_)=>Some(p.to_string_lossy().into_owned()),_=>None}) };
                    let axes = if portable {tree.fontdb().with_face_data(glyph.font, |bytes,index| rustybuzz::ttf_parser::Face::parse(bytes,index).ok().map(|face|face.variation_axes().into_iter().map(|a|a.tag.to_bytes()).collect::<Vec<_>>())).flatten().unwrap_or_default()} else {Vec::new()};
                    (font, axes)
                });
                if let Some(character)=order.iter().find(|c|c.node==node_index&&c.text_index==glyph.text_index) {
                    if let Some(font)=font {
                        let (units,point)=tree.fontdb().with_face_data(glyph.font,|bytes,index| {
                            let face=rustybuzz::ttf_parser::Face::parse(bytes,index).ok()?;
                            let mut values:Vec<_>=span.variations.iter().map(|v|(v.tag,v.value)).collect();
                            if span.font_optical_sizing==usvg::FontOpticalSizing::Auto && !values.iter().any(|(tag,_)|*tag==*b"opsz") && let Some(axis)=face.variation_axes().into_iter().find(|a|a.tag.to_bytes()==*b"opsz") {values.push((*b"opsz",glyph.font_size().clamp(axis.min_value,axis.max_value)));}
                            Some((face.units_per_em() as f32,super::variations::FontVariations::new(values).ok()?))
                        }).flatten().unwrap_or((1000.,Default::default()));
                        let compensation=scales.iter().find(|s|s.contains(character.index));
                        let font_factor=compensation.map_or(1.,|s|s.font_factor);
                        glyph_metrics.push(GlyphMetric{index:character.index,node:node_index,chunk:character.chunk,font:font.clone(),id:glyph.id.0,px:glyph.font_size()/font_factor,point,transform:glyph.transform().pre_scale(units,units),advance:glyph.cluster_advance,width:glyph.cluster_width,allowed:glyph.spacing_allowed,inline:Pt::new(glyph.inline_direction[0],glyph.inline_direction[1]),baseline:Pt::new(glyph.baseline[0],glyph.baseline[1]),length_scale:glyph.length_scale,unadjusted_advance:glyph.unadjusted_advance,upright:glyph.vertical_upright,letter_spacing:span.letter_spacing,word_spacing:span.word_spacing,variations:span.variations.iter().map(|v|(v.tag,v.value)).collect(),auto_opsz:span.font_optical_sizing==usvg::FontOpticalSizing::Auto});
                    }
                }
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
                if chars.len()>1 {clusters.push(at..at+chars.len());}
                for value in used.iter_mut().skip(at).take(chars.len()) {
                    *value = true;
                }
                let transform = text.abs_transform().pre_concat(glyph.transform());
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
                let units = tree
                    .fontdb()
                    .with_face_data(glyph.font, |bytes, index| {
                        rustybuzz::ttf_parser::Face::parse(bytes, index)
                            .map_or(1000., |f| f.units_per_em() as f32)
                    })
                    .unwrap_or(1000.);
                let scale=glyph.transform().get_scale();
                let compensation=scales.iter().find(|s|s.contains(at));
                let font_factor=compensation.map_or(1.,|s|s.font_factor);
                let inline_factor=compensation.map_or(1.,|s|s.inline_factor);
                let base_px=glyph.font_size()/font_factor;
                for i in 0..chars.len() {
                    styles.push((
                        at + i,
                        crate::geom::CharSpan {
                            font: font.clone(),
                            features: span.features.clone(),
                            font_variations: (!axes.is_empty() && font.as_ref().is_some_and(|f|f.starts_with("omatype:"))).then(|| super::variations::FontVariations::new(span.variations.iter().filter(|v|axes.contains(&v.tag)).map(|v|(v.tag,v.value)).collect()).ok()).flatten(),
                            px: Some(base_px),
                            tracking: Some(span.letter_spacing * inline_factor / base_px * 1000.),
                            baseline_shift: Some(span.baseline_shift),
                            kerning: Some(if span.features.iter().rev().find(|(tag,_)|*tag==*b"kern").map_or(span.apply_kerning,|(_,value)|*value!=0) {crate::geom::KernMode::Metrics} else {crate::geom::KernMode::None}),
                            hscale: Some(scale.0 / (base_px/units) * 100.),
                            vscale: Some(scale.1 / (base_px/units) * 100.),
                            fill: fill.clone(),
                            ..Default::default()
                        },
                    ));
                }
                let point = Pt::new(transform.tx, transform.ty);
                let tangent = Pt::new(transform.sx, transform.ky).normalized();
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
                    if !starts.insert(at + i) {
                        continue;
                    }
                    carets.retain(|caret: &(usize, Pt, Pt, Pt)| caret.0 != at + i);
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
                if !carets.iter().any(|c| c.0 == at + chars.len()) {
                    carets.push((
                        at + chars.len(),
                        origin + advance,
                        tangent,
                        Pt::new(-transform.kx, -transform.sy) * units,
                    ));
                }
            }
        }
    }
    Layout {
        tree,
        contours,
        carets,
        styles,
        bounds: bounds.unwrap_or_else(|| crate::geom::Bounds::from_pt(run.map_point(run.origin))),
        stroke_width,
        characters: order,
        glyphs:glyph_metrics,
        clusters,
    }
}

fn flow_source_root(run: &TypeRun) -> TextNode {
    let svg = run.svg_text.as_ref().unwrap();
    if run.spans.is_empty() && !overrides_font(run) {
        return svg.root.clone();
    }
    let mut original=run.clone();original.manual_kern.clear();
    for span in &mut original.spans {span.hscale=None;span.vscale=None;}
    let fragment = plain_fragment(&original, 0..run.content.chars().count());
    let Ok(xml) = roxmltree::Document::parse(&fragment) else {
        return svg.root.clone();
    };
    let mut root = collect(xml.root_element(), &sheet(&xml), &mut String::new(), true)
        .unwrap_or_else(|| svg.root.clone());

    root
}

fn flow_fragment(run: &TypeRun, flow: &Flow) -> String {
    let mut out = String::new();
    let mut start = 0;
    let count = run.content.chars().count();
    let svg = run.svg_text.as_ref().unwrap();
    let TextNode::Element { attributes, .. } = &svg.root else {
        return out;
    };
    let px = attributes
        .get("font-size")
        .and_then(|v| length(v, svg.base_px))
        .unwrap_or(svg.base_px);
    let mut baseline = px;
    out.push_str("<text");
    paint_defaults(attributes,&mut out);
    let mut css = String::new();
    for (key, value) in attributes {
        if overrides_font(run)
            && matches!(key.as_str(), "font-family" | "font-weight" | "font-style")
            || !run.font_variations.is_empty() && key == "font-variation-settings"
        {
            continue;
        }
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
    if overrides_font(run) {
        out.push_str(&font_attributes(&run.font));
        if !run.font_variations.is_empty() { css.push_str(&format!("font-variation-settings:{};", super::variations::css(&run.font_variations))); }
    }
    if !css.is_empty() {
        out.push_str(&format!(" style=\"{}\"", escape(&css)));
    }
    out.push_str(&format!(
        " data-oma-flow=\"{}\" data-oma-flow-source=\"{}\">",
        escape(&serde_json::to_string(flow).unwrap()),
        escape(
            &serde_json::to_string(&FlowSource {
                root: flow_source_root(run),
                content: run.content.clone(),
                manual_kern: run.manual_kern.clone(),
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
            let source = wrap(
                    &fragment,
                    &svg.definitions,
                    [1., 0., 0., 1., 0., 0.],
                    svg.viewport,
                );
            let content=&run.content[super::char_to_byte(&run.content,start)..super::char_to_byte(&run.content,end)];
            let source=scaling::apply(run,source,start,content)?;
            let source=positioning::pairs(run,source,start,content)?;
            layout_source(run,source,content)
        };
        let width = |end| {
            measure(end).and_then(|layout| {
                let mut nodes = Vec::new();
                texts(layout.tree.root(), &mut nodes);
                nodes.first().map(|t| t.bounding_box().width())
            })
            .unwrap_or(0.)
        };
        let mut lo = 0;
        let mut hi = ends.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if width(ends[mid]) <= flow.width {
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
                flow.y + baseline,
                if baseline > flow.height {
                    " visibility=\"hidden\""
                } else {
                    ""
                }
            ));
        }
        baseline += measure(end).map_or(flow.leading,|layout| {
            layout.characters.iter().filter_map(|c| leading(run,start+c.index).map(|v|v.pixels(c.px)))
                .reduce(f32::max).unwrap_or(flow.leading)
        });
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
        let outer = attributes(node, &sheet);
        let (mut svg, content, _) = capture(&xml, node, String::new(), 16.);
        let attrs = if let TextNode::Element { attributes, .. } = &svg.root {
            attributes.clone()
        } else {
            outer.clone()
        };
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
                .and_then(|v| {
                    length(
                        v,
                        match name {
                            "inline-size" | "x" => viewport(node)[0],
                            "y" => viewport(node)[1],
                            _ => default,
                        },
                    )
                })
                .unwrap_or(default)
        };
        let width = rect
            .and_then(|r| {
                r.attribute("width")
                    .and_then(|v| length(v, viewport(node)[0]))
            })
            .unwrap_or_else(|| number("inline-size", 0.));
        if width <= 0. {
            continue;
        }
        let px = number("font-size", 16.);
        let read = |name, default| {
            rect.and_then(|r| {
                r.attribute(name).and_then(|v| {
                    length(
                        v,
                        if matches!(name, "x" | "width") {
                            viewport(node)[0]
                        } else {
                            viewport(node)[1]
                        },
                    )
                })
            })
            .unwrap_or(default)
        };
        let flow = Flow {
            x: read("x", number("x", 0.)),
            y: read("y", number("y", 0.) - px),
            width,
            height: read("height", 100000.),
            leading: attrs
                .get("line-height")
                .and_then(|v| {
                    v.parse::<f32>()
                        .ok()
                        .map(|n| n * px)
                        .or_else(|| length(v, px))
                })
                .unwrap_or(px * 1.2),
        };
        svg.base_px = px;
        svg.flow = Some(flow.clone());
        let mut run = TypeRun {
            content,
            px,
            svg_text: Some(svg),
            ..Default::default()
        };
        let mut fragment = flow_fragment(&run, &flow);

        let mut wrapper = String::new();
        for key in [
            "transform",
            "opacity",
            "filter",
            "clip-path",
            "mask",
            "mix-blend-mode",
        ] {
            if let Some(value) = crate::formats::svg::cascaded_property(node, &sheet, key)
                .or_else(|| outer.get(key).cloned())
            {
                if safe_value(&value) {
                    if key == "mix-blend-mode" {
                        wrapper.push_str(&format!(" style=\"mix-blend-mode:{}\"", escape(&value)));
                    } else {
                        wrapper.push_str(&format!(" {key}=\"{}\"", escape(&value)));
                    }
                }
            }
        }
        if !wrapper.is_empty() {
            fragment = format!("<g{wrapper}>{fragment}</g>");
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
