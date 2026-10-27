use super::*;
use crate::tools::{Persona, Tool};

fn fixture() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.doc = crate::channels::tests::fixture();
    s.active_layer = Some(0);
    s.persona = Persona::Pixel;
    s.doc.workspace = Some(Persona::Pixel);
    s.tool = Tool::Brush;
    s
}

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("omadesign-active-selection-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}
fn space() -> SelectionSpace {
    SelectionSpace { w: 2, h: 1, transform: tiny_skia::Transform::from_translate(2., 1.) }
}
fn version(path: &std::path::Path) -> u64 {
    let text = std::fs::read_to_string(path).unwrap();
    serde_json::from_str::<serde_json::Value>(&text).unwrap()["version"].as_u64().unwrap()
}

#[test]
fn craft_active_selection_save_and_reopen_keeps_exact_soft_coverage() {
    let mut s = fixture();
    s.replace_pixel_selection(vec![64, 192], space());
    let original: Vec<f32> = s.pixel_sel.as_ref().unwrap().iter().copied().collect();
    let artwork = crate::project::encode(&s.doc).unwrap();
    let history = s.history.len();
    let path = temp("kept.oma");
    s.save_document(&path).unwrap();
    // Saving changes neither the document, its history nor the live selection.
    assert!(s.doc.active_selection.is_none());
    assert_eq!(crate::project::encode(&s.doc).unwrap(), artwork);
    assert_eq!(s.history.len(), history);
    assert!(!s.dirty);
    // Saves write the minimum content-required version: ACTIVE_SELECTION (36).
    assert_eq!(version(&path), 36);
    let loaded = crate::project::load_from(&path).unwrap();
    assert!(loaded.active_selection.is_some());
    let mut reopened = Studio::new();
    reopened.show_welcome = false;
    reopened.open_document(loaded, Some(path.clone()));
    assert!(reopened.doc.active_selection.is_none(), "selection moves into the editor");
    let values: Vec<f32> = reopened.pixel_sel.as_ref().unwrap().iter().copied().collect();
    assert_eq!(values, original);
    assert_eq!(reopened.pixel_selection_space(), Some(space()));
    assert_eq!(reopened.history.len(), 0);
    assert!(!reopened.dirty);
    std::fs::remove_file(path).ok();
}

#[test]
fn craft_active_selection_absent_selection_keeps_the_older_file_version() {
    let mut s = fixture();
    let path = temp("none.oma");
    s.save_document(&path).unwrap();
    assert!(version(&path) < crate::project::VERSION as u64);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("active_selection"));
    let mut reopened = Studio::new();
    reopened.open_document(crate::project::load_from(&path).unwrap(), Some(path.clone()));
    assert!(reopened.pixel_sel.is_none());
    std::fs::remove_file(path).ok();
}

#[test]
fn craft_active_selection_quick_mask_owns_the_selection() {
    let mut s = fixture();
    s.replace_pixel_selection(vec![64, 192], space());
    s.set_quick_mask(true).unwrap();
    assert!(s.active_selection_plane().is_none());
    let path = temp("quick.oma");
    s.save_document(&path).unwrap();
    assert!(crate::project::load_from(&path).unwrap().active_selection.is_none());
    std::fs::remove_file(path).ok();
}

#[test]
fn craft_active_selection_refuses_damaged_saved_coverage() {
    let mut s = fixture();
    s.replace_pixel_selection(vec![64, 192], space());
    s.doc.active_selection = s.active_selection_plane();
    let text = crate::project::encode(&s.doc).unwrap();
    s.doc.active_selection = None;
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    fn find(v: &mut serde_json::Value) -> Option<&mut serde_json::Value> {
        match v {
            serde_json::Value::Object(map) => {
                if map.contains_key("active_selection") {
                    return map.get_mut("active_selection")?.get_mut("width");
                }
                map.values_mut().find_map(find)
            }
            serde_json::Value::Array(items) => items.iter_mut().find_map(find),
            _ => None,
        }
    }
    let width = find(&mut value).expect("saved selection present");
    *width = 7.into();
    assert!(crate::project::decode(&value.to_string()).is_err());
}

#[test]
fn craft_active_selection_older_reader_refuses_the_new_version() {
    let mut s = fixture();
    s.replace_pixel_selection(vec![64, 192], space());
    s.doc.active_selection = s.active_selection_plane();
    let text = crate::project::encode(&s.doc).unwrap();
    s.doc.active_selection = None;
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    // The minimum version that preserves an active selection is ACTIVE_SELECTION (36).
    assert_eq!(value["version"], 36);
    value["version"] = 35.into();
    assert!(crate::project::decode(&value.to_string()).is_err(), "a version-35 reader must not silently drop it");
}
