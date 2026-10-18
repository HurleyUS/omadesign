use super::*;
use crate::{deep_color::Depth, document::Pixels};

fn fixture(depth: Depth, cmyk: bool) -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Pixel;
    s.doc = Document::new("Tree rows", 6., 1., 96.);
    s.doc.transparent = true;
    s.doc.color.depth = depth;
    s.doc.color.intent = 0;
    s.doc.color.model = if cmyk {
        crate::deep_color::cms::Model::Cmyk
    } else {
        crate::deep_color::cms::Model::Rgb
    };
    let mut a = Layer::group("A group");
    a.pass_through = false;
    let mut b = Layer::group("B group");
    b.pass_through = false;
    let pixel = |rgb: [f32; 4], inks: [f32; 5]| {
        if cmyk {
            Pixels::from_cmyk(1, 1, depth, inks.to_vec(), vec![], 0).unwrap()
        } else if depth == Depth::U8 {
            Pixels::from_rgba(1, 1, rgb.into_iter().map(crate::deep_color::byte).collect()).unwrap()
        } else {
            Pixels::from_samples(1, 1, depth, rgb.to_vec()).unwrap()
        }
    };
    let mut top = Layer::placed_raster(
        "A pixel",
        pixel([1., 0., 0., 1.], [0.75, 0.25, 0.125, 0.5, 1.]),
        Pt::ZERO,
        Pt::splat(1.),
    );
    top.parent = Some(a.id);
    let mut bottom = Layer::placed_raster(
        "B pixel",
        pixel([0., 1., 0., 1.], [0.125, 0.5, 0.75, 0.25, 1.]),
        Pt::ZERO,
        Pt::splat(1.),
    );
    bottom.parent = Some(b.id);
    let spare = Layer::placed_raster(
        "Spare",
        pixel([0., 0., 1., 1.], [0.5, 0., 0.25, 0., 1.]),
        Pt::new(4., 0.),
        Pt::splat(1.),
    );
    s.layer_expanded.extend([a.id, b.id]);
    s.doc.layers = vec![top, bottom, b, a, spare];
    s.activate_layer_tree(0);
    s.history.clear();
    s
}
fn state(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn color(s: &Studio) -> Vec<f32> {
    let p = crate::deep_color::composite(&s.doc, 1).unwrap();
    if s.doc.color.model == crate::deep_color::cms::Model::Cmyk {
        p.native.as_ref().unwrap().cmyk.as_ref().unwrap()[..5].to_vec()
    } else {
        p.sample(0).to_vec()
    }
}
fn saved(s: &Studio, before: &str, name: &str) {
    let after = state(s);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
        after
    );
    if let Some(root) = std::env::var_os("OMA_LAYER_TREE_QA_OUT") {
        let folder = std::path::PathBuf::from(root).join(name);
        std::fs::create_dir_all(&folder).unwrap();
        crate::project::save_to(
            &crate::project::decode(before).unwrap(),
            &folder.join("before.oma"),
        )
        .unwrap();
        crate::project::save_to(&s.doc, &folder.join("after.oma")).unwrap();
        std::fs::write(folder.join("case.json"),serde_json::to_vec_pretty(&serde_json::json!({"source":std::env::var("OMA_QA_SOURCE_SHA").unwrap(),"test":std::thread::current().name().unwrap(),"name":name})).unwrap()).unwrap();
    }
}

#[test]
fn craft_tree_rows_ranges_toggle_active_anchor_and_collapsed_endpoints_use_the_actual_tree() {
    let mut s = fixture(Depth::U8, false);
    let before = state(&s);
    assert_eq!(s.layer_panel_order(false), [4, 3, 0, 2, 1]);
    s.select_layer_rows(0, false, false);
    s.select_layer_rows(1, true, false);
    assert_eq!(s.selected_layer_rows(), [1, 2, 0]);
    assert_eq!(s.active_layer, Some(1));
    s.select_layer_rows(0, false, false);
    s.select_layer_rows(0, false, true);
    assert_eq!(s.selected_layer_rows(), [0]);
    assert_eq!(s.active_layer, Some(0));
    s.select_layer_rows(1, false, true);
    assert_eq!(s.selected_layer_rows(), [1, 0]);
    assert_eq!(s.active_layer, Some(1));
    assert_eq!(s.current_layer_rows().unwrap().anchor, s.doc.layers[1].id);
    s.select_layer_rows(1, false, true);
    assert_eq!(s.selected_layer_rows(), [0]);
    assert_eq!(s.active_layer, Some(0));
    s.select_layer_rows(1, false, true);
    s.select_layer_rows(3, true, false);
    assert_eq!(s.selected_layer_rows(), [1, 2, 0, 3]);
    s.layer_expanded.clear();
    s.select_layer_rows(3, false, false);
    s.select_layer_rows(2, true, false);
    assert_eq!(s.layer_panel_order(false), [4, 3, 2]);
    assert_eq!(s.selected_layer_rows(), [2, 3]);
    s.select_layer_rows(0, false, false);
    s.select_layer_rows(1, true, false);
    assert_eq!(s.selected_layer_rows(), [1, 2, 0]);
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
}

#[test]
fn craft_tree_rows_arrange_noncontiguous_groups_preserves_all_source_samples_and_one_targeted_history()
 {
    for (depth, cmyk) in [
        (Depth::U8, false),
        (Depth::U16, false),
        (Depth::F32, false),
        (Depth::F32, true),
    ] {
        for operation in ["forward", "front", "backward", "back"] {
            let mut s = fixture(depth, cmyk);
            let source = if operation.starts_with('f') { 2 } else { 3 };
            s.activate_layer_tree(source);
            let target = s.history_target();
            let before = state(&s);
            let original = s.doc.layers.clone();
            let expected = if cmyk {
                vec![0.125, 0.5, 0.75, 0.25, 1.]
            } else {
                vec![0., 1., 0., 1.]
            };
            match operation {
                "forward" => s.move_layer_tree(source, true),
                "front" => s.move_layer_tree_extreme(source, true),
                "backward" => s.move_layer_tree(source, false),
                _ => s.move_layer_tree_extreme(source, false),
            }
            assert_eq!(s.history.len(), 1);
            assert_eq!(color(&s), expected);
            s.doc.validate_hierarchy().unwrap();
            for layer in &s.doc.layers {
                assert_eq!(
                    serde_json::to_value(layer).unwrap(),
                    serde_json::to_value(original.iter().find(|l| l.id == layer.id).unwrap())
                        .unwrap()
                );
            }
            let after = state(&s);
            let final_target = s.history_target();
            saved(
                &s,
                &before,
                &format!("arrange-{depth:?}-{cmyk}-{operation}"),
            );
            s.activate_layer_tree(0);
            s.undo();
            assert_eq!(state(&s), before);
            assert_eq!(s.history_target(), target);
            s.redo();
            assert_eq!(state(&s), after);
            assert_eq!(s.history_target(), final_target);
        }
    }
}

#[test]
fn craft_tree_rows_drop_noncontiguous_subtrees_into_above_and_below_groups() {
    for place in [
        TreePlacement::Inside,
        TreePlacement::Above,
        TreePlacement::Below,
    ] {
        let mut s = fixture(Depth::F32, false);
        let source = if place == TreePlacement::Above { 2 } else { 3 };
        let target = if source == 2 { 3 } else { 2 };
        s.activate_layer_tree(source);
        let before = state(&s);
        let original = s.doc.layers.clone();
        let source_id = original[source].id;
        assert!(s.drop_layer(source, target, place));
        assert_eq!(s.history.len(), 1);
        s.doc.validate_hierarchy().unwrap();
        for layer in &s.doc.layers {
            let mut expected = original.iter().find(|l| l.id == layer.id).unwrap().clone();
            if layer.id == source_id && place == TreePlacement::Inside {
                expected.parent = Some(original[target].id);
            }
            assert_eq!(
                serde_json::to_value(layer).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
        }
        let expected = if place == TreePlacement::Inside {
            vec![1., 0., 0., 1.]
        } else {
            vec![0., 1., 0., 1.]
        };
        assert_eq!(color(&s), expected);
        saved(&s, &before, &format!("drop-{place:?}"));
        let after = state(&s);
        s.undo();
        assert_eq!(state(&s), before);
        s.redo();
        assert_eq!(state(&s), after);
    }
    let mut s = fixture(Depth::U8, false);
    let before = state(&s);
    assert!(!s.reorder_layer_tree(3, 2, true));
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
}

#[test]
fn craft_tree_rows_invalid_cycle_locked_and_excessive_depth_drop_are_atomic() {
    let mut s = fixture(Depth::U8, false);
    let before = state(&s);
    assert!(!s.drop_layer(3, 0, TreePlacement::Inside));
    assert_eq!(state(&s), before);
    s.doc.layers[0].locked = true;
    let locked = state(&s);
    assert!(!s.drop_layer(3, 2, TreePlacement::Inside));
    assert_eq!(state(&s), locked);
    s.doc.layers[0].locked = false;
    s.doc.layers[3].locked = true;
    let locked = state(&s);
    assert!(!s.reorder_layer_tree(3, 2, false));
    assert_eq!(state(&s), locked);
    assert_eq!(s.history.len(), 0);
    let mut layers = Vec::new();
    let mut parent = None;
    for _ in 0..64 {
        let mut group = Layer::group("Depth");
        group.parent = parent;
        parent = Some(group.id);
        layers.push(group);
    }
    let source = Layer::group("Source");
    let mut child = Layer::vector("Child");
    child.parent = Some(source.id);
    layers.extend([source, child]);
    s.doc.layers = layers;
    s.doc.validate_hierarchy().unwrap();
    s.activate_layer_tree(64);
    let before = state(&s);
    assert!(!s.drop_layer(64, 63, TreePlacement::Inside));
    assert!(s.status.contains("deep"));
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
}

#[test]
fn craft_tree_rows_clipping_uses_bottom_to_top_group_order_not_flat_indices() {
    let mut s = fixture(Depth::U8, false);
    let a = s.doc.layers[3].clone();
    let b = s.doc.layers[2].clone();
    let top = s.doc.layers[0].clone();
    let low = s.doc.layers[1].clone();
    let mut base = Layer::raster("A base", 1, 1).unwrap();
    base.parent = Some(a.id);
    s.doc.layers = vec![base, b, top, low, a];
    s.select_layer_rows(2, false, false);
    s.select_layer_rows(3, false, true);
    assert_eq!(s.selected_layer_rows(), [3, 2]);
    let before = state(&s);
    let target = s.history_target();
    assert!(s.set_layer_clipping(None, true));
    assert!(s.doc.layers[2].clipped);
    assert!(!s.doc.layers[3].clipped);
    assert_eq!(s.history.len(), 1);
    let after = state(&s);
    s.activate_layer_tree(0);
    s.undo();
    assert_eq!(state(&s), before);
    assert_eq!(s.history_target(), target);
    s.redo();
    assert_eq!(state(&s), after);
}
