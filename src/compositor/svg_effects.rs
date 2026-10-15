//! Evaluate retained SVG effects against current vector children at viewport scale.
use super::*;
use std::collections::HashSet;

/// Render a group's editable subtree through its source filter graph.
/// Args: document, group index, view transform, target size, motion time and overrides.
/// Returns a viewport-sized surface or a descriptive source error, without allocating a page image.
pub(super) fn render(
    doc: &Document,
    index: usize,
    transform: Transform,
    width: u32,
    height: u32,
    motion: Option<f32>,
    overrides: Option<&HashMap<u64, Pose>>,
) -> Result<Pixmap, String> {
    let group = doc.layers.get(index).ok_or("Missing SVG effect group")?;
    let mut ids = HashSet::from([group.id]);
    for _ in 0..64 {
        let before = ids.len();
        for layer in &doc.layers {
            if layer.parent.is_some_and(|id| ids.contains(&id)) {
                ids.insert(layer.id);
            }
        }
        if ids.len() == before {
            break;
        }
    }
    for source in doc.mask_sources() {
        if let Some((index, _)) = source.resolve(doc) {
            ids.insert(doc.layers[index].id);
            for ancestor in doc.layer_ancestors(index) {
                ids.insert(doc.layers[ancestor].id);
            }
        }
    }
    let mut isolated = doc.clone();
    isolated.layers.retain(|l| ids.contains(&l.id));
    isolated.transparent = true;
    let mut poses = HashMap::new();
    for layer in &mut isolated.layers {
        if layer.id == group.id {
            layer.parent = None;
            layer.opacity = 1.;
            layer.fill_opacity = 1.;
            layer.blend = crate::color::Blend::Normal;
            layer.blend_interior = false;
            layer.filters = Default::default();
            layer.mask = None;
            layer.artwork_masks.clear();
        }
        if let Some(shapes) = layer.kind.shapes_mut() {
            for shape in shapes {
                let pose = pose_of(shape.id, motion, doc, overrides);
                if !pose.is_identity() {
                    posed_paint(shape, pose);
                    if let Some(opacity) = pose.opacity {
                        shape.opacity = opacity;
                    }
                    poses.insert(shape.id, pose);
                }
            }
        }
    }
    let source = crate::svg::export_posed(&isolated, &poses)?;
    let mut options = usvg::Options::default();
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    let tree = usvg::Tree::from_str(&source, &options)
        .map_err(|e| format!("Invalid SVG effect graph: {e}"))?;
    let mut surface = Pixmap::new(width, height).ok_or("Could not allocate SVG effect viewport")?;
    resvg::render(&tree, transform, &mut surface.as_mut());
    Ok(surface)
}
