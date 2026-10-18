use super::*;
use crate::layout_components::ComponentBinding;
use crate::layout_prototype::PrototypeAction;
use crate::layout_tokens::TokenProperty;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LayerCopyTarget {
    pub document: String,
    pub revision: Instant,
    pub layer: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LayerCopyDestination {
    Document { document: String, revision: Instant },
    New { name: String },
}

fn name(value: &str, fallback: &str) -> Result<String, String> {
    let value = if value.trim().is_empty() {
        fallback
    } else {
        value.trim()
    };
    if value.len() > 1024 || value.chars().any(char::is_control) {
        return Err("Use a name of at most 1024 bytes without control characters".into());
    }
    Ok(value.to_owned())
}

fn shape_layer(doc: &Document, id: u64) -> Result<usize, String> {
    doc.layers
        .iter()
        .position(|l| l.find(id).is_some())
        .ok_or_else(|| format!("The copied layer references missing object {id}"))
}

fn shape_dependencies(
    doc: &Document,
    shape: &Shape,
    layers: &mut Vec<usize>,
) -> Result<(), String> {
    for mask in &shape.artwork_masks {
        layers.push(
            mask.source
                .resolve(doc)
                .ok_or("The copied mask source is missing")?
                .0,
        );
    }
    if let Geom::Text(run) = &shape.geom
        && let Some(path) = &run.on_path
    {
        layers.push(shape_layer(doc, path.path_id)?);
    }
    for interaction in &shape.layout.interactions {
        match interaction.action {
            PrototypeAction::Navigate { target } | PrototypeAction::OpenOverlay { target } => {
                layers.push(shape_layer(doc, target)?);
            }
            PrototypeAction::SetVariant { component } => layers.push(shape_layer(doc, component)?),
            _ => {}
        }
    }
    if let Some(binding) = &shape.layout.component {
        let family = match binding {
            ComponentBinding::Main { family, .. } => Some(*family),
            ComponentBinding::Instance { main, nodes } => {
                layers.push(shape_layer(doc, *main)?);
                for node in nodes {
                    if let Some(main) = node.nested_main {
                        layers.push(shape_layer(doc, main)?);
                    }
                    shape_dependencies(doc, &node.baseline, layers)?;
                }
                crate::layout_components::family_of(doc, *main)
            }
        };
        if let Some(family) = family {
            for (i, layer) in doc.layers.iter().enumerate() {
                if layer.kind.shapes().unwrap_or(&[]).iter().any(|s| {
                    matches!(s.layout.component, Some(ComponentBinding::Main {family: f, ..}) if f == family)
                }) {
                    layers.push(i);
                }
            }
        }
    }
    Ok(())
}

fn indices(doc: &Document, root: usize) -> Result<Vec<usize>, String> {
    let mut selected: HashSet<_> = (0..=root)
        .filter(|&i| i == root || doc.layer_ancestors(i).contains(&root))
        .collect();
    let mut inspected = HashSet::new();
    loop {
        let pending: Vec<_> = selected.difference(&inspected).copied().collect();
        if pending.is_empty() {
            break;
        }
        for i in pending {
            inspected.insert(i);
            let layer = &doc.layers[i];
            let mut needed = Vec::new();
            for mask in &layer.artwork_masks {
                needed.push(
                    mask.source
                        .resolve(doc)
                        .ok_or("The copied mask source is missing")?
                        .0,
                );
            }
            for shape in layer.kind.shapes().unwrap_or(&[]) {
                shape_dependencies(doc, shape, &mut needed)?;
            }
            for i in needed {
                if selected.contains(&i) {
                    continue;
                }
                let ancestor = doc.layer_ancestors(i).last().copied().unwrap_or(i);
                selected.extend(
                    (0..=ancestor)
                        .filter(|&i| i == ancestor || doc.layer_ancestors(i).contains(&ancestor)),
                );
            }
        }
    }
    let mut result: Vec<_> = selected.into_iter().collect();
    result.sort_unstable();
    Ok(result)
}

fn each_shape(layers: &mut [Layer], mut f: impl FnMut(&mut Shape)) {
    for layer in layers {
        for shape in layer.kind.shapes_mut().into_iter().flatten() {
            f(shape);
            if let Some(ComponentBinding::Instance { nodes, .. }) = &mut shape.layout.component {
                for node in nodes {
                    f(&mut node.baseline);
                }
            }
        }
    }
}

fn transfer(
    source: &Document,
    root: usize,
    destination: &Document,
    copy_name: &str,
    start: usize,
    parent: Option<u64>,
) -> Result<(Cmd, u64, usize), String> {
    source.validate_hierarchy()?;
    crate::layout_components::validate_properties(source)?;
    let mut copied = layer_copy::prepare_indices(source, root, indices(source, root)?, Pt::ZERO)?;
    let root_id = copied.layer_ids[&source.layers[root].id];
    let members: HashSet<_> = (0..=root)
        .filter(|&i| i == root || source.layer_ancestors(i).contains(&root))
        .map(|i| copied.layer_ids[&source.layers[i].id])
        .collect();
    let mut dependencies = Layer::group("Copy dependencies");
    dependencies.parent = parent;
    dependencies.visible = false;
    dependencies.locked = true;
    let mut other = Vec::new();
    let mut visible = Vec::new();
    let all_ids: HashSet<_> = copied.layer_ids.values().copied().collect();
    for mut layer in copied.layers {
        if members.contains(&layer.id) {
            if layer.id == root_id {
                layer.parent = parent;
                layer.name = copy_name.to_owned();
            }
            visible.push(layer);
        } else {
            if layer.parent.is_none_or(|id| !all_ids.contains(&id)) {
                layer.parent = Some(dependencies.id);
            }
            other.push(layer);
        }
    }
    let mut layers = Vec::new();
    if !other.is_empty() {
        layers.append(&mut other);
        layers.push(dependencies);
    }
    layers.append(&mut visible);

    let mut tokens = destination.layout_tokens.clone();
    let mut token_ids = HashMap::new();
    each_shape(&mut layers, |shape| {
        for property in TokenProperty::all() {
            if let Some(id) = shape.layout.tokens.get(property) {
                token_ids.entry(id).or_insert_with(crate::document::next_id);
            }
        }
    });
    if token_ids
        .keys()
        .any(|id| !source.layout_tokens.iter().any(|t| t.id == *id))
    {
        return Err("A copied object references a missing design token".into());
    }
    for original in source
        .layout_tokens
        .iter()
        .filter(|t| token_ids.contains_key(&t.id))
    {
        let mut token = original.clone();
        token.id = token_ids[&original.id];
        let base = token.name.clone();
        let mut suffix = 2;
        while tokens
            .iter()
            .any(|t| t.name.to_lowercase() == token.name.to_lowercase())
        {
            let ending = format!(" {suffix}");
            let mut prefix = base.clone();
            while prefix.len() + ending.len() > 128 {
                prefix.pop();
            }
            token.name = format!("{}{ending}", prefix.trim_end());
            suffix += 1;
        }
        tokens.push(token);
    }
    each_shape(&mut layers, |shape| {
        for property in TokenProperty::all() {
            if let Some(id) = shape.layout.tokens.get(property) {
                shape.layout.tokens.set(property, Some(token_ids[&id]));
            }
        }
    });
    let mut sources = destination.sources.clone();
    for layer in &layers {
        if let Some(id) = &layer.source_attachment {
            let source = source
                .source(id)
                .ok_or("The copied original source is missing")?;
            if let Some(existing) = sources.iter().find(|s| s.id == *id) {
                if existing != source {
                    return Err(
                        "The destination contains a different original with the same source ID"
                            .into(),
                    );
                }
            } else {
                sources.push(source.clone());
            }
        }
    }
    let mut scratch = Document::new("Layer copy", source.width, source.height, source.dpi);
    scratch.color = source.color.clone();
    scratch.layers = layers;
    scratch.layout_tokens = tokens.clone();
    crate::layout_tokens::validate_tokens(&scratch)?;
    if source.color != destination.color {
        let conversion = scratch.color_conversion(destination.color.clone())?;
        apply_cmd(&mut scratch, &conversion);
    }
    copied.layers = scratch.layers;
    let mut motion = destination.motion.clone();
    for expression in &source.motion.expressions {
        if copied.shape_ids.contains_key(&expression.shape)
            || copied.layer_ids.contains_key(&expression.shape)
        {
            if expression
                .source
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .any(|s| s == "property")
            {
                return Err("This layer has an expression channel reference. Duplicate it in this document to preserve that reference".into());
            }
        }
    }
    layer_copy::append_motion(
        &source.motion,
        &mut motion,
        &copied.shape_ids,
        &copied.layer_ids,
    );
    if motion.tracks.len() != destination.motion.tracks.len()
        || motion.expressions.len() != destination.motion.expressions.len()
    {
        motion.duration = motion.duration.max(source.motion.duration);
    }
    let root_index = start + copied.layers.iter().position(|l| l.id == root_id).unwrap();
    let mut commands: Vec<_> = copied
        .layers
        .into_iter()
        .enumerate()
        .map(|(offset, layer)| Cmd::AddLayer {
            index: start + offset,
            layer,
        })
        .collect();
    if tokens != destination.layout_tokens {
        commands.push(Cmd::SetLayoutTokens {
            before: destination.layout_tokens.clone(),
            after: tokens,
        });
    }
    if sources != destination.sources {
        commands.push(Cmd::SetSources {
            before: destination.sources.clone(),
            after: sources,
        });
    }
    if motion != destination.motion {
        commands.push(Cmd::SetMotion {
            before: destination.motion.clone(),
            after: motion,
        });
    }
    let command = Cmd::Batch(commands);
    layer_copy::validate(destination, &command)?;
    Ok((command, root_id, root_index))
}

impl Studio {
    /// Capture a concrete layer without changing its document or history.
    /// Args: source index. Returns a stable document, revision and layer identity or an unavailable-target error.
    pub(crate) fn capture_layer_copy(&self, index: usize) -> Result<LayerCopyTarget, String> {
        if !self.can_duplicate_layer_tree(index) || self.show_welcome {
            return Err("Finish the current operation before duplicating a layer".into());
        }
        Ok(LayerCopyTarget {
            document: self.swap_id.clone(),
            revision: self.last_input,
            layer: self.doc.layers[index].id,
        })
    }

    /// Resolve a copy source only while its captured document and revision are current.
    /// Args: captured source. Returns its current layer index, otherwise a readable stale-target error.
    pub(crate) fn layer_copy_current(&self, target: &LayerCopyTarget) -> Result<usize, String> {
        let index = self
            .doc
            .layers
            .iter()
            .position(|l| l.id == target.layer)
            .ok_or("The source layer changed. Reopen Duplicate Layer")?;
        if self.swap_id != target.document
            || self.last_input != target.revision
            || !self.can_duplicate_layer_tree(index)
        {
            return Err("The source document changed. Reopen Duplicate Layer".into());
        }
        Ok(index)
    }

    /// Duplicate one complete layer with a name and a captured document destination.
    /// Args: captured source, copy name and current/open/new destination. Returns an error before any mutation when preparation fails.
    pub(crate) fn duplicate_layer_to(
        &mut self,
        target: &LayerCopyTarget,
        copy_name: &str,
        destination: &LayerCopyDestination,
    ) -> Result<(), String> {
        let root = self.layer_copy_current(target)?;
        let copy_name = name(copy_name, &format!("{} copy", self.doc.layers[root].name))?;
        if let LayerCopyDestination::Document { document, revision } = destination
            && document == &self.swap_id
        {
            if *revision != self.last_input {
                return Err("The destination document changed. Reopen Duplicate Layer".into());
            }
            if !self.duplicate_layer_named(root, Pt::ZERO, Some(&copy_name)) {
                return Err(self.status.clone());
            }
        } else {
            let (destination_index, mut new_doc) = match destination {
                LayerCopyDestination::Document { document, revision } => {
                    let index = (0..self.tab_count())
                        .find(|&i| {
                            self.tab_preview_source(i)
                                .is_some_and(|(_, id, rev, _)| id == document && rev == *revision)
                        })
                        .ok_or(
                            "The destination document changed or closed. Reopen Duplicate Layer",
                        )?;
                    (Some(index), None)
                }
                LayerCopyDestination::New { name: title } => {
                    let mut doc = Document::new(
                        &name(title, &format!("{} copy", self.doc.name))?,
                        self.doc.width,
                        self.doc.height,
                        self.doc.dpi,
                    );
                    doc.layers.clear();
                    doc.color = self.doc.color.clone();
                    doc.workspace = Some(self.persona);
                    doc.transparent = true;
                    doc.text_wrap_above_only = self.doc.text_wrap_above_only;
                    doc.motion.duration = self.doc.motion.duration;
                    doc.motion.fps = self.doc.motion.fps;
                    doc.motion.looped = self.doc.motion.looped;
                    (None, Some(doc))
                }
            };
            let to = new_doc.as_ref().unwrap_or_else(|| {
                self.tab_preview_source(destination_index.unwrap())
                    .unwrap()
                    .0
            });
            let (start, parent) = destination_index
                .and_then(|i| self.layer_copy_insertion(i))
                .unwrap_or((0, None));
            let (command, root_id, root_index) =
                transfer(&self.doc, root, to, &copy_name, start, parent)?;
            if let Some(doc) = new_doc.take() {
                self.push_tab(TabState::new(doc, None));
            } else {
                self.switch_tab(destination_index.unwrap());
            }
            self.commit(command);
            if destination_index.is_none() {
                self.history.clear();
            }
            self.activate_layer_tree(root_index);
            self.selected_layer = Some(root_id);
        }
        self.status = format!("Duplicated {copy_name}");
        Ok(())
    }
}

#[cfg(test)]
#[path = "layer_copy_destination_tests.rs"]
mod tests;
