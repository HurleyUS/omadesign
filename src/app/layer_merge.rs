use super::*;
use crate::ml::Progress;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Down(u64),
    Group(u64),
    Visible,
    Flatten,
}
impl Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Down(_) => "Merge Down",
            Self::Group(_) => "Merge Group",
            Self::Visible => "Merge Visible",
            Self::Flatten => "Flatten Image",
        }
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
}

pub(crate) struct Prepared {
    target: Target,
    command: Cmd,
    layer: u64,
    kind: Kind,
}

fn index(doc: &Document, id: u64) -> Result<usize, String> {
    doc.layers
        .iter()
        .position(|l| l.id == id)
        .ok_or_else(|| "The merge layer is missing".into())
}

fn roots(doc: &Document, kind: Kind) -> Result<(Vec<usize>, usize), String> {
    match kind {
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
                .filter(|(_, l)| l.parent.is_none() && l.visible)
                .map(|(i, _)| i)
                .collect();
            if roots.len() < 2 && !roots.iter().any(|&i| doc.layers[i].is_group) {
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
    }
}

impl Request {
    /// Prepare complete output and history without touching the live editor.
    /// Args: existing worker progress/cancellation owner. Returns one validated transaction or an atomic failure.
    pub(crate) fn render(self, progress: &dyn Progress) -> Result<Prepared, String> {
        progress.check()?;
        let mut source = self.source.clone();
        let original = &self.source.layers[self.replaced];
        if matches!(self.kind, Kind::Group(_)) {
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
                    matches!(self.kind, Kind::Visible | Kind::Flatten),
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
        layer.id = original.id;
        layer.parent = original.parent;
        layer.locked = original.locked;
        if matches!(self.kind, Kind::Down(_)) {
            layer.blend = original.blend;
        }
        if matches!(self.kind, Kind::Group(_)) {
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
        let mut commands = vec![Cmd::SetSmartLayer {
            index: self.replaced,
            before: Box::new(original.clone()),
            after: Box::new(layer),
        }];
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
        for old in self.source.mask_sources() {
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
            layer: id,
            kind: self.kind,
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
        let (roots, replaced) = roots(&self.doc, kind)?;
        let ids: HashSet<_> = roots.iter().map(|&i| self.doc.layers[i].id).collect();
        let removed: Vec<_> = self
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
        if removed.iter().any(|&i| !self.layer_unlocked(i)) {
            return Err("Unlock the affected layers before merging".into());
        }
        Ok((roots, replaced, removed))
    }

    pub(crate) fn can_layer_merge(&self, kind: Kind) -> bool {
        self.layer_merge_scope(kind).is_ok()
    }

    pub(crate) fn capture_layer_merge(&self, kind: Kind) -> Result<Request, String> {
        self.doc.validate_hierarchy()?;
        let (roots, replaced, removed) = self.layer_merge_scope(kind)?;
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
        })
    }

    /// Publish a completed merge only to its unchanged owning document.
    /// Args: captured worker transaction. Returns false for stale/busy destinations and leaves their history unchanged.
    pub(crate) fn apply_layer_merge(&mut self, prepared: Prepared) -> Result<(), String> {
        if self.swap_id != prepared.target.document || self.last_input != prepared.target.revision {
            return Err("The document changed while rendering. No layers were merged".into());
        }
        self.layer_merge_scope(prepared.kind)?;
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
