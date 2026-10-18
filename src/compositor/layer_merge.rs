use super::*;
use crate::{document::Pixels, geom::Bounds, ml::Progress};
use std::collections::HashSet;

fn scope(doc: &Document, roots: &[usize]) -> Result<Document, String> {
    let root_ids: HashSet<_> = roots.iter().map(|&i| doc.layers[i].id).collect();
    let mut included = root_ids.clone();
    loop {
        let before = included.len();
        for layer in &doc.layers {
            if layer.parent.is_some_and(|id| included.contains(&id)) {
                included.insert(layer.id);
            }
            if !included.contains(&layer.id) {
                continue;
            }
            for mask in layer.artwork_masks.iter().chain(
                layer
                    .kind
                    .shapes()
                    .unwrap_or(&[])
                    .iter()
                    .flat_map(|s| &s.artwork_masks),
            ) {
                let index = mask
                    .source
                    .resolve(doc)
                    .ok_or("A merged artwork mask source is missing")?
                    .0;
                included.insert(doc.layers[index].id);
            }
        }
        if included.len() == before {
            break;
        }
    }
    let mut output = doc.clone();
    output.layers.retain(|l| included.contains(&l.id));
    for layer in &mut output.layers {
        if root_ids.contains(&layer.id) || layer.parent.is_some_and(|id| !included.contains(&id)) {
            layer.parent = None;
        }
    }
    output.transparent = true;
    output.motion.compositions.clear();
    output.motion.scene3d = None;
    output.artboards.clear();
    output.artboardless = true;
    output.collaboration = None;
    output.cloud = None;
    Ok(output)
}

fn rounded(bounds: Bounds) -> Result<Bounds, String> {
    let bounds = Bounds {
        min: Pt::new(bounds.min.x.floor(), bounds.min.y.floor()),
        max: Pt::new(bounds.max.x.ceil(), bounds.max.y.ceil()),
    };
    if ![bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y]
        .into_iter()
        .all(f32::is_finite)
        || bounds.width() <= 0.
        || bounds.height() <= 0.
        || f64::from(bounds.width()) >= f64::from(u32::MAX)
        || f64::from(bounds.height()) >= f64::from(u32::MAX)
    {
        return Err("The merged layer has invalid pixel dimensions".into());
    }
    Ok(bounds)
}

fn bounds(
    doc: &Document,
    roots: &[usize],
    scoped: &Document,
    time: f32,
    poses: &HashMap<u64, Pose>,
) -> Result<Bounds, String> {
    let mut bounds: Option<Bounds> = None;
    for &root in roots {
        for (index, layer) in doc
            .layers
            .iter()
            .enumerate()
            .filter(|(i, _)| *i == root || doc.layer_ancestors(*i).contains(&root))
        {
            let mut parents = Vec::new();
            if index != root {
                for parent in doc.layer_ancestors(index) {
                    parents.push(parent);
                    if parent == root {
                        break;
                    }
                }
            }
            if !layer.visible || parents.iter().any(|&i| !doc.layers[i].visible) {
                continue;
            }
            let pad = crate::filter::svg_pad(&layer.filters)
                + parents
                    .iter()
                    .map(|&i| crate::filter::svg_pad(&doc.layers[i].filters))
                    .sum::<f32>();
            if let Some(pixels) = layer.kind.pixels() {
                let pose = pose_of(layer.id, Some(time), doc, Some(poses));
                let placement = pose
                    .to_skia(layer.kind.raster_bounds().unwrap().center())
                    .pre_concat(layer_pixel_transform(layer));
                if let Some(b) = transformed_bounds(
                    Bounds::from_min_size(Pt::ZERO, Pt::new(pixels.w as f32, pixels.h as f32)),
                    placement,
                ) {
                    let b = b.inflate(pad + 2.);
                    bounds = Some(bounds.map_or(b, |old| old.union(b)));
                }
            }
            for shape in layer
                .kind
                .shapes()
                .unwrap_or(&[])
                .iter()
                .filter(|s| s.visible && !s.guide)
            {
                let pose = pose_of(shape.id, Some(time), doc, Some(poses));
                if let Some(b) =
                    shape_screen_bounds(shape, shape_parent_transform(doc, index, shape.id), pose)
                {
                    let b = b.inflate(pad + 2.);
                    bounds = Some(bounds.map_or(b, |old| old.union(b)));
                }
            }
            if layer.svg_filter.is_some() {
                let tree = svg_effects::scene(doc, index, Some(time), Some(poses))?;
                let b = tree.root().abs_layer_bounding_box();
                let b =
                    Bounds::from_min_size(Pt::new(b.x(), b.y()), Pt::new(b.width(), b.height()))
                        .inflate(pad + 2.);
                bounds = Some(bounds.map_or(b, |old| old.union(b)));
            }
        }
    }
    if !crate::deep_color::has_native(scoped)
        && scoped
            .layers
            .iter()
            .any(|l| l.kind.shapes().is_some_and(|s| !s.is_empty()))
    {
        let source = crate::svg::export_posed(scoped, poses)?;
        let tree = crate::svg_source::scene(&source)?;
        let b = tree.root().abs_layer_bounding_box();
        let b = Bounds::from_min_size(Pt::new(b.x(), b.y()), Pt::new(b.width(), b.height()))
            .inflate(2.);
        bounds = Some(bounds.map_or(b, |old| old.union(b)));
    }
    rounded(bounds.unwrap_or(Bounds::from_min_size(Pt::ZERO, Pt::splat(1.))))
}

fn native_scope(mut doc: Document, roots: &HashSet<u64>, origin: Pt) -> Document {
    let translation = Transform::from_translate(-origin.x, -origin.y);
    let layers: HashMap<_, _> = doc.layers.iter().map(|l| (l.id, l.id)).collect();
    let shape_ids: HashMap<_, _> = doc
        .layers
        .iter()
        .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
        .map(|s| (s.id, s.id))
        .collect();
    let keep: HashSet<_> = doc
        .layers
        .iter()
        .enumerate()
        .filter(|(i, l)| {
            roots.contains(&l.id)
                || doc
                    .layer_ancestors(*i)
                    .iter()
                    .any(|&p| roots.contains(&doc.layers[p].id))
        })
        .map(|(_, l)| l.id)
        .collect();
    for layer in &mut doc.layers {
        if !keep.contains(&layer.id) {
            layer.visible = false;
        }
        if roots.contains(&layer.id) {
            layer.parent = None;
        }
        if let Some(shapes) = layer.kind.shapes_mut() {
            for shape in shapes {
                shape.geom.translate(origin * -1.);
                for mask in &mut shape.artwork_masks {
                    mask.remap(&layers, &shape_ids, translation);
                }
            }
        } else if let LayerKind::Raster { origin: point, .. } = &mut layer.kind {
            *point = *point - origin;
        }
        layer.mask_origin = layer.mask_origin - origin;
        for mask in &mut layer.artwork_masks {
            mask.remap(&layers, &shape_ids, translation);
        }
    }
    doc.motion = Default::default();
    doc
}

/// Render selected concrete layer trees through the existing byte or native compositor.
/// Args: immutable document, painter-ordered roots, current frame, optional canvas crop, opaque background and progress/cancellation owner.
/// Returns authoritative pixels plus their document-space origin; uncropped merges keep off-canvas artwork.
pub(crate) fn render(
    doc: &Document,
    roots: &[usize],
    time: f32,
    canvas: bool,
    opaque: bool,
    progress: &dyn Progress,
) -> Result<(Pixels, Pt), String> {
    progress.check()?;
    doc.validate_hierarchy()?;
    doc.validate_artwork_masks()?;
    if roots.is_empty() || roots.iter().any(|&i| i >= doc.layers.len()) {
        return Err("Choose existing layers to merge".into());
    }
    let mut scoped = scope(doc, roots)?;
    let mut keep = || !progress.cancelled();
    let poses = doc
        .motion
        .evaluate_frame(time, &mut keep)
        .map_err(|e| e.to_string())?;
    let area = if canvas {
        rounded(Bounds::from_min_size(
            Pt::ZERO,
            Pt::new(doc.width.round().max(1.), doc.height.round().max(1.)),
        ))?
    } else {
        bounds(doc, roots, &scoped, time, &poses)?
    };
    let (w, h) = (area.width() as u32, area.height() as u32);
    let bytes = crate::deep_color::storage::count(w, h, 4)?;
    let root_ids: HashSet<_> = roots.iter().map(|&i| doc.layers[i].id).collect();
    let pixels = if crate::deep_color::has_native(&scoped) {
        let owners: HashSet<_> = scoped
            .layers
            .iter()
            .flat_map(|l| {
                std::iter::once(l.id).chain(l.kind.shapes().unwrap_or(&[]).iter().map(|s| s.id))
            })
            .collect();
        if scoped
            .motion
            .shapes()
            .into_iter()
            .any(|id| owners.contains(&id) && !doc.motion.pose(id, time).is_identity())
        {
            return Err("Native animated layer merging needs a float pose compositor; no preview pixels were substituted".into());
        }
        let depth = crate::deep_color::document_depth(&scoped);
        scoped = native_scope(scoped, &root_ids, area.min);
        scoped.width = w as f32;
        scoped.height = h as f32;
        scoped.transparent = !opaque;
        progress.report("Rendering native layers", 0, 1);
        let cancel = || progress.check();
        let mut pixels = crate::deep_color::composite_with_cancel(&scoped, 1, &cancel)?;
        if let Some(native) = &mut pixels.native {
            native.depth = depth;
        }
        pixels
    } else {
        admit_precise_export(&scoped)?;
        let _memory = crate::resources::global().reserve_now(bytes as u64, &|| progress.check())?;
        let mut image = Pixmap::new(w, h).ok_or("Could not allocate merged layer pixels")?;
        if opaque {
            image.fill(tiny_skia::Color::WHITE);
        }
        let transform = Transform::from_translate(-area.min.x, -area.min.y);
        let mask_sources = scoped.mask_sources();
        for (done, &root) in roots.iter().enumerate() {
            progress.check()?;
            progress.report("Rendering layers", done, roots.len());
            if mask_sources.contains(&crate::artwork_mask::MaskSource {
                layer: doc.layers[root].id,
                shape: None,
            }) {
                continue;
            }
            groups::draw_item(&mut image, doc, root, transform, Some(time), Some(&poses));
        }
        progress.check()?;
        Pixels::try_from_pixmap(&image)?
    };
    progress.check()?;
    progress.report("Preparing layer history", 1, 1);
    Ok((pixels, area.min))
}
