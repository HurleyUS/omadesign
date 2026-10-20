use super::*;
use crate::{app::Studio, document::{Document, Layer}};

fn studio() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.doc = Document::new("Spot", 4., 1., 72.);
    s.doc.transparent = false;
    s.doc.layers = vec![Layer::raster("Pixels", 4, 1).unwrap()];
    s.active_layer = Some(0);
    s.history.clear();
    s
}
fn ink() -> Spot {
    Spot { ink: [0., 0.5, 1.], solidity: 1. }
}

#[test]
fn craft_spot_channel_from_selection_tints_canvas_and_export_in_one_undo() {
    let mut s = studio();
    s.replace_pixel_selection(vec![0, 0, 255, 255], crate::app::masking::SelectionSpace { w: 4, h: 1, transform: tiny_skia::Transform::identity() });
    let target = s.capture_channels().unwrap();
    let id = s.add_spot_channel(&target, "", ink()).unwrap();
    assert_eq!(s.history.len(), 1);
    assert_eq!(s.doc.channels[0].name, "Spot 1");
    assert_eq!(s.doc.channels[0].spot, Some(ink()));
    let png = crate::compositor::export_png(&s.doc, 1).unwrap();
    let img = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(img.get_pixel(0, 0).0, [255, 255, 255, 255], "uncovered paper stays white");
    assert_eq!(img.get_pixel(3, 0).0, [0, 128, 255, 255], "full ink at its color");
    let target = s.capture_channels().unwrap();
    assert!(s.set_channel_spot(&target, id, Some(Spot { solidity: 0.5, ..ink() })).unwrap());
    let img = image::load_from_memory(&crate::compositor::export_png(&s.doc, 1).unwrap()).unwrap().to_rgba8();
    assert_eq!(img.get_pixel(3, 0).0, [128, 191, 255, 255], "half solidity mixes with paper");
    let target = s.capture_channels().unwrap();
    assert!(s.set_channel_spot(&target, id, None).unwrap());
    let img = image::load_from_memory(&crate::compositor::export_png(&s.doc, 1).unwrap()).unwrap().to_rgba8();
    assert_eq!(img.get_pixel(3, 0).0, [255, 255, 255, 255], "alpha channels do not tint");
    s.undo();
    s.undo();
    s.undo();
    assert!(s.doc.channels.is_empty());
}

#[test]
fn craft_spot_channel_bumps_feature_37_only_with_spot_and_old_readers_refuse() {
    let mut s = studio();
    let target = s.capture_channels().unwrap();
    let plane = crate::channels::Plane::from_mask(&[0, 64, 128, 255], 4, 1, tiny_skia::Transform::identity(), &crate::deep_color::storage::check).unwrap();
    let alpha = s.save_channel(&target, plane, "").unwrap();
    let plain: serde_json::Value = serde_json::from_str(&crate::project::encode(&s.doc).unwrap()).unwrap();
    assert_eq!(plain["version"], 32, "alpha-only files keep the channels version");
    let target = s.capture_channels().unwrap();
    s.set_channel_spot(&target, alpha, Some(ink())).unwrap();
    let encoded = crate::project::encode(&s.doc).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value["version"], 37);
    let reopened = crate::project::decode(&encoded).unwrap();
    assert_eq!(reopened.channels[0].spot, Some(ink()));
    assert_eq!(crate::project::encode(&reopened).unwrap(), encoded);
    value["version"] = 36.into();
    assert!(crate::project::decode(&value.to_string()).is_err(), "an underdeclared spot file is refused");
}

#[test]
fn craft_spot_channel_empty_new_channel_is_paintable_and_invalid_ink_refused() {
    let mut s = studio();
    let target = s.capture_channels().unwrap();
    assert!(s.add_spot_channel(&target, "Bad", Spot { ink: [2., 0., 0.], solidity: 1. }).is_err());
    assert!(s.add_spot_channel(&target, "Bad", Spot { ink: [0., 0., 0.], solidity: f32::NAN }).is_err());
    assert!(s.doc.channels.is_empty());
    let id = s.add_spot_channel(&target, "Pantone 300", ink()).unwrap();
    let c = &s.doc.channels[0];
    assert_eq!((c.plane.width, c.plane.height, c.name.as_str()), (4, 1, "Pantone 300"));
    assert!(c.plane.samples.iter().all(|v| *v == 0.));
    s.select_channel_target(Some(id)).unwrap();
    assert_eq!(s.channel_paint_target().map(|c| c.id), Some(id), "existing channel tools paint spot channels");
    let mut bad = s.doc.channels.clone();
    bad[0].spot = Some(Spot { ink: [0., 0., 0.], solidity: 3. });
    assert!(crate::channels::validate(&bad).is_err());
}

#[test]
fn craft_spot_channel_pdf_flattens_to_process_with_warning() {
    let mut s = studio();
    s.replace_pixel_selection(vec![255; 4], crate::app::masking::SelectionSpace { w: 4, h: 1, transform: tiny_skia::Transform::identity() });
    let target = s.capture_channels().unwrap();
    s.add_spot_channel(&target, "", ink()).unwrap();
    let (bytes, warnings) = crate::formats::pdf::write(&s.doc).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    assert!(warnings.iter().any(|w| w.contains("Spot channels were flattened")), "{warnings:?}");
    assert_eq!(s.doc.channels[0].spot, Some(ink()), "export never edits the document");
}
