use super::{pixel_menu_tests::Seat, *};
use crate::{
    document::{Document, Layer, Shape, Style},
    geom::{Geom, Pt},
    motion::{Ease, Prop},
    ui::welcome::keyboard_tests as keys,
};
type Hit = (usize, u64);

fn editor() -> (Studio, Hit) {
    let mut studio = Studio::new();
    studio.show_welcome = false;
    studio.persona = Persona::Motion;
    studio.doc = Document::new("Animation menu workflow", 400., 240., 96.);
    studio.doc.workspace = Some(Persona::Motion);
    studio.doc.layers = vec![Layer::vector("Motion")];
    let shape = Shape::new(
        Geom::Rect { origin: Pt::new(40., 40.), size: Pt::new(80., 60.), radius: 0. },
        Style::default(),
    );
    let hit = (0, shape.id);
    studio.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    studio.active_layer = Some(0);
    studio.selection = vec![hit];
    studio.playhead = 0.5;
    studio.mark();
    studio.history.clear();
    (studio, hit)
}
fn saved(studio: &Studio) -> String {
    crate::project::encode(&studio.doc).unwrap()
}
fn reopen(studio: &Studio) {
    let output = saved(studio);
    let doc = crate::project::decode(&output).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), output);
    assert_eq!(doc.motion, studio.doc.motion);
}
fn keys_at(studio: &Studio, id: u64, prop: Prop) -> Vec<(f32, Ease)> {
    studio.doc.motion.tracks.iter()
        .filter(|track| track.shape == id && track.prop == prop)
        .flat_map(|track| track.keys.iter().map(|key| (key.t, key.ease)))
        .collect()
}
fn undo_redo(studio: &mut Studio, before: &str) {
    let after = saved(studio);
    studio.undo();
    assert_eq!(saved(studio), before);
    studio.redo();
    assert_eq!(saved(studio), after);
}

#[test]
fn craft_animation_menu_add_keyframe_keys_selection_in_one_undo() {
    let (mut studio, hit) = editor();
    let before = saved(&studio);
    let mut seat = Seat::new();
    seat.frame(&mut studio, vec![]);
    seat.click(&mut studio, "Animation");
    seat.invoke(&mut studio, "Add keyframe K");
    assert!(keys_at(&studio, hit.1, Prop::X).contains(&(0.5, Ease::EaseInOut)));
    assert!(!keys_at(&studio, hit.1, Prop::Opacity).is_empty());
    assert_eq!(studio.history.len(), 1);
    reopen(&studio);
    undo_redo(&mut studio, &before);
}

#[test]
fn craft_animation_menu_ease_submenu_uses_the_chosen_ease() {
    for (ease, label) in animation_menu::EASES {
        let (mut studio, hit) = editor();
        let before = saved(&studio);
        let mut seat = Seat::new();
        seat.frame(&mut studio, vec![]);
        seat.click(&mut studio, "Animation");
        seat.invoke(&mut studio, "Add keyframe with ease ⏵");
        seat.invoke(&mut studio, label);
        assert!(keys_at(&studio, hit.1, Prop::X).contains(&(0.5, ease)), "{label}");
        assert_eq!(studio.history.len(), 1);
        reopen(&studio);
        undo_redo(&mut studio, &before);
    }
}

#[test]
fn craft_animation_menu_motion_preset_applies_one_step() {
    let (mut studio, hit) = editor();
    let before = saved(&studio);
    let expected = {
        let mut direct = editor().0;
        direct.selection = studio.selection.clone();
        direct.doc = studio.doc.clone();
        direct.apply_motion_preset(crate::motion_presets::Preset::PopIn);
        direct.doc.motion.clone()
    };
    let mut seat = Seat::new();
    seat.frame(&mut studio, vec![]);
    seat.click(&mut studio, "Animation");
    seat.invoke(&mut studio, "Motion presets ⏵");
    seat.invoke(&mut studio, "Pop in");
    assert_eq!(studio.doc.motion, expected);
    assert!(studio.doc.motion.tracks.iter().any(|track| track.shape == hit.1));
    assert_eq!(studio.history.len(), 1);
    reopen(&studio);
    undo_redo(&mut studio, &before);
}

#[test]
fn craft_animation_menu_loop_playback_toggles_in_one_undo() {
    let (mut studio, _) = editor();
    assert!(studio.doc.motion.looped);
    let before = saved(&studio);
    let mut seat = Seat::new();
    seat.frame(&mut studio, vec![]);
    seat.click(&mut studio, "Animation");
    seat.invoke(&mut studio, "Loop playback");
    assert!(!studio.doc.motion.looped);
    assert_eq!(studio.history.len(), 1);
    reopen(&studio);
    undo_redo(&mut studio, &before);
}

#[test]
fn craft_animation_menu_refuses_locked_hidden_empty_and_busy_targets() {
    for case in 0..4 {
        let (mut studio, hit) = editor();
        match case {
            0 => studio.doc.find_shape_mut(hit.0, hit.1).unwrap().locked = true,
            1 => studio.doc.find_shape_mut(hit.0, hit.1).unwrap().visible = false,
            2 => studio.selection.clear(),
            _ => studio.updates.freezing = true,
        }
        let before = saved(&studio);
        let mut seat = Seat::new();
        seat.frame(&mut studio, vec![]);
        seat.click(&mut studio, "Animation");
        assert!(keys::named(&seat.output, "Add keyframe K").1.is_disabled(), "case {case}");
        assert!(keys::named(&seat.output, "Add keyframe with ease ⏵").1.is_disabled(), "case {case}");
        assert!(keys::named(&seat.output, "Motion presets ⏵").1.is_disabled(), "case {case}");
        assert!(!animation_menu::can_key(&studio));
        if case == 3 {
            animation_menu::toggle_loop(&mut studio);
        }
        assert_eq!(saved(&studio), before);
        assert_eq!(studio.history.len(), 0);
    }
}

#[test]
fn craft_animation_menu_keeps_panel_shortcuts() {
    let (mut studio, _) = editor();
    let mut seat = Seat::new();
    seat.frame(&mut studio, vec![]);
    seat.click(&mut studio, "Animation");
    for panel in ["timeline", "keyframes"] {
        let label = super::super::panel_workspace::PANELS.iter().find(|e| e.0 == panel).unwrap().1;
        keys::named(&seat.output, label);
    }
}


#[test]
fn craft_animation_menu_reverse_and_clear_are_single_undo_steps() {
    use super::animation_menu::{clear_animation, reverse_animation};
    let (mut studio, (_, id)) = editor();
    assert_eq!(reverse_animation(&mut studio), 0);
    studio.playhead = 0.;
    studio.key_selection(Ease::Linear);
    studio.playhead = 0.5;
    studio.key_selection(Ease::EaseIn);
    let before = saved(&studio);
    let history = studio.history.len();
    let duration = studio.doc.motion.duration;
    assert_eq!(reverse_animation(&mut studio), 1);
    assert_eq!(studio.history.len(), history + 1);
    let track = studio.doc.motion.tracks.iter().find(|t| t.shape == id).unwrap();
    assert!(track.keys.windows(2).all(|w| w[0].t <= w[1].t));
    assert_eq!(track.keys.first().unwrap().t, duration - 0.5);
    assert_eq!(track.keys.last().unwrap().t, duration);
    reopen(&studio);
    studio.undo();
    assert_eq!(saved(&studio), before);
    assert_eq!(clear_animation(&mut studio), 1);
    assert!(!studio.doc.motion.tracks.iter().any(|t| t.shape == id));
    studio.undo();
    assert_eq!(saved(&studio), before);
}
