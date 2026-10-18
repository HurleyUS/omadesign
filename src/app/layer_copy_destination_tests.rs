use super::*;
use crate::deep_color::{Depth, cms::DocumentColor};
use crate::document::Pixels;

fn fixture() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Pixel;
    s.doc = Document::new("Original", 40., 30., 144.);
    s.doc.workspace = Some(Persona::Pixel);
    s.doc.transparent = true;
    let pixels = Pixels::from_samples(
        2,
        1,
        Depth::F32,
        vec![1.25, -0., -0.125, 0.75, 0.1234567, 0.5, 0.8, 0.25],
    )
    .unwrap();
    let mut layer = Layer::placed_raster("Native", pixels, Pt::new(3., 5.), Pt::new(20., 10.));
    layer.mask = Some(
        Pixels::from_samples(1, 1, Depth::F32, vec![0.1234567, 0.1234567, 0.1234567, 1.]).unwrap(),
    );
    s.doc.layers = vec![layer];
    s.active_layer = Some(0);
    s.history.clear();
    s
}
fn encoded(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn destination(s: &Studio, index: usize) -> LayerCopyDestination {
    let (_, id, revision, _) = s.tab_preview_source(index).unwrap();
    LayerCopyDestination::Document {
        document: id.to_owned(),
        revision,
    }
}
fn reopen_undo(s: &mut Studio, before: &str) {
    let after = encoded(s);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
        after
    );
    assert_eq!(s.history.len(), 1);
    s.undo();
    assert_eq!(encoded(s), before);
    s.redo();
    assert_eq!(encoded(s), after);
}

#[test]
fn craft_layer_destination_named_current_is_one_transaction_and_preserves_native_source() {
    let mut s = fixture();
    let target = s.capture_layer_copy(0).unwrap();
    let to = destination(&s, 0);
    let before = encoded(&s);
    let pixels = s.doc.layers[0].kind.pixels().unwrap().clone();
    s.duplicate_layer_to(&target, "  Retouch Ω  ", &to).unwrap();
    assert_eq!(s.doc.layers[1].name, "Retouch Ω");
    assert_eq!(*s.doc.layers[1].kind.pixels().unwrap(), pixels);
    assert_eq!(*s.doc.layers[0].kind.pixels().unwrap(), pixels);
    assert!(std::sync::Arc::ptr_eq(
        &s.doc.layers[1]
            .kind
            .pixels()
            .unwrap()
            .native
            .as_ref()
            .unwrap()
            .samples,
        &pixels.native.as_ref().unwrap().samples
    ));
    reopen_undo(&mut s, &before);
}

#[test]
fn craft_layer_destination_new_keeps_canvas_pixels_motion_and_transparent_empty_space() {
    let mut s = fixture();
    s.doc.transparent = false;
    s.doc.motion.duration = 5.;
    s.doc.motion.fps = 24.;
    let layer = s.doc.layers[0].id;
    s.doc
        .motion
        .set_expression(layer, Prop::X, Some("time * 2".into()))
        .unwrap();
    let before = encoded(&s);
    let target = s.capture_layer_copy(0).unwrap();
    s.duplicate_layer_to(
        &target,
        "",
        &LayerCopyDestination::New {
            name: "Extracted".into(),
        },
    )
    .unwrap();
    assert_eq!(s.tab_count(), 2);
    assert_eq!(s.active_tab, 1);
    assert_eq!(
        (s.doc.name.as_str(), s.doc.width, s.doc.height, s.doc.dpi),
        ("Extracted", 40., 30., 144.)
    );
    assert!(s.doc.transparent && s.dirty && s.path.is_none());
    assert!(s.doc.cloud.is_none() && s.doc.collaboration.is_none() && s.doc.comments.is_empty());
    assert_eq!(s.doc.workspace, Some(Persona::Pixel));
    assert_eq!((s.doc.motion.duration, s.doc.motion.fps), (5., 24.));
    assert_eq!(s.doc.motion.expressions[0].shape, s.doc.layers[0].id);
    assert_eq!(s.doc.motion.expressions[0].source, "time * 2");
    let new = encoded(&s);
    assert_eq!(s.history.len(), 0);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&new).unwrap()).unwrap(),
        new
    );
    s.undo();
    assert_eq!(encoded(&s), new);
    s.redo();
    assert_eq!(encoded(&s), new);
    s.switch_tab(0);
    assert_eq!(encoded(&s), before);
    assert_eq!(s.history.len(), 0);
}

#[test]
fn craft_layer_destination_open_inserts_above_active_sibling_and_undo_belongs_to_destination() {
    let mut s = fixture();
    let before = encoded(&s);
    let mut doc = Document::new("Destination", 80., 60., 96.);
    doc.workspace = Some(Persona::Pixel);
    let group = Layer::group("Parent");
    let mut child = Layer::vector("Active");
    child.parent = Some(group.id);
    doc.layers = vec![child, group];
    let dest_before = crate::project::encode(&doc).unwrap();
    s.push_tab(TabState::new(doc, None));
    s.activate_layer_tree(0);
    s.switch_tab(0);
    let target = s.capture_layer_copy(0).unwrap();
    let to = destination(&s, 1);
    s.duplicate_layer_to(&target, "Transferred", &to).unwrap();
    assert_eq!(s.active_tab, 1);
    assert_eq!(
        s.doc
            .layers
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>(),
        ["Active", "Transferred", "Parent"]
    );
    assert_eq!(s.doc.layers[1].parent, Some(s.doc.layers[2].id));
    assert_eq!(s.active_layer, Some(1));
    reopen_undo(&mut s, &dest_before);
    s.switch_tab(0);
    assert_eq!(encoded(&s), before);
    assert_eq!(s.history.len(), 0);
}

#[test]
fn craft_layer_destination_converts_only_transferred_raster_into_destination_color_and_keeps_mask()
{
    let mut s = fixture();
    s.doc.color.depth = Depth::F32;
    let mask = s.doc.layers[0].mask.clone();
    let source = encoded(&s);
    let mut doc = Document::new("Eight bit", 40., 30., 72.);
    doc.color = DocumentColor::default();
    let before = crate::project::encode(&doc).unwrap();
    s.push_tab(TabState::new(doc, None));
    s.switch_tab(0);
    let target = s.capture_layer_copy(0).unwrap();
    let to = destination(&s, 1);
    s.duplicate_layer_to(&target, "Converted", &to).unwrap();
    assert_eq!(s.doc.layers.last().unwrap().mask, mask);
    let copied = s.doc.layers.last().unwrap().kind.pixels().unwrap();
    assert!(copied.native.is_none());
    assert_eq!(copied.data.as_slice(), &[255, 0, 0, 191, 31, 128, 204, 64]);
    reopen_undo(&mut s, &before);
    s.switch_tab(0);
    assert_eq!(encoded(&s), source);
}

#[test]
fn craft_layer_destination_retains_external_live_masks_component_family_tokens_and_original_source()
{
    use crate::artwork_mask::{ArtworkMask, MaskSource};
    use crate::layout_tokens::DesignToken;
    let mut s = fixture();
    s.doc.layers[0] = Layer::vector("Artwork");
    let mut definitions = Layer::vector("Definitions");
    definitions.visible = false;
    let mut frame = crate::layout::make_frame(Pt::new(0., 0.), Pt::new(20., 10.));
    frame.style.fill = Fill::Solid(Rgba::rgb(120, 200, 90));
    let frame_id = frame.id;
    definitions.kind.shapes_mut().unwrap().push(frame);
    s.doc.layers.push(definitions);
    crate::layout_components::make_component(&mut s.doc, 1, frame_id).unwrap();
    let instance =
        crate::layout_components::insert_instance(&mut s.doc, frame_id, 0, Pt::new(3., 5.), None)
            .unwrap();
    let token = DesignToken::number("Corners", 2.);
    s.doc.layout_tokens.push(token.clone());
    s.doc
        .find_shape_mut(0, instance)
        .unwrap()
        .layout
        .tokens
        .radius = Some(token.id);
    let mut mask = Layer::raster("External mask", 2, 1).unwrap();
    mask.kind.set_raster_xform(Pt::ZERO, Pt::new(40., 30.), 0.);
    mask.kind
        .pixels_mut()
        .unwrap()
        .replace_bytes(vec![255, 255, 255, 255, 128, 128, 128, 255])
        .unwrap();
    let mask_id = mask.id;
    s.doc.layers.push(mask);
    s.doc.layers[0]
        .artwork_masks
        .push(ArtworkMask::new(MaskSource {
            layer: mask_id,
            shape: None,
        }));
    let original = crate::source_attachment::SourceAttachment::new(
        "original.psd",
        "psd",
        b"original bytes".to_vec(),
    )
    .unwrap();
    s.doc.layers[0].source_attachment = Some(s.doc.retain_source(original.clone()));
    crate::layout_tokens::apply_tokens(&mut s.doc);
    let before = encoded(&s);
    let expected = crate::compositor::render_export_at(&s.doc, 1., 0., true).unwrap();
    assert!(expected.data().chunks_exact(4).any(|p| p[3] > 0));
    let target = s.capture_layer_copy(0).unwrap();
    s.duplicate_layer_to(
        &target,
        "Complete",
        &LayerCopyDestination::New { name: "New".into() },
    )
    .unwrap();
    assert!(
        s.doc
            .layers
            .iter()
            .any(|l| l.name == "Copy dependencies" && l.is_group && !l.visible && l.locked)
    );
    assert_eq!(s.doc.sources, [original]);
    assert_eq!(s.doc.layout_tokens.len(), 1);
    assert_ne!(s.doc.layout_tokens[0].id, token.id);
    let actual = crate::compositor::render_export_at(&s.doc, 1., 0., true).unwrap();
    assert_eq!(actual.data(), expected.data());
    let copied = s.doc.layers.iter().find(|l| l.name == "Complete").unwrap();
    let Some(ComponentBinding::Instance { main, .. }) = copied
        .kind
        .shapes()
        .unwrap()
        .iter()
        .find_map(|shape| shape.layout.component.as_ref())
    else {
        panic!("instance binding missing")
    };
    assert_ne!(*main, frame_id);
    assert!(s.doc.layers.iter().any(|l| l.find(*main).is_some()));
    let encoded = encoded(&s);
    let mut reopened = crate::project::decode(&encoded).unwrap();
    crate::layout_components::synchronize_all(&mut reopened).unwrap();
    assert_eq!(
        crate::compositor::render_export_at(&reopened, 1., 0., true)
            .unwrap()
            .data(),
        actual.data()
    );
    s.switch_tab(0);
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
}

#[test]
fn craft_layer_destination_svg_text_pattern_filter_sources_and_font_render_survive_native_reopen() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="60"><defs><pattern id="p" width="8" height="8" patternUnits="userSpaceOnUse"><rect width="4" height="8" fill="#d15c40"/><rect x="4" width="4" height="8" fill="#418bc5"/></pattern><filter id="f"><feGaussianBlur stdDeviation="0.3"/></filter></defs><rect x="2" y="2" width="90" height="18" fill="url(#p)"/><text x="5" y="48" font-family="DejaVu Sans" font-size="24" filter="url(#f)">Native Ω</text></svg>"##;
    let (mut doc, _) = crate::formats::svg::read(svg, "Original SVG").unwrap();
    let group = Layer::group("Imported artwork");
    for layer in &mut doc.layers {
        if layer.parent.is_none() {
            layer.parent = Some(group.id);
        }
    }
    doc.layers.push(group);
    let mut s = fixture();
    s.doc = doc;
    let index = s.doc.layers.len() - 1;
    let before = encoded(&s);
    let expected = crate::compositor::render_export_at(&s.doc, 1., 0., true).unwrap();
    assert!(
        s.doc
            .layers
            .iter()
            .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
            .any(|shape| matches!(shape.geom, Geom::Text(_)))
    );
    let target = s.capture_layer_copy(index).unwrap();
    s.duplicate_layer_to(
        &target,
        "Editable SVG",
        &LayerCopyDestination::New {
            name: "Copied SVG".into(),
        },
    )
    .unwrap();
    let output = encoded(&s);
    let reopened = crate::project::decode(&output).unwrap();
    reopened.validate_sources().unwrap();
    crate::typography::validate_document_fonts(&reopened).unwrap();
    let actual = crate::compositor::render_export_at(&reopened, 1., 0., true).unwrap();
    assert_eq!(expected.data(), actual.data());
    assert!(
        reopened
            .layers
            .iter()
            .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
            .any(|shape| matches!(shape.geom, Geom::Text(_)))
    );
    s.switch_tab(0);
    assert_eq!(encoded(&s), before);
}

#[test]
fn craft_layer_destination_conflicting_unicode_token_names_remain_bounded_and_independent() {
    use crate::layout_tokens::DesignToken;
    let mut s = fixture();
    let token = DesignToken::color("É".repeat(64), Rgba::rgb(210, 30, 50));
    let mut layer = Layer::vector("Bound color");
    let mut shape = crate::layout::make_frame(Pt::ZERO, Pt::new(10., 10.));
    shape.layout.tokens.fill = Some(token.id);
    layer.kind.shapes_mut().unwrap().push(shape);
    s.doc.layers = vec![layer];
    s.doc.layout_tokens = vec![token];
    crate::layout_tokens::apply_tokens(&mut s.doc);
    let source = encoded(&s);
    let mut doc = Document::new("Destination", 40., 30., 96.);
    let existing = DesignToken::color("é".repeat(64), Rgba::rgb(10, 40, 220));
    doc.layout_tokens = vec![existing.clone()];
    let before = crate::project::encode(&doc).unwrap();
    s.push_tab(TabState::new(doc, None));
    s.switch_tab(0);
    let target = s.capture_layer_copy(0).unwrap();
    let to = destination(&s, 1);
    s.duplicate_layer_to(&target, "Independent", &to).unwrap();
    crate::layout_tokens::validate_tokens(&s.doc).unwrap();
    assert_eq!(s.doc.layout_tokens[0], existing);
    assert_eq!(s.doc.layout_tokens[1].name.len(), 128);
    assert!(s.doc.layout_tokens[1].name.ends_with(" 2"));
    assert_eq!(
        s.doc.layers.last().unwrap().kind.shapes().unwrap()[0]
            .style
            .fill,
        Fill::Solid(Rgba::rgb(210, 30, 50))
    );
    reopen_undo(&mut s, &before);
    s.switch_tab(0);
    assert_eq!(encoded(&s), source);
}

#[test]
fn craft_layer_destination_external_navigation_refuses_and_contained_frames_keep_their_links() {
    use crate::layout_prototype::{Interaction, PrototypeAction, Transition, Trigger};
    let mut s = fixture();
    let mut first = crate::layout::make_frame(Pt::ZERO, Pt::new(10., 10.));
    let second = crate::layout::make_frame(Pt::new(20., 0.), Pt::new(10., 10.));
    first.name = "Start".into();
    let second_id = second.id;
    first.layout.interactions.push(Interaction {
        trigger: Trigger::Click,
        action: PrototypeAction::Navigate { target: second_id },
        transition: Transition::Instant,
        duration_ms: 200,
    });
    let mut a = Layer::vector("First");
    a.kind.shapes_mut().unwrap().push(first);
    let mut b = Layer::vector("Second");
    b.kind.shapes_mut().unwrap().push(second.clone());
    s.doc.layers = vec![a, b];
    let before = encoded(&s);
    let target = s.capture_layer_copy(0).unwrap();
    let new = LayerCopyDestination::New {
        name: "Navigation copy".into(),
    };
    assert!(
        s.duplicate_layer_to(&target, "Copy", &new)
            .unwrap_err()
            .contains("outside the copy")
    );
    assert_eq!(encoded(&s), before);
    assert_eq!(s.tab_count(), 1);
    assert_eq!(s.history.len(), 0);
    s.doc.layers.pop();
    s.doc.layers[0].kind.shapes_mut().unwrap().push(second);
    let before = encoded(&s);
    let target = s.capture_layer_copy(0).unwrap();
    s.duplicate_layer_to(&target, "Copy", &new).unwrap();
    let shapes = s.doc.layers[0].kind.shapes().unwrap();
    let start = shapes.iter().find(|s| s.name == "Start").unwrap();
    let PrototypeAction::Navigate { target } = start.layout.interactions[0].action else {
        panic!("missing navigation")
    };
    assert_ne!(target, second_id);
    assert!(shapes.iter().any(|s| s.id == target));
    crate::layout_prototype::validate_interaction(
        &s.doc,
        0,
        start.id,
        &start.layout.interactions[0],
    )
    .unwrap();
    let output = encoded(&s);
    let reopened = crate::project::decode(&output).unwrap();
    assert_eq!(crate::project::encode(&reopened).unwrap(), output);
    s.switch_tab(0);
    assert_eq!(encoded(&s), before);
}

#[test]
fn craft_layer_destination_stale_source_destination_invalid_name_and_channel_refs_do_not_mutate() {
    let mut s = fixture();
    let target = s.capture_layer_copy(0).unwrap();
    let to = destination(&s, 0);
    let before = encoded(&s);
    assert!(s.duplicate_layer_to(&target, "bad\nname", &to).is_err());
    let mut stale = target.clone();
    stale.document = "closed-document".into();
    assert!(s.duplicate_layer_to(&stale, "Copy", &to).is_err());
    let missing = LayerCopyDestination::Document {
        document: "missing".into(),
        revision: target.revision,
    };
    assert!(s.duplicate_layer_to(&target, "Copy", &missing).is_err());
    assert_eq!(encoded(&s), before);
    assert_eq!(s.history.len(), 0);
    assert_eq!(s.tab_count(), 1);
    let id = s.doc.layers[0].id;
    s.doc
        .motion
        .set_expression(id, Prop::Y, Some(format!("property({id}, X)")))
        .unwrap();
    let before = encoded(&s);
    assert!(
        s.duplicate_layer_to(
            &target,
            "Copy",
            &LayerCopyDestination::New { name: "New".into() }
        )
        .unwrap_err()
        .contains("channel reference")
    );
    assert_eq!(encoded(&s), before);
    assert_eq!(s.tab_count(), 1);
    assert_eq!(s.history.len(), 0);
    let mut doc = Document::new("Other", 40., 30., 72.);
    doc.layers.clear();
    s.push_tab(TabState::new(doc, None));
    let old = destination(&s, 1);
    s.mark();
    s.switch_tab(0);
    let target = s.capture_layer_copy(0).unwrap();
    assert!(
        s.duplicate_layer_to(&target, "Copy", &old)
            .unwrap_err()
            .contains("changed or closed")
    );
    assert_eq!(s.active_tab, 0);
    assert_eq!(encoded(&s), before);
}
