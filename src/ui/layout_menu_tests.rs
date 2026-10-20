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
    studio.persona = Persona::Layout;
    studio.doc = Document::new("Layout menu workflow", 400., 240., 96.);
    studio.doc.workspace = Some(Persona::Layout);
    studio.doc.layers = vec![Layer::vector("Frames")];
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


fn shape(studio: &Studio, id: u64) -> &Shape {
    studio.doc.find_shape(0, id).unwrap()
}
fn open_layout(studio: &mut Studio) -> Seat {
    let mut seat = Seat::new();
    seat.frame(studio, vec![]);
    seat.click(studio, "Layout");
    seat
}

#[test]
fn craft_layout_menu_wrap_in_frame_parents_selection_in_one_undo() {
    let (mut studio, hit) = editor();
    let before = saved(&studio);
    let mut seat = open_layout(&mut studio);
    seat.invoke(&mut studio, "Wrap in frame");
    let frame = studio.selection[0].1;
    assert_ne!(frame, hit.1);
    assert!(shape(&studio, frame).layout.frame);
    assert_eq!(shape(&studio, hit.1).layout.parent, Some(frame));
    assert_eq!(studio.history.len(), 1);
    reopen(&studio);
    undo_redo(&mut studio, &before);
}

#[test]
fn craft_layout_menu_auto_layout_adds_a_stacked_frame() {
    let (mut studio, hit) = editor();
    let before = saved(&studio);
    let mut seat = open_layout(&mut studio);
    seat.invoke(&mut studio, "Add auto layout");
    let frame = shape(&studio, hit.1).layout.parent.expect("wrapped");
    assert!(shape(&studio, frame).layout.stack.is_some());
    assert_eq!(studio.history.len(), 1);
    reopen(&studio);
    undo_redo(&mut studio, &before);
}

#[test]
fn craft_layout_menu_create_component_binds_the_frame() {
    let (mut studio, _) = editor();
    studio.wrap_selection_frame();
    let frame = studio.selection[0].1;
    studio.history.clear();
    let before = saved(&studio);
    let mut seat = open_layout(&mut studio);
    seat.invoke(&mut studio, "Create component");
    assert!(matches!(
        shape(&studio, frame).layout.component,
        Some(crate::layout_components::ComponentBinding::Main { .. })
    ));
    assert_eq!(studio.history.len(), 1);
    reopen(&studio);
    undo_redo(&mut studio, &before);
    seat.click(&mut studio, "Layout");
    assert!(keys::named(&seat.output, "Create component").1.is_disabled());
}

#[test]
fn craft_layout_menu_detach_instance_keeps_objects_editable() {
    let (mut studio, _) = editor();
    studio.wrap_selection_frame();
    let main = studio.selection[0].1;
    studio.create_layout_component();
    studio.insert_layout_instance(main);
    let instance = studio.selection[0].1;
    assert_ne!(instance, main);
    studio.history.clear();
    let before = saved(&studio);
    let mut seat = open_layout(&mut studio);
    seat.invoke(&mut studio, "Detach instance");
    assert!(shape(&studio, instance).layout.component.is_none());
    assert_eq!(studio.history.len(), 1);
    reopen(&studio);
    undo_redo(&mut studio, &before);
}

#[test]
fn craft_layout_menu_refuses_locked_hidden_empty_and_busy_targets() {
    for case in 0..4 {
        let (mut studio, hit) = editor();
        match case {
            0 => studio.doc.find_shape_mut(hit.0, hit.1).unwrap().locked = true,
            1 => studio.doc.find_shape_mut(hit.0, hit.1).unwrap().visible = false,
            2 => studio.selection.clear(),
            _ => studio.updates.freezing = true,
        }
        let before = saved(&studio);
        let seat = open_layout(&mut studio);
        for name in ["Wrap in frame", "Add auto layout", "Create component", "Detach instance"] {
            assert!(keys::named(&seat.output, name).1.is_disabled(), "{name} case {case}");
        }
        assert!(!layout_menu::can_wrap(&studio));
        assert_eq!(saved(&studio), before);
        assert_eq!(studio.history.len(), 0);
    }
}

#[test]
fn craft_layout_menu_keeps_panel_shortcuts() {
    let (mut studio, _) = editor();
    let seat = open_layout(&mut studio);
    for panel in ["layout", "hierarchy", "components"] {
        let label = super::super::panel_workspace::PANELS.iter().find(|e| e.0 == panel).unwrap().1;
        keys::named(&seat.output, label);
    }
}

#[test]
fn craft_layout_menu_remove_auto_layout_keeps_frame_and_positions() {
    let (mut studio, hit) = editor();
    let mut seat = open_layout(&mut studio);
    seat.invoke(&mut studio, "Add auto layout");
    let frame = shape(&studio, hit.1).layout.parent.expect("wrapped");
    studio.selection = vec![(hit.0, frame)];
    let geometry = shape(&studio, hit.1).geom.clone();
    let before = saved(&studio);
    let history = studio.history.len();
    let mut seat = open_layout(&mut studio);
    seat.invoke(&mut studio, "Remove auto layout");
    assert!(shape(&studio, frame).layout.stack.is_none());
    assert!(shape(&studio, frame).layout.frame);
    assert_eq!(shape(&studio, hit.1).layout.parent, Some(frame));
    assert_eq!(shape(&studio, hit.1).geom, geometry);
    assert_eq!(studio.history.len(), history + 1);
    reopen(&studio);
    undo_redo(&mut studio, &before);
}
