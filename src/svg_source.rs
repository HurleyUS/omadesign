//! Retained vector definitions shared by SVG paint and filter graphs.
use crate::geom::Bounds;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tiny_skia::Transform;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SvgPattern {
    pub definitions: Arc<str>,
    pub id: String,
    pub transform: [f32; 6],
    pub opacity: f32,
}

/// Rebind definition IDs for an independent exported object.
/// Args: normalized SVG definitions and a unique prefix. Returns collision-free XML.
pub fn namespace(source: &str, prefix: &str) -> String {
    let mut output = source.to_string();
    let wrapper = format!("<svg>{source}</svg>");
    let Ok(xml) = roxmltree::Document::parse(&wrapper) else {
        return output;
    };
    let ids: Vec<_> = xml
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .map(str::to_string)
        .collect();
    for id in ids {
        output = output
            .replace(&format!("id=\"{id}\""), &format!("id=\"{prefix}{id}\""))
            .replace(&format!("url(#{id})"), &format!("url(#{prefix}{id})"))
            .replace(
                &format!("href=\"#{id}\""),
                &format!("href=\"#{prefix}{id}\""),
            );
    }
    output
}

/// Encode a stored affine matrix.
/// Args: six SVG coefficients. Returns the corresponding drawing transform.
pub fn transform(matrix: [f32; 6]) -> Transform {
    let [a, b, c, d, e, f] = matrix;
    Transform::from_row(a, b, c, d, e, f)
}

/// Format an affine map for portable SVG attributes.
/// Args: drawing transform. Returns a matrix expression.
pub fn matrix(t: Transform) -> String {
    format!(
        "matrix({} {} {} {} {} {})",
        t.sx, t.ky, t.kx, t.sy, t.tx, t.ty
    )
}

impl SvgPattern {
    /// Bind a reusable vector pattern to the current object bounds.
    /// Args: unique definition prefix and object bounds. Returns definitions and the paint URL.
    pub fn definition(&self, prefix: &str, bounds: Bounds) -> (String, String) {
        let mut definitions = String::new();
        let reference = self.append_definition(&mut definitions, prefix, bounds);
        (definitions, reference)
    }

    /// Append a pattern instance while sharing its vector resources.
    /// Args: output definitions, instance prefix and current object bounds. Returns the paint URL.
    pub fn append_definition(
        &self,
        definitions: &mut String,
        prefix: &str,
        bounds: Bounds,
    ) -> String {
        let placement = Transform::from_translate(bounds.min.x, bounds.min.y)
            .pre_scale(bounds.width().max(0.001), bounds.height().max(0.001))
            .pre_concat(transform(self.transform));
        let id = format!("{prefix}paint");
        let resource_prefix = append_source(definitions, &self.definitions);
        definitions.push_str(&format!(
            "<pattern id=\"{id}\" href=\"#{resource_prefix}{}\" patternTransform=\"{}\"/>",
            self.id,
            matrix(placement)
        ));
        format!("url(#{id})")
    }

    /// Render vector paint on a path at the requested viewport resolution.
    /// Args: destination, path, object bounds, view placement, opacity, blend and mask.
    /// Returns without changing the destination when source definitions are invalid.
    pub fn draw(
        &self,
        pm: &mut tiny_skia::Pixmap,
        path: &tiny_skia::Path,
        bounds: Bounds,
        placement: Transform,
        opacity: f32,
        blend: tiny_skia::BlendMode,
        mask: Option<&tiny_skia::Mask>,
        winding: bool,
    ) {
        let (defs, paint) = self.definition("oma-pattern-", bounds);
        let source = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\"><defs>{defs}</defs><path d=\"{}\" fill=\"{paint}\" fill-opacity=\"{}\" fill-rule=\"{}\"/></svg>",
            pm.width(),
            pm.height(),
            path_data(path),
            self.opacity.clamp(0., 1.),
            if winding { "nonzero" } else { "evenodd" }
        );
        let Ok(tree) = usvg::Tree::from_str(&source, &options()) else {
            return;
        };
        let Some(mut surface) = tiny_skia::Pixmap::new(pm.width(), pm.height()) else {
            return;
        };
        resvg::render(&tree, placement, &mut surface.as_mut());
        pm.draw_pixmap(
            0,
            0,
            surface.as_ref(),
            &tiny_skia::PixmapPaint {
                opacity,
                blend_mode: blend,
                ..Default::default()
            },
            Transform::identity(),
            mask,
        );
    }
}

/// Write exact curve segments without flattening them.
/// Args: native path. Returns portable SVG path data.
pub fn path_data(path: &tiny_skia::Path) -> String {
    use tiny_skia::PathSegment::*;
    let mut out = String::new();
    for segment in path.segments() {
        match segment {
            MoveTo(p) => out.push_str(&format!("M{} {}", p.x, p.y)),
            LineTo(p) => out.push_str(&format!("L{} {}", p.x, p.y)),
            QuadTo(a, b) => out.push_str(&format!("Q{} {} {} {}", a.x, a.y, b.x, b.y)),
            CubicTo(a, b, c) => {
                out.push_str(&format!("C{} {} {} {} {} {}", a.x, a.y, b.x, b.y, c.x, c.y))
            }
            Close => out.push('Z'),
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SvgFilterGraph {
    pub definitions: Arc<str>,
    pub original_definitions: Arc<str>,
    pub filters: Vec<String>,
    pub transform: [f32; 6],
    #[serde(default)]
    pub reductions: Vec<String>,
}

impl SvgFilterGraph {
    /// Apply the retained filter graph in its original coordinate system.
    /// Args: export definitions, unique prefix and current editable vector content.
    /// Returns a vector group; invalid singular placement leaves the content intact.
    pub fn wrap(&self, definitions: &mut String, prefix: &str, content: String) -> String {
        let placement = transform(self.transform);
        let Some(inverse) = placement.invert() else {
            return content;
        };
        let prefix = append_source(definitions, &self.definitions);
        let filters = self
            .filters
            .iter()
            .map(|id| format!("url(#{prefix}{id})"))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "<g transform=\"{}\" filter=\"{filters}\"><g transform=\"{}\">{content}</g></g>",
            matrix(placement),
            matrix(inverse)
        )
    }
}

/// Identify SVG operations the current renderer cannot execute.
/// Args: filter XML. Returns one explicit reduction per unsupported primitive or external input.
pub fn reductions(source: &str) -> Vec<String> {
    let Ok(xml) = roxmltree::Document::parse_with_options(
        source,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 400_000,
            entity_resolver: None,
        },
    ) else {
        return vec!["Invalid SVG effect source".into()];
    };
    let mut notes = Vec::new();
    for node in xml.descendants().filter(|n| n.is_element()) {
        let tag = node.tag_name().name();
        if tag.starts_with("fe")
            && !matches!(
                tag,
                "feBlend"
                    | "feColorMatrix"
                    | "feComponentTransfer"
                    | "feComposite"
                    | "feConvolveMatrix"
                    | "feDiffuseLighting"
                    | "feDisplacementMap"
                    | "feDistantLight"
                    | "feDropShadow"
                    | "feFlood"
                    | "feFuncR"
                    | "feFuncG"
                    | "feFuncB"
                    | "feFuncA"
                    | "feGaussianBlur"
                    | "feImage"
                    | "feMerge"
                    | "feMergeNode"
                    | "feMorphology"
                    | "feOffset"
                    | "fePointLight"
                    | "feSpecularLighting"
                    | "feSpotLight"
                    | "feTile"
                    | "feTurbulence"
            )
        {
            notes.push(format!(
                "SVG filter operation {tag} is not rendered; original source retained."
            ));
        }
        if tag == "feImage"
            && node.attributes().any(|a| {
                a.name() == "href" && !a.value().starts_with('#') && !a.value().starts_with("data:")
            })
        {
            notes.push("SVG filter linked feImage is not loaded; embed the image first.".into());
        }
        for attribute in ["in", "in2"] {
            if let Some(
                input @ ("BackgroundImage" | "BackgroundAlpha" | "FillPaint" | "StrokePaint"),
            ) = node.attribute(attribute)
            {
                notes.push(format!(
                    "SVG filter input {input} is reduced by the renderer; original source retained."
                ));
            }
        }
    }
    notes
}

/// Parse only self-contained vector source.
/// Returns SVG options that resolve embedded data while refusing file or network references.
pub fn options() -> usvg::Options<'static> {
    let mut options = usvg::Options::default();
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    options
}

/// Protect advanced paint from older readers that ignore new fields.
/// Args: document to save. Returns whether the SVG paint project format is required.
pub fn requires_advanced_paint(doc: &crate::document::Document) -> bool {
    let gradient = |g: &crate::gradient::Gradient| {
        g.radial_axis.is_some()
            || g.focal != [0., 0.]
            || g.inner_radius != 0.
            || g.spread != crate::gradient::Spread::Pad
    };
    doc.layers.iter().any(|layer| {
        layer.svg_filter.is_some()
            || layer.kind.shapes().is_some_and(|shapes| {
                shapes.iter().any(|shape| {
                    matches!(&shape.style.fill, crate::document::Fill::Pattern(_))
                        || shape.style.fill.gradient().as_ref().is_some_and(gradient)
                        || shape
                            .style
                            .stroke
                            .as_ref()
                            .and_then(|s| s.gradient.as_ref())
                            .is_some_and(gradient)
                })
            })
    })
}

/// Keep only resources reachable from selected definitions.
/// Args: self-contained definition XML and root IDs. Returns the transitive vector resource closure.
pub fn subset(source: &str, ids: &[String]) -> String {
    use std::collections::{HashMap, HashSet};
    let wrapper = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">{source}</svg>"
    );
    let Ok(xml) = roxmltree::Document::parse(&wrapper) else {
        return source.to_string();
    };
    let roots: Vec<_> = xml
        .root_element()
        .children()
        .filter(|n| n.is_element())
        .collect();
    let mut owners = HashMap::new();
    for (index, root) in roots.iter().enumerate() {
        for node in root.descendants() {
            if let Some(id) = node.attribute("id") {
                owners.insert(id, index);
            }
        }
    }
    let mut selected = HashSet::new();
    let mut pending = ids.to_vec();
    while let Some(id) = pending.pop() {
        let Some(&index) = owners.get(id.as_str()) else {
            continue;
        };
        if !selected.insert(index) {
            continue;
        }
        for node in roots[index].descendants() {
            for attr in node.attributes() {
                let value = attr.value();
                if attr.name() == "href" {
                    if let Some(id) = value.strip_prefix('#') {
                        pending.push(id.to_string());
                    }
                }
                for part in value.split("url(").skip(1) {
                    if let Some(value) = part.split(')').next() {
                        if let Some(id) = value.trim().trim_matches(['\'', '"']).strip_prefix('#') {
                            pending.push(id.to_string());
                        }
                    }
                }
            }
        }
    }
    roots
        .iter()
        .enumerate()
        .filter(|(i, _)| selected.contains(i))
        .map(|(_, node)| &wrapper[node.range()])
        .collect()
}

/// Serialize source definitions into inert, consistently quoted SVG XML.
/// Args: definition XML. Returns canonical XML, excluding executable content.
pub fn canonicalize(source: &str) -> Option<String> {
    let wrapper = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">{source}</svg>"
    );
    let xml = roxmltree::Document::parse(&wrapper).ok()?;
    Some(xml.root_element().children().map(xml_node).collect())
}

/// Encode one resource while excluding executable content.
/// Args: parsed SVG node. Returns equivalent standalone XML with escaped attributes.
pub(crate) fn xml_node(node: roxmltree::Node<'_, '_>) -> String {
    fn escaped(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('"', "&quot;")
    }
    if node.is_text() {
        return escaped(node.text().unwrap_or_default());
    }
    if !node.is_element()
        || matches!(
            node.tag_name().name(),
            "script" | "foreignObject" | "animate" | "animateTransform" | "set"
        )
    {
        return String::new();
    }
    let name = node.tag_name().name();
    let mut out = format!("<{name}");
    for attr in node.attributes().filter(|a| !a.name().starts_with("on")) {
        out.push_str(&format!(" {}=\"{}\"", attr.name(), escaped(attr.value())));
    }
    out.push('>');
    for child in node.children() {
        out.push_str(&xml_node(child));
    }
    out.push_str(&format!("</{name}>"));
    out
}

/// Share vector resources in the native file instead of repeating them per object.
/// Args: document copy being saved. Returns the packed source table.
pub fn pack_sources(
    doc: &mut crate::document::Document,
) -> std::collections::BTreeMap<String, Arc<str>> {
    use std::collections::{BTreeMap, HashMap};
    let mut sources = BTreeMap::new();
    let mut seen = HashMap::new();
    visit_sources(doc, &mut |source| {
        let pointer = Arc::as_ptr(source) as *const u8 as usize;
        let key = seen.entry(pointer).or_insert_with(|| {
            let key = format!("@oma-svg-source-{}", sources.len());
            sources.insert(key.clone(), source.clone());
            key
        });
        *source = Arc::from(key.as_str());
    });
    sources
}

/// Restore shared definitions after reading a project.
/// Args: decoded document and its packed source table. Returns an error for missing resources.
pub fn unpack_sources(
    doc: &mut crate::document::Document,
    sources: &std::collections::BTreeMap<String, Arc<str>>,
) -> Result<(), String> {
    let mut error = None;
    visit_sources(doc, &mut |source| {
        if let Some(shared) = sources.get(source.as_ref()) {
            *source = shared.clone();
        } else if source.starts_with("@oma-svg-source-") {
            error = Some(format!("Missing vector source {source}"));
        }
    });
    error.map_or(Ok(()), Err)
}

/// Visit each source reference without copying its XML.
/// Args: mutable document and an operation on each shared definition reference.
fn visit_sources(doc: &mut crate::document::Document, visit: &mut impl FnMut(&mut Arc<str>)) {
    for layer in &mut doc.layers {
        if let Some(graph) = &mut layer.svg_filter {
            visit(&mut graph.definitions);
            visit(&mut graph.original_definitions);
        }
        for shape in layer.kind.shapes_mut().into_iter().flatten() {
            if let crate::document::Fill::Pattern(pattern) = &mut shape.style.fill {
                visit(&mut pattern.definitions);
            }
        }
    }
}

/// Reuse a source definition across exported objects.
/// Args: output definitions and shared source XML. Returns its content-based namespace.
pub fn append_source(definitions: &mut String, source: &Arc<str>) -> String {
    use sha2::{Digest, Sha256};
    let prefix = format!("oma-resource-{:x}-", Sha256::digest(source.as_bytes()));
    let marker = format!("<!--{prefix}-->");
    if !definitions.contains(&marker) {
        definitions.push_str(&marker);
        definitions.push_str(&namespace(source, &prefix));
    }
    prefix
}
