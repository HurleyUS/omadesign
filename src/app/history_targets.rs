use super::*;
use crate::document::HistoryTarget;

impl Studio {
    /// Capture stable target IDs for one completed edit.
    /// Args: current editor. Returns no document data or renderer resources.
    pub(super) fn history_target(&self) -> HistoryTarget {
        let id_at = |i| self.doc.layers.get(i).map(|layer| layer.id);
        HistoryTarget {
            document: self.swap_id.clone(),
            active: self.active_layer.and_then(id_at),
            layer: self.selected_layer,
            objects: self
                .selection
                .iter()
                .filter_map(|&(i, id)| id_at(i).map(|layer| (layer, id)))
                .collect(),
            rows: self
                .current_layer_rows()
                .map(|rows| (rows.ids.iter().copied().collect(), rows.anchor)),
            individual: self
                .individual_object
                .and_then(|(i, id)| id_at(i).map(|layer| (layer, id))),
        }
    }

    /// Restore an accepted edit's target after its command has been applied.
    /// Args: owning document and stable IDs. Returns without touching another document.
    pub(super) fn restore_history_target(&mut self, target: HistoryTarget) {
        if target.document != self.swap_id {
            return;
        }
        let indices: HashMap<_, _> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| (l.id, i))
            .collect();
        let resolve = |(layer, id)| {
            let i = *indices.get(&layer)?;
            let kind = &self.doc.layers[i].kind;
            (if id == RASTER_ID {
                kind.pixels().is_some()
            } else {
                self.doc.find_shape(i, id).is_some()
            })
            .then_some((i, id))
        };
        self.active_layer = target.active.and_then(|id| indices.get(&id).copied());
        self.selected_layer = target.layer.filter(|id| indices.contains_key(id));
        self.selection = target.objects.into_iter().filter_map(resolve).collect();
        self.individual_object = target.individual.and_then(resolve);
        self.layer_rows = target.rows.map(|(ids, anchor)| layer_groups::LayerRows {
            document: self.swap_id.clone(),
            ids: ids
                .into_iter()
                .filter(|id| indices.contains_key(id))
                .collect(),
            anchor,
            hits: self
                .selection
                .iter()
                .map(|&(i, id)| (self.doc.layers[i].id, id))
                .collect(),
        });
    }
}
