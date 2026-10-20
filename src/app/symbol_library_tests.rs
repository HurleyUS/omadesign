use super::*;
use crate::geom::Pt;
use crate::layout_components::ComponentBinding;

fn library() -> Document {
    let mut s = crate::app::symbols::tests::fixture();
    assert!(s.create_vector_symbol("Badge"));
    s.doc
}

#[test]
fn craft_symbol_library_imports_definitions_with_fresh_ids_in_one_undo() {
    let lib = library();
    let lib_main = components::definitions(&lib)[0].1;
    let mut s = crate::app::symbols::tests::fixture();
    let before = crate::project::encode(&s.doc).unwrap();
    assert!(s.create_vector_symbol("Badge"), "a same-named local symbol");
    let history = s.history.len();
    assert_eq!(s.import_symbol_library(&lib, "Brand kit").unwrap(), 1);
    assert_eq!(s.history.len(), history + 1);
    let defs = components::definitions(&s.doc);
    assert_eq!(defs.len(), 2);
    let imported = defs.iter().find(|d| d.1 != lib_main && d.2.contains("Brand kit")).expect("renamed on collision");
    assert_ne!(imported.1, lib_main, "fresh identity");
    let layer = s.doc.layers.last().unwrap();
    assert!(layer.locked && !layer.visible);
    let main = imported.1;
    s.active_layer = Some(0);
    assert!(s.place_vector_symbol(main, Pt::new(150., 120.)), "imported symbols place normally");
    let root = s.primary().unwrap().1;
    assert!(matches!(s.doc.find_shape(0, root).unwrap().layout.component, Some(ComponentBinding::Instance { main: m, .. }) if m == main));
    let saved = crate::project::encode(&s.doc).unwrap();
    assert_eq!(crate::project::encode(&crate::project::decode(&saved).unwrap()).unwrap(), saved);
    s.undo();
    s.undo();
    s.undo();
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
}

#[test]
fn craft_symbol_library_refuses_files_without_symbols_and_busy_editor() {
    let mut s = crate::app::symbols::tests::fixture();
    let plain = Document::new("Plain", 10., 10., 72.);
    assert!(s.import_symbol_library(&plain, "Plain").is_err());
    let lib = library();
    s.updates.freezing = true;
    assert!(s.import_symbol_library(&lib, "Kit").is_err());
    assert_eq!(s.history.len(), 0);
}
