//! Map bounded native Affinity objects through the shared SVG artwork reader.

use base64::Engine as _;
use omadesign_affinity::{model as af, paint as ap};
use std::fmt::Write as _;

/// Read editable native Affinity artwork and retain the original container.
/// Args: original bytes and filename. Returns native artwork and explicit conversion diagnostics.
pub fn read(bytes: &[u8], name: &str) -> Result<(crate::document::Document, Vec<String>), String> {
    if bytes.len() > 512 * 1024 * 1024 {
        return Err("Affinity source exceeds 512 MiB".into());
    }
    let native = omadesign_affinity::read(bytes, omadesign_affinity::Limits { max_entry: 256 << 20, max_total: 512 << 20 })
        .map_err(|e| format!("Native Affinity import failed: {e}. Export SVG or PDF from Affinity for unsupported documents; keep the original file."))?;
    let mut builder = Builder::default();
    let mut body = String::new();
    let mut boards = Vec::new();
    let mut right = 0.0f64;
    let mut bottom = 0.0f64;
    for (index, spread) in native.spreads.iter().enumerate() {
        let shift = if index == 0 {
            0.0
        } else {
            right + 48.0 - spread.bounds.x0
        };
        write!(body, "<g transform=\"translate({shift} 0)\">").unwrap();
        let before = boards.len();
        for node in &spread.nodes {
            builder.node(node, &mut body, shift, &mut boards)?;
        }
        body.push_str("</g>");
        if before == boards.len() {
            for rect in if spread.pages.is_empty() {
                vec![spread.bounds]
            } else {
                spread.pages.clone()
            } {
                boards.push(board(&format!("Page {}", boards.len() + 1), rect, shift));
            }
        }
        right = right.max(spread.bounds.x1 + shift);
        bottom = bottom.max(spread.bounds.y1);
    }
    for b in &boards {
        right = right.max(f64::from(b.origin.x + b.size.x));
        bottom = bottom.max(f64::from(b.origin.y + b.size.y));
    }
    if !right.is_finite()
        || !bottom.is_finite()
        || right <= 0.0
        || bottom <= 0.0
        || right > 1e7
        || bottom > 1e7
    {
        return Err("Affinity canvas has invalid or unsupported dimensions".into());
    }
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" xmlns:inkscape=\"http://www.inkscape.org/namespaces/inkscape\" xmlns:sodipodi=\"http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd\" width=\"{right}\" height=\"{bottom}\"><defs>{}</defs>{body}</svg>",
        builder.defs
    );
    let (mut doc, svg_notes) = crate::formats::svg::read(&svg, name)?;
    doc.dpi = native.dpi as f32;
    doc.artboards = boards;
    doc.transparent = native.spreads.first().is_some_and(|s| s.transparent);
    let id = doc.retain_source(crate::source_attachment::SourceAttachment::new(
        name,
        "affinity",
        bytes.to_vec(),
    )?);
    for layer in &mut doc.layers {
        layer.source_attachment = Some(id.clone());
    }
    let mut notes = native
        .warnings
        .into_iter()
        .map(|w| format!("Affinity: {w}. Original data is retained in this project."))
        .collect::<Vec<_>>();
    notes.extend(builder.notes);
    notes.extend(svg_notes);
    notes.push("Opened native Affinity objects. Original source bytes are retained once in this project; saving OMA preserves them but does not write an Affinity document. Export SVG or PDF for interchange.".into());
    if let Some(saved_by) = native.saved_by {
        notes.push(format!("Source saved by {saved_by}."));
    }
    doc.validate_hierarchy()?;
    Ok((doc, notes))
}

fn board(name: &str, r: af::Rect, shift: f64) -> crate::document::Artboard {
    let mut b = crate::document::Artboard::new(
        0,
        crate::geom::Pt::new((r.x0 + shift) as f32, r.y0 as f32),
        crate::geom::Pt::new((r.x1 - r.x0) as f32, (r.y1 - r.y0) as f32),
    );
    b.name = name.to_string();
    b
}

#[derive(Default)]
struct Builder {
    defs: String,
    next: usize,
    notes: Vec<String>,
}

impl Builder {
    fn id(&mut self) -> String {
        self.next += 1;
        format!("af-{}", self.next)
    }
    fn warn(&mut self, note: &str) {
        if !self.notes.iter().any(|w| w == note) {
            self.notes.push(note.to_string());
        }
    }

    fn node(
        &mut self,
        n: &af::Node,
        out: &mut String,
        shift: f64,
        boards: &mut Vec<crate::document::Artboard>,
    ) -> Result<(), String> {
        let id = self.id();
        write!(out,"<g id=\"{id}\" inkscape:label=\"{}\" sodipodi:insensitive=\"{}\" opacity=\"{}\" style=\"{}mix-blend-mode:{};isolation:{}\">", escape(&n.name), n.locked, n.opacity, if n.visible { "" } else { "display:none;" }, blend(n.blend), if matches!(n.blend,af::Blend::PassThrough) { "auto" } else { "isolate" }).unwrap();
        if let Some(mask) = &n.mask {
            let id = self.clip(mask, false);
            write!(out, "<g clip-path=\"url(#{id})\">").unwrap();
        }
        if let Some(mask) = &n.pixel_mask {
            let id = self.id();
            let mut content = String::new();
            self.image(mask, &mut content)?;
            write!(self.defs,"<mask id=\"{id}\" maskUnits=\"userSpaceOnUse\" maskContentUnits=\"userSpaceOnUse\" x=\"-10000000\" y=\"-10000000\" width=\"20000000\" height=\"20000000\" style=\"mask-type:luminance\">{content}</mask>").unwrap();
            write!(out, "<g mask=\"url(#{id})\">").unwrap();
        }
        match &n.kind {
            af::Kind::Layer | af::Kind::Group | af::Kind::Unsupported => {
                for child in &n.children {
                    self.node(child, out, shift, boards)?;
                }
            }
            af::Kind::Artboard {
                rect,
                path,
                even_odd,
                background,
                strokes,
            } => {
                boards.push(board(&n.name, *rect, shift));
                self.shape(path, background, &[], *even_odd, out);
                let clip = self.clip(path, *even_odd);
                write!(out, "<g clip-path=\"url(#{clip})\">").unwrap();
                for child in &n.children {
                    self.node(child, out, shift, boards)?;
                }
                out.push_str("</g>");
                self.shape(path, &[], strokes, *even_odd, out);
            }
            af::Kind::Shape {
                path,
                fills,
                strokes,
                even_odd,
            } => {
                let behind = strokes
                    .iter()
                    .filter(|s| s.behind)
                    .cloned()
                    .collect::<Vec<_>>();
                let above = strokes
                    .iter()
                    .filter(|s| !s.behind)
                    .cloned()
                    .collect::<Vec<_>>();
                self.shape(path, &[], &behind, *even_odd, out);
                self.shape(path, fills, &[], *even_odd, out);
                if !n.children.is_empty() {
                    let clip = self.clip(path, *even_odd);
                    write!(out, "<g clip-path=\"url(#{clip})\">").unwrap();
                    for child in &n.children {
                        self.node(child, out, shift, boards)?;
                    }
                    out.push_str("</g>");
                }
                self.shape(path, &[], &above, *even_odd, out);
            }
            af::Kind::Image(image) => {
                self.image(image, out)?;
                for child in &n.children {
                    self.node(child, out, shift, boards)?;
                }
            }
            af::Kind::Text(text) => {
                self.text(text, out);
                for child in &n.children {
                    self.node(child, out, shift, boards)?;
                }
            }
        }
        if n.pixel_mask.is_some() {
            out.push_str("</g>");
        }
        if n.mask.is_some() {
            out.push_str("</g>");
        }
        out.push_str("</g>");
        Ok(())
    }

    fn clip(&mut self, path: &af::Path, even_odd: bool) -> String {
        let id = self.id();
        write!(self.defs,"<clipPath id=\"{id}\" clipPathUnits=\"userSpaceOnUse\"><path d=\"{}\" clip-rule=\"{}\"/></clipPath>",path_data(path),rule(even_odd)).unwrap();
        id
    }

    fn shape(
        &mut self,
        path: &af::Path,
        fills: &[ap::Paint],
        strokes: &[ap::Stroke],
        even_odd: bool,
        out: &mut String,
    ) {
        let data = path_data(path);
        for fill in fills {
            let paint = self.paint(fill);
            write!(
                out,
                "<path d=\"{data}\" fill=\"{paint}\" fill-rule=\"{}\"/>",
                rule(even_odd)
            )
            .unwrap();
        }
        for stroke in strokes {
            let paint = self.paint(&stroke.paint);
            write!(out,"<path d=\"{data}\" fill=\"none\" stroke=\"{paint}\" stroke-width=\"{}\" stroke-linecap=\"{}\" stroke-linejoin=\"{}\" stroke-miterlimit=\"{}\"",stroke.width,match stroke.cap {ap::Cap::Butt=>"butt",ap::Cap::Round=>"round",ap::Cap::Square=>"square"},match stroke.join{ap::Join::Miter=>"miter",ap::Join::Round=>"round",ap::Join::Bevel=>"bevel"},stroke.miter_limit).unwrap();
            if let Some((dash, phase)) = &stroke.dash {
                write!(
                    out,
                    " stroke-dasharray=\"{}\" stroke-dashoffset=\"{phase}\"",
                    dash.iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                )
                .unwrap();
            }
            if !matches!(stroke.align, ap::Align::Center) {
                self.warn("Affinity inside/outside stroke alignment currently imports centered; original source data is retained.");
            }
            out.push_str("/>");
        }
    }

    fn paint(&mut self, paint: &ap::Paint) -> String {
        match paint {
            ap::Paint::None => "none".into(),
            ap::Paint::Solid(c) => rgba(*c),
            ap::Paint::Gradient(g) => {
                let id = self.id();
                let kind = if matches!(g.kind, ap::GradientKind::Linear) {
                    "linearGradient"
                } else {
                    "radialGradient"
                };
                if matches!(g.kind, ap::GradientKind::Conical) {
                    self.warn("Affinity conical gradients import as radial gradients; original data is retained.");
                }
                write!(self.defs,"<{kind} id=\"{id}\" gradientUnits=\"userSpaceOnUse\" {} gradientTransform=\"{}\">",if kind=="linearGradient" {"x1=\"0\" y1=\"0\" x2=\"1\" y2=\"0\""}else{"cx=\"0\" cy=\"0\" r=\"1\""},matrix(g.transform)).unwrap();
                for (index, stop) in g.stops.iter().enumerate() {
                    write!(
                        self.defs,
                        "<stop offset=\"{}\" stop-color=\"{}\"/>",
                        stop.offset,
                        rgba(stop.color)
                    )
                    .unwrap();
                    if let Some(next) = g.stops.get(index + 1)
                        && (stop.bias - 0.5).abs() > 1e-6
                    {
                        let middle = mix(stop.color, next.color);
                        let offset = stop.offset + (next.offset - stop.offset) * stop.half_point();
                        write!(
                            self.defs,
                            "<stop offset=\"{offset}\" stop-color=\"{}\"/>",
                            rgba(middle)
                        )
                        .unwrap();
                    }
                }
                write!(self.defs, "</{kind}>").unwrap();
                format!("url(#{id})")
            }
        }
    }

    fn image(&mut self, image: &af::Image, out: &mut String) -> Result<(), String> {
        let data = match &image.pixels {
            af::Pixels::Encoded(data) => data.clone(),
            af::Pixels::Rgba8(data) => {
                let image = image::RgbaImage::from_raw(image.width, image.height, data.clone())
                    .ok_or("Invalid native Affinity pixels")?;
                let mut output = std::io::Cursor::new(Vec::new());
                image
                    .write_to(&mut output, image::ImageFormat::Png)
                    .map_err(|e| e.to_string())?;
                output.into_inner()
            }
        };
        let mime = if data.starts_with(&[0xff, 0xd8]) {
            "image/jpeg"
        } else {
            "image/png"
        };
        write!(out,"<image width=\"{}\" height=\"{}\" transform=\"{}\" xlink:href=\"data:{mime};base64,{}\"/>",image.width,image.height,matrix(image.transform),base64::engine::general_purpose::STANDARD.encode(data)).unwrap();
        Ok(())
    }

    fn text(&mut self, text: &af::Text, out: &mut String) {
        let anchor = match text.align {
            af::Align::Left | af::Align::Justify => "start",
            af::Align::Center => "middle",
            af::Align::Right => "end",
        };
        let (x, y) = if let Some(frame) = text.frame {
            self.warn("Affinity frame text retains editable characters and styles; exact frame flow depends on shared type integration.");
            (
                frame.x0,
                frame.y0 + text.runs.first().map_or(12., |r| r.size),
            )
        } else {
            (text.anchor.x, text.anchor.y)
        };
        write!(out,"<text x=\"{x}\" y=\"{y}\" transform=\"{}\" text-anchor=\"{anchor}\" xml:space=\"preserve\">",matrix(text.transform)).unwrap();
        for run in &text.runs {
            let fill = self.paint(&run.fill);
            let content = if run.all_caps {
                run.text.to_uppercase()
            } else {
                run.text.clone()
            };
            let family = if run.family.is_empty() {
                &run.postscript
            } else {
                &run.family
            };
            write!(out,"<tspan font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" font-style=\"{}\" letter-spacing=\"{}\" fill=\"{fill}\">{}</tspan>",escape(family),run.size,run.weight,if run.italic{"italic"}else{"normal"},run.tracking*run.size,escape(&content)).unwrap();
            if !run.features.is_empty() {
                self.warn("Affinity OpenType overrides are retained in original data; exact feature mapping depends on shared type integration.");
            }
        }
        out.push_str("</text>");
    }
}

fn rule(even_odd: bool) -> &'static str {
    if even_odd { "evenodd" } else { "nonzero" }
}
fn matrix(m: af::Affine) -> String {
    format!(
        "matrix({} {} {} {} {} {})",
        m.0[0], m.0[1], m.0[2], m.0[3], m.0[4], m.0[5]
    )
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn path_data(p: &af::Path) -> String {
    let mut out = String::new();
    for s in &p.subpaths {
        write!(out, "M{} {}", s.start.x, s.start.y).unwrap();
        for [a, b, c] in &s.segments {
            write!(out, "C{} {} {} {} {} {}", a.x, a.y, b.x, b.y, c.x, c.y).unwrap();
        }
        if s.closed {
            out.push('Z');
        }
    }
    out
}
fn channels(c: ap::Color) -> [f64; 4] {
    match c {
        ap::Color::Rgb { r, g, b, a } => [r, g, b, a],
        ap::Color::Gray { v, a } => [v, v, v, a],
        ap::Color::Cmyk { c, m, y, k, a } => [
            (1. - c) * (1. - k),
            (1. - m) * (1. - k),
            (1. - y) * (1. - k),
            a,
        ],
        ap::Color::Lab { l, a, b, alpha } => {
            let fy = (l + 16.) / 116.;
            let fx = fy + a / 500.;
            let fz = fy - b / 200.;
            let inv = |v: f64| {
                if v.powi(3) > 0.008856 {
                    v.powi(3)
                } else {
                    (v - 16. / 116.) / 7.787
                }
            };
            let x = 0.96422 * inv(fx);
            let y = inv(fy);
            let z = 0.82521 * inv(fz);
            let [x, y, z] = [
                0.9555766 * x - 0.0230393 * y + 0.0631636 * z,
                -0.0282895 * x + 1.0099416 * y + 0.0210077 * z,
                0.0122982 * x - 0.020483 * y + 1.3299098 * z,
            ];
            let enc = |v: f64| {
                if v <= 0.0031308 {
                    12.92 * v
                } else {
                    1.055 * v.powf(1. / 2.4) - 0.055
                }
            };
            [
                enc(3.2404542 * x - 1.5371385 * y - 0.4985314 * z),
                enc(-0.969266 * x + 1.8760108 * y + 0.041556 * z),
                enc(0.0556434 * x - 0.2040259 * y + 1.0572252 * z),
                alpha,
            ]
        }
    }
}
fn rgba(c: ap::Color) -> String {
    let [r, g, b, a] = channels(c);
    format!(
        "rgba({},{},{},{})",
        (r.clamp(0., 1.) * 255.).round() as u8,
        (g.clamp(0., 1.) * 255.).round() as u8,
        (b.clamp(0., 1.) * 255.).round() as u8,
        a.clamp(0., 1.)
    )
}
fn mix(a: ap::Color, b: ap::Color) -> ap::Color {
    let a = channels(a);
    let b = channels(b);
    ap::Color::Rgb {
        r: (a[0] + b[0]) / 2.,
        g: (a[1] + b[1]) / 2.,
        b: (a[2] + b[2]) / 2.,
        a: (a[3] + b[3]) / 2.,
    }
}
fn blend(b: af::Blend) -> &'static str {
    match b {
        af::Blend::Multiply => "multiply",
        af::Blend::Screen => "screen",
        af::Blend::Overlay => "overlay",
        af::Blend::Darken => "darken",
        af::Blend::Lighten => "lighten",
        af::Blend::ColorDodge => "color-dodge",
        af::Blend::ColorBurn => "color-burn",
        af::Blend::HardLight => "hard-light",
        af::Blend::SoftLight => "soft-light",
        af::Blend::Difference => "difference",
        af::Blend::Exclusion => "exclusion",
        af::Blend::Hue => "hue",
        af::Blend::Saturation => "saturation",
        af::Blend::Color => "color",
        af::Blend::Luminosity => "luminosity",
        _ => "normal",
    }
}
