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
        let bounds = keys::named(&self.output, label).1.bounds().unwrap();
        let point = egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.) as f32,
            ((bounds.y0 + bounds.y1) / 2.) as f32,
        );
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
        self.invoke(studio, "Group from Layers…");
        assert!(is_open(&self.ctx));
        keys::focused_role(
            &self.ctx,
            &self.output,
            "Name",
            egui::accesskit::Role::TextInput,
        );
    }
    fn replace(&mut self, studio: &mut Studio, value: &str) {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, "Name", |events| self.frame(studio, events));
        self.frame(studio, keys::key(Key::A, Modifiers::COMMAND));
        self.frame(studio, vec![Event::Paste(value.into())]);
        self.frame(studio, vec![]);
    }
}

fn fixture() -> Studio {
    let mut studio = Studio::new();
    studio.show_welcome = false;
    studio.persona = Persona::Pixel;
    studio.doc = Document::new("Groups", 40., 30., 96.);
    studio.doc.layers = vec![Layer::vector("Artwork"), Layer::group("Empty")];
    studio.active_layer = Some(0);
    studio.history.clear();
    studio.select_layer_rows(0, false, false);
    studio.select_layer_rows(1, false, true);
    studio
}

fn state(studio: &Studio) -> String {
    crate::project::encode(&studio.doc).unwrap()
}

#[test]
fn craft_group_dialog_native_name_cancel_group_and_stale_selection_keep_history_honest() {
    let mut studio = fixture();
    let before = state(&studio);
    let mut seat = Seat::new();
    seat.open(&mut studio);
    seat.replace(&mut studio, "Pair Ω");
    assert_eq!(state(&studio), before);
    assert_eq!(studio.history.len(), 0);
    seat.invoke(&mut studio, "Cancel");
    assert!(!is_open(&seat.ctx));
    assert_eq!(state(&studio), before);
    seat.open(&mut studio);
    seat.replace(&mut studio, "Pair Ω");
    studio.select_layer_rows(0, false, false);
    seat.invoke(&mut studio, "Group");
    assert!(is_open(&seat.ctx));
    assert_eq!(studio.history.len(), 0);
    assert_eq!(state(&studio), before);
    seat.invoke(&mut studio, "Cancel");
    studio.select_layer_rows(0, false, false);
    studio.select_layer_rows(1, false, true);
    seat.open(&mut studio);
    seat.replace(&mut studio, "Pair Ω");
    seat.invoke(&mut studio, "Group");
    assert!(!is_open(&seat.ctx));
    assert_eq!(studio.history.len(), 1);
    let group = studio
        .doc
        .layers
        .iter()
        .find(|layer| layer.id == studio.selected_layer.unwrap())
        .unwrap();
    assert_eq!(group.name, "Pair Ω");
    assert!(group.is_group);
    let grouped = state(&studio);
    studio.undo();
    assert_eq!(state(&studio), before);
    studio.redo();
    assert_eq!(state(&studio), grouped);
}
