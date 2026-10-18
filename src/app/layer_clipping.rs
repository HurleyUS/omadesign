use super::*;

impl Studio {
    fn clipping_targets(&self, owner: Option<u64>, clipped: bool) -> Result<Vec<usize>, String> {
        if self.show_welcome
            || self.op.is_some()
            || self.deformation.is_some()
            || self.free_transform.is_some()
            || self.updates.freezing
            || self.file_dialog_pending()
        {
            return Err("Finish the current operation before changing layer clipping".into());
        }
        let mut selected: Vec<_> = if let Some(id) = owner.or(self.selected_layer) {
            vec![
                self.doc
                    .layers
                    .iter()
                    .position(|l| l.id == id)
                    .ok_or("The clipping layer is missing")?,
            ]
        } else if !self.selection.is_empty() {
            self.selection.iter().map(|&(i, _)| i).collect()
        } else {
            self.active_layer.into_iter().collect()
        };
        selected.sort_unstable();
        selected.dedup();
        if selected.iter().any(|&i| i >= self.doc.layers.len()) {
            return Err("The clipping selection changed".into());
        }
        let roots: Vec<_> = selected
            .iter()
            .copied()
            .filter(|&i| {
                !self
                    .doc
                    .layer_ancestors(i)
                    .iter()
                    .any(|p| selected.contains(p))
            })
            .collect();
        let selected = if clipped && roots.len() > 1 {
            &roots[1..]
        } else {
            &roots[..]
        };
        if selected.iter().any(|&i| !self.layer_unlocked(i)) {
            return Err("Unlock the affected layers before changing clipping".into());
        }
        let changed: Vec<_> = selected
            .iter()
            .copied()
            .filter(|&i| self.doc.layers[i].clipped != clipped)
            .collect();
        if changed.is_empty() {
            return Err("No selected layer needs that clipping change".into());
        }
        if clipped
            && changed
                .iter()
                .any(|&i| self.layer_sibling(i, false).is_none())
        {
            return Err("A clipping mask needs a sibling layer below it".into());
        }
        Ok(changed)
    }

    pub(crate) fn can_set_layer_clipping(&self, owner: Option<u64>, clipped: bool) -> bool {
        self.clipping_targets(owner, clipped).is_ok()
    }

    /// Change selected or explicitly owned layer clipping in one native transaction.
    /// Args: optional concrete layer ID and requested clipping state. Returns false without mutation on stale, locked or cyclic input.
    pub(crate) fn set_layer_clipping(&mut self, owner: Option<u64>, clipped: bool) -> bool {
        let result = (|| {
            let targets = self.clipping_targets(owner, clipped)?;
            let command = Cmd::Batch(
                targets
                    .into_iter()
                    .map(|index| Cmd::SetLayerClipping {
                        index,
                        before: self.doc.layers[index].clipped,
                        after: clipped,
                    })
                    .collect(),
            );
            layer_copy::validate(&self.doc, &command)?;
            Ok::<_, String>(command)
        })();
        match result {
            Ok(command) => {
                self.commit_type_edit();
                self.commit(command);
                self.status = if clipped {
                    "Clipping mask created"
                } else {
                    "Clipping mask released"
                }
                .into();
                true
            }
            Err(error) => {
                self.status = error;
                false
            }
        }
    }
}
