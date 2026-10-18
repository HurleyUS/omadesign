use super::*;

/// Format one parsed CSS family.
/// Args: family token. Returns an escaped named family or its generic keyword.
pub(crate) fn family_css(family: &usvg::FontFamily) -> String {
    match family {
        usvg::FontFamily::Named(name) => quoted_family(name),
        other => other.to_string(),
    }
}

/// Resolve ordered family candidates through the existing shared font environment.
/// Args: inherited selection attributes and database. Returns ordered real face IDs and unavailable names.
pub(in crate::text) fn family_faces(
    attributes: &BTreeMap<String, String>,
    db: &usvg::fontdb::Database,
) -> Vec<(usvg::FontFamily, Option<usvg::fontdb::ID>)> {
    let weight = attributes
        .get("font-weight")
        .map_or(400, |s| match s.as_str() {
            "bold" => 700,
            _ => s.parse().unwrap_or(400),
        });
    let style = match attributes.get("font-style").map(String::as_str) {
        Some("italic") => usvg::fontdb::Style::Italic,
        Some("oblique") => usvg::fontdb::Style::Oblique,
        _ => usvg::fontdb::Style::Normal,
    };
    let stretch = match attributes.get("font-stretch").map(String::as_str) {
        Some("ultra-condensed") => usvg::fontdb::Stretch::UltraCondensed,
        Some("extra-condensed") => usvg::fontdb::Stretch::ExtraCondensed,
        Some("narrower" | "condensed") => usvg::fontdb::Stretch::Condensed,
        Some("semi-condensed") => usvg::fontdb::Stretch::SemiCondensed,
        Some("semi-expanded") => usvg::fontdb::Stretch::SemiExpanded,
        Some("wider" | "expanded") => usvg::fontdb::Stretch::Expanded,
        Some("extra-expanded") => usvg::fontdb::Stretch::ExtraExpanded,
        Some("ultra-expanded") => usvg::fontdb::Stretch::UltraExpanded,
        _ => usvg::fontdb::Stretch::Normal,
    };
    let mut faces: Vec<_> = usvg::parse_font_families(
        attributes
            .get("font-family")
            .map_or("serif", String::as_str),
    )
    .unwrap_or_default()
    .into_iter()
    .map(|family| {
        let query = match &family {
            usvg::FontFamily::Named(s) => usvg::fontdb::Family::Name(s),
            usvg::FontFamily::Serif => usvg::fontdb::Family::Serif,
            usvg::FontFamily::SansSerif => usvg::fontdb::Family::SansSerif,
            usvg::FontFamily::Monospace => usvg::fontdb::Family::Monospace,
            usvg::FontFamily::Cursive => usvg::fontdb::Family::Cursive,
            usvg::FontFamily::Fantasy => usvg::fontdb::Family::Fantasy,
        };
        let id = db.query(&usvg::fontdb::Query {
            families: &[query],
            weight: usvg::fontdb::Weight(weight),
            style,
            stretch,
        });
        (family, id)
    })
    .collect();
    if faces.iter().all(|(_, id)| id.is_none()) {
        let families: Vec<_> = faces.iter().map(|(f, _)| f.clone()).collect();
        let svg_style = match style {
            usvg::fontdb::Style::Italic => usvg::FontStyle::Italic,
            usvg::fontdb::Style::Oblique => usvg::FontStyle::Oblique,
            _ => usvg::FontStyle::Normal,
        };
        if let Some(path) = super::substituted_path(&families, weight, svg_style) {
            if let Some(face) = db.faces().find(|face| match &face.source {
                usvg::fontdb::Source::File(p) | usvg::fontdb::Source::SharedFile(p, _) => {
                    *p == path
                }
                _ => false,
            }) {
                if let Some((_, id)) = faces.first_mut() {
                    *id = Some(face.id);
                }
            }
        }
    }
    faces
}

/// Rebind complete family lists while preserving inherited font selection properties.
/// Args: source tree and resolver. Returns an error without fabricating an unavailable face.
pub(in crate::text) fn map_families(
    root: &mut TextNode,
    mut map: impl FnMut(&BTreeMap<String, String>, std::ops::Range<usize>) -> Result<String, String>,
) -> Result<(), String> {
    fn visit(
        node: &mut TextNode,
        inherited: &BTreeMap<String, String>,
        cursor: &mut usize,
        map: &mut impl FnMut(
            &BTreeMap<String, String>,
            std::ops::Range<usize>,
        ) -> Result<String, String>,
    ) -> Result<(), String> {
        if let TextNode::Characters { count } = node {
            *cursor += *count;
            return Ok(());
        }
        if let TextNode::Element {
            attributes,
            children,
            ..
        } = node
        {
            let start = *cursor;
            let mut style = inherited.clone();
            for key in ["font-family", "font-weight", "font-style", "font-stretch"] {
                if let Some(value) = attributes
                    .get(key)
                    .filter(|v| !matches!(v.as_str(), "inherit" | "unset"))
                {
                    style.insert(key.into(), value.clone());
                }
            }
            for child in children {
                visit(child, &style, cursor, map)?;
            }
            if style.contains_key("font-family") {
                attributes.insert("font-family".into(), map(&style, start..*cursor)?);
            }
        }
        Ok(())
    }
    visit(root, &BTreeMap::new(), &mut 0, &mut map)
}

pub(in crate::text) fn map_flow_families(
    value: &str,
    mut map: impl FnMut(&BTreeMap<String, String>) -> Result<String, String>,
) -> Result<String, String> {
    let mut flow: FlowSource = serde_json::from_str(value).map_err(|e| e.to_string())?;
    map_families(&mut flow.root, |style, _| map(style))?;
    serde_json::to_string(&flow).map_err(|e| e.to_string())
}

/// Collect project font identities from every retained source family, including unused fallbacks.
/// Args: typed SVG text source. Returns unique content-addressed font references.
pub(crate) fn references(svg: &SvgText) -> std::collections::BTreeSet<String> {
    fn visit(node: &TextNode, out: &mut std::collections::BTreeSet<String>) {
        if let TextNode::Element {
            attributes,
            children,
            ..
        } = node
        {
            if let Some(value) = attributes.get("font-family") {
                out.extend(project_families(value));
            }
            for child in children {
                visit(child, out);
            }
        }
    }
    let mut out = Default::default();
    visit(&svg.root, &mut out);
    out
}

/// Read portable identities from a complete CSS family list.
/// Args: family declaration. Returns only content-addressed project font names, without loading them.
pub(crate) fn project_families(value: &str) -> Vec<String> {
    usvg::parse_font_families(value)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|f| {
            if let usvg::FontFamily::Named(name) = f {
                name.starts_with("omatype:").then_some(name)
            } else {
                None
            }
        })
        .collect()
}
