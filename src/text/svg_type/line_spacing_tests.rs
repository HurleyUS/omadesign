use super::*;
use crate::{app::Studio, geom::Leading};

pub(crate) fn simple_source() -> String {
    pair_tests::source(
        r#"<text x="25" y="35" font-family="MetricFace" font-size="24">AB<tspan x="25" y="65">CD</tspan><tspan x="25" y="95">EF</tspan></text>"#,
    )
}

/// Compare actual baseline placement with separately authored standard SVG.
/// Args: edited native document, expected SVG and output name. Returns no value; checks native/editable roundtrips and retains independent inputs.
pub(crate) fn compare(studio: &Studio, expected: &str, name: &str) {
    metric_tests::compare(studio, expected, name);
    if let Some(root) = std::env::var_os("OMADESIGN_SVG_METRIC_QA") {
        let dir = std::path::Path::new(&root).join(name);
        std::fs::write(dir.join("expected.svg"), expected).unwrap();
    }
}

fn check(source: &str, expected: &str, mode: Leading, name: &str) -> Studio {
    let mut studio = metric_tests::studio_for(source);
    let before = crate::project::encode(&studio.doc).unwrap();
    let original = pair_tests::run(&studio).svg_text.clone();
    let pixels = crate::compositor::export_png(&studio.doc, 1).unwrap();
    studio.patch_character(|s| s.leading = Some(mode));
    assert_eq!(pair_tests::run(&studio).svg_text, original);
    assert!(
        studio
            .selected_character_metrics()
            .iter()
            .all(|s| s.leading == Some(mode))
    );
    assert_ne!(
        pixels,
        crate::compositor::export_png(&studio.doc, 1).unwrap()
    );
    compare(&studio, expected, name);
    let after = crate::project::encode(&studio.doc).unwrap();
    assert_eq!(studio.history.len(), 1);
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    studio
}

#[test]
fn svg_positioned_leading_moves_absolute_rows_without_turning_superscripts_into_lines() {
    let source = pair_tests::source(
        r#"<text x="25" y="35" font-family="MetricFace" font-size="24">AB<tspan x="25" y="65"><tspan font-size="32" baseline-shift="8">CD</tspan></tspan><tspan x="25" y="95">EF</tspan></text>"#,
    );
    let initial = metric_tests::studio_for(&source);
    assert!(
        initial
            .selected_character_metrics()
            .iter()
            .all(|s| s.leading == Some(Leading::Fixed(30.)))
    );
    let expected = source
        .replace("y=\"65\"", "y=\"85\"")
        .replace("y=\"95\"", "y=\"135\"");
    let studio = check(&source, &expected, Leading::Fixed(50.), "leading-absolute");
    let current = layout(pair_tests::run(&studio)).unwrap();
    for glyph in &current.glyphs {
        assert_eq!(
            glyph.authored.y,
            [35., 35., 85., 85., 135., 135.][glyph.index]
        );
    }
}

#[test]
fn svg_positioned_leading_preserves_relative_offsets_and_separate_columns() {
    let source = pair_tests::source(
        r#"<text x="25" y="35" font-family="MetricFace" font-size="24">AB<tspan x="25" dy="30">CD</tspan><tspan x="25" dy="30">EF</tspan><tspan x="200" y="35">GH</tspan><tspan x="200" dy="30">IJ</tspan></text>"#,
    );
    let expected = source.replace("dy=\"30\"", "dy=\"50\"");
    let studio = check(
        &source,
        &expected,
        Leading::Fixed(50.),
        "leading-relative-columns",
    );
    let current = layout(pair_tests::run(&studio)).unwrap();
    for glyph in &current.glyphs {
        assert_eq!(
            glyph.authored.y,
            [35., 35., 85., 85., 135., 135., 35., 35., 85., 85.][glyph.index]
        );
    }
}

#[test]
fn svg_positioned_leading_accounts_for_every_absolute_character_reset() {
    let source = pair_tests::source(
        r#"<text x="25 25 25" y="35 65 95" font-family="MetricFace" font-size="24">ABC</text>"#,
    );
    let expected = source.replace("35 65 95", "35 85 135");
    let studio = check(
        &source,
        &expected,
        Leading::Fixed(50.),
        "leading-absolute-slots",
    );
    for glyph in &layout(pair_tests::run(&studio)).unwrap().glyphs {
        assert_eq!(glyph.authored.y, [35., 85., 135.][glyph.index]);
    }
}

#[test]
fn svg_positioned_leading_moves_vertical_columns_in_the_authored_direction() {
    let source = pair_tests::source(
        r#"<text x="160" y="20" writing-mode="vertical-rl" font-family="MetricFace" font-size="24">AB<tspan x="120" y="20">CD</tspan><tspan x="80" y="20">EF</tspan></text>"#,
    );
    let expected = source
        .replace("x=\"120\"", "x=\"110\"")
        .replace("x=\"80\"", "x=\"60\"");
    let studio = check(&source, &expected, Leading::Fixed(50.), "leading-vertical");
    for glyph in &layout(pair_tests::run(&studio)).unwrap().glyphs {
        assert_eq!(
            glyph.authored.x,
            [160., 160., 110., 110., 60., 60.][glyph.index]
        );
    }
}

#[test]
fn svg_positioned_leading_auto_uses_row_fonts_and_range_edits_preserve_other_gaps() {
    let source = pair_tests::source(
        r#"<text x="25" y="40" font-family="MetricFace" font-size="20">A<tspan font-size="30" baseline-shift="10">B</tspan><tspan x="25" y="80">CD</tspan><tspan x="25" y="120">EF</tspan></text>"#,
    );
    let mut studio = metric_tests::studio_for(&source);
    let before = crate::project::encode(&studio.doc).unwrap();
    studio.patch_type(|r| r.set_character_style(0, 2, |s| s.leading = Some(Leading::Auto(200.))));
    let expected = source
        .replace("y=\"80\"", "y=\"100\"")
        .replace("y=\"120\"", "y=\"140\"");
    compare(&studio, &expected, "leading-auto-range");
    let auto = crate::project::encode(&studio.doc).unwrap();
    studio.patch_type(|r| r.set_character_style(2, 4, |s| s.leading = Some(Leading::Fixed(25.))));
    compare(
        &studio,
        &expected.replace("y=\"140\"", "y=\"125\""),
        "leading-second-range",
    );
    let after = crate::project::encode(&studio.doc).unwrap();
    assert_eq!(studio.history.len(), 2);
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), auto);
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.redo();
    studio.redo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
}

#[test]
fn svg_positioned_leading_keeps_single_baselines_and_real_text_paths_intact() {
    for (name, body) in [
        (
            "leading-single",
            r#"<text x="25" y="80" font-family="MetricFace" font-size="24" baseline-shift="5">ABCD</text>"#,
        ),
        (
            "leading-path",
            r##"<defs><path id="guide" d="M20 100 Q180 20 390 100"/></defs><text font-family="MetricFace" font-size="24"><textPath href="#guide" dx="0 5">ABCD</textPath></text>"##,
        ),
    ] {
        let source = pair_tests::source(body);
        let mut studio = metric_tests::studio_for(&source);
        let before = crate::compositor::export_png(&studio.doc, 1).unwrap();
        studio.patch_character(|s| s.leading = Some(Leading::Fixed(50.)));
        assert_eq!(
            before,
            crate::compositor::export_png(&studio.doc, 1).unwrap()
        );
        compare(&studio, &source, name);
    }
}

#[test]
fn svg_positioned_leading_and_authored_bidi_controls_keep_character_slots_and_actual_baselines() {
    let source = pair_tests::source(
        "<text x=\"25\" y=\"35\" font-family=\"MetricFace\" font-size=\"24\">A\u{202b}אב\u{202c}V<tspan x=\"25\" y=\"65\">CD</tspan><tspan x=\"25\" dy=\"30\">EF</tspan></text>",
    );
    let expected = source
        .replace("y=\"65\"", "y=\"85\"")
        .replace("dy=\"30\"", "dy=\"50\"");
    check(&source, &expected, Leading::Fixed(50.), "leading-bidi");
}

#[test]
fn svg_positioned_leading_scale_optical_and_editable_cycles_do_not_accumulate_offsets() {
    let source = simple_source();
    let mut studio = metric_tests::studio_for(&source);
    studio.patch_character(|s| {
        s.leading = Some(Leading::Fixed(42.));
        s.hscale = Some(125.);
        s.vscale = Some(80.);
        s.kerning = Some(crate::geom::KernMode::Optical);
    });
    let before = crate::compositor::export_png(&studio.doc, 1).unwrap();
    let story = pair_tests::run(&studio).content.clone();
    for _ in 0..4 {
        let exported = crate::svg::export_embedded(&studio.doc).unwrap();
        studio = metric_tests::studio_for(&exported);
        assert_eq!(pair_tests::run(&studio).content, story);
        assert!(
            studio
                .selected_character_metrics()
                .iter()
                .all(|s| s.leading == Some(Leading::Fixed(42.)))
        );
        assert_eq!(
            before,
            crate::compositor::export_png(&studio.doc, 1).unwrap()
        );
    }
    scale_tests::check(&studio, "leading-scaled-cycles");
}

#[test]
fn svg_positioned_leading_metadata_rejects_invalid_ranges_before_typed_loss() {
    let mut studio = metric_tests::studio_for(&simple_source());
    studio.patch_character(|s| s.leading = Some(Leading::Fixed(42.)));
    let source = source(pair_tests::run(&studio)).unwrap();
    let xml = roxmltree::Document::parse(&source).unwrap();
    let attribute = xml
        .descendants()
        .find_map(|n| n.attributes().find(|a| a.name() == positioning::META))
        .unwrap();
    let saved: serde_json::Value = serde_json::from_str(attribute.value()).unwrap();
    for leading in [
        serde_json::json!([[0,7,{"Fixed":42}]]),
        serde_json::json!([[1,1,{"Fixed":42}]]),
        serde_json::json!([[0,3,{"Fixed":-2}]]),
        serde_json::json!([[0,4,{"Fixed":42}],[3,6,{"Fixed":42}]]),
    ] {
        let mut value = saved.clone();
        value["leading"] = leading;
        let mut invalid = source.clone();
        invalid.replace_range(
            attribute.range_value(),
            &escape(&serde_json::to_string(&value).unwrap()),
        );
        let error = crate::formats::svg::read(&invalid, "Invalid line spacing")
            .err()
            .unwrap();
        assert!(error.contains("line spacing"), "{error}");
    }
}

fn cluster_range(story: &str, name: &str) {
    let source = pair_tests::source(&format!(
        "<text x=\"25\" y=\"35\" font-family=\"MetricFace\" font-size=\"24\">{story}<tspan x=\"25\" y=\"65\">AB</tspan></text>"
    ));
    let mut studio = metric_tests::studio_for(&source);
    let before = crate::project::encode(&studio.doc).unwrap();
    studio.patch_type(|r| r.set_character_style(1, 2, |s| s.leading = Some(Leading::Fixed(50.))));
    compare(&studio, &source.replace("y=\"65\"", "y=\"85\""), name);
    let after = crate::project::encode(&studio.doc).unwrap();
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
}

#[test]
fn svg_positioned_leading_range_inside_a_ligature_changes_its_actual_line() {
    cluster_range("fi", "leading-ligature-range");
}
#[test]
fn svg_positioned_leading_range_inside_a_combining_cluster_changes_its_actual_line() {
    cluster_range("A\u{301}", "leading-combining-range");
}
