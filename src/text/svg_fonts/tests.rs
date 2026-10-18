use super::*;
use crate::{
    color::Rgba,
    document::{Layer, Shape, Style},
    geom::Pt,
    text::variations::FontVariations,
};
const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/assets/fonts/EBGaramond.ttf"
);
fn sample() -> Document {
    let mut doc = Document::new("Portable SVG fonts", 520., 220., 72.);
    doc.transparent = true;
    doc.layers = vec![Layer::vector("Editable text")];
    for (i, weight) in [400., 800., 400.].into_iter().enumerate() {
        let mut run = TypeRun {
            font: FONT.into(),
            font_variations: FontVariations::new(vec![(*b"wght", weight)]).unwrap(),
            content: if i == 2 { "AVAV" } else { "Variable AV fi12" }.into(),
            px: 32.,
            origin: Pt::new(24., 50. + i as f32 * 64.),
            ..Default::default()
        };
        if i == 2 {
            run.set_character_style(0, 2, |s| {
                s.font_variations = Some(FontVariations::new(vec![(*b"wght", 800.)]).unwrap())
            });
        }
        let mut shape = Shape::new(
            Geom::Text(run),
            Style {
                fill: Fill::Solid(Rgba::BLACK),
                stroke: None,
            },
        );
        super::super::fill_contours(&mut shape.geom);
        doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    }
    doc
}
fn error(a: &Document, b: &Document) -> f64 {
    let a = crate::compositor::render_export(a, 1).unwrap();
    let b = crate::compositor::render_export(b, 1).unwrap();
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    a.data()
        .iter()
        .zip(b.data())
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum::<u64>() as f64
        / a.data().len() as f64
}
fn root() -> std::path::PathBuf {
    std::env::var_os("OMADESIGN_SVG_FONT_QA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".artifacts/svg-font-tests")
        })
}
#[test]
fn craft_svg_fonts_keep_actual_program_unicode_variations_and_native_reopen() {
    let doc = sample();
    let before = crate::project::encode(&doc).unwrap();
    let font = std::fs::read(FONT).unwrap();
    let (encoded, notes) = crate::export::encode_svg_admitted(
        &crate::export::Source::Document(doc.clone()),
        true,
        &crate::ml::NoProgress,
    )
    .unwrap();
    assert!(notes.is_empty());
    let source = std::str::from_utf8(encoded.as_slice()).unwrap();
    let xml = roxmltree::Document::parse(source).unwrap();
    assert_eq!(
        xml.descendants().filter(|n| n.has_tag_name("text")).count(),
        3
    );
    assert_eq!(
        xml.descendants().filter(|n| n.has_tag_name("path")).count(),
        0
    );
    assert_eq!(source.matches("@font-face").count(), 1);
    let data = source
        .split("data:font/ttf;base64,")
        .nth(1)
        .unwrap()
        .split(')')
        .next()
        .unwrap();
    assert_eq!(STANDARD.decode(data).unwrap(), font);
    let logical: String = xml
        .descendants()
        .filter(|n| n.is_text() && !n.parent_element().is_some_and(|p| p.has_tag_name("style")))
        .filter_map(|n| n.text())
        .collect();
    assert_eq!(
        logical
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>(),
        "VariableAVfi12VariableAVfi12AVAV"
    );
    assert!(source.contains("wght&apos; 800") || source.contains("wght' 800"));
    let (imported, notes) = crate::formats::svg::read(source, "portable.svg").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let mean = error(&doc, &imported);
    assert!(mean < 1.5, "Native embedded SVG mean {mean}");
    let font_id = format!("omatype:{:032x}", crate::typography::fingerprint(&font));
    let runs: Vec<_> = imported
        .layers
        .iter()
        .flat_map(|l| l.kind.shapes().into_iter().flatten())
        .filter_map(|s| {
            if let Geom::Text(t) = &s.geom {
                Some(t)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(runs.len(), 3);
    assert!(runs.iter().all(|r| r.font == font_id));
    assert!(runs.iter().flat_map(|r| r.spans.iter()).any(|s| {
        s.font_variations
            .as_ref()
            .is_some_and(|v| v.settings().contains(&(*b"wght", 800.)))
    }));
    let portable = crate::project::encode(&imported).unwrap();
    let reopened = crate::project::decode(&portable).unwrap();
    assert_eq!(error(&imported, &reopened), 0.);
    let again = crate::svg::export_embedded(&imported).unwrap();
    let (twice, notes) = crate::formats::svg::read(&again, "again.svg").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    assert!(error(&imported, &twice) < 1.5);
    assert_eq!(crate::project::encode(&doc).unwrap(), before);
    assert_eq!(std::fs::read(FONT).unwrap(), font);
    let out = root();
    std::fs::create_dir_all(&out).unwrap();
    for (name, bytes) in [
        ("editable.svg", source.as_bytes()),
        ("again.svg", again.as_bytes()),
        ("portable.oma", portable.as_bytes()),
        ("source.oma", before.as_bytes()),
    ] {
        std::fs::write(out.join(name), bytes).unwrap();
    }
    std::fs::write(
        out.join("native.png"),
        crate::compositor::export_png(&doc, 1).unwrap(),
    )
    .unwrap();
    std::fs::write(
        out.join("imported.png"),
        crate::compositor::export_png(&imported, 1).unwrap(),
    )
    .unwrap();
    std::fs::write(out.join("native.json"),serde_json::json!({"mean_channel_error":mean,"font_bytes":font.len(),"font_id":font_id,"text_objects":runs.len(),"font_faces":source.matches("@font-face").count()}).to_string()).unwrap();
}
#[test]
fn craft_svg_fonts_enforce_permissions_and_keep_explicit_legacy_outline_route() {
    let mut doc = sample();
    let mut bytes = std::fs::read(FONT).unwrap();
    let face = rustybuzz::ttf_parser::Face::parse(&bytes, 0).unwrap();
    let record = face
        .raw_face()
        .table_records
        .into_iter()
        .find(|r| r.tag.to_bytes() == *b"OS/2")
        .unwrap();
    let at = record.offset as usize + 8;
    bytes[at..at + 2].copy_from_slice(&2u16.to_be_bytes());
    let id = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
    super::super::register_memory_font(&id, "Restricted fixture", Arc::new(bytes)).unwrap();
    if let Geom::Text(run) = &mut doc.layers[0].kind.shapes_mut().unwrap()[0].geom {
        run.font = id;
    }
    let before = crate::project::encode(&doc).unwrap();
    assert!(
        crate::svg::export_embedded(&doc)
            .unwrap_err()
            .contains("does not permit")
    );
    assert!(crate::svg::export(&doc).unwrap().contains("<path"));
    assert_eq!(crate::project::encode(&doc).unwrap(), before);
}
#[test]
fn craft_svg_fonts_inline_parser_preserves_aliases_and_refuses_malformed_data_without_network() {
    let data = STANDARD.encode(std::fs::read(FONT).unwrap());
    let source = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="140" height="60"><style>/* a comment with {{ }} */ @font-face {{ font-family:'Local Face'; font-weight:400 800; src:local('missing'),url('data:font/ttf;base64,{data}') format('truetype'); }} .s{{font-family:'Local Face';font-size:24px;font-variation-settings:'wght' 800}}</style><text class="s" x="10" y="40">AV fi</text></svg>"#
    );
    let (doc, notes) = crate::formats::svg::read(&source, "alias.svg").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    assert!(doc.layers.iter().flat_map(|l|l.kind.shapes().into_iter().flatten()).any(|s|matches!(&s.geom,Geom::Text(t) if t.font.starts_with("omatype:") && t.spans.iter().any(|s|s.font_variations.as_ref().is_some_and(|v|v.settings().contains(&(*b"wght",800.)))))));
    let malformed = source.replace(&data, "broken!");
    assert!(
        crate::formats::svg::read(&malformed, "bad.svg")
            .unwrap_err()
            .contains("base64")
    );
    let invalid = source.replace(&data, &STANDARD.encode(b"not a font"));
    assert!(
        crate::formats::svg::read(&invalid, "bad.svg")
            .unwrap_err()
            .contains("Unsupported embedded")
    );
    let linked = source.replace(
        &format!("data:font/ttf;base64,{data}"),
        "https://127.0.0.1:9/never-loaded.ttf",
    );
    let (_, notes) = crate::formats::svg::read(&linked, "linked.svg").unwrap();
    assert!(notes.iter().any(|s| s.contains("substituted")));
}
#[test]
#[ignore = "A fresh process proves SVG and project fonts load from their actual embedded bytes"]
fn craft_svg_fonts_cold_import_and_project_reopen() {
    craft_svg_fonts_keep_actual_program_unicode_variations_and_native_reopen();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "text::svg_fonts::tests::craft_svg_fonts_cold_child",
            "--ignored",
            "--nocapture",
        ])
        .env("OMADESIGN_SVG_FONT_CHILD", root())
        .output()
        .unwrap();
    std::fs::write(
        root().join("cold.log"),
        [output.stdout, output.stderr].concat(),
    )
    .unwrap();
    assert!(output.status.success());
}
#[test]
#[ignore = "Spawned only by its actual cold-parent fixture"]
fn craft_svg_fonts_cold_child() {
    let Some(out) = std::env::var_os("OMADESIGN_SVG_FONT_CHILD") else {
        return;
    };
    let out = std::path::PathBuf::from(out);
    let source = std::fs::read_to_string(out.join("editable.svg")).unwrap();
    let (doc, notes) = crate::formats::svg::read(&source, "cold.svg").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let encoded = std::fs::read_to_string(out.join("portable.oma")).unwrap();
    let reopened = crate::project::decode(&encoded).unwrap();
    assert_eq!(error(&doc, &reopened), 0.);
    assert_eq!(
        crate::compositor::export_png(&doc, 1).unwrap(),
        std::fs::read(out.join("imported.png")).unwrap()
    );
    std::fs::write(
        out.join("cold.oma"),
        crate::project::encode(&reopened).unwrap(),
    )
    .unwrap();
}

#[test]
fn craft_svg_fonts_imported_native_typing_keeps_face_axes_one_undo_and_reopen() {
    let original = sample();
    let before_source = crate::project::encode(&original).unwrap();
    let svg = crate::svg::export_embedded(&original).unwrap();
    let (doc, notes) = crate::formats::svg::read(&svg, "Type imported font").unwrap();
    assert!(notes.is_empty());
    let mut studio = crate::app::Studio::new();
    studio.show_welcome = false;
    studio.doc = doc;
    let (layer, id, caret, old_length, font) = studio
        .doc
        .layers
        .iter()
        .enumerate()
        .flat_map(|(li, l)| l.kind.shapes().into_iter().flatten().map(move |s| (li, s)))
        .find_map(|(li, s)| {
            let Geom::Text(run) = &s.geom else {
                return None;
            };
            let n = run.content.chars().count();
            (run.content == "Variable AV fi12"
                && super::super::variations::at(run, 0)
                    .settings()
                    .contains(&(*b"wght", 800.)))
            .then(|| {
                (
                    li,
                    s.id,
                    super::super::caret_pt(run, n),
                    n,
                    run.font.clone(),
                )
            })
        })
        .unwrap();
    studio.selection = vec![(layer, id)];
    studio.active_layer = Some(layer);
    studio.mark();
    studio.history.clear();
    let before = crate::project::encode(&studio.doc).unwrap();
    studio.begin_type_edit((layer, id), caret);
    studio.type_insert(" QZ");
    studio.commit_type_edit();
    assert_eq!(studio.history.len(), 1);
    let Geom::Text(run) = &studio.doc.find_shape(layer, id).unwrap().geom else {
        panic!()
    };
    assert_eq!(run.content, "Variable AV fi12 QZ");
    assert_eq!(run.font, font);
    assert_eq!(
        run.character_style(old_length + 1).font.as_deref(),
        Some(font.as_str())
    );
    assert!(
        super::super::variations::at(run, old_length + 1)
            .settings()
            .contains(&(*b"wght", 800.))
    );
    let after = crate::project::encode(&studio.doc).unwrap();
    let reopened = crate::project::decode(&after).unwrap();
    assert_eq!(error(&studio.doc, &reopened), 0.);
    let edited = crate::svg::export_embedded(&studio.doc).unwrap();
    assert!(edited.contains(" QZ"));
    let (reimported, notes) = crate::formats::svg::read(&edited, "Edited SVG").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    assert!(error(&studio.doc, &reimported) < 1.5);
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    assert_eq!(crate::project::encode(&original).unwrap(), before_source);
    let out = root();
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("edited.svg"), edited).unwrap();
    std::fs::write(out.join("edited.oma"), after).unwrap();
    std::fs::write(
        out.join("edited-native.png"),
        crate::compositor::export_png(&studio.doc, 1).unwrap(),
    )
    .unwrap();
}
