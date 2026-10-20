//! The ordinary active pixel selection travels with the saved file. It is
//! written only while saving and moved back into the editor on open, so it
//! never enters history, recovery snapshots or Live.
use super::{Studio, masking::SelectionSpace};
use crate::channels::Plane;

impl Studio {
    /// Plane for the current selection, or None when there is nothing to keep.
    /// Quick Mask already stores the selection, and Live cannot carry it.
    pub(crate) fn active_selection_plane(&self) -> Option<Plane> {
        if self.doc.collaboration.is_some() || self.doc.quick_mask.is_some() {
            return None;
        }
        let values = self.pixel_sel.clone()?;
        let space = self.pixel_selection_space()?;
        Plane::from_coverage(values, space.w, space.h, space.transform).ok()
    }

    /// Move a saved selection from the opened document into the editor.
    /// A damaged or conflicting selection is dropped; the artwork still opens.
    pub(crate) fn restore_active_selection(&mut self) {
        let Some(plane) = self.doc.active_selection.take() else {
            return;
        };
        if self.doc.quick_mask.is_some() || plane.validate().is_err() {
            return;
        }
        let space = SelectionSpace { w: plane.width, h: plane.height, transform: plane.placement() };
        self.replace_pixel_selection_admitted(plane.samples, space);
    }
}

#[cfg(test)]
#[path = "active_selection_tests.rs"]
mod tests;
