use super::*;
use crate::{
    document::{Document, Layer},
    tools::Persona,
    ui::welcome::keyboard_tests as keys,
};
use egui::{Event, Key, Modifiers};
use std::time::{Duration, Instant};

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
    fn click(&mut self, s: &mut Studio, name: &str) {
        for _ in 0..3 {
            self.frame(s, vec![]);
        }
        let b = keys::named(&self.output, name).1.bounds().unwrap();
        let p = egui::pos2(((b.x0 + b.x1) / 2.) as f32, ((b.y0 + b.y1) / 2.) as f32);
        for pressed in [true, false] {
            self.frame(
                s,
                vec![
                    Event::PointerMoved(p),
                    Event::PointerButton {
                        pos: p,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        self.frame(s, vec![]);
    }
    fn menu(&mut self, s: &mut Studio, name: &str) {
        self.click(s, "Layer");
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, name, |events| self.frame(s, events));
        self.frame(s, keys::key(Key::Space, Modifiers::NONE));
        self.frame(s, vec![]);
    }
    fn wait(&mut self, s: &mut Studio) {
        let until = Instant::now() + Duration::from_secs(20);
        while is_open(&self.ctx) {
            self.frame(s, vec![]);
            if let Some(error) = self
                .ctx
                .data(|d| d.get_temp::<State>(Id::new(ID)))
                .and_then(|s| s.error)
            {
                panic!("actual merge worker: {error}");
            }
            assert!(Instant::now() < until, "actual merge worker timeout");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!super::super::jobs::is_running::<Prepared>(&self.ctx, ID));
    }
}
fn fixture() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Pixel;
    s.doc = Document::new("Merge UI", 16., 12., 96.);
    s.doc.workspace = Some(Persona::Pixel);
    s.doc.transparent = true;
    let mut a = Layer::raster("Lower", 2, 2).unwrap();
    a.kind
        .pixels_mut()
        .unwrap()
        .replace_bytes([30, 90, 150, 255].repeat(4))
        .unwrap();
    let mut b = Layer::raster("Upper", 2, 2).unwrap();
    b.kind
        .pixels_mut()
        .unwrap()
        .replace_bytes([210, 70, 30, 128].repeat(4))
        .unwrap();
    s.doc.layers = vec![a, b];
    s.activate_layer_tree(1);
    s.history.clear();
    s
}
fn state(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn check(s: &mut Studio, before: &str) {
    assert_eq!(s.history.len(), 1);
    let after = state(s);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
        after
    );
    s.undo();
    assert_eq!(state(s), before);
    s.redo();
    assert_eq!(state(s), after);
}

#[test]
fn craft_merge_native_layer_menu_runs_actual_down_group_visible_and_flatten_workers() {
    for label in [
        "Merge Down",
        "Merge Group",
        "Merge Visible",
        "Flatten Image",
    ] {
        let mut s = fixture();
        if label == "Merge Group" {
            let group = Layer::group("Pair");
            for l in &mut s.doc.layers {
                l.parent = Some(group.id);
            }
            s.doc.layers.push(group);
            s.activate_layer_tree(2);
        }
        let before = state(&s);
        let expected =
            crate::compositor::render_export_at(&s.doc, 1., 0., label != "Flatten Image").unwrap();
        let mut seat = Seat::new();
        seat.menu(&mut s, label);
        seat.wait(&mut s);
        assert_eq!(s.doc.layers.len(), 1, "{label}");
        assert_eq!(
            crate::compositor::render_export_at(&s.doc, 1., 0., label != "Flatten Image")
                .unwrap()
                .data(),
            expected.data(),
            "{label}"
        );
        check(&mut s, &before);
    }
}

#[test]
fn craft_merge_canvas_shortcuts_keep_field_keys_and_dispatch_actual_pixel_merge() {
    let mut s = fixture();
    let before = state(&s);
    let mut seat = Seat::new();
    for _ in 0..3 {
        seat.frame(&mut s, vec![]);
    }
    seat.frame(&mut s, keys::key(Key::F8, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    assert_eq!(
        seat.ctx.memory(|m| m.focused()),
        Some(Id::new("studio-canvas"))
    );
    seat.frame(&mut s, keys::key(Key::E, Modifiers::COMMAND));
    seat.wait(&mut s);
    assert_eq!(s.doc.layers.len(), 1);
    check(&mut s, &before);
    s.undo();
    let before = state(&s);
    crate::ui::layer_duplicate::open(&seat.ctx, &mut s, 0);
    seat.frame(&mut s, vec![]);
    seat.frame(&mut s, keys::key(Key::E, Modifiers::COMMAND));
    seat.frame(&mut s, vec![]);
    assert!(!is_open(&seat.ctx));
    assert_eq!(state(&s), before);
    seat.frame(&mut s, keys::key(Key::Escape, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
}

#[test]
fn craft_merge_actual_cancel_and_stale_document_retire_worker_before_commit() {
    for stale in [false, true] {
        let mut s = fixture();
        let before = state(&s);
        let mut seat = Seat::new();
        let id = s.doc.layers[1].id;
        start(&seat.ctx, &mut s, Kind::Down(id));
        if stale {
            s.mark();
            seat.frame(&mut s, vec![]);
        } else {
            seat.frame(&mut s, keys::key(Key::Escape, Modifiers::NONE));
        }
        seat.frame(&mut s, vec![]);
        assert!(!is_open(&seat.ctx));
        assert!(!super::super::jobs::is_running::<Prepared>(&seat.ctx, ID));
        assert_eq!(state(&s), before);
        assert_eq!(s.history.len(), 0);
        for _ in 0..5 {
            seat.frame(&mut s, vec![]);
        }
        assert_eq!(state(&s), before);
        assert_eq!(s.history.len(), 0);
    }
}
