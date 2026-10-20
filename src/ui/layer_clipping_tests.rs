use crate::ui::welcome::keyboard_tests as keys;
use crate::{
    app::Studio,
    document::{Document, Layer},
    tools::Persona,
};
use eframe::egui::{self, Event, Key, Modifiers};

struct Seat {
    ctx: egui::Context,
    output: egui::FullOutput,
}
impl Seat {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        Self {
            ctx,
            output: Default::default(),
        }
    }
    fn frame(&mut self, s: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        self.output = keys::render(&self.ctx, events, |ui| crate::ui::run(ui, s));
        self.output.clone()
    }
    fn click(&mut self, s: &mut Studio, name: &str, button: egui::PointerButton, mods: Modifiers) {
        for _ in 0..3 {
            self.frame(s, vec![]);
        }
        let b = keys::named(&self.output, name).1.bounds().unwrap();
        let p = egui::pos2(((b.x0 + b.x1) / 2.) as f32, ((b.y0 + b.y1) / 2.) as f32);
        for pressed in [true, false] {
            self.frame(
                s,
                vec![
                    Event::ModifiersChanged(mods),
                    Event::PointerMoved(p),
                    Event::PointerButton {
                        pos: p,
                        button,
                        pressed,
                        modifiers: mods,
                    },
                ],
            );
        }
        self.frame(s, vec![]);
    }
    fn invoke(&mut self, s: &mut Studio, name: &str) {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, name, |events| self.frame(s, events));
        self.frame(s, keys::key(Key::Space, Modifiers::NONE));
        self.frame(s, vec![]);
        self.frame(s, vec![]);
    }
    fn menu(&mut self, s: &mut Studio, name: &str) {
        self.click(s, "Layer", egui::PointerButton::Primary, Modifiers::NONE);
        self.invoke(s, name);
    }
}
fn fixture() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Pixel;
    s.doc = Document::new("Clipping UI", 4., 1., 96.);
    s.doc.transparent = true;
    s.doc.workspace = Some(Persona::Pixel);
    let mut lower = Layer::raster("Clip base", 4, 1).unwrap();
    lower
        .kind
        .pixels_mut()
        .unwrap()
        .replace_bytes([0, 0, 255, 128].repeat(4))
        .unwrap();
    let mut upper = Layer::raster("Clip upper", 4, 1).unwrap();
    upper
        .kind
        .pixels_mut()
        .unwrap()
        .replace_bytes([255, 0, 0, 255].repeat(4))
        .unwrap();
    s.doc.layers = vec![lower, upper];
    s.activate_layer_tree(1);
    s.history.clear();
    s
}
fn state(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}

#[test]
fn craft_clipping_native_layer_menu_create_release_keys_undo_and_reopen() {
    let mut s = fixture();
    let before = state(&s);
    let mut seat = Seat::new();
    seat.menu(&mut s, "Create Clipping Mask");
    assert!(s.doc.layers[1].clipped);
    assert_eq!(s.history.len(), 1);
    assert_eq!(
        crate::compositor::render_export(&s.doc, 1).unwrap().data(),
        [128, 0, 0, 128].repeat(4)
    );
    let after = state(&s);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
        after
    );
    seat.frame(&mut s, keys::key(Key::F8, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    assert_eq!(
        seat.ctx.memory(|m| m.focused()),
        Some(egui::Id::new("studio-canvas"))
    );
    seat.frame(&mut s, keys::key(Key::Z, Modifiers::COMMAND));
    seat.frame(&mut s, vec![]);
    assert_eq!(state(&s), before);
    seat.frame(&mut s, keys::key(Key::Y, Modifiers::COMMAND));
    seat.frame(&mut s, vec![]);
    assert_eq!(state(&s), after);
    seat.menu(&mut s, "Release Clipping Mask");
    assert!(!s.doc.layers[1].clipped);
    assert_eq!(s.history.len(), 2);
}

#[test]
fn craft_clipping_native_canvas_chord_and_real_rename_field_keep_distinct_owners() {
    let mut s = fixture();
    let mut seat = Seat::new();
    for _ in 0..3 {
        seat.frame(&mut s, vec![]);
    }
    seat.frame(&mut s, keys::key(Key::F8, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    let mods = Modifiers {
        alt: true,
        ..Modifiers::COMMAND
    };
    seat.frame(&mut s, keys::key(Key::G, mods));
    seat.frame(&mut s, vec![]);
    assert!(s.doc.layers[1].clipped);
    assert_eq!(s.history.len(), 1);
    seat.frame(&mut s, keys::key(Key::G, mods));
    seat.frame(&mut s, vec![]);
    assert!(!s.doc.layers[1].clipped);
    assert_eq!(s.history.len(), 2);
    seat.click(
        &mut s,
        "Clip upper",
        egui::PointerButton::Secondary,
        Modifiers::NONE,
    );
    seat.invoke(&mut s, "Rename");
    keys::focused_role(
        &seat.ctx,
        &seat.output,
        "Rename layer",
        egui::accesskit::Role::TextInput,
    );
    let before = state(&s);
    let history = s.history.len();
    seat.frame(&mut s, keys::key(Key::G, mods));
    seat.frame(&mut s, vec![]);
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), history);
    seat.frame(&mut s, keys::key(Key::Escape, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    assert_eq!(state(&s), before);
}

#[test]
fn craft_clipping_row_context_owner_and_shift_selected_stack_follow_actual_rows() {
    let mut s = fixture();
    s.activate_layer_tree(0);
    let mut seat = Seat::new();
    seat.click(
        &mut s,
        "Clip upper",
        egui::PointerButton::Secondary,
        Modifiers::NONE,
    );
    assert_eq!(s.active_layer, Some(0));
    seat.invoke(&mut s, "Create Clipping Mask");
    assert!(s.doc.layers[1].clipped);
    assert!(!s.doc.layers[0].clipped);
    assert_eq!(s.history.len(), 1);
    let mut s = fixture();
    let mut seat = Seat::new();
    seat.click(
        &mut s,
        "Clip base",
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    seat.click(
        &mut s,
        "Clip upper",
        egui::PointerButton::Primary,
        Modifiers::SHIFT,
    );
    assert!(s.selection.iter().any(|p| p.0 == 0) && s.selection.iter().any(|p| p.0 == 1));
    let before = state(&s);
    seat.menu(&mut s, "Create Clipping Mask");
    assert_eq!(
        s.doc.layers.iter().map(|l| l.clipped).collect::<Vec<_>>(),
        [false, true]
    );
    assert_eq!(s.history.len(), 1);
    s.undo();
    assert_eq!(state(&s), before);

    seat.click(
        &mut s,
        "Clip base",
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    seat.click(
        &mut s,
        "Clip upper",
        egui::PointerButton::Primary,
        Modifiers::COMMAND,
    );
    assert_eq!(s.selected_layer_rows(), [0, 1]);
    assert!(
        s.pixel_sel.is_none(),
        "Command-click on the name selects rows, not thumbnail alpha"
    );
    seat.click(
        &mut s,
        "Clip upper",
        egui::PointerButton::Primary,
        Modifiers::COMMAND,
    );
    assert_eq!(s.selected_layer_rows(), [0]);

    let mut s = fixture();
    let lower = Layer::group("Base group");
    let upper = Layer::group("Clipped group");
    s.doc.layers[0].parent = Some(lower.id);
    s.doc.layers[1].parent = Some(upper.id);
    s.doc.layers.insert(1, lower);
    s.doc.layers.push(upper);
    s.layer_expanded.clear();
    s.activate_layer_tree(1);
    let mut seat = Seat::new();
    seat.click(
        &mut s,
        "Base group",
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    seat.click(
        &mut s,
        "Clipped group",
        egui::PointerButton::Primary,
        Modifiers::SHIFT,
    );
    assert_eq!(
        s.selected_layer_rows(),
        [1, 3],
        "Collapsed descendants are not selected rows"
    );
    let before = state(&s);
    seat.menu(&mut s, "Create Clipping Mask");
    assert_eq!(
        s.doc.layers.iter().map(|l| l.clipped).collect::<Vec<_>>(),
        [false, false, false, true]
    );
    assert_eq!(
        crate::compositor::render_export(&s.doc, 1).unwrap().data(),
        [128, 0, 0, 128].repeat(4)
    );
    assert_eq!(s.history.len(), 1);
    let after = state(&s);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
        after
    );
    s.undo();
    assert_eq!(state(&s), before);
}
