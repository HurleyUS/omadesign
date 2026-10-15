//! Small property controls. Applications own themes, values and commands.
use eframe::egui::{self, Response, Ui};

#[derive(Clone, Copy, Default)]
pub struct FieldState<'a> {
    pub mixed: bool,
    pub locked: bool,
    pub error: Option<&'a str>,
}

pub struct FieldResponse {
    pub response: Response,
    pub changed: bool,
    pub committed: bool,
    pub cancelled: bool,
}

/// Draw a property field.
/// Args: UI, stable identity, label, value, range, unit, reset value and selection state.
/// Returns the focus, change, commit and cancel signals; the caller owns document edits.
pub fn number<N: egui::emath::Numeric>(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    unit: &str,
    reset: Option<N>,
    state: FieldState<'_>,
) -> FieldResponse {
    let before = *value;
    let key = ui.make_persistent_id(id);
    ui.push_id(key, |ui| {
        ui.vertical(|ui| {
            ui.set_min_width(0.0);
            let label_response = ui.add(egui::Label::new(label).wrap());
            let mut response = ui
                .add_enabled_ui(!state.locked, |ui| {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width()
                            - if reset.is_some() { 48.0 } else { 0.0 })
                        .max(24.0);
                        let mut drag = egui::DragValue::new(value)
                            .range(range)
                            .speed(0.5)
                            .max_decimals(2)
                            .suffix(unit);
                        if state.mixed {
                            drag = drag.custom_formatter(|_, _| "Mixed".into());
                        }
                        let mut response = ui
                            .add_sized([width, 24.0], drag)
                            .labelled_by(label_response.id)
                            .on_hover_text(label);
                        if let Some(reset) = reset {
                            if ui
                                .small_button("Reset")
                                .on_hover_text(format!("Reset {label}"))
                                .clicked()
                            {
                                *value = reset;
                                response.mark_changed();
                            }
                        }
                        response
                    })
                    .inner
                })
                .inner;
            let cancelled = (response.has_focus() || response.lost_focus())
                && ui.input(|input| input.key_pressed(egui::Key::Escape));
            if cancelled {
                *value = before;
            }
            let invalid = !value.to_f64().is_finite();
            if invalid {
                *value = before;
                response.mark_changed();
            }
            if let Some(error) = state.error.or(invalid.then_some("Enter a finite number")) {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            let changed = response.changed() && !cancelled && !invalid && !state.locked;
            let committed = changed || response.drag_stopped() || response.lost_focus();
            FieldResponse {
                response,
                changed,
                committed,
                cancelled,
            }
        })
        .inner
    })
    .inner
}

/// Draw an application action.
/// Args: UI, label and enabled state. Returns the native response for focus and activation.
pub fn action(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    ui.add_enabled(enabled, egui::Button::new(label))
}

/// Draw a panel tab.
/// Args: UI, label and active state. Returns the native response for selection and focus.
pub fn tab(ui: &mut Ui, label: &str, active: bool) -> Response {
    ui.selectable_label(active, label)
}

/// Draw a named panel section.
/// Args: UI, title and application content. Returns the application's result.
pub fn section<R>(ui: &mut Ui, title: &str, content: impl FnOnce(&mut Ui) -> R) -> R {
    ui.group(|ui| {
        ui.label(egui::RichText::new(title).strong());
        content(ui)
    })
    .inner
}
