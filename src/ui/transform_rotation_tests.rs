//! Transform panel sizing for rotated and sub-pixel objects (#487, #488).
use super::studios::{apply_shape_transform_fields, shape_transform_fields};
use crate::{
    app::Studio,
    document::{Document, Layer, Shape, Style},
    geom::{Geom, Pt},
};

const TOLERANCE: f32 = 1e-3;

fn studio_with(shapes: Vec<(Geom, f32)>) -> (Studio, Vec<u64>) {
    let mut studio = Studio::new();
    studio.doc = Document::new("Transform rotation", 800.0, 600.0, 72.0);
    let mut layer = Layer::vector("Artwork");
    let mut ids = Vec::new();
    for (geom, degrees) in shapes {
        let mut shape = Shape::new(geom, Style::default());
        shape.rotation = degrees.to_radians();
        ids.push(shape.id);
        layer.kind.shapes_mut().unwrap().push(shape);
    }
    studio.doc.layers = vec![layer];
    studio.active_layer = Some(0);
    studio.show_welcome = false;
    (studio, ids)
}

fn rect(origin: Pt, size: Pt) -> Geom {
    Geom::Rect { origin, size, radius: 0.0 }
}

fn close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < TOLERANCE,
        "{what}: expected {expected}, got {actual}"
    );
}

/// Panel W/H are the object's own size, and editing W only changes that axis.
#[test]
fn transform_panel_width_height_follow_object_axes_for_quarter_and_half_turns() {
    for degrees in [90.0_f32, -90.0, 180.0, 270.0] {
        let (mut studio, ids) = studio_with(vec![(rect(Pt::new(100.0, 100.0), Pt::new(200.0, 50.0)), degrees)]);
        let shape = studio.doc.find_shape(0, ids[0]).unwrap().clone();
        let fields = shape_transform_fields(&shape);
        close(fields[2], 200.0, &format!("{degrees}° panel W"));
        close(fields[3], 50.0, &format!("{degrees}° panel H"));
        let center = shape.geom.bbox().center();
        apply_shape_transform_fields(&mut studio, 0, ids[0], [fields[0], fields[1], 300.0, fields[3]]);
        let after = studio.doc.find_shape(0, ids[0]).unwrap();
        let size = after.geom.bbox().size();
        close(size.x, 300.0, &format!("{degrees}° own width after W edit"));
        close(size.y, 50.0, &format!("{degrees}° own height after W edit"));
        close(after.rotation, degrees.to_radians(), &format!("{degrees}° rotation kept"));
        close(after.geom.bbox().center().x, center.x, &format!("{degrees}° pivot x kept"));
        close(after.geom.bbox().center().y, center.y, &format!("{degrees}° pivot y kept"));
        // Editing H afterwards changes only the object's own height.
        let fields = shape_transform_fields(after);
        apply_shape_transform_fields(&mut studio, 0, ids[0], [fields[0], fields[1], fields[2], 80.0]);
        let size = studio.doc.find_shape(0, ids[0]).unwrap().geom.bbox().size();
        close(size.x, 300.0, &format!("{degrees}° own width after H edit"));
        close(size.y, 80.0, &format!("{degrees}° own height after H edit"));
    }
}

/// Moving a rotated object through X/Y keeps its size and rotation.
#[test]
fn transform_panel_move_keeps_rotated_object_size() {
    for degrees in [90.0_f32, -90.0, 180.0, 270.0] {
        let (mut studio, ids) = studio_with(vec![(rect(Pt::new(100.0, 100.0), Pt::new(200.0, 50.0)), degrees)]);
        let shape = studio.doc.find_shape(0, ids[0]).unwrap().clone();
        let before = shape.world_bbox();
        let fields = shape_transform_fields(&shape);
        apply_shape_transform_fields(&mut studio, 0, ids[0], [fields[0] + 10.0, fields[1] - 5.0, fields[2], fields[3]]);
        let after = studio.doc.find_shape(0, ids[0]).unwrap();
        close(after.geom.bbox().width(), 200.0, &format!("{degrees}° width"));
        close(after.geom.bbox().height(), 50.0, &format!("{degrees}° height"));
        close(after.world_bbox().min.x, before.min.x + 10.0, &format!("{degrees}° x"));
        close(after.world_bbox().min.y, before.min.y - 5.0, &format!("{degrees}° y"));
    }
}

/// A 0.25 px outlined stroke reads and keeps its true thickness (#487).
#[test]
fn transform_panel_keeps_quarter_pixel_outlined_stroke_thickness() {
    for degrees in [0.0_f32, 90.0, -90.0, 180.0, 270.0] {
        let outline = rect(Pt::new(50.0, 60.0), Pt::new(120.0, 0.25)).to_path();
        let (mut studio, ids) = studio_with(vec![(outline, degrees)]);
        let shape = studio.doc.find_shape(0, ids[0]).unwrap().clone();
        let fields = shape_transform_fields(&shape);
        close(fields[2], 120.0, &format!("{degrees}° panel W"));
        close(fields[3], 0.25, &format!("{degrees}° panel H"));
        apply_shape_transform_fields(&mut studio, 0, ids[0], [fields[0], fields[1], 240.0, fields[3]]);
        let size = studio.doc.find_shape(0, ids[0]).unwrap().geom.bbox().size();
        close(size.x, 240.0, &format!("{degrees}° resized length"));
        close(size.y, 0.25, &format!("{degrees}° thickness kept"));
        // Shrinking the thickness further is allowed down to an epsilon.
        let shape = studio.doc.find_shape(0, ids[0]).unwrap().clone();
        let fields = shape_transform_fields(&shape);
        apply_shape_transform_fields(&mut studio, 0, ids[0], [fields[0], fields[1], fields[2], 0.1]);
        close(studio.doc.find_shape(0, ids[0]).unwrap().geom.bbox().height(), 0.1, &format!("{degrees}° thinner"));
    }
}

/// #488 repro: four objects rotated 90° as a selection, then one 0.25 line's
/// width edited alone must not collapse to a 1×1 square at the old pivot.
#[test]
fn transform_panel_rotated_selection_line_width_edit_does_not_collapse() {
    let line = Geom::Line { a: Pt::new(100.0, 100.0), b: Pt::new(300.0, 100.0) };
    let outline = rect(Pt::new(100.0, 150.0), Pt::new(200.0, 0.25)).to_path();
    let (mut studio, ids) = studio_with(vec![
        (line, 0.0),
        (outline, 0.0),
        (rect(Pt::new(100.0, 200.0), Pt::new(80.0, 80.0)), 0.0),
        (rect(Pt::new(200.0, 200.0), Pt::new(300.0, 40.0)), 0.0),
    ]);
    // Rotate the whole selection 90° about its center, as the canvas rotate op does.
    let pivot = Pt::new(300.0, 190.0);
    for id in &ids {
        let shape = studio.doc.find_shape_mut(0, *id).unwrap();
        let center = shape.geom.bbox().center();
        shape.geom.translate(center.rotate_about(pivot, std::f32::consts::FRAC_PI_2) - center);
        shape.rotation += std::f32::consts::FRAC_PI_2;
    }
    for (index, own_height) in [(0usize, 0.0_f32), (1, 0.25)] {
        let id = ids[index];
        let shape = studio.doc.find_shape(0, id).unwrap().clone();
        let center = shape.geom.bbox().center();
        let fields = shape_transform_fields(&shape);
        close(fields[2], 200.0, "panel W is the line length");
        close(fields[3], own_height, "panel H is the line thickness");
        apply_shape_transform_fields(&mut studio, 0, id, [fields[0], fields[1], 150.0, fields[3]]);
        let after = studio.doc.find_shape(0, id).unwrap();
        close(after.geom.bbox().width(), 150.0, "line length edited");
        close(after.geom.bbox().height(), own_height, "line thickness kept");
        close(after.rotation, std::f32::consts::FRAC_PI_2, "rotation kept");
        close(after.geom.bbox().center().x, center.x, "own center x kept");
        close(after.geom.bbox().center().y, center.y, "own center y kept");
    }
}
