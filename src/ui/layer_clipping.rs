use crate::app::Studio;
use eframe::egui::{self, Ui};

/// Offer real create/release clipping actions for selected layers or one row owner.
/// Args: native menu, editor and optional explicit layer ID. Returns the accepted clipping state without mutating during a row borrow.
pub(super) fn menu(ui: &mut Ui, studio: &Studio, owner: Option<u64>) -> Option<bool> {
    let mut chosen = None;
    ui.push_id(("layer-clipping", &studio.swap_id, owner), |ui| {
        for (after, name) in [
            (true, "Create Clipping Mask"),
            (false, "Release Clipping Mask"),
        ] {
            let mut button = egui::Button::new(name);
            let active = owner
                .and_then(|id| studio.doc.layers.iter().find(|l| l.id == id))
                .or_else(|| studio.active_layer.and_then(|i| studio.doc.layers.get(i)));
            if studio.persona == crate::tools::Persona::Pixel
                && active.is_some_and(|l| l.clipped != after)
            {
                button = button.shortcut_text("Ctrl+Alt+G");
            }
            let response = ui.add_enabled(studio.can_set_layer_clipping(owner, after), button);
            if super::agent::named(ui, response, name).clicked() {
                chosen = Some(after);
            }
        }
    });
    if chosen.is_some() {
        ui.close();
    }
    chosen
}

#[cfg(test)]
#[path = "layer_clipping_tests.rs"]
mod tests;
