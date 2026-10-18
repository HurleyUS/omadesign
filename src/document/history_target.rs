use super::{Cmd, History};

/// Remember the editor target without serializing it into artwork.
/// Args: document identity and stable layer/object IDs. Restored only by the owning editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HistoryTarget {
    pub document: String,
    pub active: Option<u64>,
    pub layer: Option<u64>,
    pub objects: Vec<(u64, u64)>,
    pub rows: Option<(Vec<u64>, u64)>,
    pub individual: Option<(u64, u64)>,
}

#[derive(Clone)]
pub(super) struct HistoryEntry {
    pub command: Cmd,
    pub targets: Option<(HistoryTarget, HistoryTarget)>,
}

impl HistoryEntry {
    pub(super) fn new(command: Cmd) -> Self {
        Self {
            command,
            targets: None,
        }
    }
}

impl History {
    /// Attach before/after editor targets to the actual completed history entry.
    /// Args: snapshots from one accepted edit. Returns nothing; later edits start a new entry.
    pub(crate) fn remember_target(&mut self, before: HistoryTarget, after: HistoryTarget) {
        if let Some(entry) = self.undo.last_mut() {
            entry.targets = Some((before, after));
            self.field = None;
            self.sealed = true;
        }
    }

    /// Undo one command and return its original editor target.
    /// Args: none. Returns the inverse command and optional target from the same entry.
    pub(crate) fn undo_targeted(&mut self) -> Option<(Cmd, Option<HistoryTarget>)> {
        self.field = None;
        self.sealed = true;
        let entry = self.undo.pop()?;
        let target = entry.targets.as_ref().map(|(before, _)| before.clone());
        let inverse = super::invert_cmd(entry.command.clone());
        self.redo.push(entry);
        Some((inverse, target))
    }

    /// Redo one command and return its final editor target.
    /// Args: none. Returns the command and optional target from the same entry.
    pub(crate) fn redo_targeted(&mut self) -> Option<(Cmd, Option<HistoryTarget>)> {
        self.field = None;
        self.sealed = true;
        let entry = self.redo.pop()?;
        let target = entry.targets.as_ref().map(|(_, after)| after.clone());
        let command = entry.command.clone();
        self.undo.push(entry);
        Some((command, target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Layer, MAX_HISTORY};

    fn target(id: u64) -> HistoryTarget {
        HistoryTarget {
            document: "owner".into(),
            active: Some(id),
            layer: Some(id),
            objects: vec![],
            rows: None,
            individual: None,
        }
    }
    fn command() -> Cmd {
        Cmd::AddLayer {
            index: 0,
            layer: Layer::vector("Target"),
        }
    }

    #[test]
    fn craft_history_targets_move_with_their_command_and_clear_with_redo_branch() {
        let mut history = History::default();
        history.push(command());
        history.remember_target(target(1), target(2));
        history.push(command());
        assert_eq!(history.undo_targeted().unwrap().1, None);
        assert_eq!(history.undo_targeted().unwrap().1, Some(target(1)));
        assert_eq!(history.redo_targeted().unwrap().1, Some(target(2)));
        let mut copy = history.clone();
        history.push(command());
        assert!(!history.can_redo());
        assert!(copy.can_redo());
        assert_eq!(copy.redo_targeted().unwrap().1, None);
        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(copy.undo_targeted().unwrap().1, None);
        assert_eq!(copy.undo_targeted().unwrap().1, Some(target(1)));
    }

    #[test]
    fn craft_history_targets_share_entry_limit_staging_and_field_boundaries() {
        let mut history = History::default();
        for id in 0..=MAX_HISTORY as u64 {
            history.push(command());
            history.remember_target(target(id), target(id + 1));
        }
        assert_eq!(history.len(), MAX_HISTORY);
        for id in (1..=MAX_HISTORY as u64).rev() {
            assert_eq!(history.undo_targeted().unwrap().1, Some(target(id)));
        }
        assert!(history.undo_targeted().is_none());
        let mut staged = History::staging();
        for _ in 0..MAX_HISTORY + 2 {
            staged.push(command());
        }
        assert_eq!(staged.into_commands().len(), MAX_HISTORY + 2);
        let mut history = History::default();
        let doc = crate::document::Document::new("Fields", 2., 2., 96.);
        let edit = |before, after| Cmd::SetOpacity {
            layer: 0,
            id: 1,
            before,
            after,
        };
        history.push(edit(1., 0.5));
        history.remember_target(target(1), target(2));
        history.push_field(edit(0.5, 0.25), 7, &doc);
        history.push_field(edit(0.25, 0.125), 7, &doc);
        assert_eq!(history.len(), 2);
        assert_eq!(history.undo_targeted().unwrap().1, None);
        assert_eq!(history.undo_targeted().unwrap().1, Some(target(1)));
    }
}
