use super::*;
use crate::graphic_styles::Appearance;

#[test]
fn craft_linked_style_redefine_updates_every_member_in_one_undo_and_round_trips() {
    let mut s = crate::app::symbols::tests::fixture();
    let (layer, a) = s.selection[0];
    let b = s.selection[1].1;
    s.selection = vec![(layer, a), (layer, b)];
    let style = s.create_linked_style("Card").unwrap();
    assert_eq!(s.doc.linked_styles[0].members, vec![a, b]);
    let before = crate::project::encode(&s.doc).unwrap();
    s.doc.find_shape_mut(layer, a).unwrap().opacity = 0.4;
    s.selection = vec![(layer, a)];
    let history = s.history.len();
    s.redefine_linked_style(style).unwrap();
    assert_eq!(s.history.len(), history + 1);
    let target = Appearance::capture(s.doc.find_shape(layer, b).unwrap());
    assert_eq!(target.opacity, 0.4);
    assert_eq!(s.doc.linked_styles[0].appearance, target);
    let saved = crate::project::encode(&s.doc).unwrap();
    assert!(saved.contains("\"version\":39") || saved.contains("\"version\": 39"));
    let reopened = crate::project::decode(&saved).unwrap();
    assert_eq!(crate::project::encode(&reopened).unwrap(), saved);
    s.undo();
    assert_eq!(s.doc.find_shape(layer, b).unwrap().opacity, Appearance::capture(&crate::project::decode(&before).unwrap().find_shape(layer, b).unwrap()).opacity);
    assert_ne!(s.doc.linked_styles[0].appearance.opacity, 0.4);
}

#[test]
fn craft_linked_style_apply_unlink_and_delete_keep_artwork() {
    let mut s = crate::app::symbols::tests::fixture();
    let (layer, a) = s.selection[0];
    let b = s.selection[1].1;
    s.selection = vec![(layer, a)];
    s.doc.find_shape_mut(layer, a).unwrap().opacity = 0.3;
    let style = s.create_linked_style("Muted").unwrap();
    s.selection = vec![(layer, b)];
    s.apply_linked_style(style).unwrap();
    assert_eq!(s.doc.find_shape(layer, b).unwrap().opacity, 0.3);
    assert_eq!(crate::linked_styles::style_of(&s.doc.linked_styles, b), Some(style));
    s.unlink_linked_style().unwrap();
    assert_eq!(crate::linked_styles::style_of(&s.doc.linked_styles, b), None);
    assert_eq!(s.doc.find_shape(layer, b).unwrap().opacity, 0.3);
    s.delete_linked_style(style).unwrap();
    assert!(s.doc.linked_styles.is_empty());
    assert!(s.apply_linked_style(style).is_err());
    s.selection = vec![(layer, a)];
    let id = s.create_linked_style("Locked").unwrap();
    s.doc.find_shape_mut(layer, a).unwrap().locked = true;
    s.selection = vec![(layer, b)];
    let n = s.history.len();
    assert!(s.redefine_linked_style(id).is_err());
    assert_eq!(s.history.len(), n);
}
