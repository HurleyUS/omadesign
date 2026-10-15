//! Headless fixture qualification through the real document, history and persistence paths.
use crate::document::{self, Cmd, Document, History};
use serde_json::{Value, json};
use std::path::Path;

/// Describe editable structure without unstable IDs.
/// Args: imported document. Returns ordered layer facts with parent indices and original mask geometry.
fn structure(doc: &Document) -> Value {
    json!({"width":doc.width,"height":doc.height,"notes":doc.import_notes,
        "layers":doc.layers.iter().map(|l| json!({
            "name":l.name,"kind":if l.is_group {"group"} else if l.kind.pixels().is_some() {"raster"} else {"vector"},
            "visible":l.visible,"parent":l.parent.and_then(|id|doc.layers.iter().position(|p|p.id==id)),
            "opacity":l.opacity,"mask":l.mask.is_some(),"objects":l.kind.shapes().map_or(0,|s|s.len()),
            "offset":l.kind.raster_xform().map(|(p,_,_)|[p.x,p.y]),
            "artwork_masks":l.artwork_masks,
            "pixels":l.kind.pixels().map(|p|json!({"width":p.w,"height":p.h,"rgba":p.data})),
            "mask_pixels":l.mask.as_ref().map(|p|json!({"width":p.w,"height":p.h,"rgba":p.data}))
        })).collect::<Vec<_>>()})
}

/// Check a subset of expected structure.
/// Args: actual and independent expected JSON. Returns whether every stated expectation matches, allowing small float rounding.
fn contains(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => e
            .iter()
            .all(|(k, v)| a.get(k).is_some_and(|a| contains(a, v))),
        (Value::Array(a), Value::Array(e)) => {
            a.len() == e.len() && a.iter().zip(e).all(|(a, e)| contains(a, e))
        }
        (Value::Number(a), Value::Number(e)) => {
            (a.as_f64().unwrap_or(f64::NAN) - e.as_f64().unwrap_or(f64::NAN)).abs() < 0.000001
        }
        _ => actual == expected,
    }
}

/// Qualify one bounded document through the real headless paths.
/// Args: local request JSON filename. Returns an error if any required check or reference fails; writes detailed local evidence for each check.
pub fn qualify(request: &Path) -> Result<(), String> {
    let request: Value =
        serde_json::from_slice(&std::fs::read(request).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let get_path = |key: &str| {
        request[key]
            .as_str()
            .map(Path::new)
            .ok_or_else(|| format!("Missing {key}"))
    };
    let input = get_path("input")?;
    let out = get_path("output")?;
    let bytes = std::fs::read(input).map_err(|e| e.to_string())?;
    let mut doc = super::cli::document(input)?;
    let pixels = f64::from(doc.width) * f64::from(doc.height);
    let max_pixels = request["fixture"]["limits"]["max_render_pixels"]
        .as_u64()
        .ok_or("Missing render budget")?;
    if !pixels.is_finite() || pixels < 1. || pixels > max_pixels as f64 {
        return Err("Fixture render exceeds bounded viewport budget".into());
    }
    let facts = structure(&doc);
    let expected = &request["fixture"]["expected"];
    let vectors: usize = doc
        .layers
        .iter()
        .filter_map(|l| l.kind.shapes())
        .map(|s| s.len())
        .sum();
    let rasters = doc
        .layers
        .iter()
        .filter(|l| l.kind.pixels().is_some())
        .count();
    let masks: usize = doc.layers.iter().map(|l| l.artwork_masks.len()).sum();
    let mut editable = expected["min_vector_objects"]
        .as_u64()
        .is_none_or(|n| vectors >= n as usize)
        && expected["max_raster_layers"]
            .as_u64()
            .is_none_or(|n| rasters <= n as usize)
        && expected["artwork_masks"]
            .as_u64()
            .is_none_or(|n| masks == n as usize);
    let rendered = super::decode_image(&crate::compositor::export_png(&doc, 1)?)?;
    let reference_path = get_path("render_reference")?;
    let reference_bytes = std::fs::read(reference_path).map_err(|e| e.to_string())?;
    let reference = if reference_path.extension().is_some_and(|e| e == "json") {
        let specification: Value =
            serde_json::from_slice(&reference_bytes).map_err(|e| e.to_string())?;
        let subset = json!({"width":specification["width"],"height":specification["height"],"layers":specification["layers"]});
        editable &= contains(&facts, &subset);
        let w = specification["width"].as_u64().ok_or("Reference width")? as u32;
        let h = specification["height"].as_u64().ok_or("Reference height")? as u32;
        if u64::from(w) * u64::from(h) > max_pixels {
            return Err("Reference exceeds budget".into());
        }
        let rgba = |v: &Value| -> Result<Vec<u8>, String> {
            v.as_array()
                .filter(|a| a.len() == 4)
                .ok_or("Reference RGBA")?
                .iter()
                .map(|n| {
                    n.as_u64()
                        .filter(|n| *n <= 255)
                        .map(|n| n as u8)
                        .ok_or("Reference channel".into())
                })
                .collect()
        };
        let mut data = rgba(&specification["background"])?.repeat(w as usize * h as usize);
        if let Some(samples) = specification["samples"].as_array() {
            for sample in samples {
                let x = sample["x"].as_u64().ok_or("Sample x")? as u32;
                let y = sample["y"].as_u64().ok_or("Sample y")? as u32;
                if x >= w || y >= h {
                    return Err("Reference sample outside image".into());
                }
                let i = (y as usize * w as usize + x as usize) * 4;
                data[i..i + 4].copy_from_slice(&rgba(&sample["rgba"])?);
            }
        }
        crate::document::Pixels::from_rgba(w, h, data).ok_or("Reference pixels")?
    } else {
        super::decode_image(&reference_bytes)?
    };
    let mut max_error = 0u8;
    let dimensions_match = (rendered.w, rendered.h) == (reference.w, reference.h);
    if dimensions_match {
        for (a, b) in rendered
            .data
            .chunks_exact(4)
            .zip(reference.data.chunks_exact(4))
        {
            for channel in 0..4 {
                let left = if channel == 3 {
                    a[channel] as u16
                } else {
                    a[channel] as u16 * a[3] as u16 / 255
                };
                let right = if channel == 3 {
                    b[channel] as u16
                } else {
                    b[channel] as u16 * b[3] as u16 / 255
                };
                max_error = max_error.max(left.abs_diff(right) as u8);
            }
        }
    }
    let appearance = dimensions_match
        && max_error
            <= request["fixture"]["max_channel_error"]
                .as_u64()
                .unwrap_or(1) as u8;
    let before = crate::project::encode(&doc)?;
    let layer = doc.layers.first().ok_or("Fixture has no editable layer")?;
    let original_name = layer.name.clone();
    let command = Cmd::SetLayerMeta {
        index: 0,
        name: format!("{original_name} edited"),
        visible: layer.visible,
        locked: layer.locked,
        opacity: layer.opacity,
        blend: layer.blend,
        before: (
            layer.name.clone(),
            layer.visible,
            layer.locked,
            layer.opacity,
            layer.blend,
        ),
    };
    let mut history = History::default();
    document::apply(&mut doc, &command);
    history.push(command);
    editable &= doc.layers[0].name != original_name;
    let edited = crate::project::encode(&doc)?;
    let native = out.join("edited.oma");
    crate::project::save_to(&doc, &native)?;
    let reopened = crate::project::load_from(&native)?;
    let save_reopen =
        crate::project::encode(&reopened)? == edited && reopened.import_notes == doc.import_notes;
    let inverse = history.undo().ok_or("Edit did not create undo")?;
    document::apply(&mut doc, &inverse);
    let undo = crate::project::encode(&doc)? == before && !history.can_undo();
    let broken = out.join(format!(
        "broken.{}",
        input
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("invalid")
    ));
    std::fs::write(&broken, &bytes[..bytes.len().min(8)]).map_err(|e| e.to_string())?;
    let failure = super::cli::document(&broken).is_err() && crate::project::encode(&doc)? == before;
    std::fs::remove_file(&broken).map_err(|e| e.to_string())?;
    let preservation = std::fs::read(input).map_err(|e| e.to_string())? == bytes;
    let facts = json!({"structure":facts,"vector_objects":vectors,"raster_layers":rasters,"artwork_masks":masks,"appearance":{"dimensions_match":dimensions_match,"max_premultiplied_channel_error":max_error},"edited_name":reopened.layers[0].name,"native_reopen_equal":save_reopen,"undo_equal":undo,"truncated_input_refused":failure,"source_unchanged":preservation,"boundary":"headless document/history; no desktop interaction or physical evidence"});
    std::fs::write(
        out.join("facts.json"),
        serde_json::to_vec_pretty(&facts).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let checks = [("appearance",appearance),("editability",editable),("undo",undo),("save_reopen",save_reopen),("failure",failure),("preservation",preservation)].into_iter().map(|(key,passed)|(key.to_owned(),json!({"status":if passed {"passed"} else {"failed"},"artifacts":if key=="save_reopen" {vec!["facts.json","edited.oma"]} else {vec!["facts.json"]}}))).collect::<serde_json::Map<_,_>>();
    std::fs::write(
        out.join("result.json"),
        serde_json::to_vec_pretty(&json!({"checks":checks})).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if appearance && editable && undo && save_reopen && failure && preservation {
        Ok(())
    } else {
        Err("Required fixture check failed; inspect facts.json".into())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn required_public_fixture_cohort() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let out = root.join(".artifacts/suite/omadesign");
        let status = std::process::Command::new("node")
            .arg(root.join("scripts/suite/omadesign-test.mjs"))
            .arg(std::env::current_exe().unwrap())
            .arg(out)
            .status()
            .unwrap();
        assert!(status.success(), "Required headless fixture cohort failed");
    }

    #[test]
    #[ignore = "explicit suite request, no implicit private fixture pass"]
    fn qualification_driver() {
        let request = std::env::var_os("OMA_SUITE_REQUEST").expect("OMA_SUITE_REQUEST required");
        super::qualify(std::path::Path::new(&request)).unwrap();
    }
}
