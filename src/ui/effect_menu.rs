//! Design's top Effect menu: add or clear object effects on the selection as
//! one history step, ahead of the effect panel shortcuts.
use crate::app::Studio;
use crate::document::{Cmd, RASTER_ID};
use crate::filter::Fx;
use eframe::egui::{Button, Ui};

fn available(studio: &Studio) -> bool {
    !studio.file_dialog_pending() && !studio.updates.freezing
}

/// Editable, visible, unlocked selected objects (not pixel layers or guides).
pub(super) fn targets(studio: &Studio) -> Vec<(usize, u64)> {
    if !available(studio) {
        return Vec::new();
    }
    studio
        .selection
        .iter()
        .copied()
        .filter(|&(layer, id)| {
            id != RASTER_ID
                && studio.doc.layer_editable(layer)
                && studio
                    .doc
                    .find_shape(layer, id)
                    .is_some_and(|s| s.visible && !s.locked && !s.guide)
        })
        .collect()
}

fn edit_filters(studio: &mut Studio, edit: impl Fn(&mut crate::filter::FilterStack)) -> usize {
    let commands: Vec<_> = targets(studio)
        .into_iter()
        .filter_map(|(layer, id)| {
            let before = studio.doc.find_shape(layer, id)?.filters.clone();
            let mut after = before.clone();
            edit(&mut after);
            (after != before).then_some(Cmd::SetShapeFilters { layer, id, before, after })
        })
        .collect();
    let count = commands.len();
    if count > 0 {
        studio.commit(Cmd::Batch(commands));
    }
    count
}

pub(super) fn add_effect(studio: &mut Studio, make: fn() -> Fx) -> usize {
    edit_filters(studio, |stack| stack.items.push(make()))
}

pub(super) fn clear_effects(studio: &mut Studio) -> usize {
    edit_filters(studio, |stack| stack.items.clear())
}

/// The active layer when it is visible and unlocked. Returns its index.
pub(super) fn layer_target(studio: &Studio) -> Option<usize> {
    if !available(studio) {
        return None;
    }
    let index = studio.active_layer?;
    (index < studio.doc.layers.len() && studio.doc.layer_editable(index) && studio.doc.layer_visible(index)).then_some(index)
}

pub(super) fn add_layer_effect(studio: &mut Studio, make: fn() -> Fx) -> bool {
    let Some(index) = layer_target(studio) else { return false };
    let mut after = studio.doc.layers[index].filters.clone();
    after.items.push(make());
    studio.commit_filters(index, after);
    true
}

pub(super) fn clear_layer_effects(studio: &mut Studio) -> bool {
    let Some(index) = layer_target(studio) else { return false };
    if studio.doc.layers[index].filters.items.is_empty() {
        return false;
    }
    let mut after = studio.doc.layers[index].filters.clone();
    after.items.clear();
    studio.commit_filters(index, after);
    true
}

pub(super) fn menu(ui: &mut Ui, studio: &mut Studio) {
    let targets = targets(studio);
    ui.add_enabled_ui(!targets.is_empty(), |ui| {
        ui.menu_button("Add effect", |ui| {
            for (name, make) in Fx::catalog() {
                if ui.button(*name).clicked() {
                    let count = add_effect(studio, *make);
                    studio.status = format!("{name} · {count} objects");
                    ui.close();
                }
            }
        });
    });
    let has_effects = targets.iter().any(|&(layer, id)| {
        studio.doc.find_shape(layer, id).is_some_and(|s| !s.filters.items.is_empty())
    });
    if ui.add_enabled(has_effects, Button::new("Clear effects")).clicked() {
        let count = clear_effects(studio);
        studio.status = format!("effects cleared · {count} objects");
        ui.close();
    }
    let layer = layer_target(studio);
    ui.add_enabled_ui(layer.is_some(), |ui| {
        ui.menu_button("Add layer effect", |ui| {
            for (name, make) in Fx::catalog() {
                if ui.button(*name).clicked() {
                    add_layer_effect(studio, *make);
                    studio.status = format!("{name} · layer");
                    ui.close();
                }
            }
        });
    });
    let layer_has = layer.is_some_and(|i| !studio.doc.layers[i].filters.items.is_empty());
    if ui.add_enabled(layer_has, Button::new("Clear layer effects")).clicked() {
        clear_layer_effects(studio);
        studio.status = "layer effects cleared".into();
        ui.close();
    }
    ui.separator();
}
