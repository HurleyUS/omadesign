use super::*;
use crate::channels::Target as T;
use crate::deep_color::storage;

fn plane(v: &[u8], t: tiny_skia::Transform) -> Plane {
    Plane::from_mask(v, v.len() as u32, 1, t, &storage::check).unwrap()
}
fn bytes(p: &Plane) -> Vec<u8> {
    p.samples.iter().map(|v| (v * 255.).round() as u8).collect()
}

#[test]
fn craft_calculations_blend_modes_match_photoshop_formulas() {
    let id = tiny_skia::Transform::identity();
    let a = plane(&[0, 128, 255, 64], id);
    let b = plane(&[255, 128, 0, 192], id);
    let run = |m: Blend| bytes(&calculate(&a, false, &b, false, m, 1.).unwrap());
    assert_eq!(run(Blend::Normal), [0, 128, 255, 64]);
    assert_eq!(run(Blend::Multiply), [0, 64, 0, 48]);
    assert_eq!(run(Blend::Screen), [255, 192, 255, 208]);
    assert_eq!(run(Blend::Add), [255, 255, 255, 255]);
    assert_eq!(run(Blend::Subtract), [255, 0, 0, 128]);
    assert_eq!(run(Blend::Difference), [255, 0, 255, 128]);
    assert_eq!(run(Blend::Darken), [0, 128, 0, 64]);
    assert_eq!(run(Blend::Lighten), [255, 128, 255, 192]);
    assert_eq!(bytes(&calculate(&a, true, &b, false, Blend::Normal, 0.5).unwrap()), [255, 128, 0, 192], "inverted source 1 at 50%");
    assert!(calculate(&a, false, &b, false, Blend::Normal, 1.5).is_err());
    let shifted = plane(&[255, 255], tiny_skia::Transform::from_translate(2., 0.));
    assert_eq!(bytes(&calculate(&shifted, false, &b, false, Blend::Lighten, 1.).unwrap()), [255, 128, 255, 255], "source 1 resampled into source 2's grid");
}

fn studio() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.doc = crate::channels::tests::fixture();
    s.active_layer = Some(0);
    s.history.clear();
    s
}

#[test]
fn craft_calculations_new_channel_is_one_undo_and_selection_is_not_history() {
    let mut s = studio();
    let target = s.capture_channels().unwrap();
    let w = s.doc.width as u32 * s.doc.height as u32;
    let a = s.save_channel(&target, Plane::from_mask(&vec![200; w as usize], s.doc.width as u32, s.doc.height as u32, tiny_skia::Transform::identity(), &storage::check).unwrap(), "A").unwrap();
    s.history.clear();
    let before = crate::project::encode(&s.doc).unwrap();
    let spec = Spec { source1: T::Saved(a), invert1: true, source2: T::Saved(a), invert2: false, blend: Blend::Multiply, opacity: 1. };
    let target = s.capture_channels().unwrap();
    let id = s.calculate_channels(&target, spec, Output::NewChannel).unwrap().unwrap();
    assert_eq!(s.history.len(), 1);
    let made = s.doc.channels.iter().find(|c| c.id == id).unwrap();
    let want = ((55. / 255.) * (200. / 255.) * 255.0f32).round() as u8;
    assert!(bytes(&made.plane).iter().all(|v| v.abs_diff(want) <= 1), "{want}");
    s.undo();
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    let target = s.capture_channels().unwrap();
    assert_eq!(s.calculate_channels(&target, Spec { blend: Blend::Normal, ..spec }, Output::Selection).unwrap(), None);
    assert_eq!(s.history.len(), 0, "selection output records no history");
    assert!(s.pixel_sel.as_ref().unwrap().iter().all(|v| (v - 55. / 255.).abs() < 1e-4));
    let target = s.capture_channels().unwrap();
    s.set_pixel_sel(None);
    assert!(s.calculate_channels(&target, spec, Output::NewChannel).is_err(), "stale owner refuses");
    let target = s.capture_channels().unwrap();
    assert!(s.calculate_channels(&target, Spec { source1: T::Saved(999_999), ..spec }, Output::NewChannel).is_err());
    assert!(s.calculate_channels(&target, Spec { source1: T::Composite, source2: T::Transparency, ..spec }, Output::NewChannel).unwrap().is_some(), "composite gray and transparency are valid sources");
}
