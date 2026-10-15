use super::*;
use crate::{
    color::Rgba,
    document::{Document, Fill, Layer, Pixels, Shape, Style},
    geom::{Geom, Pt},
};

fn rectangle(x: f32, y: f32, w: f32, h: f32, color: Rgba) -> Shape {
    Shape::new(
        Geom::Rect {
            origin: Pt::new(x, y),
            size: Pt::new(w, h),
            radius: 0.,
        },
        Style {
            fill: Fill::Solid(color),
            stroke: None,
        },
    )
}

fn reference(source: &str, scale: u32) -> tiny_skia::Pixmap {
    let tree = usvg::Tree::from_str(source, &usvg::Options::default()).unwrap();
    let mut pm = tiny_skia::Pixmap::new(
        (tree.size().width() * scale as f32) as u32,
        (tree.size().height() * scale as f32) as u32,
    )
    .unwrap();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale as f32, scale as f32),
        &mut pm.as_mut(),
    );
    pm
}

fn matches(actual: &tiny_skia::Pixmap, expected: &tiny_skia::Pixmap, name: &str) {
    assert_eq!(
        (actual.width(), actual.height()),
        (expected.width(), expected.height())
    );
    let mut worst = 0;
    let mut total = 0u64;
    for (a, b) in actual.data().iter().zip(expected.data()) {
        let difference = a.abs_diff(*b);
        worst = worst.max(difference);
        total += u64::from(difference);
    }
    assert!(
        worst <= 5 && total as f64 / (actual.data().len() as f64) < 0.4,
        "{name}: largest channel error {worst}, total {total}"
    );
}

#[test]
fn svg_masks_keep_editable_sources_and_match_independent_rendering() {
    let fixtures = [
        (
            "clip",
            r##"<defs><clipPath id="c"><circle cx="16" cy="13" r="10"/></clipPath></defs><rect width="40" height="30" fill="red" clip-path="url(#c)"/>"##,
        ),
        (
            "transformed-clip",
            r##"<defs><clipPath id="c" transform="translate(3 2)"><rect width="12" height="10"/></clipPath></defs><g transform="translate(7 4)" clip-path="url(#c)"><rect width="30" height="25" fill="red"/></g>"##,
        ),
        (
            "bbox-clip",
            r##"<defs><clipPath id="c" clipPathUnits="objectBoundingBox"><rect width=".5" height="1"/></clipPath></defs><rect x="5" y="4" width="30" height="20" fill="red" clip-path="url(#c)"/>"##,
        ),
        (
            "luminance",
            r##"<defs><linearGradient id="g"><stop stop-color="black"/><stop offset="1" stop-color="white"/></linearGradient><mask id="m"><rect width="40" height="30" fill="url(#g)"/></mask></defs><rect width="40" height="30" fill="red" mask="url(#m)"/>"##,
        ),
        (
            "alpha",
            r##"<defs><mask id="m" mask-type="alpha"><rect x="5" y="4" width="20" height="16" fill="black" opacity=".5"/></mask></defs><rect width="40" height="30" fill="blue" mask="url(#m)"/>"##,
        ),
        (
            "mask-region",
            r##"<defs><mask id="m" x=".25" y=".25" width=".5" height=".5"><rect width="40" height="30" fill="white"/></mask></defs><rect width="40" height="30" fill="red" mask="url(#m)"/>"##,
        ),
        (
            "bbox-mask-content",
            r##"<defs><mask id="m" maskContentUnits="objectBoundingBox"><rect width=".5" height="1" fill="white"/></mask></defs><rect x="5" y="4" width="30" height="20" fill="red" mask="url(#m)"/>"##,
        ),
        (
            "nested",
            r##"<defs><clipPath id="a"><rect x="5" y="3" width="25" height="24"/></clipPath><clipPath id="b" clip-path="url(#a)"><rect x="8" width="26" height="20"/></clipPath><mask id="m"><rect width="40" height="30" fill="white" opacity=".5"/></mask></defs><g mask="url(#m)" clip-path="url(#b)"><rect width="40" height="30" fill="red"/></g>"##,
        ),
        (
            "rotated-mask",
            r##"<defs><mask id="m" maskUnits="userSpaceOnUse" x="0" y="0" width="40" height="30"><rect x="8" y="5" width="20" height="15" fill="white"/></mask></defs><g transform="rotate(15 20 15)" mask="url(#m)"><rect width="40" height="30" fill="red"/></g>"##,
        ),
        (
            "filtered-mask",
            r##"<defs><filter id="f"><feGaussianBlur stdDeviation="1"/></filter><mask id="m"><rect x="8" y="6" width="20" height="16" fill="white"/></mask></defs><g filter="url(#f)" mask="url(#m)"><rect x="5" y="4" width="28" height="22" fill="red"/></g>"##,
        ),
    ];
    for (name, body) in fixtures {
        let source = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30">{body}</svg>"#
        );
        let (doc, notes) = crate::formats::svg::read(&source, name).unwrap();
        assert!(!doc.mask_sources().is_empty(), "{name}");
        assert!(
            doc.layers
                .iter()
                .all(|l| l.mask.is_none() && l.kind.pixels().is_none()),
            "{name}: mask must retain vectors"
        );
        assert!(notes.is_empty(), "{name}: {notes:?}");
        let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        let svg = crate::svg::export(&reopened).unwrap();
        assert!(
            !svg.contains("data:image"),
            "{name}: vector export must retain paths"
        );
        for scale in [1, 4] {
            let expected = reference(&source, scale);
            let actual = crate::compositor::render_export(&reopened, scale).unwrap();
            matches(&actual, &expected, &format!("{name} native {scale}x"));
            matches(
                &reference(&svg, scale),
                &expected,
                &format!("{name} exported {scale}x"),
            );
        }
        let (roundtrip, _) = crate::formats::svg::read(&svg, name).unwrap();
        matches(
            &crate::compositor::render_export(&roundtrip, 1).unwrap(),
            &reference(&source, 1),
            &format!("{name} roundtrip"),
        );
    }
}

#[test]
fn sources_stay_live_for_layers_objects_frames_images_and_groups() {
    for source_kind in ["shape", "layer", "image", "frame", "group"] {
        for target_kind in ["shape", "layer", "image", "frame", "group"] {
            let mut doc = Document::new("live", 40., 30., 96.);
            doc.transparent = true;
            let mut source = Layer::vector("Mask source");
            let mut shape = rectangle(4., 3., 12., 10., Rgba::BLACK);
            if source_kind == "frame" {
                shape.layout = crate::layout::FrameLayout::frame();
            }
            let source_id = shape.id;
            source.kind.shapes_mut().unwrap().push(shape);
            if source_kind == "image" {
                source = Layer::placed_raster(
                    "Mask image",
                    Pixels::from_rgba(12, 10, [0, 0, 0, 255].repeat(120)).unwrap(),
                    Pt::new(4., 3.),
                    Pt::new(12., 10.),
                );
            }
            if source_kind == "group" {
                let group = Layer::group("Mask group");
                source.parent = Some(group.id);
                doc.layers = vec![source, group];
            } else {
                doc.layers = vec![source];
            }
            let source_layer = doc.layers.last().unwrap().id;
            let source = MaskSource {
                layer: source_layer,
                shape: matches!(source_kind, "shape" | "frame").then_some(source_id),
            };
            let mut target = Layer::vector("Target");
            let mut shape = rectangle(0., 0., 40., 30., Rgba::rgb(255, 0, 0));
            let mask = ArtworkMask::new(source);
            if target_kind == "frame" {
                shape.layout = crate::layout::FrameLayout::frame();
            }
            if matches!(target_kind, "shape" | "frame") {
                shape.artwork_masks.push(mask.clone());
            }
            target.kind.shapes_mut().unwrap().push(shape);
            if target_kind == "image" {
                target = Layer::placed_raster(
                    "Image",
                    Pixels::from_rgba(40, 30, [255, 0, 0, 255].repeat(1200)).unwrap(),
                    Pt::ZERO,
                    Pt::new(40., 30.),
                );
            }
            if target_kind == "group" {
                let mut group = Layer::group("Target group");
                group.artwork_masks.push(mask);
                target.parent = Some(group.id);
                doc.layers.extend([target, group]);
            } else {
                if matches!(target_kind, "layer" | "image") {
                    target.artwork_masks.push(mask);
                }
                doc.layers.push(target);
            }
            doc.validate_hierarchy().unwrap();
            let first = crate::compositor::render_export(&doc, 1).unwrap();
            assert_eq!(
                first.pixel(8, 8).unwrap().red(),
                255,
                "{source_kind} -> {target_kind}"
            );
            assert_eq!(
                first.pixel(25, 8).unwrap().alpha(),
                0,
                "{source_kind} -> {target_kind}"
            );
            if let Some(shape) = doc.layers[0].kind.shapes_mut().and_then(|s| s.first_mut()) {
                shape.geom.translate(Pt::new(16., 0.));
            } else if let crate::document::LayerKind::Raster { origin, .. } =
                &mut doc.layers[0].kind
            {
                origin.x += 16.;
            }
            let moved = crate::compositor::render_export(&doc, 1).unwrap();
            assert_eq!(
                moved.pixel(8, 8).unwrap().alpha(),
                0,
                "moved {source_kind} -> {target_kind}"
            );
            assert_eq!(
                moved.pixel(25, 8).unwrap().red(),
                255,
                "moved {source_kind} -> {target_kind}"
            );
            let exported = crate::svg::export(&doc).unwrap();
            matches(
                &reference(&exported, 1),
                &moved,
                &format!("live export {source_kind} -> {target_kind}"),
            );
        }
    }
}

#[test]
fn mask_cycles_are_rejected_and_history_restores_the_binding() {
    let mut doc = Document::new("cycle", 40., 30., 96.);
    doc.layers = vec![Layer::vector("A"), Layer::vector("B")];
    let a = MaskSource {
        layer: doc.layers[0].id,
        shape: None,
    };
    let b = MaskSource {
        layer: doc.layers[1].id,
        shape: None,
    };
    let command = crate::document::Cmd::SetArtworkMasks {
        layer: 0,
        id: None,
        before: vec![],
        after: vec![ArtworkMask::new(b)],
    };
    crate::document::apply(&mut doc, &command);
    let mut history = crate::document::History::default();
    history.push(command.clone());
    doc.validate_artwork_masks().unwrap();
    doc.layers[1].artwork_masks.push(ArtworkMask::new(a));
    assert!(doc.validate_artwork_masks().is_err());
    doc.layers[1].artwork_masks.clear();
    crate::document::apply(&mut doc, &history.undo().unwrap());
    assert!(doc.layers[0].artwork_masks.is_empty());
    crate::document::apply(&mut doc, &history.redo().unwrap());
    assert_eq!(doc.layers[0].artwork_masks[0].source, b);
    assert!(
        crate::project::encode(&doc)
            .unwrap()
            .contains("\"version\":13")
    );
}
