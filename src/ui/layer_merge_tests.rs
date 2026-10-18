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
        self.click_mods(s, name, Modifiers::NONE);
    }
    fn click_mods(&mut self, s: &mut Studio, name: &str, modifiers: Modifiers) {
        for _ in 0..3 {
            self.frame(s, vec![]);
        }
        let b = keys::named(&self.output, name).1.bounds().unwrap();
        let p = egui::pos2(((b.x0 + b.x1) / 2.) as f32, ((b.y0 + b.y1) / 2.) as f32);
        for pressed in [true, false] {
            self.frame(
                s,
                vec![
                    Event::ModifiersChanged(modifiers),
                    Event::PointerMoved(p),
                    Event::PointerButton {
                        pos: p,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers,
                    },
                ],
            );
        }
        self.frame(s, vec![]);
    }
    fn menu(&mut self, s: &mut Studio, name: &str) {
        self.click(s, "Layer");
        self.invoke(s, name);
    }
    fn invoke(&mut self, s: &mut Studio, name: &str) {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, name, |events| self.frame(s, events));
        self.frame(s, keys::key(Key::Space, Modifiers::NONE));
        self.frame(s, vec![]);
        self.frame(s, vec![]);
    }
    fn context(&mut self, s: &mut Studio, name: &str) {
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
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
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
fn craft_merge_row_context_uses_its_concrete_owner_and_rename_keeps_its_native_keys() {
    let mut s = fixture();
    s.activate_layer_tree(0);
    let lower = s.doc.layers[0].id;
    let before = state(&s);
    let mut seat = Seat::new();
    seat.context(&mut s, "Upper");
    assert_eq!(s.active_layer, Some(0));
    seat.invoke(&mut s, "Merge Down");
    seat.wait(&mut s);
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.layers[0].id, lower);
    check(&mut s, &before);
    let mut s = fixture();
    let before = state(&s);
    let mut seat = Seat::new();
    seat.context(&mut s, "Upper");
    seat.invoke(&mut s, "Rename");
    keys::focused_role(
        &seat.ctx,
        &seat.output,
        "Rename layer",
        egui::accesskit::Role::TextInput,
    );
    seat.frame(&mut s, keys::key(Key::E, Modifiers::COMMAND));
    seat.frame(&mut s, vec![]);
    assert!(!is_open(&seat.ctx));
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
    seat.frame(&mut s, keys::key(Key::Escape, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    assert_eq!(state(&s), before);
}

#[test]
fn craft_merge_actual_cancel_and_stale_document_retire_worker_before_commit() {
    for stale in [false, true] {
        let mut s = fixture();
        let before = state(&s);
        let selected = s.selected_layer;
        let active = s.active_layer;
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
        assert_eq!(s.selected_layer, selected);
        assert_eq!(s.active_layer, active);
        for _ in 0..5 {
            seat.frame(&mut s, vec![]);
        }
        assert_eq!(state(&s), before);
        assert_eq!(s.history.len(), 0);
    }
}

#[test]
fn craft_selected_merge_native_rows_menu_and_undo_reopen_use_actual_selection() {
    let mut s = fixture();
    let mut seat = Seat::new();
    seat.click(&mut s, "Lower");
    seat.click_mods(&mut s, "Upper", Modifiers::SHIFT);
    assert_eq!(s.selected_layer_rows(), [0, 1]);
    let before = state(&s);
    let expected = crate::compositor::render_export(&s.doc, 1).unwrap();
    seat.menu(&mut s, "Merge Layers");
    seat.wait(&mut s);
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.layers[0].name, "Upper");
    assert_eq!(
        crate::compositor::render_export(&s.doc, 1).unwrap().data(),
        expected.data()
    );
    check(&mut s, &before);
}

#[test]
fn craft_stamp_native_alt_menu_and_canvas_keys_preserve_real_source_and_field_owners() {
    let mut s = fixture();
    let before = state(&s);
    let original = serde_json::to_value(&s.doc.layers).unwrap();
    let mut seat = Seat::new();
    seat.click(&mut s, "Layer");
    seat.click_mods(&mut s, "Merge Visible", Modifiers::ALT);
    seat.wait(&mut s);
    assert_eq!(s.doc.layers.len(), 3);
    assert_eq!(serde_json::to_value(&s.doc.layers[..2]).unwrap(), original);
    check(&mut s, &before);
    let mut s = fixture();
    let before = state(&s);
    let upper = serde_json::to_value(&s.doc.layers[1]).unwrap();
    let mut seat = Seat::new();
    for _ in 0..3 {
        seat.frame(&mut s, vec![]);
    }
    seat.frame(&mut s, keys::key(Key::F8, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    seat.frame(
        &mut s,
        keys::key(
            Key::E,
            Modifiers {
                alt: true,
                ..Modifiers::COMMAND
            },
        ),
    );
    seat.wait(&mut s);
    assert_eq!(s.doc.layers.len(), 2);
    assert_eq!(serde_json::to_value(&s.doc.layers[1]).unwrap(), upper);
    assert_eq!(s.active_layer, Some(1));
    check(&mut s, &before);
    seat.context(&mut s, "Upper");
    seat.invoke(&mut s, "Rename");
    keys::focused_role(
        &seat.ctx,
        &seat.output,
        "Rename layer",
        egui::accesskit::Role::TextInput,
    );
    let before = state(&s);
    let history = s.history.len();
    seat.frame(
        &mut s,
        keys::key(
            Key::E,
            Modifiers {
                alt: true,
                shift: true,
                ..Modifiers::COMMAND
            },
        ),
    );
    seat.frame(&mut s, vec![]);
    assert!(!is_open(&seat.ctx));
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), history);
}

#[test]
fn craft_content_bounds_actual_merge_menu_canvas_pick_and_native_history_use_painted_size() {
    let mut s = fixture();
    let before = state(&s);
    let mut seat = Seat::new();
    seat.menu(&mut s, "Merge Visible");
    seat.wait(&mut s);
    let after = state(&s);
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(
        s.doc.layers[0].kind.raster_xform().unwrap().1,
        crate::geom::Pt::new(2., 2.)
    );
    seat.frame(&mut s, keys::key(Key::F8, Modifiers::NONE));
    seat.frame(&mut s, keys::key(Key::V, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    assert_eq!(s.tool, crate::tools::Tool::Select);
    let rect = s.canvas_rect.unwrap();
    let point = s.view.world_to_window(
        crate::geom::Pt::new(rect.min.x, rect.min.y),
        crate::geom::Pt::new(0.5, 0.5),
    );
    let p = egui::pos2(point.x, point.y);
    for pressed in [true, false] {
        seat.frame(
            &mut s,
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
    seat.frame(&mut s, vec![]);
    assert_eq!(s.primary(), Some((0, crate::document::RASTER_ID)));
    assert_eq!(
        seat.ctx.memory(|m| m.focused()),
        Some(Id::new("studio-canvas"))
    );
    assert_eq!(state(&s), after);
    assert_eq!(s.history.len(), 1);
    seat.frame(&mut s, keys::key(Key::Z, Modifiers::COMMAND));
    seat.frame(&mut s, vec![]);
    assert_eq!(state(&s), before);
    seat.frame(&mut s, keys::key(Key::Y, Modifiers::COMMAND));
    seat.frame(&mut s, vec![]);
    assert_eq!(state(&s), after);
    let reopened = crate::project::decode(&after).unwrap();
    assert_eq!(
        reopened.layers[0].kind.raster_xform().unwrap().1,
        crate::geom::Pt::new(2., 2.)
    );
}

fn cross_group_ui_fixture() -> Studio {
    let mut s = fixture();
    s.doc = Document::new("Cross-group UI", 6., 1., 96.);
    s.doc.transparent = true;
    s.doc.workspace = Some(Persona::Pixel);
    let mut left = Layer::group("Left group");
    left.pass_through = false;
    let mut right = Layer::group("Right group");
    right.pass_through = false;
    let mut a = Layer::placed_raster(
        "Left pixel",
        crate::document::Pixels::from_rgba(1, 1, vec![255, 0, 0, 255]).unwrap(),
        crate::geom::Pt::ZERO,
        crate::geom::Pt::splat(1.),
    );
    a.parent = Some(left.id);
    let mut b = Layer::placed_raster(
        "Right pixel",
        crate::document::Pixels::from_rgba(1, 1, vec![0, 255, 0, 255]).unwrap(),
        crate::geom::Pt::ZERO,
        crate::geom::Pt::splat(1.),
    );
    b.parent = Some(right.id);
    s.layer_expanded.extend([left.id, right.id]);
    s.doc.layers = vec![a, left, b, right];
    s.activate_layer_tree(2);
    s.history.clear();
    s
}

#[test]
fn craft_cross_group_native_rows_menu_and_document_undo_redo_follow_actual_parent() {
    let mut s = cross_group_ui_fixture();
    let parent = s.doc.layers[3].id;
    let mut seat = Seat::new();
    seat.click(&mut s, "Left pixel");
    seat.click_mods(&mut s, "Right pixel", Modifiers::COMMAND);
    assert_eq!(s.selected_layer_rows(), [0, 2]);
    let before = state(&s);
    seat.menu(&mut s, "Merge Layers");
    seat.wait(&mut s);
    assert_eq!(s.doc.layers.len(), 3);
    assert_eq!(s.doc.layers[1].name, "Right pixel");
    assert_eq!(s.doc.layers[1].parent, Some(parent));
    assert_eq!(
        s.doc.layers[1].kind.pixels().unwrap().data.as_slice(),
        [0, 255, 0, 255]
    );
    assert_eq!(s.history.len(), 1);
    let after = state(&s);
    seat.frame(&mut s, keys::key(Key::F8, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    seat.frame(&mut s, keys::key(Key::Z, Modifiers::COMMAND));
    seat.frame(&mut s, vec![]);
    assert_eq!(state(&s), before);
    seat.frame(&mut s, keys::key(Key::Y, Modifiers::COMMAND));
    seat.frame(&mut s, vec![]);
    assert_eq!(state(&s), after);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
        after
    );
}

#[test]
fn craft_cross_group_native_alt_menu_and_canvas_shortcut_keep_all_source_layers() {
    for shortcut in [false, true] {
        let mut s = cross_group_ui_fixture();
        let original = serde_json::to_value(&s.doc.layers).unwrap();
        let parent = s.doc.layers[3].id;
        let mut seat = Seat::new();
        seat.click(&mut s, "Left pixel");
        seat.click_mods(&mut s, "Right pixel", Modifiers::COMMAND);
        let before = state(&s);
        if shortcut {
            seat.frame(&mut s, keys::key(Key::F8, Modifiers::NONE));
            seat.frame(&mut s, vec![]);
            seat.frame(
                &mut s,
                keys::key(
                    Key::E,
                    Modifiers {
                        alt: true,
                        ..Modifiers::COMMAND
                    },
                ),
            );
        } else {
            seat.click(&mut s, "Layer");
            seat.click_mods(&mut s, "Merge Layers", Modifiers::ALT);
        }
        seat.wait(&mut s);
        assert_eq!(s.doc.layers.len(), 5);
        assert_eq!(s.doc.layers[3].parent, Some(parent));
        assert_eq!(s.doc.layers[3].name, "Right pixel (merged)");
        assert_eq!(
            s.doc.layers[3].kind.pixels().unwrap().data.as_slice(),
            [0, 255, 0, 255]
        );
        let remaining: Vec<_> = s
            .doc
            .layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| (i != 3).then_some(l))
            .collect();
        assert_eq!(serde_json::to_value(remaining).unwrap(), original);
        check(&mut s, &before);
    }
}

#[test]
fn craft_history_targets_native_merge_undo_redo_restore_rows_after_other_native_clicks() {
    for stamp in [false, true] {
        let mut s = cross_group_ui_fixture();
        let mut seat = Seat::new();
        seat.click(&mut s, "Left pixel");
        seat.click_mods(&mut s, "Right pixel", Modifiers::COMMAND);
        let before = state(&s);
        let before_active = s.doc.layers[s.active_layer.unwrap()].id;
        if stamp {
            seat.click(&mut s, "Layer");
            seat.click_mods(&mut s, "Merge Layers", Modifiers::ALT);
        } else {
            seat.menu(&mut s, "Merge Layers");
        }
        seat.wait(&mut s);
        let after = state(&s);
        let result = s.doc.layers[s.active_layer.unwrap()].id;
        seat.click(&mut s, "Left group");
        seat.frame(&mut s, keys::key(Key::F8, Modifiers::NONE));
        seat.frame(&mut s, vec![]);
        seat.frame(&mut s, keys::key(Key::Z, Modifiers::COMMAND));
        seat.frame(&mut s, vec![]);
        assert_eq!(state(&s), before);
        assert_eq!(s.selected_layer_rows(), [0, 2]);
        assert!(s.layer_row_selected(0));
        assert!(s.layer_row_selected(2));
        assert_eq!(s.doc.layers[s.active_layer.unwrap()].id, before_active);
        seat.click(&mut s, "Right group");
        seat.frame(&mut s, keys::key(Key::F8, Modifiers::NONE));
        seat.frame(&mut s, vec![]);
        seat.frame(&mut s, keys::key(Key::Y, Modifiers::COMMAND));
        seat.frame(&mut s, vec![]);
        assert_eq!(state(&s), after);
        assert_eq!(s.doc.layers[s.active_layer.unwrap()].id, result);
        assert_eq!(s.selected_layer, Some(result));
        assert_eq!(s.selected_layer_rows().len(), 1);
        assert_eq!(
            crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
            after
        );
    }
}

fn tree_rows_ui_fixture() -> Studio {
    let mut s = cross_group_ui_fixture();
    let original = s.doc.layers.clone();
    let spare = Layer::placed_raster(
        "Spare pixel",
        crate::document::Pixels::from_rgba(1, 1, vec![0, 0, 255, 255]).unwrap(),
        crate::geom::Pt::new(4., 0.),
        crate::geom::Pt::splat(1.),
    );
    s.doc.layers = vec![
        original[0].clone(),
        original[2].clone(),
        original[3].clone(),
        original[1].clone(),
        spare,
    ];
    s.doc.name = "Native tree rows".into();
    s.activate_layer_tree(0);
    s.history.clear();
    s
}

#[test]
fn craft_tree_rows_native_group_headers_shift_merge_and_history_follow_actual_tree() {
    let mut s = tree_rows_ui_fixture();
    let mut seat = Seat::new();
    for _ in 0..3 {
        seat.frame(&mut s, vec![]);
    }
    let ys: Vec<_> = [
        "Spare pixel",
        "Left group",
        "Left pixel",
        "Right group",
        "Right pixel",
    ]
    .into_iter()
    .map(|name| {
        let b = keys::named(&seat.output, name).1.bounds().unwrap();
        (b.y0 + b.y1) / 2.
    })
    .collect();
    assert!(
        ys.windows(2).all(|v| v[0] < v[1]),
        "actual tree rows {ys:?}"
    );
    seat.click(&mut s, "Left pixel");
    seat.click_mods(&mut s, "Right pixel", Modifiers::SHIFT);
    assert_eq!(s.selected_layer_rows(), [1, 2, 0]);
    let before = state(&s);
    let parent = s.doc.layers[3].id;
    seat.menu(&mut s, "Merge Layers");
    seat.wait(&mut s);
    assert_eq!(s.doc.layers.len(), 3);
    assert_eq!(s.doc.layers[0].parent, Some(parent));
    assert_eq!(
        s.doc.layers[0].kind.pixels().unwrap().data.as_slice(),
        [255, 0, 0, 255]
    );
    assert_eq!(s.history.len(), 1);
    let after = state(&s);
    seat.frame(&mut s, keys::key(Key::F8, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    seat.frame(&mut s, keys::key(Key::Z, Modifiers::COMMAND));
    seat.frame(&mut s, vec![]);
    assert_eq!(state(&s), before);
    assert_eq!(s.selected_layer_rows(), [1, 2, 0]);
    assert_eq!(s.active_layer, Some(1));
    seat.frame(&mut s, keys::key(Key::Y, Modifiers::COMMAND));
    seat.frame(&mut s, vec![]);
    assert_eq!(state(&s), after);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
        after
    );
}

#[test]
fn craft_tree_rows_native_toggle_keeps_last_target_and_shift_uses_latest_active_anchor() {
    let mut s = tree_rows_ui_fixture();
    let mut seat = Seat::new();
    let before = state(&s);
    seat.click(&mut s, "Left pixel");
    seat.click_mods(&mut s, "Left pixel", Modifiers::COMMAND);
    assert_eq!(s.selected_layer_rows(), [0]);
    assert_eq!(s.active_layer, Some(0));
    seat.click_mods(&mut s, "Right pixel", Modifiers::COMMAND);
    assert_eq!(s.selected_layer_rows(), [1, 0]);
    assert_eq!(s.active_layer, Some(1));
    seat.click_mods(&mut s, "Right pixel", Modifiers::COMMAND);
    assert_eq!(s.selected_layer_rows(), [0]);
    assert_eq!(s.active_layer, Some(0));
    seat.click_mods(&mut s, "Right pixel", Modifiers::COMMAND);
    seat.click_mods(&mut s, "Left group", Modifiers::SHIFT);
    assert_eq!(s.selected_layer_rows(), [1, 2, 0, 3]);
    assert_eq!(s.active_layer, Some(3));
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
}

#[test]
fn craft_tree_rows_native_group_drag_and_arrange_key_keep_parent_data_and_one_undo() {
    for drag in [false, true] {
        let mut s = tree_rows_ui_fixture();
        let mut seat = Seat::new();
        let left = s.doc.layers[3].id;
        let right = s.doc.layers[2].id;
        seat.click(&mut s, "Left group");
        let before = state(&s);
        if drag {
            let point = |name: &str| {
                let b = keys::named(&seat.output, name).1.bounds().unwrap();
                egui::pos2(((b.x0 + b.x1) / 2.) as f32, ((b.y0 + b.y1) / 2.) as f32)
            };
            let from = point("Left group");
            let to = point("Right group");
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            seat.frame(&mut s, vec![Event::PointerMoved(from), button(from, true)]);
            seat.frame(
                &mut s,
                vec![Event::PointerMoved(from + egui::vec2(0., 10.))],
            );
            seat.frame(&mut s, vec![Event::PointerMoved(to)]);
            seat.frame(&mut s, vec![button(to, false)]);
            seat.frame(&mut s, vec![]);
        } else {
            seat.frame(&mut s, keys::key(Key::F8, Modifiers::NONE));
            seat.frame(&mut s, vec![]);
            seat.frame(&mut s, keys::key(Key::OpenBracket, Modifiers::COMMAND));
            seat.frame(&mut s, vec![]);
        }
        assert_eq!(s.history.len(), 1, "drag={drag}");
        s.doc.validate_hierarchy().unwrap();
        assert_eq!(
            s.doc.layers.iter().find(|l| l.id == left).unwrap().parent,
            if drag { Some(right) } else { None }
        );
        let pm = crate::compositor::render_export(&s.doc, 1).unwrap();
        assert_eq!(
            &pm.data()[..4],
            if drag {
                &[255, 0, 0, 255]
            } else {
                &[0, 255, 0, 255]
            }
        );
        let after = state(&s);
        seat.click(&mut s, "Spare pixel");
        seat.frame(&mut s, keys::key(Key::F8, Modifiers::NONE));
        seat.frame(&mut s, vec![]);
        seat.frame(&mut s, keys::key(Key::Z, Modifiers::COMMAND));
        seat.frame(&mut s, vec![]);
        assert_eq!(state(&s), before);
        assert_eq!(s.selected_layer, Some(left));
        seat.frame(&mut s, keys::key(Key::Y, Modifiers::COMMAND));
        seat.frame(&mut s, vec![]);
        assert_eq!(state(&s), after);
        assert_eq!(s.selected_layer, Some(left));
        assert_eq!(
            crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
            after
        );
    }
}
