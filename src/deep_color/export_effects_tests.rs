//! ICC-tagged placed photos with effects must export without changing the document.
use super::{Depth, cms, has_native};
use crate::color::{Blend, Rgba};
use crate::document::{Document, Layer, Pixels};
use crate::export::{self, Format, Settings, Source};
use crate::geom::Pt;

fn tagged_photo(depth: Depth) -> Pixels {
    let srgb = include_bytes!("../formats/psd/fixtures/srgb-cc0.icc").to_vec();
    let mut pixels = Pixels::from_samples(4, 4, depth, [0.8, 0.4, 0.2, 1.].repeat(16)).unwrap();
    pixels.assign_rgb_profile(srgb, 1).unwrap();
    pixels
}

fn shadowed(pixels: Pixels) -> Layer {
    let mut layer = Layer::placed_raster("Photo", pixels, Pt::ZERO, Pt::new(4., 4.));
    layer.filters.items.push(crate::filter::Fx::Shadow {
        dx: 2.,
        dy: 2.,
        blur: 0.,
        spread: 0.,
        color: Rgba::rgb(0, 0, 0),
        knockout: false,
        blend: Blend::Normal,
        opacity: 1.,
    });
    layer
}

fn doc_with(layer: Layer, color: cms::DocumentColor) -> Document {
    let mut doc = Document::new("Preorder", 8., 8., 72.);
    doc.transparent = true;
    doc.color = color;
    doc.layers = vec![layer];
    doc
}

#[test]
fn tagged_photo_with_shadow_exports_every_format_without_mutation() {
    let doc = doc_with(shadowed(tagged_photo(Depth::U8)), cms::DocumentColor::default());
    assert!(has_native(&doc));
    let before = crate::project::encode(&doc).unwrap();
    let source = Source::Document(doc.clone());
    for format in Format::ALL {
        let settings = Settings { format, ..Default::default() };
        let (bytes, _) = export::encode(&source, settings, &crate::ml::NoProgress)
            .unwrap_or_else(|e| panic!("{format:?}: {e}"));
        assert!(!bytes.is_empty(), "{format:?}");
    }
    let png = export::encode(&source, Settings::default(), &crate::ml::NoProgress).unwrap().0;
    let png = image::load_from_memory(&png).unwrap().into_rgba8();
    assert!(png.get_pixel(5, 5)[3] > 200, "shadow is rendered");
    let dir = std::env::temp_dir().join(format!("oma-preorder-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for extension in ["png", "jpg", "pdf", "svg", "ora", "idml", "tif", "psd"] {
        let path = dir.join(format!("preorder.{extension}"));
        crate::formats::cli::export(&doc, &path).unwrap_or_else(|e| panic!("{extension}: {e}"));
        assert!(std::fs::metadata(&path).unwrap().len() > 0, "{extension}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), before);
    assert!(doc.layers[0].kind.pixels().unwrap().native.is_some());
}

#[test]
fn deep_photo_effects_render_in_float_export() {
    let color = cms::DocumentColor { depth: Depth::U16, ..Default::default() };
    let doc = doc_with(shadowed(tagged_photo(Depth::U16)), color);
    let before = crate::project::encode(&doc).unwrap();
    let pixels = super::composite(&doc, 1).unwrap();
    assert!(pixels.sample(5 * 8 + 5)[3] > 0.8, "shadow is rendered");
    assert!((pixels.sample(0)[0] - 0.8).abs() < 0.01);
    for format in [Format::Png, Format::Tiff] {
        let settings = Settings { format, ..Default::default() };
        export::encode(&Source::Document(doc.clone()), settings, &crate::ml::NoProgress)
            .unwrap_or_else(|e| panic!("{format:?}: {e}"));
    }
    assert_eq!(crate::project::encode(&doc).unwrap(), before);
}

#[test]
fn placed_tagged_photo_converts_to_destination_color() {
    let place = |studio: &mut crate::app::Studio, pixels: Pixels| {
        let mut asset = Document::new("Photo", 4., 4., 72.);
        asset.layers = vec![Layer::placed_raster("Photo", pixels, Pt::ZERO, Pt::new(4., 4.))];
        studio
            .place_imported_at(crate::import::Imported::Document(asset), Pt::new(4., 4.), None)
            .unwrap();
        studio.doc.layers.iter().rev().find_map(|l| l.kind.pixels()).unwrap().clone()
    };
    let mut studio = crate::app::Studio::new();
    assert!(place(&mut studio, tagged_photo(Depth::U8)).native.is_none());
    assert!(!has_native(&studio.doc));
    let deep = place(&mut studio, tagged_photo(Depth::U16));
    let native = deep.native.as_ref().unwrap();
    assert_eq!((native.depth, native.icc_profile.is_empty()), (Depth::U16, true));

    let profile = include_bytes!("../formats/psd/fixtures/srgb-cc0.icc").to_vec();
    studio.doc = Document::new("Deep", 8., 8., 72.);
    studio.doc.color = cms::DocumentColor { depth: Depth::F32, icc_profile: profile.clone(), intent: 0, ..Default::default() };
    let placed = place(&mut studio, tagged_photo(Depth::U16));
    let native = placed.native.as_ref().unwrap();
    assert_eq!((native.depth, native.intent, native.icc_profile.as_slice()), (Depth::F32, 0, profile.as_slice()));
}
