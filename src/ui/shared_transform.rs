//! Shared-selection fields retain the document's typed batch command path.
use super::fields::{self, FieldState};
use crate::{
    app::Studio,
    document::{Cmd, RASTER_ID},
    geom::{Bounds, Pt},
};
use eframe::egui::Ui;

/// Edit a mixed vector selection without changing untouched properties.
/// Args: panel UI and studio. Returns true when the selection was handled.
pub(super) fn mixed(ui: &mut Ui, studio: &mut Studio) -> bool {
    if studio.selection.len() < 2 {
        return false;
    }
    let shapes: Vec<_> = studio
        .selection
        .iter()
        .filter_map(|&(layer, id)| {
            if id == RASTER_ID {
                return None;
            }
            studio
                .doc
                .find_shape(layer, id)
                .map(|shape| (layer, shape.clone()))
        })
        .collect();
    if shapes.len() != studio.selection.len() {
        ui.label("Select only vector objects to edit shared geometry");
        return true;
    }
    let values: Vec<[f32; 5]> = shapes
        .iter()
        .map(|(_, shape)| {
            let bounds = shape.geom.bbox();
            let pose = studio.live_pose(shape.id);
            let bounds = if studio.is_motion() {
                pose.map_bounds(shape.world_bbox())
            } else {
                bounds
            };
            [
                bounds.min.x,
                bounds.min.y,
                bounds.width(),
                bounds.height(),
                (shape.rotation
                    + if studio.is_motion() {
                        pose.rotation
                    } else {
                        0.0
                    })
                .to_degrees(),
            ]
        })
        .collect();
    let mut after = values[0];
    let mut changed = [false; 5];
    for (index, (label, unit, range, reset)) in [
        ("X", " px", -100000.0..=100000.0, Some(0.0)),
        ("Y", " px", -100000.0..=100000.0, Some(0.0)),
        ("Width", " px", 1.0..=20000.0, None),
        ("Height", " px", 1.0..=20000.0, None),
        ("Rotation", "°", -180.0..=180.0, Some(0.0)),
    ]
    .into_iter()
    .enumerate()
    {
        let state = FieldState {
            mixed: values
                .iter()
                .any(|value| (value[index] - after[index]).abs() > 0.001),
            ..Default::default()
        };
        changed[index] = fields::number(
            ui,
            ("mixed-transform", index),
            label,
            &mut after[index],
            range,
            unit,
            reset,
            state,
        )
        .changed;
    }
    apply(studio, after, changed);
    true
}

/// Apply only edited shared geometry through one typed undo command.
/// Args: studio, desired X/Y/width/height/rotation and changed-property flags.
/// Returns whether an editable selection changed; locked or invalid edits leave artwork untouched.
fn apply(studio: &mut Studio, after: [f32; 5], changed: [bool; 5]) -> bool {
    if !changed.iter().any(|changed| *changed) || !after.iter().all(|value| value.is_finite()) {
        return false;
    }
    let shapes: Vec<_> = studio
        .selection
        .iter()
        .filter_map(|&(layer, id)| {
            studio
                .doc
                .find_shape(layer, id)
                .cloned()
                .map(|shape| (layer, shape))
        })
        .collect();
    if shapes.is_empty()
        || shapes.len() != studio.selection.len()
        || shapes
            .iter()
            .any(|(layer, shape)| !studio.layer_unlocked(*layer) || shape.locked)
    {
        return false;
    }
    if studio.is_motion() {
        let mut motion = studio.doc.motion.clone();
        for (_, shape) in shapes {
            let bounds = shape.world_bbox();
            let pose = studio.live_pose(shape.id);
            let shown = pose.map_bounds(bounds);
            for (index, prop, value) in [
                (0, crate::motion::Prop::X, pose.dx + after[0] - shown.min.x),
                (1, crate::motion::Prop::Y, pose.dy + after[1] - shown.min.y),
                (
                    2,
                    crate::motion::Prop::Width,
                    pose.width_scale * after[2] / shown.width().max(1.0),
                ),
                (
                    3,
                    crate::motion::Prop::Height,
                    pose.height_scale * after[3] / shown.height().max(1.0),
                ),
                (
                    4,
                    crate::motion::Prop::Rotation,
                    after[4].to_radians() - shape.rotation,
                ),
            ] {
                if changed[index] {
                    motion.set_key(
                        shape.id,
                        prop,
                        studio.playhead,
                        value,
                        crate::motion::Ease::EaseInOut,
                    );
                }
            }
        }
        studio.commit_motion(motion);
    } else {
        let commands = shapes
            .into_iter()
            .map(|(layer, shape)| {
                let bounds = shape.geom.bbox();
                let mut value = [
                    bounds.min.x,
                    bounds.min.y,
                    bounds.width(),
                    bounds.height(),
                    shape.rotation.to_degrees(),
                ];
                for index in 0..5 {
                    if changed[index] {
                        value[index] = after[index];
                    }
                }
                let before = shape.geom.clone();
                let mut geometry = before.clone();
                geometry.map_into_with_text_scale(
                    before.bbox(),
                    Bounds::from_min_size(Pt::new(value[0], value[1]), Pt::new(value[2], value[3])),
                );
                Cmd::SetGeom {
                    layer,
                    id: shape.id,
                    before,
                    after: geometry,
                    rot_before: shape.rotation,
                    rot_after: value[4].to_radians(),
                }
            })
            .collect();
        studio.commit(Cmd::Batch(commands));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Studio {
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Shared fields", 800.0, 600.0, 72.0);
        let mut layer = crate::document::Layer::vector("Artwork");
        for x in [20.0, 220.0] {
            let shape = crate::document::Shape::new(
                crate::geom::Geom::Rect {
                    origin: Pt::new(x, 30.0),
                    size: Pt::new(100.0, 80.0),
                    radius: 0.0,
                },
                Default::default(),
            );
            studio.selection.push((0, shape.id));
            layer.kind.shapes_mut().unwrap().push(shape);
        }
        studio.doc.layers = vec![layer];
        studio.active_layer = Some(0);
        studio.show_welcome = false;
        studio
    }
    #[test]
    fn shared_transform_preserves_mixed_values_undo_and_native_roundtrip() {
        let mut studio = fixture();
        let before = studio.doc.clone();
        assert!(apply(
            &mut studio,
            [0.0, 50.0, 100.0, 80.0, 0.0],
            [false, true, false, false, false]
        ));
        let positions: Vec<_> = studio.doc.layers[0]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .map(|s| s.geom.bbox().min)
            .collect();
        assert_eq!(positions, vec![Pt::new(20.0, 50.0), Pt::new(220.0, 50.0)]);
        let encoded = crate::project::encode(&studio.doc).unwrap();
        let reopened = crate::project::decode(&encoded).unwrap();
        assert_eq!(
            reopened.layers[0].kind.shapes().unwrap()[1].geom.bbox().min,
            Pt::new(220.0, 50.0)
        );
        studio.undo();
        assert_eq!(
            studio.doc.layers[0].kind.shapes().unwrap()[0].geom,
            before.layers[0].kind.shapes().unwrap()[0].geom
        );
        studio.redo();
        assert_eq!(
            studio.doc.layers[0].kind.shapes().unwrap()[0]
                .geom
                .bbox()
                .min
                .y,
            50.0
        );
        studio.doc.layers[0].locked = true;
        let encoded = serde_json::to_vec(&studio.doc).unwrap();
        assert!(!apply(
            &mut studio,
            [0.0, 90.0, 100.0, 80.0, 0.0],
            [false, true, false, false, false]
        ));
        assert_eq!(encoded, serde_json::to_vec(&studio.doc).unwrap());
    }
}
