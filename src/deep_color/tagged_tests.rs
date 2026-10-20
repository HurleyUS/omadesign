use super::*;

#[test]
fn craft_gray_lab_conversions_round_trip_and_are_neutral() {
    for rgb in [[1., 1., 1.], [0., 0., 0.], [0.8, 0.2, 0.4], [0.1, 0.6, 0.9]] {
        let back = lab_to_srgb(srgb_to_lab(rgb));
        for k in 0..3 {
            assert!((back[k] - rgb[k]).abs() < 1e-3, "{rgb:?} -> {back:?}");
        }
    }
    let white = srgb_to_lab([1., 1., 1.]);
    assert!((white[0] - 100.).abs() < 0.01 && white[1].abs() < 0.01 && white[2].abs() < 0.01);
    assert!((gray_of([0.5, 0.5, 0.5]) - 0.5).abs() < 1e-4);
    assert!(gray_of([0., 1., 0.]) > gray_of([0., 0., 1.]));
}

#[test]
fn craft_gray_tag_finishes_pixels_and_encodes_gray_png() {
    let mut pm = tiny_skia::Pixmap::new(2, 1).unwrap();
    pm.pixels_mut()[0] = tiny_skia::ColorU8::from_rgba(255, 0, 0, 255).premultiply();
    pm.pixels_mut()[1] = tiny_skia::ColorU8::from_rgba(0, 0, 255, 128).premultiply();
    let mut lab = pm.clone();
    finish(&mut lab, Some(ColorTag::Lab));
    assert_eq!(lab.data(), pm.data());
    finish(&mut pm, Some(ColorTag::Gray));
    let c = pm.pixels()[0].demultiply();
    assert!(c.red() == c.green() && c.green() == c.blue());
    assert_eq!(pm.pixels()[1].demultiply().alpha(), 128);
    let png = encode_gray_png(&pm).unwrap();
    let decoded = image::load_from_memory(&png).unwrap();
    assert_eq!(decoded.color(), image::ColorType::La8);
}

fn psd(mode: u16, planes: &[&[u8]]) -> Vec<u8> {
    let mut b = b"8BPS".to_vec();
    b.extend_from_slice(&1u16.to_be_bytes());
    b.extend_from_slice(&[0; 6]);
    b.extend_from_slice(&(planes.len() as u16).to_be_bytes());
    b.extend_from_slice(&1u32.to_be_bytes());
    b.extend_from_slice(&2u32.to_be_bytes());
    b.extend_from_slice(&8u16.to_be_bytes());
    b.extend_from_slice(&mode.to_be_bytes());
    b.extend_from_slice(&[0; 12]);
    b.extend_from_slice(&0u16.to_be_bytes());
    for p in planes {
        b.extend_from_slice(p);
    }
    b
}

#[test]
fn craft_gray_lab_psd_import_tags_and_converts() {
    let lab = crate::formats::psd::decode(&psd(9, &[&[255, 0], &[128, 128], &[128, 128]]), "lab").unwrap();
    assert_eq!(lab.document.color_tag, Some(ColorTag::Lab));
    let pm = crate::compositor::render_export(&lab.document, 1).unwrap();
    let [w, k] = [pm.pixels()[0].demultiply(), pm.pixels()[1].demultiply()];
    assert!(w.red() > 250 && w.green() > 250 && w.blue() > 250, "{w:?}");
    assert!(k.red() < 5, "{k:?}");
    let gray = crate::formats::psd::decode(&psd(1, &[&[200, 10]]), "gray").unwrap();
    assert_eq!(gray.document.color_tag, Some(ColorTag::Gray));
    let mut deep = psd(9, &[&[0, 0, 0, 0], &[0, 0, 0, 0], &[0, 0, 0, 0]]);
    deep[22..24].copy_from_slice(&16u16.to_be_bytes());
    assert!(crate::formats::psd::decode(&deep, "x").is_err());
}
