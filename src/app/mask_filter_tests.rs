use super::*;
use crate::document::{Document, Layer};

fn studio(values: &[u8]) -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    let w = values.len() as u32;
    s.doc = Document::new("Mask", w as f32, 1.0, 96.0);
    s.doc.layers = vec![Layer::raster("Pixels", w, 1).unwrap()];
    s.active_layer = Some(0);
    s.add_layer_mask(0, true);
    let mask = s.doc.layers[0].mask.as_mut().unwrap();
    for (pixel, &v) in mask.bytes_mut().unwrap().as_chunks_mut::<4>().0.iter_mut().zip(values) {
        pixel.copy_from_slice(&[v, v, v, 255]);
    }
    mask.touch();
    s.history.clear();
    s
}
fn mask(s: &Studio) -> Vec<u8> {
    let m = s.doc.layers[0].mask.as_ref().unwrap();
    (0..m.w as usize).map(|i| (m.sample(i)[0] * 255.).round() as u8).collect()
}

#[test]
fn craft_mask_filter_threshold_contrast_and_blur_are_one_undo_each() {
    let mut s = studio(&[0, 64, 192, 255]);
    let original = mask(&s);
    assert!(s.filter_layer_mask(0, &MaskFilter::Harden.filter()).unwrap());
    assert_eq!(mask(&s), [0, 0, 255, 255]);
    assert_eq!(s.history.len(), 1);
    s.undo();
    assert_eq!(mask(&s), original);
    assert!(s.filter_layer_mask(0, &MaskFilter::Contrast.filter()).unwrap());
    let contrasted = mask(&s);
    assert!(contrasted[1] < 64 && contrasted[2] > 192, "{contrasted:?}");
    let mut hard = studio(&[0, 0, 0, 0, 255, 255, 255, 255]);
    assert!(hard.filter_layer_mask(0, &MaskFilter::SoftenSmall.filter()).unwrap());
    let soft = mask(&hard);
    assert!(soft[3] > 0 && soft[4] < 255 && soft.windows(2).all(|p| p[0] <= p[1]), "{soft:?}");
    hard.undo();
    hard.redo();
    assert_eq!(mask(&hard), soft);
    let json = crate::project::encode(&hard.doc).unwrap();
    let reopened = crate::project::decode(&json).unwrap();
    assert_eq!(crate::project::encode(&reopened).unwrap(), json);
}

#[test]
fn craft_mask_filter_respects_the_pixel_selection() {
    let mut s = studio(&[0, 64, 192, 255]);
    s.pixel_sel = Some(crate::deep_color::storage::adopt(vec![1., 1., 0., 0.]).unwrap());
    assert!(s.filter_layer_mask(0, &MaskFilter::Harden.filter()).unwrap());
    assert_eq!(mask(&s), [0, 0, 192, 255]);
}

#[test]
fn craft_mask_filter_refuses_missing_locked_hidden_busy_and_no_change() {
    let mut s = studio(&[0, 255]);
    let before = crate::project::encode(&s.doc).unwrap();
    assert!(!s.filter_layer_mask(0, &MaskFilter::Harden.filter()).unwrap(), "already hard");
    assert!(s.filter_layer_mask(5, &MaskFilter::Harden.filter()).is_err());
    s.doc.layers[0].locked = true;
    assert!(s.filter_layer_mask(0, &MaskFilter::SoftenSmall.filter()).is_err());
    s.doc.layers[0].locked = false;
    s.doc.layers[0].visible = false;
    assert!(s.filter_layer_mask(0, &MaskFilter::SoftenSmall.filter()).is_err());
    s.doc.layers[0].visible = true;
    s.updates.freezing = true;
    assert!(s.filter_layer_mask(0, &MaskFilter::SoftenSmall.filter()).is_err());
    s.updates.freezing = false;
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    assert_eq!(s.history.len(), 0);
    s.doc.layers[0].mask = None;
    assert!(s.filter_layer_mask(0, &MaskFilter::SoftenSmall.filter()).is_err());
}
