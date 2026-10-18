use super::*;
use crate::{
    app::Studio,
    geom::{Geom, KernMode},
};
use base64::Engine;

pub(crate) fn source() -> String {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/assets/fonts/EBGaramond.ttf"
    ))
    .unwrap();
    let font = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="420" height="180"><style>@font-face{{font-family:MetricFace;src:url(data:font/ttf;base64,{font})}}</style><text xml:space="preserve" x="25" y="80" font-family="MetricFace" font-size="20" style="font-feature-settings:'kern' 0,'liga' 0,'tnum' 1;font-kerning:none"><tspan font-size="40" letter-spacing="4" baseline-shift="10"><tspan baseline-shift="2">AV</tspan></tspan> next</text></svg>"#
    )
}

pub(crate) fn studio() -> Studio {
    let (doc, notes) = crate::formats::svg::read(&source(), "Imported character metrics").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let mut studio = Studio::new();
    studio.show_welcome = false;
    studio.doc = doc;
    let (li, id) = studio
        .doc
        .layers
        .iter()
        .enumerate()
        .find_map(|(li, layer)| {
            layer
                .kind
                .shapes()
                .into_iter()
                .flatten()
                .find(|s| matches!(s.geom, Geom::Text(_)))
                .map(|s| (li, s.id))
        })
        .unwrap();
    studio.selection = vec![(li, id)];
    studio.active_layer = Some(li);
    studio.mark();
    studio.history.clear();
    studio
}

fn run(studio: &Studio) -> &TypeRun {
    let (layer, id) = studio.selection[0];
    let Geom::Text(run) = &studio.doc.find_shape(layer, id).unwrap().geom else {
        panic!()
    };
    run
}

pub(crate) fn compare(studio: &Studio, expected: &str, name: &str) {
    let (expected_doc, notes) = crate::formats::svg::read(expected, "Expected metrics").unwrap();
    assert!(notes.is_empty());
    let image = crate::compositor::render_export(&studio.doc, 1).unwrap();
    let wanted = crate::compositor::render_export(&expected_doc, 1).unwrap();
    let error = image
        .data()
        .iter()
        .zip(wanted.data())
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum::<u64>() as f64
        / image.data().len() as f64;
    assert!(error < 0.1, "{name}: {error}");
    let saved = crate::project::encode(&studio.doc).unwrap();
    let reopened = crate::project::decode(&saved).unwrap();
    assert_eq!(
        image.encode_png().unwrap(),
        crate::compositor::export_png(&reopened, 1).unwrap()
    );
    let exported = crate::svg::export_embedded(&studio.doc).unwrap();
    let (again, notes) = crate::formats::svg::read(&exported, "Exported metrics").unwrap();
    assert!(notes.is_empty());
    let again_png = crate::compositor::export_png(&again, 1).unwrap();
    if image.encode_png().unwrap() != again_png {
        if let Some(root) = std::env::var_os("OMADESIGN_SVG_METRIC_QA") {
            let out = std::path::Path::new(&root).join(format!("failure-{name}"));
            std::fs::create_dir_all(&out).unwrap();
            std::fs::write(out.join("editable.svg"), &exported).unwrap();
            std::fs::write(out.join("original.oma"), &saved).unwrap();
            std::fs::write(
                out.join("again.oma"),
                crate::project::encode(&again).unwrap(),
            )
            .unwrap();
            std::fs::write(out.join("original.png"), image.encode_png().unwrap()).unwrap();
            std::fs::write(out.join("again.png"), &again_png).unwrap();
            std::fs::write(
                out.join("original-runtime.svg"),
                super::source(run(studio)).unwrap(),
            )
            .unwrap();
            let after = again
                .layers
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
                .unwrap();
            std::fs::write(out.join("again-runtime.svg"), super::source(after).unwrap()).unwrap();
            let points = |r: &TypeRun| {
                (0..r.content.chars().count())
                    .map(|i| {
                        let p = crate::text::caret_pt(r, i);
                        [p.x, p.y]
                    })
                    .collect::<Vec<_>>()
            };
            std::fs::write(
                out.join("positions.json"),
                serde_json::to_vec(
                    &serde_json::json!({"before":points(run(studio)),"after":points(after)}),
                )
                .unwrap(),
            )
            .unwrap();
        }
    }
    assert_eq!(image.encode_png().unwrap(), again_png);
    if let Some(root) = std::env::var_os("OMADESIGN_SVG_METRIC_QA") {
        let out = std::path::Path::new(&root).join(name);
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("editable.svg"), exported).unwrap();
        std::fs::write(out.join("native.png"), image.encode_png().unwrap()).unwrap();
        std::fs::write(out.join("expected.png"), wanted.encode_png().unwrap()).unwrap();
        std::fs::write(out.join("source.oma"), saved).unwrap();
        let text = run(studio);
        let positions = (0..text.content.chars().count())
            .map(|i| {
                let p = crate::text::caret_pt(text, i);
                [p.x, p.y]
            })
            .collect::<Vec<_>>();
        std::fs::write(
            out.join("positions.json"),
            serde_json::to_vec(&positions).unwrap(),
        )
        .unwrap();
    }
}

fn history(studio: &mut Studio, before: &str) {
    assert_eq!(studio.history.len(), 1);
    let after = crate::project::encode(&studio.doc).unwrap();
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
}

#[test]
fn svg_character_metrics_report_inherited_size_tracking_shift_and_scaled_length_without_mutation() {
    let mut s = studio();
    let before = crate::project::encode(&s.doc).unwrap();
    let values = s.selected_character_metrics();
    assert_eq!(values.len(), 7);
    for value in &values[..2] {
        assert_eq!(value.px, Some(40.));
        assert_eq!(value.tracking, Some(100.));
        assert_eq!(value.baseline_shift, Some(12.));
        assert_eq!(value.kerning, Some(KernMode::None));
    }
    for value in &values[2..] {
        assert_eq!(value.px, Some(20.));
        assert_eq!(value.tracking, Some(0.));
        assert_eq!(value.baseline_shift, Some(0.));
    }
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    let scaled = source().replace(
        "letter-spacing=\"4\"",
        "letter-spacing=\"4\" textLength=\"160\" lengthAdjust=\"spacingAndGlyphs\"",
    );
    s.doc = crate::formats::svg::read(&scaled, "Scaled metrics")
        .unwrap()
        .0;
    let (li, id) = s
        .doc
        .layers
        .iter()
        .enumerate()
        .find_map(|(li, l)| {
            l.kind
                .shapes()
                .into_iter()
                .flatten()
                .find(|s| matches!(s.geom, Geom::Text(_)))
                .map(|s| (li, s.id))
        })
        .unwrap();
    s.selection = vec![(li, id)];
    let scaled = s.selected_character_metrics();
    assert!(scaled[0].hscale.unwrap() > 150.);
    assert!((scaled[0].vscale.unwrap() - 100.).abs() < 0.001);
}

#[test]
fn svg_character_tracking_uses_each_actual_font_size_and_one_document_undo() {
    let mut s = studio();
    let before = crate::project::encode(&s.doc).unwrap();
    s.patch_character(|style| style.tracking = Some(200.));
    assert!(
        s.selected_character_metrics()
            .iter()
            .all(|m| m.tracking == Some(200.))
    );
    let expected = source()
        .replace("x=\"25\"", "letter-spacing=\"4\" x=\"25\"")
        .replace(
            "letter-spacing=\"4\" baseline-shift",
            "letter-spacing=\"8\" baseline-shift",
        );
    compare(&s, &expected, "tracking");
    history(&mut s, &before);
}

#[test]
fn svg_character_baseline_sets_an_absolute_shift_through_nested_source_shifts() {
    for shift in [0., 6., -4.] {
        let mut s = studio();
        let before = crate::project::encode(&s.doc).unwrap();
        s.patch_character(|style| style.baseline_shift = Some(shift));
        let expected = source()
            .replace("y=\"80\"", &format!("y=\"{}\"", 80. - shift))
            .replace("baseline-shift=\"2\"", "baseline-shift=\"-10\"");
        assert!(
            s.selected_character_metrics()
                .iter()
                .all(|m| m.baseline_shift == Some(shift))
        );
        compare(&s, &expected, &format!("baseline-{shift}"));
        history(&mut s, &before);
    }
}

#[test]
fn svg_character_metrics_kerning_overrides_an_imported_low_level_tag_and_preserves_other_features()
{
    let mut s = studio();
    let before = crate::project::encode(&s.doc).unwrap();
    let image = crate::compositor::export_png(&s.doc, 1).unwrap();
    s.patch_character(|style| style.kerning = Some(KernMode::Metrics));
    let expected = source()
        .replace("'kern' 0", "'kern' 1")
        .replace("font-kerning:none", "font-kerning:normal");
    assert_ne!(crate::compositor::export_png(&s.doc, 1).unwrap().len(), 0);
    assert!(image != crate::compositor::export_png(&s.doc, 1).unwrap());
    compare(&s, &expected, "kerning");
    assert_eq!(crate::text::feature_value(run(&s), 0, *b"tnum"), 1);
    history(&mut s, &before);
}

#[test]
fn svg_character_nested_length_groups_keep_parent_budget_and_explicit_child_widths() {
    for (name,content,adjust,width) in [
        ("length-parent","AV".to_owned(),"spacingAndGlyphs",160),
        ("length-mixed","AV<tspan font-size=\"20\" fill=\"#248\"> To</tspan>X".to_owned(),"spacingAndGlyphs",160),
        ("length-child-only","<tspan font-size=\"20\" textLength=\"60\" lengthAdjust=\"spacingAndGlyphs\" fill=\"#248\"> To</tspan>".to_owned(),"spacingAndGlyphs",200),
        ("length-child","AV<tspan font-size=\"20\" textLength=\"60\" lengthAdjust=\"spacingAndGlyphs\" fill=\"#248\"> To</tspan>X".to_owned(),"spacingAndGlyphs",200),
        ("length-spacing","AV<tspan font-size=\"20\" fill=\"#248\"> To</tspan>X".to_owned(),"spacing",160),
    ] {
        let source=source().replace("letter-spacing=\"4\"",&format!("letter-spacing=\"4\" textLength=\"{width}\" lengthAdjust=\"{adjust}\""))
            .replace(">AV</tspan>",&format!(">{content}</tspan>"));
        let mut s=studio();s.doc=crate::formats::svg::read(&source,name).unwrap().0;
        let (li,id)=s.doc.layers.iter().enumerate().find_map(|(li,l)|l.kind.shapes().into_iter().flatten().find(|s|matches!(s.geom,Geom::Text(_))).map(|s|(li,s.id))).unwrap();s.selection=vec![(li,id)];
        compare(&s,&source,name);
        if name=="length-parent" {
            let text=run(&s);let bytes=crate::text::font_bytes_for(text).unwrap();
            let face=rustybuzz::ttf_parser::Face::parse(&bytes,0).unwrap();
            let space=f32::from(face.glyph_hor_advance(face.glyph_index(' ').unwrap()).unwrap())*20./f32::from(face.units_per_em());
            assert!((crate::text::caret_pt(text,3).x-(25.+width as f32+space)).abs()<0.01);
        }
    }
}

#[test]
fn svg_character_export_requests_precise_geometry_and_retains_explicit_source_rendering_hints() {
    let s = studio();
    let generated = crate::svg::export_embedded(&s.doc).unwrap();
    assert!(generated.contains("text-rendering=\"geometricPrecision\""));
    let original = source().replace(
        "<text xml:space",
        "<text text-rendering=\"optimizeSpeed\" xml:space",
    );
    let doc = crate::formats::svg::read(&original, "Authored rendering hint")
        .unwrap()
        .0;
    let generated = crate::svg::export_embedded(&doc).unwrap();
    assert!(generated.contains("text-rendering=\"optimizeSpeed\""));
    let again = crate::formats::svg::read(&generated, "Preserved hint")
        .unwrap()
        .0;
    assert_eq!(
        crate::compositor::export_png(&doc, 1).unwrap(),
        crate::compositor::export_png(&again, 1).unwrap()
    );
}
