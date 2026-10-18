use crate::app::{
    Studio,
    layer_merge::{Kind, Prepared, Target},
};
use eframe::egui::{self, Id, Ui};

const ID: &str = "native-layer-merge";
#[derive(Clone)]
struct State {
    instance: Id,
    target: Target,
    kind: Kind,
    error: Option<String>,
}

pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<State>(Id::new(ID)).is_some())
}

/// Start the existing document-owned job mechanism for a real layer operation.
/// Args: UI context, live editor and captured layer operation. Returns nothing; failure never edits the document.
pub(crate) fn start(ctx: &egui::Context, studio: &mut Studio, kind: Kind) {
    if is_open(ctx) {
        return;
    }
    let request = match studio.capture_layer_merge(kind) {
        Ok(request) => request,
        Err(error) => {
            studio.status = error;
            return;
        }
    };
    let state = State {
        instance: super::keyboard_controls::draft_id(ctx, ID),
        target: request.target.clone(),
        kind,
        error: None,
    };
    ctx.data_mut(|data| data.insert_temp(Id::new(ID), state));
    super::jobs::start_with_progress(ctx, ID, 1, move |progress| {
        request.render(progress.as_ref())
    });
}

/// Add the actual merge commands to an existing Layer menu.
/// Args: native menu UI, editor and optional concrete row owner. Returns nothing; each accepted command starts its captured job.
pub(super) fn menu(ui: &mut Ui, studio: &Studio, owner: Option<u64>) -> Option<Kind> {
    let mut selected = None;
    for kind in [
        owner.map(Kind::Down),
        owner.map(Kind::Group),
        Some(Kind::Visible),
        Some(Kind::Flatten),
    ]
    .into_iter()
    .flatten()
    {
        let enabled = !is_open(ui.ctx()) && studio.can_layer_merge(kind);
        let mut button = egui::Button::new(kind.name());
        if studio.persona == crate::tools::Persona::Pixel {
            if kind == Kind::Visible {
                button = button.shortcut_text("Ctrl+Shift+E");
            } else if studio
                .active_layer
                .and_then(|i| studio.doc.layers.get(i))
                .map(|l| l.id)
                == owner
                && studio.pixel_merge_shortcut(false) == Some(kind)
            {
                button = button.shortcut_text("Ctrl+E");
            }
        }
        if ui.add_enabled(enabled, button).clicked() {
            selected = Some(kind);
        }
    }
    if selected.is_some() {
        ui.close();
    }
    selected
}

/// Publish completed output after drawing the real Cancel control.
/// Args: UI context and owning editor. Returns nothing; Cancel/Escape/stale owners retire the receiver before publication.
pub(super) fn show(ctx: &egui::Context, studio: &mut Studio) {
    let Some(mut state) = ctx.data(|data| data.get_temp::<State>(Id::new(ID))) else {
        return;
    };
    let escape = ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let stale =
        studio.swap_id != state.target.document || studio.last_input != state.target.revision;
    let mut close = false;
    let modal = egui::Modal::new(Id::new(ID))
        .backdrop_color(super::theme::bg_window().gamma_multiply(0.55))
        .show(ctx, |ui| {
            ui.set_max_width((ctx.content_rect().width() - 32.).clamp(120., 380.));
            ui.heading(state.kind.name());
            if let Some(error) = &state.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            } else {
                let stage = super::jobs::stage::<Prepared>(ctx, ID);
                ui.label(if stage.is_empty() {
                    "Preparing layers"
                } else {
                    stage.as_str()
                });
                let (done, total) =
                    super::jobs::frame_progress::<Prepared>(ctx, ID).unwrap_or((0, 1));
                ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32));
            }
            let label = if state.error.is_some() {
                "Close"
            } else {
                "Cancel"
            };
            close = super::find_font::control(ui, state.instance.with("cancel"), label, |ui| {
                ui.button(label)
            })
            .clicked();
        });
    close |= escape || modal.should_close() || stale;
    if !close {
        if let Some(result) = super::jobs::poll::<Prepared>(ctx, ID) {
            match result.and_then(|prepared| studio.apply_layer_merge(prepared)) {
                Ok(()) => close = true,
                Err(error) => {
                    studio.status = error.clone();
                    state.error = Some(error);
                }
            }
        }
    }
    if close {
        super::jobs::cancel::<Prepared>(ctx, ID);
        ctx.data_mut(|data| data.remove::<State>(Id::new(ID)));
        if stale {
            studio.status = "The merge was cancelled because its document changed".into();
        }
        ctx.request_discard("Finish the layer merge modal before document input");
    } else {
        ctx.data_mut(|data| data.insert_temp(Id::new(ID), state));
        ctx.request_repaint();
    }
}

#[cfg(test)]
#[path = "layer_merge_tests.rs"]
mod tests;
