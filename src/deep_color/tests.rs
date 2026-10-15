use super::*;
use crate::{
    document::{Cmd, Document, Layer},
    geom::Pt,
};

#[test]
fn native_samples_survive_project_masks_filters_undo_and_psd() {
    for depth in [Depth::U16, Depth::F32] {
        let values = if depth == Depth::U16 {
            vec![1001. / 65535., 1002. / 65535., 0.7, 1.]
        } else {
            vec![1.25, 0.1234567, -0.125, 1.]
        };
        let pixels = Pixels::from_samples(1, 1, depth, values.clone()).unwrap();
        let mut doc = Document::new("Native", 1., 1., 72.);
        doc.transparent = true;
        doc.layers = vec![Layer::placed_raster(
            "Native",
            pixels.clone(),
            Pt::ZERO,
            Pt::new(1., 1.),
        )];
        let saved = crate::project::encode(&doc).unwrap();
        let mut loaded = crate::project::decode(&saved).unwrap();
        assert_eq!(loaded.layers[0].kind.pixels().unwrap().samples(), values);
        let mut settings = crate::raster::Settings::new(crate::raster::Kind::Exposure);
        settings.values[0] = 0.5;
        let edited = pixels.filtered(&settings, None).unwrap();
        assert!((edited.sample(0)[0] - values[0] * 2f32.sqrt()).abs() < 1e-6);
        let cmd = Cmd::NativePixels {
            layer: 0,
            mask: false,
            before: pixels.clone(),
            after: edited,
        };
        crate::document::apply(&mut loaded, &cmd);
        crate::document::apply(&mut loaded, &crate::document::invert_cmd(cmd));
        assert_eq!(loaded.layers[0].kind.pixels().unwrap().samples(), values);
        for large in [false, true] {
            let out = crate::formats::psd::encode(&doc, large).unwrap();
            assert_eq!(
                u16::from_be_bytes(out.bytes[22..24].try_into().unwrap()),
                depth.bits()
            );
            let imported = crate::formats::psd::decode(&out.bytes, "Native").unwrap();
            let got = imported.document.layers[0].kind.pixels().unwrap().sample(0);
            for c in 0..4 {
                assert!(
                    (got[c] - values[c]).abs() < 1. / 65535. + 1e-7,
                    "{got:?} != {values:?}"
                );
            }
        }
        let mask =
            Pixels::from_samples(1, 1, Depth::U16, vec![0.50001, 0.50001, 0.50001, 1.]).unwrap();
        doc.layers[0].mask = Some(mask.clone());
        let out = composite(&doc, 1).unwrap();
        assert!((out.sample(0)[3] - 0.50001).abs() < 1e-6);
        let masked = pixels.masked(&mask).unwrap();
        assert!((masked.sample(0)[3] - 0.50001).abs() < 1e-6);
    }
}

#[test]
fn float_blending_retains_sub_byte_values_and_hdr() {
    let b = [0.123456, 0.4, 0.8, 1.];
    let s = [0.333333, 0.7, 1.5, 0.50001];
    let out = blend_pixel(b, s, crate::color::Blend::Normal, 1.);
    assert!((out[0] - (b[0] * (1. - s[3]) + s[0] * s[3])).abs() < 1e-7);
    assert!(blend_pixel([0.; 4], [2., -0.2, 1., 1.], crate::color::Blend::Normal, 1.)[0] > 1.);
}

#[test]
fn independent_raw_depth_fixtures_keep_adjacent_samples_and_hdr() {
    for (bytes, depth, expected) in [
        (
            include_bytes!("../formats/psd/fixtures/independent-16.psd").as_slice(),
            Depth::U16,
            [1000. / 65535., 32767. / 65535., 60000. / 65535., 1.],
        ),
        (
            include_bytes!("../formats/psd/fixtures/independent-32.psd").as_slice(),
            Depth::F32,
            [1.25, 0.1234567, -0.125, 1.],
        ),
    ] {
        let doc = crate::formats::psd::decode(bytes, "Independent")
            .unwrap()
            .document;
        let p = doc.layers[0].kind.pixels().unwrap();
        assert_eq!(p.native.as_ref().unwrap().depth, depth);
        assert_eq!(p.sample(0), expected);
        assert_ne!(p.sample(0)[0], p.sample(1)[0]);
        let out = crate::formats::psd::encode(&doc, false).unwrap();
        if let Ok(dir) = std::env::var("OMADESIGN_DEEP_QA_DIR") {
            std::fs::write(
                std::path::Path::new(&dir).join(format!("native-{}.psd", depth.bits())),
                out.bytes,
            )
            .unwrap();
        }
    }
}

#[test]
fn native_mask_resize_and_tiny_alpha_keep_authored_values() {
    let pixels =
        Pixels::from_samples(1, 1, Depth::F32, vec![1.25, 0.1234567, -0.125, 0.00001]).unwrap();
    assert!(!pixels.is_invisible());
    let mask = Pixels::from_samples(
        2,
        1,
        Depth::U16,
        vec![
            1001. / 65535.,
            1001. / 65535.,
            1001. / 65535.,
            1.,
            1002. / 65535.,
            1002. / 65535.,
            1002. / 65535.,
            1.,
        ],
    )
    .unwrap();
    let resized = mask.resized(4, 1).unwrap();
    assert!(resized.sample(1)[0] > mask.sample(0)[0]);
    assert!(resized.sample(1)[0] < mask.sample(1)[0]);
    let mut invalid = pixels;
    invalid.native.as_mut().unwrap().samples.clear();
    assert!(invalid.validate_native().is_err());
}

#[test]
fn native_clipping_uses_authoritative_alpha_and_mask_samples() {
    let mut doc = Document::new("Clipping", 1., 1., 72.);
    let mut base = Layer::placed_raster(
        "Base",
        Pixels::from_samples(1, 1, Depth::F32, vec![1., 0., 0., 0.50001]).unwrap(),
        Pt::ZERO,
        Pt::new(1., 1.),
    );
    base.mask = Some(
        Pixels::from_samples(1, 1, Depth::F32, vec![0.333333, 0.333333, 0.333333, 1.]).unwrap(),
    );
    let mut clipped = Layer::placed_raster(
        "Clipped",
        Pixels::from_samples(1, 1, Depth::F32, vec![0., 1., 0., 1.]).unwrap(),
        Pt::ZERO,
        Pt::new(1., 1.),
    );
    clipped.mask =
        Some(Pixels::from_samples(1, 1, Depth::F32, vec![0.70001, 0.70001, 0.70001, 1.]).unwrap());
    doc.layers = vec![base, clipped];
    crate::formats::psd::test_native_clipping(&mut doc);
    assert!(
        (doc.layers[1].mask.as_ref().unwrap().sample(0)[0] - 0.50001 * 0.333333 * 0.70001).abs()
            < 1e-7
    );
}
