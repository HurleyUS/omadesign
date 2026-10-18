use super::*;
use crate::ml::Progress;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Down(u64),
    Group(u64),
    Selected,
    Visible,
    Flatten,
    StampDown(u64),
    StampGroup(u64),
    StampSelected,
    StampVisible,
}
impl Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Down(_) => "Merge Down",
            Self::Group(_) => "Merge Group",
            Self::Selected => "Merge Layers",
            Self::Visible => "Merge Visible",
            Self::Flatten => "Flatten Image",
            Self::StampDown(_) => "Stamp Down",
            Self::StampGroup(_) => "Stamp Group",
            Self::StampSelected => "Stamp Layers",
            Self::StampVisible => "Stamp Visible",
        }
    }

    pub(crate) fn stamp(self) -> Self {
        match self {
            Self::Down(id) => Self::StampDown(id),
            Self::Group(id) => Self::StampGroup(id),
            Self::Selected => Self::StampSelected,
            Self::Visible => Self::StampVisible,
            other => other,
        }
    }

    fn source(self) -> Self {
        match self {
            Self::StampDown(id) => Self::Down(id),
            Self::StampGroup(id) => Self::Group(id),
            Self::StampSelected => Self::Selected,
            Self::StampVisible => Self::Visible,
            other => other,
        }
    }

    fn keeps_originals(self) -> bool {
        matches!(
            self,
            Self::StampGroup(_) | Self::StampSelected | Self::StampVisible
        )
    }
}

#[derive(Clone)]
pub(crate) struct Target {
    pub document: String,
    pub revision: Instant,
}

pub(crate) struct Request {
    pub target: Target,
    source: Document,
    kind: Kind,
    roots: Vec<usize>,
    replaced: usize,
    removed: Vec<usize>,
    time: f32,
    insertion: Option<(usize, Option<u64>)>,
}

pub(crate) struct Prepared {
    target: Target,
    command: Cmd,
    layer: u64,
    kind: Kind,
    roots: Vec<u64>,
    insertion: Option<usize>,
}

fn index(doc: &Document, id: u64) -> Result<usize, String> {
    doc.layers
        .iter()
        .position(|l| l.id == id)
        .ok_or_else(|| "The merge layer is missing".into())
}

fn roots(doc: &Document, kind: Kind) -> Result<(Vec<usize>, usize), String> {
    match kind.source() {
        Kind::Down(id) => {
            let upper = index(doc, id)?;
            let parent = doc.layers[upper].parent;
            let lower = (0..upper)
                .rev()
                .find(|&i| doc.layers[i].parent == parent)
                .ok_or("There is no sibling layer below to merge into")?;
            Ok((vec![lower, upper], lower))
        }
        Kind::Group(id) => {
            let i = index(doc, id)?;
            if !doc.layers[i].is_group {
                return Err("Choose a group to merge".into());
            }
            if !doc.layer_visible(i) {
                return Err("The group is hidden".into());
            }
            Ok((vec![i], i))
        }
        Kind::Visible => {
            let roots: Vec<_> = doc
                .layers
                .iter()
                .enumerate()
                .filter(|(i, l)| l.parent.is_none() && doc.layer_visible(*i))
                .map(|(i, _)| i)
                .collect();
            if kind != Kind::StampVisible
                && roots.len() < 2
                && !roots.iter().any(|&i| doc.layers[i].is_group)
            {
                return Err("There are not enough visible layers to merge".into());
            }
            let first = *roots.first().ok_or("No visible layers to merge")?;
            Ok((roots, first))
        }
        Kind::Flatten => {
            let roots: Vec<_> = doc
                .layers
                .iter()
                .enumerate()
                .filter(|(_, l)| l.parent.is_none())
                .map(|(i, _)| i)
                .collect();
            let first = *roots.first().ok_or("No layers to flatten")?;
            Ok((roots, first))
        }
        _ => Err("Choose at least two sibling layers to merge".into()),
    }
}

impl Request {
    /// Prepare complete output and history without touching the live editor.
    /// Args: existing worker progress/cancellation owner. Returns one validated transaction or an atomic failure.
    pub(crate) fn render(self, progress: &dyn Progress) -> Result<Prepared, String> {
        progress.check()?;
        let mut source = self.source.clone();
        if matches!(self.kind, Kind::StampDown(_)) {
            source.layers[self.replaced].visible = true;
        }
        let original = &self.source.layers[self.replaced];
        if matches!(self.kind.source(), Kind::Group(_)) {
            let group = &mut source.layers[self.replaced];
            group.opacity = 1.;
            group.blend = crate::color::Blend::Normal;
            group.pass_through = false;
        }
        let (pixels, origin) =
            if self.kind == Kind::Flatten && !crate::deep_color::has_native(&source) {
                progress.report("Flattening current frame", 0, 1);
                let count = crate::deep_color::storage::count(
                    source.width.round().max(1.) as u32,
                    source.height.round().max(1.) as u32,
                    4,
                )?;
                let _memory =
                    crate::resources::global().reserve_now(count as u64, &|| progress.check())?;
                let mut keep = || !progress.cancelled();
                let picture = crate::compositor::render_export_at_cancellable(
                    &source, 1., self.time, false, &mut keep,
                )?;
                progress.check()?;
                (
                    crate::document::Pixels::try_from_pixmap(&picture)?,
                    Pt::ZERO,
                )
            } else {
                if self.kind == Kind::Flatten {
                    crate::motion::require_flat_artwork(&source, "Native Flatten Image")?;
                }
                crate::compositor::layer_merge::render(
                    &source,
                    &self.roots,
                    self.time,
                    matches!(self.kind.source(), Kind::Visible | Kind::Flatten),
                    self.kind == Kind::Flatten,
                    progress,
                )?
            };
        progress.check()?;
        let size = Pt::new(pixels.w as f32, pixels.h as f32);
        let mut layer = Layer::placed_raster(
            if self.kind == Kind::Flatten {
                "Background"
            } else {
                &original.name
            },
            pixels,
            origin,
            size,
        );
        if self.insertion.is_none() {
            layer.id = original.id;
        }
        layer.parent = self.insertion.map_or(original.parent, |(_, parent)| parent);
        layer.locked = !self.kind.keeps_originals() && original.locked;
        layer.clipped = self.source.layers[self.roots[0]].clipped
            && matches!(
                self.kind.source(),
                Kind::Down(_) | Kind::Group(_) | Kind::Selected
            );
        if matches!(self.kind.source(), Kind::Down(_)) {
            layer.blend = original.blend;
        }
        if matches!(self.kind, Kind::StampDown(_)) {
            layer.visible = original.visible;
        }
        if matches!(self.kind.source(), Kind::Group(_)) {
            layer.opacity = original.opacity;
            layer.blend = if original.pass_through {
                crate::color::Blend::Normal
            } else {
                original.blend
            };
        }
        if self.kind == Kind::Flatten {
            layer.parent = None;
            layer.locked = true;
        }
        let id = layer.id;
        let command = if let Some((index, _)) = self.insertion {
            layer.name = format!("{} (merged)", original.name);
            Cmd::AddLayer { index, layer }
        } else {
            Cmd::SetSmartLayer {
                index: self.replaced,
                before: Box::new(original.clone()),
                after: Box::new(layer),
            }
        };
        let mut commands = vec![command];
        for &i in self.removed.iter().rev().filter(|&&i| i != self.replaced) {
            commands.push(Cmd::RemoveLayer {
                index: i,
                layer: self.source.layers[i].clone(),
            });
        }
        let mut motion = self.source.motion.clone();
        let ids: Vec<_> = self
            .removed
            .iter()
            .flat_map(|&i| {
                let layer = &self.source.layers[i];
                std::iter::once(layer.id)
                    .chain(layer.kind.shapes().unwrap_or(&[]).iter().map(|s| s.id))
            })
            .collect();
        motion.drop_shapes(&ids);
        if self.kind == Kind::Flatten {
            motion.compositions.clear();
            motion.scene3d = None;
        }
        if motion != self.source.motion {
            commands.push(Cmd::SetMotion {
                before: self.source.motion.clone(),
                after: motion,
            });
        }
        if self.kind == Kind::Flatten && self.source.transparent {
            let before = crate::document::CanvasSettings::read(&self.source);
            let mut after = before.clone();
            after.transparent = false;
            commands.push(Cmd::SetCanvas { before, after });
        }
        let mut candidate = self.source.clone();
        for command in &commands {
            apply_cmd(&mut candidate, command);
        }
        let remaining_sources = candidate.mask_sources();
        for old in self
            .source
            .mask_sources()
            .into_iter()
            .filter(|_| !self.kind.keeps_originals())
        {
            let Some((source_index, _)) = old.resolve(&self.source) else {
                continue;
            };
            if self.removed.contains(&source_index) {
                continue;
            }
            let Some((index, shape)) = old.resolve(&candidate) else {
                continue;
            };
            let current = crate::artwork_mask::MaskSource {
                layer: candidate.layers[index].id,
                shape,
            };
            if remaining_sources.contains(&current) {
                continue;
            }
            let command = if let Some(id) = shape {
                let shape = candidate.find_shape(index, id).unwrap();
                if !shape.visible {
                    continue;
                }
                let mut parent = shape.layout.parent;
                for _ in 0..64 {
                    let Some(owner) = parent.and_then(|id| candidate.find_shape(index, id)) else {
                        break;
                    };
                    if owner.layout.stack.is_some() {
                        return Err("This live mask source participates in auto layout. Detach the source from auto layout before baking its mask".into());
                    }
                    parent = owner.layout.parent;
                }
                Cmd::SetShapeMeta {
                    layer: index,
                    id,
                    name: shape.name.clone(),
                    visible: false,
                    locked: shape.locked,
                    before: (shape.name.clone(), shape.visible, shape.locked),
                }
            } else {
                let layer = &candidate.layers[index];
                if !layer.visible {
                    continue;
                }
                Cmd::SetLayerMeta {
                    index,
                    name: layer.name.clone(),
                    visible: false,
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
                }
            };
            apply_cmd(&mut candidate, &command);
            commands.push(command);
        }
        candidate
            .motion
            .evaluate_frame(self.time, &mut || !progress.cancelled())
            .map_err(|error| {
                format!("The merge would leave an unresolved animation channel: {error}")
            })?;
        let command = Cmd::Batch(commands);
        layer_copy::validate(&self.source, &command)?;
        progress.check()?;
        Ok(Prepared {
            target: self.target,
            command,
            layer: if matches!(self.kind, Kind::StampDown(_)) {
                self.source.layers[self.roots[1]].id
            } else {
                id
            },
            kind: self.kind,
            roots: self
                .roots
                .iter()
                .map(|&i| self.source.layers[i].id)
                .collect(),
            insertion: self.insertion.map(|(i, _)| i),
        })
    }
}

impl Studio {
    pub(crate) fn pixel_merge_shortcut(&self, visible: bool) -> Option<Kind> {
        if self.persona != Persona::Pixel {
            return None;
        }
        if visible {
            Some(Kind::Visible)
        } else {
            if self.merge_row_roots().len() > 1 {
                return Some(Kind::Selected);
            }
            self.active_layer
                .and_then(|i| self.doc.layers.get(i))
                .map(|l| {
                    if l.is_group {
                        Kind::Group(l.id)
                    } else {
                        Kind::Down(l.id)
                    }
                })
        }
    }

    fn merge_row_roots(&self) -> Vec<usize> {
        let rows = self.selected_layer_rows();
        rows.iter()
            .copied()
            .filter(|&i| i < self.doc.layers.len())
            .filter(|&i| !self.doc.layer_ancestors(i).iter().any(|p| rows.contains(p)))
            .collect()
    }

    fn selected_merge_roots(&self) -> Result<Vec<usize>, String> {
        if self
            .selected_layer_rows()
            .iter()
            .any(|&i| i >= self.doc.layers.len())
        {
            return Err("The selected layer rows changed".into());
        }
        let roots = self.merge_row_roots();
        if roots.len() < 2 {
            return Err("Choose at least two sibling layers to merge".into());
        }
        let parent = self.doc.layers[roots[0]].parent;
        if roots.iter().any(|&i| self.doc.layers[i].parent != parent) {
            return Err("Merge Layers currently needs selected siblings in the same group".into());
        }
        if !roots.iter().any(|&i| self.doc.layer_visible(i)) {
            return Err("The selected layers are all hidden".into());
        }
        Ok(roots)
    }

    pub(crate) fn layer_merge_primary(&self, owner: Option<u64>) -> Option<Kind> {
        let id = owner?;
        let roots = self.merge_row_roots();
        if roots.len() > 1 && roots.iter().any(|&i| self.doc.layers[i].id == id) {
            return Some(Kind::Selected);
        }
        self.doc.layers.iter().find(|l| l.id == id).map(|l| {
            if l.is_group {
                Kind::Group(id)
            } else {
                Kind::Down(id)
            }
        })
    }

    /// Capture the concrete merge scope before starting a background render.
    /// Args: requested layer operation. Returns a captured document/revision and immutable source or a readable refusal.
    fn layer_merge_scope(&self, kind: Kind) -> Result<(Vec<usize>, usize, Vec<usize>), String> {
        if self.show_welcome
            || self.op.is_some()
            || self.deformation.is_some()
            || self.free_transform.is_some()
            || self.pending_nav.is_some()
            || self.file_dialog_pending()
            || self.updates.freezing
        {
            return Err("Finish the current operation before merging layers".into());
        }
        let (roots, replaced) = if kind.source() == Kind::Selected {
            let roots = self.selected_merge_roots()?;
            let top = *roots.last().unwrap();
            (roots, top)
        } else {
            roots(&self.doc, kind)?
        };
        let ids: HashSet<_> = roots.iter().map(|&i| self.doc.layers[i].id).collect();
        let mut removed: Vec<_> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .filter(|(i, l)| {
                ids.contains(&l.id)
                    || self
                        .doc
                        .layer_ancestors(*i)
                        .iter()
                        .any(|&a| ids.contains(&self.doc.layers[a].id))
            })
            .map(|(i, _)| i)
            .collect();
        if kind.keeps_originals() {
            removed.clear();
        } else if matches!(kind, Kind::StampDown(_)) {
            if self.doc.layers[replaced].kind.pixels().is_none() {
                return Err("Stamp Down needs a pixel layer below".into());
            }
            if !self.doc.layer_visible(roots[1]) {
                return Err("The stamp layer is hidden".into());
            }
            removed.retain(|&i| i == replaced);
        }
        if removed.iter().any(|&i| !self.layer_unlocked(i)) {
            return Err("Unlock the affected layers before merging".into());
        }
        if kind.keeps_originals() {
            let above = if kind == Kind::StampVisible {
                self.active_layer
                    .ok_or("Choose an active layer for the stamp")?
            } else {
                replaced
            };
            if self.doc.layers.get(above).is_none() {
                return Err("The stamp destination is missing".into());
            }
            if !self.layer_ancestors_unlocked(above) {
                return Err("Unlock the destination group before stamping".into());
            }
        }
        Ok((roots, replaced, removed))
    }

    pub(crate) fn can_layer_merge(&self, kind: Kind) -> bool {
        self.layer_merge_scope(kind).is_ok()
    }

    pub(crate) fn capture_layer_merge(&self, kind: Kind) -> Result<Request, String> {
        self.doc.validate_hierarchy()?;
        let (roots, replaced, removed) = self.layer_merge_scope(kind)?;
        let insertion = kind.keeps_originals().then(|| {
            let above = if kind == Kind::StampVisible {
                self.active_layer.unwrap()
            } else {
                replaced
            };
            (above + 1, self.doc.layers[above].parent)
        });
        Ok(Request {
            target: Target {
                document: self.swap_id.clone(),
                revision: self.last_input,
            },
            source: self.doc.clone(),
            kind,
            roots,
            replaced,
            removed,
            time: self.playhead,
            insertion,
        })
    }

    /// Publish a completed merge only to its unchanged owning document.
    /// Args: captured worker transaction. Returns false for stale/busy destinations and leaves their history unchanged.
    pub(crate) fn apply_layer_merge(&mut self, prepared: Prepared) -> Result<(), String> {
        if self.swap_id != prepared.target.document || self.last_input != prepared.target.revision {
            return Err("The document changed while rendering. No layers were merged".into());
        }
        let (roots, _, _) = self.layer_merge_scope(prepared.kind)?;
        if prepared.kind.source() == Kind::Selected
            && roots
                .iter()
                .map(|&i| self.doc.layers[i].id)
                .collect::<Vec<_>>()
                != prepared.roots
        {
            return Err(
                "The selected layers changed while rendering. No layers were merged".into(),
            );
        }
        if prepared.kind == Kind::StampVisible
            && self.active_layer.map(|i| i + 1) != prepared.insertion
        {
            return Err(
                "The stamp destination changed while rendering. No layers were added".into(),
            );
        }
        self.commit_type_edit();
        self.commit(prepared.command);
        if let Some(index) = self.doc.layers.iter().position(|l| l.id == prepared.layer) {
            self.activate_layer_tree(index);
        }
        self.status = format!("{} complete", prepared.kind.name());
        Ok(())
    }
}

#[cfg(test)]
#[path = "layer_merge_tests.rs"]
mod tests;
