//! Application panel registry. Canvas and document commands remain outside docking.
use super::workspace::{Place, Workspace};
use crate::app::Studio;
use eframe::egui::{self, Ui};

const PANELS: &[(&str, &str)] = &[
    ("properties", "Properties"),
    ("layers", "Layers"),
    ("timeline", "Timeline"),
];

/// Draw task panels around the document canvas.
/// Args: root UI and studio. Returns no value; saved layout changes never enter artwork undo.
#[allow(deprecated)]
pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let mut book = std::mem::take(&mut studio.panel_workspaces);
    let before = book.clone();
    let presets = [
        (
            "Design",
            Workspace::preset(&[("properties", Place::Right), ("layers", Place::Left)]),
        ),
        (
            "Layout",
            Workspace::preset(&[("properties", Place::Right), ("layers", Place::Left)]),
        ),
        (
            "Pixel",
            Workspace::preset(&[("properties", Place::Right), ("layers", Place::Right)]),
        ),
        (
            "Motion",
            Workspace::preset(&[
                ("properties", Place::Right),
                ("layers", Place::Left),
                ("timeline", Place::Bottom),
            ]),
        ),
    ];
    for (_, layout) in &mut book.tasks {
        layout.normalize(&PANELS.iter().map(|p| p.0).collect::<Vec<_>>());
    }
    book.toolbar(ui, &presets, PANELS);
    let active = book.active.clone();
    let center =
        book.tasks
            .get_mut(&active)
            .unwrap()
            .show(ui, "oma-workspace", PANELS, |ui, panel| match panel {
                "properties" => super::studios::inspector_content(ui, studio),
                "layers" => super::studios::layers_content(ui, studio),
                "timeline" => super::timeline::content(ui, studio),
                _ => {}
            });
    ui.scope_builder(egui::UiBuilder::new().max_rect(center), |ui| {
        super::canvas::show(ui, studio)
    });
    let changed = before != book;
    studio.panel_workspaces = book;
    let dirty_key = egui::Id::new("oma-workspaces-dirty");
    let dirty = ui
        .ctx()
        .data_mut(|data| data.get_temp::<bool>(dirty_key))
        .unwrap_or(false)
        || changed;
    if dirty && !ui.input(|input| input.pointer.any_down()) {
        studio.save_panel_workspaces();
        ui.ctx().data_mut(|data| data.insert_temp(dirty_key, false));
    } else if dirty {
        ui.ctx().data_mut(|data| data.insert_temp(dirty_key, true));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_workspace_keeps_canvas_visible_at_all_scales() {
        for scale in [1.0, 1.5, 2.0] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(scale);
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0 / scale, 700.0 / scale),
                )),
                ..Default::default()
            };
            let mut layout = Workspace::preset(&[
                ("properties", Place::Right),
                ("layers", Place::Left),
                ("timeline", Place::Bottom),
            ]);
            let _ = ctx.run_ui(input, |ui| {
                let center = layout.show(ui, "test", PANELS, |ui, _| {
                    ui.label("Panel body");
                });
                assert!(center.width() >= 240.0);
                assert!(center.height() >= 160.0);
            });
        }
    }
}
