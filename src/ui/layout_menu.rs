//! Layout's top menu: real frame, auto-layout and component commands
//! ahead of the layout panel shortcuts.
use crate::app::Studio;
use crate::layout_components as components;
use eframe::egui::Ui;

fn available(studio: &Studio) -> bool {
    !studio.file_dialog_pending() && !studio.updates.freezing
}

fn editable(studio: &Studio, (layer, id): (usize, u64)) -> bool {
    studio.doc.layer_editable(layer)
        && studio
            .doc
            .find_shape(layer, id)
            .is_some_and(|shape| shape.visible && !shape.locked && !shape.guide)
}

/// Every selected object is editable and on one layer.
pub(super) fn can_wrap(studio: &Studio) -> bool {
    available(studio)
        && !studio.selection.is_empty()
        && studio.selection.iter().all(|&hit| editable(studio, hit) && hit.0 == studio.selection[0].0)
}

pub(super) fn can_make_component(studio: &Studio) -> bool {
    available(studio)
        && studio.selected_frame().is_some_and(|hit| {
            editable(studio, hit)
                && studio.doc.find_shape(hit.0, hit.1).is_some_and(|s| s.layout.component.is_none())
        })
}

pub(super) fn can_detach(studio: &Studio) -> bool {
    available(studio)
        && studio.primary().is_some_and(|(layer, id)| {
            components::instance_ancestor(&studio.doc, layer, id)
                .is_some_and(|root| editable(studio, (layer, root)))
        })
}

pub(super) fn menu(ui: &mut Ui, studio: &mut Studio) {
    let wrap = can_wrap(studio);
    if ui.add_enabled(wrap, eframe::egui::Button::new("Wrap in frame")).clicked() {
        studio.wrap_selection_frame();
        ui.close();
    }
    if ui
        .add_enabled(wrap, eframe::egui::Button::new("Add auto layout"))
        .on_hover_text("Stack the selection in a responsive frame")
        .clicked()
    {
        studio.auto_layout_selection();
        ui.close();
    }
    ui.separator();
    if ui
        .add_enabled(can_make_component(studio), eframe::egui::Button::new("Create component"))
        .clicked()
    {
        studio.create_layout_component();
        ui.close();
    }
    if ui
        .add_enabled(can_detach(studio), eframe::egui::Button::new("Detach instance"))
        .clicked()
    {
        studio.detach_layout_instance();
        ui.close();
    }
    ui.separator();
}
