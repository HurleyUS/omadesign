//! Photo's top Develop menu: real develop commands, each one photo history
//! step, ahead of the develop panel shortcuts.
use crate::app::Studio;
use crate::photo::DevelopParams;
use eframe::egui::{Button, Ui};

pub(super) fn available(studio: &Studio) -> bool {
    !studio.photo.is_batching() && !studio.photo.is_saving() && studio.photo.selected().is_some()
}

/// Edit the selected photo's develop settings as one undo step.
pub(super) fn develop_step(studio: &mut Studio, edit: impl FnOnce(&mut crate::photo::PhotoImage)) {
    if !available(studio) {
        return;
    }
    studio.photo.finish_edit();
    let Some(image) = studio.photo.selected_mut() else { return };
    let before = image.develop.clone();
    edit(image);
    studio.photo.record_edit(before, false);
    studio.photo.finish_edit();
}

pub(super) fn menu(ui: &mut Ui, studio: &mut Studio) {
    let ready = available(studio);
    let develop = studio.photo.selected().map(|image| image.develop.clone());
    if ui
        .add_enabled(ready, Button::new("Auto light"))
        .on_hover_text("Balance exposure and contrast automatically")
        .clicked()
    {
        develop_step(studio, |image| crate::photo::auto_tone(&mut image.develop, &image.preview));
        studio.status = "auto light".into();
        ui.close();
    }
    if ui
        .add_enabled(ready, Button::new("Auto white balance"))
        .on_hover_text("Neutralize the average color with temperature and tint")
        .clicked()
    {
        develop_step(studio, |image| crate::photo::auto_white_balance(&mut image.develop, &image.preview));
        studio.status = "auto white balance".into();
        ui.close();
    }
    let cropped = develop.as_ref().is_some_and(|p| p.crop.is_some());
    if ui.add_enabled(ready && cropped, Button::new("Clear crop")).clicked() {
        develop_step(studio, |image| image.develop.crop = None);
        studio.status = "crop cleared".into();
        ui.close();
    }
    let adjusted = develop.as_ref().is_some_and(|p| *p != DevelopParams::default());
    if ui
        .add_enabled(ready && adjusted, Button::new("Reset all adjustments"))
        .clicked()
    {
        develop_step(studio, |image| image.develop = DevelopParams::default());
        studio.status = "adjustments reset".into();
        ui.close();
    }
    ui.separator();
    let mut original = studio.photo.show_original;
    if ui
        .add_enabled(studio.photo.selected().is_some(), eframe::egui::Checkbox::new(&mut original, "Show original"))
        .clicked()
    {
        studio.photo.show_original = original;
        ui.close();
    }
    ui.separator();
}
