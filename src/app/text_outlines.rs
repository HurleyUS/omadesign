//! Editable text paint becomes paths inside the existing appearance frame.
use super::*;
use crate::{
    gradient::{Gradient, GradientKind},
    layout::{Constraint, FrameLayout},
};

struct Outline {
    hit: (usize, u64),
    shapes: Vec<Shape>,
}

fn gradient_in_bounds(
    source: &Gradient,
    before: Bounds,
    after: Bounds,
) -> Result<Gradient, String> {
    if source.kind == GradientKind::Shape {
        return Err(
            "Shape-distance text paint needs a shared outline paint space; it was not converted"
                .into(),
        );
    }
    let point = |value: [f32; 2]| {
        [
            (before.min.x + value[0] * before.width().max(0.001) - after.min.x)
                / after.width().max(0.001),
            (before.min.y + value[1] * before.height().max(0.001) - after.min.y)
                / after.height().max(0.001),
        ]
    };
    let mut result = source.clone();
    result.from = point(source.from);
    result.to = point(source.to);
    if source.kind == GradientKind::Radial {
        let transform = source.radial_transform(before);
        result.radial_axis = Some([
            transform.kx / after.width().max(0.001),
            transform.sy / after.height().max(0.001),
        ]);
    }
    Ok(result)
}

fn fill_in_bounds(source: &Fill, before: Bounds, after: Bounds) -> Result<Fill, String> {
    Ok(match source {
        Fill::None | Fill::Solid(_) => source.clone(),
        Fill::Linear { from, to, c0, c1 } => {
            let mut gradient = Gradient::new(GradientKind::Linear, *c0, *c1);
            gradient.from = *from;
            gradient.to = *to;
            let placed = gradient_in_bounds(&gradient, before, after)?;
            Fill::Linear {
                from: placed.from,
                to: placed.to,
                c0: *c0,
                c1: *c1,
            }
        }
        Fill::Radial { c0, c1 } => {
            let mut gradient = Gradient::new(GradientKind::Radial, *c0, *c1);
            let radius = before.width().max(before.height()) * 0.5;
            gradient.to = [0.5 + radius / before.width().max(0.001), 0.5];
            Fill::Gradient(gradient_in_bounds(&gradient, before, after)?)
        }
        Fill::Gradient(gradient) => Fill::Gradient(gradient_in_bounds(gradient, before, after)?),
        Fill::Pattern(pattern) => {
            let old = tiny_skia::Transform::from_translate(before.min.x, before.min.y)
                .pre_scale(before.width().max(0.001), before.height().max(0.001));
            let new = tiny_skia::Transform::from_translate(after.min.x, after.min.y)
                .pre_scale(after.width().max(0.001), after.height().max(0.001));
            let placement = new
                .invert()
                .ok_or("Outline paint bounds are singular")?
                .pre_concat(old)
                .pre_concat(crate::svg_source::transform(pattern.transform));
            let mut result = pattern.clone();
            result.transform = [
                placement.sx,
                placement.ky,
                placement.kx,
                placement.sy,
                placement.tx,
                placement.ty,
            ];
            Fill::Pattern(result)
        }
    })
}

fn path_child(root: &Shape, geom: Geom, style: Style, label: &str) -> Shape {
    let mut child = Shape::new(geom, style);
    child.name = format!("{} {label}", root.name);
    child.layout = FrameLayout {
        parent: Some(root.id),
        constraint_x: Constraint::Scale,
        constraint_y: Constraint::Scale,
        absolute: true,
        ..Default::default()
    };
    child
}

struct Painted {
    fill: Fill,
    stroke: Option<Stroke>,
    geom: Geom,
}

enum Draft {
    Join {
        fill: Fill,
        stroke: Option<Stroke>,
        winding: bool,
        builder: tiny_skia::PathBuilder,
    },
    Painted(Painted),
}

fn close_fill(mut geom: Geom) -> Geom {
    match &mut geom {
        Geom::Path { closed, .. } => *closed = true,
        Geom::Paths { paths, .. } => paths.iter_mut().for_each(|path| path.closed = true),
        _ => {}
    }
    geom
}

fn joinable(fill: &Fill, stroke: &Option<Stroke>) -> bool {
    matches!(fill, Fill::None | Fill::Solid(_))
        && stroke
            .as_ref()
            .is_none_or(|stroke| stroke.gradient.is_none())
}

/// Convert one SVG paint server into the fill stored on an outline path.
/// Args: paint, combined opacity, path transform, path bounds and retained definitions.
/// Returns the document fill, or the named refusal when the paint cannot be kept.
fn svg_fill(
    paint: &usvg::Paint,
    alpha: f32,
    transform: tiny_skia::Transform,
    bounds: Bounds,
    definitions: &str,
) -> Result<Fill, String> {
    if let usvg::Paint::Pattern(pattern) = paint {
        let id = pattern.id().to_string();
        let kept = crate::svg_source::subset(definitions, &[id.clone()]);
        if !kept.contains(&format!("id=\"{id}\"")) && !kept.contains(&format!("id='{id}'")) {
            return Err(
                "Imported SVG text pattern paint needs its source definition; it was not converted"
                    .into(),
            );
        }
        let placement = tiny_skia::Transform::from_scale(
            1. / bounds.width().max(0.001),
            1. / bounds.height().max(0.001),
        )
        .pre_translate(-bounds.min.x, -bounds.min.y)
        .pre_concat(transform)
        .pre_concat(pattern.transform());
        return Ok(Fill::Pattern(crate::svg_source::SvgPattern {
            definitions: kept.into(),
            id,
            transform: [
                placement.sx,
                placement.ky,
                placement.kx,
                placement.sy,
                placement.tx,
                placement.ty,
            ],
            opacity: alpha,
        }));
    }
    crate::formats::svg::paint(paint, alpha, transform, bounds).ok_or_else(|| {
        "Imported SVG text paint needs a shared outline paint space; it was not converted".into()
    })
}

fn push_joined(
    drafts: &mut Vec<Draft>,
    fill: Fill,
    stroke: Option<Stroke>,
    winding: bool,
    path: &tiny_skia::Path,
) {
    if let Some(Draft::Join {
        fill: old_fill,
        stroke: old_stroke,
        winding: old_winding,
        builder,
    }) = drafts.last_mut()
    {
        if *old_fill == fill && *old_stroke == stroke && *old_winding == winding {
            builder.push_path(path);
            return;
        }
    }
    let mut builder = tiny_skia::PathBuilder::new();
    builder.push_path(path);
    drafts.push(Draft::Join {
        fill,
        stroke,
        winding,
        builder,
    });
}

fn push_paint(
    drafts: &mut Vec<Draft>,
    fill: Fill,
    mut stroke: Option<Stroke>,
    winding: bool,
    path: &tiny_skia::Path,
    geom: Geom,
    bounds: Bounds,
) -> Result<(), String> {
    if joinable(&fill, &stroke) && winding {
        push_joined(drafts, fill, stroke, winding, path);
        return Ok(());
    }
    let fill = if fill
        .gradient()
        .is_some_and(|gradient| gradient.kind == GradientKind::Shape)
    {
        fill
    } else {
        fill_in_bounds(&fill, bounds, geom.bbox())?
    };
    if let Some(stroke) = &mut stroke {
        if let Some(gradient) = stroke.gradient.clone() {
            stroke.gradient = Some(gradient_in_bounds(&gradient, bounds, geom.bbox())?);
        }
    }
    let geom = if fill.is_none() {
        geom
    } else {
        close_fill(geom)
    };
    drafts.push(Draft::Painted(Painted { fill, stroke, geom }));
    Ok(())
}

fn outlined_stroke(
    path: &usvg::Path,
    stroke: &usvg::Stroke,
    opacity: f32,
    transform: tiny_skia::Transform,
    definitions: &str,
) -> Result<Draft, String> {
    let scale = tiny_skia::PathStroker::compute_resolution_scale(&transform);
    let settings = stroke.to_tiny_skia();
    let dashed = settings
        .dash
        .as_ref()
        .map(|dash| path.data().dash(dash, scale));
    let source = match &dashed {
        Some(Some(path)) => path,
        Some(None) => {
            return Err("Imported SVG text stroke could not be converted".into());
        }
        None => path.data(),
    };
    let Some(outline) = source
        .stroke(&settings, scale)
        .and_then(|outline| outline.transform(transform))
    else {
        return Err("Imported SVG text stroke could not be converted".into());
    };
    let Some(geom) = crate::geom::path_geom(&outline, true) else {
        return Err("Imported SVG text stroke could not be converted".into());
    };
    let geom = close_fill(geom);
    let paint = svg_fill(
        stroke.paint(),
        stroke.opacity().get() * opacity,
        transform,
        geom.bbox(),
        definitions,
    )?;
    Ok(Draft::Painted(Painted {
        fill: paint,
        stroke: None,
        geom,
    }))
}

fn stroke_style(
    stroke: &usvg::Stroke,
    opacity: f32,
    transform: tiny_skia::Transform,
    bounds: Bounds,
    definitions: &str,
) -> Result<Stroke, String> {
    let (sx, _) = transform.get_scale();
    let paint = svg_fill(
        stroke.paint(),
        stroke.opacity().get() * opacity,
        transform,
        bounds,
        definitions,
    )?;
    Ok(Stroke {
        gradient: paint.gradient(),
        color: paint.solid_or(Rgba::BLACK),
        width: stroke.width().get() * sx,
        cap: match stroke.linecap() {
            usvg::LineCap::Butt => Cap::Butt,
            usvg::LineCap::Round => Cap::Round,
            usvg::LineCap::Square => Cap::Square,
        },
        join: match stroke.linejoin() {
            usvg::LineJoin::Round => Join::Round,
            usvg::LineJoin::Bevel => Join::Bevel,
            _ => Join::Miter,
        },
        dash: stroke
            .dasharray()
            .filter(|dash| dash.len() == 2)
            .map(|dash| (dash[0] * sx, dash[1] * sx)),
        dash_offset: stroke.dashoffset() * sx,
        ..Default::default()
    })
}

fn add_svg_stroke(
    path: &usvg::Path,
    opacity: f32,
    transform: tiny_skia::Transform,
    transformed: &tiny_skia::Path,
    geom: &Geom,
    winding: bool,
    definitions: &str,
    drafts: &mut Vec<Draft>,
) -> Result<(), String> {
    let Some(stroke) = path.stroke() else {
        return Ok(());
    };
    let (sx, sy) = transform.get_scale();
    let axes = transform.sx * transform.kx + transform.ky * transform.sy;
    let complex = matches!(stroke.paint(), usvg::Paint::Pattern(_))
        || (sx - sy).abs() > 0.01
        || axes.abs() > 0.0001 * (sx * sy).max(1.)
        || stroke.linejoin() == usvg::LineJoin::MiterClip
        || stroke.linejoin() == usvg::LineJoin::Miter
            && (stroke.miterlimit().get() - 4.).abs() > 0.001
        || stroke.dasharray().is_some_and(|dash| dash.len() != 2);
    if complex {
        drafts.push(outlined_stroke(
            path,
            stroke,
            opacity,
            transform,
            definitions,
        )?);
        return Ok(());
    }
    push_paint(
        drafts,
        Fill::None,
        Some(stroke_style(
            stroke,
            opacity,
            transform,
            geom.bbox(),
            definitions,
        )?),
        winding,
        transformed,
        geom.clone(),
        geom.bbox(),
    )
}

fn collect_path(
    path: &usvg::Path,
    opacity: f32,
    definitions: &str,
    inherited: Option<&InheritedDistance>,
    drafts: &mut Vec<Draft>,
    coverage: &mut tiny_skia::PathBuilder,
) -> Result<(), String> {
    if !path.is_visible() {
        return Ok(());
    }
    let transform = path.abs_transform();
    let Some(transformed) = path.data().clone().transform(transform) else {
        return Err("Imported SVG text has a path that was not converted".into());
    };
    let winding = path
        .fill()
        .is_none_or(|fill| fill.rule() == usvg::FillRule::NonZero);
    let Some(geom) = crate::geom::path_geom(&transformed, winding) else {
        return Ok(());
    };
    coverage.push_path(&transformed);
    let stroke_first = path.paint_order() == usvg::PaintOrder::StrokeAndFill;
    if stroke_first {
        add_svg_stroke(
            path,
            opacity,
            transform,
            &transformed,
            &geom,
            winding,
            definitions,
            drafts,
        )?;
    }
    if let Some(fill) = path.fill() {
        let paint = if let Some(inherited) = inherited
            && let usvg::Paint::Pattern(pattern) = fill.paint()
            && pattern.id() == inherited.server
        {
            let mut gradient = inherited.gradient.clone();
            gradient.silhouette = Some(inherited.silhouette.clone());
            Fill::Gradient(gradient)
        } else {
            svg_fill(
                fill.paint(),
                fill.opacity().get() * opacity,
                transform,
                geom.bbox(),
                definitions,
            )?
        };
        push_paint(
            drafts,
            paint,
            None,
            winding,
            &transformed,
            geom.clone(),
            geom.bbox(),
        )?;
    }
    if !stroke_first {
        add_svg_stroke(
            path,
            opacity,
            transform,
            &transformed,
            &geom,
            winding,
            definitions,
            drafts,
        )?;
    }
    if path.fill().is_none() && path.stroke().is_none() {
        push_paint(
            drafts,
            Fill::None,
            None,
            winding,
            &transformed,
            geom,
            Bounds::from_pt(Pt::ZERO),
        )?;
    }
    Ok(())
}

fn collect_group(
    group: &usvg::Group,
    opacity: f32,
    definitions: &str,
    inherited: Option<&InheritedDistance>,
    drafts: &mut Vec<Draft>,
    coverage: &mut tiny_skia::PathBuilder,
    depth: usize,
) -> Result<(), String> {
    if depth > 64 {
        return Err("Imported SVG text nesting exceeds 64 levels; it was not converted".into());
    }
    if group.clip_path().is_some()
        || group.mask().is_some()
        || !group.filters().is_empty()
        || group.blend_mode() != usvg::BlendMode::Normal
    {
        return Err(
            "Imported SVG text clips, masks, filters or blends need an outline appearance adapter; it was not converted"
                .into(),
        );
    }
    let opacity = opacity * group.opacity().get();
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => collect_group(
                child,
                opacity,
                definitions,
                inherited,
                drafts,
                coverage,
                depth + 1,
            )?,
            usvg::Node::Text(text) => collect_group(
                text.flattened(),
                opacity,
                definitions,
                inherited,
                drafts,
                coverage,
                depth + 1,
            )?,
            usvg::Node::Path(path) => {
                collect_path(path, opacity, definitions, inherited, drafts, coverage)?
            }
            usvg::Node::Image(_) => {
                return Err(
                    "Imported SVG text images need an outline appearance adapter; it was not converted"
                        .into(),
                );
            }
        }
    }
    Ok(())
}

fn seal(drafts: Vec<Draft>) -> Result<Vec<Painted>, String> {
    let mut painted = Vec::new();
    for draft in drafts {
        match draft {
            Draft::Painted(piece) => painted.push(piece),
            Draft::Join {
                fill,
                stroke,
                winding,
                builder,
            } => {
                let path = builder
                    .finish()
                    .ok_or("This text has no visible outlines to convert")?;
                let geom = crate::geom::path_geom(&path, winding)
                    .ok_or("This text has no visible outlines to convert")?;
                let geom = if fill.is_none() {
                    geom
                } else {
                    close_fill(geom)
                };
                painted.push(Painted { fill, stroke, geom });
            }
        }
    }
    Ok(painted)
}

/// Contours the imported distance field measures.
/// Args: the imported run. Returns those contours, or nothing when the story has no outline.
fn story_silhouette(run: &TypeRun) -> Option<crate::gradient::DistanceSilhouette> {
    if run.contours.is_empty() {
        return None;
    }
    crate::gradient::DistanceSilhouette::from_geom(
        &Geom::Poly {
            contours: run.contours.clone(),
            winding: true,
        }
        .to_path(),
    )
}

/// Object shape-distance fill inherited by imported glyphs.
struct InheritedDistance {
    server: String,
    gradient: Gradient,
    silhouette: crate::gradient::DistanceSilhouette,
}

/// Capture an imported object's shape-distance fill when the story can share it.
/// Args: the text and its run. Returns that fill, nothing when the object fill is not shape-distance, or the named refusal.
fn inherited_distance(shape: &Shape, run: &TypeRun) -> Result<Option<InheritedDistance>, String> {
    let Some(gradient) = shape
        .style
        .fill
        .gradient()
        .filter(|gradient| gradient.kind == GradientKind::Shape)
    else {
        return Ok(None);
    };
    let silhouette = story_silhouette(run).ok_or(
        "Shape-distance text paint needs a shared outline paint space; it was not converted",
    )?;
    Ok(Some(InheritedDistance {
        server: crate::text::svg_type::paint_server_id(&shape.style.fill),
        gradient,
        silhouette,
    }))
}

/// Convert imported SVG text into editable paint paths.
/// Args: the selected text shape and its run. Returns replacement shapes, or the named refusal.
fn svg_outlines(shape: &Shape, run: &TypeRun) -> Result<Vec<Shape>, String> {
    if !shape.text_wrap.is_none() || shape.layout.image.is_some() || shape.layout.frame {
        return Err("Text with contour wrapping, image fill or existing frame children needs an outline appearance adapter; it was not converted".into());
    }
    let painted_run;
    let run = if run
        .svg_text
        .as_ref()
        .is_some_and(|svg| shape.style.fill != svg.base_fill)
    {
        painted_run = crate::text::svg_type::object_paint(run, &shape.style.fill);
        &painted_run
    } else {
        run
    };
    let layout = crate::text::svg_type::layout(run).ok_or_else(|| {
        "Imported SVG text needs its full styled outline tree; it was not converted".to_string()
    })?;
    let definitions = run
        .svg_text
        .as_ref()
        .map(|svg| svg.definitions.as_str())
        .unwrap_or("");
    let inherited = inherited_distance(shape, run)?;
    let mut drafts = Vec::new();
    let mut coverage = tiny_skia::PathBuilder::new();
    collect_group(
        layout.tree.root(),
        1.,
        definitions,
        inherited.as_ref(),
        &mut drafts,
        &mut coverage,
        0,
    )?;
    let mut painted = seal(drafts)?;
    if let Some(mut stroke) = shape
        .style
        .stroke
        .clone()
        .filter(|stroke| stroke.width > 0.)
    {
        let path = coverage
            .finish()
            .ok_or("This text has no visible outlines to convert")?;
        let geom = crate::geom::path_geom(&path, true)
            .ok_or("This text has no visible outlines to convert")?;
        if let Some(gradient) = stroke.gradient.clone()
            && gradient.kind != GradientKind::Shape
        {
            stroke.gradient = Some(gradient_in_bounds(
                &gradient,
                shape.geom.bbox(),
                geom.bbox(),
            )?);
        }
        painted.push(Painted {
            fill: Fill::None,
            stroke: Some(stroke),
            geom,
        });
    }
    if painted.is_empty() {
        return Err("This text has no visible outlines to convert".into());
    }
    if painted.len() == 1 {
        let mut result = shape.clone();
        result.geom = painted[0].geom.clone();
        result.style.fill = painted[0].fill.clone();
        result.style.stroke = painted[0].stroke.clone();
        return Ok(vec![result]);
    }
    let bounds = shape.geom.bbox();
    let mut root = shape.clone();
    root.geom = Geom::Rect {
        origin: bounds.min,
        size: bounds.size(),
        radius: 0.,
    };
    root.style = Style {
        fill: Fill::None,
        stroke: None,
    };
    root.layout.frame = true;
    root.layout.clip = false;
    let mut result = vec![root];
    for (index, piece) in painted.iter().enumerate() {
        let label = if piece.stroke.is_some() && piece.fill.is_none() {
            "stroke".to_string()
        } else {
            format!("paint {}", index + 1)
        };
        result.push(path_child(
            shape,
            piece.geom.clone(),
            Style {
                fill: piece.fill.clone(),
                stroke: piece.stroke.clone(),
            },
            &label,
        ));
    }
    Ok(result)
}

fn prepare(shape: &Shape) -> Result<Vec<Shape>, String> {
    let Geom::Text(run) = &shape.geom else {
        return Err("Select editable text to outline".into());
    };
    if run.svg_text.is_some() {
        return svg_outlines(shape, run);
    }
    if run.contours.is_empty() {
        return Err("This text has no visible outlines to convert".into());
    }
    if !shape.text_wrap.is_none() || shape.layout.image.is_some() || shape.layout.frame {
        return Err("Text with contour wrapping, image fill or existing frame children needs an outline appearance adapter; it was not converted".into());
    }
    let geometry = crate::text::outline_path(run)
        .and_then(|path| crate::geom::path_geom(&path, false))
        .unwrap_or_else(|| Geom::Poly {
            contours: run.contours.clone(),
            winding: false,
        });
    let painted = crate::text::has_character_paint(run);
    if !painted && run.frame.is_none() && run.on_path.is_none() {
        let mut result = shape.clone();
        result.geom = geometry;
        return Ok(vec![result]);
    }
    let bounds = shape.geom.bbox();
    let mut root = shape.clone();
    root.geom = Geom::Rect {
        origin: bounds.min,
        size: bounds.size(),
        radius: 0.,
    };
    root.style = Style {
        fill: Fill::None,
        stroke: None,
    };
    root.layout.frame = true;
    root.layout.clip = false;
    let mut result = vec![root];
    let fills = if painted {
        crate::text::paint_paths(run, &shape.style.fill)
            .into_iter()
            .filter_map(|(fill, path)| crate::geom::path_geom(&path, true).map(|geom| (fill, geom)))
            .collect::<Vec<_>>()
    } else {
        vec![(shape.style.fill.clone(), geometry.clone())]
    };
    for (index, (fill, geom)) in fills.into_iter().enumerate() {
        let fill = if fill
            .gradient()
            .is_some_and(|gradient| gradient.kind == GradientKind::Shape)
        {
            if run.frame.is_some() || run.on_path.is_some() {
                return Err(
                    "Shape-distance text paint needs a shared outline paint space; it was not converted"
                        .into(),
                );
            }
            let mut gradient = fill.gradient().unwrap();
            gradient.silhouette = Some(crate::gradient::DistanceSilhouette::from_geom(&geometry).ok_or(
                "Shape-distance text paint needs a shared outline paint space; it was not converted",
            )?);
            Fill::Gradient(gradient)
        } else {
            fill_in_bounds(&fill, bounds, geom.bbox())?
        };
        result.push(path_child(
            shape,
            geom,
            Style { fill, stroke: None },
            &format!("paint {}", index + 1),
        ));
    }
    if let Some(mut stroke) = shape
        .style
        .stroke
        .clone()
        .filter(|stroke| stroke.width > 0.)
    {
        if let Some(gradient) = &stroke.gradient {
            if gradient.kind == GradientKind::Shape {
                if run.frame.is_some() || run.on_path.is_some() {
                    return Err(
                        "Shape-distance text paint needs a shared outline paint space; it was not converted"
                            .into(),
                    );
                }
            } else {
                stroke.gradient = Some(gradient_in_bounds(gradient, bounds, geometry.bbox())?);
            }
        }
        result.push(path_child(
            shape,
            geometry,
            Style {
                fill: Fill::None,
                stroke: Some(stroke),
            },
            "stroke",
        ));
    }
    Ok(result)
}

/// Tell a paint channel from a transform or opacity channel.
/// Args: motion property. Returns true when outlining must move that channel onto the paint that draws it.
fn paint_channel(prop: crate::motion::Prop) -> bool {
    use crate::motion::Prop;
    matches!(
        prop,
        Prop::StrokeReveal
            | Prop::FillReveal
            | Prop::GradientAngle
            | Prop::StrokeWidth
            | Prop::Fill
            | Prop::Dash
            | Prop::Gap
            | Prop::DashLength
    )
}

/// Authored fill identity after outline rebases gradient endpoints.
/// Args: outlined fill and the text object's fill. Returns true when they are the same paint.
fn same_authored_fill(child: &Fill, object: &Fill) -> bool {
    match (child.gradient(), object.gradient()) {
        (Some(child_gradient), Some(object_gradient)) => {
            child_gradient.kind == object_gradient.kind
                && child_gradient.stops == object_gradient.stops
        }
        (None, None) => match (child, object) {
            (Fill::Solid(child), Fill::Solid(object)) => child == object,
            (Fill::None, Fill::None) => true,
            (Fill::Pattern(child), Fill::Pattern(object)) => {
                child.id == object.id && child.definitions == object.definitions
            }
            _ => false,
        },
        _ => false,
    }
}

/// Choose the outlined paths that still draw one paint channel.
/// Args: channel, authored fill and stroke, and the paths that replaced the text. Returns their ids.
fn paint_targets(
    prop: crate::motion::Prop,
    object_fill: &Fill,
    object_stroke: Option<&Stroke>,
    children: &[&Shape],
) -> Vec<u64> {
    use crate::motion::Prop;
    match prop {
        Prop::Fill => children
            .iter()
            .filter(|child| same_authored_fill(&child.style.fill, object_fill))
            .map(|child| child.id)
            .collect(),
        Prop::FillReveal => children
            .iter()
            .filter(|child| {
                !child.style.fill.is_none() || same_authored_fill(&child.style.fill, object_fill)
            })
            .map(|child| child.id)
            .collect(),
        Prop::GradientAngle => {
            let mut ids: Vec<u64> = children
                .iter()
                .filter(|child| {
                    object_fill.gradient().is_some()
                        && same_authored_fill(&child.style.fill, object_fill)
                })
                .map(|child| child.id)
                .collect();
            if object_stroke
                .and_then(|stroke| stroke.gradient.as_ref())
                .is_some()
            {
                ids.extend(
                    children
                        .iter()
                        .filter(|child| {
                            child
                                .style
                                .stroke
                                .as_ref()
                                .and_then(|stroke| stroke.gradient.as_ref())
                                .is_some()
                        })
                        .map(|child| child.id),
                );
            }
            ids.sort_unstable();
            ids.dedup();
            ids
        }
        Prop::StrokeReveal | Prop::StrokeWidth | Prop::Dash | Prop::Gap | Prop::DashLength => {
            if object_stroke.is_none() {
                return Vec::new();
            }
            children
                .iter()
                .filter(|child| child.style.stroke.is_some())
                .map(|child| child.id)
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Whether this text animates a fill reveal.
/// Args: motion and the text id. Returns true when a reveal track or an enabled reveal expression is present.
fn fill_reveal_animated(motion: &crate::motion::Motion, source: u64) -> bool {
    motion.tracks.iter().any(|track| {
        track.shape == source
            && track.prop == crate::motion::Prop::FillReveal
            && !track.keys.is_empty()
    }) || motion.expressions.iter().any(|expression| {
        expression.shape == source
            && expression.enabled
            && expression.prop == crate::motion::Prop::FillReveal
    })
}

/// Closed rectangle used only to share one fill-reveal bound.
/// Args: bounds and whether to walk the corners backwards. Returns one closed contour.
fn bounds_contour(bounds: Bounds, reverse: bool) -> crate::geom::PathContour {
    let mut corners = [
        bounds.min,
        Pt::new(bounds.max.x, bounds.min.y),
        bounds.max,
        Pt::new(bounds.min.x, bounds.max.y),
    ];
    if reverse {
        corners.reverse();
    }
    crate::geom::PathContour {
        anchors: corners
            .into_iter()
            .map(crate::geom::Anchor::corner)
            .collect(),
        closed: true,
    }
}

/// Whether `inner` already reaches `outer`.
/// Args: candidate bounds and the text block. Returns true when no extra contour is needed.
fn bounds_cover(inner: Bounds, outer: Bounds) -> bool {
    inner.min.x <= outer.min.x + 0.05
        && inner.min.y <= outer.min.y + 0.05
        && inner.max.x >= outer.max.x - 0.05
        && inner.max.y >= outer.max.y - 0.05
}

/// Give a filled outline the text block's bounds without painting that rectangle.
/// Args: outline geometry and the text bounds. Opposite copies cancel, so the ink stays put.
fn extend_fill_bounds(geom: &mut Geom, bounds: Bounds) {
    if bounds_cover(geom.bbox(), bounds) {
        return;
    }
    match geom {
        Geom::Paths { paths, winding } => {
            if *winding {
                paths.push(bounds_contour(bounds, false));
                paths.push(bounds_contour(bounds, true));
            } else {
                let contour = bounds_contour(bounds, false);
                paths.push(contour.clone());
                paths.push(contour);
            }
        }
        Geom::Path { anchors, closed } => {
            let existing = crate::geom::PathContour {
                anchors: std::mem::take(anchors),
                closed: *closed,
            };
            let contour = bounds_contour(bounds, false);
            *geom = Geom::Paths {
                winding: false,
                paths: vec![existing, contour.clone(), contour],
            };
        }
        Geom::Poly { contours, winding } => {
            let points = |reverse: bool| {
                let mut corners = vec![
                    bounds.min,
                    Pt::new(bounds.max.x, bounds.min.y),
                    bounds.max,
                    Pt::new(bounds.min.x, bounds.max.y),
                ];
                if reverse {
                    corners.reverse();
                }
                corners
            };
            if *winding {
                contours.push(points(false));
                contours.push(points(true));
            } else {
                let contour = points(false);
                contours.push(contour.clone());
                contours.push(contour);
            }
        }
        _ => {}
    }
}

/// Share the text block's reveal bounds across solid fills.
/// Args: replacement shapes, frame first, and the authored fill. Solid fills grow to the frame; strokes do not.
fn pin_shared_fill_reveal(shapes: &mut [Shape], object_fill: &Fill) {
    if shapes.len() < 2 {
        return;
    }
    let bounds = shapes[0].geom.bbox();
    if bounds.width() < 1e-3 || bounds.height() < 1e-3 {
        return;
    }
    for child in shapes.iter_mut().skip(1) {
        let pin = match &child.style.fill {
            Fill::Solid(_) => true,
            Fill::None => same_authored_fill(&child.style.fill, object_fill),
            _ => false,
        };
        if pin {
            extend_fill_bounds(&mut child.geom, bounds);
        }
    }
}

/// Move paint tracks onto the outlines that still draw them.
/// Args: motion, text id, authored fill and stroke, and the replacement shapes. Transform tracks stay on the text id.
fn retarget_outline_motion(
    motion: &crate::motion::Motion,
    source: u64,
    object_fill: &Fill,
    object_stroke: Option<&Stroke>,
    outlined: &[Shape],
) -> crate::motion::Motion {
    let animated =
        motion.tracks.iter().any(|track| {
            track.shape == source && !track.keys.is_empty() && paint_channel(track.prop)
        }) || motion.expressions.iter().any(|expression| {
            expression.shape == source && expression.enabled && paint_channel(expression.prop)
        });
    if !animated || (outlined.len() == 1 && outlined[0].id == source) {
        return motion.clone();
    }
    let children: Vec<&Shape> = outlined.iter().filter(|shape| shape.id != source).collect();
    let mut after = motion.clone();
    let tracks: Vec<_> = after
        .tracks
        .iter()
        .filter(|track| {
            track.shape == source && !track.keys.is_empty() && paint_channel(track.prop)
        })
        .cloned()
        .collect();
    let expressions: Vec<_> = after
        .expressions
        .iter()
        .filter(|expression| {
            expression.shape == source && expression.enabled && paint_channel(expression.prop)
        })
        .cloned()
        .collect();
    after.tracks.retain(|track| {
        !(track.shape == source && !track.keys.is_empty() && paint_channel(track.prop))
    });
    after.expressions.retain(|expression| {
        !(expression.shape == source && expression.enabled && paint_channel(expression.prop))
    });
    for track in &tracks {
        for id in paint_targets(track.prop, object_fill, object_stroke, &children) {
            let mut moved = track.clone();
            moved.shape = id;
            after.tracks.push(moved);
        }
    }
    for expression in &expressions {
        for id in paint_targets(expression.prop, object_fill, object_stroke, &children) {
            let mut moved = expression.clone();
            moved.shape = id;
            after.expressions.push(moved);
        }
    }
    after
}

impl Studio {
    fn outline_text_hits(&self) -> Vec<(usize, u64)> {
        let mut hits = Vec::new();
        for &(layer, id) in &self.selection {
            for id in std::iter::once(id).chain(crate::layout::descendants(&self.doc, layer, id)) {
                if !hits.contains(&(layer, id))
                    && self
                        .doc
                        .find_shape(layer, id)
                        .is_some_and(|s| matches!(s.geom, Geom::Text(_)))
                {
                    hits.push((layer, id));
                }
            }
        }
        hits
    }

    fn outline_text_editable(&self, hit: (usize, u64)) -> bool {
        if !self.text_structure_editable(hit) {
            return false;
        }
        let mut current = self
            .doc
            .find_shape(hit.0, hit.1)
            .and_then(|s| s.layout.parent);
        let mut seen = HashSet::new();
        while let Some(id) = current {
            if !seen.insert(id) || seen.len() > 64 {
                return false;
            }
            let Some(parent) = self.doc.find_shape(hit.0, id) else {
                return false;
            };
            if !parent.visible || parent.locked {
                return false;
            }
            current = parent.layout.parent;
        }
        true
    }

    /// Check whether the selected roots contain writable editable text.
    /// Args: none. Returns false for missing, hidden, locked or busy targets and stories.
    pub(crate) fn has_outline_text_targets(&self) -> bool {
        let hits = self.outline_text_hits();
        !hits.is_empty() && hits.iter().all(|&hit| self.outline_text_editable(hit))
    }

    /// Convert selected native text to real editable paths in one Undo operation.
    /// Args: none. Non-text roots stay unchanged; nested text uses its existing parent.
    pub(crate) fn create_text_outlines(&mut self) {
        let hits = self.outline_text_hits();
        self.outline_text_targets(&hits);
    }

    /// Apply preflighted outline plans without changing persistent object or layer identities.
    /// Args: concrete text targets. Returns after one ordinary command transaction or a named refusal.
    pub(super) fn outline_text_targets(&mut self, hits: &[(usize, u64)]) {
        if hits.is_empty() {
            self.status = "Select editable text to outline".into();
            return;
        }
        let mut plans = Vec::new();
        for &hit in hits {
            if !self.outline_text_editable(hit) {
                self.status =
                    "Unlock the text, its parents and its whole story before outlining".into();
                return;
            }
            let shape = self.doc.find_shape(hit.0, hit.1).unwrap();
            match prepare(shape) {
                Ok(shapes) => plans.push(Outline { hit, shapes }),
                Err(error) => {
                    self.status = error;
                    return;
                }
            }
        }
        self.commit_type_edit();
        self.text_structure_transaction(|studio| {
            let motion_before = studio.doc.motion.clone();
            let mut motion_after = motion_before.clone();
            for plan in plans {
                let selected = studio.selection.iter().position(|hit| *hit == plan.hit);
                if selected.is_none() && studio.actions.depth == 0 {
                    studio
                        .actions
                        .pause("Nested text outlining is outside the recorded selection");
                }
                let hit = plan.hit;
                let (object_fill, object_stroke) = studio
                    .doc
                    .find_shape(hit.0, hit.1)
                    .map(|shape| (shape.style.fill.clone(), shape.style.stroke.clone()))
                    .unwrap_or((Fill::None, None));
                let mut shapes = plan.shapes;
                if fill_reveal_animated(&motion_before, hit.1) {
                    pin_shared_fill_reveal(&mut shapes, &object_fill);
                }
                motion_after = retarget_outline_motion(
                    &motion_after,
                    hit.1,
                    &object_fill,
                    object_stroke.as_ref(),
                    &shapes,
                );
                studio.with_action_step(crate::actions::Step::Outline { selected }, |studio| {
                    let previous =
                        studio
                            .doc
                            .find_shape(hit.0, hit.1)
                            .and_then(|shape| match &shape.geom {
                                Geom::Text(run) => {
                                    run.thread.as_ref().and_then(|thread| thread.prev)
                                }
                                _ => None,
                            });
                    if let Some(previous) = previous {
                        if let Some(layer) = studio.doc.layers.iter().position(|layer| {
                            layer
                                .kind
                                .shapes()
                                .unwrap_or(&[])
                                .iter()
                                .any(|shape| shape.id == previous)
                        }) {
                            studio.break_text_thread((layer, previous));
                        }
                    }
                    studio.break_text_thread(hit);
                    let before = studio.doc.layers[hit.0].kind.shapes().unwrap().to_vec();
                    let mut after = Vec::with_capacity(before.len() + shapes.len() - 1);
                    for shape in &before {
                        if shape.id == hit.1 {
                            after.extend(shapes.iter().cloned());
                        } else {
                            after.push(shape.clone());
                        }
                    }
                    studio.commit(Cmd::SetVectorShapes {
                        layer: hit.0,
                        before,
                        after,
                    });
                });
            }
            if motion_after != motion_before {
                studio.commit(Cmd::SetMotion {
                    before: motion_before,
                    after: motion_after,
                });
            }
        });
        self.node_sel.clear();
        self.reset_snap_gesture();
        self.status = format!(
            "{} text object(s) outlined · Undo restores editable text",
            hits.len()
        );
    }
}

#[cfg(test)]
#[path = "text_outlines_tests.rs"]
mod tests;
