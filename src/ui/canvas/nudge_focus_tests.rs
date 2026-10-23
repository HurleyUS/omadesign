use crate::{
    app::Studio,
    document::{Document, Layer, Shape, Style},
    geom::{Geom, Pt},
    tools::Tool,
};
use eframe::egui::{self, Event, Id, Key, Modifiers};

fn press(key: Key) -> Vec<Event> {
    [true, false].into_iter().map(|pressed| Event::Key {
        key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE,
    }).collect()
}

fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) {
    let mut output = ctx.run_ui(egui::RawInput {
        events,
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640., 480.))),
        ..Default::default()
    }, |ui| {
        studio.handle_shortcuts(ui.ctx());
        // Real focusable neighbours above the canvas: native arrow navigation lands on these.
        let _ = ui.button("Before canvas");
        let _ = ui.button("After canvas");
        super::show(ui, studio);
    });
    output.textures_delta.clear();
}

fn origin(studio: &Studio) -> Pt {
    let (li, id) = studio.selection[0];
    match studio.doc.layers[li].kind.shapes().unwrap().iter().find(|s| s.id == id).unwrap().geom {
        Geom::Rect { origin, .. } => origin,
        _ => unreachable!(),
    }
}

/// Regression: after the first arrow nudge, egui arrow navigation moved focus off the
/// canvas onto UI buttons, so later presses cycled buttons instead of nudging.
#[test]
fn repeated_arrow_nudges_keep_moving_selection_and_canvas_focus() {
    let ctx = egui::Context::default();
    let mut studio = Studio::new();
    studio.show_welcome = false;
    studio.doc = Document::new("Nudge", 400., 300., 96.);
    studio.doc.layers = vec![Layer::vector("Art")];
    let shape = Shape::new(Geom::Rect { origin: Pt::new(50., 60.), size: Pt::new(40., 30.), radius: 0. }, Style::default());
    let id = shape.id;
    studio.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    studio.active_layer = Some(0);
    studio.selection = vec![(0, id)];
    studio.tool = Tool::Select;
    studio.mark();
    frame(&ctx, &mut studio, vec![]);
    ctx.memory_mut(|m| m.request_focus(Id::new("studio-canvas")));
    frame(&ctx, &mut studio, vec![]);
    assert_eq!(ctx.memory(|m| m.focused()), Some(Id::new("studio-canvas")));
    let start = origin(&studio);
    let mut last = start;
    for (n, key) in [Key::ArrowUp, Key::ArrowUp, Key::ArrowUp, Key::ArrowLeft, Key::ArrowRight, Key::ArrowDown].into_iter().enumerate() {
        frame(&ctx, &mut studio, press(key));
        frame(&ctx, &mut studio, vec![]);
        assert_eq!(ctx.memory(|m| m.focused()), Some(Id::new("studio-canvas")), "press {n} moved focus off the canvas");
        let now = origin(&studio);
        assert_ne!(now, last, "press {n} did not nudge the selection");
        last = now;
    }
    assert!(last.x == start.x && last.y < start.y);
}
