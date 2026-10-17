use super::*;
use crate::{
    document::{Cmd, Document, Fill, Layer, Shape, Style},
    geom::{Geom, Pt, TypeRun},
    tools::Persona,
    ui::welcome::keyboard_tests as keys,
};
use egui::{Event, Key, Modifiers, PointerButton, Pos2};

fn editor() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Layout;
    s.doc = Document::new("Native Find Font", 500., 220., 96.);
    s.doc.workspace = Some(Persona::Layout);
    s.doc.layers = vec![Layer::vector("Editable stories")];
    for (i, text) in ["Cat cat catalog", "the CAT"].into_iter().enumerate() {
        let mut run = TypeRun {
            content: text.into(),
            origin: Pt::new(35., 75. + i as f32 * 70.),
            px: 30.,
            font: concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/assets/fonts/EBGaramond.ttf"
            )
            .into(),
            ..Default::default()
        };
        run.set_character_style(0, 3, |span| {
            span.fill = Some(Fill::Solid(crate::color::Rgba::from_hex(0xd6224a)))
        });
        let mut shape = Shape::new(Geom::Text(run), Style::default());
        shape.name = format!("Story {}", i + 1);
        crate::text::fill_contours(&mut shape.geom);
        s.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    }
    let id = s.doc.layers[0].kind.shapes().unwrap()[0].id;
    s.selection = vec![(0, id)];
    s.active_layer = Some(0);
    s.mark();
    s.history.clear();
    s
}
fn wire(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn texts(s: &Studio) -> Vec<String> {
    s.doc
        .layers
        .iter()
        .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
        .filter_map(|s| {
            if let Geom::Text(r) = &s.geom {
                Some(r.content.clone())
            } else {
                None
            }
        })
        .collect()
}
fn emit(name: &str, s: &Studio) {
    if let Some(root) = std::env::var_os("OMADESIGN_FIND_FONT_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join(name), wire(s)).unwrap();
    }
}
struct Seat {
    ctx: egui::Context,
    time: f64,
    out: egui::FullOutput,
    size: egui::Vec2,
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
            out: Default::default(),
            size: egui::vec2(1600., 1000.),
        }
    }
    fn frame(&mut self, s: &mut Studio, mut events: Vec<Event>) -> egui::FullOutput {
        self.time += 0.2;
        let modifiers = events
            .iter()
            .find_map(|e| {
                if let Event::Key { modifiers, .. } = e {
                    Some(*modifiers)
                } else {
                    None
                }
            })
            .unwrap_or_default();
        events.insert(0, Event::ModifiersChanged(modifiers));
        let mut out = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, self.size)),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ui| crate::ui::run(ui, s),
        );
        out.textures_delta.clear();
        self.out = out.clone();
        out
    }
    fn click(&mut self, s: &mut Studio, name: &str) {
        for _ in 0..3 {
            self.frame(s, vec![]);
        }
        let b = keys::named(&self.out, name).1.bounds().unwrap();
        let pos = egui::pos2(((b.x0 + b.x1) * 0.5) as f32, ((b.y0 + b.y1) * 0.5) as f32);
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
    fn type_field(&mut self, s: &mut Studio, name: &str, text: &str) {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, name, |e| self.frame(s, e));
        self.frame(s, keys::key(Key::A, Modifiers::CTRL | Modifiers::COMMAND));
        self.frame(s, vec![Event::Paste(text.into())]);
        self.frame(s, vec![]);
        keys::focused(&self.ctx, &self.out, name);
    }
    fn open(&mut self, s: &mut Studio) {
        self.frame(s, vec![]);
        self.click(s, "Type");
        self.invoke(s, "Find Font…");
        assert!(is_open(&self.ctx));
        self.frame(s, vec![]);
        assert!(self.ctx.memory(|m| m.focused()).is_some());
    }
    fn canvas_focus(&mut self, s: &mut Studio) {
        for _ in 0..3 {
            self.frame(s, vec![]);
        }
        let response = self.ctx.read_response(Id::new("studio-canvas")).unwrap();
        assert_eq!(self.ctx.memory(|m| m.focused()), Some(response.id));
        assert_eq!(
            self.out
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .focus,
            response.id.accesskit_id()
        );
    }
}

#[test]
fn craft_find_font_native_menu_filter_style_selected_change_undo_and_reopen() {
    let mut s = editor();
    let mut seat = Seat::new();
    let before = wire(&s);
    seat.open(&mut s);
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 0);
    let catalog = s.font_catalog();
    let family = catalog
        .families()
        .into_iter()
        .find(|f| f != "EB Garamond" && !catalog.styles(f).is_empty())
        .unwrap();
    seat.type_field(&mut s, "Filter font families", &family);
    let ctx = seat.ctx.clone();
    keys::tab_to(&ctx, "Font family", |e| seat.frame(&mut s, e));
    let options: Vec<_> = catalog
        .families()
        .into_iter()
        .filter(|f| f == &family || f == "EB Garamond")
        .collect();
    let key = if options.first() == Some(&family) {
        Key::Home
    } else {
        Key::End
    };
    seat.frame(&mut s, keys::key(key, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    keys::focused(&seat.ctx, &seat.out, "Font family");
    let chosen = seat.ctx.data(|d| d.get_temp::<Draft>(Id::new(ID))).unwrap();
    assert_eq!(chosen.family, family);
    seat.invoke(&mut s, "Change");
    assert_eq!(s.history.len(), 1);
    assert_ne!(wire(&s), before);
    let shapes = s.doc.layers[0].kind.shapes().unwrap();
    let Geom::Text(first) = &shapes[0].geom else {
        panic!()
    };
    let Geom::Text(second) = &shapes[1].geom else {
        panic!()
    };
    assert!(first.font.starts_with("omatype:"));
    assert!(second.font.ends_with("EBGaramond.ttf"));
    let after = wire(&s);
    let doc = crate::project::decode(&after).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), after);
    assert_eq!(
        crate::compositor::export_png(&doc, 1).unwrap(),
        crate::compositor::export_png(&s.doc, 1).unwrap()
    );
    emit("dialog-after.oma", &s);
    seat.invoke(&mut s, "Done");
    assert!(!is_open(&seat.ctx));
    seat.canvas_focus(&mut s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
}
#[test]
fn craft_find_font_native_find_all_cancel_stale_and_narrow_modal_keep_document_unchanged() {
    let mut s = editor();
    let mut seat = Seat::new();
    let before = wire(&s);
    seat.open(&mut s);
    seat.invoke(&mut s, "Find");
    assert_eq!(s.selection.len(), 2);
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 0);
    seat.invoke(&mut s, "Done");
    seat.canvas_focus(&mut s);
    seat.open(&mut s);
    seat.size = egui::vec2(800., 600.);
    seat.frame(&mut s, vec![]);
    for name in [
        "Font family",
        "Font style",
        "Find",
        "Change",
        "Change All",
        "Done",
    ] {
        let (_, node) = keys::named(&seat.out, name);
        let b = node.bounds().unwrap();
        assert!(
            b.x0 >= 0. && b.y0 >= 0. && b.x1 <= 800. && b.y1 <= 600.,
            "{name}: {b:?}"
        );
    }
    s.canvas_gen = s.canvas_gen.wrapping_add(1);
    seat.frame(&mut s, vec![]);
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 0);
    seat.invoke(&mut s, "Done");
    assert!(!is_open(&seat.ctx));
    seat.canvas_focus(&mut s);
}
