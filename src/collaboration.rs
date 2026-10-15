//! Durable property merges and author-owned undo for live native documents.

use crate::document::Document;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

type Snapshot = BTreeMap<String, Cell>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Cell {
    Object,
    List,
    Variant(String),
    Value(Value),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Id {
    pub actor: String,
    pub seq: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Write {
    pub path: String,
    pub value: Option<Cell>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Edit {
    Write { writes: Vec<Write> },
    Toggle { target: Id, enabled: bool },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub id: Id,
    pub clock: u64,
    pub seen: BTreeMap<String, u64>,
    pub edit: Edit,
}

/// Saved collaboration state. Credentials and ephemeral presence never enter `.oma`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Replica {
    pub session: String,
    pub actor: String,
    #[serde(default)]
    pub project: String,
    pub base: Snapshot,
    pub changes: Vec<Change>,
    #[serde(default)]
    pub pending: BTreeSet<Id>,
    #[serde(default)]
    pub cursor: u64,
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    undo: Vec<Id>,
    #[serde(default)]
    redo: Vec<Id>,
}

/// Turn native editable state into stable property addresses.
/// Args: document to share. Returns its cells, excluding device and session metadata.
pub fn snapshot(doc: &Document) -> Result<Snapshot, String> {
    let mut value = serde_json::to_value(doc).map_err(|e| e.to_string())?;
    for key in ["cloud", "collaboration", "workspace"] {
        value.as_object_mut().unwrap().remove(key);
    }
    for (index, layer) in doc.layers.iter().enumerate() {
        for (position, shape) in layer.kind.shapes().unwrap_or_default().iter().enumerate() {
            value["layers"][index]["kind"]["Vector"]["shapes"][position]["layout"] =
                serde_json::to_value(&shape.layout).map_err(|e| e.to_string())?;
        }
    }
    let mut result = BTreeMap::new();
    flatten("", &value, &mut result)?;
    Ok(result)
}

fn child(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn flatten(path: &str, value: &Value, out: &mut Snapshot) -> Result<(), String> {
    match value {
        Value::Object(fields)
            if fields.len() == 1
                && fields.keys().next().is_some_and(|key| {
                    key.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
                }) =>
        {
            let (variant, payload) = fields.iter().next().unwrap();
            out.insert(path.into(), Cell::Variant(variant.clone()));
            flatten(&child(path, variant), payload, out)?;
        }
        Value::Object(fields) => {
            out.insert(path.into(), Cell::Object);
            for (key, value) in fields {
                flatten(&child(path, key), value, out)?;
            }
        }
        Value::Array(items)
            if ["/layers", "/artboards", "/comments", "/layout_tokens"].contains(&path)
                || (path.starts_with("/@entities/")
                    && path.ends_with("/kind/Vector/shapes")
                    && path.split('/').count() == 6) =>
        {
            out.insert(path.into(), Cell::List);
            let mut previous = Value::Null;
            let mut ids = BTreeSet::new();
            for item in items {
                let id = item
                    .get("id")
                    .and_then(Value::as_u64)
                    .ok_or("Live objects need stable IDs")?
                    .to_string();
                if !ids.insert(id.clone()) || out.contains_key(&child("/@parent", &id)) {
                    return Err("Duplicate object IDs must be repaired before live editing".into());
                }
                out.insert(
                    child("/@parent", &id),
                    Cell::Value(Value::String(path.into())),
                );
                flatten(&child("/@entities", &id), item, out)?;
                out.insert(child("/@position", &id), Cell::Value(previous));
                previous = Value::String(id);
            }
        }
        _ => {
            out.insert(path.into(), Cell::Value(value.clone()));
        }
    }
    Ok(())
}

fn inflate(path: &str, cells: &Snapshot) -> Option<Value> {
    match cells.get(path)? {
        Cell::Value(value) => Some(value.clone()),
        Cell::Variant(variant) => Some(Value::Object(
            [(variant.clone(), inflate(&child(path, variant), cells)?)]
                .into_iter()
                .collect(),
        )),
        Cell::Object => {
            let prefix = format!("{path}/");
            let fields = cells
                .range(prefix.clone()..)
                .take_while(|(key, _)| key.starts_with(&prefix))
                .filter_map(|(key, _)| {
                    let tail = key.strip_prefix(&prefix)?;
                    if tail.contains('/') {
                        return None;
                    }
                    let name = tail.replace("~1", "/").replace("~0", "~");
                    inflate(key, cells).map(|value| (name, value))
                })
                .collect();
            Some(Value::Object(fields))
        }
        Cell::List => {
            let prefix = "/@parent/".to_string();
            let positions: BTreeMap<String, Option<String>> = cells
                .range(prefix.clone()..)
                .take_while(|(key, _)| key.starts_with(&prefix))
                .filter_map(|(key, cell)| match cell {
                    Cell::Value(value) if value.as_str() == Some(path) => {
                        let id = &key[prefix.len()..];
                        let after = match cells.get(&child("/@position", id)) {
                            Some(Cell::Value(value)) => value.as_str().map(str::to_owned),
                            _ => None,
                        };
                        Some((id.into(), after))
                    }
                    _ => None,
                })
                .collect();
            let mut order = Vec::new();
            let mut visited = BTreeSet::new();
            for id in positions.keys() {
                let mut chain = Vec::new();
                let mut current = Some(id.as_str());
                while let Some(id) = current {
                    if !positions.contains_key(id) || !visited.insert(id.to_owned()) {
                        break;
                    }
                    chain.push(id.to_owned());
                    current = positions.get(id).and_then(|after| after.as_deref());
                }
                order.extend(chain.into_iter().rev());
            }
            Some(Value::Array(
                order
                    .into_iter()
                    .filter_map(|id| inflate(&child("/@entities", &id), cells))
                    .collect(),
            ))
        }
    }
}

impl Replica {
    /// Begin an independent replica of the same session seed.
    /// Args: session ID, unique client actor, seed cells. Returns empty local history.
    pub fn new(session: String, actor: String, base: Snapshot) -> Self {
        Self {
            session,
            actor,
            project: String::new(),
            base,
            changes: Vec::new(),
            pending: BTreeSet::new(),
            cursor: 0,
            device: String::new(),
            paused: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    fn append(&mut self, edit: Edit) -> Id {
        let mut seen = BTreeMap::<String, u64>::new();
        for change in &self.changes {
            seen.entry(change.id.actor.clone())
                .and_modify(|seq| *seq = (*seq).max(change.id.seq))
                .or_insert(change.id.seq);
        }
        let id = Id {
            actor: self.actor.clone(),
            seq: seen.get(&self.actor).copied().unwrap_or(0) + 1,
        };
        let clock = self.changes.iter().map(|c| c.clock).max().unwrap_or(0) + 1;
        self.changes.push(Change {
            id: id.clone(),
            clock,
            seen,
            edit,
        });
        self.pending.insert(id.clone());
        id
    }

    /// Record only properties changed locally since the last merged view.
    /// Args: edited native document. Returns whether a transaction was created.
    pub fn record(&mut self, doc: &Document) -> Result<bool, String> {
        let raw = self.cells();
        let before = snapshot(&self.document(doc)?)?;
        let after = snapshot(doc)?;
        if before == after {
            return Ok(false);
        }
        let keys: BTreeSet<_> = before.keys().chain(after.keys()).collect();
        let writes: Vec<_> = keys
            .into_iter()
            .filter(|key| {
                before.get(*key) != after.get(*key)
                    || (after.contains_key(*key) && raw.get(*key) != after.get(*key))
            })
            .filter(|key| {
                if after.contains_key(*key) || !key.starts_with("/@entities/") {
                    return true;
                }
                let id = key["/@entities/".len()..].split('/').next().unwrap();
                after.contains_key(&format!("/@entities/{id}"))
            })
            .map(|key| Write {
                path: key.clone(),
                value: after.get(key).cloned(),
            })
            .collect();
        if writes.is_empty() {
            return Ok(false);
        }
        let id = self.append(Edit::Write { writes });
        self.undo.push(id);
        self.redo.clear();
        Ok(true)
    }

    /// Receive immutable changes without entering another author's undo history.
    /// Args: remote changes. Returns an error for forged undo or changed operation IDs.
    pub fn receive(&mut self, changes: impl IntoIterator<Item = Change>) -> Result<(), String> {
        let mut added = Vec::new();
        for change in changes {
            if let Edit::Toggle { target, .. } = &change.edit
                && target.actor != change.id.actor
            {
                return Err("An author cannot undo another client's work".into());
            }
            if let Some(old) = self
                .changes
                .iter()
                .chain(added.iter())
                .find(|c| c.id == change.id)
            {
                if old != &change {
                    return Err("A live operation ID was reused".into());
                }
            } else {
                added.push(change);
            }
        }
        self.changes.extend(added);
        Ok(())
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Fork a saved session for another independently active client.
    /// Args: new actor, device fingerprint, current local artwork. Returns preserved offline edits.
    pub fn fork(&mut self, actor: String, device: String, local: &Document) -> Result<(), String> {
        self.changes
            .retain(|change| !self.pending.contains(&change.id));
        self.pending.clear();
        self.actor = actor;
        self.device = device;
        self.undo.clear();
        self.redo.clear();
        self.record(local)?;
        Ok(())
    }

    /// Undo this client's latest transaction by withdrawing its writes.
    /// Args: none. Returns whether local history changed; remote winners remain intact.
    pub fn undo(&mut self) -> bool {
        let Some(target) = self.undo.pop() else {
            return false;
        };
        self.append(Edit::Toggle {
            target: target.clone(),
            enabled: false,
        });
        self.redo.push(target);
        true
    }

    /// Restore this client's withdrawn transaction at its original priority.
    /// Args: none. Returns whether a transaction was restored.
    pub fn redo(&mut self) -> bool {
        let Some(target) = self.redo.pop() else {
            return false;
        };
        self.append(Edit::Toggle {
            target: target.clone(),
            enabled: true,
        });
        self.undo.push(target);
        true
    }

    fn active(&self) -> Vec<&Change> {
        let mut ordered: Vec<_> = self.changes.iter().collect();
        ordered.sort_by(|a, b| (a.clock, &a.id).cmp(&(b.clock, &b.id)));
        let mut enabled = BTreeMap::new();
        for change in &ordered {
            if let Edit::Toggle {
                target,
                enabled: value,
            } = &change.edit
            {
                enabled.insert(target, *value);
            }
        }
        ordered
            .into_iter()
            .filter(|c| enabled.get(&c.id).copied().unwrap_or(true))
            .collect()
    }

    fn cells(&self) -> Snapshot {
        let mut cells = self.base.clone();
        let mut parents: BTreeMap<&str, (&Change, &Option<Cell>)> = BTreeMap::new();
        for change in self.active() {
            if let Edit::Write { writes } = &change.edit {
                for write in writes {
                    if write.path.starts_with("/@parent/") {
                        if let Some((old, value)) = parents.get(write.path.as_str()) {
                            let concurrent = change.seen.get(&old.id.actor).copied().unwrap_or(0)
                                < old.id.seq
                                && old.seen.get(&change.id.actor).copied().unwrap_or(0)
                                    < change.id.seq;
                            if concurrent && value.is_some() && write.value.is_none() {
                                continue;
                            }
                        }
                        parents.insert(&write.path, (change, &write.value));
                    }
                    if let Some(value) = &write.value {
                        cells.insert(write.path.clone(), value.clone());
                    } else {
                        cells.remove(&write.path);
                    }
                }
            }
        }
        cells
    }

    /// Materialize a converged native document, retaining this device's cloud link.
    /// Args: local document for metadata. Returns editable data or a validation error.
    pub fn document(&self, local: &Document) -> Result<Document, String> {
        let cells = self.cells();
        let value = inflate("", &cells).ok_or("Live document has no root")?;
        let mut doc: Document =
            serde_json::from_value(value).map_err(|e| format!("Invalid live document: {e}"))?;
        doc.cloud = local.cloud.clone();
        doc.workspace = local.workspace;
        if doc.artboards.is_empty() {
            doc.artboardless = true;
        }
        let mut present: BTreeSet<u64> = doc
            .layers
            .iter()
            .flat_map(|layer| {
                layer
                    .kind
                    .shapes()
                    .unwrap_or_default()
                    .iter()
                    .map(|shape| shape.id)
            })
            .collect();
        for (key, parent) in cells
            .range("/@parent/".into()..)
            .take_while(|(key, _)| key.starts_with("/@parent/"))
        {
            let Cell::Value(Value::String(parent)) = parent else {
                continue;
            };
            if !parent.ends_with("/kind/Vector/shapes") {
                continue;
            }
            let id = &key["/@parent/".len()..];
            let Some(value) = inflate(&child("/@entities", id), &cells) else {
                continue;
            };
            let Ok(shape) = serde_json::from_value::<crate::document::Shape>(value) else {
                continue;
            };
            if !present.insert(shape.id) {
                continue;
            }
            let layer = doc
                .layers
                .iter()
                .enumerate()
                .filter(|(_, layer)| layer.kind.shapes().is_some() && !layer.is_group)
                .min_by_key(|(_, layer)| layer.id)
                .map(|(index, _)| index)
                .unwrap_or_else(|| {
                    let mut layer = crate::document::Layer::vector("Recovered live objects");
                    layer.id = (1..(1 << 53))
                        .rev()
                        .find(|id| !cells.contains_key(&child("/@entities", &id.to_string())))
                        .unwrap();
                    doc.layers.push(layer);
                    doc.layers.len() - 1
                });
            doc.layers[layer].kind.shapes_mut().unwrap().push(shape);
        }
        let groups: BTreeMap<_, _> = doc
            .layers
            .iter()
            .filter(|layer| layer.is_group)
            .map(|layer| (layer.id, layer.parent))
            .collect();
        for layer in &mut doc.layers {
            let mut seen = BTreeSet::from([layer.id]);
            let mut parent = layer.parent;
            while let Some(id) = parent {
                if !seen.insert(id) || seen.len() > 64 || !groups.contains_key(&id) {
                    layer.parent = None;
                    break;
                }
                parent = groups[&id];
            }
            let Some(shapes) = layer.kind.shapes_mut() else {
                continue;
            };
            let frames: BTreeMap<_, _> = shapes
                .iter()
                .filter(|shape| shape.layout.frame)
                .map(|shape| (shape.id, shape.layout.parent))
                .collect();
            for shape in shapes {
                let mut seen = BTreeSet::from([shape.id]);
                let mut parent = shape.layout.parent;
                while let Some(id) = parent {
                    if !seen.insert(id) || seen.len() > 64 || !frames.contains_key(&id) {
                        shape.layout.parent = None;
                        break;
                    }
                    parent = frames[&id];
                }
            }
        }
        doc.ensure_ids()?;
        doc.validate_hierarchy()?;
        Ok(doc)
    }

    /// Identify concurrent property writes for an explicit conflict receipt.
    /// Args: none. Returns stable addresses where concurrent values differ.
    pub fn conflicts(&self) -> BTreeSet<String> {
        let active = self.active();
        let mut writes: BTreeMap<&str, Vec<(&Change, &Option<Cell>)>> = BTreeMap::new();
        let mut conflicts = BTreeSet::new();
        let cells = self.cells();
        for (key, parent) in cells
            .range("/@parent/".into()..)
            .take_while(|(key, _)| key.starts_with("/@parent/"))
        {
            if let Cell::Value(Value::String(parent)) = parent
                && !matches!(cells.get(parent), Some(Cell::List))
            {
                conflicts.insert(key.clone());
            }
        }
        for suffix in ["/parent", "/layout/parent"] {
            for (path, cell) in &cells {
                if !path.starts_with("/@entities/") || !path.ends_with(suffix) {
                    continue;
                }
                let Cell::Value(value) = cell else {
                    continue;
                };
                let Some(mut id) = value.as_u64() else {
                    continue;
                };
                let own = path["/@entities/".len()..]
                    .split('/')
                    .next()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap_or(0);
                let mut seen = BTreeSet::from([own]);
                loop {
                    if !seen.insert(id) || seen.len() > 64 {
                        conflicts.insert(path.clone());
                        break;
                    }
                    let parent_path = format!("/@entities/{id}{suffix}");
                    if !cells.contains_key(&format!("/@entities/{id}")) {
                        conflicts.insert(path.clone());
                        break;
                    }
                    let Some(Cell::Value(value)) = cells.get(&parent_path) else {
                        break;
                    };
                    let Some(parent) = value.as_u64() else {
                        break;
                    };
                    id = parent;
                }
            }
        }
        for change in active {
            if let Edit::Write { writes: changes } = &change.edit {
                for write in changes {
                    let candidates = writes.entry(&write.path).or_default();
                    if candidates.iter().any(|(other, value)| {
                        *value != &write.value
                            && change.seen.get(&other.id.actor).copied().unwrap_or(0) < other.id.seq
                            && other.seen.get(&change.id.actor).copied().unwrap_or(0)
                                < change.id.seq
                    }) {
                        conflicts.insert(write.path.clone());
                    }
                    candidates.push((change, &write.value));
                }
            }
        }
        conflicts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::{Shape, Style},
        geom::{Geom, Pt},
    };

    fn shape(name: &str) -> Shape {
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::splat(10.),
                radius: 0.,
            },
            Style::default(),
        );
        shape.name = name.into();
        shape
    }
    fn pair() -> (Document, Replica, Replica) {
        let doc = Document::new("Live", 100., 100., 72.);
        let base = snapshot(&doc).unwrap();
        (
            doc,
            Replica::new("session".into(), "a".repeat(64), base.clone()),
            Replica::new("session".into(), "b".repeat(64), base),
        )
    }
    fn exchange(a: &mut Replica, b: &mut Replica) {
        a.receive(b.changes.clone()).unwrap();
        b.receive(a.changes.clone()).unwrap();
    }
    fn names(doc: &Document) -> Vec<String> {
        doc.layers[1]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .map(|s| s.name.clone())
            .collect()
    }

    #[test]
    fn offline_clients_reconnect_and_undo_only_their_own_objects() {
        let (doc, mut a, mut b) = pair();
        let mut left = doc.clone();
        left.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .push(shape("Alice"));
        let mut right = doc.clone();
        right.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .push(shape("Bob"));
        a.record(&left).unwrap();
        b.record(&right).unwrap();
        exchange(&mut a, &mut b);
        assert_eq!(
            snapshot(&a.document(&doc).unwrap()).unwrap(),
            snapshot(&b.document(&doc).unwrap()).unwrap()
        );
        assert_eq!(names(&a.document(&doc).unwrap()).len(), 2);
        a.undo();
        exchange(&mut a, &mut b);
        assert_eq!(names(&b.document(&doc).unwrap()), ["Bob"]);
        a.redo();
        exchange(&mut a, &mut b);
        assert_eq!(names(&b.document(&doc).unwrap()).len(), 2);
        b.undo();
        exchange(&mut a, &mut b);
        assert_eq!(names(&a.document(&doc).unwrap()), ["Alice"]);
    }

    #[test]
    fn conflicts_converge_and_undo_reveals_the_other_authors_value() {
        let (doc, mut a, mut b) = pair();
        let mut left = doc.clone();
        left.name = "Alice".into();
        let mut right = doc.clone();
        right.name = "Bob".into();
        a.record(&left).unwrap();
        b.record(&right).unwrap();
        exchange(&mut a, &mut b);
        assert_eq!(a.document(&doc).unwrap().name, "Bob");
        assert!(a.conflicts().contains("/name"));
        b.undo();
        exchange(&mut a, &mut b);
        assert_eq!(a.document(&doc).unwrap().name, "Alice");
        a.undo();
        exchange(&mut a, &mut b);
        assert_eq!(b.document(&doc).unwrap().name, "Live");
    }

    #[test]
    fn duplicate_reordered_changes_and_deleted_anchors_keep_every_object() {
        let (mut doc, mut a, mut b) = pair();
        doc.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([shape("first"), shape("last")]);
        a.base = snapshot(&doc).unwrap();
        b.base = a.base.clone();
        let mut left = doc.clone();
        left.layers[1].kind.shapes_mut().unwrap().remove(0);
        let mut right = doc.clone();
        right.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .insert(1, shape("inserted"));
        a.record(&left).unwrap();
        b.record(&right).unwrap();
        a.receive(b.changes.clone().into_iter().rev()).unwrap();
        exchange(&mut a, &mut b);
        exchange(&mut a, &mut b);
        let result = names(&a.document(&doc).unwrap());
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"last".into()));
        assert!(result.contains(&"inserted".into()));
        assert_eq!(
            snapshot(&a.document(&doc).unwrap()).unwrap(),
            snapshot(&b.document(&doc).unwrap()).unwrap()
        );
    }

    #[test]
    fn offline_queue_and_local_undo_survive_native_save_reopen() {
        let (mut doc, mut a, _) = pair();
        doc.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .push(shape("Saved offline"));
        a.record(&doc).unwrap();
        doc.collaboration = Some(a);
        let encoded = crate::project::encode(&doc).unwrap();
        let json: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(
            json["doc"]["collaboration"]["pending"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let mut reopened = crate::project::decode(&encoded).unwrap();
        let mut state = reopened.collaboration.take().unwrap();
        assert_eq!(state.pending.len(), 1);
        assert!(state.undo());
        assert!(names(&state.document(&reopened).unwrap()).is_empty());
    }

    #[test]
    fn forged_foreign_undo_and_reused_operation_ids_are_rejected_atomically() {
        let (doc, mut a, mut b) = pair();
        let mut left = doc.clone();
        left.name = "A".into();
        a.record(&left).unwrap();
        let mut forged = a.changes[0].clone();
        forged.id.actor = b.actor.clone();
        forged.edit = Edit::Toggle {
            target: a.changes[0].id.clone(),
            enabled: false,
        };
        assert!(b.receive([forged]).is_err());
        assert!(b.changes.is_empty());
        b.receive(a.changes.clone()).unwrap();
        let mut changed = a.changes[0].clone();
        changed.clock += 1;
        assert!(b.receive([changed]).is_err());
        assert_eq!(b.changes.len(), 1);
    }
    #[test]
    fn moves_and_distinct_properties_keep_one_shared_identity() {
        let (mut doc, mut a, mut b) = pair();
        let shared = shape("Shared");
        let id = shared.id;
        doc.layers[1].kind.shapes_mut().unwrap().push(shared);
        doc.layers.push(crate::document::Layer::vector("Second"));
        doc.layers.push(crate::document::Layer::vector("Third"));
        a.base = snapshot(&doc).unwrap();
        b.base = a.base.clone();
        let mut left = doc.clone();
        let mut right = doc.clone();
        let moved = left.layers[1].kind.shapes_mut().unwrap().remove(0);
        left.layers[2].kind.shapes_mut().unwrap().push(moved);
        let mut moved = right.layers[1].kind.shapes_mut().unwrap().remove(0);
        moved.name = "Renamed".into();
        right.layers[3].kind.shapes_mut().unwrap().push(moved);
        a.record(&left).unwrap();
        b.record(&right).unwrap();
        exchange(&mut a, &mut b);
        let merged = a.document(&doc).unwrap();
        let copies: Vec<_> = merged
            .layers
            .iter()
            .flat_map(|l| l.kind.shapes().unwrap_or_default())
            .filter(|s| s.id == id)
            .collect();
        assert_eq!(copies.len(), 1);
        assert_eq!(copies[0].name, "Renamed");
        assert!(!a.record(&merged).unwrap());
        assert_eq!(
            snapshot(&merged).unwrap(),
            snapshot(&b.document(&doc).unwrap()).unwrap()
        );
    }

    #[test]
    fn enum_conflicts_and_previously_omitted_layout_defaults_remain_valid() {
        let (mut doc, mut a, mut b) = pair();
        doc.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .push(shape("Shared"));
        a.base = snapshot(&doc).unwrap();
        b.base = a.base.clone();
        let mut left = doc.clone();
        let mut right = doc.clone();
        let left_shape = &mut left.layers[1].kind.shapes_mut().unwrap()[0];
        left_shape.layout.constraint_x = crate::layout::Constraint::End;
        left_shape.style.fill = crate::document::Fill::Linear {
            from: [0., 0.],
            to: [1., 1.],
            c0: crate::color::Rgba::WHITE,
            c1: crate::color::Rgba::BLACK,
        };
        let right_shape = &mut right.layers[1].kind.shapes_mut().unwrap()[0];
        right_shape.layout.constraint_y = crate::layout::Constraint::End;
        right_shape.style.fill = crate::document::Fill::Radial {
            c0: crate::color::Rgba::WHITE,
            c1: crate::color::Rgba::BLACK,
        };
        a.record(&left).unwrap();
        b.record(&right).unwrap();
        exchange(&mut a, &mut b);
        let merged = a.document(&doc).unwrap();
        let result = &merged.layers[1].kind.shapes().unwrap()[0];
        assert_eq!(result.layout.constraint_x, crate::layout::Constraint::End);
        assert_eq!(result.layout.constraint_y, crate::layout::Constraint::End);
        assert!(matches!(
            result.style.fill,
            crate::document::Fill::Radial { .. }
        ));
        assert!(!a.record(&merged).unwrap());
        crate::project::decode(&crate::project::encode(&merged).unwrap()).unwrap();
    }

    #[test]
    fn deleted_container_recovers_remote_additions_and_reports_the_conflict() {
        let (doc, mut a, mut b) = pair();
        let mut left = doc.clone();
        left.layers.remove(1);
        let mut right = doc.clone();
        right.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .push(shape("Remote child"));
        a.record(&left).unwrap();
        b.record(&right).unwrap();
        exchange(&mut a, &mut b);
        let merged = a.document(&doc).unwrap();
        assert!(
            merged
                .layers
                .iter()
                .flat_map(|l| l.kind.shapes().unwrap_or_default())
                .any(|s| s.name == "Remote child")
        );
        assert!(!a.conflicts().is_empty());
        assert_eq!(
            snapshot(&merged).unwrap(),
            snapshot(&b.document(&doc).unwrap()).unwrap()
        );
        assert!(!a.record(&merged).unwrap());
    }

    #[test]
    fn concurrent_group_cycles_are_detached_without_losing_layers() {
        let (mut doc, mut a, mut b) = pair();
        let first = crate::document::Layer::group("A");
        let second = crate::document::Layer::group("B");
        let (first_id, second_id) = (first.id, second.id);
        doc.layers.extend([first, second]);
        a.base = snapshot(&doc).unwrap();
        b.base = a.base.clone();
        let mut left = doc.clone();
        left.layers[2].parent = Some(second_id);
        let mut right = doc.clone();
        right.layers[3].parent = Some(first_id);
        a.record(&left).unwrap();
        b.record(&right).unwrap();
        exchange(&mut a, &mut b);
        let merged = a.document(&doc).unwrap();
        merged.validate_hierarchy().unwrap();
        assert_eq!(merged.layers.len(), 4);
        assert_eq!(
            snapshot(&merged).unwrap(),
            snapshot(&b.document(&doc).unwrap()).unwrap()
        );
    }

    #[test]
    fn sibling_merges_are_stable_and_forked_files_have_distinct_authors() {
        let (mut doc, mut a, mut b) = pair();
        let mut left = doc.clone();
        left.layers[1].kind.shapes_mut().unwrap().push(shape("A"));
        let mut right = doc.clone();
        right.layers[1].kind.shapes_mut().unwrap().push(shape("B"));
        a.record(&left).unwrap();
        b.record(&right).unwrap();
        exchange(&mut a, &mut b);
        doc = a.document(&doc).unwrap();
        assert!(!a.record(&doc).unwrap());
        a.fork("c".repeat(64), "another device".into(), &doc)
            .unwrap();
        assert!(
            a.changes
                .iter()
                .filter(|c| a.pending.contains(&c.id))
                .all(|c| c.id.actor == a.actor)
        );
        assert_eq!(names(&a.document(&doc).unwrap()).len(), 2);
    }
    #[test]
    fn moving_an_object_out_of_a_concurrently_deleted_layer_preserves_its_payload() {
        let (mut doc, mut a, mut b) = pair();
        doc.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .push(shape("Moved out"));
        doc.layers
            .push(crate::document::Layer::vector("Destination"));
        a.base = snapshot(&doc).unwrap();
        b.base = a.base.clone();
        let mut left = doc.clone();
        left.layers.remove(1);
        let mut right = doc.clone();
        let moved = right.layers[1].kind.shapes_mut().unwrap().remove(0);
        right.layers[2].kind.shapes_mut().unwrap().push(moved);
        a.record(&left).unwrap();
        b.record(&right).unwrap();
        exchange(&mut a, &mut b);
        let merged = a.document(&doc).unwrap();
        assert!(
            merged
                .layers
                .iter()
                .flat_map(|l| l.kind.shapes().unwrap_or_default())
                .any(|s| s.name == "Moved out")
        );
        assert_eq!(
            snapshot(&merged).unwrap(),
            snapshot(&b.document(&doc).unwrap()).unwrap()
        );
    }
    #[test]
    fn authored_order_after_concurrent_insertions_matches_the_editor() {
        let (mut doc, mut a, mut b) = pair();
        let mut first = shape("3");
        first.id = 1003;
        doc.layers[1].kind.shapes_mut().unwrap().push(first);
        a.base = snapshot(&doc).unwrap();
        b.base = a.base.clone();
        let mut left = doc.clone();
        let mut right = doc.clone();
        let mut one = shape("1");
        one.id = 1001;
        let mut two = shape("2");
        two.id = 1002;
        left.layers[1].kind.shapes_mut().unwrap().push(one);
        right.layers[1].kind.shapes_mut().unwrap().push(two);
        a.record(&left).unwrap();
        b.record(&right).unwrap();
        exchange(&mut a, &mut b);
        let mut merged = a.document(&doc).unwrap();
        assert_eq!(names(&merged), ["3", "1", "2"]);
        merged.layers[1].kind.shapes_mut().unwrap().rotate_left(1);
        a.record(&merged).unwrap();
        exchange(&mut a, &mut b);
        assert_eq!(names(&a.document(&doc).unwrap()), ["1", "2", "3"]);
        assert_eq!(
            snapshot(&merged).unwrap(),
            snapshot(&b.document(&doc).unwrap()).unwrap()
        );
    }
    #[test]
    fn recovered_containers_remain_editable_and_durable() {
        let (doc, mut a, mut b) = pair();
        let mut left = doc.clone();
        left.layers.remove(1);
        let mut right = doc.clone();
        right.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .push(shape("Recovered"));
        a.record(&left).unwrap();
        b.record(&right).unwrap();
        exchange(&mut a, &mut b);
        let mut merged = a.document(&doc).unwrap();
        let layer = merged
            .layers
            .iter_mut()
            .find(|l| l.kind.shapes().is_some())
            .unwrap();
        layer.name = "Renamed recovery".into();
        layer.kind.shapes_mut().unwrap().push(shape("New sibling"));
        a.record(&merged).unwrap();
        exchange(&mut a, &mut b);
        let output = b.document(&doc).unwrap();
        assert!(
            output
                .layers
                .iter()
                .any(|l| l.name == "Renamed recovery" && l.kind.shapes().unwrap().len() == 2)
        );
        assert_eq!(
            snapshot(&output).unwrap(),
            snapshot(&a.document(&doc).unwrap()).unwrap()
        );
    }
}
