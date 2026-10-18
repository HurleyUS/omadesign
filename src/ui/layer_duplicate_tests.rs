use super::*;
use crate::{
    document::{Document, Layer},
    tools::Persona,
    ui::welcome::keyboard_tests as keys,
};
use egui::{Event, Key, Modifiers};

struct Seat {
    ctx: egui::Context,
    output: egui::FullOutput,
}
impl Seat {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        Self {
            ctx,
            output: Default::default(),
        }
    }
    fn frame(&mut self, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        self.output = keys::render(&self.ctx, events, |ui| crate::ui::run(ui, studio));
        self.output.clone()
    }
    fn invoke(&mut self, studio: &mut Studio, label: &str) {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, label, |events| self.frame(studio, events));
        self.frame(studio, keys::key(Key::Space, Modifiers::NONE));
        self.frame(studio, vec![]);
    }
    fn click(&mut self, studio: &mut Studio, label: &str) {
        self.frame(studio, vec![]);
        let b = keys::named(&self.output, label).1.bounds().unwrap();
        let point = egui::pos2(((b.x0 + b.x1) / 2.) as f32, ((b.y0 + b.y1) / 2.) as f32);
        for pressed in [true, false] {
            self.frame(
                studio,
                vec![
                    Event::PointerMoved(point),
                    Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        self.frame(studio, vec![]);
    }
    fn open(&mut self, studio: &mut Studio) {
        for _ in 0..3 {
            self.frame(studio, vec![]);
        }
        self.click(studio, "Layer");
        self.invoke(studio, "Duplicate layer…");
        assert!(is_open(&self.ctx));
        keys::focused_role(
            &self.ctx,
            &self.output,
            "Layer name",
            egui::accesskit::Role::TextInput,
        );
    }
    fn replace(&mut self, studio: &mut Studio, label: &str, value: &str) {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, label, |events| self.frame(studio, events));
        self.frame(studio, keys::key(Key::A, Modifiers::COMMAND));
        self.frame(studio, vec![Event::Paste(value.into())]);
        self.frame(studio, vec![]);
    }
    fn choose(&mut self, studio: &mut Studio, name: &str) {
        self.invoke(studio, "Destination document");
        self.invoke(studio, name);
    }
}
fn fixture() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Pixel;
    s.doc = Document::new("Original", 40., 30., 144.);
    s.doc.layers = vec![Layer::raster("Original layer", 2, 1).unwrap()];
    s.active_layer = Some(0);
    s.history.clear();
    s
}
fn state(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}

#[test]
fn craft_duplicate_dialog_native_name_paste_cancel_and_current_document_accept_undo_reopen() {
    let mut s = fixture();
    let before = state(&s);
    let mut seat = Seat::new();
    seat.open(&mut s);
    seat.replace(&mut s, "Layer name", "Retouched Ω");
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
    seat.invoke(&mut s, "Cancel");
    assert!(!is_open(&seat.ctx));
    assert_eq!(state(&s), before);
    seat.open(&mut s);
    seat.replace(&mut s, "Layer name", "Retouched Ω");
    seat.invoke(&mut s, "Duplicate");
    assert!(!is_open(&seat.ctx));
    assert_eq!(s.doc.layers[1].name, "Retouched Ω");
    assert_eq!(s.history.len(), 1);
    let after = state(&s);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
        after
    );
    s.undo();
    assert_eq!(state(&s), before);
    s.redo();
    assert_eq!(state(&s), after);
}

#[test]
fn craft_duplicate_dialog_native_new_document_choice_name_and_source_retention() {
    let mut s = fixture();
    let before = state(&s);
    let mut seat = Seat::new();
    seat.open(&mut s);
    seat.choose(&mut s, "New document");
    seat.replace(&mut s, "New document name", "Extracted artwork");
    seat.replace(&mut s, "Layer name", "Hero");
    seat.invoke(&mut s, "Duplicate");
    assert!(!is_open(&seat.ctx));
    assert_eq!(s.tab_count(), 2);
    assert_eq!(s.active_tab, 1);
    assert_eq!(s.doc.name, "Extracted artwork");
    assert_eq!(s.doc.layers[0].name, "Hero");
    assert_eq!((s.doc.width, s.doc.height, s.doc.dpi), (40., 30., 144.));
    assert!(s.doc.transparent);
    let copy = state(&s);
    assert_eq!(s.history.len(), 0);
    s.undo();
    assert_eq!(state(&s), copy);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&copy).unwrap()).unwrap(),
        copy
    );
    s.switch_tab(0);
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
}

#[test]
fn craft_duplicate_dialog_native_other_document_choice_stale_refusal_and_escape_preserve_docs() {
    let mut s = fixture();
    let before = state(&s);
    s.new_tab();
    s.doc.name = "Destination".into();
    s.doc.workspace = Some(Persona::Pixel);
    s.mark();
    let destination_before = state(&s);
    s.switch_tab(0);
    let mut seat = Seat::new();
    seat.open(&mut s);
    seat.choose(&mut s, "Destination");
    seat.invoke(&mut s, "Duplicate");
    assert_eq!(s.active_tab, 1);
    assert_eq!(s.doc.layers.last().unwrap().name, "Original layer copy");
    s.undo();
    assert_eq!(state(&s), destination_before);
    s.switch_tab(0);
    assert_eq!(state(&s), before);
    seat.open(&mut s);
    seat.choose(&mut s, "Destination");
    s.switch_tab(1);
    s.mark();
    s.switch_tab(0);
    seat.invoke(&mut s, "Duplicate");
    assert!(is_open(&seat.ctx));
    assert_eq!(s.active_tab, 0);
    assert_eq!(state(&s), before);
    assert!(s.status.contains("changed or closed"));
    seat.frame(&mut s, keys::key(Key::Escape, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    assert!(!is_open(&seat.ctx));
    assert_eq!(state(&s), before);
}
