use super::*;
use crate::artwork_mask::{ArtworkMask, MaskMode};
use std::cell::Cell;

thread_local! { static DEPTH: Cell<usize> = const { Cell::new(0) }; }

struct Depth;
impl Drop for Depth {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

pub(super) fn render(
    masks: &[ArtworkMask],
    doc: &Document,
    view: Transform,
    w: u32,
    h: u32,
    motion: Option<f32>,
    overrides: Option<&HashMap<u64, Pose>>,
) -> Option<tiny_skia::Mask> {
    if masks.is_empty() {
        return None;
    }
    let mut result = tiny_skia::Mask::new(w, h)?;
    if DEPTH.with(|depth| depth.get() >= 64) {
        return Some(result);
    }
    DEPTH.with(|depth| depth.set(depth.get() + 1));
    let _depth = Depth;
    result.data_mut().fill(255);
    for mask in masks {
        let mut image = Pixmap::new(w, h)?;
        let placement = view.pre_concat(mask.placement());
        if let Some(index) = doc.layers.iter().position(|l| l.id == mask.source.layer) {
            if let Some(id) = mask.source.shape {
                frames::draw_item(&mut image, doc, index, id, placement, motion, overrides);
            } else {
                groups::draw_item(&mut image, doc, index, placement, motion, overrides);
            }
        }
        let mut values = tiny_skia::Mask::from_pixmap(
            image.as_ref(),
            match mask.mode {
                MaskMode::Alpha => tiny_skia::MaskType::Alpha,
                MaskMode::Luminance => tiny_skia::MaskType::Luminance,
            },
        );
        if let Some(bounds) = mask.region {
            let mut region = tiny_skia::Mask::new(w, h)?;
            if let Some(rect) = tiny_skia::Rect::from_xywh(
                bounds.min.x,
                bounds.min.y,
                bounds.width(),
                bounds.height(),
            ) {
                region.fill_path(
                    &PathBuilder::from_rect(rect),
                    FillRule::Winding,
                    true,
                    placement.pre_concat(mask.region_placement()),
                );
            }
            multiply(&mut values, &region);
        }
        multiply(&mut result, &values);
    }
    Some(result)
}

pub(super) fn multiply(target: &mut tiny_skia::Mask, other: &tiny_skia::Mask) {
    for (a, b) in target.data_mut().iter_mut().zip(other.data()) {
        *a = ((u16::from(*a) * u16::from(*b) + 127) / 255) as u8;
    }
}
