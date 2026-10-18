use super::*;
use crate::{
    app::Studio,
    geom::{Geom, Leading},
};

pub(crate) fn source(body: &str) -> String {
    let source = super::metric_tests::source();
    let start = source.find("<text ").unwrap();
    let end = source.rfind("</text>").unwrap() + "</text>".len();
    format!("{}{body}{}", &source[..start], &source[end..])
}

pub(crate) fn run(studio: &Studio) -> &TypeRun {
    let (layer, id) = studio.selection[0];
    let Geom::Text(run) = &studio.doc.find_shape(layer, id).unwrap().geom else {
        panic!()
    };
    run
}

fn edit_pair(source: &str, at: usize, value: f32, expected: &str, name: &str) -> Studio {
    let mut studio = super::metric_tests::studio_for(source);
    let before = crate::project::encode(&studio.doc).unwrap();
    let captured = run(&studio).svg_text.clone();
    studio.patch_type(|run| {
        run.manual_kern.insert(at, value);
    });
    assert_eq!(
        run(&studio).svg_text,
        captured,
        "Source text stays authoritative"
    );
    assert_eq!(pair_kerning(run(&studio), at), value);
    super::metric_tests::compare(&studio, expected, name);
    let after = crate::project::encode(&studio.doc).unwrap();
    assert_eq!(studio.history.len(), 1);
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    studio
}

#[test]
fn svg_pair_offsets_replace_inherited_values_in_each_resolved_em_and_clear_to_zero() {
    let source = source(
        r#"<text x="25" y="80" font-family="MetricFace" font-size="40" style="font-kerning:none;font-feature-settings:'liga' 0"><tspan dx="3 7">AV</tspan><tspan font-size="20" dx="5"> next</tspan></text>"#,
    );
    let before = super::metric_tests::studio_for(&source);
    assert!((pair_kerning(run(&before), 1) - 175.).abs() < 0.001);
    let expected = source.replace("dx=\"3 7\"", "dx=\"3 -4\"");
    edit_pair(&source, 1, -100., &expected, "pair-mixed");
    let expected = source.replace("dx=\"3 7\"", "dx=\"3 0\"");
    edit_pair(&source, 1, 0., &expected, "pair-zero");
    let expected = source.replace("dx=\"5\"", "dx=\"2\"");
    edit_pair(&source, 2, 100., &expected, "pair-small-em");
}

#[test]
fn svg_pair_inside_ligature_splits_only_affected_characters_and_retains_unicode() {
    let source = source(
        r#"<text x="25" y="80" font-family="MetricFace" font-size="40">fi office AV</text>"#,
    );
    let expected = source.replace("font-size=\"40\"", "font-size=\"40\" dx=\"0 6\"");
    let studio = edit_pair(&source, 1, 150., &expected, "pair-ligature");
    let layout = layout(run(&studio)).unwrap();
    let mut nodes = Vec::new();
    texts(layout.tree.root(), &mut nodes);
    let glyphs: Vec<_> = nodes
        .iter()
        .flat_map(|t| t.layouted())
        .flat_map(|s| &s.positioned_glyphs)
        .map(|g| g.text.as_str())
        .collect();
    assert!(glyphs.contains(&"f") && glyphs.contains(&"i"), "{glyphs:?}");
    assert!(
        glyphs.iter().any(|g| g.chars().count() > 1),
        "Unedited office ligature stays shaped: {glyphs:?}"
    );
    assert_eq!(run(&studio).content, "fi office AV");
}

#[test]
fn svg_pair_rtl_vertical_and_text_path_use_existing_inline_coordinates() {
    for (name, body, at, value, from, to) in [
        (
            "pair-rtl",
            r#"<text x="350" y="80" direction="rtl" font-family="MetricFace" font-size="40">אב ג AV</text>"#,
            1,
            -100.,
            "font-size=\"40\"",
            "font-size=\"40\" dx=\"0 -4\"",
        ),
        (
            "pair-vertical",
            r#"<text x="120" y="15" writing-mode="vertical-rl" font-family="MetricFace" font-size="40">AVAV</text>"#,
            1,
            100.,
            "font-size=\"40\"",
            "font-size=\"40\" dy=\"0 4\"",
        ),
        (
            "pair-path",
            r##"<defs><path id="curve" d="M20 100 Q180 20 390 100"/></defs><text font-family="MetricFace" font-size="40"><textPath href="#curve">AV office</textPath></text>"##,
            1,
            125.,
            "font-size=\"40\"",
            "font-size=\"40\" dx=\"0 5\"",
        ),
    ] {
        let source = source(body);
        let expected = source.replace(from, to);
        edit_pair(&source, at, value, &expected, name);
    }
}

pub(crate) fn flow_source() -> String {
    source(
        r#"<text xml:space="preserve" x="25" y="35" font-family="MetricFace" font-size="24" style="inline-size:160px;line-height:28px">AVAV AVAV AVAV AVAV AVAV AVAV AVAV</text>"#,
    )
}

#[test]
fn svg_flow_leading_uses_accepted_fixed_auto_and_range_values_with_exact_reopen() {
    for (name, mode, expected) in [
        ("flow-leading-fixed", Leading::Fixed(42.), "42px"),
        ("flow-leading-auto", Leading::Auto(150.), "150%"),
    ] {
        let source = flow_source();
        let mut studio = super::metric_tests::studio_for(&source);
        let before = crate::project::encode(&studio.doc).unwrap();
        assert_eq!(
            studio.selected_character_metrics()[0].leading,
            Some(Leading::Fixed(28.))
        );
        studio.patch_character(|s| s.leading = Some(mode));
        super::metric_tests::compare(&studio, &source.replace("28px", expected), name);
        let after = crate::project::encode(&studio.doc).unwrap();
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        studio.redo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    }
    let source = source(
        "<text xml:space=\"preserve\" x=\"25\" y=\"35\" font-family=\"MetricFace\" font-size=\"24\" style=\"inline-size:350px;line-height:28px\">First row\nSecond row\nThird row</text>",
    );
    let mut studio = super::metric_tests::studio_for(&source);
    studio.patch_type(|r| {
        r.set_character_style(0, 9, |s| s.leading = Some(Leading::Fixed(42.)));
        r.set_character_style(10, 20, |s| s.leading = Some(Leading::Fixed(58.)));
    });
    let a = crate::text::caret_pt(run(&studio), 0);
    let b = crate::text::caret_pt(run(&studio), 10);
    let c = crate::text::caret_pt(run(&studio), 21);
    assert!((b.y - a.y - 42.).abs() < 0.001, "{a:?} {b:?}");
    assert!((c.y - b.y - 58.).abs() < 0.001, "{b:?} {c:?}");
    let expected = self::source(
        "<text xml:space=\"preserve\" font-family=\"MetricFace\" font-size=\"24\"><tspan x=\"25\" y=\"35\">First row</tspan><tspan x=\"25\" y=\"77\">Second row</tspan><tspan x=\"25\" y=\"135\">Third row</tspan></text>",
    );
    super::metric_tests::compare(&studio, &expected, "flow-leading-ranges");
}

#[test]
fn svg_flow_pair_affects_wrap_and_retains_editable_pair_when_svg_reflows_again() {
    let source = flow_source();
    let mut studio = super::metric_tests::studio_for(&source);
    let before = layout(run(&studio)).unwrap();
    studio.patch_type(|r| {
        r.manual_kern.insert(6, 1800.);
    });
    let after = layout(run(&studio)).unwrap();
    assert_ne!(before.carets, after.carets);
    let saved = crate::project::encode(&studio.doc).unwrap();
    let reopened = crate::project::decode(&saved).unwrap();
    assert_eq!(
        crate::compositor::export_png(&studio.doc, 1).unwrap(),
        crate::compositor::export_png(&reopened, 1).unwrap()
    );
    let exported = crate::svg::export_embedded(&studio.doc).unwrap();
    let again = super::metric_tests::studio_for(&exported);
    assert_eq!(run(&again).manual_kern.get(&6), Some(&1800.));
    assert_eq!(
        crate::compositor::export_png(&studio.doc, 1).unwrap(),
        crate::compositor::export_png(&again.doc, 1).unwrap()
    );
}
