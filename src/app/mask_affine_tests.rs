use super::*;
use crate::{channels::Plane, deep_color::storage};
use std::sync::Arc;

fn studio() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.doc = crate::channels::tests::fixture();
    s.active_layer = Some(0);
    s
}
fn world(t: Transform, x: f32, y: f32) -> (f32, f32) {
    let mut p = [tiny_skia::Point::from_xy(x, y)];
    t.map_points(&mut p);
    ((p[0].x * 1000.).round() / 1000., (p[0].y * 1000.).round() / 1000.)
}
fn saved(s: &mut Studio) -> u64 {
    let transform = Transform::from_translate(3., 1.);
    let plane = Plane::from_mask(&[0, 64, 128, 255], 4, 1, transform, &storage::check).unwrap();
    let target = s.capture_channels().unwrap();
    let id = s.save_channel(&target, plane, "Edge").unwrap();
    s.history.clear();
    id
}

#[test]
fn craft_mask_affine_pivot_is_the_covered_centre() {
    // Covered pixels 1..4 of a 4x1 grid at x=3: centre is world x = 3 + 2.5.
    let flipped = about_bounds(Transform::from_translate(3., 1.), 4, 1, Some((1, 0, 4, 1)), MaskAffine::FlipHorizontal);
    assert_eq!(world(flipped, 2.5, 0.5), (5.5, 1.5));
    assert_eq!(world(flipped, 1., 0.), (7., 1.));
    let turned = about_bounds(Transform::identity(), 4, 2, None, MaskAffine::RotateClockwise);
    assert_eq!(world(turned, 2., 1.), (2., 1.));
    assert_eq!(world(turned, 4., 1.), (2., 3.));
    for edit in MaskAffine::ALL {
        let once = about_bounds(Transform::identity(), 4, 2, None, edit);
        let back = match edit {
            MaskAffine::RotateClockwise => MaskAffine::RotateCounterclockwise,
            MaskAffine::RotateCounterclockwise => MaskAffine::RotateClockwise,
            other => other,
        };
        let restored = about_bounds(once, 4, 2, None, back);
        assert_eq!(world(restored, 1., 1.), (1., 1.), "{edit:?}");
        assert_eq!(world(restored, 3., 0.), (3., 0.), "{edit:?}");
    }
}

#[test]
fn craft_mask_affine_channel_edit_shares_samples_in_one_undo_and_reopens() {
    let mut s = studio();
    let id = saved(&mut s);
    let before = crate::project::encode(&s.doc).unwrap();
    let samples = s.doc.channels[0].plane.samples.clone();
    for edit in MaskAffine::ALL {
        let target = s.capture_channels().unwrap();
        assert!(s.transform_channel(&target, id, edit).unwrap(), "{edit:?}");
        assert!(Arc::ptr_eq(&s.doc.channels[0].plane.samples, &samples), "samples are never resampled");
    }
    assert_eq!(s.history.len(), MaskAffine::ALL.len());
    let after = crate::project::encode(&s.doc).unwrap();
    assert_eq!(crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(), after);
    for _ in MaskAffine::ALL {
        s.undo();
    }
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    for _ in MaskAffine::ALL {
        s.redo();
    }
    assert_eq!(crate::project::encode(&s.doc).unwrap(), after);
}

#[test]
fn craft_mask_affine_channel_refuses_missing_empty_busy_and_live() {
    let mut s = studio();
    let id = saved(&mut s);
    let target = s.capture_channels().unwrap();
    assert!(s.transform_channel(&target, id + 999, MaskAffine::FlipVertical).is_err());
    let empty = Plane::from_mask(&[0, 0], 2, 1, Transform::identity(), &storage::check).unwrap();
    let blank = s.save_channel(&target, empty, "Empty").unwrap();
    let target = s.capture_channels().unwrap();
    let before = crate::project::encode(&s.doc).unwrap();
    let history = s.history.len();
    assert!(s.transform_channel(&target, blank, MaskAffine::FlipVertical).is_err());
    s.updates.freezing = true;
    assert!(s.capture_channels().is_err());
    assert!(s.transform_channel(&target, id, MaskAffine::FlipVertical).is_err(), "stale owner refuses");
    assert!(!s.transform_pixel_selection(MaskAffine::FlipVertical));
    s.updates.freezing = false;
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    assert_eq!(s.history.len(), history);
}

#[test]
fn craft_mask_affine_selection_keeps_soft_coverage_and_records_no_history() {
    let mut s = studio();
    let space = SelectionSpace { w: 2, h: 1, transform: Transform::from_translate(2., 1.) };
    s.replace_pixel_selection(vec![64, 192], space);
    let values = s.pixel_sel.clone().unwrap();
    let document = crate::project::encode(&s.doc).unwrap();
    assert!(s.transform_pixel_selection(MaskAffine::FlipHorizontal));
    assert!(Arc::ptr_eq(s.pixel_sel.as_ref().unwrap(), &values));
    let flipped = s.pixel_selection_space().unwrap().transform;
    assert_eq!(world(flipped, 0.5, 0.5), (3.5, 1.5));
    assert_eq!(world(flipped, 1.5, 0.5), (2.5, 1.5));
    assert!(s.transform_pixel_selection(MaskAffine::FlipHorizontal));
    assert_eq!(world(s.pixel_selection_space().unwrap().transform, 0.5, 0.5), (2.5, 1.5));
    assert_eq!(s.history.len(), 0);
    assert_eq!(crate::project::encode(&s.doc).unwrap(), document);
    s.set_pixel_sel(None);
    assert!(!s.transform_pixel_selection(MaskAffine::Rotate180));
}
