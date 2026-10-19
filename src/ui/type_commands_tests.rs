use super::*;
use crate::{
    app::Studio,
    document::{Document, Fill, Layer, Shape, Style},
    geom::{Geom, Pt, TypeRun},
    tools::Persona,
    ui::welcome::keyboard_tests as keys,
};
use eframe::egui::{Event, Key, Modifiers, PointerButton, Pos2};

fn editor() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Layout;
    s.doc = Document::new("Native Type content", 480., 200., 96.);
    s.doc.workspace = Some(Persona::Layout);
    s.doc.layers = vec![Layer::vector("Live type")];
    let mut run = TypeRun {
        content: "\"hello wORLD\" -- ...".into(),
        origin: Pt::new(35., 75.),
        px: 30.,
        font: concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/assets/fonts/EBGaramond.ttf"
        )
        .into(),
        ..Default::default()
    };
    run.set_character_style(0, 6, |span| {
        span.fill = Some(Fill::Solid(crate::color::Rgba::rgb(220, 35, 70)))
    });
    let mut shape = Shape::new(Geom::Text(run), Style::default());
    crate::text::fill_contours(&mut shape.geom);
    let id = shape.id;
    s.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    s.active_layer = Some(0);
    s.selection = vec![(0, id)];
    s.mark();
    s.history.clear();
    s
}
fn content(s: &Studio) -> String {
    s.selected_type().unwrap().content
}
fn wire(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn emit(name: &str, s: &Studio) {
    if let Some(root) = std::env::var_os("OMADESIGN_TYPE_CONTENT_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join(name), wire(s)).unwrap();
    }
}
fn canvas_focus(seat: &Seat) {
    let response = seat.ctx.read_response(Id::new("studio-canvas")).unwrap();
    let id = response.id.accesskit_id();
    assert_eq!(seat.ctx.memory(|m| m.focused()), Some(response.id));
    assert_eq!(
        seat.output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .focus,
        id
    );
    assert_eq!(keys::named(&seat.output, "Artwork canvas").0, id);
    fn border(shape: &egui::Shape, rect: egui::Rect, stroke: egui::Stroke) -> bool {
        match shape {
            egui::Shape::Rect(r) => r.rect == rect && r.stroke == stroke,
            egui::Shape::Vec(shapes) => shapes.iter().any(|s| border(s, rect, stroke)),
            _ => false,
        }
    }
    let rect = response.rect.shrink(2.);
    let stroke = seat.ctx.global_style().visuals.selection.stroke;
    assert!(
        seat.output
            .shapes
            .iter()
            .any(|s| border(&s.shape, rect, stroke)),
        "actual canvas focus border is not painted"
    );
}
struct Seat {
    ctx: egui::Context,
    time: f64,
    output: egui::FullOutput,
}
impl Seat {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.);
        Self {
            ctx,
            time: 0.,
            output: Default::default(),
        }
    }
    fn frame(&mut self, s: &mut Studio, mut events: Vec<Event>) -> egui::FullOutput {
        self.time += 0.2;
        let modifiers = events
            .iter()
            .find_map(|e| match e {
                Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        events.insert(0, Event::ModifiersChanged(modifiers));
        let mut out = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1600., 1000.),
                )),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ui| crate::ui::run(ui, s),
        );
        out.textures_delta.clear();
        self.output = out.clone();
        out
    }
    fn click(&mut self, s: &mut Studio, name: &str) {
        for _ in 0..3 {
            self.frame(s, vec![]);
        }
        let bounds = keys::named(&self.output, name).1.bounds().unwrap();
        let pos = egui::pos2(
            ((bounds.x0 + bounds.x1) * 0.5) as f32,
            ((bounds.y0 + bounds.y1) * 0.5) as f32,
        );
        self.frame(s, vec![Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            self.frame(
                s,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        self.frame(s, vec![]);
    }
    fn invoke(&mut self, s: &mut Studio, name: &str) {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, name, |e| self.frame(s, e));
        self.frame(s, keys::key(Key::Space, Modifiers::NONE));
        self.frame(s, vec![]);
    }
}

#[test]
fn craft_type_content_native_case_and_insert_menus_keep_one_undo_and_reopen() {
    let mut s = editor();
    let before = wire(&s);
    let mut seat = Seat::new();
    seat.click(&mut s, "Type");
    seat.click(&mut s, "Change case ⏵");
    seat.invoke(&mut s, "UPPERCASE");
    assert_eq!(content(&s), "\"HELLO WORLD\" -- ...");
    assert_eq!(s.history.len(), 1);
    let case = wire(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), case);
    seat.click(&mut s, "Type");
    seat.click(&mut s, "Insert special character ⏵");
    seat.invoke(&mut s, "Copyright ©");
    assert!(content(&s).ends_with('©'));
    assert_eq!(s.history.len(), 2);
    let after = wire(&s);
    let doc = crate::project::decode(&after).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), after);
    emit("native-case-insert.oma", &s);
    s.undo();
    assert_eq!(wire(&s), case);
}

#[test]
fn craft_type_content_native_punctuation_dialog_blocks_background_undo_and_applies_real_options() {
    let mut s = editor();
    let hit = s.selection[0];
    s.commit(crate::document::Cmd::SetOpacity {
        layer: hit.0,
        id: hit.1,
        before: 1.,
        after: 0.85,
    });
    let mut seat = Seat::new();
    let before = wire(&s);
    seat.click(&mut s, "Type");
    seat.invoke(&mut s, "Smart punctuation…");
    assert!(is_open(&seat.ctx));
    keys::focused(&seat.ctx, &seat.output, "Typographic quotes");
    seat.frame(
        &mut s,
        keys::key(
            Key::Z,
            Modifiers {
                ctrl: true,
                command: true,
                ..Modifiers::NONE
            },
        ),
    );
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 1);
    seat.invoke(&mut s, "Double quotation marks");
    seat.invoke(&mut s, "« … »");
    seat.invoke(&mut s, "Apply smart punctuation");
    assert!(!is_open(&seat.ctx));
    assert_eq!(content(&s), "«hello wORLD» – …");
    canvas_focus(&seat);
    assert_eq!(s.history.len(), 2);
    let after = wire(&s);
    emit("native-punctuation.oma", &s);
    let doc = crate::project::decode(&after).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), after);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
}

#[test]
fn craft_type_content_native_stale_dialog_and_live_cancel_leave_text_and_draft_intact() {
    let mut s = editor();
    let hit = s.selection[0];
    s.begin_type_edit(hit, Pt::new(35., 75.));
    s.type_edit.as_mut().unwrap().caret = 3;
    s.type_edit.as_mut().unwrap().anchor = 1;
    let before = wire(&s);
    let mut seat = Seat::new();
    seat.click(&mut s, "Type");
    seat.invoke(&mut s, "Smart punctuation…");
    seat.frame(&mut s, keys::key(Key::Escape, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    assert!(!is_open(&seat.ctx));
    canvas_focus(&seat);
    assert_eq!(wire(&s), before);
    assert_eq!(
        s.type_edit.as_ref().map(|e| (e.caret, e.anchor)),
        Some((3, 1))
    );
    seat.click(&mut s, "Type");
    seat.invoke(&mut s, "Smart punctuation…");
    s.commit(crate::document::Cmd::SetOpacity {
        layer: hit.0,
        id: hit.1,
        before: 1.,
        after: 0.7,
    });
    seat.frame(&mut s, vec![]);
    assert!(
        keys::named(&seat.output, "Apply smart punctuation")
            .1
            .is_disabled()
    );
    let stale = wire(&s);
    seat.click(&mut s, "Apply smart punctuation");
    assert!(is_open(&seat.ctx));
    assert_eq!(wire(&s), stale);
    seat.frame(&mut s, keys::key(Key::Escape, Modifiers::NONE));
    assert!(!is_open(&seat.ctx));
    assert_eq!(wire(&s), stale);
}

#[test]
fn craft_type_size_native_type_menu_steps_one_undo_and_reopens() {
    let mut s = editor();
    let before = wire(&s);
    let mut seat = Seat::new();
    seat.click(&mut s, "Type");
    seat.invoke(&mut s, "Increase font size");
    let text = s.selected_type().unwrap();
    assert_eq!(text.character_style(0).px, Some(32.));
    assert_eq!(
        text.character_style(0).fill,
        Some(Fill::Solid(crate::color::Rgba::rgb(220, 35, 70)))
    );
    assert_eq!(text.character_style(10).px, Some(32.));
    assert_eq!(text.content, "\"hello wORLD\" -- ...");
    assert_eq!(s.history.len(), 1);
    let increased = wire(&s);
    let doc = crate::project::decode(&increased).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), increased);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), increased);
    seat.click(&mut s, "Type");
    seat.invoke(&mut s, "Decrease font size");
    assert_eq!(s.selected_type().unwrap().character_style(0).px, Some(30.));
    assert_eq!(s.history.len(), 2);
    let decreased = wire(&s);
    s.undo();
    assert_eq!(wire(&s), increased);
    s.redo();
    assert_eq!(wire(&s), decreased);
}

#[test]
fn craft_type_size_native_menu_stays_disabled_on_locked_text() {
    let mut s = editor();
    let hit = s.selection[0];
    s.doc.find_shape_mut(hit.0, hit.1).unwrap().locked = true;
    let before = wire(&s);
    let mut seat = Seat::new();
    seat.click(&mut s, "Type");
    assert!(
        keys::named(&seat.output, "Increase font size")
            .1
            .is_disabled()
    );
    assert!(
        keys::named(&seat.output, "Decrease font size")
            .1
            .is_disabled()
    );
    assert_eq!(s.history.len(), 0);
    assert_eq!(wire(&s), before);
}

fn area_editor() -> Studio {
    let mut s = editor();
    let hit = s.selection[0];
    if let Geom::Text(run) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        run.content = "Headline\nBody copy".into();
        run.px = 24.;
        run.frame = Some(crate::text_geometry::TextFrame {
            size: Pt::new(420., 160.),
            inset: [0., 10., 0., 10.],
            ..Default::default()
        });
        run.set_character_style(9, run.content.chars().count(), |span| {
            span.tracking = Some(18.);
        });
        run.layout = None;
    }
    crate::text::fill_contours(&mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom);
    s.mark();
    s.history.clear();
    s
}

#[test]
fn craft_type_fit_headline_native_type_menu_fills_one_undo_and_reopens() {
    let mut s = area_editor();
    let before = wire(&s);
    let mut seat = Seat::new();
    seat.click(&mut s, "Type");
    seat.invoke(&mut s, "Fit headline");
    let text = s.selected_type().unwrap();
    let layout = crate::text_geometry::layout_frame(&text, &text, 0, &[], false);
    let line = layout.export_lines.first().expect("headline");
    assert_eq!(line.line.text, "Headline");
    let avail = line.run.wrap_width.expect("span");
    assert!(
        (line.line.width - avail).abs() < 1.,
        "{} of {avail}",
        line.line.width
    );
    assert!(text.character_style(0).tracking.unwrap() > 100.);
    assert_eq!(text.character_style(9).tracking, Some(18.));
    assert_eq!(text.content, "Headline\nBody copy");
    assert_eq!(s.history.len(), 1);
    let fitted = wire(&s);
    let doc = crate::project::decode(&fitted).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), fitted);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), fitted);
}

#[test]
fn craft_type_fit_headline_native_menu_refuses_point_type_and_locked_text() {
    let mut s = editor();
    let before = wire(&s);
    let mut seat = Seat::new();
    seat.click(&mut s, "Type");
    seat.invoke(&mut s, "Fit headline");
    assert_eq!(s.status, "Select area type");
    assert_eq!(s.history.len(), 0);
    assert_eq!(wire(&s), before);

    let mut s = area_editor();
    let hit = s.selection[0];
    s.doc.find_shape_mut(hit.0, hit.1).unwrap().locked = true;
    let before = wire(&s);
    let mut seat = Seat::new();
    seat.click(&mut s, "Type");
    assert!(keys::named(&seat.output, "Fit headline").1.is_disabled());
    assert_eq!(s.history.len(), 0);
    assert_eq!(wire(&s), before);
}

#[test]
fn craft_type_placeholder_native_type_menu_replaces_one_undo_and_reopens() {
    let mut s = editor();
    let before = wire(&s);
    let mut seat = Seat::new();
    seat.click(&mut s, "Type");
    seat.invoke(&mut s, "Fill with placeholder text");
    let text = s.selected_type().unwrap();
    assert_eq!(
        text.content,
        "Placeholder text sits here while the frame is still open."
    );
    assert_eq!(
        text.character_style(0).fill,
        Some(Fill::Solid(crate::color::Rgba::rgb(220, 35, 70)))
    );
    assert_eq!(s.history.len(), 1);
    let filled = wire(&s);
    let doc = crate::project::decode(&filled).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), filled);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), filled);
}

#[test]
fn craft_type_placeholder_native_menu_stays_disabled_on_locked_text() {
    let mut s = editor();
    let hit = s.selection[0];
    s.doc.find_shape_mut(hit.0, hit.1).unwrap().locked = true;
    let before = wire(&s);
    let mut seat = Seat::new();
    seat.click(&mut s, "Type");
    assert!(
        keys::named(&seat.output, "Fill with placeholder text")
            .1
            .is_disabled()
    );
    assert_eq!(s.history.len(), 0);
    assert_eq!(wire(&s), before);
}

fn shown_face(reference: &str) -> (String, String) {
    let info = crate::text::font_inventory::describe(
        &crate::text::font_inventory::Request::Stored(reference.into()),
        reference,
        &std::collections::BTreeSet::new(),
    );
    assert_eq!(info.status, crate::text::font_inventory::Status::Exact);
    (info.family, info.style)
}

#[test]
fn craft_type_font_menu_native_type_menu_applies_one_undo_and_reopens() {
    let mut s = editor();
    s.font_recents = vec!["/usr/share/fonts/liberation/LiberationSans-Regular.ttf".into()];
    let before = wire(&s);
    let mut seat = Seat::new();
    seat.click(&mut s, "Type");
    seat.click(&mut s, "Recent fonts ⏵");
    seat.invoke(&mut s, "Liberation Sans");
    let text = s.selected_type().unwrap();
    assert_eq!(
        text.font,
        "/usr/share/fonts/liberation/LiberationSans-Regular.ttf"
    );
    assert_eq!(
        text.character_style(0).fill,
        Some(Fill::Solid(crate::color::Rgba::rgb(220, 35, 70)))
    );
    assert_eq!(s.history.len(), 1);
    let recent = wire(&s);
    let doc = crate::project::decode(&recent).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), recent);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), recent);

    seat.click(&mut s, "Type");
    seat.invoke(&mut s, "Bold");
    assert_eq!(
        shown_face(&s.selected_type().unwrap().font),
        ("Liberation Sans".into(), "Bold".into())
    );
    assert_eq!(s.history.len(), 2);
    seat.click(&mut s, "Type");
    seat.click(&mut s, "Font ⏵");
    seat.invoke(&mut s, "Liberation Sans");
    assert_eq!(
        shown_face(&s.selected_type().unwrap().font),
        ("Liberation Sans".into(), "Regular".into())
    );
    assert!(s.selected_type().unwrap().font.starts_with("omatype:"));
    assert_eq!(s.history.len(), 3);
    let family = wire(&s);
    s.undo();
    s.undo();
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    s.redo();
    s.redo();
    assert_eq!(wire(&s), family);
}

#[test]
fn craft_type_font_menu_native_menu_stays_disabled_on_locked_text() {
    let mut s = editor();
    let hit = s.selection[0];
    s.doc.find_shape_mut(hit.0, hit.1).unwrap().locked = true;
    let before = wire(&s);
    let mut seat = Seat::new();
    seat.click(&mut s, "Type");
    for name in ["Font ⏵", "Recent fonts ⏵", "Bold", "Italic"] {
        assert!(keys::named(&seat.output, name).1.is_disabled(), "{name}");
    }
    assert_eq!(s.history.len(), 0);
    assert_eq!(wire(&s), before);
}

fn ink_union(run: &TypeRun) -> crate::geom::Bounds {
    let mut bounds: Option<crate::geom::Bounds> = None;
    for point in crate::text::shape(run).into_iter().flatten() {
        match &mut bounds {
            Some(bounds) => bounds.union_pt(point),
            None => bounds = Some(crate::geom::Bounds::from_pt(point)),
        }
    }
    bounds.expect("text ink")
}

#[test]
fn craft_type_orientation_native_menu_applies_one_undo() {
    let mut s = editor();
    assert_eq!(super::selected_orientation(&s), Some(false));
    let before = wire(&s);
    let mut seat = Seat::new();
    seat.click(&mut s, "Type");
    seat.click(&mut s, "Type orientation ⏵");
    let horizontal = keys::named(&seat.output, "Horizontal").1;
    let vertical = keys::named(&seat.output, "Vertical").1;
    assert_eq!(horizontal.toggled(), Some(egui::accesskit::Toggled::True));
    assert_eq!(vertical.toggled(), Some(egui::accesskit::Toggled::False));
    seat.invoke(&mut s, "Vertical");
    let text = s.selected_type().unwrap();
    assert!(text.vertical);
    assert_eq!(
        text.character_style(0).fill,
        Some(Fill::Solid(crate::color::Rgba::rgb(220, 35, 70)))
    );
    let tall = ink_union(&text);
    assert!(
        tall.height() > tall.width(),
        "{} x {}",
        tall.width(),
        tall.height()
    );
    assert_eq!(s.history.len(), 1);
    assert_eq!(super::selected_orientation(&s), Some(true));
    let saved = wire(&s);
    let doc = crate::project::decode(&saved).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), saved);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), saved);
    seat.click(&mut s, "Type");
    seat.click(&mut s, "Type orientation ⏵");
    assert_eq!(
        keys::named(&seat.output, "Vertical").1.toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    seat.invoke(&mut s, "Horizontal");
    assert!(!s.selected_type().unwrap().vertical);
    let wide = ink_union(&s.selected_type().unwrap());
    assert!(wide.width() > wide.height());
    assert_eq!(s.history.len(), 2);
    assert_eq!(super::selected_orientation(&s), Some(false));

    let mut other = TypeRun {
        content: "Side".into(),
        origin: Pt::new(240., 40.),
        px: 20.,
        vertical: true,
        font: s.selected_type().unwrap().font.clone(),
        ..Default::default()
    };
    other.layout = None;
    let shape = Shape::new(Geom::Text(other), Style::default());
    let other_id = shape.id;
    s.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    s.selection = vec![s.selection[0], (0, other_id)];
    assert_eq!(super::selected_orientation(&s), None);
    s.selection.clear();
    assert_eq!(super::selected_orientation(&s), None);
}

#[test]
fn craft_type_orientation_native_menu_stays_disabled_on_locked_text() {
    let mut s = editor();
    let hit = s.selection[0];
    s.doc.find_shape_mut(hit.0, hit.1).unwrap().locked = true;
    let before = wire(&s);
    let mut seat = Seat::new();
    seat.click(&mut s, "Type");
    assert!(
        keys::named(&seat.output, "Type orientation ⏵")
            .1
            .is_disabled()
    );
    assert_eq!(s.history.len(), 0);
    assert_eq!(wire(&s), before);
}
