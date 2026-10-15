//! Native composition authoring in the motion timeline.
use crate::{app::Studio, motion::compositions::Composition};
use eframe::egui::{self, Ui};

pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    ui.menu_button("Compositions", |ui| {
        if ui.button("Add project…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Motion project", &["oma", "omamotion"])
                .pick_file()
        {
            let imported = if path.extension().is_some_and(|e| e == "omamotion") {
                std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|s| crate::motion::compositions::import(&s))
            } else {
                crate::project::load_from(&path)
            };
            match imported {
                Ok(doc) => {
                    let mut after = studio.doc.motion.clone();
                    let comp = Composition::new(doc);
                    after.duration = after.duration.max(comp.end);
                    after.compositions.push(comp);
                    let mut candidate = studio.doc.clone();
                    candidate.motion = after.clone();
                    match crate::motion::compositions::validate(&candidate) {
                        Ok(()) => studio.commit_motion_step(after),
                        Err(e) => studio.status = e,
                    }
                }
                Err(e) => studio.status = e,
            }
        }
        if ui.button("Export motion project…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Motion project", &["omamotion"])
                .set_file_name(format!("{}.omamotion", studio.doc.name))
                .save_file()
        {
            studio.status = match crate::motion::compositions::export(&studio.doc)
                .and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string()))
            {
                Ok(()) => "Motion project saved".into(),
                Err(e) => e,
            };
        }
        let mut after = studio.doc.motion.clone();
        let mut changed = false;
        let mut remove = None;
        for comp in &mut after.compositions {
            ui.push_id(comp.id, |ui| {
                ui.collapsing(comp.name.clone(), |ui| {
                    changed |= ui.checkbox(&mut comp.visible, "Visible").changed();
                    changed |= ui.text_edit_singleline(&mut comp.name).changed();
                    for (name, value) in [
                        ("Start", &mut comp.start),
                        ("End", &mut comp.end),
                        ("Source time", &mut comp.source_start),
                        ("Speed", &mut comp.speed),
                        ("X", &mut comp.position.x),
                        ("Y", &mut comp.position.y),
                        ("Anchor X", &mut comp.anchor.x),
                        ("Anchor Y", &mut comp.anchor.y),
                        ("Scale X", &mut comp.scale.x),
                        ("Scale Y", &mut comp.scale.y),
                        ("Opacity", &mut comp.opacity),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(name);
                            changed |= ui.add(egui::DragValue::new(value).speed(0.05)).changed();
                        });
                    }
                    let mut degrees = comp.rotation.to_degrees();
                    ui.horizontal(|ui| {
                        ui.label("Rotate");
                        if ui
                            .add(egui::DragValue::new(&mut degrees).suffix("°"))
                            .changed()
                        {
                            comp.rotation = degrees.to_radians();
                            changed = true;
                        }
                    });
                    changed |= ui.checkbox(&mut comp.looped, "Loop source").changed();
                    if ui.button("Remove composition").clicked() {
                        remove = Some(comp.id);
                    }
                });
            });
        }
        if let Some(id) = remove {
            after.compositions.retain(|c| c.id != id);
            after.drop_shape(id);
            changed = true;
        }
        if changed {
            after.duration = after
                .compositions
                .iter()
                .map(|c| c.end)
                .fold(after.duration, f32::max);
            let mut doc = studio.doc.clone();
            doc.motion = after.clone();
            match crate::motion::compositions::validate(&doc) {
                Ok(()) => studio.commit_motion(after),
                Err(e) => studio.status = e,
            }
        }
    });
}
