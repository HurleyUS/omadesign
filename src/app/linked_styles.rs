//! Commands for document-level linked styles; each is one Undo step.
use super::*;
use crate::linked_styles::LinkedStyle;

impl Studio {
    fn linked_ready(&self) -> Result<(), String> {
        if self.op.is_some() || self.type_edit.is_some() || self.free_transform.is_some() || self.file_dialog_pending() || self.updates.freezing {
            return Err("Finish the current edit before changing linked styles".into());
        }
        Ok(())
    }

    fn locate(&self, id: u64) -> Option<usize> {
        (0..self.doc.layers.len()).find(|&layer| self.doc.find_shape(layer, id).is_some())
    }

    /// Apply an appearance to objects and rewrite style membership as one Batch.
    fn restyle(&mut self, targets: &[u64], appearance: &crate::graphic_styles::Appearance, styles: Vec<LinkedStyle>) -> Result<(), String> {
        let mut layers = std::collections::BTreeMap::<usize, Vec<Shape>>::new();
        for &id in targets {
            let layer = self.locate(id).ok_or("A linked object is missing")?;
            let shapes = layers.entry(layer).or_insert_with(|| self.doc.layers[layer].kind.shapes().unwrap().to_vec());
            appearance.apply(shapes.iter_mut().find(|s| s.id == id).unwrap());
        }
        let mut commands: Vec<Cmd> = layers
            .into_iter()
            .filter_map(|(layer, after)| {
                let before = self.doc.layers[layer].kind.shapes().unwrap();
                (before != after).then(|| Cmd::SetVectorShapes { layer, before: before.to_vec(), after })
            })
            .collect();
        if styles != self.doc.linked_styles {
            commands.push(Cmd::SetLinkedStyles { before: self.doc.linked_styles.clone(), after: styles });
        }
        if !commands.is_empty() {
            self.commit(Cmd::Batch(commands));
        }
        Ok(())
    }

    fn selected_ids(&self) -> Vec<u64> {
        self.selection.iter().map(|s| s.1).collect()
    }

    fn without(styles: &mut [LinkedStyle], ids: &[u64]) {
        for style in styles {
            style.members.retain(|m| !ids.contains(m));
        }
    }

    /// Create a document style from the primary object and link the selection.
    /// Args: name. Returns the new style id.
    pub(crate) fn create_linked_style(&mut self, name: &str) -> Result<u64, String> {
        self.linked_ready()?;
        let appearance = self.capture_graphic_style()?;
        let name = name.trim();
        if name.is_empty() {
            return Err("Name the linked style".into());
        }
        let ids = self.selected_ids();
        let mut styles = self.doc.linked_styles.clone();
        Self::without(&mut styles, &ids);
        let id = crate::document::next_id();
        styles.push(LinkedStyle { id, name: name.into(), appearance, members: ids });
        crate::linked_styles::validate(&styles)?;
        self.commit(Cmd::SetLinkedStyles { before: self.doc.linked_styles.clone(), after: styles });
        Ok(id)
    }

    /// Apply a document style to the selection and link it. Args: style id.
    pub(crate) fn apply_linked_style(&mut self, style: u64) -> Result<(), String> {
        self.linked_ready()?;
        let ids = self.selected_ids();
        if ids.is_empty() {
            return Err("Select artwork to apply the linked style".into());
        }
        let mut styles = self.doc.linked_styles.clone();
        Self::without(&mut styles, &ids);
        let entry = styles.iter_mut().find(|s| s.id == style).ok_or("That linked style no longer exists")?;
        entry.members.extend(&ids);
        let appearance = entry.appearance.clone();
        self.restyle(&ids, &appearance, styles)
    }

    /// Redefine a style from the primary object and update every linked object.
    /// Args: style id. Missing members are dropped from the style.
    pub(crate) fn redefine_linked_style(&mut self, style: u64) -> Result<(), String> {
        self.linked_ready()?;
        let appearance = self.capture_graphic_style()?;
        let mut styles = self.doc.linked_styles.clone();
        let live: Vec<u64> = {
            let entry = styles.iter_mut().find(|s| s.id == style).ok_or("That linked style no longer exists")?;
            entry.members.retain(|&m| self.locate(m).is_some());
            entry.appearance = appearance.clone();
            entry.members.clone()
        };
        for &id in &live {
            let layer = self.locate(id).unwrap();
            if !super::layout_components::component_object_editable(&self.doc, layer, id) || self.doc.find_shape(layer, id).is_some_and(|s| s.locked) {
                return Err("Unlock every linked object before redefining the style".into());
            }
        }
        self.restyle(&live, &appearance, styles)
    }

    /// Detach selected objects from their styles, keeping their look. Args: none.
    pub(crate) fn unlink_linked_style(&mut self) -> Result<(), String> {
        self.linked_ready()?;
        let mut styles = self.doc.linked_styles.clone();
        Self::without(&mut styles, &self.selected_ids());
        if styles != self.doc.linked_styles {
            self.commit(Cmd::SetLinkedStyles { before: self.doc.linked_styles.clone(), after: styles });
        }
        Ok(())
    }

    /// Delete a document style; objects keep their current appearance. Args: style id.
    pub(crate) fn delete_linked_style(&mut self, style: u64) -> Result<(), String> {
        self.linked_ready()?;
        let mut styles = self.doc.linked_styles.clone();
        styles.retain(|s| s.id != style);
        if styles.len() == self.doc.linked_styles.len() {
            return Err("That linked style no longer exists".into());
        }
        self.commit(Cmd::SetLinkedStyles { before: self.doc.linked_styles.clone(), after: styles });
        Ok(())
    }
}

#[cfg(test)]
#[path = "linked_styles_tests.rs"]
mod tests;
