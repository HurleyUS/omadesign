use crate::app::{LayerCopyDestination, LayerCopyTarget, Studio};
use eframe::egui::{self, Id};

const ID: &str = "duplicate-layer-dialog";

#[derive(Clone)]
struct Draft {
    instance: Id,
    source: LayerCopyTarget,
    name: String,
    destinations: Vec<(String, LayerCopyDestination)>,
    selected: usize,
    document_name: String,
    focus: bool,
    error: Option<String>,
}

pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<Draft>(Id::new(ID)).is_some())
}

/// Open copy options for the concrete source without editing its document.
/// Args: native UI context, current editor and source index. Returns nothing; Cancel leaves both documents unchanged.
pub(super) fn open(ctx: &egui::Context, studio: &mut Studio, index: usize) {
    let source = match studio.capture_layer_copy(index) {
        Ok(source) => source,
        Err(error) => {
            studio.status = error;
            return;
        }
    };
    let destinations: Vec<_> = (0..studio.tab_count())
        .filter_map(|i| {
            let (doc, id, revision, _) = studio.tab_preview_source(i)?;
            Some((
                if doc.name.is_empty() {
                    "Untitled".into()
                } else {
                    doc.name.clone()
                },
                LayerCopyDestination::Document {
                    document: id.to_owned(),
                    revision,
                },
            ))
        })
        .collect();
    let selected = destinations.iter().position(|(_, d)| {
        matches!(d, LayerCopyDestination::Document {document, ..} if *document == source.document)
    }).unwrap_or(0);
    let draft = Draft {
        instance: super::keyboard_controls::draft_id(ctx, ID),
        source,
        name: format!("{} copy", studio.doc.layers[index].name),
        destinations,
        selected,
        document_name: String::new(),
        focus: true,
        error: None,
    };
    ctx.data_mut(|data| data.insert_temp(Id::new(ID), draft));
}

/// Draw native copy options and dispatch one validated layer transaction.
/// Args: UI context and current editor. Returns nothing; destination identities and revisions are checked again on acceptance.
pub(super) fn show(ctx: &egui::Context, studio: &mut Studio) {
    let Some(mut draft) = ctx.data(|data| data.get_temp::<Draft>(Id::new(ID))) else {
        return;
    };
    let current = studio.layer_copy_current(&draft.source);
    let owner = draft.instance;
    let mut apply = false;
    let mut cancel = false;
    let modal = egui::Modal::new(Id::new(ID))
        .backdrop_color(super::theme::bg_window().gamma_multiply(0.55))
        .show(ctx, |ui| {
            ui.set_max_width(380f32.min(ctx.content_rect().width() - 32.).max(120.));
            ui.heading("Duplicate Layer");
            egui::ScrollArea::vertical()
                .id_salt(owner.with("scroll"))
                .max_height((ctx.content_rect().height() - 140.).max(40.))
                .min_scrolled_height(0.)
                .animated(false)
                .show(ui, |ui| {
                    if let Err(error) = &current {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    ui.add_enabled_ui(current.is_ok(), |ui| {
                        let label = ui.label("Layer name");
                        let field =
                            super::find_font::control(ui, owner.with("name"), "Layer name", |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut draft.name)
                                        .char_limit(1024)
                                        .desired_width(ui.available_width()),
                                )
                            })
                            .labelled_by(label.id);
                        if draft.focus {
                            field.request_focus();
                            draft.focus = false;
                        }
                        ui.label("Document");
                        let selected = draft
                            .destinations
                            .get(draft.selected)
                            .map(|(name, _)| name.as_str())
                            .unwrap_or("New document");
                        let combo = egui::ComboBox::from_id_salt(owner.with("document"))
                            .selected_text(selected)
                            .width(ui.available_width().min(360.))
                            .show_ui(ui, |ui| {
                                for (i, (label, _)) in draft.destinations.iter().enumerate() {
                                    if ui.selectable_value(&mut draft.selected, i, label).clicked()
                                    {
                                        ui.close();
                                    }
                                }
                                let new = draft.destinations.len();
                                if ui
                                    .selectable_value(&mut draft.selected, new, "New document")
                                    .clicked()
                                {
                                    ui.close();
                                }
                            });
                        ctx.accesskit_node_builder(combo.response.id, |node| {
                            node.set_label("Destination document")
                        });
                        super::welcome::focus_control(ui, &combo.response);
                        if draft.selected == draft.destinations.len() {
                            let label = ui.label("New document name");
                            super::find_font::control(
                                ui,
                                owner.with("document-name"),
                                "New document name",
                                |ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut draft.document_name)
                                            .char_limit(1024)
                                            .desired_width(ui.available_width()),
                                    )
                                },
                            )
                            .labelled_by(label.id);
                        }
                        if let Some(error) = &draft.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        ui.horizontal_wrapped(|ui| {
                            apply = super::find_font::control(
                                ui,
                                owner.with("apply"),
                                "Duplicate",
                                |ui| ui.button("Duplicate"),
                            )
                            .clicked();
                        });
                    });
                    cancel = super::find_font::control(ui, owner.with("cancel"), "Cancel", |ui| {
                        ui.button("Cancel")
                    })
                    .clicked();
                });
        });
    cancel |= modal.should_close();
    if apply && !cancel {
        let destination = draft
            .destinations
            .get(draft.selected)
            .map(|(_, d)| d.clone())
            .unwrap_or_else(|| LayerCopyDestination::New {
                name: draft.document_name.clone(),
            });
        match studio.duplicate_layer_to(&draft.source, &draft.name, &destination) {
            Ok(()) => cancel = true,
            Err(error) => {
                studio.status = error.clone();
                draft.error = Some(error);
            }
        }
    }
    if cancel {
        ctx.data_mut(|data| data.remove::<Draft>(Id::new(ID)));
        ctx.request_discard("Close Duplicate Layer before the next document input");
    } else {
        ctx.data_mut(|data| data.insert_temp(Id::new(ID), draft));
    }
}

#[cfg(test)]
#[path = "layer_duplicate_tests.rs"]
mod tests;
