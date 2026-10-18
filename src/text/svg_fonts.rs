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
    fn families(&mut self, style:&BTreeMap<String,String>, db:&usvg::fontdb::Database)->Result<String,String> {
        let mut families=Vec::new();
        let mut installed=Vec::new();
        for (family,id) in super::svg_type::family_faces(style,db) {
            let value=if let Some(id)=id {
                let bytes=db.with_face_data(id,super::font_catalog::face_bytes).ok_or("SVG font is unavailable")??;
                if let Ok(face)=rustybuzz::ttf_parser::Face::parse(&bytes,0) {
                    if let Some(name)=[16,1].into_iter().find_map(|id|face.names().into_iter().filter(|n|n.name_id==id).find_map(|n|n.to_string().filter(|s|!s.trim().is_empty()))) {
                        let value=super::svg_type::family_css(&usvg::FontFamily::Named(name));
                        if !installed.contains(&value){installed.push(value);}
                    }
                }
                format!("'{}'",self.add(Arc::new(bytes))?)
            } else {super::svg_type::family_css(&family)};
            if !families.contains(&value) {families.push(value);}
        }
        families.extend(installed);
        if families.is_empty(){return Err("Cannot resolve an SVG text font for embedding".into());}
        Ok(families.join(", "))
    }

    fn rich(&mut self, source:&str, db:&usvg::fontdb::Database)->Result<String,String> {
        let xml=roxmltree::Document::parse(source).map_err(|e|e.to_string())?;
        let mut edits=Vec::new();
        for node in xml.descendants().filter(|n|n.is_element() && matches!(n.tag_name().name(),"text"|"tspan"|"textPath")) {
            if node.children().any(|n|n.is_text() && n.text().is_some_and(|v|!v.is_empty())) || node.attribute("font-family").is_some() {
                let mut style=BTreeMap::new();
                for key in ["font-family","font-weight","font-style","font-stretch"] {
                    if let Some(value)=node.ancestors().find_map(|n|n.attribute(key).filter(|v|!matches!(*v,"inherit"|"unset"))) {style.insert(key.into(),value.into());}
                }
                let value=self.families(&style,db)?;
                let value=super::svg_type::escape(&value);
                if let Some(attr)=node.attributes().find(|a|a.name()=="font-family") {edits.push((attr.range_value(),value));}
                else if let Some(end)=source[node.range()].find('>') {let at=node.range().start+end;edits.push((at..at,format!(" font-family=\"{value}\"")));}
            }
            if let Some(attr)=node.attributes().find(|a|a.name()=="data-oma-flow-source") {
                let value=super::svg_type::map_flow_families(attr.value(),|style|self.families(style,db))?;
                edits.push((attr.range_value(),super::svg_type::escape(&value)));
            }
        }
        edits.sort_by_key(|(r,_)|std::cmp::Reverse(r.start));
        let mut out=source.to_string();for(r,v) in edits {out.replace_range(r,&v);}Ok(out)
    }
}
