use super::*;

pub(super) struct Copied {
    pub indices: Vec<usize>,
    pub layers: Vec<Layer>,
    pub layer_ids: HashMap<u64, u64>,
    pub shape_ids: HashMap<u64, u64>,
}

/// Copy animation owners without changing explicit expression source references.
/// Args: original motion, destination motion and copied shape/layer identities. Returns nothing.
pub(super) fn append_motion(
    source: &Motion,
    destination: &mut Motion,
    shape_ids: &HashMap<u64, u64>,
    layer_ids: &HashMap<u64, u64>,
) {
    let mapped = |id| shape_ids.get(&id).or_else(|| layer_ids.get(&id));
    for track in &source.tracks {
        if let Some(&id) = mapped(track.shape) {
            let mut copy = track.clone();
            copy.shape = id;
            destination.tracks.push(copy);
        }
    }
    for expression in &source.expressions {
        if let Some(&id) = mapped(expression.shape) {
            let mut copy = expression.clone();
            copy.shape = id;
            destination.expressions.push(copy);
        }
    }
}

/// Validate a prepared copy against the complete current document.
/// Args: source document and proposed command. Returns an error before any history or artwork changes.
pub(super) fn validate(doc: &Document, command: &Cmd) -> Result<(), String> {
    let mut candidate = doc.clone();
    apply_cmd(&mut candidate, command);
    candidate.validate_hierarchy()?;
    candidate.validate_artwork_masks()?;
    candidate.validate_sources()?;
    crate::layout_components::validate_properties(&candidate)?;
    crate::motion::compositions::validate(&candidate)
}

/// Prepare a layer subtree with fresh identities and existing native copy rules.
/// Args: source document, root index and translation. Returns copied layers and identity maps without changing the document.
pub(super) fn prepare(doc: &Document, root: usize, delta: Pt) -> Result<Copied, String> {
    doc.validate_hierarchy()?;
    if root >= doc.layers.len() || !delta.x.is_finite() || !delta.y.is_finite() {
        return Err("Choose an existing layer to duplicate".into());
    }
    let indices: Vec<_> = (0..=root)
        .filter(|&i| i == root || doc.layer_ancestors(i).contains(&root))
        .collect();
    if indices.windows(2).any(|p| p[1] != p[0] + 1) {
        return Err("This layer group has an inconsistent order and cannot be copied".into());
    }
    let layer_ids: HashMap<_, _> = indices
        .iter()
        .map(|&i| (doc.layers[i].id, crate::document::next_id()))
        .collect();
    let shape_ids: HashMap<_, _> = indices
        .iter()
        .flat_map(|&i| doc.layers[i].kind.shapes().unwrap_or(&[]))
        .map(|s| (s.id, crate::document::next_id()))
        .collect();
    let placement = tiny_skia::Transform::from_translate(delta.x, delta.y);
    let mut layers = Vec::with_capacity(indices.len());
    for &i in &indices {
        let mut layer = doc.layers[i].clone();
        layer.id = layer_ids[&layer.id];
        for mask in &mut layer.artwork_masks {
            mask.remap(&layer_ids, &shape_ids, placement);
        }
        if i == root {
            layer.name = format!("{} copy", layer.name);
        } else {
            layer.parent = layer.parent.and_then(|id| layer_ids.get(&id).copied());
        }
        if let Some(shapes) = layer.kind.shapes_mut() {
            for shape in shapes {
                for mask in &mut shape.artwork_masks {
                    mask.remap(&layer_ids, &shape_ids, placement);
                }
                shape.id = shape_ids[&shape.id];
                crate::layout_components::remap_duplicate(shape, &shape_ids);
                if let Some(crate::layout_components::ComponentBinding::Instance {
                    nodes, ..
                }) = &mut shape.layout.component
                {
                    for node in nodes {
                        for mask in &mut node.baseline.artwork_masks {
                            mask.remap(&layer_ids, &shape_ids, placement);
                        }
                        crate::text_geometry::remap_copy_in_document(
                            doc,
                            &mut node.baseline,
                            &shape_ids,
                        );
                    }
                }
                crate::text_geometry::remap_copy_in_document(doc, shape, &shape_ids);
                shape.layout.parent = shape
                    .layout
                    .parent
                    .and_then(|id| shape_ids.get(&id).copied());
                if delta != Pt::ZERO {
                    shape.geom.translate(delta);
                }
            }
        } else if delta != Pt::ZERO
            && let Some((origin, size, rotation)) = layer.kind.raster_xform()
        {
            layer.kind.set_raster_xform(origin + delta, size, rotation);
        }
        if delta != Pt::ZERO && layer.kind.pixels().is_none() && layer.mask.is_some() {
            if layer.mask_size.x <= 0. || layer.mask_size.y <= 0. {
                let mask = layer.mask.as_ref().unwrap();
                layer.mask_size = Pt::new(mask.w as f32, mask.h as f32);
            }
            layer.mask_origin += delta;
        }
        layers.push(layer);
    }
    Ok(Copied {
        indices,
        layers,
        layer_ids,
        shape_ids,
    })
}

impl Studio {
    /// Check whether the concrete layer can be copied without interrupting another operation.
    /// Args: current layer index. Returns false for missing targets or an active operation; source visibility/locks are preserved.
    pub fn can_duplicate_layer_tree(&self, index: usize) -> bool {
        self.doc.layers.get(index).is_some()
            && self.op.is_none()
            && self.deformation.is_none()
            && self.free_transform.is_none()
            && self.pending_nav.is_none()
            && !self.file_dialog_pending()
            && !self.updates.freezing
    }

    /// Copy the complete concrete layer beside its source.
    /// Args: current layer index. Returns true after one accepted native history transaction.
    pub fn duplicate_layer_tree(&mut self, index: usize) -> bool {
        self.duplicate_layer_tree_by(index, Pt::ZERO)
    }

    pub(super) fn duplicate_layer_tree_by(&mut self, index: usize, delta: Pt) -> bool {
        if !self.can_duplicate_layer_tree(index) {
            return false;
        }
        let copied = match prepare(&self.doc, index, delta) {
            Ok(copied) => copied,
            Err(error) => {
                self.status = error;
                return false;
            }
        };
        let root = copied.layer_ids[&self.doc.layers[index].id];
        let mut commands: Vec<_> = copied
            .layers
            .iter()
            .enumerate()
            .map(|(offset, layer)| Cmd::AddLayer {
                index: index + 1 + offset,
                layer: layer.clone(),
            })
            .collect();
        let mut after = self.doc.motion.clone();
        append_motion(
            &self.doc.motion,
            &mut after,
            &copied.shape_ids,
            &copied.layer_ids,
        );
        if after != self.doc.motion {
            commands.push(Cmd::SetMotion {
                before: self.doc.motion.clone(),
                after,
            });
        }
        let command = Cmd::Batch(commands);
        if let Err(error) = validate(&self.doc, &command) {
            self.status = error;
            return false;
        }
        self.commit_type_edit();
        self.end_deform(false);
        self.reset_snap_gesture();
        self.commit(command);
        self.activate_layer_tree(index + copied.layers.len());
        self.selected_layer = Some(root);
        self.status = format!("Duplicated {}", self.doc.layers[index].name);
        true
    }
}

#[cfg(test)]
#[path = "layer_copy_tests.rs"]
mod tests;
