//! Calculations: blend two scalar channels into a new channel or the selection.
use super::{Studio, channels::Target, masking::{self, SelectionSpace}};
use crate::channels::{self, Channel, Plane};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Blend { Normal, Multiply, Screen, Overlay, Add, Subtract, Difference, Darken, Lighten }
impl Blend {
    pub(crate) const ALL: [Self; 9] = [Self::Normal, Self::Multiply, Self::Screen, Self::Overlay, Self::Add, Self::Subtract, Self::Difference, Self::Darken, Self::Lighten];
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Normal => "Normal", Self::Multiply => "Multiply", Self::Screen => "Screen", Self::Overlay => "Overlay",
            Self::Add => "Add", Self::Subtract => "Subtract", Self::Difference => "Difference", Self::Darken => "Darken", Self::Lighten => "Lighten",
        }
    }
    /// Blend top over bottom, both 0..=1.
    pub(crate) fn apply(self, top: f32, bottom: f32) -> f32 {
        match self {
            Self::Normal => top,
            Self::Multiply => top * bottom,
            Self::Screen => 1. - (1. - top) * (1. - bottom),
            Self::Overlay => if bottom < 0.5 { 2. * top * bottom } else { 1. - 2. * (1. - top) * (1. - bottom) },
            Self::Add => top + bottom,
            Self::Subtract => bottom - top,
            Self::Difference => (top - bottom).abs(),
            Self::Darken => top.min(bottom),
            Self::Lighten => top.max(bottom),
        }
        .clamp(0., 1.)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Spec {
    pub source1: channels::Target,
    pub invert1: bool,
    pub source2: channels::Target,
    pub invert2: bool,
    pub blend: Blend,
    pub opacity: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Output { NewChannel, Selection }

/// Blend two planes in source 2's grid; source 1 is resampled there when placements differ.
/// Result = mix(source2, blend(source1, source2), opacity).
pub(crate) fn calculate(a: &Plane, invert1: bool, b: &Plane, invert2: bool, blend: Blend, opacity: f32) -> Result<Plane, String> {
    if !(0. ..=1.).contains(&opacity) {
        return Err("Opacity must be between 0% and 100%".into());
    }
    a.validate()?;
    b.validate()?;
    let space = |p: &Plane| SelectionSpace { w: p.width, h: p.height, transform: p.placement() };
    let top = if space(a) == space(b) { a.samples.clone() } else { masking::resample_admitted(&a.samples, space(a), space(b))? };
    let values = top.iter().zip(b.samples.iter()).map(|(&t, &u)| {
        let t = if invert1 { 1. - t } else { t };
        let u = if invert2 { 1. - u } else { u };
        u + (blend.apply(t, u) - u) * opacity
    });
    Ok(Plane { samples: crate::deep_color::storage::collect(values)?, ..b.clone() })
}

impl Studio {
    /// Run Calculations on the current document and publish the result.
    /// Args: captured owner, sources/blend and output. A new channel is one Undo step; a selection is not history.
    pub(crate) fn calculate_channels(&mut self, target: &Target, spec: Spec, output: Output) -> Result<Option<u64>, String> {
        if !self.channels_current(target) {
            return Err("The document or selection changed; run Calculations again".into());
        }
        let cancel = crate::deep_color::storage::check;
        let a = channels::render(&self.doc, spec.source1, &cancel)?;
        let b = channels::render(&self.doc, spec.source2, &cancel)?;
        let plane = calculate(&a, spec.invert1, &b, spec.invert2, spec.blend, spec.opacity)?;
        match output {
            Output::Selection => {
                let space = SelectionSpace { w: plane.width, h: plane.height, transform: plane.placement() };
                self.load_channel_selection(target, plane.samples, space)?;
                self.status = format!("Calculations ({}) loaded as selection", spec.blend.name());
                Ok(None)
            }
            Output::NewChannel => {
                let id = crate::document::next_id();
                let name = channels::next_name(&self.doc.channels, "Alpha");
                let mut after = self.doc.channels.clone();
                after.push(Channel { id, name, plane, spot: None });
                self.set_channels(target, after, &format!("Calculations ({})", spec.blend.name()))?;
                Ok(Some(id))
            }
        }
    }
}

#[cfg(test)]
#[path = "calculations_tests.rs"]
mod tests;
