//! Editable artwork references used by layer and object masks.
use crate::document::Document;
use crate::geom::Bounds;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MaskSource {
    pub layer: u64,
    pub shape: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaskMode {
    #[default]
    Alpha,
    Luminance,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtworkMask {
    pub source: MaskSource,
    pub mode: MaskMode,
    pub transform: [f32; 6],
    pub region: Option<Bounds>,
    pub region_transform: [f32; 6],
}

impl ArtworkMask {
    /// Reference editable artwork.
    /// `source` identifies a layer or object. Returns an alpha mask in document space.
    pub fn new(source: MaskSource) -> Self {
        Self {
            source,
            mode: MaskMode::Alpha,
            transform: [1., 0., 0., 1., 0., 0.],
            region: None,
            region_transform: [1., 0., 0., 1., 0., 0.],
        }
    }

    /// Map source artwork into its mask placement.
    /// Returns the stored SVG affine transform.
    pub fn placement(&self) -> tiny_skia::Transform {
        let [a, b, c, d, e, f] = self.transform;
        tiny_skia::Transform::from_row(a, b, c, d, e, f)
    }

    /// Map the SVG mask region into source coordinates.
    /// Returns an affine transform independent of the source's editable geometry.
    pub fn region_placement(&self) -> tiny_skia::Transform {
        let [a, b, c, d, e, f] = self.region_transform;
        tiny_skia::Transform::from_row(a, b, c, d, e, f)
    }

    /// Keep references and placement when copying artwork.
    /// `layers` and `shapes` map copied IDs; `placement` maps the copied source coordinates.
    /// Updates this mask in place and leaves references to uncopied sources unchanged.
    pub fn remap(
        &mut self,
        layers: &HashMap<u64, u64>,
        shapes: &HashMap<u64, u64>,
        placement: tiny_skia::Transform,
    ) {
        let copied = self.source.shape.map_or_else(
            || layers.contains_key(&self.source.layer),
            |id| shapes.contains_key(&id),
        );
        if !copied {
            return;
        }
        self.source.layer = layers
            .get(&self.source.layer)
            .copied()
            .unwrap_or(self.source.layer);
        self.source.shape = self
            .source
            .shape
            .map(|id| shapes.get(&id).copied().unwrap_or(id));
        let transform = placement
            .pre_concat(self.placement())
            .pre_concat(placement.invert().unwrap_or_default());
        self.transform = [
            transform.sx,
            transform.ky,
            transform.kx,
            transform.sy,
            transform.tx,
            transform.ty,
        ];
        let region = placement.pre_concat(self.region_placement());
        self.region_transform = [
            region.sx, region.ky, region.kx, region.sy, region.tx, region.ty,
        ];
    }
}

impl Document {
    /// List artwork used only as mask sources.
    /// Returns source identifiers; the layers panel keeps these items editable.
    pub fn mask_sources(&self) -> HashSet<MaskSource> {
        self.layers
            .iter()
            .flat_map(|layer| {
                layer.artwork_masks.iter().chain(
                    layer
                        .kind
                        .shapes()
                        .unwrap_or(&[])
                        .iter()
                        .flat_map(|s| &s.artwork_masks),
                )
            })
            .map(|mask| mask.source)
            .collect()
    }

    /// Identify artwork hidden from normal painting because it supplies a mask.
    /// `layer` is the layer index and `id` is an optional object. Returns true for source descendants too.
    pub fn is_mask_source(&self, layer: usize, id: Option<u64>) -> bool {
        self.mask_source_member(layer, id, &self.mask_sources())
    }

    pub(crate) fn mask_source_member(
        &self,
        layer: usize,
        id: Option<u64>,
        sources: &HashSet<MaskSource>,
    ) -> bool {
        if sources.is_empty() {
            return false;
        }
        let Some(item) = self.layers.get(layer) else {
            return false;
        };
        if sources.contains(&MaskSource {
            layer: item.id,
            shape: None,
        }) || self.layer_ancestors(layer).iter().any(|&i| {
            sources.contains(&MaskSource {
                layer: self.layers[i].id,
                shape: None,
            })
        }) {
            return true;
        }
        let mut shape = id;
        for _ in 0..64 {
            let Some(id) = shape else {
                break;
            };
            if sources.contains(&MaskSource {
                layer: item.id,
                shape: Some(id),
            }) {
                return true;
            }
            shape = self.find_shape(layer, id).and_then(|s| s.layout.parent);
        }
        false
    }

    /// Check mask dependencies together with layer and frame containment.
    /// Returns an error for cycles, invalid placement, or excessive nesting.
    pub fn validate_artwork_masks(&self) -> Result<(), String> {
        let mut graph: HashMap<MaskSource, Vec<MaskSource>> = HashMap::new();
        for layer in &self.layers {
            let root = MaskSource {
                layer: layer.id,
                shape: None,
            };
            let edges = graph.entry(root).or_default();
            edges.extend(layer.artwork_masks.iter().map(|m| m.source));
            if let Some(parent) = layer.parent {
                graph
                    .entry(MaskSource {
                        layer: parent,
                        shape: None,
                    })
                    .or_default()
                    .push(root);
            }
            for shape in layer.kind.shapes().unwrap_or(&[]) {
                let item = MaskSource {
                    layer: layer.id,
                    shape: Some(shape.id),
                };
                graph
                    .entry(MaskSource {
                        layer: layer.id,
                        shape: shape.layout.parent,
                    })
                    .or_default()
                    .push(item);
                graph
                    .entry(item)
                    .or_default()
                    .extend(shape.artwork_masks.iter().map(|m| m.source));
            }
            for mask in layer.artwork_masks.iter().chain(
                layer
                    .kind
                    .shapes()
                    .unwrap_or(&[])
                    .iter()
                    .flat_map(|s| &s.artwork_masks),
            ) {
                if !mask
                    .transform
                    .iter()
                    .chain(&mask.region_transform)
                    .all(|v| v.is_finite())
                    || mask.region.is_some_and(|r| {
                        ![r.min.x, r.min.y, r.max.x, r.max.y]
                            .iter()
                            .all(|v| v.is_finite())
                    })
                {
                    return Err("Invalid artwork mask placement".into());
                }
            }
        }
        fn visit(
            node: MaskSource,
            graph: &HashMap<MaskSource, Vec<MaskSource>>,
            active: &mut HashSet<MaskSource>,
            done: &mut HashSet<MaskSource>,
        ) -> Result<(), String> {
            if done.contains(&node) {
                return Ok(());
            }
            if active.len() >= 64 || !active.insert(node) {
                return Err("Artwork masks cannot contain themselves or form a cycle".into());
            }
            for &child in graph.get(&node).into_iter().flatten() {
                visit(child, graph, active, done)?;
            }
            active.remove(&node);
            done.insert(node);
            Ok(())
        }
        let mut done = HashSet::new();
        let mut active = HashSet::new();
        for &node in graph.keys() {
            visit(node, &graph, &mut active, &mut done)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
