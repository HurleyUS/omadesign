use super::*;
use crate::app::Studio;

/// Import selectable text with real authored directional controls.
/// Args: text body. Returns a selected imported text object using the embedded fixture font.
pub(crate) fn fixture(body: &str) -> Studio {
    use base64::Engine;
    let font = std::fs::read("/usr/share/fonts/liberation/LiberationSans-Regular.ttf").unwrap();
    let source = pair_tests::source(&format!(
        "<style>@font-face{{font-family:BidiFace;src:url(data:font/ttf;base64,{})}}</style>{body}",
        base64::engine::general_purpose::STANDARD.encode(font)
    ));
    metric_tests::studio_for(&source)
}

#[test]
fn svg_authored_bidi_controls_keep_original_character_slots() {
    for (direction, content, indices) in [
        ("ltr", "A\u{202b}אב\u{202c}V", vec![0, 2, 3, 5]),
        ("ltr", "A\u{2067}אב\u{2069}V", vec![0, 2, 3, 5]),
        ("rtl", "A\u{202b}אב\u{202c}V", vec![0, 2, 3, 5]),
        ("rtl", "\u{202b}אב\u{202c}A", vec![1, 2, 4]),
    ] {
        let studio = fixture(&format!(
            "<text x=\"25 65 105 145 185 225\" y=\"80\" font-family=\"BidiFace\" font-size=\"32\" direction=\"{direction}\">{content}</text>"
        ));
        let run = pair_tests::run(&studio);
        assert_eq!(run.content, content);
        let layout = layout(run).unwrap();
        let mut actual: Vec<_> = layout.glyphs.iter().map(|g| g.index).collect();
        actual.sort_unstable();
        assert_eq!(actual, indices, "{direction}: {content:?}");
        assert_eq!(layout.addressable_count, content.chars().count());
        assert!(layout.glyphs.iter().all(|g| g.id != 0));
    }
}

#[test]
fn svg_runtime_bidi_insertion_map_distinguishes_identical_authored_prefixes_and_entities() {
    let original = "<svg xmlns=\"http://www.w3.org/2000/svg\"><text direction=\"rtl\" x=\"25\" y=\"80\">\u{202b}אב\u{202c}A</text><text direction=\"ltr\" unicode-bidi=\"isolate\"><tspan direction=\"rtl\" x=\"25\">A&amp;B</tspan></text></svg>";
    let (runtime, inserted) = direction::indexed(original);
    assert_eq!(runtime, renderer_source(original));
    assert_eq!(inserted, vec![vec![0, 6], vec![0, 1, 5, 6]]);
    let xml = roxmltree::Document::parse(&runtime).unwrap();
    let text = xml.descendants().find(|n| n.has_tag_name("text")).unwrap();
    let value: String = text
        .descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect();
    assert_eq!(value, "\u{202b}\u{202b}אב\u{202c}A\u{202c}");
}

/// Check a selected source letter's actual baseline change and all untouched glyphs.
/// Args: original and edited runs. Returns no value; requires only logical character two to move.
pub(crate) fn baseline(before: &TypeRun, after: &TypeRun) {
    assert_eq!(before.content, after.content);
    assert_eq!(before.svg_text, after.svg_text);
    let a = layout(before).unwrap();
    let b = layout(after).unwrap();
    assert_eq!(a.glyphs.len(), b.glyphs.len());
    for g in &a.glyphs {
        let changed = b
            .glyphs
            .iter()
            .find(|other| other.index == g.index)
            .unwrap();
        assert_eq!((g.id, &g.font), (changed.id, &changed.font));
        assert_eq!(g.transform.sx, changed.transform.sx);
        assert_eq!(g.transform.kx, changed.transform.kx);
        assert_eq!(g.transform.ky, changed.transform.ky);
        assert_eq!(g.transform.sy, changed.transform.sy);
        assert!((changed.transform.tx - g.transform.tx).abs() < 0.001);
        let expected = if g.index == 2 { -12. } else { 0. };
        assert!(
            (changed.transform.ty - g.transform.ty - expected).abs() < 0.001,
            "{}: {:?} -> {:?}",
            g.index,
            g.transform,
            changed.transform
        );
    }
}

#[test]
fn svg_authored_bidi_range_baseline_targets_only_the_original_hebrew_letter() {
    for (name, direction, content) in [
        ("bidi-embedding-baseline", "ltr", "A\u{202b}אב\u{202c}V"),
        ("bidi-isolate-baseline", "ltr", "A\u{2067}אב\u{2069}V"),
        ("bidi-rtl-baseline", "rtl", "A\u{202b}אב\u{202c}V"),
    ] {
        let mut studio = fixture(&format!(
            "<text x=\"25 65 105 145 185 225\" y=\"80\" font-family=\"BidiFace\" font-size=\"32\" direction=\"{direction}\">{content}</text>"
        ));
        let original = pair_tests::run(&studio).clone();
        let before = crate::project::encode(&studio.doc).unwrap();
        let pixels = crate::compositor::export_png(&studio.doc, 1).unwrap();
        studio
            .patch_type(|r| r.set_character_style(2, 3, |style| style.baseline_shift = Some(12.)));
        baseline(&original, pair_tests::run(&studio));
        assert_ne!(
            pixels,
            crate::compositor::export_png(&studio.doc, 1).unwrap()
        );
        scale_tests::check(&studio, name);
        let after = crate::project::encode(&studio.doc).unwrap();
        assert_eq!(studio.history.len(), 1);
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        studio.redo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    }
}

#[test]
fn svg_authored_bidi_pair_preserves_control_offsets_and_trailing_character_identity() {
    let mut studio = fixture(
        "<text x=\"25\" y=\"80\" dx=\"0 7 0 0 -3 0\" font-family=\"BidiFace\" font-size=\"32\">A\u{202b}אב\u{202c}V</text>",
    );
    let before = crate::project::encode(&studio.doc).unwrap();
    let original = pair_tests::run(&studio).svg_text.clone();
    let old = layout(pair_tests::run(&studio)).unwrap();
    studio.patch_type(|r| {
        r.manual_kern.insert(5, 125.);
    });
    let run = pair_tests::run(&studio);
    assert_eq!(run.svg_text, original);
    assert_eq!(pair_kerning(run, 5), 125.);
    let new = layout(run).unwrap();
    for g in &old.glyphs {
        let changed = new
            .glyphs
            .iter()
            .find(|other| other.index == g.index)
            .unwrap();
        assert!(
            (changed.transform.tx - g.transform.tx - if g.index == 5 { 4. } else { 0. }).abs()
                < 0.001,
            "{}: {:?} -> {:?}",
            g.index,
            g.transform,
            changed.transform
        );
        assert_eq!(g.transform.ty, changed.transform.ty);
    }
    let emitted = source(run).unwrap();
    let xml = roxmltree::Document::parse(&emitted).unwrap();
    assert_eq!(
        xml.descendants()
            .find(|n| n.has_tag_name("text"))
            .unwrap()
            .attribute("dx"),
        Some("0 7 0 0 -3 4")
    );
    scale_tests::check(&studio, "bidi-pair-offsets");
    let after = crate::project::encode(&studio.doc).unwrap();
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
}

fn scale_case(name: &str, placement: &str, content: &str) {
    let mut studio = fixture(&format!(
        "<text {placement} font-family=\"BidiFace\" font-size=\"32\">{content}</text>"
    ));
    let before = crate::project::encode(&studio.doc).unwrap();
    let original = pair_tests::run(&studio).svg_text.clone();
    let old = layout(pair_tests::run(&studio)).unwrap();
    assert!(old.glyphs.iter().any(|g| g.index == 2));
    studio.patch_type(|r| {
        r.set_character_style(2, 3, |s| {
            s.hscale = Some(125.);
            s.vscale = Some(80.);
        })
    });
    let run = pair_tests::run(&studio);
    assert_eq!(run.content, content);
    assert_eq!(run.svg_text, original);
    let new = layout(run).unwrap();
    let identity = |layout: &Layout| {
        let mut values: Vec<_> = layout.glyphs.iter().map(|g| (g.index, g.id)).collect();
        values.sort_unstable();
        values
    };
    assert_eq!(identity(&old), identity(&new));
    let g = new.glyphs.iter().find(|g| g.index == 2).unwrap();
    assert!((g.transform.sx.hypot(g.transform.ky) / g.px - 1.25).abs() < 0.002);
    assert!((g.transform.kx.hypot(g.transform.sy) / g.px - 0.8).abs() < 0.002);
    scale_tests::check(&studio, name);
    let after = crate::project::encode(&studio.doc).unwrap();
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
}

#[test]
fn svg_authored_bidi_scaled_rotation_keeps_original_logical_targets() {
    scale_case(
        "bidi-scaled-rotation",
        r#"x="25" y="80" rotate="15""#,
        "A\u{202b}אב\u{202c}V",
    );
}

#[test]
fn svg_authored_bidi_scaled_flow_keeps_original_logical_targets_after_wrapping() {
    scale_case(
        "bidi-scaled-flow",
        r#"xml:space="preserve" x="25" y="35" style="inline-size:160px;line-height:40px""#,
        "A\u{202b}אב\u{202c}V A\u{2067}אב\u{2069}V AV AV AV AV AV",
    );
}
