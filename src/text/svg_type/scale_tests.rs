use super::*;
use crate::{app::Studio, geom::Geom};

fn studio(body: &str) -> Studio {
    metric_tests::studio_for(&pair_tests::source(body))
}
fn run(studio: &Studio) -> &TypeRun {
    pair_tests::run(studio)
}
fn png(studio: &Studio) -> Vec<u8> {
    crate::compositor::export_png(&studio.doc, 1).unwrap()
}
fn axes(g: &GlyphMetric) -> [f32; 2] {
    [
        g.transform.sx.hypot(g.transform.ky),
        g.transform.kx.hypot(g.transform.sy),
    ]
}
fn scaled(studio: &mut Studio, h: f32, v: f32) {
    studio.patch_character(|s| {
        s.hscale = Some(h);
        s.vscale = Some(v);
    });
}

pub(crate) fn check(studio: &Studio, name: &str) {
    let pixels = png(studio);
    let saved = crate::project::encode(&studio.doc).unwrap();
    let reopened = crate::project::decode(&saved).unwrap();
    assert_eq!(pixels, crate::compositor::export_png(&reopened, 1).unwrap());
    let exported = crate::svg::export_embedded(&studio.doc).unwrap();
    let (again, notes) = crate::formats::svg::read(&exported, "Character scale reopen").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(
        pixels,
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
        let before = run(studio).spans.iter().find(|s| s.start <= i && i < s.end);
        if let Some(before) = before {
            let got = after.character_style(i);
            if before.hscale.is_some() {
                assert_eq!(got.hscale, before.hscale, "{name} {i}");
            }
            if before.vscale.is_some() {
                assert_eq!(got.vscale, before.vscale, "{name} {i}");
            }
        }
    }
    if let Some(root) = std::env::var_os("OMADESIGN_SVG_METRIC_QA") {
        let out = std::path::Path::new(&root).join(name);
        std::fs::create_dir_all(&out).unwrap();
        for (file, value) in [
            ("editable.svg", exported.into_bytes()),
            ("native.png", pixels),
            ("source.oma", saved.into_bytes()),
        ] {
            std::fs::write(out.join(file), value).unwrap();
        }
        let positions: Vec<_> = (0..run(studio).content.chars().count())
            .map(|i| {
                let p = crate::text::caret_pt(run(studio), i);
                [p.x, p.y]
            })
            .collect();
        std::fs::write(
            out.join("positions.json"),
            serde_json::to_vec(&positions).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn svg_character_scale_changes_actual_glyph_axes_without_changing_font_size_or_source() {
    for (name, h, v) in [
        ("scale-horizontal", 150., 100.),
        ("scale-vertical", 100., 175.),
        ("scale-both", 125., 80.),
    ] {
        let mut s = studio(
            r#"<text x="25" y="80" font-family="MetricFace" font-size="40" style="font-kerning:none;font-feature-settings:'liga' 0">AVWaTo</text>"#,
        );
        let before = crate::project::encode(&s.doc).unwrap();
        let source = run(&s).svg_text.clone();
        let base = layout(run(&s)).unwrap();
        let pixels = png(&s);
        scaled(&mut s, h, v);
        assert_ne!(png(&s), pixels);
        assert_eq!(run(&s).svg_text, source);
        assert_eq!(s.history.len(), 1);
        let after = layout(run(&s)).unwrap();
        assert_eq!(base.glyphs.len(), after.glyphs.len());
        for (a, b) in base.glyphs.iter().zip(&after.glyphs) {
            assert_eq!((a.index, a.id, &a.font), (b.index, b.id, &b.font));
            let old = axes(a);
            let new = axes(b);
            assert!(
                (new[0] / old[0] - h / 100.).abs() < 0.001,
                "{name} {:?} {:?}",
                old,
                new
            );
            assert!(
                (new[1] / old[1] - v / 100.).abs() < 0.001,
                "{name} {:?} {:?}",
                old,
                new
            );
            assert!((b.px - 40.).abs() < 0.001, "{name} font size {}", b.px);
        }
        assert!(
            s.selected_character_metrics()
                .iter()
                .all(|m| m.px == Some(40.) && m.hscale == Some(h) && m.vscale == Some(v))
        );
        check(&s, name);
        let changed = crate::project::encode(&s.doc).unwrap();
        s.undo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
        s.redo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), changed);
    }
}

#[test]
fn svg_character_scale_range_keeps_untouched_ligatures_and_other_metrics() {
    let mut s = studio(
        r#"<text x="25" y="80" font-family="MetricFace" font-size="40" letter-spacing="4">fi office AV</text>"#,
    );
    let before = crate::project::encode(&s.doc).unwrap();
    s.patch_type(|r| {
        r.set_character_style(1, 2, |s| {
            s.hscale = Some(140.);
            s.vscale = Some(75.);
        })
    });
    let current = layout(run(&s)).unwrap();
    let mut nodes = Vec::new();
    texts(current.tree.root(), &mut nodes);
    let labels: Vec<_> = nodes
        .iter()
        .flat_map(|t| t.layouted())
        .flat_map(|s| &s.positioned_glyphs)
        .map(|g| g.text.as_str())
        .collect();
    assert!(labels.contains(&"f") && labels.contains(&"i"), "{labels:?}");
    assert!(labels.iter().any(|s| s.chars().count() > 1), "{labels:?}");
    assert!(
        s.selected_character_metrics()
            .iter()
            .all(|m| (m.tracking.unwrap() - 100.).abs() < 0.001 && m.px == Some(40.))
    );
    check(&s, "scale-ligature-range");
    let changed = crate::project::encode(&s.doc).unwrap();
    assert_eq!(s.history.len(), 1);
    s.undo();
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    s.redo();
    assert_eq!(crate::project::encode(&s.doc).unwrap(), changed);
}

fn other(body: &str) -> String {
    use base64::Engine;
    let bytes = std::fs::read("/usr/share/fonts/liberation/LiberationSans-Regular.ttf").unwrap();
    format!(
        "<style>@font-face{{font-family:Other;src:url(data:font/ttf;base64,{})}}</style>{body}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

fn scale_cases() -> Vec<(&'static str, String)> {
    vec![
        ("scale-rotation-paragraph",r#"<text x="25" y="80" rotate="0 20 -15 10 30 -25" font-family="MetricFace" font-size="32" style="font-feature-settings:'liga' 0">AVWaTo</text>"#.to_string()),
        ("scale-rotation-offsets",r#"<text x="25" y="80" rotate="0 20 -15 10 30 -25" dx="0 -18 4 -16 2 -8" letter-spacing="-2" font-family="MetricFace" font-size="32" style="font-feature-settings:'liga' 0">AVWaTo</text>"#.to_string()),
        ("scale-rotation-rtl",other(r#"<text x="350" y="80" rotate="0 20 -15 10 30 -25" font-family="Other" font-size="32" direction="rtl" letter-spacing="2">אבג דה</text>"#)),
        ("scale-mixed",r#"<text x="25" y="80" font-family="MetricFace" font-size="40"><tspan>AV</tspan><tspan font-size="20">WaTo</tspan></text>"#.to_string()),
        ("scale-length",r#"<text x="25" y="80" font-family="MetricFace" font-size="40" textLength="220" lengthAdjust="spacingAndGlyphs"><tspan>AV</tspan><tspan font-size="20" textLength="50" lengthAdjust="spacingAndGlyphs">WaTo</tspan> AV</text>"#.to_string()),
        ("scale-anchors",r#"<text x="25 70 110 155" y="50 80 60 90" rotate="0 20 -15 10" font-family="MetricFace" font-size="32">AVWA</text>"#.to_string()),
        ("scale-vertical-latin",r#"<text x="120" y="15" font-family="MetricFace" font-size="32" writing-mode="vertical-rl">AVWA</text>"#.to_string()),
        ("scale-upright",other(r#"<text x="120" y="15" font-family="Other" font-size="32" writing-mode="vertical-rl">○A●V</text>"#)),
        ("scale-path",r##"<defs><path id="guide" d="M20 110 Q190 10 390 100"/></defs><text font-family="MetricFace" font-size="32"><textPath href="#guide">AVWaTo</textPath></text>"##.to_string()),
        ("scale-rtl",other(r#"<text x="350" y="80" font-family="Other" font-size="40" direction="rtl">אבג דה</text>"#)),

]
}
fn geometry_case(name: &str) {
    let body = scale_cases()
        .into_iter()
        .find(|(case, _)| *case == name)
        .unwrap()
        .1;
    let mut s = studio(&body);
    let before = png(&s);
    let original = run(&s).svg_text.clone();
    let base = layout(run(&s)).unwrap();
    scaled(&mut s, 125., 80.);
    assert_eq!(run(&s).svg_text, original);
    assert_ne!(png(&s), before, "{name}");
    let now = layout(run(&s)).unwrap();
    assert_eq!(
        base.glyphs
            .iter()
            .map(|g| (g.index, g.id, &g.font))
            .collect::<Vec<_>>(),
        now.glyphs
            .iter()
            .map(|g| (g.index, g.id, &g.font))
            .collect::<Vec<_>>(),
        "{name}"
    );
    diagnostic(&s, name);
    for g in &now.glyphs {
        assert_ne!(g.id, 0, "{name}: missing fixture glyph");
        let a = axes(g);
        assert!(
            (a[0] / g.px - 1.25).abs() < 0.002,
            "{name} x {:?} px{}",
            a,
            g.px
        );
        assert!(
            (a[1] / g.px - 0.8).abs() < 0.002,
            "{name} y {:?} px{}",
            a,
            g.px
        );
    }
    check(&s, name);
}
#[test]
fn svg_character_scale_mixed_keeps_real_glyph_axes() {
    geometry_case("scale-mixed");
}
#[test]
fn svg_character_scale_nested_lengths_keeps_real_glyph_axes() {
    geometry_case("scale-length");
}
#[test]
fn svg_character_scale_fixed_anchors_and_rotations_keeps_real_glyph_axes() {
    geometry_case("scale-anchors");
}
#[test]
fn svg_character_scale_vertical_latin_keeps_real_glyph_axes() {
    geometry_case("scale-vertical-latin");
}
#[test]
fn svg_character_scale_vertical_upright_symbols_keeps_real_glyph_axes() {
    geometry_case("scale-upright");
}
#[test]
fn svg_character_scale_curved_path_keeps_real_glyph_axes() {
    geometry_case("scale-path");
}
#[test]
fn svg_character_scale_rtl_keeps_real_glyph_axes() {
    geometry_case("scale-rtl");
}

#[test]
fn svg_character_scales_reflow_using_actual_scaled_widths_and_reopen_the_same_story() {
    let mut s = metric_tests::studio_for(&pair_tests::flow_source());
    let before = png(&s);
    let source = run(&s).svg_text.clone();
    scaled(&mut s, 135., 85.);
    assert_ne!(png(&s), before);
    assert_eq!(run(&s).svg_text, source);
    check(&s, "scale-flow");
}

#[test]
fn svg_character_scales_refuse_malformed_metadata_before_replacing_artwork() {
    let mut s =
        studio(r#"<text x="25" y="80" font-family="MetricFace" font-size="40">AVWaTo</text>"#);
    scaled(&mut s, 125., 80.);
    let exported = crate::svg::export_embedded(&s.doc).unwrap();
    let xml = roxmltree::Document::parse(&exported).unwrap();
    let attr = xml
        .descendants()
        .flat_map(|n| n.attributes())
        .find(|a| a.name() == scaling::META)
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(attr.value()).unwrap();
    let before = crate::project::encode(&s.doc).unwrap();
    for bad in [
        serde_json::json!({"version":2,"length":6,"edits":[]}),
        serde_json::json!({"version":1,"length":0,"edits":[]}),
        {
            let mut v = value.clone();
            v["edits"][0]["font_factor"] = serde_json::json!(0);
            v
        },
        {
            let mut v = value.clone();
            v["edits"][0]["before"]["onload"] = serde_json::json!("bad");
            v
        },
    ] {
        let mut source = exported.clone();
        source.replace_range(attr.range_value(), &escape(&bad.to_string()));
        assert!(crate::formats::svg::read(&source, "Invalid character scales").is_err());
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    }
}

fn diagnostic(studio: &Studio, name: &str) {
    if let Some(root) = std::env::var_os("OMADESIGN_SVG_METRIC_QA") {
        let out = std::path::Path::new(&root).join(name);
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(
            out.join("unqualified.svg"),
            crate::svg::export_embedded(&studio.doc).unwrap(),
        )
        .unwrap();
        std::fs::write(out.join("unqualified.png"), png(studio)).unwrap();
        std::fs::write(
            out.join("unqualified.oma"),
            crate::project::encode(&studio.doc).unwrap(),
        )
        .unwrap();
        let l = layout(run(studio)).unwrap();
        let glyphs:Vec<_>=l.glyphs.iter().map(|g|serde_json::json!({"index":g.index,"glyph":g.id,"font":g.font,"px":g.px,"upright":g.upright,"length_scale":g.length_scale,"axes":axes(g),"matrix":[g.transform.sx,g.transform.ky,g.transform.kx,g.transform.sy,g.transform.tx,g.transform.ty],"baseline":[g.baseline.x,g.baseline.y]})).collect();
        std::fs::write(
            out.join("unqualified-glyphs.json"),
            serde_json::to_vec_pretty(&glyphs).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn svg_character_scale_editable_cycles_keep_the_same_pixels_without_growing_wrapper_depth() {
    let mut s =
        studio(r#"<text x="25" y="80" font-family="MetricFace" font-size="40">AVWaTo</text>"#);
    s.patch_character(|c| {
        c.hscale = Some(125.);
        c.vscale = Some(80.);
        c.kerning = Some(crate::geom::KernMode::Optical);
        c.tracking = Some(100.);
        c.baseline_shift = Some(3.);
    });
    let pixels = png(&s);
    let mut doc = s.doc.clone();
    let mut normalized = None;
    fn depth(n: &TextNode) -> usize {
        match n {
            TextNode::Characters { .. } => 0,
            TextNode::Element { children, .. } => 1 + children.iter().map(depth).max().unwrap_or(0),
        }
    }
    for _ in 0..4 {
        let svg = crate::svg::export_embedded(&doc).unwrap();
        let (again, notes) = crate::formats::svg::read(&svg, "Repeated scale roundtrip").unwrap();
        assert!(notes.is_empty());
        assert_eq!(pixels, crate::compositor::export_png(&again, 1).unwrap());
        let r = again
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
        let d = depth(&r.svg_text.as_ref().unwrap().root);
        if let Some(n) = normalized {
            assert!(d <= n, "wrapper depth {d} > {n}");
        } else {
            normalized = Some(d);
        }
        doc = again;
    }
}

#[test]
fn svg_character_scale_keeps_loaded_values_and_uses_the_existing_native_paint_limits() {
    for h in [0.5, 1500.] {
        let mut s =
            studio(r#"<text x="25" y="80" font-family="MetricFace" font-size="40">AV</text>"#);
        scaled(&mut s, h, 80.);
        let current = layout(run(&s)).unwrap();
        for g in &current.glyphs {
            assert!((axes(g)[0] / g.px - h.clamp(1., 1000.) / 100.).abs() < 0.002);
        }
        let exported = crate::svg::export_embedded(&s.doc).unwrap();
        let (doc, notes) = crate::formats::svg::read(&exported, "Loaded scale bounds").unwrap();
        assert!(notes.is_empty());
        let r = doc
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
        assert_eq!(r.character_style(0).hscale, Some(h));
        assert_eq!(png(&s), crate::compositor::export_png(&doc, 1).unwrap());
    }
}

#[test]
fn svg_character_scale_rotated_paragraph_keeps_shaping_context_and_local_axes() {
    geometry_case("scale-rotation-paragraph");
}
#[test]
fn svg_character_scale_rotated_negative_offsets_keep_shaping_and_local_axes() {
    geometry_case("scale-rotation-offsets");
}
#[test]
fn svg_character_scale_rotated_rtl_keeps_shaping_and_local_axes() {
    geometry_case("scale-rotation-rtl");
}
#[test]
fn svg_character_scale_guides_do_not_expose_intrinsic_distances_as_pair_edits() {
    let body = scale_cases()
        .into_iter()
        .find(|(name, _)| *name == "scale-rotation-offsets")
        .unwrap()
        .1;
    let mut s = studio(&body);
    let original: (Vec<_>, Vec<_>) = (
        (0..6)
            .map(|i| positioning::pair_kerning(run(&s), i))
            .collect(),
        (0..6)
            .map(|i| run(&s).character_style(i).tracking)
            .collect(),
    );
    scaled(&mut s, 125., 80.);
    for i in 0..6 {
        assert!(
            (positioning::pair_kerning(run(&s), i) - original.0[i]).abs() < 0.001,
            "pair{i}"
        );
        assert_eq!(run(&s).character_style(i).tracking, original.1[i]);
    }
}
#[test]
fn svg_character_scale_guides_refuse_stale_unicode_and_bad_source_metadata() {
    let body = scale_cases()
        .into_iter()
        .find(|(name, _)| *name == "scale-rotation-paragraph")
        .unwrap()
        .1;
    let mut s = studio(&body);
    scaled(&mut s, 125., 80.);
    let source = crate::svg::export_embedded(&s.doc).unwrap();
    let xml = roxmltree::Document::parse(&source).unwrap();
    let attr = xml
        .descendants()
        .flat_map(|n| n.attributes())
        .find(|a| a.name() == guides::META)
        .unwrap();
    let meta: serde_json::Value = serde_json::from_str(attr.value()).unwrap();
    let before = crate::project::encode(&s.doc).unwrap();
    for bad in [
        {
            let mut v = meta.clone();
            v["version"] = serde_json::json!(2);
            v
        },
        {
            let mut v = meta.clone();
            v["content"] = serde_json::json!("stale!");
            v
        },
        {
            let mut v = meta.clone();
            v["positions"][0]["index"] = serde_json::json!(99999);
            v
        },
    ] {
        let mut changed = source.clone();
        changed.replace_range(attr.range_value(), &escape(&bad.to_string()));
        assert!(crate::formats::svg::read(&changed, "Invalid guide snapshot").is_err());
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    }
}

#[test]
fn svg_character_scale_guides_bake_spacing_without_stretching_synthetic_jumps() {
    for name in [
        "scale-rotation-offsets",
        "scale-rotation-rtl",
        "scale-upright",
    ] {
        let body = scale_cases()
            .into_iter()
            .find(|(n, _)| *n == name)
            .unwrap()
            .1;
        let mut s = studio(&body);
        scaled(&mut s, 125., 80.);
        let rendered = source(run(&s)).unwrap();
        let xml = roxmltree::Document::parse(&rendered).unwrap();
        assert!(
            xml.descendants().any(|n| n.has_tag_name("textPath")),
            "{name}"
        );
        for node in xml.descendants().filter(|n| n.is_element()) {
            for property in ["dx", "dy", "letter-spacing", "word-spacing"] {
                if let Some(value) = node.attribute(property) {
                    assert_eq!(value, "0", "{name}: {property}");
                }
            }
        }
        for g in layout(run(&s)).unwrap().glyphs {
            assert!((g.px - 32.).abs() < 0.001, "{name}: {}", g.px);
        }
    }
}
