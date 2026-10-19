use super::*;
use crate::{
    color::Rgba,
    document::{Document, Layer},
    geom::{ParagraphStyle, TextAlign},
};

fn editor(text: &str) -> (Studio, (usize, u64)) {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Layout;
    s.doc = Document::new("Type content", 500., 240., 96.);
    s.doc.workspace = Some(Persona::Layout);
    s.doc.layers = vec![Layer::vector("Editable text")];
    let mut run = TypeRun {
        content: text.into(),
        origin: Pt::new(35., 65.),
        px: 28.,
        font: concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/assets/fonts/EBGaramond.ttf"
        )
        .into(),
        ..Default::default()
    };
    run.set_character_style(0, text.chars().count().min(6), |span| {
        span.fill = Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
    });
    let mut shape = Shape::new(
        Geom::Text(run),
        Style {
            fill: Fill::Solid(Rgba::rgb(25, 110, 185)),
            stroke: None,
        },
    );
    shape.name = "Styled text".into();
    crate::text::fill_contours(&mut shape.geom);
    let hit = (0, shape.id);
    s.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    s.selection = vec![hit];
    s.active_layer = Some(0);
    s.mark();
    s.history.clear();
    (s, hit)
}
fn run(s: &Studio, hit: (usize, u64)) -> &TypeRun {
    match &s.doc.find_shape(hit.0, hit.1).unwrap().geom {
        Geom::Text(r) => r,
        _ => panic!("lost live text"),
    }
}
fn wire(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn apply(s: &mut Studio, op: TypeCommand) -> usize {
    let target = s.capture_type_command(TextScope::Selection).unwrap();
    s.apply_type_command(&target, op).unwrap()
}
fn reopen(s: &Studio) {
    let saved = wire(s);
    let doc = crate::project::decode(&saved).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), saved);
    assert!(
        crate::compositor::render_export(&doc, 1).unwrap().data()
            == crate::compositor::render_export(&s.doc, 1).unwrap().data()
    );
}
fn emit(name: &str, s: &Studio) {
    if let Some(root) = std::env::var_os("OMADESIGN_TYPE_CONTENT_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join(name), wire(s)).unwrap();
    }
}

#[test]
fn craft_type_content_unicode_case_keeps_styles_paragraphs_and_one_undo() {
    let (mut s, hit) = editor("straße café. NEXT!\nagain");
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.paragraphs = vec![ParagraphStyle {
            start: 19,
            align: TextAlign::End,
            ..Default::default()
        }];
        r.manual_kern.insert(21, -1.5);
    }
    s.mark();
    let before = wire(&s);
    emit("case-before.oma", &s);
    assert_eq!(apply(&mut s, TypeCommand::Case(TextCase::Upper)), 1);
    assert_eq!(run(&s, hit).content, "STRASSE CAFÉ. NEXT!\nAGAIN");
    assert_eq!(run(&s, hit).paragraphs[0].start, 20);
    assert_eq!(run(&s, hit).paragraphs[0].align, TextAlign::End);
    for at in 0..7 {
        assert_eq!(
            run(&s, hit).character_style(at).fill,
            Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
        );
    }
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    let after = wire(&s);
    emit("case-after.oma", &s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
    for (case, expected) in [
        (TextCase::Lower, "strasse café. next!\nagain"),
        (TextCase::Title, "Strasse Café. Next!\nAgain"),
        (TextCase::Sentence, "Strasse café. Next!\nAgain"),
    ] {
        apply(&mut s, TypeCommand::Case(case));
        assert_eq!(run(&s, hit).content, expected);
    }
}

#[test]
fn craft_type_content_live_reversed_range_retains_caret_and_draft_history() {
    let (mut s, hit) = editor("straße / untouched");
    s.begin_type_edit(hit, Pt::new(35., 65.));
    {
        let edit = s.type_edit.as_mut().unwrap();
        edit.anchor = 6;
        edit.caret = 0;
    }
    let before = wire(&s);
    apply(&mut s, TypeCommand::Case(TextCase::Upper));
    assert_eq!(run(&s, hit).content, "STRASSE / untouched");
    let edit = s.type_edit.as_ref().unwrap();
    assert_eq!((edit.caret, edit.anchor), (0, 7));
    assert_eq!(s.history.len(), 0);
    s.commit_type_edit();
    assert_eq!(s.history.len(), 1);
    let after = wire(&s);
    reopen(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
}

#[test]
fn craft_type_content_punctuation_keeps_style_boundaries_paragraphs_and_kerning() {
    let (mut s, hit) = editor("\"Hi\" it's 1--2 and 3---4...\nkeep me");
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.spans.clear();
        r.set_character_style(1, 3, |span| {
            span.fill = Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
        });
        let start = r.content.find('\n').unwrap() + 1;
        r.paragraphs = vec![ParagraphStyle {
            start,
            align: TextAlign::Center,
            ..Default::default()
        }];
        r.manual_kern.insert(start + 3, 2.5);
    }
    s.mark();
    let before = wire(&s);
    apply(
        &mut s,
        TypeCommand::Punctuation(SmartPunctuation::default()),
    );
    let expected = "“Hi” it’s 1–2 and 3—4…\nkeep me";
    assert_eq!(run(&s, hit).content, expected);
    let at = expected.chars().position(|c| c == '\n').unwrap() + 1;
    assert_eq!(run(&s, hit).paragraphs[0].start, at);
    assert_eq!(run(&s, hit).manual_kern.get(&(at + 3)), Some(&2.5));
    for n in 1..3 {
        assert_eq!(
            run(&s, hit).character_style(n).fill,
            Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
        );
    }
    reopen(&s);
    emit("punctuation.oma", &s);
    s.undo();
    assert_eq!(wire(&s), before);
    let shape = s.doc.find_shape_mut(hit.0, hit.1).unwrap();
    let Geom::Text(r) = &mut shape.geom else {
        unreachable!()
    };
    r.content = "--...".into();
    r.spans.clear();
    r.paragraphs.clear();
    r.manual_kern.clear();
    r.set_character_style(1, 2, |span| span.px = Some(31.));
    s.mark();
    apply(
        &mut s,
        TypeCommand::Punctuation(SmartPunctuation {
            quotes: false,
            ..Default::default()
        }),
    );
    assert_eq!(run(&s, hit).content, "--…");
}

#[test]
fn craft_type_content_all_insertions_use_native_unicode_and_live_pending_style() {
    let (mut s, hit) = editor("A");
    let text: String = SPECIAL_CHARACTERS
        .iter()
        .chain(WHITESPACE_CHARACTERS)
        .chain(BREAK_CHARACTERS)
        .map(|(_, text)| *text)
        .collect();
    assert_eq!(
        text.chars().map(u32::from).collect::<Vec<_>>(),
        vec![
            0x2022, 0x00a9, 0x00ae, 0x2122, 0x2026, 0x00b6, 0x00a7, 0x2014, 0x2013, 0x00ad, 0x2011,
            0x201c, 0x201d, 0x2018, 0x2019, 0x2003, 0x2002, 0x200a, 0x2006, 0x2009, 0x00a0, 0x2007,
            0x2008, 0x2004, 0x2005, 0x0009, 0x000a, 0x000a
        ]
    );
    assert_eq!(
        SPECIAL_CHARACTERS.len() + WHITESPACE_CHARACTERS.len() + BREAK_CHARACTERS.len(),
        28
    );
    let before = wire(&s);
    apply(&mut s, TypeCommand::Insert(text.clone()));
    assert_eq!(run(&s, hit).content, format!("A{text}"));
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.begin_type_edit(hit, Pt::new(35., 65.));
    {
        let e = s.type_edit.as_mut().unwrap();
        e.caret = 1;
        e.anchor = 1;
    }
    s.patch_character(|span| span.fill = Some(Fill::Solid(Rgba::rgb(20, 170, 80))));
    apply(&mut s, TypeCommand::Insert("©".into()));
    assert!(s.type_edit.as_ref().unwrap().pending_style.is_none());
    assert_eq!(run(&s, hit).content, "A©");
    assert_eq!(
        s.type_edit.as_ref().map(|e| (e.caret, e.anchor)),
        Some((2, 2))
    );
    assert_eq!(
        run(&s, hit).character_style(1).fill,
        Some(Fill::Solid(Rgba::rgb(20, 170, 80)))
    );
    s.commit_type_edit();
    reopen(&s);
}

#[test]
fn craft_type_content_document_deduplicates_hidden_linked_stories_and_stale_targets() {
    let (mut s, hit) = editor("\"First\" -- one...");
    s.convert_text_frame(true);
    let next = s.empty_thread_frame(hit, Pt::new(250., 25.)).unwrap();
    s.thread_text_frames(hit, next).unwrap();
    let (other, _) = editor("\"Other\" --- two...");
    let mut second = other.doc.layers[0].kind.shapes().unwrap()[0].clone();
    second.id = crate::document::next_id();
    second.visible = false;
    s.doc.layers[0].kind.shapes_mut().unwrap().push(second);
    s.selection = vec![next];
    s.mark();
    s.history.clear();
    let before = wire(&s);
    let target = s.capture_type_command(TextScope::Document).unwrap();
    assert_eq!(target.hits.len(), 2);
    assert_eq!(
        s.apply_type_command(
            &target,
            TypeCommand::Punctuation(SmartPunctuation::default())
        )
        .unwrap(),
        2
    );
    assert!(run(&s, hit).content.starts_with("“First” –"));
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    assert!(
        s.apply_type_command(&target, TypeCommand::Case(TextCase::Upper))
            .is_err()
    );
    s.undo();
    assert_eq!(wire(&s), before);
    let target = s.capture_type_command(TextScope::Selection).unwrap();
    s.doc.find_shape_mut(hit.0, hit.1).unwrap().locked = true;
    assert!(!s.type_command_current(&target));
    let locked = wire(&s);
    assert!(
        s.apply_type_command(&target, TypeCommand::Case(TextCase::Upper))
            .is_err()
    );
    assert_eq!(wire(&s), locked);
}

#[test]
fn craft_type_content_noop_keeps_live_ime_and_redo_while_changed_owners_refuse() {
    let (mut s, hit) = editor("ALREADY UPPER");
    s.commit(Cmd::SetOpacity {
        layer: 0,
        id: hit.1,
        before: 1.,
        after: 0.7,
    });
    s.undo();
    s.begin_type_edit(hit, Pt::new(35., 65.));
    s.type_edit.as_mut().unwrap().ime.preedit = Some(super::super::type_ime::Preedit {
        text: "ni".into(),
        active: Some(0..2),
    });
    let before = wire(&s);
    assert_eq!(apply(&mut s, TypeCommand::Case(TextCase::Upper)), 0);
    assert_eq!(wire(&s), before);
    assert!(s.history.can_redo());
    assert_eq!(
        s.type_edit
            .as_ref()
            .unwrap()
            .ime
            .preedit
            .as_ref()
            .unwrap()
            .text,
        "ni"
    );
    let target = s.capture_type_command(TextScope::Selection).unwrap();
    s.type_edit.as_mut().unwrap().caret += 1;
    assert!(
        s.apply_type_command(&target, TypeCommand::Insert("x".into()))
            .is_err()
    );
    assert_eq!(wire(&s), before);
    s.type_edit.as_mut().unwrap().caret -= 1;
    s.updates.freezing = true;
    assert!(s.capture_type_command(TextScope::Document).is_err());
}

#[test]
fn craft_type_content_imported_svg_retains_tspans_paints_and_positions() {
    let (mut s, _) = editor("unused");
    let (doc,_)=crate::formats::svg::read(r##"<svg xmlns="http://www.w3.org/2000/svg" width="260" height="100"><text x="10" y="50" font-size="28" fill="#cc2244">straße <tspan fill="#2266cc" dx="3">café</tspan></text></svg>"##,"Styled SVG").unwrap();
    s.doc = doc;
    s.doc.workspace = Some(Persona::Layout);
    let hit = s
        .doc
        .layers
        .iter()
        .enumerate()
        .find_map(|(layer, l)| {
            l.kind
                .shapes()
                .unwrap_or(&[])
                .iter()
                .find(|s| matches!(s.geom, Geom::Text(_)))
                .map(|s| (layer, s.id))
        })
        .unwrap();
    s.selection = vec![hit];
    s.active_layer = Some(hit.0);
    s.mark();
    s.history.clear();
    let before = wire(&s);
    apply(&mut s, TypeCommand::Case(TextCase::Upper));
    assert_eq!(run(&s, hit).content, "STRASSE CAFÉ");
    assert!(run(&s, hit).svg_text.is_some());
    let exported = crate::svg::export(&s.doc).unwrap();
    assert!(exported.contains("tspan"));
    assert!(exported.contains("dx=\"3\""));
    let parsed = crate::text::svg_type::layout(run(&s, hit)).unwrap();
    assert!(
        parsed
            .styles
            .iter()
            .any(|(_, style)| style.fill == Some(Fill::Solid(Rgba::rgb(34, 102, 204))))
    );
    reopen(&s);
    s.undo();
    assert_eq!(wire(&s), before);
}

#[test]
fn craft_type_content_batch_rewrite_refuses_invalid_ranges_and_preserves_large_style_runs() {
    let mut run = TypeRun {
        content: "aß\nabc".into(),
        ..Default::default()
    };
    run.set_character_style(1, 2, |s| s.px = Some(35.));
    for edits in [
        vec![CharacterRewrite {
            start: 2,
            end: 3,
            text: "x".into(),
        }],
        vec![
            CharacterRewrite {
                start: 0,
                end: 2,
                text: "X".into(),
            },
            CharacterRewrite {
                start: 1,
                end: 2,
                text: "Y".into(),
            },
        ],
        vec![CharacterRewrite {
            start: 0,
            end: 100,
            text: "x".into(),
        }],
    ] {
        let before = run.clone();
        assert!(run.rewrite_characters(&edits).is_err());
        assert_eq!(run, before);
    }
    run.rewrite_characters(&[CharacterRewrite {
        start: 1,
        end: 2,
        text: "SS".into(),
    }])
    .unwrap();
    assert_eq!(run.content, "aSS\nabc");
    for at in 1..3 {
        assert_eq!(run.character_style(at).px, Some(35.));
    }
    run.content = "abß ".repeat(5000);
    run.spans.clear();
    run.set_character_style(0, 20000, |s| s.px = Some(24.));
    let edits = rewrites(
        &run,
        0..run.content.chars().count(),
        &TypeCommand::Case(TextCase::Upper),
    );
    run.rewrite_characters(&edits).unwrap();
    assert_eq!(run.content, "ABSS ".repeat(5000));
    assert_eq!(run.spans.len(), 1);
    assert_eq!(run.spans[0].end, 25000);
}

#[test]
fn craft_type_content_rotated_point_area_path_and_affine_type_keep_the_original_frame() {
    for mode in 0..4 {
        let (mut s, hit) = editor("small mixed letters become wider");
        if mode == 1 {
            s.convert_text_frame(true);
        }
        if mode == 2 {
            let guide = Shape::new(
                Geom::Line {
                    a: Pt::new(30., 150.),
                    b: Pt::new(450., 105.),
                },
                Style {
                    fill: Fill::None,
                    stroke: None,
                },
            );
            let id = guide.id;
            s.doc.layers[0].kind.shapes_mut().unwrap().push(guide);
            s.selection = vec![hit, (0, id)];
            s.attach_text_path();
            s.selection = vec![hit];
        }
        let shape = s.doc.find_shape_mut(hit.0, hit.1).unwrap();
        shape.rotation = 0.37;
        if mode == 3
            && let Geom::Text(r) = &mut shape.geom
        {
            r.apply_affine([1.1, 0.13, 0.2, 0.9, 3., -4.]);
        }
        s.mark();
        s.history.clear();
        let source = s.doc.find_shape(hit.0, hit.1).unwrap().clone();
        let Geom::Text(r) = &source.geom else {
            unreachable!()
        };
        let point = r.origin;
        let anchor = r
            .map_point(point)
            .rotate_about(source.geom.bbox().center(), source.rotation);
        let before = wire(&s);
        apply(&mut s, TypeCommand::Case(TextCase::Upper));
        let after = s.doc.find_shape(hit.0, hit.1).unwrap();
        let Geom::Text(r) = &after.geom else {
            unreachable!()
        };
        let actual = r
            .map_point(point)
            .rotate_about(after.geom.bbox().center(), after.rotation);
        assert!(
            (anchor - actual).length() < 0.0001,
            "mode {mode}: {anchor:?} → {actual:?}"
        );
        assert_eq!(after.rotation, source.rotation);
        assert_eq!(after.style, source.style);
        assert_eq!(s.history.len(), 1);
        reopen(&s);
        s.undo();
        assert_eq!(wire(&s), before);
    }
}

#[test]
fn craft_type_size_steps_mixed_stories_in_one_undo() {
    let (mut s, hit) = editor("Colour size");
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.px = 16.;
        r.set_character_style(0, 4, |span| span.px = Some(20.));
        r.set_character_style(4, r.content.chars().count(), |span| span.px = Some(40.));
        r.manual_kern.insert(3, -1.25);
        r.layout = None;
    }
    crate::text::fill_contours(&mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom);
    let mut second_run = TypeRun {
        content: "Second".into(),
        origin: Pt::new(35., 140.),
        px: 12.,
        font: run(&s, hit).font.clone(),
        ..Default::default()
    };
    second_run.layout = None;
    let mut second = Shape::new(Geom::Text(second_run), Style::default());
    crate::text::fill_contours(&mut second.geom);
    let second_hit = (0, second.id);
    s.doc.layers[0].kind.shapes_mut().unwrap().push(second);
    s.selection = vec![hit, second_hit];
    s.mark();
    s.history.clear();
    let before = wire(&s);
    let before_width = s.doc.find_shape(hit.0, hit.1).unwrap().geom.bbox().width();
    assert_eq!(apply(&mut s, TypeCommand::Size(2.)), 2);
    assert_eq!(run(&s, hit).content, "Colour size");
    assert_eq!(run(&s, hit).character_style(0).px, Some(22.));
    assert_eq!(
        run(&s, hit).character_style(0).fill,
        Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
    );
    assert_eq!(run(&s, hit).character_style(5).px, Some(42.));
    assert_eq!(
        run(&s, hit).character_style(5).fill,
        Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
    );
    assert_eq!(run(&s, hit).character_style(8).px, Some(42.));
    assert_eq!(run(&s, hit).character_style(8).fill, None);
    assert_eq!(run(&s, hit).manual_kern.get(&3), Some(&-1.25));
    assert_eq!(run(&s, second_hit).content, "Second");
    assert_eq!(run(&s, second_hit).character_style(0).px, Some(14.));
    assert!(s.doc.find_shape(hit.0, hit.1).unwrap().geom.bbox().width() > before_width + 0.5);
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    let after = wire(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
}

#[test]
fn craft_type_size_live_range_keeps_the_rest_and_a_collapsed_caret_stays_pending() {
    let (mut s, hit) = editor("Colour size");
    s.begin_type_edit(hit, Pt::new(35., 65.));
    let before = wire(&s);
    assert_eq!(apply(&mut s, TypeCommand::Size(2.)), 0);
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 0);
    assert!(s.type_edit.as_ref().unwrap().pending_style.is_none());
    {
        let edit = s.type_edit.as_mut().unwrap();
        edit.pending_style = None;
        edit.anchor = 0;
        edit.caret = 4;
    }
    assert_eq!(apply(&mut s, TypeCommand::Size(2.)), 1);
    assert_eq!(run(&s, hit).character_style(0).px, Some(30.));
    assert!(run(&s, hit).character_style(5).px.is_none());
    assert_eq!(run(&s, hit).px, 28.);
    assert_eq!(
        run(&s, hit).character_style(0).fill,
        Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
    );
    assert_eq!(s.history.len(), 0);
    s.commit_type_edit();
    assert_eq!(s.history.len(), 1);
    let after = wire(&s);
    reopen(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
}

#[test]
fn craft_type_size_refuses_locked_text_and_a_size_already_at_the_limit() {
    let (mut s, hit) = editor("Colour size");
    s.doc.find_shape_mut(hit.0, hit.1).unwrap().locked = true;
    let before = wire(&s);
    s.type_selection_command(TypeCommand::Size(2.));
    assert!(s.status.contains("Unlock"), "{}", s.status);
    assert_eq!(s.history.len(), 0);
    assert_eq!(wire(&s), before);

    let (mut s, hit) = editor("Colour size");
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.px = 1296.;
        r.spans.clear();
        r.layout = None;
    }
    s.mark();
    s.history.clear();
    let before = wire(&s);
    assert_eq!(apply(&mut s, TypeCommand::Size(2.)), 0);
    assert_eq!(s.history.len(), 0);
    assert_eq!(wire(&s), before);
    assert_eq!(run(&s, hit).px, 1296.);
    assert!(run(&s, hit).spans.iter().all(|span| span.px.is_none()));

    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.px = 0.1;
    }
    s.mark();
    s.history.clear();
    let before = wire(&s);
    assert_eq!(apply(&mut s, TypeCommand::Size(-2.)), 0);
    assert_eq!(s.history.len(), 0);
    assert_eq!(wire(&s), before);
    assert_eq!(run(&s, hit).px, 0.1);

    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.px = 1.;
    }
    s.mark();
    s.history.clear();
    assert_eq!(apply(&mut s, TypeCommand::Size(-2.)), 1);
    assert_eq!(run(&s, hit).character_style(0).px, Some(0.1));
    assert_eq!(s.history.len(), 1);
}

#[test]
fn craft_type_size_steps_imported_svg_sizes_without_replacing_the_face() {
    let (mut s, hit) = editor("Colour & curves");
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.px = 38.;
        r.spans.clear();
        r.set_character_style(0, 6, |span| {
            span.fill = Some(Fill::Solid(Rgba::rgb(220, 40, 75)));
        });
        r.set_character_style(9, 15, |span| {
            span.fill = Some(Fill::Solid(Rgba::rgb(20, 140, 210)));
            span.px = Some(42.);
        });
        r.layout = None;
    }
    s.mark();
    let svg = crate::svg::export_embedded(&s.doc).unwrap();
    let (doc, notes) = crate::formats::svg::read(&svg, "Sized SVG text").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Layout;
    s.doc = doc;
    s.doc.workspace = Some(Persona::Layout);
    let hit = s
        .doc
        .layers
        .iter()
        .enumerate()
        .find_map(|(layer, item)| {
            item.kind
                .shapes()
                .unwrap_or(&[])
                .iter()
                .find_map(|shape| match &shape.geom {
                    Geom::Text(text) if text.svg_text.is_some() => Some((layer, shape.id)),
                    _ => None,
                })
        })
        .expect("imported SVG text");
    s.active_layer = Some(hit.0);
    s.selection = vec![hit];
    s.mark();
    s.history.clear();
    let count = run(&s, hit).content.chars().count();
    let base = run(&s, hit).px;
    let before_styles: Vec<_> = (0..count)
        .map(|index| run(&s, hit).character_style(index))
        .collect();
    let before_width = s.doc.find_shape(hit.0, hit.1).unwrap().geom.bbox().width();
    assert!(
        (before_styles[9].px.unwrap_or(base) - before_styles[0].px.unwrap_or(base)).abs() > 1.,
        "imported sizes collapsed"
    );
    let before = wire(&s);
    assert_eq!(apply(&mut s, TypeCommand::Size(2.)), 1);
    assert!(run(&s, hit).svg_text.is_some());
    assert_eq!(run(&s, hit).content.chars().count(), count);
    for (index, style) in before_styles.iter().enumerate() {
        let after = run(&s, hit).character_style(index);
        assert_eq!(after.font, style.font, "character {index} face changed");
        assert_eq!(after.fill, style.fill, "character {index} fill changed");
        let expected = style.px.unwrap_or(base) + 2.;
        let actual = after.px.expect("stepped size");
        assert!(
            (actual - expected).abs() < 0.001,
            "{index}: {expected} -> {actual}"
        );
    }
    assert!(s.doc.find_shape(hit.0, hit.1).unwrap().geom.bbox().width() > before_width + 0.5);
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    let after = wire(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
}

fn area(text: &str, size: Pt) -> (Studio, (usize, u64)) {
    let (mut s, hit) = editor(text);
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.px = 24.;
        r.tracking = 1.5;
        r.frame = Some(crate::text_geometry::TextFrame {
            size,
            inset: [4., 12., 6., 8.],
            ..Default::default()
        });
        r.manual_kern.insert(2, -4.);
        let body = text
            .chars()
            .position(|c| c == '\n')
            .map(|index| index + 1)
            .unwrap_or(text.chars().count());
        if body < text.chars().count() {
            r.set_character_style(body, text.chars().count(), |span| {
                span.tracking = Some(30.);
            });
        }
        r.layout = None;
    }
    crate::text::fill_contours(&mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom);
    s.mark();
    s.history.clear();
    (s, hit)
}

fn first_line(run: &TypeRun) -> (String, f32, f32) {
    let layout = crate::text_geometry::layout_frame(run, run, 0, &[], false);
    let line = layout.export_lines.first().expect("area line");
    (
        line.line.text.clone(),
        line.line.width,
        line.run.wrap_width.expect("frame span"),
    )
}

#[test]
fn craft_type_fit_headline_fills_the_first_line_and_keeps_the_body() {
    let (mut s, hit) = area("Headline\nBody copy", Pt::new(420., 180.));
    let mut point = Shape::new(
        Geom::Text(TypeRun {
            content: "point".into(),
            origin: Pt::new(40., 200.),
            px: 18.,
            font: run(&s, hit).font.clone(),
            tracking: 3.,
            ..Default::default()
        }),
        Style::default(),
    );
    crate::text::fill_contours(&mut point.geom);
    let point_hit = (0, point.id);
    s.doc.layers[0].kind.shapes_mut().unwrap().push(point);
    s.selection = vec![hit, point_hit];
    s.mark();
    s.history.clear();
    let before = wire(&s);
    assert_eq!(apply(&mut s, TypeCommand::FitHeadline), 1);
    let fitted = run(&s, hit);
    let (text, width, avail) = first_line(fitted);
    assert_eq!(text, "Headline");
    assert!((width - avail).abs() < 1., "first line {width} of {avail}");
    assert!(fitted.character_style(0).tracking.unwrap() > 100.);
    assert_eq!(
        fitted.character_style(0).fill,
        Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
    );
    assert_eq!(fitted.character_style(9).tracking, Some(30.));
    assert_eq!(fitted.tracking, 1.5);
    assert_eq!(fitted.manual_kern.get(&2), Some(&-4.));
    assert_eq!(fitted.content, "Headline\nBody copy");
    assert!(run(&s, point_hit).frame.is_none());
    assert_eq!(run(&s, point_hit).tracking, 3.);
    assert_eq!(run(&s, point_hit).content, "point");
    assert!(run(&s, point_hit).spans.is_empty());
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    let after = wire(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
    assert_eq!(apply(&mut s, TypeCommand::FitHeadline), 0);
    assert_eq!(s.history.len(), 1);
    assert_eq!(wire(&s), after);
    s.undo();
    s.begin_type_edit(hit, Pt::new(50., 70.));
    assert!(s.type_edit.is_some());
    assert_eq!(apply(&mut s, TypeCommand::FitHeadline), 1);
    assert!(s.type_edit.is_none());
    assert_eq!(s.history.len(), 1);
    let (text, width, avail) = first_line(run(&s, hit));
    assert_eq!(text, "Headline");
    assert!((width - avail).abs() < 1., "{width} of {avail}");
}

#[test]
fn craft_type_fit_headline_tightens_a_line_that_is_already_too_wide() {
    let (mut s, hit) = area("HEADLINE", Pt::new(140., 90.));
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.px = 48.;
        r.layout = None;
    }
    s.history.clear();
    let before = wire(&s);
    let (text, wide, avail) = first_line(run(&s, hit));
    assert_eq!(text, "HEADLINE");
    assert!(wide > avail, "{wide} of {avail}");
    assert_eq!(apply(&mut s, TypeCommand::FitHeadline), 1);
    let fitted = run(&s, hit);
    let tracking = fitted.character_style(0).tracking.unwrap();
    assert!(tracking < 0., "{tracking}");
    assert!(tracking >= -1000.);
    let (text, width, avail) = first_line(fitted);
    assert_eq!(text, "HEADLINE");
    assert!((width - avail).abs() < 1., "{width} of {avail}");
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    s.undo();
    assert_eq!(wire(&s), before);
}

#[test]
fn craft_type_fit_headline_refuses_point_path_empty_and_locked_text() {
    let (mut point, _hit) = editor("point");
    let before = wire(&point);
    let target = point.capture_type_command(TextScope::Selection).unwrap();
    assert_eq!(
        point
            .apply_type_command(&target, TypeCommand::FitHeadline)
            .unwrap_err(),
        "Select area type"
    );
    assert_eq!(point.history.len(), 0);
    assert_eq!(wire(&point), before);

    let (mut path, hit) = area("On path\nrest", Pt::new(300., 120.));
    if let Geom::Text(r) = &mut path.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.on_path = Some(crate::text_geometry::TextOnPath::default());
    }
    path.mark();
    path.history.clear();
    let before = wire(&path);
    let target = path.capture_type_command(TextScope::Selection).unwrap();
    assert_eq!(
        path.apply_type_command(&target, TypeCommand::FitHeadline)
            .unwrap_err(),
        "Select area type"
    );
    assert_eq!(wire(&path), before);
    assert_eq!(path.history.len(), 0);

    let (mut empty, hit) = area("\nBody only", Pt::new(300., 120.));
    let before = wire(&empty);
    assert_eq!(apply(&mut empty, TypeCommand::FitHeadline), 0);
    assert_eq!(empty.history.len(), 0);
    assert_eq!(wire(&empty), before);
    assert_eq!(run(&empty, hit).content, "\nBody only");

    let (mut broken, hit) = area("Headline", Pt::new(300., 120.));
    if let Geom::Text(r) = &mut broken.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.frame.as_mut().unwrap().size.x = f32::NAN;
    }
    broken.history.clear();
    let spans = run(&broken, hit).spans.clone();
    let target = broken.capture_type_command(TextScope::Selection).unwrap();
    assert_eq!(
        broken
            .apply_type_command(&target, TypeCommand::FitHeadline)
            .unwrap_err(),
        "The type frame has no finite width"
    );
    assert_eq!(broken.history.len(), 0);
    assert_eq!(run(&broken, hit).spans, spans);
    assert_eq!(run(&broken, hit).content, "Headline");

    let (mut locked, _) = area("Headline\nBody", Pt::new(300., 120.));
    let id = locked.selection[0];
    locked.doc.find_shape_mut(id.0, id.1).unwrap().locked = true;
    let before = wire(&locked);
    locked.type_selection_command(TypeCommand::FitHeadline);
    assert!(locked.status.contains("Unlock"));
    assert_eq!(locked.history.len(), 0);
    assert_eq!(wire(&locked), before);
}

const POINT_PLACEHOLDER: &str = "Placeholder text sits here while the frame is still open.";

#[test]
fn craft_type_placeholder_replaces_point_text_keeps_the_first_style_and_one_undo() {
    let (mut s, hit) = editor("Old words stay");
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        let count = r.content.chars().count();
        r.set_character_style(6, count, |span| {
            span.fill = Some(Fill::Solid(Rgba::rgb(20, 140, 90)));
        });
        r.manual_kern.insert(2, -3.);
        r.layout = None;
    }
    s.mark();
    s.history.clear();
    let before = wire(&s);
    assert_eq!(apply(&mut s, TypeCommand::Placeholder), 1);
    let text = run(&s, hit);
    assert_eq!(text.content, POINT_PLACEHOLDER);
    assert!(text.manual_kern.is_empty());
    assert_eq!(text.tracking, 0.);
    let count = text.content.chars().count();
    for index in 0..count {
        assert_eq!(
            text.character_style(index).fill,
            Some(Fill::Solid(Rgba::rgb(210, 30, 70))),
            "character {index}"
        );
    }
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    let after = wire(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
    assert_eq!(apply(&mut s, TypeCommand::Placeholder), 0);
    assert_eq!(s.history.len(), 1);
    assert_eq!(wire(&s), after);
    s.undo();
    s.begin_type_edit(hit, Pt::new(40., 65.));
    assert!(s.type_edit.is_some());
    assert_eq!(apply(&mut s, TypeCommand::Placeholder), 1);
    assert!(s.type_edit.is_none());
    assert_eq!(run(&s, hit).content, POINT_PLACEHOLDER);
    assert_eq!(s.history.len(), 1);
}

#[test]
fn craft_type_placeholder_fills_an_area_frame_and_leaves_point_type_as_a_sentence() {
    let (mut s, hit) = area("Short\nBody", Pt::new(420., 180.));
    let mut point = Shape::new(
        Geom::Text(TypeRun {
            content: "point".into(),
            origin: Pt::new(40., 220.),
            px: 18.,
            font: run(&s, hit).font.clone(),
            ..Default::default()
        }),
        Style::default(),
    );
    crate::text::fill_contours(&mut point.geom);
    let point_hit = (0, point.id);
    s.doc.layers[0].kind.shapes_mut().unwrap().push(point);
    s.selection = vec![hit, point_hit];
    s.mark();
    s.history.clear();
    let before = wire(&s);
    let frame = run(&s, hit).frame.clone();
    assert_eq!(apply(&mut s, TypeCommand::Placeholder), 2);
    let area_text = run(&s, hit);
    assert!(area_text.content.len() > POINT_PLACEHOLDER.len());
    assert!(area_text.content.starts_with("Placeholder text sits here"));
    assert_eq!(area_text.frame, frame);
    assert_eq!(area_text.tracking, 1.5);
    assert!(area_text.content.chars().all(|c| c.is_ascii()));
    for index in 0..area_text.content.chars().count() {
        assert_eq!(
            area_text.character_style(index).fill,
            Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
        );
        assert!(area_text.character_style(index).tracking.is_none());
    }
    assert_eq!(run(&s, point_hit).content, POINT_PLACEHOLDER);
    assert!(run(&s, point_hit).frame.is_none());
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    let after = wire(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
    assert_eq!(apply(&mut s, TypeCommand::Placeholder), 0);
    assert_eq!(s.history.len(), 1);
}

#[test]
fn craft_type_placeholder_refuses_a_non_finite_frame_and_locked_text() {
    let (mut broken, hit) = area("Headline", Pt::new(240., 80.));
    if let Geom::Text(r) = &mut broken.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.frame.as_mut().unwrap().size.y = f32::NAN;
    }
    broken.history.clear();
    let content = run(&broken, hit).content.clone();
    let target = broken.capture_type_command(TextScope::Selection).unwrap();
    assert_eq!(
        broken
            .apply_type_command(&target, TypeCommand::Placeholder)
            .unwrap_err(),
        "The type frame has no finite size"
    );
    assert_eq!(broken.history.len(), 0);
    assert_eq!(run(&broken, hit).content, content);

    let (mut empty, hit) = editor("");
    assert_eq!(apply(&mut empty, TypeCommand::Placeholder), 1);
    assert_eq!(run(&empty, hit).content, POINT_PLACEHOLDER);
    assert_eq!(empty.history.len(), 1);

    let (mut path, hit) = area("On a path", Pt::new(300., 100.));
    if let Geom::Text(r) = &mut path.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.on_path = Some(crate::text_geometry::TextOnPath::default());
        r.layout = None;
    }
    path.mark();
    path.history.clear();
    assert_eq!(apply(&mut path, TypeCommand::Placeholder), 1);
    assert_eq!(run(&path, hit).content, POINT_PLACEHOLDER);
    assert!(run(&path, hit).frame.is_some());
    assert_eq!(path.history.len(), 1);

    let (mut locked, _) = editor("Locked");
    let id = locked.selection[0];
    locked.doc.find_shape_mut(id.0, id.1).unwrap().locked = true;
    let before = wire(&locked);
    locked.type_selection_command(TypeCommand::Placeholder);
    assert!(locked.status.contains("Unlock"));
    assert_eq!(locked.history.len(), 0);
    assert_eq!(wire(&locked), before);
}

const SANS: &str = "/usr/share/fonts/liberation/LiberationSans-Regular.ttf";
const SANS_ITALIC: &str = "/usr/share/fonts/liberation/LiberationSans-Italic.ttf";
const ONE_FACE: &str = "/usr/share/fonts/noto/NotoSansBuhid-Regular.ttf";

fn shown_face(reference: &str) -> (String, String) {
    let info = crate::text::font_inventory::describe(
        &crate::text::font_inventory::Request::Stored(reference.into()),
        reference,
        &std::collections::BTreeSet::new(),
    );
    assert_eq!(
        info.status,
        crate::text::font_inventory::Status::Exact,
        "{reference}"
    );
    (info.family, info.style)
}

#[test]
fn craft_type_font_menu_applies_a_family_keeps_the_authored_style_and_one_undo() {
    let styles = [
        "Light",
        "Narrow",
        "Regular",
        "Italic",
        "Narrow Italic",
        "SemiBold",
        "Bold",
        "Bold Condensed",
        "Narrow Bold",
        "Bold Italic",
        "Black",
    ]
    .map(str::to_string);
    assert_eq!(
        super::pick_style(&styles, true, false, "Regular"),
        Some("Bold")
    );
    assert_eq!(
        super::pick_style(&styles, true, false, "Narrow"),
        Some("Narrow Bold")
    );
    assert_eq!(
        super::pick_style(&styles, false, true, "Regular"),
        Some("Italic")
    );
    assert_eq!(
        super::pick_style(&styles, true, true, "Italic"),
        Some("Bold Italic")
    );
    assert_ne!(
        super::pick_style(&styles, true, false, "Regular"),
        Some("SemiBold")
    );
    assert_ne!(
        super::pick_style(&styles, true, false, "Regular"),
        Some("Bold Condensed")
    );
    let (bold, regular, italic, rest) = super::style_parts("SemiBold");
    assert!(!bold && !regular && !italic && rest.is_empty());

    let (mut s, hit) = editor("Old words stay");
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.set_character_style(3, 6, |span| span.font = Some(SANS_ITALIC.into()));
        r.manual_kern.insert(2, -3.);
        r.layout = None;
    }
    s.mark();
    s.history.clear();
    let before = wire(&s);
    assert_eq!(
        apply(
            &mut s,
            TypeCommand::Face(TypeFace::Family("Liberation Sans".into()))
        ),
        1
    );
    let text = run(&s, hit);
    assert_eq!(text.content, "Old words stay");
    assert_eq!(text.manual_kern.get(&2), Some(&-3.));
    assert_eq!(
        text.character_style(0).fill,
        Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
    );
    assert_eq!(
        shown_face(&text.font),
        ("Liberation Sans".into(), "Regular".into())
    );
    assert!(text.font.starts_with("omatype:"));
    assert_eq!(
        shown_face(
            text.spans
                .iter()
                .find(|span| span.font.is_some())
                .unwrap()
                .font
                .as_deref()
                .unwrap()
        ),
        ("Liberation Sans".into(), "Regular".into())
    );
    assert_eq!(s.selection, vec![hit]);
    assert_eq!(s.history.len(), 1);
    let changed = wire(&s);
    let doc = crate::project::decode(&changed).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), changed);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), changed);
    assert_eq!(
        apply(
            &mut s,
            TypeCommand::Face(TypeFace::Family("Liberation Sans".into()))
        ),
        0
    );
    assert_eq!(s.history.len(), 1);

    s.begin_type_edit(hit, Pt::new(35., 65.));
    assert!(s.type_edit.is_some());
    assert_eq!(
        apply(&mut s, TypeCommand::Face(TypeFace::Recent(SANS.into()))),
        1
    );
    assert!(s.type_edit.is_none());
    assert_eq!(run(&s, hit).font, SANS);
    assert_eq!(s.history.len(), 2);
    assert_eq!(s.font_recents.first().map(String::as_str), Some(SANS));
    s.undo();
    assert_eq!(wire(&s), changed);
}

#[test]
fn craft_type_font_menu_refuses_missing_faces_locked_text_and_a_family_without_bold() {
    let (mut s, hit) = editor("Stay put");
    let before = wire(&s);
    let target = s.capture_type_command(TextScope::Selection).unwrap();
    assert_eq!(
        s.apply_type_command(
            &target,
            TypeCommand::Face(TypeFace::Family("Oma Missing Menu Family".into()))
        )
        .unwrap_err(),
        "Choose an available font family"
    );
    assert_eq!(
        s.apply_type_command(
            &target,
            TypeCommand::Face(TypeFace::Recent("/no/such/omadesign-font.ttf".into()))
        )
        .unwrap_err(),
        "Choose an available font"
    );
    assert_eq!(s.history.len(), 0);
    assert_eq!(wire(&s), before);
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.font = ONE_FACE.into();
        r.layout = None;
    }
    s.mark();
    s.history.clear();
    let alone = wire(&s);
    let err = s
        .capture_type_command(TextScope::Selection)
        .and_then(|target| s.apply_type_command(&target, TypeCommand::Face(TypeFace::Bold)))
        .unwrap_err();
    assert!(err.contains("has no Bold style"), "{err}");
    assert_eq!(run(&s, hit).font, ONE_FACE);
    assert_eq!(s.history.len(), 0);
    assert_eq!(wire(&s), alone);

    let mut sibling = TypeRun {
        content: "Other".into(),
        origin: Pt::new(35., 140.),
        px: 28.,
        font: SANS.into(),
        ..Default::default()
    };
    sibling.layout = None;
    let mut shape = Shape::new(Geom::Text(sibling), Style::default());
    crate::text::fill_contours(&mut shape.geom);
    let other = (0, shape.id);
    s.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    s.selection = vec![hit, other];
    s.mark();
    s.history.clear();
    let before = wire(&s);
    let err = s
        .capture_type_command(TextScope::Selection)
        .and_then(|target| s.apply_type_command(&target, TypeCommand::Face(TypeFace::Bold)))
        .unwrap_err();
    assert!(err.contains("has no Bold style"), "{err}");
    assert_eq!(s.history.len(), 0);
    assert_eq!(wire(&s), before);
    assert_eq!(run(&s, other).font, SANS);

    let (mut locked, _) = editor("Locked");
    let id = locked.selection[0];
    locked.doc.find_shape_mut(id.0, id.1).unwrap().locked = true;
    let before = wire(&locked);
    locked.type_selection_command(TypeCommand::Face(TypeFace::Family(
        "Liberation Sans".into(),
    )));
    assert!(locked.status.contains("Unlock"));
    assert_eq!(locked.history.len(), 0);
    assert_eq!(wire(&locked), before);
}

#[test]
fn craft_type_font_menu_recent_bold_and_italic_use_the_familys_own_faces() {
    let (mut s, hit) = editor("Styled");
    let original = run(&s, hit).font.clone();
    s.font_recents = vec![SANS_ITALIC.into(), SANS.into()];
    assert_eq!(
        apply(&mut s, TypeCommand::Face(TypeFace::Recent(SANS.into()))),
        1
    );
    assert_eq!(run(&s, hit).font, SANS);
    assert_eq!(s.font_recents[0], SANS);
    assert_eq!(shown_face(&run(&s, hit).font).0, "Liberation Sans");
    assert_eq!(apply(&mut s, TypeCommand::Face(TypeFace::Bold)), 1);
    assert_eq!(
        shown_face(&run(&s, hit).font),
        ("Liberation Sans".into(), "Bold".into())
    );
    assert!(run(&s, hit).font.starts_with("omatype:"));
    assert_ne!(shown_face(&run(&s, hit).font).0, "Noto Sans");
    assert_eq!(apply(&mut s, TypeCommand::Face(TypeFace::Italic)), 1);
    assert_eq!(
        shown_face(&run(&s, hit).font),
        ("Liberation Sans".into(), "Bold Italic".into())
    );
    assert_eq!(apply(&mut s, TypeCommand::Face(TypeFace::Bold)), 1);
    assert_eq!(
        shown_face(&run(&s, hit).font),
        ("Liberation Sans".into(), "Italic".into())
    );
    assert_eq!(apply(&mut s, TypeCommand::Face(TypeFace::Italic)), 1);
    assert_eq!(
        shown_face(&run(&s, hit).font),
        ("Liberation Sans".into(), "Regular".into())
    );
    assert_eq!(s.history.len(), 5);
    assert_eq!(run(&s, hit).content, "Styled");
    assert_eq!(
        run(&s, hit).character_style(0).fill,
        Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
    );
    let saved = wire(&s);
    s.undo();
    s.undo();
    s.undo();
    s.undo();
    s.undo();
    assert_eq!(run(&s, hit).font, original);
    assert_eq!(s.history.len(), 0);
    s.redo();
    s.redo();
    s.redo();
    s.redo();
    s.redo();
    assert_eq!(wire(&s), saved);
}
