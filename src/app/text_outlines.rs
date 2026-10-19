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
            let paint = |prop| {
                !matches!(
                    prop,
                    crate::motion::Prop::X
                        | crate::motion::Prop::Y
                        | crate::motion::Prop::Rotation
                        | crate::motion::Prop::Scale
                        | crate::motion::Prop::Width
                        | crate::motion::Prop::Height
                        | crate::motion::Prop::Opacity
                )
            };
            if self
                .doc
                .motion
                .tracks
                .iter()
                .any(|track| track.shape == hit.1 && !track.keys.is_empty() && paint(track.prop))
                || self.doc.motion.expressions.iter().any(|expression| {
                    expression.shape == hit.1 && expression.enabled && paint(expression.prop)
                })
            {
                self.status =
                    "Animated text paint needs an outline animation adapter; it was not converted"
                        .into();
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
            for plan in plans {
                let selected = studio.selection.iter().position(|hit| *hit == plan.hit);
                if selected.is_none() && studio.actions.depth == 0 {
                    studio
                        .actions
                        .pause("Nested text outlining is outside the recorded selection");
                }
                studio.with_action_step(crate::actions::Step::Outline { selected }, |studio| {
                    let previous = studio.doc.find_shape(plan.hit.0, plan.hit.1).and_then(
                        |shape| match &shape.geom {
                            Geom::Text(run) => run.thread.as_ref().and_then(|thread| thread.prev),
                            _ => None,
                        },
                    );
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
                    studio.break_text_thread(plan.hit);
                    let before = studio.doc.layers[plan.hit.0]
                        .kind
                        .shapes()
                        .unwrap()
                        .to_vec();
                    let mut after = Vec::with_capacity(before.len() + plan.shapes.len() - 1);
                    for shape in &before {
                        if shape.id == plan.hit.1 {
                            after.extend(plan.shapes.iter().cloned());
                        } else {
                            after.push(shape.clone());
                        }
                    }
                    studio.commit(Cmd::SetVectorShapes {
                        layer: plan.hit.0,
                        before,
                        after,
                    });
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
