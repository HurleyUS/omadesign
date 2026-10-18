use crate::{
    app::Studio,
    color::{Blend, Rgba},
    deep_color::{Depth, composite},
    document::{Cmd, Document, Layer, Pixels},
    geom::Pt,
    tools::Persona,
};

fn layer(name: &str, values: &[u8]) -> Layer {
    let mut layer = Layer::raster(name, (values.len() / 4) as u32, 1).unwrap();
    layer
        .kind
        .pixels_mut()
        .unwrap()
        .replace_bytes(values.to_vec())
        .unwrap();
    layer
}
fn fixture() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Pixel;
    s.doc = Document::new("Editable clipping", 4., 1., 96.);
    s.doc.workspace = Some(Persona::Pixel);
    s.doc.transparent = true;
    s.doc.layers = vec![
        layer(
            "Base",
            &[0, 0, 255, 0, 0, 0, 255, 64, 0, 0, 255, 128, 0, 0, 255, 255],
        ),
        layer("Red", &[255, 0, 0, 255].repeat(4)),
        layer("Green", &[0, 255, 0, 128].repeat(4)),
    ];
    s.activate_layer_tree(2);
    s.history.clear();
    s
}
fn state(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn rgba(doc: &Document) -> Vec<u8> {
    crate::compositor::render_export(doc, 1)
        .unwrap()
        .data()
        .to_vec()
}
fn record(s: &Studio, before: &str) {
    let Some(root) = std::env::var_os("OMA_LAYER_CLIPPING_QA_OUT") else {
        return;
    };
    let thread = std::thread::current();
    let test = thread.name().unwrap();
    let root = std::path::PathBuf::from(root).join(test.rsplit("::").next().unwrap());
    std::fs::create_dir_all(&root).unwrap();
    crate::project::save_to(
        &crate::project::decode(before).unwrap(),
        &root.join("before.oma"),
    )
    .unwrap();
    crate::project::save_to(&s.doc, &root.join("after.oma")).unwrap();
    std::fs::write(root.join("case.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "source":std::env::var("OMA_QA_SOURCE_SHA").unwrap_or_default(),"test":test,
        "before":"before.oma","after":"after.oma","boundary":"Actual saved files; final test/compiler receipt determines qualification"
    })).unwrap()).unwrap();
}
fn undo_reopen(s: &mut Studio, before: &str) {
    let after = state(s);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
        after
    );
    record(s, before);
    s.undo();
    assert_eq!(state(s), before);
    s.redo();
    assert_eq!(state(s), after);
}

#[test]
fn craft_clipping_selected_layers_keep_live_base_alpha_and_one_history_step() {
    let mut s = fixture();
    s.selected_layer = None;
    s.selection = (0..3).map(|i| (i, crate::document::RASTER_ID)).collect();
    let before = state(&s);
    assert!(s.set_layer_clipping(None, true));
    assert_eq!(
        s.doc.layers.iter().map(|l| l.clipped).collect::<Vec<_>>(),
        [false, true, true]
    );
    assert_eq!(s.history.len(), 1);
    assert_eq!(
        rgba(&s.doc),
        [0, 0, 0, 0, 32, 32, 0, 64, 64, 64, 0, 128, 127, 128, 0, 255]
    );
    assert!(s.doc.layers.iter().all(|l| l.mask.is_none()));
    assert_eq!(s.doc.layers[0].kind.pixels().unwrap().data[11], 128);
    undo_reopen(&mut s, &before);
    let before = state(&s);
    assert!(s.set_layer_clipping(None, false));
    assert!(s.doc.layers.iter().all(|l| !l.clipped));
    assert!(rgba(&s.doc).chunks_exact(4).all(|p| p[3] == 255));
    s.undo();
    assert_eq!(state(&s), before);
}

#[test]
fn craft_clipping_base_edits_hide_reorder_and_hit_testing_stay_live() {
    let mut s = fixture();
    s.doc.layers[1].clipped = true;
    s.doc.layers[2].clipped = true;
    assert_eq!(s.doc.hit_test(Pt::new(0.5, 0.5), 0.), None);
    assert_eq!(
        s.doc.hit_test(Pt::new(2.5, 0.5), 0.),
        Some((2, crate::document::RASTER_ID))
    );
    let mut pixels = s.doc.layers[0].kind.pixels().unwrap().clone();
    let original = pixels.clone();
    pixels.bytes_mut().unwrap()[3] = 192;
    s.commit(Cmd::NativePixels {
        layer: 0,
        mask: false,
        before: original,
        after: pixels,
    });
    assert_eq!(rgba(&s.doc)[3], 192);
    s.undo();
    assert_eq!(rgba(&s.doc)[3], 0);
    s.doc.layers[0].visible = false;
    assert!(rgba(&s.doc).iter().all(|v| *v == 0));
    assert!(!s.doc.layer_visible(2));
    s.doc.layers[0].visible = true;
    let other = layer("Other base", &[80, 90, 100, 255].repeat(4));
    let other_id = other.id;
    s.doc.layers.insert(2, other);
    let before = state(&s);
    s.history.clear();
    assert_eq!(s.doc.clipping_base(3), Some(2));
    s.move_layer_tree(3, false);
    let clip = s.doc.layers.iter().position(|l| l.name == "Green").unwrap();
    assert_ne!(
        s.doc.layers[s.doc.clipping_base(clip).unwrap()].id,
        other_id
    );
    assert_eq!(s.history.len(), 1);
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_clipping_float_hdr_negative_profile_masks_and_unchanged_sources_reopen() {
    let mut s = fixture();
    s.doc.width = 1.;
    s.doc.color.depth = Depth::F32;
    let profile = include_bytes!("formats/psd/fixtures/srgb-cc0.icc").to_vec();
    s.doc.color.icc_profile = profile.clone();
    let mut base =
        Pixels::from_samples(1, 1, Depth::F32, vec![1.25, -0.125, 0.5, 0.50001]).unwrap();
    base.assign_rgb_profile(profile.clone(), 0).unwrap();
    let mut top = Pixels::from_samples(1, 1, Depth::F32, vec![2., -0.5, 0.25, 0.70001]).unwrap();
    top.assign_rgb_profile(profile, 0).unwrap();
    let mut lower = Layer::placed_raster("HDR base", base, Pt::ZERO, Pt::splat(1.));
    lower.opacity = 0.75;
    lower.fill_opacity = 0.5;
    lower.mask = Some(
        Pixels::from_samples(1, 1, Depth::F32, vec![0.333333, 0.333333, 0.333333, 1.]).unwrap(),
    );
    let mut upper = Layer::placed_raster("Signed source", top, Pt::ZERO, Pt::splat(1.));
    upper.mask = Some(Pixels::from_samples(1, 1, Depth::F32, vec![0.5, 0.5, 0.5, 1.]).unwrap());
    s.doc.layers = vec![lower, upper];
    s.activate_layer_tree(1);
    let before = state(&s);
    let id = s.doc.layers[1].id;
    let original = s.doc.layers[1]
        .kind
        .pixels()
        .unwrap()
        .native
        .as_ref()
        .unwrap()
        .samples
        .clone();
    assert!(s.set_layer_clipping(Some(id), true));
    let pixels = composite(&s.doc, 1).unwrap();
    let actual = pixels.sample(0);
    let amount = 0.70001f32 * 0.5;
    let expected = [
        1.25 * (1. - amount) + 2. * amount,
        -0.125 * (1. - amount) - 0.5 * amount,
        0.5 * (1. - amount) + 0.25 * amount,
        0.50001 * 0.333333 * 0.75 * 0.5,
    ];
    for c in 0..4 {
        assert!(
            (actual[c] - expected[c]).abs() < 2e-7,
            "{actual:?} != {expected:?}"
        );
    }
    assert!(actual[0] > 1. && actual[1] < 0.);
    assert_eq!(
        pixels.native.as_ref().unwrap().icc_profile.as_slice(),
        s.doc.color.icc_profile
    );
    assert_eq!(
        s.doc.layers[1]
            .kind
            .pixels()
            .unwrap()
            .native
            .as_ref()
            .unwrap()
            .samples
            .as_slice(),
        original.as_slice()
    );
    assert_eq!(s.history.len(), 1);
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_clipping_cmyka_inks_alpha_and_intent_use_the_same_float_stack() {
    let mut s = fixture();
    s.doc.width = 1.;
    s.doc.color.depth = Depth::F32;
    s.doc.color.model = crate::deep_color::cms::Model::Cmyk;
    s.doc.color.intent = 0;
    let base = Pixels::from_cmyk(
        1,
        1,
        Depth::F32,
        vec![0.125, 0.25, 0.375, 0.5, 0.5],
        vec![],
        0,
    )
    .unwrap();
    let top = Pixels::from_cmyk(
        1,
        1,
        Depth::F32,
        vec![0.75, 0.625, 0.5, 0.375, 0.75],
        vec![],
        0,
    )
    .unwrap();
    s.doc.layers = vec![
        Layer::placed_raster("Inks base", base, Pt::ZERO, Pt::splat(1.)),
        Layer::placed_raster("Inks clip", top, Pt::ZERO, Pt::splat(1.)),
    ];
    s.activate_layer_tree(1);
    let before = state(&s);
    let id = s.doc.layers[1].id;
    assert!(s.set_layer_clipping(Some(id), true));
    let result = composite(&s.doc, 1).unwrap();
    let native = result.native.as_ref().unwrap();
    let expected = [
        0.125 * 0.25 + 0.75 * 0.75,
        0.25 * 0.25 + 0.625 * 0.75,
        0.375 * 0.25 + 0.5 * 0.75,
        0.5 * 0.25 + 0.375 * 0.75,
        0.5,
    ];
    for (&a, b) in native.cmyk.as_ref().unwrap().iter().zip(expected) {
        assert!((a - b).abs() < 2e-7);
    }
    assert_eq!(native.intent, 0);
    assert_eq!(s.history.len(), 1);
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_clipping_groups_masks_opacity_and_outer_shadow_keep_their_order() {
    let mut s = fixture();
    s.doc.width = 5.;
    s.doc.height = 4.;
    let mut base = Layer::group("Styled base");
    base.opacity = 0.5;
    base.filters.items.push(crate::filter::Fx::Shadow {
        dx: 1.,
        dy: 0.,
        blur: 0.,
        spread: 0.,
        color: Rgba::rgb(0, 0, 0),
        knockout: false,
        blend: Blend::Normal,
        opacity: 1.,
    });
    let mut child = layer("Child", &[0, 0, 255, 255].repeat(2));
    child
        .kind
        .set_raster_xform(Pt::new(1., 1.), Pt::new(2., 1.), 0.);
    child.parent = Some(base.id);
    let mut top = layer("Clipped red", &[255, 0, 0, 255].repeat(5));
    top.kind.set_raster_xform(Pt::ZERO, Pt::new(5., 4.), 0.);
    s.doc.layers = vec![child, base, top];
    s.activate_layer_tree(2);
    let before = state(&s);
    let id = s.doc.layers[2].id;
    assert!(s.set_layer_clipping(Some(id), true));
    let p = rgba(&s.doc);
    let pixel = |x: usize, y: usize| &p[(y * 5 + x) * 4..(y * 5 + x + 1) * 4];
    assert_eq!(pixel(1, 1), [128, 0, 0, 128]);
    assert_eq!(pixel(3, 1), [0, 0, 0, 128]);
    assert_eq!(pixel(4, 1), [0, 0, 0, 0]);
    assert_eq!(s.history.len(), 1);
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_clipping_passthrough_group_keeps_backdrop_blends_and_native_signed_delta() {
    let mut s = fixture();
    s.doc.width = 1.;
    s.doc.height = 1.;
    s.doc.color.depth = Depth::F32;
    let raster = |name: &str, color: Vec<f32>| {
        Layer::placed_raster(
            name,
            Pixels::from_samples(1, 1, Depth::F32, color).unwrap(),
            Pt::ZERO,
            Pt::splat(1.),
        )
    };
    let backdrop = raster("Backdrop", vec![0.2, 0.4, 0.6, 1.]);
    let group = Layer::group("Pass through");
    let mut child = raster("Multiply child", vec![0.8, 0.6, 0.2, 1.]);
    child.parent = Some(group.id);
    child.blend = Blend::Multiply;
    let mut clip = raster("Clipped red", vec![1., 0., 0., 0.5]);
    clip.clipped = true;
    s.doc.layers = vec![backdrop, child, group, clip];
    let actual = composite(&s.doc, 1).unwrap().sample(0);
    for (a, b) in actual.into_iter().zip([0.26, -0.06, 0.02, 1.]) {
        assert!((a - b).abs() < 2e-7, "{actual:?}");
    }
    s.doc.color.depth = Depth::U8;
    for (i, bytes) in [
        (0, vec![51, 102, 153, 255]),
        (1, vec![204, 153, 51, 255]),
        (3, vec![255, 0, 0, 128]),
    ] {
        *s.doc.layers[i].kind.pixels_mut().unwrap() = Pixels::from_rgba(1, 1, bytes).unwrap();
    }
    assert_eq!(rgba(&s.doc), [67, 0, 5, 255]);
}

#[test]
fn craft_clipping_photoshop_psd_psb_retain_editable_flags_and_native_samples() {
    let mut s = fixture();
    s.doc.color.depth = Depth::F32;
    s.doc.layers[1].clipped = true;
    s.doc.layers[2].clipped = true;
    let before = state(&s);
    for large in [false, true] {
        let bytes = crate::formats::psd::encode_admitted(&s.doc, large).unwrap();
        let opened =
            crate::formats::psd::decode(bytes.bytes.as_slice(), "Editable clipping").unwrap();
        assert_eq!(
            opened
                .document
                .layers
                .iter()
                .map(|l| l.clipped)
                .collect::<Vec<_>>(),
            [false, true, true]
        );
        assert!(opened.document.layers.iter().all(|l| l.mask.is_none()));
        assert!(
            !opened
                .warnings
                .iter()
                .any(|n| n.contains("converted to pixel masks"))
        );
        let original = composite(&s.doc, 1).unwrap();
        let imported = composite(&opened.document, 1).unwrap();
        for i in 0..4 {
            for c in 0..4 {
                assert!((original.sample(i)[c] - imported.sample(i)[c]).abs() < 2e-7);
            }
        }
        if let Some(root) = std::env::var_os("OMA_LAYER_CLIPPING_QA_OUT") {
            let path = std::path::PathBuf::from(root).join("photoshop");
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(
                path.join(if large {
                    "clipping.psb"
                } else {
                    "clipping.psd"
                }),
                bytes.bytes.as_slice(),
            )
            .unwrap();
            crate::project::save_to(&s.doc, &path.join("source.oma")).unwrap();
        }
    }
    assert_eq!(state(&s), before);
}

#[test]
fn craft_clipping_merge_down_bakes_base_coverage_group_merge_retains_binding() {
    let mut s = fixture();
    s.doc.layers.truncate(2);
    s.doc.layers[1].clipped = true;
    let before = state(&s);
    let expected = rgba(&s.doc);
    let id = s.doc.layers[1].id;
    let prepared = s
        .capture_layer_merge(crate::app::layer_merge::Kind::Down(id))
        .unwrap()
        .render(&crate::ml::NoProgress)
        .unwrap();
    s.apply_layer_merge(prepared).unwrap();
    assert_eq!(s.doc.layers.len(), 1);
    assert!(!s.doc.layers[0].clipped);
    assert_eq!(rgba(&s.doc), expected);
    undo_reopen(&mut s, &before);
    let mut s = fixture();
    s.doc.layers.truncate(2);
    let mut group = Layer::group("Clipped group");
    group.clipped = true;
    s.doc.layers[1].parent = Some(group.id);
    let id = group.id;
    s.doc.layers.push(group);
    s.activate_layer_tree(2);
    let before = state(&s);
    let expected = rgba(&s.doc);
    let prepared = s
        .capture_layer_merge(crate::app::layer_merge::Kind::Group(id))
        .unwrap()
        .render(&crate::ml::NoProgress)
        .unwrap();
    s.apply_layer_merge(prepared).unwrap();
    assert!(s.doc.layers[1].clipped);
    assert!(!s.doc.layers[1].is_group);
    assert_eq!(rgba(&s.doc), expected);
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_clipping_locked_busy_and_dependency_cycles_reject_without_mutation() {
    let mut s = fixture();
    let owner = s.doc.layers[2].id;
    s.doc.layers[2].locked = true;
    let before = state(&s);
    assert!(!s.set_layer_clipping(Some(owner), true));
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
    s.doc.layers[2].locked = false;
    let mut mask = crate::artwork_mask::ArtworkMask::new(crate::artwork_mask::MaskSource {
        layer: owner,
        shape: None,
    });
    mask.mode = crate::artwork_mask::MaskMode::Alpha;
    s.doc.layers[1].artwork_masks.push(mask);
    let before = state(&s);
    assert!(!s.set_layer_clipping(Some(owner), true));
    assert!(s.status.contains("cycle"));
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
}

#[test]
fn craft_clipping_native_cancel_keeps_all_sources_and_releases_scratch() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut s = fixture();
    s.doc.width = 256.;
    s.doc.height = 128.;
    s.doc.color.depth = Depth::F32;
    let p = Pixels::from_samples(256, 128, Depth::F32, vec![0.5; 256 * 128 * 4]).unwrap();
    s.doc.layers = vec![
        Layer::placed_raster("Large base", p.clone(), Pt::ZERO, Pt::new(256., 128.)),
        Layer::placed_raster("Large clip", p, Pt::ZERO, Pt::new(256., 128.)),
    ];
    s.doc.layers[1].clipped = true;
    let before = state(&s);
    let used = crate::resources::global().usage().0;
    let calls = AtomicUsize::new(0);
    let cancel = || {
        if calls.fetch_add(1, Ordering::SeqCst) > 16 {
            Err("Cancelled".into())
        } else {
            Ok(())
        }
    };
    let error = crate::deep_color::composite_with_cancel(&s.doc, 1, &cancel)
        .err()
        .unwrap();
    assert!(error.contains("Cancelled"));
    assert!(calls.load(Ordering::SeqCst) > 16);
    assert_eq!(crate::resources::global().usage().0, used);
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
}

#[test]
fn craft_clipping_exchange_refusals_are_named_and_leave_native_artwork_unchanged() {
    let mut s = fixture();
    s.doc.layers[1].clipped = true;
    let before = state(&s);
    for result in [
        crate::svg::export(&s.doc).map(|_| ()),
        crate::formats::pdf::write(&s.doc).map(|_| ()),
        crate::formats::idml::write(&s.doc).map(|_| ()),
        crate::formats::openraster::write(&s.doc).map(|_| ()),
        crate::motion::export_lottie(&s.doc).map(|_| ()),
    ] {
        assert!(result.unwrap_err().contains("LayerClippingUnsupported"));
    }
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
    s.doc.layers[1].clipped = false;
    crate::svg::export(&s.doc).unwrap();
    crate::formats::pdf::write(&s.doc).unwrap();
    crate::formats::idml::write(&s.doc).unwrap();
    crate::formats::openraster::write(&s.doc).unwrap();
}
