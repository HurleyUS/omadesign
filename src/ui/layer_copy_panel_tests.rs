use super::*;
use crate::document::{Document, Layer};
use crate::ui::welcome::keyboard_tests as keys;
use eframe::egui::{self, Event, Key, Modifiers, PointerButton};

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
        Self {
            ctx,
            time: 0.,
            output: Default::default(),
        }
    }
    fn frame(&mut self, s: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        self.time += 0.2;
        let mut output = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(1000., 900.),
                )),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ui| {
                Panel::left("copy-layers-fixture")
                    .exact_size(320.)
                    .show(ui, |ui| layers_content(ui, s));
                crate::ui::layer_duplicate::show(ui.ctx(), s);
            },
        );
        output.textures_delta.clear();
        self.output = output.clone();
        output
    }
}
fn fixture() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Pixel;
    s.doc = Document::new("Layer panel copy", 16., 16., 96.);
    let group = Layer::group("Pair");
    let mut child = Layer::raster("Child", 16, 16).unwrap();
    child.parent = Some(group.id);
    s.doc.layers = vec![child, group, Layer::vector("Other")];
    s.activate_layer_tree(1);
    s.history.clear();
    s
}
fn check(s: &mut Studio, before: &str) {
    assert_eq!(s.doc.layers.len(), 5);
    assert_eq!(s.doc.layers[3].name, "Pair copy");
    assert_eq!(s.doc.layers[2].parent, Some(s.doc.layers[3].id));
    assert_eq!(s.active_layer, Some(3));
    assert_eq!(s.selected_layer, Some(s.doc.layers[3].id));
    let after = crate::project::encode(&s.doc).unwrap();
    assert_eq!(s.history.len(), 1);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
        after
    );
    s.undo();
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    s.redo();
    assert_eq!(crate::project::encode(&s.doc).unwrap(), after);
}

#[test]
fn craft_layer_copy_bottom_icon_uses_the_explicit_selected_group() {
    let mut s = fixture();
    let before = crate::project::encode(&s.doc).unwrap();
    let mut seat = Seat::new();
    for _ in 0..3 {
        seat.frame(&mut s, vec![]);
    }
    let ctx = seat.ctx.clone();
    keys::tab_to(&ctx, "Duplicate layer", |events| seat.frame(&mut s, events));
    seat.frame(&mut s, keys::key(Key::Space, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    check(&mut s, &before);
}

#[test]
fn craft_layer_copy_row_context_uses_its_owner_instead_of_the_active_layer() {
    let mut s = fixture();
    s.activate_layer_tree(2);
    let other = s.doc.layers[2].id;
    let before = crate::project::encode(&s.doc).unwrap();
    let mut seat = Seat::new();
    for _ in 0..3 {
        seat.frame(&mut s, vec![]);
    }
    let bounds = keys::named(&seat.output, "Pair").1.bounds().unwrap();
    let pos = egui::pos2(
        ((bounds.x0 + bounds.x1) / 2.) as f32,
        ((bounds.y0 + bounds.y1) / 2.) as f32,
    );
    for pressed in [true, false] {
        seat.frame(
            &mut s,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Secondary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    for _ in 0..3 {
        seat.frame(&mut s, vec![]);
    }
    assert_eq!(s.active_layer.map(|i| s.doc.layers[i].id), Some(other));
    let ctx = seat.ctx.clone();
    keys::tab_to(&ctx, "Duplicate layer…", |events| seat.frame(&mut s, events));
    seat.frame(&mut s, keys::key(Key::Space, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    keys::tab_to(&ctx, "Duplicate", |events| seat.frame(&mut s, events));
    seat.frame(&mut s, keys::key(Key::Space, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    check(&mut s, &before);
}
