//! External symbol libraries: copy every symbol definition of another document
//! into a locked, hidden library layer with fresh identities.
use super::Studio;
use crate::document::{Cmd, Document, Layer};
use crate::layout_components as components;
use std::collections::{HashMap, HashSet};

impl Studio {
    /// Ask for an .oma file and import its symbols. Args: none. Returns nothing.
    pub(crate) fn import_symbol_library_dialog(&mut self) {
        self.request_file_dialog(
            || rfd::FileDialog::new().add_filter("omadesign", &["oma"]).pick_file(),
            |_, studio, path| {
                let name = path.file_stem().map_or("Library".into(), |n| n.to_string_lossy().into_owned());
                let result = std::fs::read_to_string(&path)
                    .map_err(|e| format!("read failed: {e}"))
                    .and_then(|text| crate::project::decode(&text))
                    .and_then(|lib| studio.import_symbol_library(&lib, &name));
                if let Err(e) = result {
                    studio.status = e;
                }
            },
        );
    }

    /// Import all symbol definitions from a library document as one Undo step.
    /// Args: library document and its display name. Returns how many symbols were added.
    pub(crate) fn import_symbol_library(&mut self, library: &Document, name: &str) -> Result<usize, String> {
        if self.op.is_some() || self.doc.collaboration.is_some() || self.file_dialog_pending() || self.updates.freezing {
            return Err("Finish the current edit before importing symbols".into());
        }
        library.validate_hierarchy()?;
        let definitions = components::definitions(library);
        if definitions.is_empty() {
            return Err("That file has no symbols to import".into());
        }
        let mut included: HashSet<(usize, u64)> = HashSet::new();
        for &(layer, main, _, _) in &definitions {
            included.insert((layer, main));
            included.extend(crate::layout::descendants(library, layer, main).into_iter().map(|id| (layer, id)));
        }
        let mut sources = Vec::new();
        for (index, layer) in library.layers.iter().enumerate() {
            for shape in layer.kind.shapes().unwrap_or_default() {
                if included.contains(&(index, shape.id)) {
                    sources.push(shape.clone());
                }
            }
        }
        let ids: HashMap<u64, u64> = sources.iter().map(|s| (s.id, crate::document::next_id())).collect();
        let existing: HashSet<String> = components::definitions(&self.doc).into_iter().map(|d| d.2.to_lowercase()).collect();
        let mains: HashSet<u64> = definitions.iter().map(|d| d.1).collect();
        let mut shapes = Vec::with_capacity(sources.len());
        for mut shape in sources {
            let was_main = mains.contains(&shape.id);
            crate::text_geometry::remap_copy_in_document(library, &mut shape, &ids);
            shape.id = ids[&shape.id];
            components::remap_duplicate(&mut shape, &ids);
            shape.layout.parent = shape.layout.parent.and_then(|p| ids.get(&p).copied());
            if was_main && existing.contains(&shape.name.to_lowercase()) {
                shape.name = format!("{} ({name})", shape.name);
            }
            shapes.push(shape);
        }
        let mut layer = Layer::vector(format!("Symbol library: {name}"));
        layer.visible = false;
        layer.locked = true;
        *layer.kind.shapes_mut().ok_or("Library layer is not vector")? = shapes;
        let mut candidate = self.doc.clone();
        candidate.layers.push(layer.clone());
        candidate.validate_hierarchy()?;
        components::validate_properties(&candidate)?;
        let count = components::definitions(&candidate).len() - components::definitions(&self.doc).len();
        self.commit(Cmd::AddLayer { index: self.doc.layers.len(), layer });
        self.status = format!("Imported {count} symbols from {name}");
        Ok(count)
    }
}

#[cfg(test)]
#[path = "symbol_library_tests.rs"]
mod tests;
