use super::tests::{FONT, root};
use super::*;
use crate::{
    color::Rgba,
    document::{Layer, Shape, Style},
    geom::{Bounds, Overflow, PathAlign, Pt, TextAlign, TextFrame, TextOnPath, TextThread, VAlign},
    text::variations::FontVariations,
};
fn doc() -> Document {
    let mut d = Document::new("SVG native text layouts", 760., 400., 72.);
    d.transparent = true;
    d.layers = vec![Layer::vector("Live text")];
    d
}
fn text(content: &str) -> TypeRun {
    TypeRun {
        font: FONT.into(),
        font_variations: FontVariations::new(vec![(*b"wght", 700.)]).unwrap(),
        content: content.into(),
        px: 30.,
        origin: Pt::new(25., 60.),
        ..Default::default()
    }
}
fn shape(run: TypeRun) -> Shape {
    let mut s = Shape::new(
        Geom::Text(run),
        Style {
            fill: Fill::Solid(Rgba::BLACK),
            stroke: None,
        },
    );
    crate::text::fill_contours(&mut s.geom);
    s
}
fn add(d: &mut Document, s: Shape) -> u64 {
    let id = s.id;
    d.layers[0].kind.shapes_mut().unwrap().push(s);
    id
}
fn check(mut d: Document, name: &str) -> (Document, String) {
    crate::text_geometry::reflow(&mut d);
    let before = crate::project::encode(&d).unwrap();
    let report = crate::svg::export_embedded_report(&d).unwrap();
    assert!(report.warnings.is_empty(), "{name}: {:?}", report.warnings);
    let expected = crate::compositor::render_export(&d, 1).unwrap();
    let out = root().join(name);
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("editable.svg"), &report.svg).unwrap();
    std::fs::write(out.join("native.png"), expected.encode_png().unwrap()).unwrap();
    std::fs::write(out.join("source.oma"), &before).unwrap();
    let (imported, notes) = crate::formats::svg::read(&report.svg, "Advanced SVG type").unwrap();
    assert!(notes.is_empty(), "{name}: {notes:?}");
    let actual = crate::compositor::render_export(&imported, 1).unwrap();
    std::fs::write(out.join("imported.png"), actual.encode_png().unwrap()).unwrap();
    let mut bounds = Bounds::from_pt(Pt::new(760., 400.));
    let mut max = Pt::ZERO;
    let mut occupied = false;
    for (i, (a, b)) in expected
        .data()
        .chunks_exact(4)
        .zip(actual.data().chunks_exact(4))
        .enumerate()
    {
        if a[3] != 0 || b[3] != 0 {
            let p = Pt::new((i % 760) as f32, (i / 760) as f32);
            bounds.min.x = bounds.min.x.min(p.x);
            bounds.min.y = bounds.min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
            occupied = true;
        }
    }
    assert!(occupied);
    let [x0, y0, x1, y1] = [
        bounds.min.x as u32,
        bounds.min.y as u32,
        max.x as u32 + 1,
        max.y as u32 + 1,
    ];
    let mut error = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * 760 + x) * 4) as usize;
            for c in 0..4 {
                error += u64::from(expected.data()[i + c].abs_diff(actual.data()[i + c]));
            }
        }
    }
    let mean = error as f64 / ((x1 - x0) * (y1 - y0) * 4) as f64;
    std::fs::write(out.join("native.json"),serde_json::json!({"case":name,"ink_roi":[x0,y0,x1,y1],"mean_channel_error":mean,"limit":1.5,"warnings":notes,"dimensions":[760,400]}).to_string()).unwrap();
    assert!(mean < 1.5, "{name}: ink ROI mean {mean}");
    assert_eq!(crate::project::encode(&d).unwrap(), before);
    let saved = crate::project::encode(&imported).unwrap();
    let reopened = crate::project::decode(&saved).unwrap();
    assert_eq!(
        crate::compositor::export_png(&reopened, 1).unwrap(),
        actual.encode_png().unwrap()
    );
    std::fs::write(out.join("portable.oma"), saved).unwrap();
    (d, report.svg)
}
#[test]
fn craft_svg_text_layout_native_features_and_metrics_remain_live() {
    let mut d = doc();
    let mut run = text("office AVATAR fi 1234");
    run.liga = false;
    run.kern = false;
    run.tnum = true;
    run.smcp = true;
    add(&mut d, shape(run));
    let mut run = text("AVATAR fi font");
    run.origin.y = 140.;
    run.tracking = 55.;
    run.manual_kern.insert(3, 160.);
    run.set_character_style(0, 6, |s| {
        s.hscale = Some(145.);
        s.vscale = Some(80.);
        s.kerning = Some(crate::geom::KernMode::Optical);
        s.baseline_shift = Some(8.);
    });
    add(&mut d, shape(run));
    let (_, svg) = check(d, "metrics");
    assert!(svg.contains("textLength="));
    assert!(svg.contains("&apos;liga&apos; 0"));
    assert!(!svg.contains("<path"));
}
#[test]
fn craft_svg_text_layout_gradient_pattern_affine_and_native_stroke_remain_live() {
    let mut d = doc();
    let mut run = text("Painted variable type");
    run.transform = Some([1.15, 0.13, 0.18, 0.85, 18., 8.]);
    run.set_character_style(0, 7, |s| {
        s.fill = Some(Fill::Linear {
            from: [0., 0.],
            to: [1., 0.2],
            c0: Rgba::rgb(230, 20, 30),
            c1: Rgba::rgb(15, 45, 230),
        })
    });
    let mut s = shape(run);
    s.style.stroke = Some(crate::document::Stroke {
        width: 0.7,
        color: Rgba::rgb(25, 100, 65),
        ..Default::default()
    });
    s.rotation = 0.08;
    add(&mut d, s);
    let mut run = text("Pattern in the text");
    run.origin.y = 190.;
    let pattern=crate::svg_source::SvgPattern{definitions:r##"<pattern id="tile" patternUnits="userSpaceOnUse" width="8" height="8"><rect width="8" height="8" fill="#08b"/><rect width="3" height="8" fill="#e52"/></pattern>"##.into(),id:"tile".into(),transform:[1.,0.,0.,1.,0.,0.],opacity:0.8};
    run.set_character_style(0, 7, |s| s.fill = Some(Fill::Pattern(pattern.clone())));
    add(&mut d, shape(run));
    let (_, svg) = check(d, "paint");
    assert!(svg.contains("<linearGradient"));
    assert!(svg.contains("<pattern"));
    assert_eq!(
        roxmltree::Document::parse(&svg)
            .unwrap()
            .descendants()
            .filter(|n| n.has_tag_name("text"))
            .count(),
        2
    );
}
#[test]
fn craft_svg_text_layout_frames_share_wrap_clip_alignment_and_visible_lines() {
    let mut d = doc();
    let mut run = text(
        "Variable area text wraps around the object and keeps its chosen faces. Several lines remain visible while this final hidden overflow stays in the native story.",
    );
    run.px = 23.;
    run.frame = Some(TextFrame {
        size: Pt::new(330., 155.),
        valign: VAlign::Center,
        overflow: Overflow::Clip,
        ..Default::default()
    });
    run.transform = Some([1., 0.08, 0.05, 1., 10., 0.]);
    run.align = TextAlign::Justify {
        last: crate::geom::LastLine::Start,
    };
    run.set_character_style(0, 8, |s| {
        s.font_variations = Some(FontVariations::new(vec![(*b"wght", 800.)]).unwrap());
        s.px = Some(29.);
    });
    let id = add(&mut d, shape(run));
    let mut obstacle = Shape::new(
        Geom::Rect {
            origin: Pt::new(205., 75.),
            size: Pt::new(75., 46.),
            radius: 0.,
        },
        Style {
            fill: Fill::Solid(Rgba::rgb(200, 225, 245)),
            stroke: None,
        },
    );
    obstacle.text_wrap.mode = crate::geom::WrapMode::ObjectShape;
    obstacle.text_wrap.offset = [5.; 4];
    add(&mut d, obstacle);
    let (native, svg) = check(d, "frame");
    let Geom::Text(run) = &native.find_shape(0, id).unwrap().geom else {
        panic!()
    };
    let layout = run.layout.as_ref().unwrap();
    assert!(layout.export_lines.len() > 2);
    assert!(layout.overflow);
    assert!(
        layout
            .export_lines
            .iter()
            .all(|p| p.run.content.chars().count() == p.line.end)
    );
    assert!(svg.contains("oma-embedded-frame"));
    assert_eq!(
        roxmltree::Document::parse(&svg)
            .unwrap()
            .descendants()
            .filter(|n| n.has_tag_name("text"))
            .count(),
        1
    );
    assert!(!serde_json::to_string(run).unwrap().contains("export_lines"));
}
#[test]
fn craft_svg_text_layout_threaded_frames_export_each_visible_story_part_once() {
    let mut d = doc();
    let mut first = text(
        "A real threaded story moves across both frames and keeps its original variable font. Each visible line is written once. The tail uses the same story source instead of an empty frame placeholder.",
    );
    first.px = 25.;
    first.frame = Some(TextFrame {
        size: Pt::new(235., 95.),
        ..Default::default()
    });
    let head = add(&mut d, shape(first.clone()));
    let mut tail = first;
    tail.origin = Pt::new(350., 80.);
    tail.content.clear();
    tail.spans.clear();
    tail.frame.as_mut().unwrap().size.y = 230.;
    tail.thread = Some(TextThread {
        story: head,
        prev: Some(head),
        next: None,
    });
    let tail = add(&mut d, shape(tail));
    if let Geom::Text(head_run) = &mut d.find_shape_mut(0, head).unwrap().geom {
        head_run.thread = Some(TextThread {
            story: head,
            prev: None,
            next: Some(tail),
        });
    }
    let (native, svg) = check(d, "threads");
    let Geom::Text(a) = &native.find_shape(0, head).unwrap().geom else {
        panic!()
    };
    let Geom::Text(b) = &native.find_shape(0, tail).unwrap().geom else {
        panic!()
    };
    assert!(a.layout.as_ref().unwrap().visible_end > 0);
    assert_eq!(
        a.layout.as_ref().unwrap().visible_end,
        b.layout.as_ref().unwrap().visible_start
    );
    assert!(!b.layout.as_ref().unwrap().export_lines.is_empty());
    assert_eq!(
        roxmltree::Document::parse(&svg)
            .unwrap()
            .descendants()
            .filter(|n| n.has_tag_name("text"))
            .count(),
        2
    );
}
fn path_doc(closed: bool, flip: bool, align: PathAlign) -> Document {
    let mut d = doc();
    let guide = if closed {
        Shape::new(
            Geom::Ellipse {
                center: Pt::new(360., 190.),
                radii: Pt::new(210., 120.),
            },
            Style::default(),
        )
    } else {
        let mut a = crate::geom::Anchor::corner(Pt::new(35., 210.));
        a.h_out = Pt::new(170., -150.);
        let mut b = crate::geom::Anchor::corner(Pt::new(700., 220.));
        b.h_in = Pt::new(-220., 130.);
        Shape::new(
            Geom::Path {
                anchors: vec![a, b],
                closed: false,
            },
            Style::default(),
        )
    };
    let mut guide = guide;
    guide.guide = true;
    let id = add(&mut d, guide);
    let mut run = text("AV fi follows a real curve");
    run.on_path = Some(TextOnPath {
        path_id: id,
        start: if closed { 0.83 } else { 0.08 },
        end: if closed { 0.35 } else { 0.92 },
        flip,
        align,
        baseline_shift: 3.,
        ..Default::default()
    });
    run.origin = Pt::ZERO;
    run.set_character_style(0, 5, |s| {
        s.font_variations = Some(FontVariations::new(vec![(*b"wght", 800.)]).unwrap())
    });
    add(&mut d, shape(run));
    d
}
#[test]
fn craft_svg_text_layout_open_path_retains_live_text_and_cubic_guide() {
    let (_, svg) = check(path_doc(false, false, PathAlign::Baseline), "path");
    assert!(svg.contains("<textPath"));
    assert!(svg.contains(" C "));
}
#[test]
fn craft_svg_text_layout_closed_reversed_path_retains_live_text_and_native_baseline() {
    let (_, svg) = check(path_doc(true, true, PathAlign::Center), "closed-path");
    assert!(svg.contains("<textPath"));
}
#[test]
fn craft_svg_text_layout_unsupported_path_details_keep_pixels_and_report_reduction() {
    let mut d = path_doc(false, false, PathAlign::Baseline);
    for s in d.layers[0].kind.shapes_mut().unwrap() {
        if let Geom::Text(r) = &mut s.geom {
            r.manual_kern.insert(2, 100.);
        }
    }
    crate::text_geometry::reflow(&mut d);
    let before = crate::project::encode(&d).unwrap();
    let source = crate::export::Source::Document(d.clone());
    let (encoded, notes) =
        crate::export::encode_svg_admitted(&source, true, &crate::ml::NoProgress).unwrap();
    assert_eq!(notes.len(), 1);
    assert!(notes[0].contains("outlined"));
    let svg = std::str::from_utf8(encoded.as_slice()).unwrap();
    assert!(!svg.contains("<text"));
    assert!(svg.contains("<path"));
    assert!(
        crate::svg::export_embedded(&d)
            .unwrap_err()
            .contains("SvgReduction")
    );
    let (reopened, _) = crate::formats::svg::read(svg, "path fallback").unwrap();
    assert!(super::tests::error(&d, &reopened) < 1.5);
    assert_eq!(crate::project::encode(&d).unwrap(), before);
}
