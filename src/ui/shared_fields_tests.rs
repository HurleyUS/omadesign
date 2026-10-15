use super::fields::*;
use eframe::egui;
mod tests {
    use super::*;
    #[test]
    fn shared_fields_fit_scales_and_selection_states() {
        for scale in [1.0, 1.5, 2.0] {
            for state in [
                FieldState::default(),
                FieldState {
                    mixed: true,
                    ..Default::default()
                },
                FieldState {
                    locked: true,
                    ..Default::default()
                },
                FieldState {
                    error: Some("Value out of range"),
                    ..Default::default()
                },
            ] {
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(scale);
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320.0, 600.0),
                    )),
                    ..Default::default()
                };
                let mut value = 50.0;
                let _ = ctx.run_ui(input, |ui| {
                    let rect = ui.max_rect();
                    let result = number(
                        ui,
                        "gain",
                        "Long property label that must wrap",
                        &mut value,
                        0.0..=100.0,
                        " %",
                        Some(100.0),
                        state,
                    );
                    assert!(rect.contains_rect(result.response.rect));
                    assert!(!result.changed);
                    assert_eq!(result.response.enabled(), !state.locked);
                });
                assert_eq!(value, 50.0);
            }
        }
    }
}
