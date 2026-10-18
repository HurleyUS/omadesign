//! Portable SVG text embeds the actual OpenType program instead of replacing Unicode with paths.
use super::*;
use crate::{
    document::{Document, Fill},
    geom::Geom,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::collections::{BTreeMap, HashMap};
#[cfg(test)]
mod advanced_tests;
mod layout;
#[cfg(test)]
mod tests;

pub(crate) struct Embedded {
    pub css: String,
    pub fragments: HashMap<u64, String>,
    pub outlined: std::collections::HashSet<u64>,
    pub warnings: Vec<String>,
    fonts: BTreeMap<u128, Arc<Vec<u8>>>,
    total: usize,
}
impl Embedded {
    /// Prepare real editable SVG text without changing the native document or font bytes.
    /// Args: validated source artwork. Returns bounded embedded font definitions and positioned Unicode fragments.
    pub fn new(doc: &Document) -> Result<Self, String> {
        let mut output = Self {
            css: String::new(),
            fragments: HashMap::new(),
            outlined: Default::default(),
            warnings: vec![],
            fonts: BTreeMap::new(),
            total: 0,
        };
        for layer in &doc.layers {
            for shape in layer.kind.shapes().into_iter().flatten() {
                let Geom::Text(run) = &shape.geom else {
                    continue;
                };
                let fragment = if run.svg_text.is_some() {
                    let painted = if run
                        .svg_text
                        .as_ref()
                        .is_some_and(|s| s.base_fill != shape.style.fill)
                    {
                        super::svg_type::object_paint(run, &shape.style.fill)
                    } else {
                        run.clone()
                    };
                    let layout = super::svg_type::layout(&painted)
                        .ok_or("Cannot lay out this SVG text for font embedding")?;
                    let source = super::svg_type::export_fragment_embedded(&painted, shape.id)
                        .ok_or("Cannot export this SVG text")?;
                    output.rich(&source, layout.tree.fontdb())?
                } else {
                    let before = output
                        .fonts
                        .keys()
                        .copied()
                        .collect::<std::collections::HashSet<_>>();
                    let css = output.css.len();
                    let total = output.total;
                    match output.native_layout(shape) {
                        Ok(fragment) => fragment,
                        Err(layout::Issue::Invalid(error)) => return Err(error),
                        Err(layout::Issue::Outline(reason)) => {
                            output.css.truncate(css);
                            output.total = total;
                            output.fonts.retain(|key, _| before.contains(key));
                            output.outlined.insert(shape.id);
                            output.warnings.push(format!("{}: {reason}", shape.name));
                            continue;
                        }
                    }
                };
                output.fragments.insert(shape.id, fragment);
            }
        }
        Ok(output)
    }
    fn add(&mut self, bytes: Arc<Vec<u8>>) -> Result<String, String> {
        use rustybuzz::ttf_parser::{Face, Permissions};
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("SVG font exceeds the portable 16 MiB limit".into());
        }
        let face = Face::parse(&bytes, 0).map_err(|_| "Invalid SVG export font")?;
        if !matches!(
            face.permissions(),
            Some(Permissions::Installable | Permissions::Editable)
        ) || !face.is_outline_embedding_allowed()
        {
            return Err(
                "This font does not permit editable SVG embedding. Choose outlined text instead."
                    .into(),
            );
        }
        let key = crate::typography::fingerprint(&bytes);
        let family = format!("oma-font-{key:032x}");
        if let Some(old) = self.fonts.get(&key) {
            if **old != *bytes {
                return Err("SVG font fingerprint collision".into());
            }
            return Ok(family);
        }
        self.total = self
            .total
            .checked_add(bytes.len())
            .ok_or("SVG font size overflow")?;
        if self.total > 64 * 1024 * 1024 || self.fonts.len() >= 64 {
            return Err("SVG embedding exceeds 64 faces or 64 MiB".into());
        }
        let weight = if let Some(axis) = face
            .variation_axes()
            .into_iter()
            .find(|a| a.tag.to_bytes() == *b"wght")
        {
            format!("{} {}", axis.min_value, axis.max_value)
        } else {
            face.weight().to_number().to_string()
        };
        self.css.push_str(&format!("@font-face{{font-family:'{family}';font-style:{};font-weight:{weight};src:url(data:font/ttf;base64,{}) format('truetype');}}\n", if face.is_italic(){"italic"}else{"normal"}, STANDARD.encode(bytes.as_slice())));
        self.fonts.insert(key, bytes);
        Ok(family)
    }
    fn rich(&mut self, source: &str, db: &usvg::fontdb::Database) -> Result<String, String> {
        let xml = roxmltree::Document::parse(source).map_err(|e| e.to_string())?;
        let mut edits = Vec::new();
        for node in xml.descendants().filter(|n| {
            n.is_element() && matches!(n.tag_name().name(), "text" | "tspan" | "textPath")
        }) {
            if !node
                .children()
                .any(|n| n.is_text() && n.text().is_some_and(|v| !v.is_empty()))
                && node.attribute("font-family").is_none()
            {
                continue;
            }
            let value = node
                .ancestors()
                .find_map(|n| n.attribute("font-family"))
                .unwrap_or("sans-serif");
            let names: Vec<_> = value
                .split(',')
                .map(|s| s.trim().trim_matches(['\'', '"']))
                .collect();
            let families: Vec<_> = names
                .iter()
                .map(|s| match *s {
                    "serif" => usvg::fontdb::Family::Serif,
                    "sans-serif" => usvg::fontdb::Family::SansSerif,
                    "monospace" => usvg::fontdb::Family::Monospace,
                    _ => usvg::fontdb::Family::Name(s),
                })
                .collect();
            let weight = node
                .ancestors()
                .find_map(|n| n.attribute("font-weight"))
                .map_or(400, |v| match v {
                    "bold" => 700,
                    _ => v.parse().unwrap_or(400),
                });
            let style = match node.ancestors().find_map(|n| n.attribute("font-style")) {
                Some("italic") => usvg::fontdb::Style::Italic,
                Some("oblique") => usvg::fontdb::Style::Oblique,
                _ => usvg::fontdb::Style::Normal,
            };
            let id = db
                .query(&usvg::fontdb::Query {
                    families: &families,
                    weight: usvg::fontdb::Weight(weight),
                    style,
                    ..Default::default()
                })
                .ok_or("Cannot resolve an SVG text font for embedding")?;
            let bytes = db
                .with_face_data(id, super::font_catalog::face_bytes)
                .ok_or("SVG font is unavailable")??;
            let family = self.add(Arc::new(bytes))?;
            if let Some(attr) = node.attributes().find(|a| a.name() == "font-family") {
                edits.push((attr.range_value(), family));
            } else if let Some(end) = source[node.range()].find('>') {
                let at = node.range().start + end;
                edits.push((at..at, format!(" font-family=\"{family}\"")));
            }
        }
        edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
        let mut out = source.to_string();
        for (r, v) in edits {
            out.replace_range(r, &v);
        }
        Ok(out)
    }
}
