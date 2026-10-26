use super::*;
use crate::{deep_color::Depth, resources};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn content_crop_uses_float_alpha_and_preserves_exact_native_display_profile_and_ink_bits() {
    let mut pixels = Pixels::from_samples(7, 5, Depth::F32, vec![0.; 7 * 5 * 4]).unwrap();
    let mut samples = pixels
        .copy_samples(&crate::deep_color::storage::check)
        .unwrap();
    for (x, y, value) in [
        (2, 1, [1.25, -0.0, -0.125, 0.000001]),
        (4, 3, [-0.25, 2., 0.375, 0.75]),
    ] {
        samples.as_mut_slice()[(y * 7 + x) * 4..(y * 7 + x + 1) * 4].copy_from_slice(&value);
    }
    pixels.set_sample_allocation(Arc::new(samples)).unwrap();
    pixels
        .assign_rgb_profile(
            include_bytes!("../formats/psd/fixtures/srgb-cc0.icc").to_vec(),
            0,
        )
        .unwrap();
    assert_eq!(
        pixels.data[(7 + 2) * 4 + 3],
        0,
        "Real float coverage is below byte display precision"
    );
    let original = pixels.clone();
    let (cropped, origin) =
        content_bounds(pixels, Pt::new(-8., 12.), &crate::ml::NoProgress).unwrap();
    assert_eq!(origin, Pt::new(-6., 13.));
    assert_eq!((cropped.w, cropped.h), (3, 3));
    let native = cropped.native.as_ref().unwrap();
    let before = original.native.as_ref().unwrap();
    assert!(Arc::ptr_eq(&native.icc_profile, &before.icc_profile));
    assert_eq!(native.intent, 0);
    assert_eq!(native.depth, Depth::F32);
    for y in 0..3 {
        for x in 0..3 {
            let i = (y * 3 + x) * 4;
            let j = ((y + 1) * 7 + x + 2) * 4;
            assert_eq!(&cropped.data[i..i + 4], &original.data[j..j + 4]);
            for c in 0..4 {
                assert_eq!(
                    native.samples[i + c].to_bits(),
                    before.samples[j + c].to_bits()
                );
            }
        }
    }
    assert_eq!(native.samples[1].to_bits(), (-0.0f32).to_bits());
    let mut inks = vec![0.; 7 * 5 * 5];
    for (x, y) in [(2, 1), (4, 3)] {
        inks[(y * 7 + x) * 5..(y * 7 + x + 1) * 5]
            .copy_from_slice(&[0.125, 0.25, 0.375, 0.5, 0.000001]);
    }
    let pixels = Pixels::from_cmyk(7, 5, Depth::U16, inks, vec![], 3).unwrap();
    let original = pixels.clone();
    let (cropped, _) = content_bounds(pixels, Pt::ZERO, &crate::ml::NoProgress).unwrap();
    assert_eq!((cropped.w, cropped.h), (3, 3));
    let native = cropped.native.as_ref().unwrap();
    assert_eq!(native.depth, Depth::U16);
    assert_eq!(native.intent, 3);
    for y in 0..3 {
        for x in 0..3 {
            let i = (y * 3 + x) * 5;
            let j = ((y + 1) * 7 + x + 2) * 5;
            for c in 0..5 {
                assert_eq!(
                    native.cmyk.as_ref().unwrap()[i + c].to_bits(),
                    original.native.as_ref().unwrap().cmyk.as_ref().unwrap()[j + c].to_bits()
                );
            }
        }
    }
    assert_eq!(cropped.sample(0)[3].to_bits(), 0.000001f32.to_bits());
}

#[test]
fn content_crop_retains_full_storage_and_canonicalizes_empty_images_without_losing_metadata() {
    let pixels = Pixels::from_rgba(3, 2, [30, 60, 90, 255].repeat(6)).unwrap();
    let original = pixels.clone();
    let (output, origin) =
        content_bounds(pixels, Pt::new(-2., 3.), &crate::ml::NoProgress).unwrap();
    assert!(Arc::ptr_eq(&output.data, &original.data));
    assert_eq!(origin, Pt::new(-2., 3.));
    let mut pixels = Pixels::from_samples(3, 2, Depth::F32, vec![0.; 24]).unwrap();
    pixels
        .assign_rgb_profile(
            include_bytes!("../formats/psd/fixtures/srgb-cc0.icc").to_vec(),
            0,
        )
        .unwrap();
    let profile = pixels.native.as_ref().unwrap().icc_profile.clone();
    let (empty, origin) =
        content_bounds(pixels, Pt::new(-4., -5.), &crate::ml::NoProgress).unwrap();
    assert_eq!((empty.w, empty.h), (1, 1));
    assert_eq!(origin, Pt::ZERO);
    assert_eq!(empty.sample(0), [0.; 4]);
    assert_eq!(empty.data.as_slice(), [0; 4]);
    let native = empty.native.as_ref().unwrap();
    assert!(Arc::ptr_eq(&native.icc_profile, &profile));
    assert_eq!(native.intent, 0);
    let document = Document {
        layers: vec![Layer::placed_raster(
            "Empty",
            empty,
            Pt::ZERO,
            Pt::splat(1.),
        )],
        ..Document::new("Empty", 3., 2., 96.)
    };
    let decoded = crate::project::decode(&crate::project::encode(&document).unwrap()).unwrap();
    assert_eq!(decoded.layers[0].kind.pixels().unwrap().sample(0), [0.; 4]);
}

#[test]
fn content_crop_cancel_during_scan_and_allocation_releases_scratch_and_keeps_sources() {
    let _pool = crate::resources::isolated();
    let baseline = resources::global().usage().0;
    let pending = resources::global().pending_bytes();
    let pixels = Pixels::from_samples(32, 32, Depth::F32, vec![0.; 32 * 32 * 4]).unwrap();
    let original = pixels.clone();
    let used = resources::global().usage().0;
    struct Stop(AtomicUsize);
    impl crate::ml::Progress for Stop {
        fn report(&self, _: &str, _: usize, _: usize) {}
        fn cancelled(&self) -> bool {
            self.0.fetch_add(1, Ordering::SeqCst) > 4
        }
    }
    assert!(
        content_bounds(pixels, Pt::ZERO, &Stop(AtomicUsize::new(0)))
            .unwrap_err()
            .contains("Cancelled")
    );
    assert_eq!(resources::global().usage().0, used);
    assert_eq!(original.sample(0), [0.; 4]);
    let pool = resources::global();
    struct StopCopy {
        baseline: u64,
        peak: AtomicUsize,
    }
    impl crate::ml::Progress for StopCopy {
        fn report(&self, _: &str, _: usize, _: usize) {}
        fn cancelled(&self) -> bool {
            let used = resources::global().usage().0;
            self.peak.fetch_max(used as usize, Ordering::SeqCst);
            used > self.baseline
        }
    }
    let stop = StopCopy {
        baseline: used,
        peak: AtomicUsize::new(used as usize),
    };
    let before = original.native.as_ref().unwrap().samples.clone();
    assert!(
        content_bounds(original.clone(), Pt::ZERO, &stop)
            .unwrap_err()
            .contains("Cancelled")
    );
    assert!(Arc::ptr_eq(
        &before,
        &original.native.as_ref().unwrap().samples
    ));
    assert!(stop.peak.load(Ordering::SeqCst) > used as usize);
    assert_eq!(pool.usage().0, used);
    drop(before);
    drop(original);
    assert_eq!(pool.usage().0, baseline);
    assert_eq!(pool.pending_bytes(), pending);
}
