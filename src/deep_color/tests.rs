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

#[test]
fn authoritative_sample_validation_rejects_malformed_project_inks() {
    let pixels = Pixels::from_cmyk(
        1,
        1,
        Depth::U16,
        vec![0.1, 0.2, 0.3, 0.4, 1.],
        Vec::new(),
        1,
    )
    .unwrap();
    let mut doc = Document::new("Malformed", 1., 1., 72.);
    doc.layers = vec![Layer::placed_raster(
        "Ink",
        pixels,
        Pt::ZERO,
        Pt::new(1., 1.),
    )];
    let mut value: serde_json::Value =
        serde_json::from_str(&crate::project::encode(&doc).unwrap()).unwrap();
    value["doc"]["layers"][0]["kind"]["Raster"]["pixels"]["native"]["cmyk"] = serde_json::json!([]);
    assert!(crate::project::decode(&value.to_string()).is_err());
    assert_eq!(cms::runtime_probe().unwrap(), 2190);
}

#[test]
fn native_brush_growth_and_group_mask_resize_preserve_sub_byte_values() {
    let pixels = Pixels::from_cmyk(
        1,
        1,
        Depth::U16,
        vec![0.123456, 0.2, 0.3, 0.4, 0.5],
        Vec::new(),
        1,
    )
    .unwrap();
    let grown = pixels.reframed(3, 3, 1, 1).unwrap();
    assert_eq!(
        grown.native.as_ref().unwrap().cmyk.as_ref().unwrap()[20..25],
        [0.123456, 0.2, 0.3, 0.4, 0.5]
    );
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

#[test]
fn raster_export_preserves_native_color_and_float_mask_precision() {
    use crate::export::{self, Format, Settings, Source};
    let mut doc = Document::new("Export", 2., 1., 72.);
    doc.transparent = true;
    doc.layers = vec![Layer::placed_raster(
        "Native",
        Pixels::from_samples(
            2,
            1,
            Depth::U16,
            vec![1000. / 65535., 0.25, 0.5, 1., 1001. / 65535., 0.5, 0.75, 1.],
        )
        .unwrap(),
        Pt::ZERO,
        Pt::new(2., 1.),
    )];
    for format in [Format::Png, Format::Tiff] {
        let (bytes, _) = export::encode(
            &Source::Document(doc.clone()),
            Settings {
                format,
                scale: 1,
                ai: None,
            },
            &crate::ml::NoProgress,
        )
        .unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!(decoded.color(), image::ColorType::Rgba16);
        let pixels = decoded.to_rgba16();
        assert_eq!(pixels.get_pixel(0, 0)[0], 1000);
        assert_eq!(pixels.get_pixel(1, 0)[0], 1001);
        if format == Format::Png
            && let Ok(dir) = std::env::var("OMADESIGN_DEEP_QA_DIR")
        {
            std::fs::write(std::path::Path::new(&dir).join("native-16.png"), bytes).unwrap();
        }
    }
    doc.layers[0].mask = Some(
        Pixels::from_samples(
            2,
            1,
            Depth::F32,
            vec![
                0.333333, 0.333333, 0.333333, 1., 0.777777, 0.777777, 0.777777, 1.,
            ],
        )
        .unwrap(),
    );
    let (bytes, _) = export::encode(
        &Source::Document(doc.clone()),
        Settings {
            format: Format::Tiff,
            scale: 1,
            ai: None,
        },
        &crate::ml::NoProgress,
    )
    .unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!(decoded.color(), image::ColorType::Rgba32F);
    assert!((decoded.to_rgba32f().get_pixel(0, 0)[3] - 0.333333).abs() < 1e-7);
    if let Ok(dir) = std::env::var("OMADESIGN_DEEP_QA_DIR") {
        std::fs::write(
            std::path::Path::new(&dir).join("native-float-mask.tif"),
            bytes,
        )
        .unwrap();
    }
    assert!(crate::compositor::export_png(&doc, 1).is_err());
    assert!(crate::svg::export(&doc).is_err());
    assert!(crate::formats::pdf::write(&doc).is_err());
    assert!(crate::formats::openraster::write(&doc).is_err());
}

#[test]
fn native_mask_on_vector_layer_keeps_float_coverage() {
    let mut doc = Document::new("Vector mask", 2., 2., 72.);
    doc.transparent = true;
    let mut layer = Layer::vector("Vector");
    layer
        .kind
        .shapes_mut()
        .unwrap()
        .push(crate::document::Shape::new(
            crate::geom::Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(2., 2.),
                radius: 0.,
            },
            crate::document::Style::default(),
        ));
    layer.mask = Some(
        Pixels::from_samples(2, 2, Depth::F32, [0.50001, 0.50001, 0.50001, 1.].repeat(4)).unwrap(),
    );
    doc.layers = vec![layer];
    let out = composite(&doc, 1).unwrap();
    assert!((out.sample(0)[3] - 0.50001).abs() < 1e-7);
}

#[test]
fn profile_aware_export_and_document_conversion_preserve_real_authoring_data() {
    use crate::export::{self, Format, Settings, Source};
    let profile = include_bytes!("../formats/psd/fixtures/srgb-cc0.icc").to_vec();
    let mut pixels = Pixels::from_samples(
        2,
        1,
        Depth::U16,
        vec![1000. / 65535., 0.25, 0.5, 1., 1001. / 65535., 0.5, 0.75, 1.],
    )
    .unwrap();
    pixels.assign_rgb_profile(profile.clone(), 1).unwrap();
    let mut doc = Document::new("Tagged export", 2., 1., 72.);
    doc.transparent = true;
    doc.color = cms::DocumentColor {
        depth: Depth::U16,
        icc_profile: profile.clone(),
        intent: 1,
        ..Default::default()
    };
    doc.layers = vec![Layer::placed_raster(
        "Tagged",
        pixels,
        Pt::ZERO,
        Pt::new(2., 1.),
    )];
    for format in [Format::Png, Format::Tiff] {
        let (bytes, _) = export::encode(
            &Source::Document(doc.clone()),
            Settings {
                format,
                scale: 1,
                ai: None,
            },
            &crate::ml::NoProgress,
        )
        .unwrap();
        let decoded = crate::formats::decode_image(&bytes).unwrap();
        assert_eq!(decoded.native.as_ref().unwrap().icc_profile, profile);
        assert!((decoded.sample(0)[0] - 1000. / 65535.).abs() < 1e-6);
        if let Ok(dir) = std::env::var("OMADESIGN_DEEP_QA_DIR") {
            std::fs::write(
                std::path::Path::new(&dir).join(format!("profile-16.{}", format.extension())),
                bytes,
            )
            .unwrap();
        }
    }
    let before = doc.layers[0].kind.pixels().unwrap().samples();
    let inks = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    let command = doc
        .color_conversion(cms::DocumentColor {
            model: cms::Model::Cmyk,
            depth: Depth::U16,
            icc_profile: inks.clone(),
            intent: 1,
        })
        .unwrap();
    crate::document::apply(&mut doc, &command);
    let (bytes, _) = export::encode(
        &Source::Document(doc.clone()),
        Settings {
            format: Format::Tiff,
            scale: 1,
            ai: None,
        },
        &crate::ml::NoProgress,
    )
    .unwrap();
    let decoded = crate::formats::decode_image(&bytes).unwrap();
    assert_eq!(decoded.native.as_ref().unwrap().depth, Depth::U16);
    assert_eq!(decoded.native.as_ref().unwrap().icc_profile, inks);
    assert!(decoded.native.as_ref().unwrap().cmyk.is_some());
    if let Ok(dir) = std::env::var("OMADESIGN_DEEP_QA_DIR") {
        std::fs::write(
            std::path::Path::new(&dir).join("profile-cmyk-16.tif"),
            bytes,
        )
        .unwrap();
    }
    doc = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(doc.color.model, cms::Model::Cmyk);
    crate::document::apply(&mut doc, &crate::document::invert_cmd(command));
    assert_eq!(doc.layers[0].kind.pixels().unwrap().samples(), before);
}

#[test]
fn cmyk_native_retouch_and_all_blends_preserve_ink_authoring() {
    let profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    let inks: Vec<f32> = (0..8 * 8)
        .flat_map(|i| [0.1 + i as f32 / 65535., 0.2, 0.3, 0.4, 0.75])
        .collect();
    let p = Pixels::from_cmyk(8, 8, Depth::U16, inks.clone(), profile.clone(), 1).unwrap();
    let brush = crate::paint::Brush {
        size: 4.,
        flow: 1.,
        opacity: 0.5,
        ..Default::default()
    };
    let edited = super::retouch::stroke(
        &p,
        &p,
        Pt::new(4., 4.),
        Pt::new(4., 4.),
        Pt::new(-2., 0.),
        crate::tools::Tool::Clone,
        &brush,
    )
    .unwrap();
    let n = edited.native.as_ref().unwrap();
    assert_eq!(n.depth, Depth::U16);
    assert_eq!(n.icc_profile, profile);
    assert_eq!(&n.cmyk.as_ref().unwrap()[..5], &inks[..5]);
    assert_ne!(edited.samples(), p.samples());
    let mut doc = Document::new("Blend inks", 8., 8., 72.);
    doc.transparent = true;
    doc.color = cms::DocumentColor {
        model: cms::Model::Cmyk,
        depth: Depth::U16,
        icc_profile: profile.clone(),
        intent: 1,
    };
    doc.layers = vec![
        Layer::placed_raster("Ink", p, Pt::ZERO, Pt::new(8., 8.)),
        Layer::placed_raster("Edited", edited, Pt::ZERO, Pt::new(8., 8.)),
    ];
    for blend in [
        crate::color::Blend::Normal,
        crate::color::Blend::Multiply,
        crate::color::Blend::Screen,
        crate::color::Blend::Overlay,
        crate::color::Blend::Darken,
        crate::color::Blend::Lighten,
        crate::color::Blend::ColorDodge,
        crate::color::Blend::ColorBurn,
        crate::color::Blend::HardLight,
        crate::color::Blend::SoftLight,
        crate::color::Blend::Difference,
        crate::color::Blend::Exclusion,
        crate::color::Blend::Hue,
        crate::color::Blend::Saturation,
        crate::color::Blend::Color,
        crate::color::Blend::Luminosity,
    ] {
        doc.layers[1].blend = blend;
        let out = super::composite(&doc, 1).unwrap();
        let native = out.native.unwrap();
        assert_eq!(native.icc_profile, profile);
        assert_eq!(native.cmyk.unwrap().len(), 8 * 8 * 5);
        assert!(native.samples.iter().all(|v| v.is_finite()));
    }
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
}

#[test]
fn cmyk_float_ffi_matches_independent_transicc_reference() {
    let profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc");
    let samples = [
        0., 0., 0., 0., 0.1, 0.2, 0.3, 0.4, 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 1.,
    ];
    let expected = [
        255., 255., 255., 163.3108, 148.8488, 132.5648, -590.2940, 160.5262, 227.7222, 230.1068,
        12.1681, 128.1444, 43.0867, 42.7438, 41.8995,
    ];
    let transformed = cms::transform(&samples, cms::Model::Cmyk, profile, true, 1).unwrap();
    for (a, b) in transformed.iter().zip(expected) {
        assert!((a - b / 255.).abs() < 0.0002, "{a} != {b}/255");
    }
}
