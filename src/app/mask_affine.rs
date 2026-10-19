//! Exact affine edits of saved alpha channels and the active selection.
//! Only the stored placement changes; coverage samples are shared, never resampled.
use super::{Studio, channels::Target, masking::SelectionSpace};
use tiny_skia::Transform;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaskAffine {
    FlipHorizontal,
    FlipVertical,
    RotateClockwise,
    RotateCounterclockwise,
    Rotate180,
}
impl MaskAffine {
    pub(crate) const ALL: [Self; 5] = [
        Self::FlipHorizontal,
        Self::FlipVertical,
        Self::RotateClockwise,
        Self::RotateCounterclockwise,
        Self::Rotate180,
    ];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::FlipHorizontal => "Flip horizontal",
            Self::FlipVertical => "Flip vertical",
            Self::RotateClockwise => "Rotate 90° clockwise",
            Self::RotateCounterclockwise => "Rotate 90° counterclockwise",
            Self::Rotate180 => "Rotate 180°",
        }
    }
    fn matrix(self) -> Transform {
        match self {
            Self::FlipHorizontal => Transform::from_scale(-1., 1.),
            Self::FlipVertical => Transform::from_scale(1., -1.),
            Self::RotateClockwise => Transform::from_row(0., 1., -1., 0., 0., 0.),
            Self::RotateCounterclockwise => Transform::from_row(0., -1., 1., 0., 0., 0.),
            Self::Rotate180 => Transform::from_scale(-1., -1.),
        }
    }
}

/// Apply an edit about the world-space centre of the covered pixels.
/// Args: placement, grid size, covered grid bounds (whole grid when None) and edit. Returns the new placement.
pub(crate) fn about_bounds(
    placement: Transform,
    width: u32,
    height: u32,
    bounds: Option<(u32, u32, u32, u32)>,
    edit: MaskAffine,
) -> Transform {
    let (x0, y0, x1, y1) = bounds.unwrap_or((0, 0, width, height));
    let mut pivot = [tiny_skia::Point::from_xy((x0 + x1) as f32 / 2., (y0 + y1) as f32 / 2.)];
    placement.map_points(&mut pivot);
    let [pivot] = pivot;
    Transform::from_translate(pivot.x, pivot.y)
        .pre_concat(edit.matrix())
        .pre_concat(Transform::from_translate(-pivot.x, -pivot.y))
        .pre_concat(placement)
}

impl Studio {
    /// Flip or rotate one saved alpha channel as one ordinary Undo step.
    /// Args: captured owner, channel ID and edit. Returns whether the channel changed.
    pub(crate) fn transform_channel(&mut self, target: &Target, id: u64, edit: MaskAffine) -> Result<bool, String> {
        let mut after = self.doc.channels.clone();
        let channel = after.iter_mut().find(|c| c.id == id).ok_or("That channel no longer exists")?;
        let plane = &channel.plane;
        let bounds = crate::paint::selection_bounds(&plane.samples, plane.width, plane.height);
        if bounds.is_none() {
            return Err("The channel has no coverage to transform".into());
        }
        let next = about_bounds(plane.placement(), plane.width, plane.height, bounds, edit);
        if !crate::channels::valid_placement(next) {
            return Err("That transform would make the channel unusable".into());
        }
        channel.plane.transform = [next.sx, next.ky, next.kx, next.sy, next.tx, next.ty];
        self.set_channels(target, after, &format!("Channel: {}", edit.label()))
    }

    /// Flip or rotate the active pixel selection about its covered bounds.
    /// Selections are not history, matching the other selection edits.
    pub(crate) fn transform_pixel_selection(&mut self, edit: MaskAffine) -> bool {
        self.end_pixel_stroke(false);
        if self.capture_channels().is_err() {
            return false;
        }
        let (Some(values), Some(space)) = (self.pixel_sel.clone(), self.pixel_selection_space()) else {
            return false;
        };
        let Some(bounds) = crate::paint::selection_bounds(&values, space.w, space.h) else {
            return false;
        };
        let next = about_bounds(space.transform, space.w, space.h, Some(bounds), edit);
        if !crate::channels::valid_placement(next) {
            return false;
        }
        let done = self.replace_pixel_selection_admitted(values, SelectionSpace { transform: next, ..space });
        if done {
            self.status = format!("Selection: {}", edit.label());
        }
        done
    }
}

#[cfg(test)]
#[path = "mask_affine_tests.rs"]
mod tests;
