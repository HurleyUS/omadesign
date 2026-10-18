use crate::app::Studio;
use crate::ui::masking;
use eframe::egui::{self, Button, Ui};

enum Action {
    Pixel,
    Group,
    DuplicateLayer(usize),
    Duplicate,
    Delete(usize),
    Arrange(usize, bool, bool),
    Mask(usize, masking::Action),
    Merge(crate::app::layer_merge::Kind),
}

/// Keep the native Layer menu alive while a nested menu owns its popup.
/// Args: menu bar and current editor. Returns nothing; choices keep their existing typed routes.
pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let key = ui.make_persistent_id(("pixel-layer-popup", &studio.swap_id));
    let mut open = ui.data(|data| data.get_temp::<bool>(key)).unwrap_or(false);
    let response = ui.button("Layer");
    if response.clicked() {
        open = !open;
    }
    egui::Popup::menu(&response)
        .open_bool(&mut open)
        .show(|ui| {
            if !response.enabled() {
                ui.disable();
            }
            menu(ui, studio);
        });
    let popup = egui::Popup::default_response_id(&response);
    if open {
        if !egui::Popup::is_any_open(ui.ctx()) || egui::Popup::is_id_open(ui.ctx(), popup) {
            egui::Popup::open_id(ui.ctx(), popup);
        }
    } else if egui::Popup::is_id_open(ui.ctx(), popup) {
        egui::Popup::close_id(ui.ctx(), popup);
    }
    ui.data_mut(|data| data.insert_temp(key, open));
    crate::ui::welcome::focus_control(ui, &response);
}

/// Run the native Pixel Layer menu.
/// Args: menu UI and editor. Returns nothing; accepted choices use existing typed layer and mask commands.
pub(super) fn menu(ui: &mut Ui, studio: &mut Studio) {
    let active = studio.active_layer.filter(|&i| i < studio.doc.layers.len());
    let owner = active.map(|i| studio.doc.layers[i].id);
    let mut action = None;
    ui.push_id(("pixel-layer-menu", &studio.swap_id, owner), |ui| {
        ui.add_enabled_ui(!studio.updates.freezing && studio.op.is_none(), |ui| {
            if ui.button("New pixel layer").clicked() {
                action = Some(Action::Pixel);
            }
            if ui.button("New group").clicked() {
                action = Some(Action::Group);
            }
            if ui
                .add_enabled(
                    active.is_some_and(|i| studio.can_duplicate_layer_tree(i)),
                    Button::new("Duplicate layer…"),
                )
                .clicked()
            {
                action = active.map(Action::DuplicateLayer);
            }
            let selection = !studio.selection.is_empty() || studio.selected_layer.is_some();
            if ui
                .add_enabled(selection, Button::new("Duplicate selection"))
                .clicked()
            {
                action = Some(Action::Duplicate);
            }
            let unlocked = active.is_some_and(|i| studio.layer_unlocked(i));
            let deletable = active.is_some_and(|i| {
                unlocked
                    && studio
                        .layer_tree_indices(i)
                        .iter()
                        .all(|&child| !studio.doc.layers[child].locked)
            });
            if ui
                .add_enabled(deletable, Button::new("Delete layer"))
                .clicked()
            {
                action = active.map(Action::Delete);
            }
            ui.separator();
            ui.add_enabled_ui(unlocked, |ui| {
                ui.menu_button("Arrange", |ui| {
                    for (label, up, extreme) in [
                        ("Bring to front", true, true),
                        ("Bring forward", true, false),
                        ("Send backward", false, false),
                        ("Send to back", false, true),
                    ] {
                        let enabled = active.is_some_and(|i| studio.layer_sibling(i, up).is_some());
                        if ui.add_enabled(enabled, Button::new(label)).clicked() {
                            action = active.map(|i| Action::Arrange(i, up, extreme));
                        }
                    }
                });
            });
            let visible = active.is_some_and(|i| studio.doc.layer_visible(i));
            ui.add_enabled_ui(unlocked && visible, |ui| {
                ui.menu_button("Layer mask", |ui| {
                    if let Some(index) = active
                        && let Some(chosen) = masking::menu(ui, studio, index)
                    {
                        action = Some(Action::Mask(index, chosen));
                    }
                });
            });
            ui.separator();
            if let Some(kind) = crate::ui::layer_merge::menu(ui, studio, owner) {
                action = Some(Action::Merge(kind));
            }
        });
    });
    if let Some(action) = action {
        match action {
            Action::Pixel => studio.add_layer(true),
            Action::Group => studio.add_layer_group(),
            Action::DuplicateLayer(i) => {
                crate::ui::layer_duplicate::open(ui.ctx(), studio, i);
            }
            Action::Duplicate => studio.duplicate_selection(),
            Action::Delete(i) => studio.delete_layer_tree(i),
            Action::Arrange(i, up, true) => studio.move_layer_tree_extreme(i, up),
            Action::Arrange(i, up, false) => studio.move_layer_tree(i, up),
            Action::Mask(i, chosen) => chosen.run(studio, i),
            Action::Merge(kind) => crate::ui::layer_merge::start(ui.ctx(), studio, kind),
        }
        ui.close_kind(egui::UiKind::Menu);
    }
}

#[cfg(test)]
#[path = "pixel_layer_menu_tests.rs"]
mod tests;
