use crate::{
    app::Studio,
    document::{Document, Layer, Shape, Style},
    geom::{Geom, Pt},
    ui::{self, icons::ph},
};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, Rect};
fn fixture() -> (egui::Context, Studio) {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);
    let mut s = Studio::new();
    s.show_welcome = false;
    s.show_key_hud = false;
    s.need_fit = false;
    s.doc = Document::new("UI align", 800., 600., 72.);
    s.doc.layers = vec![Layer::vector("Artwork")];
    s.selection.clear();
    s.active_layer = Some(0);
    for (x, y) in [(60., 60.), (270., 160.), (600., 400.)] {
        let sh = Shape::new(
            Geom::Rect {
                origin: Pt::new(x, y),
                size: Pt::new(30., 40.),
                radius: 0.,
            },
            Style::default(),
        );
        s.selection.push((0, sh.id));
        s.doc.layers[0].kind.shapes_mut().unwrap().push(sh);
    }
    // Align is its own dock tab beside Transform and Pathfinder; bring it forward.
    assert!(crate::ui::panel_workspace::open(&ctx, &mut s, "align"));
    (ctx, s)
}
fn frame(
    ctx: &egui::Context,
    s: &mut Studio,
    width: f32,
    events: Vec<Event>,
) -> Vec<(String, Pos2)> {
    s.last_input = std::time::Instant::now();
    s.dirty = false;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 1000.))),
            events,
            ..Default::default()
        },
        |ui| crate::ui::run(ui, s),
    );
    output.textures_delta.clear();
    fn visit(shape: &egui::Shape, labels: &mut Vec<(String, Pos2)>) {
        match shape {
            egui::Shape::Text(t) => {
                labels.push((t.galley.job.text.clone(), t.pos + t.galley.size() * 0.5))
            }
            egui::Shape::Vec(v) => {
                for shape in v {
                    visit(shape, labels)
                }
            }
            _ => {}
        }
    }
    let mut labels = vec![];
    for sh in output.shapes {
        visit(&sh.shape, &mut labels);
    }
    labels
}
fn click(ctx: &egui::Context, s: &mut Studio, label: &str, modifiers: Modifiers) {
    let labels = frame(ctx, s, 1600., vec![]);
    let p = labels
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("missing {label}"))
        .1;
    for pressed in [true, false] {
        frame(
            ctx,
            s,
            1600.,
            vec![
                Event::ModifiersChanged(modifiers),
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers,
                },
            ],
        );
    }
}
/// Press a point through real pointer events.
fn click_at(ctx: &egui::Context, s: &mut Studio, p: Pos2, modifiers: Modifiers) {
    for pressed in [true, false] {
        frame(ctx, s, 1600., vec![
            Event::ModifiersChanged(modifiers),
            Event::PointerMoved(p),
            Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers },
        ]);
    }
}
/// The Align pane's icon rows, anchored on its "Align" section header:
/// (order row above the header, align row, distribute row) for one glyph.
fn align_pane_glyph(labels: &[(String, Pos2)], glyph: &str) -> (Vec<Pos2>, Vec<Pos2>) {
    let header = labels
        .iter()
        .filter(|(text, _)| text == "Align")
        .map(|(_, p)| *p)
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .expect("Align pane section header");
    let near: Vec<Pos2> = labels
        .iter()
        .filter(|(text, p)| text == glyph && (p.x - header.x).abs() < 300. && (p.y - header.y).abs() < 120.)
        .map(|(_, p)| *p)
        .collect();
    let (mut above, mut below): (Vec<_>, Vec<_>) = near.into_iter().partition(|p| p.y < header.y);
    above.sort_by(|a, b| a.y.total_cmp(&b.y));
    below.sort_by(|a, b| a.y.total_cmp(&b.y));
    (above, below)
}
#[test]
fn inspector_routes_all_alignment_distribution_and_order_controls() {
    let (ctx, mut s) = fixture();
    for _ in 0..3 {
        frame(&ctx, &mut s, 1600., vec![]);
    }
    // The Align tab draws each alignment once; the centre glyphs are reused
    // by the distribute row beneath it.
    for (glyph, rows) in [
        (ph::ALIGN_LEFT, 1),
        (ph::ALIGN_CENTER_H, 2),
        (ph::ALIGN_RIGHT, 1),
        (ph::ALIGN_TOP, 1),
        (ph::ALIGN_CENTER_V, 2),
        (ph::ALIGN_BOTTOM, 1),
    ] {
        let labels = frame(&ctx, &mut s, 1600., vec![]);
        assert_eq!(labels.iter().filter(|(text, _)| text == glyph).count(), rows, "single Align home");
        let (_, below) = align_pane_glyph(&labels, glyph);
        assert_eq!(below.len(), rows, "Align tab rows for {glyph:?}");
        let before = crate::project::encode(&s.doc).unwrap();
        let h = s.history.len();
        click_at(&ctx, &mut s, below[0], Modifiers::NONE);
        assert_eq!(s.history.len(), h + 1);
        s.undo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    }
    for glyph in [ph::ALIGN_CENTER_H, ph::ALIGN_CENTER_V] {
        let labels = frame(&ctx, &mut s, 1600., vec![]);
        let distribute = align_pane_glyph(&labels, glyph).1[1];
        let h = s.history.len();
        click_at(&ctx, &mut s, distribute, Modifiers::SHIFT);
        assert_eq!(s.history.len(), h + 1);
        s.undo();
    }
    let ids = s.selection.clone();
    s.selection = vec![ids[1]];
    for glyph in [ph::SKIP_FORWARD, ph::SKIP_BACK, ph::ARROW_UP, ph::ARROW_DOWN] {
        let labels = frame(&ctx, &mut s, 1600., vec![]);
        let order = align_pane_glyph(&labels, glyph).0;
        assert_eq!(order.len(), 1, "one order control {glyph:?}");
        let h = s.history.len();
        click_at(&ctx, &mut s, order[0], Modifiers::NONE);
        assert_eq!(s.history.len(), h + 1);
    }
}
#[test]
fn inspector_single_layer_and_shift_align_are_live_and_arrange_menu_is_gone() {
    let (ctx, mut s) = fixture();
    for _ in 0..3 {
        frame(&ctx, &mut s, 1600., vec![]);
    }
    let labels = frame(&ctx, &mut s, 1600., vec![]);
    assert!(
        !labels
            .iter()
            .any(|(text, p)| text == "Arrange" && p.y < 50.)
    );
    let ids = s.selection.clone();
    s.selection.truncate(1);
    click(&ctx, &mut s, ph::ALIGN_RIGHT, Modifiers::NONE);
    assert_eq!(
        s.doc.find_shape(0, ids[0].1).unwrap().world_bbox().max.x,
        800.
    );
    s.undo();
    s.activate_layer_tree(0);
    click(&ctx, &mut s, ph::ALIGN_LEFT, Modifiers::NONE);
    assert_eq!(
        s.doc.find_shape(0, ids[0].1).unwrap().world_bbox().min.x,
        0.
    );
    s.undo();
    s.selection = ids.clone();
    s.selected_layer = None;
    click(&ctx, &mut s, ph::ALIGN_CENTER_H, Modifiers::SHIFT);
    for &(li, id) in &ids {
        assert_eq!(
            s.doc.find_shape(li, id).unwrap().world_bbox().center().x,
            400.
        );
    }
    s.undo();
    s.selection.clear();
    s.selected_layer = None;
    let h = s.history.len();
    click(&ctx, &mut s, ph::ALIGN_LEFT, Modifiers::NONE);
    assert_eq!(s.history.len(), h);
    frame(&ctx, &mut s, 900., vec![]);
    let labels = frame(&ctx, &mut s, 900., vec![]);
    let p = labels.iter().find(|(text, _)| text == "Menu").unwrap().1;
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut s,
            900.,
            vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    let labels = frame(&ctx, &mut s, 900., vec![]);
    assert!(
        !labels
            .iter()
            .any(|(text, p)| text == "Arrange" && p.x < 600.)
    );
    frame(
        &ctx,
        &mut s,
        900.,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
}

#[test]
fn ordering_shortcuts_work_in_every_drawing_persona_without_arrange_menu() {
    for persona in [crate::tools::Persona::Design,crate::tools::Persona::Layout,crate::tools::Persona::Pixel,crate::tools::Persona::Motion] {
        let (ctx,mut s)=fixture();s.persona=persona;
        let id=s.selection[1].1;s.selection=vec![(0,id)];
        for (key,shift,expected) in [(Key::CloseBracket,false,2),(Key::OpenBracket,true,0),(Key::OpenBracket,false,0),(Key::CloseBracket,true,2)] {
            let modifiers=Modifiers {ctrl:true,command:true,shift,..Modifiers::NONE};
            let mut output=ctx.run_ui(egui::RawInput {events:vec![Event::Key {key,physical_key:Some(key),pressed:true,repeat:false,modifiers}],..Default::default()},|ui|s.handle_shortcuts(ui.ctx()));
            output.textures_delta.clear();assert_eq!(s.doc.layers[0].kind.shapes().unwrap()[expected].id,id,"{persona:?} {key:?} shift={shift}");
        }
    }
}
