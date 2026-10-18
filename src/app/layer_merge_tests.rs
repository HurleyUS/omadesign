use super::*;
use crate::deep_color::Depth;
use crate::document::Pixels;

fn fixture() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Pixel;
    s.doc = Document::new("Merge", 16., 12., 96.);
    s.doc.transparent = true;
    let mut bottom = Layer::raster("Lower", 4, 4).unwrap();
    bottom
        .kind
        .pixels_mut()
        .unwrap()
        .replace_bytes([30, 80, 140, 255].repeat(16))
        .unwrap();
    bottom
        .kind
        .set_raster_xform(Pt::new(-2., 1.), Pt::new(8., 8.), 0.);
    let mut top = Layer::raster("Upper", 2, 2).unwrap();
    top.kind
        .pixels_mut()
        .unwrap()
        .replace_bytes([210, 60, 40, 128].repeat(4))
        .unwrap();
    top.kind
        .set_raster_xform(Pt::new(0., -2.), Pt::new(4., 4.), 0.);
    s.doc.layers = vec![bottom, top];
    s.active_layer = Some(1);
    s.history.clear();
    s
}
fn state(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn picture(doc: &Document) -> Vec<u8> {
    crate::compositor::render_export(doc, 1)
        .unwrap()
        .data()
        .to_vec()
}
fn view(doc: &Document) -> Vec<u8> {
    crate::compositor::render_view(
        doc,
        crate::compositor::View {
            scale: 1.,
            offset: Pt::new(12., 12.),
        },
        64,
        48,
        crate::compositor::Draft::none(),
    )
    .unwrap()
    .data()
    .to_vec()
}
fn run(s: &mut Studio, kind: Kind) {
    let prepared = s
        .capture_layer_merge(kind)
        .unwrap()
        .render(&crate::ml::NoProgress)
        .unwrap();
    s.apply_layer_merge(prepared).unwrap();
}
fn undo_reopen(s: &mut Studio, before: &str) {
    assert_eq!(s.history.len(), 1);
    let after = state(s);
    let reopened = crate::project::decode(&after).unwrap();
    assert_eq!(crate::project::encode(&reopened).unwrap(), after);
    if let Some(root) = std::env::var_os("OMA_LAYER_MERGE_QA_OUT") {
        let thread = std::thread::current();
        let test = thread.name().unwrap();
        let mut name = test.rsplit("::").next().unwrap().to_owned();
        if name == "craft_stamp_selected_native_hdr_cmyka_clipping_keeps_all_original_samples" {
            name.push_str(
                if s.doc.color.model == crate::deep_color::cms::Model::Cmyk {
                    "-cmyka"
                } else {
                    "-rgb"
                },
            );
        }
        if name.starts_with("craft_content_bounds_") {
            name.push_str(
                &format!(
                    "-{:?}-{:?}-{}-{}",
                    s.doc.color.model, s.doc.color.depth, s.doc.name, s.status
                )
                .replace(' ', "-")
                .to_lowercase(),
            );
        }
        let folder = std::path::PathBuf::from(root).join(name);
        std::fs::create_dir_all(&folder).unwrap();
        let original = crate::project::decode(before).unwrap();
        crate::project::save_to(&original, &folder.join("before.oma")).unwrap();
        crate::project::save_to(&s.doc, &folder.join("after.oma")).unwrap();
        std::fs::write(folder.join("case.json"),serde_json::to_vec_pretty(&serde_json::json!({"source":std::env::var("OMA_QA_SOURCE_SHA").unwrap_or_default(),"test":test,"before":"before.oma","after":"after.oma","boundary":"Actual saved files after render assertions; terminal compiler/test receipt determines final result"})).unwrap()).unwrap();
    }
    s.undo();
    assert_eq!(state(s), before);
    s.redo();
    assert_eq!(state(s), after);
}

#[test]
fn craft_merge_down_preserves_off_canvas_pixels_parent_sibling_placement_and_one_undo() {
    let mut s = fixture();
    let group = Layer::group("Parent");
    for layer in &mut s.doc.layers {
        layer.parent = Some(group.id);
    }
    s.doc.layers.push(group);
    s.doc.layers.push(Layer::vector("Unrelated"));
    let lower = s.doc.layers[0].id;
    let upper = s.doc.layers[1].id;
    let parent = s.doc.layers[2].id;
    let untouched = serde_json::to_value(&s.doc.layers[3]).unwrap();
    let before = state(&s);
    let pixels = picture(&s.doc);
    let expanded = view(&s.doc);
    run(&mut s, Kind::Down(upper));
    assert_eq!(s.doc.layers.len(), 3);
    assert_eq!(s.doc.layers[0].id, lower);
    assert_eq!(s.doc.layers[0].parent, Some(parent));
    assert!(!s.doc.layers[0].is_group);
    assert_eq!(s.doc.layers[0].name, "Lower");
    let (origin, size, _) = s.doc.layers[0].kind.raster_xform().unwrap();
    assert!(
        origin.x <= -2. && origin.y <= -2. && origin.x + size.x >= 6. && origin.y + size.y >= 9.
    );
    assert_eq!(serde_json::to_value(&s.doc.layers[2]).unwrap(), untouched);
    assert_eq!(picture(&s.doc), pixels);
    assert_eq!(view(&s.doc), expanded);
    assert_eq!(s.active_layer, Some(0));
    assert_eq!(s.selected_layer, Some(lower));
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_merge_group_applies_masks_effects_and_retains_group_opacity_blend_name_and_paint() {
    let mut s = fixture();
    let mut group = Layer::group("Retouched group");
    group.opacity = 0.5;
    for layer in &mut s.doc.layers {
        layer.parent = Some(group.id);
    }
    group.mask = Some(Pixels::new(2, 2).unwrap());
    group
        .mask
        .as_mut()
        .unwrap()
        .replace_bytes([128, 128, 128, 255].repeat(4))
        .unwrap();
    group.mask_origin = Pt::new(-4., -4.);
    group.mask_size = Pt::new(20., 20.);
    group.filters.items.push(crate::filter::Fx::Shadow {
        dx: 2.,
        dy: 1.,
        blur: 0.,
        spread: 0.,
        color: Rgba::rgb(30, 20, 10),
        knockout: false,
        blend: crate::color::Blend::Normal,
        opacity: 1.,
    });
    let id = group.id;
    s.doc.layers.push(group);
    s.active_layer = Some(2);
    let before = state(&s);
    let painted = picture(&s.doc);
    let off_canvas = view(&s.doc);
    run(&mut s, Kind::Group(id));
    assert_eq!(s.doc.layers.len(), 1);
    let layer = &s.doc.layers[0];
    assert_eq!(layer.id, id);
    assert_eq!(layer.name, "Retouched group");
    assert_eq!(layer.opacity, 0.5);
    assert!(
        layer.mask.is_none() && layer.filters.is_empty() && !layer.is_group && !layer.pass_through
    );
    assert_eq!(picture(&s.doc), painted);
    assert_eq!(view(&s.doc), off_canvas);
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_merge_visible_keeps_hidden_root_content_and_flattens_only_current_visible_layers() {
    let mut s = fixture();
    let mut hidden = Layer::raster("Hidden original", 1, 1).unwrap();
    hidden.visible = false;
    hidden
        .kind
        .pixels_mut()
        .unwrap()
        .replace_bytes(vec![7, 14, 21, 255])
        .unwrap();
    s.doc.layers.insert(1, hidden);
    let hidden = serde_json::to_value(&s.doc.layers[1]).unwrap();
    let before = state(&s);
    let painted = picture(&s.doc);
    run(&mut s, Kind::Visible);
    assert_eq!(s.doc.layers.len(), 2);
    assert_eq!(serde_json::to_value(&s.doc.layers[1]).unwrap(), hidden);
    assert_eq!(picture(&s.doc), painted);
    assert!(s.doc.transparent);
    assert_eq!(s.doc.layers[0].kind.raster_xform().unwrap().0, Pt::ZERO);
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_flatten_bakes_white_background_current_animation_and_embedded_composition_then_reopens() {
    let mut s = fixture();
    let id = s.doc.layers[1].id;
    s.doc.motion.set_key(id, Prop::X, 0., 0., Ease::Linear);
    s.doc.motion.set_key(id, Prop::X, 1., 4., Ease::Linear);
    s.playhead = 0.5;
    let mut child = Document::new("Child", 16., 12., 96.);
    child.transparent = true;
    let mut shape = crate::layout::make_frame(Pt::new(10., 7.), Pt::new(3., 3.));
    shape.style.fill = Fill::Solid(Rgba::rgb(20, 190, 60));
    child.layers[0] = Layer::vector("Child vector");
    child.layers[0].kind.shapes_mut().unwrap().push(shape);
    s.doc
        .motion
        .compositions
        .push(crate::motion::compositions::Composition::new(child));
    let mut hidden = Layer::vector("Hidden");
    hidden.visible = false;
    s.doc.layers.push(hidden);
    let before = state(&s);
    let expected = crate::compositor::render_export_at(&s.doc, 1., s.playhead, false).unwrap();
    run(&mut s, Kind::Flatten);
    assert_eq!(s.doc.layers.len(), 1);
    let layer = &s.doc.layers[0];
    assert_eq!(layer.name, "Background");
    assert!(layer.locked);
    assert!(!s.doc.transparent);
    assert!(
        layer
            .kind
            .pixels()
            .unwrap()
            .data
            .chunks_exact(4)
            .all(|p| p[3] == 255)
    );
    assert!(
        s.doc.motion.tracks.is_empty()
            && s.doc.motion.compositions.is_empty()
            && s.doc.motion.scene3d.is_none()
    );
    assert_eq!(picture(&s.doc), expected.data());
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_merge_native_hdr_negative_samples_profile_and_alpha_use_float_source_not_preview() {
    let mut s = fixture();
    s.doc.width = 3.;
    s.doc.height = 2.;
    s.doc.color.depth = Depth::F32;
    let mut lower = Pixels::from_samples(
        2,
        1,
        Depth::F32,
        vec![1.25, -0.125, 0.5, 1., 0.125, 2.25, 0.5, 1.],
    )
    .unwrap();
    lower
        .assign_rgb_profile(
            include_bytes!("../formats/psd/fixtures/srgb-cc0.icc").to_vec(),
            1,
        )
        .unwrap();
    s.doc.color.icc_profile = include_bytes!("../formats/psd/fixtures/srgb-cc0.icc").to_vec();
    let mut upper = Pixels::from_samples(1, 1, Depth::F32, vec![0.25, 0.75, 2., 0.5]).unwrap();
    upper
        .assign_rgb_profile(s.doc.color.icc_profile.clone(), 1)
        .unwrap();
    s.doc.layers = vec![
        Layer::placed_raster("Native lower", lower, Pt::new(-1., 0.), Pt::new(2., 1.)),
        Layer::placed_raster("Native upper", upper, Pt::ZERO, Pt::new(1., 1.)),
    ];
    let before = state(&s);
    let id = s.doc.layers[1].id;
    run(&mut s, Kind::Down(id));
    let layer = &s.doc.layers[0];
    let pixels = layer.kind.pixels().unwrap();
    let native = pixels.native.as_ref().unwrap();
    assert_eq!(native.depth, Depth::F32);
    assert_eq!(&native.icc_profile[..], &s.doc.color.icc_profile);
    let origin = layer.kind.raster_xform().unwrap().0;
    let sample = |x: i32, y: i32| {
        pixels.sample(
            ((y - origin.y as i32) as u32 * pixels.w + (x - origin.x as i32) as u32) as usize,
        )
    };
    assert_eq!(
        sample(-1, 0).map(f32::to_bits),
        [1.25f32, -0.125, 0.5, 1.].map(f32::to_bits)
    );
    assert_eq!(sample(0, 0), [0.1875, 1.5, 1.25, 1.]);
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_merge_cmyka_inks_stay_authoritative_through_native_merge_and_project_reopen() {
    let mut s = fixture();
    s.doc.width = 1.;
    s.doc.height = 1.;
    s.doc.color.model = crate::deep_color::cms::Model::Cmyk;
    s.doc.color.depth = Depth::F32;
    s.doc.color.intent = 0;
    let bottom = Pixels::from_cmyk(
        1,
        1,
        Depth::F32,
        vec![0.125, 0.25, 0.5, 0.75, 1.],
        vec![],
        0,
    )
    .unwrap();
    let top = Pixels::from_cmyk(
        1,
        1,
        Depth::F32,
        vec![0.375, 0.5, 0.75, 0.25, 0.5],
        vec![],
        0,
    )
    .unwrap();
    s.doc.layers = vec![
        Layer::placed_raster("Inks", bottom, Pt::ZERO, Pt::splat(1.)),
        Layer::placed_raster("Top", top, Pt::ZERO, Pt::splat(1.)),
    ];
    let before = state(&s);
    let id = s.doc.layers[1].id;
    run(&mut s, Kind::Down(id));
    let layer = &s.doc.layers[0];
    let native = layer.kind.pixels().unwrap().native.as_ref().unwrap();
    let origin = layer.kind.raster_xform().unwrap().0;
    let index =
        ((-origin.y as usize) * layer.kind.pixels().unwrap().w as usize + (-origin.x as usize)) * 5;
    let inks = &native.cmyk.as_ref().unwrap()[index..index + 5];
    assert_eq!(inks, &[0.25, 0.375, 0.625, 0.5, 1.]);
    assert_eq!(native.intent, 0);
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_merge_baked_external_mask_source_stays_nonpainting_and_undo_restores_its_live_role() {
    use crate::artwork_mask::{ArtworkMask, MaskSource};
    let mut s = fixture();
    let group = Layer::group("Masked");
    for layer in &mut s.doc.layers {
        layer.parent = Some(group.id);
    }
    let id = group.id;
    s.doc.layers.push(group);
    let mut mask = Layer::raster("Editable mask source", 1, 1).unwrap();
    mask.kind
        .pixels_mut()
        .unwrap()
        .replace_bytes(vec![255, 255, 255, 128])
        .unwrap();
    mask.kind
        .set_raster_xform(Pt::new(-4., -4.), Pt::new(24., 20.), 0.);
    let source = mask.id;
    s.doc.layers.push(mask);
    s.doc.layers[2]
        .artwork_masks
        .push(ArtworkMask::new(MaskSource {
            layer: source,
            shape: None,
        }));
    let before = state(&s);
    let expected = picture(&s.doc);
    run(&mut s, Kind::Group(id));
    assert_eq!(s.doc.layers.len(), 2);
    assert!(!s.doc.layers[1].visible);
    assert_eq!(s.doc.layers[1].id, source);
    assert_eq!(
        s.doc.layers[1].kind.pixels().unwrap().data.as_slice(),
        &[255, 255, 255, 128]
    );
    assert_eq!(picture(&s.doc), expected);
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_merge_svg_graph_pattern_and_editable_text_match_current_pixels_and_native_history() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="50"><defs><pattern id="p" width="6" height="6" patternUnits="userSpaceOnUse"><rect width="3" height="6" fill="#d24"/><rect x="3" width="3" height="6" fill="#28b"/></pattern><filter id="f" x="-20%" y="-20%" width="160%" height="160%"><feGaussianBlur stdDeviation="0.35"/></filter></defs><g filter="url(#f)"><rect x="5" y="4" width="55" height="18" fill="url(#p)"/></g><text x="6" y="43" font-family="DejaVu Sans" font-size="18">Native Ω</text></svg>"##;
    let (mut doc, _) = crate::formats::svg::read(svg, "Graph merge").unwrap();
    doc.transparent = true;
    let mut group = Layer::group("Imported design");
    group.pass_through = false;
    let id = group.id;
    for layer in &mut doc.layers {
        if layer.parent.is_none() {
            layer.parent = Some(id);
        }
    }
    doc.layers.push(group);
    let mut s = fixture();
    s.doc = doc;
    let before = state(&s);
    let expected = picture(&s.doc);
    let viewport = view(&s.doc);
    run(&mut s, Kind::Group(id));
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(picture(&s.doc), expected);
    assert_eq!(view(&s.doc), viewport);
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_merge_external_channel_and_auto_layout_mask_dependencies_refuse_before_mutation() {
    use crate::artwork_mask::{ArtworkMask, MaskSource};
    let mut s = fixture();
    let lower = s.doc.layers[0].id;
    let upper = s.doc.layers[1].id;
    let other = Layer::raster("Channel consumer", 1, 1).unwrap();
    let other_id = other.id;
    s.doc.layers.push(other);
    s.doc.motion.set_key(lower, Prop::X, 0., 0., Ease::Linear);
    s.doc
        .motion
        .set_expression(other_id, Prop::Y, Some(format!("property({lower}, X)+1")))
        .unwrap();
    let before = state(&s);
    let error = s
        .capture_layer_merge(Kind::Down(upper))
        .unwrap()
        .render(&crate::ml::NoProgress)
        .err()
        .unwrap();
    assert!(error.contains("unresolved animation channel"), "{error}");
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
    let mut s = fixture();
    let group = Layer::group("Masked");
    let group_id = group.id;
    for layer in &mut s.doc.layers {
        layer.parent = Some(group_id);
    }
    s.doc.layers.push(group);
    let mut layout = crate::layout::make_frame(Pt::ZERO, Pt::new(8., 8.));
    layout.layout.stack = Some(Default::default());
    let mut mask = crate::layout::make_frame(Pt::ZERO, Pt::new(2., 2.));
    mask.layout.parent = Some(layout.id);
    let id = mask.id;
    let mut source = Layer::vector("Auto layout source");
    let source_id = source.id;
    source.kind.shapes_mut().unwrap().extend([layout, mask]);
    s.doc.layers.push(source);
    s.doc.layers[2]
        .artwork_masks
        .push(ArtworkMask::new(MaskSource {
            layer: source_id,
            shape: Some(id),
        }));
    let before = state(&s);
    let error = s
        .capture_layer_merge(Kind::Group(group_id))
        .unwrap()
        .render(&crate::ml::NoProgress)
        .err()
        .unwrap();
    assert!(error.contains("auto layout"), "{error}");
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
}

#[test]
fn craft_merge_cancel_refusal_and_stale_result_preserve_original_and_redo_without_resource_leaks() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Stop {
        calls: AtomicUsize,
    }
    impl Progress for Stop {
        fn report(&self, _: &str, _: usize, _: usize) {}
        fn cancelled(&self) -> bool {
            self.calls.fetch_add(1, Ordering::SeqCst) > 20
        }
    }
    let mut s = fixture();
    s.doc.color.depth = Depth::F32;
    let pixels = Pixels::from_samples(256, 256, Depth::F32, vec![0.25; 256 * 256 * 4]).unwrap();
    s.doc.layers[0] = Layer::placed_raster("Large", pixels, Pt::ZERO, Pt::splat(256.));
    let id = s.doc.layers[1].id;
    let before = state(&s);
    let used = crate::resources::global().usage().0;
    let stop = Stop {
        calls: AtomicUsize::new(0),
    };
    let request = s.capture_layer_merge(Kind::Down(id)).unwrap();
    assert!(request.render(&stop).err().unwrap().contains("Cancelled"));
    assert!(stop.calls.load(Ordering::SeqCst) > 20);
    assert_eq!(crate::resources::global().usage().0, used);
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
    s.doc.layers[0]
        .filters
        .items
        .push(crate::filter::Fx::Blur { std: 1. });
    let before = state(&s);
    assert!(
        s.capture_layer_merge(Kind::Down(id))
            .unwrap()
            .render(&crate::ml::NoProgress)
            .is_err()
    );
    assert_eq!(state(&s), before);
    assert_eq!(crate::resources::global().usage().0, used);
    let mut s = fixture();
    let id = s.doc.layers[1].id;
    let before = state(&s);
    let prepared = s
        .capture_layer_merge(Kind::Down(id))
        .unwrap()
        .render(&crate::ml::NoProgress)
        .unwrap();
    s.mark();
    assert!(s.apply_layer_merge(prepared).is_err());
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
    assert!(
        s.capture_layer_merge(Kind::Down(s.doc.layers[0].id))
            .is_err()
    );
    s.doc.layers[0].locked = true;
    assert!(!s.can_layer_merge(Kind::Down(id)));
}

#[test]
fn craft_selected_merge_keeps_unselected_rows_top_identity_and_one_undo() {
    let mut s = fixture();
    let mut middle = Layer::placed_raster(
        "Unselected",
        Pixels::from_rgba(1, 1, vec![0, 255, 0, 255]).unwrap(),
        Pt::new(12., 8.),
        Pt::splat(1.),
    );
    middle.locked = true;
    let original_middle = serde_json::to_value(&middle).unwrap();
    s.doc.layers.insert(1, middle);
    s.select_layer_rows(0, false, false);
    s.select_layer_rows(2, false, true);
    assert_eq!(s.pixel_merge_shortcut(false), Some(Kind::Selected));
    let before = state(&s);
    let expected = picture(&s.doc);
    let top = s.doc.layers[2].id;
    run(&mut s, Kind::Selected);
    assert_eq!(s.doc.layers.len(), 2);
    assert_eq!(
        serde_json::to_value(&s.doc.layers[0]).unwrap(),
        original_middle
    );
    assert_eq!(s.doc.layers[1].id, top);
    assert_eq!(s.doc.layers[1].name, "Upper");
    assert_eq!(picture(&s.doc), expected);
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_stamp_visible_copies_current_pixels_keeps_originals_and_selects_new_layer() {
    let mut s = fixture();
    s.doc.layers[0].locked = true;
    let before = state(&s);
    let original = serde_json::to_value(&s.doc.layers).unwrap();
    let expected = picture(&s.doc);
    run(&mut s, Kind::StampVisible);
    assert_eq!(s.doc.layers.len(), 3);
    assert_eq!(s.active_layer, Some(2));
    assert_eq!(serde_json::to_value(&s.doc.layers[..2]).unwrap(), original);
    let mut isolated = s.doc.clone();
    isolated.layers = vec![s.doc.layers[2].clone()];
    assert_eq!(picture(&isolated), expected);
    assert!(!isolated.layers[0].locked);
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_stamp_down_keeps_upper_source_and_active_owner_while_editing_lower() {
    let mut s = fixture();
    let before = state(&s);
    let expected = picture(&s.doc);
    let upper = serde_json::to_value(&s.doc.layers[1]).unwrap();
    let lower = s.doc.layers[0].id;
    let upper_id = s.doc.layers[1].id;
    run(&mut s, Kind::StampDown(upper_id));
    assert_eq!(s.doc.layers.len(), 2);
    assert_eq!(s.doc.layers[0].id, lower);
    assert_eq!(serde_json::to_value(&s.doc.layers[1]).unwrap(), upper);
    assert_eq!(s.active_layer, Some(1));
    let mut isolated = s.doc.clone();
    isolated.layers.truncate(1);
    assert_eq!(picture(&isolated), expected);
    undo_reopen(&mut s, &before);
}

#[test]
fn craft_stamp_selected_native_hdr_cmyka_clipping_keeps_all_original_samples() {
    for cmyk in [false, true] {
        let mut s = fixture();
        s.doc.width = 1.;
        s.doc.height = 1.;
        s.doc.color.depth = Depth::F32;
        let pixel = |values: Vec<f32>| {
            if cmyk {
                Pixels::from_cmyk(1, 1, Depth::F32, values, vec![], 0).unwrap()
            } else {
                Pixels::from_samples(1, 1, Depth::F32, values).unwrap()
            }
        };
        let lower = if cmyk {
            vec![0.125, 0.25, 0.375, 0.5, 0.5]
        } else {
            vec![1.25, -0.125, 0.5, 0.5]
        };
        let upper = if cmyk {
            vec![0.75, 0.625, 0.5, 0.375, 0.75]
        } else {
            vec![2., -0.5, 0.25, 0.75]
        };
        s.doc.color.model = if cmyk {
            crate::deep_color::cms::Model::Cmyk
        } else {
            crate::deep_color::cms::Model::Rgb
        };
        s.doc.layers = vec![
            Layer::placed_raster("Base", pixel(lower), Pt::ZERO, Pt::splat(1.)),
            Layer::placed_raster("Clip", pixel(upper), Pt::ZERO, Pt::splat(1.)),
        ];
        s.doc.layers[1].clipped = true;
        s.select_layer_rows(0, false, false);
        s.select_layer_rows(1, true, false);
        let before = state(&s);
        let sources = serde_json::to_value(&s.doc.layers).unwrap();
        run(&mut s, Kind::StampSelected);
        assert_eq!(s.doc.layers.len(), 3);
        assert_eq!(serde_json::to_value(&s.doc.layers[..2]).unwrap(), sources);
        let mut isolated = s.doc.clone();
        isolated.layers = vec![s.doc.layers[2].clone()];
        let actual = crate::deep_color::composite(&isolated, 1).unwrap();
        if cmyk {
            for (&a, b) in actual
                .native
                .as_ref()
                .unwrap()
                .cmyk
                .as_ref()
                .unwrap()
                .iter()
                .zip([0.59375, 0.53125, 0.46875, 0.40625, 0.5])
            {
                assert!((a - b).abs() < 2e-7);
            }
        } else {
            for (a, b) in actual
                .sample(0)
                .into_iter()
                .zip([1.8125, -0.40625, 0.3125, 0.5])
            {
                assert!((a - b).abs() < 2e-7);
            }
        }
        undo_reopen(&mut s, &before);
    }
}

#[test]
fn craft_stamp_and_selected_merge_refuse_stale_locked_and_cancelled_results_atomically() {
    let mut s = fixture();
    s.select_layer_rows(0, false, false);
    s.select_layer_rows(1, true, false);
    let before = state(&s);
    let prepared = s
        .capture_layer_merge(Kind::Selected)
        .unwrap()
        .render(&crate::ml::NoProgress)
        .unwrap();
    s.activate_layer_tree(1);
    assert!(
        s.apply_layer_merge(prepared)
            .unwrap_err()
            .contains("two sibling")
    );
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
    s.select_layer_rows(0, false, false);
    s.select_layer_rows(1, true, false);
    s.doc.layers[0].locked = true;
    assert!(s.capture_layer_merge(Kind::Selected).is_err());
    assert!(s.capture_layer_merge(Kind::StampSelected).is_ok());
    let before = state(&s);
    let progress = crate::ml::NoProgress;
    let request = s.capture_layer_merge(Kind::StampVisible).unwrap();
    let mut prepared = request.render(&progress).unwrap();
    prepared.target.document.push_str("stale");
    assert!(s.apply_layer_merge(prepared).is_err());
    assert_eq!(state(&s), before);
    struct Cancel;
    impl crate::ml::Progress for Cancel {
        fn report(&self, _: &str, _: usize, _: usize) {}
        fn cancelled(&self) -> bool {
            true
        }
    }
    assert!(
        s.capture_layer_merge(Kind::StampVisible)
            .unwrap()
            .render(&Cancel)
            .is_err()
    );
    assert_eq!(state(&s), before);
    assert_eq!(s.history.len(), 0);
}

#[test]
fn craft_visible_merge_and_stamp_do_not_reveal_hidden_clipping_followers() {
    let mut s = fixture();
    s.doc.layers[0].visible = false;
    s.doc.layers[1].clipped = true;
    for name in ["Visible one", "Visible two"] {
        s.doc.layers.push(Layer::placed_raster(
            name,
            Pixels::from_rgba(1, 1, vec![50, 100, 150, 128]).unwrap(),
            Pt::new(8., 6.),
            Pt::splat(1.),
        ));
    }
    s.activate_layer_tree(3);
    let expected = picture(&s.doc);
    let before = state(&s);
    run(&mut s, Kind::Visible);
    assert_eq!(s.doc.layers.len(), 3);
    assert!(!s.doc.layers[0].visible);
    assert!(s.doc.layers[1].clipped);
    assert_eq!(picture(&s.doc), expected);
    undo_reopen(&mut s, &before);
    s.undo();
    s.history.clear();
    let original = serde_json::to_value(&s.doc.layers).unwrap();
    let inserted = s.active_layer.unwrap() + 1;
    run(&mut s, Kind::StampVisible);
    assert_eq!(s.active_layer, Some(inserted));
    assert_eq!(
        serde_json::to_value(
            s.doc
                .layers
                .iter()
                .enumerate()
                .filter_map(|(i, l)| (i != inserted).then_some(l))
                .collect::<Vec<_>>()
        )
        .unwrap(),
        original
    );
    let mut isolated = s.doc.clone();
    isolated.layers = vec![s.doc.layers[inserted].clone()];
    assert_eq!(picture(&isolated), expected);
}

#[test]
fn craft_stamp_group_keeps_locked_source_mask_styles_and_full_off_canvas_paint() {
    let mut s = fixture();
    let mut group = Layer::group("Styled source");
    group.pass_through = false;
    group.opacity = 0.5;
    group.locked = true;
    for layer in &mut s.doc.layers {
        layer.parent = Some(group.id);
    }
    group.mask = Some(Pixels::from_rgba(16, 12, [128, 128, 128, 255].repeat(16 * 12)).unwrap());
    group.filters.items.push(crate::filter::Fx::Shadow {
        dx: 2.,
        dy: 1.,
        blur: 0.,
        spread: 0.,
        color: Rgba::rgb(30, 20, 10),
        knockout: false,
        blend: crate::color::Blend::Normal,
        opacity: 1.,
    });
    let owner = group.id;
    s.doc.layers.push(group);
    s.activate_layer_tree(2);
    let before = state(&s);
    let original = serde_json::to_value(&s.doc.layers).unwrap();
    let expected = picture(&s.doc);
    let outside = view(&s.doc);
    run(&mut s, Kind::StampGroup(owner));
    assert_eq!(s.doc.layers.len(), 4);
    assert_eq!(s.active_layer, Some(3));
    assert_eq!(serde_json::to_value(&s.doc.layers[..3]).unwrap(), original);
    assert!(!s.doc.layers[3].locked);
    let mut isolated = s.doc.clone();
    isolated.layers = vec![s.doc.layers[3].clone()];
    assert_eq!(picture(&isolated), expected);
    assert_eq!(view(&isolated), outside);
    undo_reopen(&mut s, &before);
}

fn content_fixture(depth: Depth, cmyk: bool) -> Studio {
    let mut s = fixture();
    s.doc = Document::new("Content bounds", 300., 400., 96.);
    s.doc.transparent = true;
    s.doc.color.depth = depth;
    s.doc.color.model = if cmyk {
        crate::deep_color::cms::Model::Cmyk
    } else {
        crate::deep_color::cms::Model::Rgb
    };
    s.doc.layers.clear();
    for (name, origin, w, h, color) in [
        (
            "First",
            Pt::new(36., 296.),
            20usize,
            30usize,
            [0.75, 0.25, 0.125],
        ),
        (
            "Second",
            Pt::new(86., 316.),
            25usize,
            20usize,
            [0.125, 0.375, 0.75],
        ),
    ] {
        let mut rgba = vec![0.; 64 * 64 * 4];
        let mut inks = vec![0.; 64 * 64 * 5];
        for y in 4..4 + h {
            for x in 4..4 + w {
                let i = y * 64 + x;
                let r = if depth == Depth::F32 {
                    color[0] * 2.
                } else {
                    color[0]
                };
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[r, color[1], color[2], 1.]);
                inks[i * 5..i * 5 + 5].copy_from_slice(&[color[0], color[1], color[2], 0.25, 1.]);
            }
        }
        let pixels = if cmyk {
            Pixels::from_cmyk(64, 64, depth, inks, vec![], 0).unwrap()
        } else if depth == Depth::U8 {
            Pixels::from_rgba(
                64,
                64,
                rgba.into_iter().map(crate::deep_color::byte).collect(),
            )
            .unwrap()
        } else {
            Pixels::from_samples(64, 64, depth, rgba).unwrap()
        };
        s.doc
            .layers
            .push(Layer::placed_raster(name, pixels, origin, Pt::splat(64.)));
    }
    s.activate_layer_tree(1);
    s.history.clear();
    s
}

#[test]
fn craft_content_bounds_merges_and_stamps_hug_paint_across_all_native_color_depths() {
    for depth in [Depth::U8, Depth::U16, Depth::F32] {
        for cmyk in [false, true] {
            for op in 0..6 {
                let mut s = content_fixture(depth, cmyk);
                let owner = s.doc.layers[1].id;
                let kind = match op {
                    0 => Kind::Down(owner),
                    1 => Kind::Selected,
                    2 => Kind::Visible,
                    3 => Kind::StampDown(owner),
                    4 => Kind::StampSelected,
                    _ => Kind::StampVisible,
                };
                if matches!(kind, Kind::Selected | Kind::StampSelected) {
                    s.select_layer_rows(0, false, false);
                    s.select_layer_rows(1, true, false);
                }
                let before = state(&s);
                let expected = picture(&s.doc);
                run(&mut s, kind);
                let target = if matches!(kind, Kind::StampVisible | Kind::StampSelected) {
                    2
                } else {
                    0
                };
                let layer = &s.doc.layers[target];
                let pixels = layer.kind.pixels().unwrap();
                assert_eq!(pixels.w, 75, "{depth:?}/{cmyk}/{kind:?}");
                assert_eq!(pixels.h, 40, "{depth:?}/{cmyk}/{kind:?}");
                assert_eq!(layer.kind.raster_xform().unwrap().0, Pt::new(40., 300.));
                assert_eq!(layer.kind.raster_xform().unwrap().1, Pt::new(75., 40.));
                assert_eq!(picture(&s.doc), expected, "{depth:?}/{cmyk}/{kind:?}");
                assert!(pixels.sample(5 * 75 + 5)[3] > 0.99);
                assert!(pixels.sample(25 * 75 + 55)[3] > 0.99);
                assert_eq!(pixels.sample(10 * 75 + 30)[3], 0.);
                if cmyk {
                    assert!(pixels.native.as_ref().unwrap().cmyk.is_some());
                }
                if depth != Depth::U8 {
                    assert_eq!(pixels.native.as_ref().unwrap().depth, depth);
                }
                undo_reopen(&mut s, &before);
            }
        }
    }
}

#[test]
fn craft_content_bounds_masked_fill_and_live_clipping_follow_actual_coverage() {
    for clipped in [false, true] {
        let mut s = fixture();
        s.doc = Document::new(format!("Masked fill {clipped}"), 40., 32., 96.);
        s.doc.transparent = true;
        let mut fill = Layer::raster("Masked fill", 40, 32).unwrap();
        fill.kind
            .pixels_mut()
            .unwrap()
            .replace_bytes([30, 90, 150, 255].repeat(40 * 32))
            .unwrap();
        fill.kind.set_raster_xform(Pt::ZERO, Pt::new(40., 32.), 0.);
        let mut mask = vec![0; 40 * 32 * 4];
        for y in 8..13 {
            for x in 7..11 {
                mask[(y * 40 + x) * 4..(y * 40 + x + 1) * 4].copy_from_slice(&[255; 4]);
            }
        }
        fill.mask = Some(Pixels::from_rgba(40, 32, mask).unwrap());
        let mut top = Layer::placed_raster(
            "Top",
            Pixels::from_rgba(2, 3, [210, 70, 20, 255].repeat(6)).unwrap(),
            Pt::new(14., 17.),
            Pt::new(2., 3.),
        );
        top.clipped = clipped;
        s.doc.layers = vec![fill, top];
        s.activate_layer_tree(1);
        let before = state(&s);
        let expected = picture(&s.doc);
        run(&mut s, Kind::Visible);
        let layer = &s.doc.layers[0];
        assert_eq!(layer.kind.raster_xform().unwrap().0, Pt::new(7., 8.));
        assert_eq!(
            layer.kind.raster_xform().unwrap().1,
            if clipped {
                Pt::new(4., 5.)
            } else {
                Pt::new(9., 12.)
            }
        );
        assert_eq!(picture(&s.doc), expected);
        undo_reopen(&mut s, &before);
    }
}

#[test]
fn craft_content_bounds_empty_merge_uses_one_pixel_and_flatten_keeps_opaque_canvas() {
    for depth in [Depth::U8, Depth::F32] {
        let mut s = fixture();
        s.doc = Document::new("Empty content", 40., 32., 96.);
        s.doc.transparent = true;
        s.doc.color.depth = depth;
        s.doc.layers = vec![
            Layer::placed_raster(
                "Empty first",
                Pixels::new(3, 2).unwrap(),
                Pt::new(4., 7.),
                Pt::new(3., 2.),
            ),
            Layer::placed_raster(
                "Empty last",
                Pixels::new(3, 2).unwrap(),
                Pt::new(11., 14.),
                Pt::new(3., 2.),
            ),
        ];
        s.activate_layer_tree(1);
        let before = state(&s);
        run(&mut s, Kind::Visible);
        let layer = &s.doc.layers[0];
        assert_eq!(layer.kind.raster_xform().unwrap().0, Pt::ZERO);
        assert_eq!(layer.kind.raster_xform().unwrap().1, Pt::splat(1.));
        assert_eq!(layer.kind.pixels().unwrap().sample(0), [0.; 4]);
        undo_reopen(&mut s, &before);
        s.undo();
        s.history.clear();
        let before = state(&s);
        run(&mut s, Kind::Flatten);
        let layer = &s.doc.layers[0];
        assert_eq!(layer.kind.raster_xform().unwrap().0, Pt::ZERO);
        assert_eq!(layer.kind.raster_xform().unwrap().1, Pt::new(40., 32.));
        assert_eq!(layer.kind.pixels().unwrap().sample(0), [1.; 4]);
        assert!(layer.locked);
        undo_reopen(&mut s, &before);
    }
}
