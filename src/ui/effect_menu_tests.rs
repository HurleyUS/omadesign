use super::{pixel_menu_tests::Seat, *};
use crate::{
    document::{Document, Layer, Shape, Style},
    geom::{Geom, Pt},
    ui::welcome::keyboard_tests as keys,
};
type Hit = (usize, u64);

fn editor() -> (Studio, Hit) {
    let mut studio = Studio::new();
    studio.show_welcome = false;
    studio.persona = Persona::Design;
    studio.doc = Document::new("Effect menu workflow", 400., 240., 96.);
    studio.doc.workspace = Some(Persona::Design);
    studio.doc.layers = vec![Layer::vector("Art")];
    let shape = Shape::new(
        Geom::Rect { origin: Pt::new(40., 40.), size: Pt::new(80., 60.), radius: 0. },
        Style::default(),
    );
    let hit = (0, shape.id);
    studio.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    studio.active_layer = Some(0);
    studio.selection = vec![hit];
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
    assert_eq!(doc.layers[0].kind.shapes(), studio.doc.layers[0].kind.shapes());
}
fn undo_redo(studio: &mut Studio, before: &str) {
    let after = saved(studio);
    studio.undo();
    assert_eq!(saved(studio), before);
    studio.redo();
    assert_eq!(saved(studio), after);
}


fn second(studio: &mut Studio) -> Hit {
    let shape = Shape::new(
        Geom::Rect { origin: Pt::new(200., 60.), size: Pt::new(50., 50.), radius: 4. },
        Style::default(),
    );
    let hit = (0, shape.id);
    studio.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    studio.selection.push(hit);
    studio.history.clear();
    hit
}
fn effects(studio: &Studio, hit: Hit) -> Vec<String> {
    studio.doc.find_shape(hit.0, hit.1).unwrap().filters.items.iter().map(|fx| fx.name().to_string()).collect()
}
fn open_effect(studio: &mut Studio) -> Seat {
    let mut seat = Seat::new();
    seat.frame(studio, vec![]);
    seat.click(studio, "Effect");
    seat
}
fn shadow() -> crate::filter::Fx {
    crate::filter::Fx::catalog().iter().find(|e| e.0 == "Drop shadow").unwrap().1()
}

#[test]
fn craft_effect_menu_add_effect_reaches_every_selected_object_in_one_undo() {
    let (mut studio, a) = editor();
    let b = second(&mut studio);
    let before = saved(&studio);
    let mut seat = open_effect(&mut studio);
    seat.invoke(&mut studio, "Add effect ⏵");
    seat.invoke(&mut studio, "Drop shadow");
    let name = shadow().name().to_string();
    assert_eq!(effects(&studio, a), vec![name.clone()]);
    assert_eq!(effects(&studio, b), vec![name]);
    assert_eq!(studio.history.len(), 1);
    reopen(&studio);
    undo_redo(&mut studio, &before);
}

#[test]
fn craft_effect_menu_clear_effects_is_one_undo() {
    let (mut studio, a) = editor();
    let b = second(&mut studio);
    effect_menu::add_effect(&mut studio, || shadow());
    effect_menu::add_effect(&mut studio, crate::filter::Fx::catalog()[0].1);
    studio.history.clear();
    let before = saved(&studio);
    let mut seat = open_effect(&mut studio);
    seat.invoke(&mut studio, "Clear effects");
    assert!(effects(&studio, a).is_empty() && effects(&studio, b).is_empty());
    assert_eq!(studio.history.len(), 1);
    reopen(&studio);
    undo_redo(&mut studio, &before);
    seat.click(&mut studio, "Effect");
    assert!(keys::named(&seat.output, "Clear effects").1.is_disabled());
}

#[test]
fn craft_effect_menu_skips_locked_objects_in_a_mixed_selection() {
    let (mut studio, a) = editor();
    let b = second(&mut studio);
    studio.doc.find_shape_mut(b.0, b.1).unwrap().locked = true;
    assert_eq!(effect_menu::add_effect(&mut studio, || shadow()), 1);
    assert_eq!(effects(&studio, a).len(), 1);
    assert!(effects(&studio, b).is_empty());
    assert_eq!(studio.history.len(), 1);
}

#[test]
fn craft_effect_menu_refuses_locked_hidden_empty_and_busy_targets() {
    for case in 0..4 {
        let (mut studio, hit) = editor();
        match case {
            0 => studio.doc.find_shape_mut(hit.0, hit.1).unwrap().locked = true,
            1 => studio.doc.find_shape_mut(hit.0, hit.1).unwrap().visible = false,
            2 => studio.selection.clear(),
            _ => studio.updates.freezing = true,
        }
        let before = saved(&studio);
        let seat = open_effect(&mut studio);
        assert!(keys::named(&seat.output, "Add effect ⏵").1.is_disabled(), "case {case}");
        assert!(keys::named(&seat.output, "Clear effects").1.is_disabled(), "case {case}");
        assert_eq!(effect_menu::add_effect(&mut studio, || shadow()), 0);
        assert_eq!(effect_menu::clear_effects(&mut studio), 0);
        assert_eq!(saved(&studio), before);
        assert_eq!(studio.history.len(), 0);
    }
}

#[test]
fn craft_effect_menu_keeps_panel_shortcuts() {
    let (mut studio, _) = editor();
    let seat = open_effect(&mut studio);
    for panel in ["effects", "appearance", "transparency"] {
        let label = super::super::panel_workspace::PANELS.iter().find(|e| e.0 == panel).unwrap().1;
        keys::named(&seat.output, label);
    }
}
