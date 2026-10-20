//! Scalar filters on a layer mask, through the shared channel filter engine.
use super::Studio;
use crate::channels::{Plane, filter::Filter};
use crate::document::Cmd;

/// The layer-mask filter choices offered from the mask menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaskFilter {
    SoftenSmall,
    SoftenLarge,
    Harden,
    Contrast,
}
impl MaskFilter {
    pub(crate) const ALL: [Self; 4] = [Self::SoftenSmall, Self::SoftenLarge, Self::Harden, Self::Contrast];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::SoftenSmall => "Gaussian blur 2 px",
            Self::SoftenLarge => "Gaussian blur 8 px",
            Self::Harden => "Threshold 50%",
            Self::Contrast => "Increase contrast",
        }
    }
    pub(crate) fn filter(self) -> Filter {
        match self {
            Self::SoftenSmall => Filter::GaussianBlur { radius: 2. },
            Self::SoftenLarge => Filter::GaussianBlur { radius: 8. },
            Self::Harden => Filter::Threshold { level: 0.5 },
            Self::Contrast => Filter::Levels { black: 0.25, white: 0.75, gamma: 1., output_black: 0., output_white: 1. },
        }
    }
}

impl Studio {
    /// Filter the layer's mask as one Undo step, limited by a matching pixel selection.
    /// Args: layer index and filter. Returns whether the mask changed; refusals leave it untouched.
    pub(crate) fn filter_layer_mask(&mut self, index: usize, filter: &Filter) -> Result<bool, String> {
        self.end_pixel_stroke(false);
        if self.file_dialog_pending() || self.updates.freezing {
            return Err("Wait for the current operation to finish".into());
        }
        let layer = self.doc.layers.get(index).ok_or("That layer no longer exists")?;
        if !self.layer_unlocked(index) || !self.doc.layer_visible(index) {
            return Err("Choose an unlocked, visible layer".into());
        }
        let mask = layer.mask.as_ref().ok_or("The layer has no mask")?;
        let (w, h) = (mask.w, mask.h);
        let n = w as usize * h as usize;
        let samples = crate::deep_color::storage::collect((0..n).map(|i| {
            let p = mask.sample(i);
            ((p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722) * p[3]).clamp(0., 1.)
        }))?;
        let source = Plane { width: w, height: h, transform: [1., 0., 0., 1., 0., 0.], samples };
        let selection = match self.cached_pixel_sel_mask(index) {
            Some(sel) if sel.len() == n => Some(sel.to_vec()),
            Some(_) => return Err("The pixel selection does not match this mask; clear it first".into()),
            None => None,
        };
        let out = crate::channels::filter::apply(&source, filter, selection.as_deref(), &|| Ok(()))?;
        if out.samples.as_slice() == source.samples.as_slice() {
            self.status = "The mask filter made no change".into();
            return Ok(false);
        }
        let mut after = mask.clone();
        if after.native.is_some() {
            let values = crate::deep_color::storage::collect(out.samples.iter().flat_map(|&v| [v, v, v, 1.]))?;
            after.set_sample_allocation(values)?;
        } else {
            let bytes = after.bytes_mut()?;
            for (pixel, &v) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(out.samples.iter()) {
                let v = (v * 255.).round().clamp(0., 255.) as u8;
                pixel.copy_from_slice(&[v, v, v, 255]);
            }
            after.touch();
        }
        let before = layer.mask.clone();
        self.commit(Cmd::SetLayerMask { index, before, after: Some(after) });
        self.status = format!("Mask: {}", filter.name());
        Ok(true)
    }
}

#[cfg(test)]
#[path = "mask_filter_tests.rs"]
mod tests;
