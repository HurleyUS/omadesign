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
fn placed_photos_take_destination_profile_and_depth() {
    let place = |studio: &mut crate::app::Studio, pixels: Pixels| {
        let mut asset = Document::new("Photo", 4., 4., 72.);
        asset.layers = vec![Layer::placed_raster("Photo", pixels, Pt::ZERO, Pt::new(4., 4.))];
        studio
            .place_imported_at(crate::import::Imported::Document(asset), Pt::new(4., 4.), None)
            .unwrap();
        studio.doc.layers.iter().rev().find_map(|l| l.kind.pixels()).unwrap().clone()
    };
    let mut studio = crate::app::Studio::new();
    for depth in [Depth::U8, Depth::U16] {
        assert!(place(&mut studio, tagged_photo(depth)).native.is_none());
    }
    assert!(!has_native(&studio.doc));

    let wide = wide_profile();
    for (doc_depth, photo_depth) in [(Depth::F32, Depth::U16), (Depth::U16, Depth::F32)] {
        studio.doc = Document::new("Deep", 8., 8., 72.);
        studio.doc.color = cms::DocumentColor { depth: doc_depth, icc_profile: wide.clone(), intent: 0, ..Default::default() };
        let placed = place(&mut studio, tagged_photo(photo_depth));
        let native = placed.native.as_ref().unwrap();
        assert_eq!((native.depth, native.intent, native.icc_profile.as_slice()), (doc_depth, 0, wide.as_slice()));
    }
}

/// Build an ICC v2 RGB matrix/TRC profile with Adobe RGB (1998) D50 colorants and gamma 2.2.
fn wide_profile() -> Vec<u8> {
    fn s15(v: f64) -> [u8; 4] {
        ((v * 65536.).round() as i32).to_be_bytes()
    }
    let xyz = |v: [f64; 3]| [b"XYZ ".as_slice(), &[0; 4], &s15(v[0]), &s15(v[1]), &s15(v[2])].concat();
    let curve = [b"curv".as_slice(), &[0; 4], &1u32.to_be_bytes(), &563u16.to_be_bytes(), &[0; 2]].concat();
    let text = b"Wide test\0";
    let desc = [b"desc".as_slice(), &[0; 4], &(text.len() as u32).to_be_bytes(), text, &[0; 8], &[0; 3], &[0; 67]].concat();
    let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"desc", desc),
        (b"wtpt", xyz([0.9642, 1., 0.8249])),
        (b"rXYZ", xyz([0.6097, 0.3111, 0.0195])),
        (b"gXYZ", xyz([0.2053, 0.6257, 0.0609])),
        (b"bXYZ", xyz([0.1492, 0.0632, 0.7446])),
        (b"rTRC", curve.clone()),
        (b"gTRC", curve.clone()),
        (b"bTRC", curve),
    ];
    let mut offset = 128 + 4 + 12 * tags.len();
    let (mut table, mut data) = ((tags.len() as u32).to_be_bytes().to_vec(), Vec::new());
    for (sig, body) in &tags {
        table.extend([sig.as_slice(), &(offset as u32).to_be_bytes(), &(body.len() as u32).to_be_bytes()].concat());
        data.extend(body);
        offset += body.len();
    }
    let mut header = vec![0u8; 128];
    header[0..4].copy_from_slice(&(offset as u32).to_be_bytes());
    header[8..12].copy_from_slice(&0x0210_0000u32.to_be_bytes());
    header[12..16].copy_from_slice(b"mntr");
    header[16..20].copy_from_slice(b"RGB ");
    header[20..24].copy_from_slice(b"XYZ ");
    header[36..40].copy_from_slice(b"acsp");
    for (i, v) in [0.9642, 1., 0.8249].into_iter().enumerate() {
        header[68 + i * 4..72 + i * 4].copy_from_slice(&s15(v));
    }
    [header, table, data].concat()
}

/// A saturated green outside sRGB, authored in the wide profile.
fn wide_green(depth: Depth) -> Pixels {
    let mut pixels = Pixels::from_samples(4, 4, depth, [0., 1., 0., 1.].repeat(16)).unwrap();
    pixels.assign_rgb_profile(wide_profile(), 1).unwrap();
    pixels
}

fn wide_doc(depth: Depth) -> Document {
    doc_with(
        shadowed(wide_green(depth)),
        cms::DocumentColor { depth, icc_profile: wide_profile(), intent: 1, ..Default::default() },
    )
}

#[test]
fn wide_gamut_effects_keep_native_content_in_layers_groups_and_clips() {
    let wide = wide_profile();
    cms::validate_profile(&wide, cms::Model::Rgb).unwrap();
    // lcms reference: this green lies outside sRGB, so an sRGB intermediate cannot carry it.
    let srgb = cms::transform_alpha(&[0., 1., 0., 1.], cms::Model::Rgb, &wide, true, 1).unwrap();
    assert!(srgb[0] < -0.1 || srgb[2] < -0.1);
    let layer = wide_doc(Depth::U16);
    let mut group = layer.clone();
    let mut parent = Layer::group("Group");
    parent.filters = group.layers[0].filters.clone();
    group.layers[0].filters = Default::default();
    group.layers[0].parent = Some(parent.id);
    group.layers.insert(0, parent);
    let mut clip = layer.clone();
    let mut clipped = Layer::placed_raster("Clipped", wide_green(Depth::U16), Pt::new(2., 0.), Pt::new(4., 4.));
    clipped.clipped = true;
    clip.layers.push(clipped);
    for doc in [layer, group, clip] {
        let before = crate::project::encode(&doc).unwrap();
        let pixels = super::composite(&doc, 1).unwrap();
        let content = pixels.sample(8 + 1);
        for (got, want) in content.iter().zip([0., 1., 0., 1.]) {
            assert!((got - want).abs() < 1e-3, "{content:?}");
        }
        let shadow = pixels.sample(5 * 8 + 5);
        assert!(shadow[3] > 0.99 && shadow[..3].iter().all(|c| c.abs() < 1e-3), "{shadow:?}");
        assert_eq!(crate::project::encode(&doc).unwrap(), before);
    }
}

#[test]
fn tagged_deep_and_cmyk_documents_export_every_format() {
    let wide = wide_profile();
    let fogra = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    let mut cmyk = Document::new("Inks", 8., 8., 72.);
    cmyk.color = cms::DocumentColor { model: cms::Model::Cmyk, depth: Depth::U16, icc_profile: fogra.clone(), intent: 1 };
    let inks = Pixels::from_cmyk(4, 4, Depth::U16, [0.1, 0.2, 0.3, 0.4, 1.].repeat(16), fogra, 1).unwrap();
    cmyk.layers = vec![Layer::placed_raster("Inks", inks, Pt::ZERO, Pt::new(4., 4.))];
    for doc in [wide_doc(Depth::U16), wide_doc(Depth::F32), cmyk] {
        let before = crate::project::encode(&doc).unwrap();
        let source = Source::Document(doc.clone());
        for format in Format::ALL {
            let settings = Settings { format, ..Default::default() };
            let (bytes, _) = export::encode(&source, settings, &crate::ml::NoProgress)
                .unwrap_or_else(|e| panic!("{:?} {format:?}: {e}", doc.color.model));
            assert!(!bytes.is_empty());
        }
        assert_eq!(crate::project::encode(&doc).unwrap(), before);
    }
    // JPEG and PNG keep the document's wide encoding and embed its profile instead of clipping to sRGB.
    let source = Source::Document(wide_doc(Depth::U16));
    for format in [Format::Jpeg, Format::Png] {
        use image::ImageDecoder;
        let bytes = export::encode(&source, Settings { format, ..Default::default() }, &crate::ml::NoProgress).unwrap().0;
        let mut decoder = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format().unwrap().into_decoder().unwrap();
        assert_eq!(decoder.icc_profile().unwrap().as_deref(), Some(wide.as_slice()), "{format:?}");
        let px = image::DynamicImage::from_decoder(decoder).unwrap().into_rgba8().get_pixel(1, 1).0;
        assert!(px[0] < 3 && px[1] > 252 && px[2] < 3, "{format:?} {px:?}");
    }
}
