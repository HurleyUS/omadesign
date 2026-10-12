//! Render actual brush strokes for downloadable pack previews.
use base64::{Engine, engine::general_purpose::STANDARD};
use omadesign::{brushes, color::Rgba, geom::Pt, paint::Brush};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("brush_preview PACK OUTPUT.svg");
        std::process::exit(2);
    }
    let pack = brushes::import(std::path::Path::new(&args[1])).unwrap();
    let height = pack.brushes.len() as u32 * 60;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"640\" height=\"{height}\" viewBox=\"0 0 640 {height}\"><rect width=\"640\" height=\"{height}\" fill=\"#f8f5ef\"/>"
    );
    for (i, preset) in pack.brushes.iter().enumerate() {
        let mut pm = tiny_skia::Pixmap::new(440, 60).unwrap();
        let mut brush = Brush::default();
        preset.apply(&mut brush);
        brush.size = 38.;
        brush.color = Rgba::rgb(44, 37, 54);
        brushes::stamp(&mut pm, Pt::new(20., 30.), &brush, &preset.tip, false);
        brushes::stroke(
            &mut pm,
            Pt::new(20., 30.),
            Pt::new(420., 30.),
            &brush,
            &preset.tip,
            false,
        );
        let png = STANDARD.encode(pm.encode_png().unwrap());
        let name = preset
            .name
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let y = i * 60;
        svg.push_str(&format!("<text x=\"24\" y=\"{}\" font-family=\"system-ui,sans-serif\" font-size=\"17\" fill=\"#2c2536\">{name}</text><image x=\"180\" y=\"{y}\" width=\"440\" height=\"60\" href=\"data:image/png;base64,{png}\"/>",y+36));
    }
    svg.push_str("</svg>");
    std::fs::write(&args[2], svg).unwrap();
}
