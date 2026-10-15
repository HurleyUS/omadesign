use super::*;
use crate::{document::Document, geom::Geom};

fn types(doc: &Document) -> Vec<&TypeRun> {
    doc.layers
        .iter()
        .filter_map(|l| l.kind.shapes())
        .flatten()
        .filter_map(|s| {
            if let Geom::Text(t) = &s.geom {
                Some(t)
            } else {
                None
            }
        })
        .collect()
}
fn render(source: &str) -> tiny_skia::Pixmap {
    let tree = usvg::Tree::from_str(&renderer_source(source), &options()).unwrap();
    let mut pm =
        tiny_skia::Pixmap::new(tree.size().width() as u32, tree.size().height() as u32).unwrap();
    resvg::render(&tree, Transform::identity(), &mut pm.as_mut());
    pm
}
fn error(a: &tiny_skia::Pixmap, b: &tiny_skia::Pixmap) -> f32 {
    assert_eq!(a.data().len(), b.data().len());
    a.data()
        .iter()
        .zip(b.data())
        .map(|(a, b)| a.abs_diff(*b) as f32)
        .sum::<f32>()
        / a.data().len() as f32
}

#[test]
fn svg_type_rotated_reflected_mixed_positioned_rtl_vertical_and_path_roundtrip() {
    for text in [
        r##"<text x="15" y="55" font-size="25" fill="#248" transform="matrix(.9 .25 -.2 1.1 30 4)">Affine <tspan font-size="36" fill="#d24" font-weight="bold">type</tspan></text>"##,
        r##"<text x="15 52 95" y="55 68 55" dx="0 3 -2" dy="0 -2 1" rotate="0 22 -15" font-size="32" fill="#248" transform="translate(245 0) scale(-1 1)">ABC</text>"##,
        r##"<text x="220" y="55" font-size="32" fill="#248" direction="rtl" unicode-bidi="bidi-override">مرحبا بالعالم</text>"##,
        r##"<text x="100" y="25" font-size="25" fill="#248" writing-mode="tb">Vertical</text>"##,
        r##"<defs><path id="curve" d="M10 80 Q130 -15 250 80"/></defs><text font-size="25" fill="#248"><textPath href="#curve" startOffset="12">Editable path text</textPath></text>"##,
    ] {
        let source = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="280" height="150">{text}</svg>"#
        );
        let (doc, notes) = crate::formats::svg::read(&source, "Type acceptance").unwrap();
        assert!(!notes.iter().any(|n| n.contains("outlines")), "{notes:?}");
        let runs = types(&doc);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].svg_text.is_some());
        assert!(!runs[0].contours.is_empty());
        let expected = render(&source);
        let actual = crate::compositor::render_export(&doc, 1).unwrap();
        assert!(
            error(&actual, &expected) < 0.1,
            "{}: {}",
            runs[0].content,
            error(&actual, &expected)
        );
        let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&doc).unwrap(),
            serde_json::to_value(&reopened).unwrap()
        );
        assert!(
            error(
                &actual,
                &crate::compositor::render_export(&reopened, 1).unwrap()
            ) < 0.001
        );
        let svg = crate::svg::export(&reopened).unwrap();
        assert!(svg.contains("<text"));
        assert!(error(&render(&svg), &expected) < 0.1);
        let again = crate::formats::svg::read(&svg, "Exported SVG").unwrap().0;
        assert_eq!(
            types(&again)
                .iter()
                .map(|t| t.content.as_str())
                .collect::<Vec<_>>(),
            runs.iter().map(|t| t.content.as_str()).collect::<Vec<_>>()
        );
    }
}

#[test]
fn svg_type_edit_style_positions_matrix_undo_and_font_substitution() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="280" height="150"><text x="15" y="55" font-family="Deliberately Missing SVG Font" font-size="25" transform="rotate(12 20 50)">Keep <tspan fill="#d24" font-size="32">style</tspan></text></svg>"##;
    let (mut doc, notes) = crate::formats::svg::read(source, "Editing").unwrap();
    assert!(notes.iter().any(|n| n.contains("substituted")), "{notes:?}");
    let shape = doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
        .find(|s| matches!(s.geom, Geom::Text(_)))
        .unwrap();
    let before = shape.geom.clone();
    let Geom::Text(run) = &mut shape.geom else {
        unreachable!()
    };
    run.replace_text(5, 10, "editable", None);
    run.set_character_style(5, 13, |s| {
        s.fill = Some(crate::document::Fill::Solid(crate::color::Rgba::rgb(
            20, 170, 60,
        )));
        s.px = Some(36.);
    });
    let after = crate::text::shape(run);
    assert!(!after.is_empty());
    assert_ne!(after, run.contours);
    assert_eq!(run.content, "Keep editable");
    assert_eq!(crate::text::hit_char(run, crate::text::caret_pt(run, 0)), 0);
    assert!(
        !crate::text_geometry::selection_quads(run, 5, 13)
            .unwrap()
            .is_empty()
    );
    let (top, bottom) = crate::text::caret_segment(run, 5);
    assert!((top.x - bottom.x).abs() > 1.);
    shape.geom = before;
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(types(&reopened)[0].content, "Keep style");
}

#[test]
fn svg_type_flow_retains_story_reflows_edits_and_exports_standard_text() {
    for source in [
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="280" height="180"><defs><rect id="frame" x="15" y="10" width="150" height="140"/></defs><text font-size="22" fill="#248" style="shape-inside:url(#frame);line-height:28">Editable flowing words <tspan fill="#d24">retain their style</tspan> after reflow.</text></svg>"##,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="280" height="180"><flowRoot font-size="22" fill="#248"><flowRegion><rect x="15" y="10" width="150" height="140"/></flowRegion><flowPara>Editable flowing words retain their style after reflow.</flowPara></flowRoot></svg>"##,
    ] {
        let (mut doc, notes) = crate::formats::svg::read(source, "Flow").unwrap();
        assert!(!notes.iter().any(|n| n.contains("outlines")), "{notes:?}");
        assert_eq!(types(&doc).len(), 1);
        let run = types(&doc)[0];
        assert!(run.svg_text.as_ref().unwrap().flow.is_some());
        assert!(run.content.contains("Editable flowing words"));
        let old = crate::compositor::render_export(&doc, 1).unwrap();
        let shape = doc
            .layers
            .iter_mut()
            .filter_map(|l| l.kind.shapes_mut())
            .flatten()
            .find(|s| matches!(s.geom, Geom::Text(_)))
            .unwrap();
        let Geom::Text(run) = &mut shape.geom else {
            unreachable!()
        };
        run.replace_text(0, 8, "Short", None);
        run.contours = crate::text::shape(run);
        let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        let current = crate::compositor::render_export(&reopened, 1).unwrap();
        assert!(error(&old, &current) > 0.1);
        let svg = crate::svg::export(&reopened).unwrap();
        assert!(svg.contains("data-oma-flow"));
        assert!(error(&render(&svg), &current) < 0.1);
        let again = crate::formats::svg::read(&svg, "Flow SVG").unwrap().0;
        assert_eq!(types(&again)[0].content, types(&reopened)[0].content);
        assert!(types(&again)[0].svg_text.as_ref().unwrap().flow.is_some());
    }
}

#[test]
fn svg_type_independent_librsvg_appearance_and_editable_export() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let source = include_str!("../../../tests/assets/svg-type/affine-styles.svg");
    fn independent(source: &str) -> tiny_skia::Pixmap {
        let mut child = Command::new("rsvg-convert")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("librsvg is required for independent SVG type qualification");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        tiny_skia::Pixmap::decode_png(&output.stdout).unwrap()
    }
    let (doc, notes) = crate::formats::svg::read(source, "Independent type").unwrap();
    assert!(!notes.iter().any(|n| n.contains("outlines")), "{notes:?}");
    assert_eq!(types(&doc).len(), 5);
    let expected = independent(source);
    let native = crate::compositor::render_export(&doc, 1).unwrap();
    assert!(
        error(&native, &expected) < 1.5,
        "independent native mean={}",
        error(&native, &expected)
    );
    let exported = crate::svg::export(&doc).unwrap();
    assert_eq!(exported.matches("<text ").count(), 5);
    assert!(
        error(&independent(&exported), &expected) < 1.5,
        "independent exported mean={}",
        error(&independent(&exported), &expected)
    );
}

#[test]
fn svg_type_rtl_repeated_letters_keep_logical_carets_and_source_direction() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="100"><text x="270" y="55" font-family="DejaVu Sans" font-size="30" direction="rtl" unicode-bidi="bidi-override">مرحبا بالعالم</text></svg>"##;
    let (doc, _) = crate::formats::svg::read(source, "Logical RTL").unwrap();
    let run = types(&doc)[0];
    assert_eq!(run.content, "مرحبا بالعالم");
    let first = crate::text::caret_pt(run, 0);
    let last = crate::text::caret_pt(run, run.content.chars().count());
    assert!(first.x > last.x, "{first:?}, {last:?}");
    assert_eq!(crate::text::hit_char(run, first), 0);
    let exported = crate::svg::export(&doc).unwrap();
    assert!(exported.contains("direction=\"rtl\""));
    assert!(!exported.contains('\u{202e}'));
    assert_eq!(
        types(&crate::formats::svg::read(&exported, "RTL").unwrap().0)[0].content,
        run.content
    );
}

#[test]
fn svg_type_saved_metadata_cannot_add_executable_svg_content() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><text x="20" y="50" transform="rotate(10)">Safe type</text></svg>"##;
    let (mut doc, _) = crate::formats::svg::read(source, "Safe type").unwrap();
    let shape = doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
        .find(|s| matches!(s.geom, Geom::Text(_)))
        .unwrap();
    let Geom::Text(run) = &mut shape.geom else {
        unreachable!()
    };
    let svg = run.svg_text.as_mut().unwrap();
    svg.definitions
        .push_str("<script>alert(1)</script><image href=\"https://invalid.example/private.png\"/>");
    let TextNode::Element {
        attributes,
        children,
        ..
    } = &mut svg.root
    else {
        unreachable!()
    };
    attributes.insert("onload".into(), "alert(1)".into());
    children.push(TextNode::Element {
        name: "script".into(),
        attributes: BTreeMap::new(),
        children: vec![],
    });
    let exported = crate::svg::export(&doc).unwrap();
    let xml = roxmltree::Document::parse(&exported).unwrap();
    assert!(!xml.descendants().any(|n| n.has_tag_name("script")
        || n.attribute("onload").is_some()
        || n.has_tag_name("image")));
}
