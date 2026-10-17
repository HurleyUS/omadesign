//! Portable SVG text embeds the actual OpenType program instead of replacing Unicode with paths.
use super::*;
use crate::{
    document::{Document, Fill},
    geom::Geom,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::collections::{BTreeMap, HashMap};
#[cfg(test)]
mod tests;

pub(crate) struct Embedded {
    pub css: String,
    pub fragments: HashMap<u64, String>,
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
                    output.native(run)?
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
    fn native(&mut self, run: &TypeRun) -> Result<String, String> {
        if run.frame.is_some() || run.on_path.is_some() || !run.manual_kern.is_empty() {
            return Err("SVG font embedding for framed, path or manually kerned native text is not available. Choose outlined text instead.".into());
        }
        let mut out = String::from("<text xml:space=\"preserve\"");
        if let Some(m) = run.transform {
            out.push_str(&format!(
                " transform=\"matrix({} {} {} {} {} {})\"",
                m[0], m[1], m[2], m[3], m[4], m[5]
            ));
        }
        out.push('>');
        for line in super::compose(run).iter() {
            if line.hyphenated {
                return Err("SVG font embedding for automatic hyphenation is not available. Choose outlined text instead.".into());
            }
            for range in run.character_style_runs(line.start, line.end) {
                let metrics = super::character_metrics(run, range.start);
                if metrics.hscale != Some(100.)
                    || metrics.vscale != Some(100.)
                    || metrics.kerning == Some(crate::geom::KernMode::Optical)
                {
                    return Err("SVG font embedding for scaled or optically kerned characters is not available. Choose outlined text instead.".into());
                }
                let font = range.font.as_ref().unwrap_or(&run.font);
                let bytes = super::font_bytes_for(&TypeRun {
                    font: font.clone(),
                    ..Default::default()
                })
                .ok_or("The font needed for SVG export is unavailable")?;
                let face = rustybuzz::ttf_parser::Face::parse(&bytes, 0)
                    .map_err(|_| "Invalid SVG export font")?;
                let authored = super::variations::at(run, range.start);
                let axes = super::variations::FontVariations::new(
                    face.variation_axes()
                        .into_iter()
                        .map(|a| {
                            let v = authored
                                .settings()
                                .iter()
                                .find(|(t, _)| *t == a.tag.to_bytes())
                                .map_or(a.def_value, |(_, v)| *v);
                            (a.tag.to_bytes(), v.clamp(a.min_value, a.max_value))
                        })
                        .collect(),
                )?;
                let weight = face.weight().to_number();
                let italic = face.is_italic();
                let family = self.add(bytes)?;
                let px = metrics.px.unwrap_or(run.px);
                let mut features = run.features.clone();
                for (tag, value) in &range.features {
                    if let Some(old) = features.iter_mut().find(|(t, _)| t == tag) {
                        old.1 = *value;
                    } else {
                        features.push((*tag, *value));
                    }
                }
                let css = format!(
                    "font-optical-sizing:none;font-variation-settings:{};font-feature-settings:{};font-kerning:{};",
                    if axes.is_empty() {
                        "normal".into()
                    } else {
                        super::variations::css(&axes)
                    },
                    if features.is_empty() {
                        "normal".into()
                    } else {
                        super::feature_css(&features)
                    },
                    if metrics.kerning == Some(crate::geom::KernMode::None) {
                        "none"
                    } else {
                        "normal"
                    }
                );
                out.push_str(&format!("<tspan x=\"{}\" y=\"{}\" font-family=\"{family}\" font-size=\"{px}\" font-weight=\"{weight}\" font-style=\"{}\" letter-spacing=\"{}\" style=\"{}\"",run.origin.x+line.caret_x(range.start),run.origin.y+line.baseline-metrics.baseline_shift.unwrap_or(0.),if italic{"italic"}else{"normal"},metrics.tracking.unwrap_or(0.)*px/1000.,super::svg_type::escape(&css)));
                if let Some(fill) = &range.fill {
                    match fill {
                    Fill::Solid(c)=>out.push_str(&format!(" fill=\"{}\" fill-opacity=\"1\"",c.css())),
                    Fill::None=>out.push_str(" fill=\"none\""),
                    _=>return Err("SVG font embedding for painted character ranges is not available. Choose outlined text instead.".into()),
                }
                }
                let text: String = run
                    .content
                    .chars()
                    .skip(range.start)
                    .take(range.end - range.start)
                    .collect();
                out.push_str(&format!(">{}</tspan>", super::svg_type::escape(&text)));
            }
        }
        out.push_str("</text>");
        Ok(out)
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
