use crate::{color::Blend, document::Document};

/// Find the end of one base and its following clipped siblings.
/// Args: document, painter-ordered siblings and base position. Returns an exclusive sibling position.
pub(crate) fn stack_end(doc: &Document, siblings: &[usize], start: usize) -> usize {
    let mut end = start + 1;
    if !doc.layers[siblings[start]].clipped {
        while end < siblings.len() && doc.layers[siblings[end]].clipped {
            end += 1;
        }
    }
    end
}

/// Blend a clipped source without expanding its base's transparency.
/// Args: straight base/source RGBA and the source's actual blend mode. Returns straight RGBA with unchanged base alpha.
pub(crate) fn atop(base: [f32; 4], source: [f32; 4], blend: Blend) -> [f32; 4] {
    if base[3] <= 0. {
        return [0.; 4];
    }
    let mut opaque = base;
    opaque[3] = 1.;
    let mut result = crate::deep_color::blend_pixel(opaque, source, blend, 1.);
    result[3] = base[3];
    result
}

impl Document {
    /// Resolve a clipped layer against the current sibling order.
    /// Args: concrete layer index. Returns the nearest preceding unclipped sibling, never an ancestor or child.
    pub(crate) fn clipping_base(&self, index: usize) -> Option<usize> {
        let layer = self.layers.get(index)?;
        if !layer.clipped {
            return None;
        }
        (0..index)
            .rev()
            .find(|&i| self.layers[i].parent == layer.parent && !self.layers[i].clipped)
    }

    /// Visit clipping constraints on an item and its containing layer groups.
    /// Args: concrete layer index. Returns current base indices from the item outward.
    pub(crate) fn clipping_bases(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::once(index)
            .chain(self.layer_ancestors(index))
            .filter_map(|i| self.clipping_base(i))
    }

    /// Refuse a writer that has not implemented editable layer clipping.
    /// Args: human-readable writer name. Returns a named refusal before output or success for ordinary documents.
    pub(crate) fn require_no_layer_clipping(&self, writer: &str) -> Result<(), String> {
        if crate::motion::compositions::documents(self)?
            .iter()
            .any(|doc| doc.layers.iter().any(|l| l.clipped))
        {
            Err(format!(
                "LayerClippingUnsupported: {writer} cannot retain layer clipping yet. Save .oma or export PSD/PSB, PNG or TIFF."
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "layer_clipping_tests.rs"]
mod tests;
