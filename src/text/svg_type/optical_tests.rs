use super::*;
use crate::{
    app::Studio,
    geom::{Geom, KernMode},
};

fn source(body: &str) -> String {
    super::pair_tests::source(body)
}
fn studio(body: &str) -> Studio {
    super::metric_tests::studio_for(&source(body))
}
fn run(studio: &Studio) -> &TypeRun {
    super::pair_tests::run(studio)
}
fn set(studio: &mut Studio, mode: KernMode) {
    studio.patch_character(|s| s.kerning = Some(mode));
}
fn pixels(studio: &Studio) -> Vec<u8> {
    crate::compositor::export_png(&studio.doc, 1).unwrap()
}
fn glyphs(run: &TypeRun) -> Vec<(usize, u16, String)> {
    let layout = layout(run).unwrap();
    layout
        .glyphs
        .iter()
        .map(|g| (g.index, g.id, g.font.clone()))
        .collect()
}
pub(crate) fn check(studio: &Studio, name: &str) {
    let png = pixels(studio);
    let saved = crate::project::encode(&studio.doc).unwrap();
    let reopened = crate::project::decode(&saved).unwrap();
    assert_eq!(png, crate::compositor::export_png(&reopened, 1).unwrap());
    let exported = crate::svg::export_embedded(&studio.doc).unwrap();
    let (again, notes) = crate::formats::svg::read(&exported, "Optical roundtrip").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(
        png,
        crate::compositor::export_png(&again, 1).unwrap(),
        "{name}"
    );
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
    assert_eq!(after.content, run(studio).content);
    for i in 0..after.content.chars().count() {
        if run(studio).character_style(i).kerning == Some(KernMode::Optical) {
            assert_eq!(after.character_style(i).kerning, Some(KernMode::Optical));
        }
    }
    if let Some(root) = std::env::var_os("OMADESIGN_SVG_METRIC_QA") {
        let out = std::path::Path::new(&root).join(name);
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("editable.svg"), exported).unwrap();
        std::fs::write(out.join("native.png"), png).unwrap();
        std::fs::write(out.join("source.oma"), saved).unwrap();
        let positions: Vec<_> = (0..run(studio).content.chars().count())
            .map(|i| {
                let p = crate::text::caret_pt(run(studio), i);
                [p.x, p.y]
            })
            .collect();
        std::fs::write(out.join("positions.json"), serde_json::to_vec(&positions).unwrap()).unwrap();
    }
}
#[test]
fn svg_optical_matches_shared_native_outlines_and_keeps_one_whole_document_undo() {
    let mut s = studio(
        r#"<text x="25" y="80" font-family="MetricFace" font-size="40" style="font-feature-settings:'liga' 0">AVWaTo</text>"#,
    );
    let before = crate::project::encode(&s.doc).unwrap();
    let root = run(&s).svg_text.clone();
    set(&mut s, KernMode::Optical);
    assert_eq!(run(&s).svg_text, root);
    assert_eq!(s.history.len(), 1);
    let mut native = run(&s).clone();
    native.svg_text = None;
    native.transform = None;
    native.origin = Pt::new(25., 80.);
    native.contours = crate::text::shape(&native);
    let a = crate::text::caret_pt(run(&s), 0);
    let b = crate::text::caret_pt(&native, 0);
    for i in 1..native.content.chars().count() {
        let x = crate::text::caret_pt(run(&s), i).x - a.x;
        let y = crate::text::caret_pt(&native, i).x - b.x;
        assert!((x - y).abs() < 0.001, "{i}: {x} vs {y}");
    }
    check(&s, "optical-native");
    let after = crate::project::encode(&s.doc).unwrap();
    s.undo();
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    s.redo();
    assert_eq!(crate::project::encode(&s.doc).unwrap(), after);
}
#[test]
fn svg_optical_keeps_tracking_and_accepted_manual_pair_offsets_additive() {
    let mut s = studio(
        r#"<text x="25" y="80" dx="0 0 7" font-family="MetricFace" font-size="40" style="font-feature-settings:'liga' 0">AVWaTo</text>"#,
    );
    set(&mut s, KernMode::Optical);
    let baseline: Vec<_> = (0..6).map(|i| crate::text::caret_pt(run(&s), i)).collect();
    s.patch_character(|v| v.tracking = Some(100.));
    for (i, b) in baseline.iter().enumerate() {
        let p = crate::text::caret_pt(run(&s), i);
        assert!(
            (p.x - b.x - i as f32 * 4.).abs() < 0.002,
            "{i}: {p:?} {b:?}"
        );
    }
    let tracked: Vec<_> = (0..6).map(|i| crate::text::caret_pt(run(&s), i)).collect();
    s.patch_type(|r| {
        r.manual_kern.insert(2, 150.);
    });
    for (i, b) in tracked.iter().enumerate() {
        let p = crate::text::caret_pt(run(&s), i);
        assert!(
            (p.x - b.x - if i >= 2 { -1. } else { 0. }).abs() < 0.002,
            "{i}: {p:?} {b:?}"
        );
    }
    check(&s, "optical-tracking-pair");
}
#[test]
fn svg_optical_range_inside_optional_ligature_splits_only_the_affected_boundary() {
    let mut s = studio(
        r#"<text x="25" y="80" font-family="MetricFace" font-size="40">fi office AV</text>"#,
    );
    let mut r = run(&s).clone();
    r.set_character_style(1, 2, |v| v.kerning = Some(KernMode::Optical));
    s.patch_type(|v| *v = r);
    let layout = layout(run(&s)).unwrap();
    let mut nodes = Vec::new();
    texts(layout.tree.root(), &mut nodes);
    let labels: Vec<_> = nodes
        .iter()
        .flat_map(|t| t.layouted())
        .flat_map(|s| &s.positioned_glyphs)
        .map(|g| g.text.as_str())
        .collect();
    assert!(labels.contains(&"f") && labels.contains(&"i"), "{labels:?}");
    assert!(labels.iter().any(|s| s.chars().count() > 1), "{labels:?}");
    check(&s, "optical-ligature-range");
}
#[test]
fn svg_optical_mixed_em_sizes_vertical_and_curved_ranges_stay_editable() {
    for (name, body) in [
        (
            "optical-mixed",
            r#"<text x="25" y="80" font-family="MetricFace" font-size="40"><tspan>AV</tspan><tspan font-size="20">WaTo</tspan></text>"#,
        ),
        (
            "optical-vertical",
            r#"<text x="120" y="15" font-family="MetricFace" font-size="40" writing-mode="vertical-rl">AVWA</text>"#,
        ),
        (
            "optical-path",
            r##"<defs><path id="guide" d="M20 110 Q190 10 390 100"/></defs><text font-family="MetricFace" font-size="32"><textPath href="#guide">AVWaTo</textPath></text>"##,
        ),
    ] {
        let mut s = studio(body);
        set(&mut s, KernMode::None);
        let before = pixels(&s);
        let ids = glyphs(run(&s));
        set(&mut s, KernMode::Optical);
        assert_ne!(pixels(&s), before, "{name}");
        assert_eq!(glyphs(run(&s)), ids, "{name}: shaping stays intact");
        check(&s, name);
    }
}
#[test]
fn svg_optical_rtl_keeps_the_first_logical_anchor_and_cursive_connections() {
    use base64::Engine;
    for (name, font, text) in [
        (
            "optical-rtl",
            "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
            "אבג דה",
        ),
        (
            "optical-cursive",
            "/usr/share/fonts/noto/NotoSansArabic-Regular.ttf",
            "ببب سلام",
        ),
    ] {
        let bytes = std::fs::read(font).unwrap();
        let css = format!(
            "@font-face{{font-family:Other;src:url(data:font/ttf;base64,{})}}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        let svg = source(&format!(
            r#"<style>{css}</style><text x="350" y="80" font-family="Other" font-size="40" direction="rtl">{text}</text>"#
        ));
        let mut s = super::metric_tests::studio_for(&svg);
        set(&mut s, KernMode::None);
        let anchor = crate::text::caret_pt(run(&s), 0);
        let before = pixels(&s);
        let ids = glyphs(run(&s));
        set(&mut s, KernMode::Optical);
        assert_eq!(glyphs(run(&s)), ids);
        let after = crate::text::caret_pt(run(&s), 0);
        assert!((after - anchor).length() < 0.001);
        if name == "optical-cursive" {
            assert_eq!(pixels(&s), before);
        } else {
            assert_ne!(pixels(&s), before);
        }
        check(&s, name);
    }
}
#[test]
fn svg_optical_positioning_refuses_corrupt_metadata_without_mutating_artwork() {
    let mut s =
        studio(r#"<text x="25" y="80" font-family="MetricFace" font-size="40">AVWaTo</text>"#);
    set(&mut s, KernMode::Optical);
    let svg = crate::svg::export_embedded(&s.doc).unwrap();
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let attr = xml
        .descendants()
        .flat_map(|n| n.attributes())
        .find(|a| a.name() == positioning::META)
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(attr.value()).unwrap();
    let before = crate::project::encode(&s.doc).unwrap();
    for (key, bad) in [
        ("version", serde_json::json!(2)),
        ("length", serde_json::json!(0)),
        ("dx", serde_json::json!([])),
        ("optical", serde_json::json!([[0, 99]])),
    ] {
        let mut v = value.clone();
        v[key] = bad;
        let mut source = svg.clone();
        source.replace_range(attr.range_value(), &escape(&v.to_string()));
        assert!(
            crate::formats::svg::read(&source, "Malformed optical").is_err(),
            "{key}"
        );
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    }
}

#[test]
fn svg_optical_participates_in_flow_wrap_without_losing_source_or_leading() {
    let source = super::pair_tests::flow_source();
    let mut s = super::metric_tests::studio_for(&source);
    set(&mut s, KernMode::None);
    let before = pixels(&s);
    let source_root = run(&s).svg_text.clone();
    set(&mut s, KernMode::Optical);
    assert_ne!(pixels(&s), before);
    assert_eq!(run(&s).svg_text, source_root);
    check(&s, "optical-flow");
}
