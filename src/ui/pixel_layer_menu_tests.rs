use crate::app::Studio;
use crate::document::{Document, Layer};
use crate::tools::Persona;
use crate::ui::{chrome::pixel_menu_tests::Seat, welcome::keyboard_tests as keys};

fn fixture() -> Studio {
    let mut studio = Studio::new();
    studio.show_welcome = false;
    studio.persona = Persona::Pixel;
    studio.doc = Document::new("Pixel layer menus", 4., 1., 96.);
    studio.doc.transparent = true;
    let mut layer = Layer::raster("Source", 4, 1).unwrap();
    layer
        .kind
        .pixels_mut()
        .unwrap()
        .replace_bytes(
            [
                10, 40, 90, 255, 80, 150, 220, 200, 240, 80, 70, 128, 60, 50, 40, 0,
            ]
            .to_vec(),
        )
        .unwrap();
    studio.doc.layers = vec![layer];
    studio.active_layer = Some(0);
    studio.history.clear();
    studio
}

fn encoded(studio: &Studio) -> String {
    crate::project::encode(&studio.doc).unwrap()
}

fn undo_reopen(studio: &mut Studio, before: &str, history: usize) {
    let after = encoded(studio);
    assert_ne!(after, before);
    assert_eq!(studio.history.len(), history + 1);
    let reopened = crate::project::decode(&after).unwrap();
    assert_eq!(crate::project::encode(&reopened).unwrap(), after);
    studio.undo();
    assert_eq!(encoded(studio), before);
    studio.redo();
    assert_eq!(encoded(studio), after);
}

fn menu(seat: &mut Seat, studio: &mut Studio, nested: Option<&str>, command: &str) {
    seat.click(studio, "Layer");
    if let Some(nested) = nested {
        seat.click(studio, &format!("{nested} ⏵"));
    }
    seat.invoke(studio, command);
}

#[test]
fn craft_pixel_layer_masks_native_menu_preserves_pixels_history_and_reopen() {
    let mut studio = fixture();
    let source = serde_json::to_value(studio.doc.layers[0].kind.pixels().unwrap()).unwrap();
    let mut seat = Seat::new();
    for (command, value) in [
        ("Reveal all", 255),
        ("Reset to hide all", 0),
        ("Reset to reveal all", 255),
        ("Invert mask", 0),
    ] {
        let before = encoded(&studio);
        let history = studio.history.len();
        menu(&mut seat, &mut studio, Some("Layer mask"), command);
        assert_eq!(
            studio.doc.layers[0].mask.as_ref().unwrap().data.as_slice(),
            [value, value, value, 255].repeat(4)
        );
        assert_eq!(
            serde_json::to_value(studio.doc.layers[0].kind.pixels().unwrap()).unwrap(),
            source
        );
        undo_reopen(&mut studio, &before, history);
    }
    let before = encoded(&studio);
    let history = studio.history.len();
    menu(&mut seat, &mut studio, Some("Layer mask"), "Remove mask");
    assert!(studio.doc.layers[0].mask.is_none());
    assert!(!studio.paint_mask);
    undo_reopen(&mut studio, &before, history);
}

#[test]
fn craft_pixel_layer_mask_selection_and_native_apply_use_existing_sample_path() {
    let mut studio = fixture();
    let samples = [
        1.25, -0.125, 0.5, 0.75, 0.3, 0.4, 0.8, 1., 0.1, 0.2, 0.3, 0.25, 0., 0., 0., 0.,
    ];
    studio.doc.layers[0]
        .kind
        .pixels_mut()
        .unwrap()
        .set_samples(samples.to_vec())
        .unwrap();
    studio.replace_pixel_selection(
        vec![0, 64, 128, 255],
        crate::app::masking::SelectionSpace {
            w: 4,
            h: 1,
            transform: tiny_skia::Transform::identity(),
        },
    );
    let original = serde_json::to_value(studio.doc.layers[0].kind.pixels().unwrap()).unwrap();
    let mut seat = Seat::new();
    let before = encoded(&studio);
    let history = studio.history.len();
    menu(&mut seat, &mut studio, Some("Layer mask"), "From selection");
    assert_eq!(
        serde_json::to_value(studio.doc.layers[0].kind.pixels().unwrap()).unwrap(),
        original
    );
    let mask = studio.doc.layers[0].mask.as_ref().unwrap();
    for (i, amount) in [0., 64. / 255., 128. / 255., 1.].into_iter().enumerate() {
        assert!((mask.sample(i)[0] - amount).abs() < 1e-6);
    }
    undo_reopen(&mut studio, &before, history);
    let before = encoded(&studio);
    let history = studio.history.len();
    menu(
        &mut seat,
        &mut studio,
        Some("Layer mask"),
        "Apply to pixels",
    );
    assert!(studio.doc.layers[0].mask.is_none());
    let actual = studio.doc.layers[0].kind.pixels().unwrap();
    assert!(actual.native.is_some());
    for (i, amount) in [0., 64. / 255., 128. / 255., 1.].into_iter().enumerate() {
        let got = actual.sample(i);
        for channel in 0..3 {
            assert_eq!(got[channel].to_bits(), samples[i * 4 + channel].to_bits());
        }
        assert!((got[3] - samples[i * 4 + 3] * amount).abs() < 1e-6);
    }
    undo_reopen(&mut studio, &before, history);
}

#[test]
fn craft_pixel_layer_arrange_moves_whole_groups_and_keeps_stable_target_history() {
    for (command, target) in [
        ("Bring to front", 3),
        ("Bring forward", 3),
        ("Send backward", 1),
        ("Send to back", 1),
    ] {
        let mut studio = fixture();
        let group = Layer::group("Pair");
        let mut child = Layer::raster("Child", 4, 1).unwrap();
        child.parent = Some(group.id);
        studio
            .doc
            .layers
            .extend([child, group, Layer::vector("Top")]);
        let group_id = studio.doc.layers[2].id;
        let child_id = studio.doc.layers[1].id;
        studio.activate_layer_tree(2);
        let before = encoded(&studio);
        let mut seat = Seat::new();
        menu(&mut seat, &mut studio, Some("Arrange"), command);
        studio.doc.validate_hierarchy().unwrap();
        assert_eq!(studio.doc.layers[target].id, group_id);
        assert_eq!(studio.doc.layers[target - 1].id, child_id);
        assert_eq!(
            studio.active_layer.map(|i| studio.doc.layers[i].id),
            Some(group_id)
        );
        assert_eq!(studio.selected_layer, Some(group_id));
        undo_reopen(&mut studio, &before, 0);
    }
}

#[test]
fn craft_pixel_layer_creation_deletion_and_invalid_owners_follow_real_menu_gates() {
    let mut studio = fixture();
    let mut seat = Seat::new();
    for command in ["New group", "New pixel layer", "Delete layer"] {
        let before = encoded(&studio);
        let history = studio.history.len();
        menu(&mut seat, &mut studio, None, command);
        studio.doc.validate_hierarchy().unwrap();
        undo_reopen(&mut studio, &before, history);
    }
    for case in 0..5 {
        let mut studio = fixture();
        match case {
            0 => studio.active_layer = None,
            1 => studio.doc.layers[0].locked = true,
            2 => {
                let mut group = Layer::group("Locked parent");
                group.locked = true;
                studio.doc.layers[0].parent = Some(group.id);
                studio.doc.layers.push(group);
            }
            3 => studio.doc.layers[0].visible = false,
            _ => studio.updates.freezing = true,
        }
        let before = encoded(&studio);
        let mut seat = Seat::new();
        seat.click(&mut studio, "Layer");
        assert!(keys::named(&seat.output, "Layer mask ⏵").1.is_disabled());
        if case != 3 {
            assert!(keys::named(&seat.output, "Arrange ⏵").1.is_disabled());
            assert!(keys::named(&seat.output, "Delete layer").1.is_disabled());
        }
        assert_eq!(encoded(&studio), before);
        assert!(studio.history.is_empty());
    }
}
