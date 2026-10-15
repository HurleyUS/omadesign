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
fn cmyk_profile_authoring_survives_masks_edits_projects_and_psd() {
    use super::cms::{DocumentColor, Model};
    let profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    let inks = vec![
        1001. / 65535.,
        0.25,
        0.5,
        0.125,
        1.,
        1002. / 65535.,
        0.5,
        0.1,
        0.7,
        1.,
    ];
    let pixels = Pixels::from_cmyk(2, 1, Depth::U16, inks.clone(), profile.clone(), 1).unwrap();
    let device = Pixels::from_cmyk(2, 1, Depth::U16, inks.clone(), Vec::new(), 1).unwrap();
    assert_ne!(pixels.data, device.data);
    let mut doc = Document::new("CMYK", 2., 1., 72.);
    doc.transparent = true;
    doc.color = DocumentColor {
        model: Model::Cmyk,
        depth: Depth::U16,
        icc_profile: profile.clone(),
        intent: 1,
    };
    doc.layers = vec![Layer::placed_raster(
        "Ink",
        pixels.clone(),
        Pt::ZERO,
        Pt::new(2., 1.),
    )];
    let doc = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(doc.color.icc_profile, profile);
    assert_eq!(
        doc.layers[0]
            .kind
            .pixels()
            .unwrap()
            .native
            .as_ref()
            .unwrap()
            .cmyk
            .as_ref()
            .unwrap(),
        &inks
    );
    let composite = super::composite(&doc, 1).unwrap();
    for (&a, &b) in composite
        .native
        .as_ref()
        .unwrap()
        .cmyk
        .as_ref()
        .unwrap()
        .iter()
        .zip(&inks)
    {
        assert!((a - b).abs() < 1e-6);
    }
    let mask = Pixels::from_samples(
        2,
        1,
        Depth::U16,
        vec![0.333333, 0.333333, 0.333333, 1., 1., 1., 1., 1.],
    )
    .unwrap();
    let masked = pixels.masked(&mask).unwrap();
    let got = masked.native.as_ref().unwrap().cmyk.as_ref().unwrap();
    assert_eq!(&got[..4], &inks[..4]);
    assert!((got[4] - 0.333333).abs() < 1e-6);
    let mut exposure = crate::raster::Settings::new(crate::raster::Kind::Exposure);
    exposure.values[0] = 0.2;
    let edited = pixels.filtered(&exposure, Some(&[255, 0])).unwrap();
    let got = edited.native.as_ref().unwrap().cmyk.as_ref().unwrap();
    assert_ne!(&got[..4], &inks[..4]);
    assert_eq!(&got[5..], &inks[5..]);
    for large in [false, true] {
        let out = crate::formats::psd::encode(&doc, large).unwrap();
        assert_eq!(out.bytes[25], 4);
        let reopened = crate::formats::psd::decode(&out.bytes, "Ink")
            .unwrap()
            .document;
        assert_eq!(reopened.color.icc_profile, profile);
        let got = reopened.layers[0]
            .kind
            .pixels()
            .unwrap()
            .native
            .as_ref()
            .unwrap()
            .cmyk
            .as_ref()
            .unwrap();
        for (&a, &b) in got.iter().zip(&inks) {
            assert!((a - b).abs() < 1. / 65535. + 1e-7);
        }
    }
}

#[test]
fn invalid_and_mismatched_icc_profiles_are_rejected() {
    let rgb = include_bytes!("../formats/psd/fixtures/srgb-cc0.icc");
    assert!(cms::validate_profile(rgb, cms::Model::Cmyk).is_err());
    assert!(cms::transform(&[0.1, 0.2, 0.3], cms::Model::Rgb, b"not ICC", true, 1).is_err());
    let values = [0.1234567, 0.5, 0.9];
    let converted = cms::transform(&values, cms::Model::Rgb, rgb, true, 1).unwrap();
    for (a, b) in values.iter().zip(converted) {
        assert!((a - b).abs() < 0.0002);
    }
}

#[test]
fn cmyk_tiff_keeps_ink_alpha_profile_and_depth() {
    let profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    for depth in [Depth::U8, Depth::U16, Depth::F32] {
        let inks = vec![0.1234567, 0.25, 0.5, 0.7, 0.333333];
        let pixels = Pixels::from_cmyk(1, 1, depth, inks.clone(), profile.clone(), 1).unwrap();
        let bytes = super::tiff::encode(&pixels).unwrap();
        let got = super::tiff::decode(&bytes).unwrap().unwrap();
        let native = got.native.unwrap();
        assert_eq!(native.depth, depth);
        assert_eq!(native.icc_profile, profile);
        let tolerance = match depth {
            Depth::U8 => 1. / 255.,
            Depth::U16 => 1. / 65535.,
            Depth::F32 => 1e-7,
        };
        for (&a, &b) in native.cmyk.unwrap().iter().zip(&inks) {
            assert!((a - b).abs() < tolerance);
        }
    }
}
