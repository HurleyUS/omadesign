use crate::app::{GroupRequest, Studio};
use eframe::egui::{self, Id};

const ID: &str = "group-from-layers-dialog";

#[derive(Clone)]
struct Draft {
    instance: Id,
    request: GroupRequest,
    name: String,
    focus: bool,
    error: Option<String>,
}

pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<Draft>(Id::new(ID)).is_some())
}

/// Open a named group for the current rows or objects without editing them.
/// Args: native UI context and current editor. Returns nothing; Cancel leaves the document unchanged.
pub(super) fn open(ctx: &egui::Context, studio: &mut Studio) {
    let request = match studio.capture_group_request() {
        Ok(request) => request,
        Err(error) => {
            studio.status = error;
            return;
        }
    };
    let draft = Draft {
        instance: super::keyboard_controls::draft_id(ctx, ID),
        request,
        name: "Group".into(),
        focus: true,
        error: None,
    };
    ctx.data_mut(|data| data.insert_temp(Id::new(ID), draft));
}

/// Draw the native name dialog and dispatch one validated grouping transaction.
/// Args: UI context and current editor. Returns nothing; Group rechecks the captured document and selection.
pub(super) fn show(ctx: &egui::Context, studio: &mut Studio) {
    let Some(mut draft) = ctx.data(|data| data.get_temp::<Draft>(Id::new(ID))) else {
        return;
    };
    let current = studio.group_request_current(&draft.request);
    let owner = draft.instance;
    let mut apply = false;
    let mut cancel = false;
    let modal = egui::Modal::new(Id::new(ID))
        .backdrop_color(super::theme::bg_window().gamma_multiply(0.55))
        .show(ctx, |ui| {
            ui.set_max_width(380f32.min(ctx.content_rect().width() - 32.).max(120.));
            ui.heading("Group from Layers");
            if let Err(error) = &current {
                studio.status = error.clone();
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_enabled_ui(current.is_ok(), |ui| {
                let label = ui.label("Name");
                let field = super::find_font::control(ui, owner.with("name"), "Name", |ui| {
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
                if let Some(error) = &draft.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });
            ui.horizontal_wrapped(|ui| {
                apply = super::find_font::control(ui, owner.with("apply"), "Group", |ui| {
                    ui.button("Group")
                })
                .clicked();
                cancel = super::find_font::control(ui, owner.with("cancel"), "Cancel", |ui| {
                    ui.button("Cancel")
                })
                .clicked();
            });
        });
    cancel |= modal.should_close();
    if apply && !cancel {
        match studio.group_request_current(&draft.request) {
            Ok(()) => {
                let history = studio.history.len();
                studio.group_named(draft.name.clone());
                if studio.history.len() > history {
                    cancel = true;
                } else {
                    draft.error = Some(studio.status.clone());
                }
            }
            Err(error) => {
                studio.status = error.clone();
                draft.error = Some(error);
            }
        }
    }
    if cancel {
        ctx.data_mut(|data| data.remove::<Draft>(Id::new(ID)));
        ctx.request_discard("Close Group from Layers before the next document input");
    } else {
        ctx.data_mut(|data| data.insert_temp(Id::new(ID), draft));
    }
}

#[cfg(test)]
#[path = "layer_group_tests.rs"]
mod tests;
