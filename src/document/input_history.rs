use super::{Cmd, History, MAX_HISTORY, coalesce};

impl History {
    /// End field grouping after an edit from another owner.
    /// Args: none. Returns nothing; the next edit starts a separate history step.
    pub(crate) fn finish_field(&mut self) {
        self.field = None;
        self.sealed = true;
    }

    /// Keep one starting and final command per property in a field gesture.
    /// Args: applied command and document/selection/field/gesture identity.
    /// Returns nothing; unrelated commands always keep their own history step.
    pub(crate) fn push_field(&mut self, cmd: Cmd, field: u64) {
        let mut next = Vec::new();
        if !flatten(cmd.clone(), &mut next) {
            self.sealed = true;
            self.push(cmd);
            self.sealed = true;
            return;
        }
        let mut previous = Vec::new();
        if self.field == Some(field)
            && let Some(last) = self.undo.last()
            && flatten(last.clone(), &mut previous)
        {
            for command in next {
                if !previous.iter_mut().any(|old| merge(old, &command)) {
                    previous.push(command);
                }
            }
            *self.undo.last_mut().unwrap() = pack(previous);
        } else {
            self.undo.push(pack(next));
            if self.undo.len() > MAX_HISTORY {
                self.undo.remove(0);
            }
        }
        self.field = Some(field);
        self.sealed = true;
        self.redo.clear();
    }
}

fn pack(mut commands: Vec<Cmd>) -> Cmd {
    if commands.len() == 1 {
        commands.remove(0)
    } else {
        Cmd::Batch(commands)
    }
}

fn flatten(cmd: Cmd, commands: &mut Vec<Cmd>) -> bool {
    match cmd {
        Cmd::Batch(batch) => batch.into_iter().all(|command| flatten(command, commands)),
        command @ (Cmd::SetGeom { .. }
        | Cmd::SetGeoms { .. }
        | Cmd::SetVectorShapes { .. }
        | Cmd::SetStyle { .. }
        | Cmd::SetOpacity { .. }
        | Cmd::SetFillOpacity { .. }
        | Cmd::SetLayerMeta { .. }
        | Cmd::SetMotion { .. }
        | Cmd::SetFilters { .. }
        | Cmd::SetShapeFilters { .. }
        | Cmd::SetRasterXform { .. }
        | Cmd::SetRasterShear { .. }
        | Cmd::SetArtboards { .. }
        | Cmd::SetCorners { .. }
        | Cmd::SetLayout { .. }) => {
            if !commands.iter_mut().any(|old| merge(old, &command)) {
                commands.push(command);
            }
            true
        }
        _ => false,
    }
}

fn merge(previous: &mut Cmd, next: &Cmd) -> bool {
    match (&mut *previous, next) {
        (
            Cmd::SetGeom {
                layer,
                id,
                after,
                rot_after,
                ..
            },
            Cmd::SetGeom {
                layer: l,
                id: i,
                after: a,
                rot_after: r,
                ..
            },
        ) if layer == l && id == i => {
            *after = a.clone();
            *rot_after = *r;
            true
        }
        (Cmd::SetGeoms { items }, Cmd::SetGeoms { items: next }) => {
            for item in next {
                if let Some(old) = items
                    .iter_mut()
                    .find(|old| old.0 == item.0 && old.1 == item.1)
                {
                    old.3 = item.3.clone();
                    old.5 = item.5;
                } else {
                    items.push(item.clone());
                }
            }
            true
        }
        (
            Cmd::SetVectorShapes { layer, after, .. },
            Cmd::SetVectorShapes {
                layer: l, after: a, ..
            },
        ) if layer == l => {
            *after = a.clone();
            true
        }
        (
            Cmd::SetFillOpacity {
                layer, id, after, ..
            },
            Cmd::SetFillOpacity {
                layer: l,
                id: i,
                after: a,
                ..
            },
        ) if layer == l && id == i => {
            *after = *a;
            true
        }
        (
            Cmd::SetRasterShear { layer, after, .. },
            Cmd::SetRasterShear {
                layer: l, after: a, ..
            },
        ) if layer == l => {
            *after = *a;
            true
        }
        _ => coalesce(previous, next),
    }
}
