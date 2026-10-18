use super::*;
use crate::geom::Geom;
use base64::Engine;

fn primary() -> Vec<u8> {
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/assets/fonts/EBGaramond.ttf"
    ))
    .to_vec()
}
fn hebrew() -> Vec<u8> {
    std::fs::read("/usr/share/fonts/liberation/LiberationSans-Regular.ttf").unwrap()
}
fn arabic() -> Vec<u8> {
    std::fs::read("/usr/share/fonts/noto/NotoSansArabic-Regular.ttf").unwrap()
}
fn id(bytes: &[u8]) -> String {
    format!("omatype:{:032x}", crate::typography::fingerprint(bytes))
}
fn document_source(fonts: &[(&str, &[u8])], families: &str, body: &str) -> String {
    let css: String = fonts
        .iter()
        .map(|(name, bytes)| {
            format!(
                "@font-face{{font-family:'{name}';src:url(data:font/ttf;base64,{})}}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        })
        .collect();
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="420" height="180" text-rendering="geometricPrecision"><style>{css}</style><text x="25" y="80" font-size="40" font-family="{families}">{body}</text></svg>"#
    )
}
fn run(doc: &crate::document::Document) -> &TypeRun {
    doc.layers
        .iter()
        .filter_map(|l| l.kind.shapes())
        .flatten()
        .find_map(|s| {
            if let Geom::Text(r) = &s.geom {
                Some(r)
            } else {
                None
            }
        })
        .unwrap()
}
fn chosen(run: &TypeRun) -> Vec<(String, String, u16)> {
    let layout = layout(run).unwrap();
    let mut nodes = Vec::new();
    texts(layout.tree.root(), &mut nodes);
    nodes
        .iter()
        .flat_map(|t| t.layouted())
        .flat_map(|s| &s.positioned_glyphs)
        .map(|g| {
            (
                g.text.clone(),
                crate::text::webfonts::reference(layout.tree.fontdb(), g.font).unwrap(),
                g.id.0,
            )
        })
        .collect()
}
fn check(name: &str, source: &str) -> crate::document::Document {
    let (doc, notes) = crate::formats::svg::read(source, "Fallback").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let png = crate::compositor::export_png(&doc, 1).unwrap();
    let again = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(png, crate::compositor::export_png(&again, 1).unwrap());
    let exported = crate::svg::export_embedded(&doc).unwrap();
    let (again, notes) = crate::formats::svg::read(&exported, "Portable fallback").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(
        png,
        crate::compositor::export_png(&again, 1).unwrap(),
        "{name}"
    );
    assert!(
        run(&doc).spans.iter().all(|s| s.font.is_none()),
        "Computed fallback must not become an author override"
    );
    if let Some(root) = std::env::var_os("OMADESIGN_SVG_METRIC_QA") {
        let out = std::path::Path::new(&root).join(name);
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("original.svg"), source).unwrap();
        std::fs::write(out.join("editable.svg"), exported).unwrap();
        std::fs::write(out.join("native.png"), png).unwrap();
        std::fs::write(
            out.join("source.oma"),
            crate::project::encode(&doc).unwrap(),
        )
        .unwrap();
        std::fs::write(
            out.join("glyphs.json"),
            serde_json::to_vec_pretty(&chosen(run(&doc))).unwrap(),
        )
        .unwrap();
    }
    doc
}

/// Restrict only a real font's Unicode cmap while retaining its original glyph and shaping tables.
/// Args: original SFNT and allowed codepoints. Returns a valid derived font with corrected checksums.
fn subset(bytes: &[u8], allowed: &[char], variation: Option<(char, char, char)>) -> Vec<u8> {
    let face = rustybuzz::ttf_parser::Face::parse(bytes, 0).unwrap();
    let mut mappings: Vec<_> = allowed
        .iter()
        .filter_map(|&c| face.glyph_index(c).map(|g| (c as u32, g.0 as u32)))
        .collect();
    mappings.sort();
    mappings.dedup();
    let mut format12 = Vec::new();
    format12.extend(12u16.to_be_bytes());
    format12.extend(0u16.to_be_bytes());
    format12.extend((16u32 + 12 * mappings.len() as u32).to_be_bytes());
    format12.extend(0u32.to_be_bytes());
    format12.extend((mappings.len() as u32).to_be_bytes());
    for (c, g) in mappings {
        format12.extend(c.to_be_bytes());
        format12.extend(c.to_be_bytes());
        format12.extend(g.to_be_bytes());
    }
    let records = if variation.is_some() { 2u16 } else { 1 };
    let mut cmap = Vec::new();
    cmap.extend(0u16.to_be_bytes());
    cmap.extend(records.to_be_bytes());
    cmap.extend(3u16.to_be_bytes());
    cmap.extend(10u16.to_be_bytes());
    cmap.extend((4u32 + 8 * records as u32).to_be_bytes());
    if variation.is_some() {
        cmap.extend(0u16.to_be_bytes());
        cmap.extend(5u16.to_be_bytes());
        cmap.extend((4u32 + 8 * records as u32 + format12.len() as u32).to_be_bytes());
    }
    cmap.extend(format12);
    if let Some((base, selector, target)) = variation {
        let mut f = Vec::new();
        f.extend(14u16.to_be_bytes());
        f.extend(30u32.to_be_bytes());
        f.extend(1u32.to_be_bytes());
        f.extend(&(selector as u32).to_be_bytes()[1..]);
        f.extend(0u32.to_be_bytes());
        f.extend(21u32.to_be_bytes());
        f.extend(1u32.to_be_bytes());
        f.extend(&(base as u32).to_be_bytes()[1..]);
        f.extend(face.glyph_index(target).unwrap().0.to_be_bytes());
        cmap.extend(f);
    }
    let n = u16::from_be_bytes(bytes[4..6].try_into().unwrap()) as usize;
    let mut tables = Vec::new();
    for record in bytes[12..12 + n * 16].chunks_exact(16) {
        let tag: [u8; 4] = record[..4].try_into().unwrap();
        let offset = u32::from_be_bytes(record[8..12].try_into().unwrap()) as usize;
        let len = u32::from_be_bytes(record[12..16].try_into().unwrap()) as usize;
        let mut data = if &tag == b"cmap" {
            cmap.clone()
        } else {
            bytes[offset..offset + len].to_vec()
        };
        if &tag == b"head" {
            data[8..12].fill(0);
        }
        tables.push((tag, data));
    }
    fn sum(b: &[u8]) -> u32 {
        b.chunks(4)
            .map(|c| {
                let mut x = [0; 4];
                x[..c.len()].copy_from_slice(c);
                u32::from_be_bytes(x)
            })
            .fold(0, u32::wrapping_add)
    }
    let mut out = bytes[..12].to_vec();
    out.resize(12 + n * 16, 0);
    let mut head = 0;
    for (i, (tag, data)) in tables.into_iter().enumerate() {
        let offset = out.len();
        if &tag == b"head" {
            head = offset;
        }
        let record = 12 + i * 16;
        out[record..record + 4].copy_from_slice(&tag);
        out[record + 4..record + 8].copy_from_slice(&sum(&data).to_be_bytes());
        out[record + 8..record + 12].copy_from_slice(&(offset as u32).to_be_bytes());
        out[record + 12..record + 16].copy_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    let adjustment = 0xb1b0afbau32.wrapping_sub(sum(&out));
    out[head + 8..head + 12].copy_from_slice(&adjustment.to_be_bytes());
    assert_eq!(sum(&out), 0xb1b0afba);
    rustybuzz::ttf_parser::Face::parse(&out, 0).unwrap();
    out
}

#[test]
fn svg_fallback_preserves_primary_latin_and_explicit_hebrew_family_through_portable_roundtrip() {
    let p = primary();
    let f = hebrew();
    let source = document_source(
        &[("FallbackPrimary", &p), ("FallbackHebrew", &f)],
        "FallbackPrimary, FallbackHebrew",
        "AV אבג AV",
    );
    let doc = check("fallback-hebrew", &source);
    let glyphs = chosen(run(&doc));
    assert!(
        glyphs
            .iter()
            .filter(|(s, _, _)| s.chars().any(|c| matches!(c, 'A' | 'V')))
            .all(|(_, font, _)| font == &id(&p)),
        "{glyphs:?}"
    );
    assert!(
        glyphs
            .iter()
            .filter(|(s, _, _)| s.chars().any(|c| matches!(c, 'א'..='ת')))
            .all(|(_, font, g)| font == &id(&f) && *g != 0),
        "{glyphs:?}"
    );
    let refs = references(run(&doc).svg_text.as_ref().unwrap());
    assert!(refs.contains(&id(&p)) && refs.contains(&id(&f)));
}
#[test]
fn svg_fallback_preserves_arabic_joining_and_ligatures_without_replacing_latin() {
    let p = primary();
    let f = arabic();
    let source = document_source(
        &[("FallbackPrimary", &p), ("FallbackArabic", &f)],
        "FallbackPrimary, FallbackArabic",
        "AV ببب سلام لا AV",
    );
    let doc = check("fallback-arabic", &source);
    let actual = chosen(run(&doc));
    assert!(
        actual
            .iter()
            .filter(|(s, _, _)| s.chars().any(|c| c.is_ascii_alphabetic()))
            .all(|(_, font, _)| font == &id(&p))
    );
    let only = source.replace(
        "font-family=\"FallbackPrimary, FallbackArabic\"",
        "font-family=\"FallbackArabic\"",
    );
    let other = crate::formats::svg::read(&only, "Arabic reference")
        .unwrap()
        .0;
    let ids = |g: Vec<(String, String, u16)>| {
        g.into_iter()
            .filter(|(s, _, _)| s.chars().any(|c| matches!(c, '\u{600}'..='\u{6ff}')))
            .map(|(s, _, g)| (s, g))
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(actual), ids(chosen(run(&other))));
}
#[test]
fn svg_fallback_matches_whole_combining_grapheme_and_keeps_canonical_primary_composition() {
    let full = primary();
    let ascii: Vec<_> = (' '..='~').collect();
    let p = subset(&full, &ascii, None);
    let f = hebrew();
    let source = document_source(
        &[("ClusterPrimary", &p), ("ClusterFallback", &f)],
        "ClusterPrimary, ClusterFallback",
        "AV A&#x301; AV",
    );
    let doc = check("fallback-combining", &source);
    let glyphs = chosen(run(&doc));
    assert!(
        glyphs
            .iter()
            .any(|(s, font, g)| s == "A\u{301}" && font == &id(&f) && *g != 0),
        "{glyphs:?}"
    );
    assert!(
        glyphs
            .iter()
            .filter(|(s, _, _)| s == "V")
            .all(|(_, font, _)| font == &id(&p))
    );
    let mut composed = ascii;
    composed.push('é');
    let p = subset(&full, &composed, None);
    let source = document_source(
        &[("CanonicalPrimary", &p), ("ClusterFallback", &f)],
        "CanonicalPrimary, ClusterFallback",
        "AV e&#x301; AV",
    );
    let doc = check("fallback-canonical", &source);
    assert!(
        chosen(run(&doc))
            .iter()
            .all(|(_, font, g)| font == &id(&p) && *g != 0)
    );
}
#[test]
fn svg_fallback_uses_variation_sequence_face_and_ignores_unsupported_selector_without_font_loss() {
    let full = primary();
    let ascii: Vec<_> = (' '..='~').collect();
    let p = subset(&full, &ascii, None);
    let f = subset(&full, &ascii, Some(('A', '\u{fe00}', 'B')));
    let doc = check(
        "fallback-variation",
        &document_source(
            &[("VariationPrimary", &p), ("VariationFallback", &f)],
            "VariationPrimary, VariationFallback",
            "V A&#xFE00; V A&#xFE01; V",
        ),
    );
    let glyphs = chosen(run(&doc));
    let expected = rustybuzz::ttf_parser::Face::parse(&f, 0)
        .unwrap()
        .glyph_index('B')
        .unwrap()
        .0;
    assert!(
        glyphs
            .iter()
            .any(|(s, font, g)| s.contains('\u{fe00}') && font == &id(&f) && *g == expected),
        "{glyphs:?}"
    );
    assert!(
        glyphs
            .iter()
            .any(|(s, font, _)| s.contains('\u{fe01}') && font == &id(&p)),
        "{glyphs:?}"
    );
}
#[test]
fn svg_fallback_retains_ordered_unused_faces_and_typing_reselects_inherited_primary() {
    let p = primary();
    let f = hebrew();
    let source = document_source(
        &[("Comma, Primary", &p), ("Next Face", &f)],
        "'Unavailable First', 'Comma, Primary', 'Next Face'",
        "AV אב",
    );
    let mut doc = check("fallback-family-list", &source);
    let text = doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
        .find_map(|s| {
            if let Geom::Text(r) = &mut s.geom {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    let end = text.content.chars().count();
    text.replace_text(end, end, "AV", None);
    let glyphs = chosen(text);
    assert!(
        glyphs
            .iter()
            .filter(|(s, _, _)| s.chars().any(|c| matches!(c, 'A' | 'V')))
            .all(|(_, font, _)| font == &id(&p)),
        "{glyphs:?}"
    );
    let pending = text.typing_style(end - 1);
    assert!(pending.font.is_none());
    assert!(references(text.svg_text.as_ref().unwrap()).contains(&id(&f)));
}
