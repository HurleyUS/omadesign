use super::{pixel_menu_tests::Seat, *};
use crate::{photo::DevelopParams, ui::welcome::keyboard_tests as keys};

fn editor() -> Studio {
    let mut studio = Studio::new();
    studio.show_welcome = false;
    studio.persona = Persona::Photo;
    let (w, h) = (64u32, 48u32);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let v = (40 + (x * 2 + y) % 90) as u8;
            pixels.extend_from_slice(&[v, v / 2 + 20, 255 - v, 255]);
        }
    }
    studio.photo.import_image(
        "Develop menu".into(),
        crate::photo::RgbaImage::new(w, h, pixels).unwrap(),
    );
    studio.photo.select_image(0);
    studio
}
fn develop(studio: &Studio) -> DevelopParams {
    studio.photo.selected().unwrap().develop.clone()
}
fn reopen(studio: &Studio) {
    let saved = serde_json::to_string(&develop(studio)).unwrap();
    let back: DevelopParams = serde_json::from_str(&saved).unwrap();
    assert_eq!(back, develop(studio));
}
fn undo_redo(studio: &mut Studio, before: &DevelopParams) {
    let after = develop(studio);
    studio.undo();
    assert_eq!(&develop(studio), before);
    studio.redo();
    assert_eq!(develop(studio), after);
}
fn open_develop(studio: &mut Studio) -> Seat {
    let mut seat = Seat::new();
    seat.frame(studio, vec![]);
    seat.click(studio, "Develop");
    seat
}

#[test]
fn craft_develop_menu_auto_light_is_one_photo_undo() {
    let mut studio = editor();
    let before = develop(&studio);
    let mut expected = before.clone();
    crate::photo::auto_tone(&mut expected, &studio.photo.selected().unwrap().preview);
    assert_ne!(expected, before, "fixture must give auto light something to do");
    let mut seat = open_develop(&mut studio);
    seat.invoke(&mut studio, "Auto light");
    assert_eq!(develop(&studio), expected);
    assert_eq!(studio.photo.history_steps(), vec![1]);
    reopen(&studio);
    undo_redo(&mut studio, &before);
}

#[test]
fn craft_develop_menu_clear_crop_and_reset_each_take_one_step() {
    let mut studio = editor();
    studio.photo.selected_mut().unwrap().develop.exposure = 1.25;
    studio.photo.selected_mut().unwrap().develop.crop = Some([0.1, 0.2, 0.8, 0.9]);
    let start = develop(&studio);
    let mut seat = open_develop(&mut studio);
    seat.invoke(&mut studio, "Clear crop");
    assert_eq!(develop(&studio).crop, None);
    assert_eq!(develop(&studio).exposure, 1.25);
    assert_eq!(studio.photo.history_steps().len(), 1);
    undo_redo(&mut studio, &start);
    let cleared = develop(&studio);
    seat.click(&mut studio, "Develop");
    seat.invoke(&mut studio, "Reset all adjustments");
    assert_eq!(develop(&studio), DevelopParams::default());
    assert_eq!(studio.photo.history_steps().len(), 2);
    reopen(&studio);
    undo_redo(&mut studio, &cleared);
    seat.click(&mut studio, "Develop");
    assert!(keys::named(&seat.output, "Clear crop").1.is_disabled());
}

#[test]
fn craft_develop_menu_show_original_changes_no_settings() {
    let mut studio = editor();
    let before = develop(&studio);
    let mut seat = open_develop(&mut studio);
    seat.invoke(&mut studio, "Show original");
    assert!(studio.photo.show_original);
    assert_eq!(develop(&studio), before);
    assert!(studio.photo.history_steps().is_empty());
}

#[test]
fn craft_develop_menu_refuses_without_a_photo_or_while_saving() {
    let mut empty = Studio::new();
    empty.show_welcome = false;
    empty.persona = Persona::Photo;
    let seat = open_develop(&mut empty);
    for name in ["Auto light", "Clear crop", "Reset all adjustments", "Show original"] {
        assert!(keys::named(&seat.output, name).1.is_disabled(), "{name}");
    }
    let mut studio = editor();
    let seat = open_develop(&mut studio);
    assert!(keys::named(&seat.output, "Clear crop").1.is_disabled());
    assert!(keys::named(&seat.output, "Reset all adjustments").1.is_disabled());
    let before = develop(&studio);
    studio.photo.selection.clear();
    studio.photo.selected = None;
    develop_menu::develop_step(&mut studio, |image| image.develop.exposure = 2.0);
    assert!(studio.photo.history_steps().is_empty());
    assert_eq!(studio.photo.images[0].develop, before);
}

#[test]
fn craft_develop_menu_keeps_panel_shortcuts() {
    let mut studio = editor();
    let seat = open_develop(&mut studio);
    for panel in ["photo-develop", "histogram", "presets"] {
        let label = super::super::panel_workspace::PANELS.iter().find(|e| e.0 == panel).unwrap().1;
        keys::named(&seat.output, label);
    }
}

fn panel_frame(ctx: &eframe::egui::Context, studio: &mut Studio, events: Vec<eframe::egui::Event>) -> eframe::egui::FullOutput {
    let mut output = ctx.run_ui(
        eframe::egui::RawInput {
            screen_rect: Some(eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO, eframe::egui::vec2(420., 1000.))),
            events,
            ..Default::default()
        },
        |ui| crate::ui::photo::develop_content(ui, studio),
    );
    output.textures_delta.clear();
    output
}

fn click_panel(ctx: &eframe::egui::Context, studio: &mut Studio, name: &str) {
    use eframe::egui::{Event, Modifiers, PointerButton};
    let output = panel_frame(ctx, studio, vec![]);
    let output = if output.platform_output.accesskit_update.is_some() { output } else { panel_frame(ctx, studio, vec![]) };
    let bounds = keys::named(&output, name).1.bounds().unwrap();
    let pos = eframe::egui::pos2(((bounds.x0 + bounds.x1) / 2.) as f32, ((bounds.y0 + bounds.y1) / 2.) as f32);
    panel_frame(ctx, studio, vec![Event::PointerMoved(pos)]);
    for pressed in [true, false] {
        panel_frame(ctx, studio, vec![Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }]);
    }
    panel_frame(ctx, studio, vec![]);
}

#[test]
fn craft_develop_auto_light_panel_button_is_one_photo_undo() {
    let mut studio = editor();
    let before = develop(&studio);
    let mut expected = before.clone();
    crate::photo::auto_tone(&mut expected, &studio.photo.selected().unwrap().preview);
    assert_ne!(expected, before);
    let ctx = eframe::egui::Context::default();
    ctx.enable_accesskit();
    crate::ui::theme::apply(&ctx);
    click_panel(&ctx, &mut studio, "Auto light");
    assert_eq!(develop(&studio), expected);
    assert_eq!(studio.photo.history_steps(), vec![1]);
    reopen(&studio);
    undo_redo(&mut studio, &before);
    // A second press is a separate step, never merged into the first.
    studio.photo.selected_mut().unwrap().develop.exposure = -1.0;
    let changed = develop(&studio);
    click_panel(&ctx, &mut studio, "Auto light");
    assert_eq!(studio.photo.history_steps().len(), 2);
    studio.undo();
    assert_eq!(develop(&studio), changed);
}
