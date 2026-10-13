use omadesign::{
    brushes::Library,
    color::Rgba,
    document::{Layer, Pixels},
    geom::Pt,
    motion::{Ease, Key, Prop, Track},
    paint::Brush,
    tools::Persona,
};
use std::{fs, path::Path};

fn main() {
    let root = Path::new("examples/product-media");
    let library = Library::load(root.join("private-brushes"));
    let mut svg = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1600\" height=\"1000\" viewBox=\"0 0 1600 1000\"><rect width=\"1600\" height=\"1000\" fill=\"#f4f1e8\"/><g fill=\"#153d34\" font-family=\"Liberation Sans\"><text x=\"64\" y=\"75\" font-size=\"23\" letter-spacing=\"5\">FIELDWORK / BRUSH STUDIES</text><text x=\"64\" y=\"160\" font-size=\"64\" font-weight=\"700\">Every mark has a shape.</text>",
    );
    let mut ink = tiny_skia::Pixmap::new(1600, 1000).unwrap();
    for (i, preset) in library.packs.iter().flat_map(|p| &p.brushes).enumerate() {
        let x = 64. + (i % 4) as f32 * 390.;
        let y = 250. + (i / 4) as f32 * 185.;
        svg.push_str(&format!(
            "<text x=\"{x}\" y=\"{y}\" font-size=\"21\">{:02} / {}</text>",
            i + 1,
            preset.name
        ));
        let mut brush = Brush::default();
        preset.apply(&mut brush);
        brush.size = 40.;
        brush.color = Rgba::from_hex(0x153d34);
        brush.seed = i as u64 + 1;
        let mut remaining = 0.;
        for j in 0..180 {
            let point = |t: f32| Pt::new(x + t, y + 65. + (t / 40.).sin() * 19.);
            omadesign::brushes::stroke_spaced(
                &mut ink,
                point(j as f32 * 1.7),
                point((j + 1) as f32 * 1.7),
                &brush,
                &preset.tip,
                false,
                &mut remaining,
            );
        }
    }
    svg.push_str("</g></svg>");
    fs::write(root.join("pixel.svg"), &svg).unwrap();
    let (mut doc, notes) = omadesign::formats::svg::read(&svg, "Brush studies").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    doc.layers.push(Layer::placed_raster(
        "Native brush samples",
        Pixels::from_pixmap(&ink),
        Pt::ZERO,
        Pt::new(1600., 1000.),
    ));
    doc.layers.push(Layer::raster("Your marks", 1600, 1000));
    doc.workspace = Some(Persona::Pixel);
    fs::write(
        root.join("pixel.oma"),
        omadesign::project::encode(&doc).unwrap(),
    )
    .unwrap();

    let mut motion = omadesign::project::load_from(&root.join("motion.oma")).unwrap();
    motion.workspace = Some(Persona::Motion);
    motion.motion.duration = 4.;
    motion.motion.looped = true;
    for shape in motion
        .layers
        .iter()
        .filter_map(|l| l.kind.shapes())
        .flatten()
        .filter(|s| matches!(s.geom, omadesign::geom::Geom::Ellipse { .. }))
    {
        motion.motion.tracks.push(Track {
            shape: shape.id,
            prop: Prop::Scale,
            keys: vec![
                Key::at(0., 1., Ease::EaseInOut),
                Key::at(2., 0.6, Ease::EaseInOut),
                Key::at(4., 1., Ease::EaseInOut),
            ],
        });
    }
    fs::write(
        root.join("motion.oma"),
        omadesign::project::encode(&motion).unwrap(),
    )
    .unwrap();
}
