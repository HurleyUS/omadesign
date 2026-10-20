//! Motion's top Animation menu: real keyframe, preset and loop commands
//! ahead of the timeline panel shortcuts.
use crate::app::Studio;
use crate::motion::Ease;
use crate::motion_presets::Preset;
use eframe::egui::{Button, Ui};

pub(super) const EASES: [(Ease, &str); 5] = [
    (Ease::Linear, "Linear keyframe"),
    (Ease::EaseIn, "Ease-in keyframe"),
    (Ease::EaseOut, "Ease-out keyframe"),
    (Ease::EaseInOut, "Ease-in-out keyframe"),
    (Ease::Hold, "Hold keyframe"),
];

fn available(studio: &Studio) -> bool {
    !studio.file_dialog_pending() && !studio.updates.freezing
}

/// True when at least one selected object can take a key.
pub(super) fn can_key(studio: &Studio) -> bool {
    available(studio)
        && studio
            .selection
            .iter()
            .any(|&(layer, id)| crate::motion::target(&studio.doc, layer, id).is_some())
}

/// Flip looped playback as one discrete history step.
pub(super) fn toggle_loop(studio: &mut Studio) {
    if !available(studio) {
        return;
    }
    let mut after = studio.doc.motion.clone();
    after.looped = !after.looped;
    studio.commit_motion_step(after);
    studio.status = if studio.doc.motion.looped { "loop playback on" } else { "loop playback off" }.into();
}

fn animated_targets(studio: &Studio) -> Vec<u64> {
    if !available(studio) {
        return vec![];
    }
    let mut ids: Vec<u64> = studio
        .selection
        .iter()
        .filter_map(|&(layer, id)| crate::motion::target(&studio.doc, layer, id).map(|t| t.id))
        .filter(|id| studio.doc.motion.tracks.iter().any(|t| t.shape == *id && !t.keys.is_empty()))
        .collect();
    ids.dedup();
    ids
}

/// Mirror the selection's keys in time (t → duration − t) as one history step.
/// Returns how many objects changed.
pub(super) fn reverse_animation(studio: &mut Studio) -> usize {
    let ids = animated_targets(studio);
    if ids.is_empty() {
        return 0;
    }
    let mut after = studio.doc.motion.clone();
    let duration = after.duration;
    for track in after.tracks.iter_mut().filter(|t| ids.contains(&t.shape)) {
        for key in &mut track.keys {
            key.t = (duration - key.t).clamp(0., duration);
        }
        track.keys.reverse();
    }
    studio.commit_motion_step(after);
    studio.status = format!("animation reversed · {} objects", ids.len());
    ids.len()
}

/// Remove every key track of the selection as one history step. Returns objects cleared.
pub(super) fn clear_animation(studio: &mut Studio) -> usize {
    let ids = animated_targets(studio);
    if ids.is_empty() {
        return 0;
    }
    let mut after = studio.doc.motion.clone();
    after.tracks.retain(|t| !ids.contains(&t.shape));
    studio.commit_motion_step(after);
    studio.status = format!("animation cleared · {} objects", ids.len());
    ids.len()
}

pub(super) fn menu(ui: &mut Ui, studio: &mut Studio) {
    let keyable = can_key(studio);
    if ui
        .add_enabled(keyable, Button::new("Add keyframe").shortcut_text("K"))
        .on_hover_text("Key the selection at the playhead")
        .clicked()
    {
        studio.key_selection(Ease::EaseInOut);
        ui.close();
    }
    ui.add_enabled_ui(keyable, |ui| {
        ui.menu_button("Add keyframe with ease", |ui| {
            for (ease, label) in EASES {
                if ui.button(label).clicked() {
                    studio.key_selection(ease);
                    ui.close();
                }
            }
        });
    });
    let any_preset = available(studio) && Preset::ALL.iter().any(|p| studio.motion_preset_count(*p) > 0);
    ui.add_enabled_ui(any_preset, |ui| {
        ui.menu_button("Motion presets", |ui| {
            for preset in Preset::ALL {
                let count = studio.motion_preset_count(preset);
                if ui
                    .add_enabled(count > 0, Button::new(preset.name()))
                    .on_hover_text(preset.hint())
                    .clicked()
                {
                    studio.apply_motion_preset(preset);
                    ui.close();
                }
            }
        });
    });
    let animated = !animated_targets(studio).is_empty();
    if ui.add_enabled(animated, Button::new("Reverse animation")).on_hover_text("Play the selection's keys backwards").clicked() {
        reverse_animation(studio);
        ui.close();
    }
    if ui.add_enabled(animated, Button::new("Clear animation")).clicked() {
        clear_animation(studio);
        ui.close();
    }
    let mut looped = studio.doc.motion.looped;
    if ui
        .add_enabled(available(studio), eframe::egui::Checkbox::new(&mut looped, "Loop playback"))
        .clicked()
    {
        toggle_loop(studio);
        ui.close();
    }
    ui.separator();
}
