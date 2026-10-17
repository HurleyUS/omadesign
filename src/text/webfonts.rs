//! Embedded SVG fonts use the existing font database and content-addressed project loader.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};

const MAX_FONT: usize = 16 * 1024 * 1024;
const MAX_TOTAL: usize = 64 * 1024 * 1024;

/// Load inline SVG font faces without fetching external resources.
/// Args: parsed SVG and shared parser options. Returns an error for invalid or oversized embedded faces.
pub(crate) fn load(
    xml: &roxmltree::Document<'_>,
    options: &mut usvg::Options<'_>,
) -> Result<(), String> {
    let mut total = 0usize;
    let mut count = 0;
    for style in xml
        .descendants()
        .filter(|n| n.has_tag_name("style"))
        .filter_map(|n| n.text())
    {
        if !style.to_ascii_lowercase().contains("@font-face") {
            continue;
        }
        for block in faces(style)? {
            let declarations: HashMap<_, _> = simplecss::DeclarationTokenizer::from(block)
                .map(|d| (d.name.to_ascii_lowercase(), d.value))
                .collect();
            let Some(src) = declarations.get("src") else {
                continue;
            };
            let Some(data) = inline_url(src)? else {
                continue;
            };
            let Some(family) = declarations.get("font-family").and_then(|v| family(v)) else {
                return Err("Embedded SVG font has no valid family name".into());
            };
            let (mime, encoded) = data
                .split_once(',')
                .ok_or("Invalid embedded SVG font URL")?;
            if !mime.ends_with(";base64") {
                return Err("Embedded SVG fonts require base64 font data".into());
            }
            if encoded.len() > MAX_FONT.div_ceil(3) * 4 {
                return Err("Embedded SVG font exceeds 16 MiB".into());
            }
            let bytes = STANDARD
                .decode(encoded)
                .map_err(|_| "Invalid embedded SVG font base64")?;
            if bytes.len() > MAX_FONT {
                return Err("Embedded SVG font exceeds 16 MiB".into());
            }
            total = total
                .checked_add(bytes.len())
                .ok_or("Embedded SVG fonts overflow")?;
            count += 1;
            if total > MAX_TOTAL || count > 64 {
                return Err("Embedded SVG fonts exceed 64 faces or 64 MiB".into());
            }
            if bytes.starts_with(b"ttcf") {
                return Err("Embedded SVG font collections need a standalone face".into());
            }
            let face = rustybuzz::ttf_parser::Face::parse(&bytes, 0)
                .map_err(|_| "Unsupported embedded SVG font; use an OpenType or TrueType font")?;
            if face.number_of_glyphs() == 0 || ab_glyph::FontRef::try_from_slice(&bytes).is_err() {
                return Err("Invalid embedded SVG font outlines".into());
            }
            let db = options.fontdb_mut();
            let ids = db.load_font_source(usvg::fontdb::Source::Binary(Arc::new(bytes)));
            if ids.is_empty() {
                return Err("Invalid embedded SVG font".into());
            }
            for id in ids {
                let mut info = db
                    .face(id)
                    .cloned()
                    .ok_or("Embedded SVG font disappeared")?;
                info.families =
                    vec![(family.clone(), usvg::fontdb::Language::English_UnitedStates)];
                if let Some(weight) = declarations
                    .get("font-weight")
                    .and_then(|v| v.trim().parse::<u16>().ok())
                {
                    if (1..=1000).contains(&weight) {
                        info.weight = usvg::fontdb::Weight(weight);
                    }
                }
                if let Some(style) = declarations.get("font-style") {
                    info.style = match style.trim() {
                        "italic" => usvg::fontdb::Style::Italic,
                        "oblique" => usvg::fontdb::Style::Oblique,
                        _ => usvg::fontdb::Style::Normal,
                    };
                }
                db.remove_face(id);
                db.push_face_info(info);
            }
        }
    }
    Ok(())
}

/// Resolve an actual parser face into the existing portable font namespace.
/// Args: font database and selected face. Returns its path or validated content-addressed bytes.
pub(crate) fn reference(db: &usvg::fontdb::Database, id: usvg::fontdb::ID) -> Option<String> {
    let face = db.face(id)?;
    match &face.source {
        usvg::fontdb::Source::File(p) | usvg::fontdb::Source::SharedFile(p, _) => {
            Some(p.to_string_lossy().into_owned())
        }
        usvg::fontdb::Source::Binary(_) => {
            let bytes = db
                .with_face_data(id, super::font_catalog::face_bytes)?
                .ok()?;
            if bytes.len() > MAX_FONT {
                return None;
            }
            for (alias, _) in &face.families {
                if super::BYTES
                    .get()
                    .and_then(|cache| cache.lock().ok()?.get(Path::new(alias)).cloned())
                    .is_some_and(|known| *known == bytes)
                {
                    return Some(alias.clone());
                }
            }
            let name = super::font_name(&bytes).unwrap_or_else(|| "Embedded SVG font".into());
            let reference = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
            super::register_memory_font(&reference, &name, Arc::new(bytes)).ok()?;
            Some(reference)
        }
    }
}

fn family(value: &str) -> Option<String> {
    let value = value.trim();
    let value = if value.starts_with(['\'', '"']) {
        let quote = value.chars().next()?;
        if !value.ends_with(quote) || value.len() < 2 {
            return None;
        }
        &value[1..value.len() - 1]
    } else {
        value
    };
    if value.is_empty() || value.contains(['\\', '\n', '\r', '{', '}', ';']) {
        return None;
    }
    Some(value.into())
}

fn inline_url(source: &str) -> Result<Option<&str>, String> {
    let mut rest = source;
    while let Some(at) = rest.find("url(") {
        rest = &rest[at + 4..];
        let Some(end) = rest.find(')') else {
            return Err("Invalid SVG font URL".into());
        };
        let url = rest[..end].trim().trim_matches(['\'', '"']);
        if url.starts_with("data:") {
            return Ok(Some(url));
        }
        rest = &rest[end + 1..];
    }
    Ok(None)
}

fn faces(source: &str) -> Result<Vec<&str>, String> {
    let bytes = source.as_bytes();
    let mut at = 0;
    let mut blocks = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut font = false;
    while at < bytes.len() {
        if bytes[at..].starts_with(b"/*") {
            let leading = depth == 0 && source[start..at].trim().is_empty();
            at += 2;
            let end = source[at..]
                .find("*/")
                .ok_or("Unterminated SVG stylesheet comment")?;
            at += end + 2;
            if leading {
                start = at;
            }
            continue;
        }
        if matches!(bytes[at], b'\'' | b'"') {
            let quote = bytes[at];
            at += 1;
            while at < bytes.len() && bytes[at] != quote {
                if bytes[at] == b'\\' {
                    at += 1;
                }
                at += 1;
            }
            if at == bytes.len() {
                return Err("Unterminated SVG stylesheet string".into());
            }
        } else if bytes[at] == b'{' {
            if depth == 0 {
                font = source[start..at].trim().eq_ignore_ascii_case("@font-face");
                start = at + 1;
            }
            depth += 1;
        } else if bytes[at] == b'}' {
            depth = depth.checked_sub(1).ok_or("Unbalanced SVG stylesheet")?;
            if depth == 0 {
                if font {
                    blocks.push(&source[start..at]);
                }
                start = at + 1;
            }
        } else if bytes[at] == b';' && depth == 0 {
            start = at + 1;
        }
        at += 1;
    }
    if depth != 0 {
        return Err("Unbalanced SVG stylesheet".into());
    }
    Ok(blocks)
}

/// Bind embedded CSS family aliases to the actual portable faces selected for this source.
/// Args: captured text metadata and its original font database. Returns no value; ordinary missing family requests stay unchanged.
pub(crate) fn bind(svg: &mut super::svg_type::SvgText, db: &usvg::fontdb::Database) {
    fn visit(
        node: &mut super::svg_type::TextNode,
        db: &usvg::fontdb::Database,
        inherited: &std::collections::BTreeMap<String, String>,
    ) {
        let super::svg_type::TextNode::Element {
            attributes,
            children,
            ..
        } = node
        else {
            return;
        };
        let mut style = inherited.clone();
        for key in ["font-family", "font-weight", "font-style"] {
            if let Some(value) = attributes
                .get(key)
                .filter(|v| !matches!(v.as_str(), "inherit" | "unset"))
            {
                style.insert(key.into(), value.clone());
            }
        }
        if let Some(family) = style.get("font-family") {
            let family = super::font_inventory::first_family(family);
            let weight = style.get("font-weight").map_or(400, |s| {
                if s == "bold" {
                    700
                } else {
                    s.parse().unwrap_or(400)
                }
            });
            let slant = match style.get("font-style").map(String::as_str) {
                Some("italic") => usvg::fontdb::Style::Italic,
                Some("oblique") => usvg::fontdb::Style::Oblique,
                _ => usvg::fontdb::Style::Normal,
            };
            if let Some(id) = db.query(&usvg::fontdb::Query {
                families: &[usvg::fontdb::Family::Name(&family)],
                weight: usvg::fontdb::Weight(weight),
                style: slant,
                ..Default::default()
            }) && db
                .face(id)
                .is_some_and(|f| matches!(f.source, usvg::fontdb::Source::Binary(_)))
                && let Some(reference) = reference(db, id)
            {
                attributes.insert("font-family".into(), format!("'{reference}'"));
            }
        }
        for child in children {
            visit(child, db, &style);
        }
    }
    visit(&mut svg.root, db, &Default::default());
}
