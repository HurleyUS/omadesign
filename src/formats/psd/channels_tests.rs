use crate::channels::{Channel, Plane, spot::Spot};
use crate::document::{Document, Layer};

fn doc() -> Document {
    let mut doc = Document::new("Inks", 4., 2., 72.);
    doc.layers = vec![Layer::raster("Pixels", 4, 2).unwrap()];
    let alpha = Plane::from_mask(&[0, 64, 128, 255, 255, 128, 64, 0], 4, 2, tiny_skia::Transform::identity(), &crate::deep_color::storage::check).unwrap();
    let spot = Plane::from_mask(&[255, 255, 0, 0], 2, 2, tiny_skia::Transform::from_translate(2., 0.), &crate::deep_color::storage::check).unwrap();
    doc.channels = vec![
        Channel { id: crate::document::next_id(), name: "Edge mask".into(), plane: alpha, spot: None },
        Channel { id: crate::document::next_id(), name: "PANTONE 300 C".into(), plane: spot, spot: Some(Spot { ink: [0., 0.37, 0.72], solidity: 0.8 }) },
    ];
    doc
}
fn bytes(c: &Channel) -> Vec<u8> {
    c.plane.samples.iter().map(|v| (v * 255.).round() as u8).collect()
}

#[test]
fn craft_psd_channels_alpha_and_spot_round_trip_with_names_inks_and_placement() {
    let source = doc();
    let out = crate::formats::psd::encode(&source, false).unwrap();
    let imported = crate::formats::psd::decode(&out.bytes, "inks").unwrap();
    let channels = &imported.document.channels;
    assert_eq!(channels.len(), 2, "{:?}", imported.warnings);
    assert_eq!(channels[0].name, "Edge mask");
    assert_eq!(channels[0].spot, None);
    assert_eq!(bytes(&channels[0]), [0, 64, 128, 255, 255, 128, 64, 0]);
    assert_eq!(channels[1].name, "PANTONE 300 C");
    let spot = channels[1].spot.unwrap();
    assert!((spot.solidity - 0.8).abs() < 1e-6);
    assert!(spot.ink.iter().zip([0., 0.37, 0.72]).all(|(a, b)| (a - b).abs() < 1e-4));
    assert_eq!((channels[1].plane.width, channels[1].plane.height), (4, 2), "resampled onto the canvas grid");
    assert_eq!(bytes(&channels[1]), [0, 0, 255, 255, 0, 0, 0, 0], "translated spot coverage lands at x=2..4");
    assert!(imported.warnings.iter().all(|w| !w.contains("Extra merged")), "{:?}", imported.warnings);
    assert!(crate::project::decode(&crate::project::encode(&imported.document).unwrap()).is_ok());
}

#[test]
fn craft_psd_channels_store_spot_ink_inverted_and_16_bit_keeps_precision() {
    let mut source = doc();
    source.color.depth = crate::deep_color::Depth::U16;
    let out = crate::formats::psd::encode(&source, false).unwrap();
    let imported = crate::formats::psd::decode(&out.bytes, "inks").unwrap().document;
    let alpha = &imported.channels[0].plane.samples;
    assert!((alpha[1] - 64. / 255.).abs() < 1. / 65535., "16-bit channel keeps sub-byte precision");
    let names = super::extra_planes(&source, 4, 2, 8).unwrap();
    assert_eq!(names[1][..4], [255, 255, 0, 0], "Photoshop spot data: black (0) is full ink");
}

#[test]
fn craft_psd_channels_damaged_resources_warn_and_extras_without_names_stay_unimported() {
    let mut info = super::Info::default();
    let mut warnings = vec![];
    info.read(1045, &[0, 0, 0, 9, 0, 65], &mut warnings);
    assert_eq!(warnings.len(), 1);
    assert!(info.names.is_empty());
    let plain = Document::new("Plain", 4., 2., 72.);
    let out = crate::formats::psd::encode(&plain, false).unwrap();
    assert!(crate::formats::psd::decode(&out.bytes, "plain").unwrap().document.channels.is_empty());
}
