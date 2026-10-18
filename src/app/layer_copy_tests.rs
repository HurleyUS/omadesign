use super::*;
use crate::document::Pixels;
use std::sync::Arc;

fn state(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}

fn history(s: &mut Studio, before: &str, count: usize) {
    let after = state(s);
    assert_ne!(before, after);
    assert_eq!(s.history.len(), count + 1);
    let reopened = crate::project::decode(&after).unwrap();
    assert_eq!(crate::project::encode(&reopened).unwrap(), after);
    s.undo();
    assert_eq!(state(s), before);
    s.redo();
    assert_eq!(state(s), after);
}

fn nested() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Pixel;
    s.doc = Document::new("Copy layers", 64., 48., 96.);
    s.doc.transparent = true;
    let parent = Layer::group("Parent");
    let mut group = Layer::group("Nested");
    group.parent = Some(parent.id);
    let mut pixels = Pixels::from_samples(
        2,
        1,
        crate::deep_color::Depth::F32,
        vec![1.25, -0., -0.125, 0.75, 0.1234567, 0.5, 0.8, 0.25],
    )
    .unwrap();
    pixels
        .assign_rgb_profile(
            include_bytes!("../formats/psd/fixtures/srgb-cc0.icc").to_vec(),
            1,
        )
        .unwrap();
    let mut child =
        Layer::placed_raster("Native pixels", pixels, Pt::new(7., 9.), Pt::new(24., 12.));
    child.parent = Some(group.id);
    if let LayerKind::Raster {
        rotation, shear, ..
    } = &mut child.kind
    {
        *rotation = 0.27;
        *shear = -0.125;
    }
    child.mask = Some(
        Pixels::from_samples(
            2,
            1,
            crate::deep_color::Depth::F32,
            vec![0.1234567, 0.1234567, 0.1234567, 1., 0.75, 0.75, 0.75, 1.],
        )
        .unwrap(),
    );
    child.opacity = 0.625;
    child.blend = crate::color::Blend::Multiply;
    s.doc.layers = vec![child, group, parent, Layer::vector("Sibling")];
    s.active_layer = Some(0);
    s.history.clear();
    s
}

#[test]
fn craft_layer_copy_nested_raster_keeps_native_bits_profile_mask_and_parent_order() {
    for locked in [false, true] {
        let mut s = nested();
        s.doc.layers[0].locked = locked;
        s.doc.layers[2].locked = locked;
        s.doc.layers[0].visible = !locked;
        let original = s.doc.layers[0].clone();
        let before = state(&s);
        assert!(s.duplicate_layer_tree(0));
        s.doc.validate_hierarchy().unwrap();
        let copy = &s.doc.layers[1];
        assert_ne!(copy.id, original.id);
        assert_eq!(copy.parent, original.parent);
        assert_eq!(copy.name, "Native pixels copy");
        assert_eq!(copy.opacity, original.opacity);
        assert_eq!(copy.blend, original.blend);
        assert_eq!(copy.locked, original.locked);
        assert_eq!(copy.visible, original.visible);
        assert_eq!(
            serde_json::to_value(&copy.kind).unwrap(),
            serde_json::to_value(&original.kind).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&copy.mask).unwrap(),
            serde_json::to_value(&original.mask).unwrap()
        );
        let native = copy.kind.pixels().unwrap().native.as_ref().unwrap();
        let old = original.kind.pixels().unwrap().native.as_ref().unwrap();
        assert!(Arc::ptr_eq(&native.samples, &old.samples));
        assert!(Arc::ptr_eq(&native.icc_profile, &old.icc_profile));
        assert_eq!(s.doc.layers[2].name, "Nested");
        assert_eq!(s.doc.layers[3].name, "Parent");
        assert_eq!(s.active_layer, Some(1));
        assert_eq!(s.selected_layer, Some(copy.id));
        assert_eq!(
            serde_json::to_value(&s.doc.layers[0]).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        history(&mut s, &before, 0);
        let before_bits: Vec<_> = s.doc.layers[0]
            .kind
            .pixels()
            .unwrap()
            .native
            .as_ref()
            .unwrap()
            .samples
            .iter()
            .map(|v| v.to_bits())
            .collect();
        let edited = vec![0.25, 0.5, 0.75, 1.].repeat(2);
        s.doc.layers[1]
            .kind
            .pixels_mut()
            .unwrap()
            .set_samples(edited)
            .unwrap();
        assert_eq!(
            s.doc.layers[0]
                .kind
                .pixels()
                .unwrap()
                .native
                .as_ref()
                .unwrap()
                .samples
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            before_bits
        );
    }
}

#[test]
fn craft_layer_copy_nested_group_is_adjacent_and_copies_animation_owners() {
    let mut s = nested();
    let original_group = s.doc.layers[1].id;
    let original_child = s.doc.layers[0].id;
    let parent = s.doc.layers[2].id;
    s.doc.motion.tracks.push(crate::motion::Track {
        shape: original_child,
        prop: Prop::X,
        keys: vec![
            crate::motion::Key::at(0., 0., Ease::Hold),
            crate::motion::Key::at(2., 16., Ease::EaseIn),
        ],
    });
    s.doc
        .motion
        .set_expression(original_child, Prop::Y, Some("time*3".into()))
        .unwrap();
    s.doc
        .motion
        .set_expression(original_group, Prop::Opacity, Some("0.75".into()))
        .unwrap();
    let before = state(&s);
    assert!(s.duplicate_layer_tree(1));
    s.doc.validate_hierarchy().unwrap();
    let child = s.doc.layers[2].id;
    let group = s.doc.layers[3].id;
    assert_eq!(
        s.doc
            .layers
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>(),
        [
            "Native pixels",
            "Nested",
            "Native pixels",
            "Nested copy",
            "Parent",
            "Sibling"
        ]
    );
    assert_ne!(group, original_group);
    assert_ne!(child, original_child);
    assert_eq!(s.doc.layers[2].parent, Some(group));
    assert_eq!(s.doc.layers[3].parent, Some(parent));
    assert_eq!(
        s.doc
            .motion
            .tracks
            .iter()
            .find(|t| t.shape == child)
            .unwrap()
            .keys,
        s.doc
            .motion
            .tracks
            .iter()
            .find(|t| t.shape == original_child)
            .unwrap()
            .keys
    );
    for time in [0., 0.5, 1., 2.] {
        let mut keep = || true;
        let mut evaluation =
            crate::motion::expressions::Evaluation::new(&s.doc.motion, time, &mut keep);
        assert_eq!(evaluation.value(child, Prop::Y).unwrap(), Some(time * 3.));
        assert_eq!(evaluation.value(group, Prop::Opacity).unwrap(), Some(0.75));
    }
    assert_eq!(s.doc.motion.expressions.len(), 4);
    history(&mut s, &before, 0);
}

#[test]
fn craft_layer_copy_vector_frames_masks_components_and_prototype_targets_are_internal() {
    use crate::layout_components::{self as components, ComponentBinding, PropertyKind};
    let mut s = crate::app::symbols::tests::fixture();
    assert!(s.create_vector_symbol("Badge"));
    let main = s.primary().unwrap().1;
    let child = crate::layout::children(&s.doc, 0, main)[0];
    let property =
        components::expose_property(&mut s.doc, 0, main, child, "Ink", PropertyKind::Opacity)
            .unwrap();
    let instance =
        components::insert_instance(&mut s.doc, main, 0, Pt::new(110., 15.), None).unwrap();
    s.doc.find_shape_mut(0, instance).unwrap().opacity = 0.375;
    s.doc
        .find_shape_mut(0, instance)
        .unwrap()
        .layout
        .interactions
        .push(crate::layout_prototype::Interaction {
            trigger: crate::layout_prototype::Trigger::Click,
            action: crate::layout_prototype::PrototypeAction::Navigate { target: main },
            transition: crate::layout_prototype::Transition::Dissolve,
            duration_ms: 180,
        });
    let layer_id = s.doc.layers[0].id;
    let source_mask = crate::artwork_mask::ArtworkMask::new(crate::artwork_mask::MaskSource {
        layer: layer_id,
        shape: Some(child),
    });
    s.doc
        .find_shape_mut(0, instance)
        .unwrap()
        .artwork_masks
        .push(source_mask);
    s.doc.validate_artwork_masks().unwrap();
    let source_ids: HashSet<_> = s.doc.layers[0]
        .kind
        .shapes()
        .unwrap()
        .iter()
        .map(|s| s.id)
        .collect();
    s.history.clear();
    let before = state(&s);
    assert!(s.duplicate_layer_tree(0));
    let copied = s.doc.layers[1].kind.shapes().unwrap();
    assert!(copied.iter().all(|s| !source_ids.contains(&s.id)));
    let new_main = copied
        .iter()
        .find(|s| matches!(s.layout.component, Some(ComponentBinding::Main { .. })))
        .unwrap()
        .id;
    let new_instance = copied
        .iter()
        .find(|s| matches!(s.layout.component, Some(ComponentBinding::Instance { .. })))
        .unwrap()
        .id;
    let Some(ComponentBinding::Instance {
        main: linked,
        nodes,
    }) = &s.doc.find_shape(1, new_instance).unwrap().layout.component
    else {
        panic!()
    };
    assert_eq!(*linked, new_main);
    assert_eq!(
        s.doc
            .find_shape(1, new_instance)
            .unwrap()
            .layout
            .interactions[0]
            .action,
        crate::layout_prototype::PrototypeAction::Navigate { target: new_main }
    );
    assert!(
        nodes
            .iter()
            .all(|n| !source_ids.contains(&n.instance) && n.baseline.id == n.instance)
    );
    let mask = &s.doc.find_shape(1, new_instance).unwrap().artwork_masks[0];
    assert_eq!(mask.source.layer, s.doc.layers[1].id);
    assert!(!source_ids.contains(&mask.source.shape.unwrap()));
    let bound = components::property_object(&s.doc, 1, new_main, property).unwrap();
    assert!(!source_ids.contains(&bound));
    history(&mut s, &before, 0);
    s.doc.find_shape_mut(1, bound).unwrap().opacity = 0.2;
    components::synchronize_all(&mut s.doc).unwrap();
    assert_eq!(s.doc.find_shape(0, child).unwrap().opacity, 1.);
    assert_eq!(s.doc.find_shape(1, bound).unwrap().opacity, 0.2);
}

#[test]
fn craft_layer_copy_text_threads_keep_complete_story_or_materialize_only_copied_frames() {
    for partial in [false, true] {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.doc = Document::new("Thread copies", 520., 180., 96.);
        s.doc.transparent = true;
        s.doc.layers = vec![Layer::vector("Story")];
        s.active_layer = Some(0);
        s.text_px = 18.;
        s.place_area_text(Bounds::from_min_size(Pt::new(15., 15.), Pt::new(180., 65.)));
        s.type_insert(&"The copied story keeps its actual words and character styling. ".repeat(5));
        s.commit_type_edit();
        let head = s.primary().unwrap();
        let next = s.empty_thread_frame(head, Pt::new(260., 15.)).unwrap();
        s.thread_text_frames(head, next).unwrap();
        if partial {
            let follower = s.doc.layers[0].kind.shapes_mut().unwrap().pop().unwrap();
            assert_eq!(follower.id, next.1);
            let mut other = Layer::vector("Other story frame");
            other.kind.shapes_mut().unwrap().push(follower);
            s.doc.layers.push(other);
            crate::text_geometry::reflow(&mut s.doc);
        }
        s.history.clear();
        let before = state(&s);
        let picture = crate::compositor::render_export(&s.doc, 1)
            .unwrap()
            .data()
            .to_vec();
        let old = s.doc.layers[0].kind.shapes().unwrap().to_vec();
        assert!(s.duplicate_layer_tree(0));
        let shapes = s.doc.layers[1].kind.shapes().unwrap();
        assert_eq!(shapes.len(), old.len());
        let Geom::Text(run) = &shapes[0].geom else {
            panic!()
        };
        if partial {
            assert!(run.thread.is_none());
            let Geom::Text(original) = &old[0].geom else {
                panic!()
            };
            assert_eq!(
                run.content,
                original
                    .layout
                    .as_ref()
                    .unwrap()
                    .visible_run
                    .as_ref()
                    .unwrap()
                    .content
            );
        } else {
            assert_eq!(run.thread.as_ref().unwrap().story, shapes[0].id);
            assert_eq!(run.thread.as_ref().unwrap().next, Some(shapes[1].id));
            let Geom::Text(tail) = &shapes[1].geom else {
                panic!()
            };
            assert_eq!(tail.thread.as_ref().unwrap().prev, Some(shapes[0].id));
        }
        let mut isolated = s.doc.clone();
        isolated.layers[0].visible = false;
        assert_eq!(
            crate::compositor::render_export(&isolated, 1)
                .unwrap()
                .data(),
            picture.as_slice()
        );
        history(&mut s, &before, 0);
    }
}

#[test]
fn craft_layer_copy_cmyka_ownership_is_shared_until_the_copy_is_edited() {
    let mut s = nested();
    let inks = vec![0.1234567, 0.25, 0.5, 0.125, 0.75, 0.75, 0.5, 0.25, 0., 1.];
    let p = Pixels::from_cmyk(2, 1, crate::deep_color::Depth::F32, inks, vec![], 0).unwrap();
    *s.doc.layers[0].kind.pixels_mut().unwrap() = p;
    let before = state(&s);
    assert!(s.duplicate_layer_tree(0));
    let original = s.doc.layers[0].kind.pixels().unwrap();
    let copy = s.doc.layers[1].kind.pixels().unwrap();
    assert_eq!(
        serde_json::to_value(copy).unwrap(),
        serde_json::to_value(original).unwrap()
    );
    assert!(Arc::ptr_eq(
        copy.native.as_ref().unwrap().cmyk.as_ref().unwrap(),
        original.native.as_ref().unwrap().cmyk.as_ref().unwrap()
    ));
    history(&mut s, &before, 0);
}

#[test]
fn craft_layer_copy_legacy_empty_layer_selection_duplicates_inside_its_parent() {
    let mut s = nested();
    s.activate_layer_tree(0);
    assert!(s.selection.is_empty());
    let before = state(&s);
    s.duplicate_selection_by(Pt::new(3., 5.));
    s.doc.validate_hierarchy().unwrap();
    assert_eq!(s.doc.layers.len(), 5);
    assert_eq!(
        s.doc.layers[1].kind.raster_xform().unwrap().0,
        Pt::new(10., 14.)
    );
    assert_eq!(s.doc.layers[1].parent, s.doc.layers[0].parent);
    history(&mut s, &before, 0);
}

#[test]
fn craft_layer_copy_refuses_invalid_busy_and_expression_limit_without_losing_redo() {
    for case in 0..4 {
        let mut s = nested();
        s.duplicate_layer_tree(0);
        s.undo();
        let target = if case == 0 { 999 } else { 0 };
        if case == 1 {
            s.updates.freezing = true;
        }
        if case == 2 {
            s.doc.layers[0].parent = Some(s.doc.layers[0].id);
        }
        if case == 3 {
            s.doc.motion.expressions = (0..crate::motion::expressions::MAX_EXPRESSIONS)
                .map(|i| crate::motion::expressions::Expression {
                    shape: if i == 0 {
                        s.doc.layers[0].id
                    } else {
                        crate::document::next_id()
                    },
                    prop: Prop::X,
                    source: "1".into(),
                    enabled: true,
                })
                .collect();
        }
        let before = serde_json::to_value(&s.doc).unwrap();
        let active = s.active_layer;
        let selected = s.selected_layer;
        assert!(!s.duplicate_layer_tree(target));
        assert_eq!(serde_json::to_value(&s.doc).unwrap(), before);
        assert_eq!(s.active_layer, active);
        assert_eq!(s.selected_layer, selected);
        assert_eq!(s.history.len(), 0);
        assert!(s.history.can_redo());
    }
}
