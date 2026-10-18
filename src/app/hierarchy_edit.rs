//! Editable layer groups and object/frame sidebar moves.
use super::*;

/// Visual sidebar placement; document vectors use bottom-to-top painter order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreePlacement {
    Above,
    Below,
    Inside,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn studio() -> Studio {
        let mut studio = Studio::new();
        studio.doc = Document::new("Tree", 400., 400., 96.);
        studio.doc.layers = vec![Layer::vector("Artwork"), Layer::vector("Frames")];
        studio.active_layer = Some(0);
        studio.selection.clear();
        studio.selected_layer = None;
        studio.history = History::default();
        studio
    }
    fn rect(x: f32) -> Shape {
        Shape::new(
            Geom::Rect {
                origin: Pt::new(x, 20.),
                size: Pt::new(30., 40.),
                radius: 0.,
            },
            Style::default(),
        )
    }
    fn state(s: &Studio) -> String {
        serde_json::to_string(&s.doc.layers).unwrap()
    }
    fn points(doc: &Document, li: usize, id: u64) -> Vec<Pt> {
        let shape = doc.find_shape(li, id).unwrap();
        let mut points: Vec<_> = shape.world_contours(12).into_iter().flatten().collect();
        for frame in shape_ancestors(doc, li, id) {
            for p in &mut points {
                *p = p.rotate_about(frame.geom.bbox().center(), frame.rotation);
            }
        }
        points
    }
    fn same_points(a: &[Pt], b: &[Pt]) {
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert!(
                (a.x - b.x).abs() < 0.002 && (a.y - b.y).abs() < 0.002,
                "{a:?} != {b:?}"
            );
        }
    }

    #[test]
    fn real_layer_group_preserves_objects_roundtrip_and_single_undo() {
        let mut s = studio();
        let objects = vec![rect(0.), rect(50.), rect(100.)];
        let ids: Vec<_> = objects.iter().map(|s| s.id).collect();
        *s.doc.layers[0].kind.shapes_mut().unwrap() = objects.clone();
        s.selection = vec![(0, ids[0]), (0, ids[2])];
        let before = state(&s);
        s.group_selected();
        assert_eq!(s.history.len(), 1);
        let group_id = s.selected_layer.unwrap();
        let group = s.doc.layers.iter().find(|l| l.id == group_id).unwrap();
        assert!(group.is_group && group.pass_through);
        let child = s
            .doc
            .layers
            .iter()
            .find(|l| l.parent == Some(group_id))
            .unwrap();
        assert_eq!(
            child.kind.shapes().unwrap(),
            &[objects[0].clone(), objects[2].clone()]
        );
        assert!(s.doc.validate_hierarchy().is_ok());
        let saved: Document =
            serde_json::from_str(&serde_json::to_string(&s.doc).unwrap()).unwrap();
        assert!(saved.validate_hierarchy().is_ok());
        let grouped = state(&s);
        s.undo();
        assert_eq!(state(&s), before);
        assert!(
            s.selection
                .iter()
                .all(|&(li, id)| li == 0 && ids.contains(&id))
        );
        s.redo();
        assert_eq!(state(&s), grouped);
        s.selected_layer = Some(group_id);
        s.ungroup_selected();
        assert!(!s.doc.layers.iter().any(|l| l.id == group_id));
        assert!(s.doc.layers.iter().all(|l| l.parent.is_none()));
        s.undo();
        assert_eq!(state(&s), grouped);
    }

    #[test]
    fn layer_drop_nests_unnests_and_rejects_cycles_and_locked_descendants() {
        let mut s = studio();
        let group = Layer::group("Group");
        let group_id = group.id;
        s.doc.layers.push(group);
        let before = state(&s);
        assert!(s.drop_layer(0, 2, TreePlacement::Inside));
        let child = s
            .doc
            .layers
            .iter()
            .position(|l| l.parent == Some(group_id))
            .unwrap();
        let group = s.doc.layers.iter().position(|l| l.id == group_id).unwrap();
        assert!(!s.drop_layer(group, child, TreePlacement::Inside));
        assert!(s.doc.validate_hierarchy().is_ok());
        s.undo();
        assert_eq!(state(&s), before);
        s.redo();
        let child = s
            .doc
            .layers
            .iter()
            .position(|l| l.parent == Some(group_id))
            .unwrap();
        let group = s.doc.layers.iter().position(|l| l.id == group_id).unwrap();
        assert!(s.drop_layer(child, group, TreePlacement::Above));
        assert!(s.doc.layers.iter().all(|l| l.parent.is_none()));
        s.undo();
        let child = s
            .doc
            .layers
            .iter()
            .position(|l| l.parent == Some(group_id))
            .unwrap();
        s.doc.layers[child].locked = true;
        let group = s.doc.layers.iter().position(|l| l.id == group_id).unwrap();
        let sibling = s
            .doc
            .layers
            .iter()
            .position(|l| l.name == "Frames")
            .unwrap();
        assert!(!s.drop_layer(group, sibling, TreePlacement::Above));
    }

    #[test]
    fn rotated_frame_subtree_cross_layer_drop_and_detach_preserve_world_coordinates() {
        let mut s = studio();
        let mut outer = crate::layout::make_frame(Pt::ZERO, Pt::splat(250.));
        outer.rotation = 0.4;
        let mut nested = crate::layout::make_frame(Pt::splat(40.), Pt::splat(100.));
        nested.layout.parent = Some(outer.id);
        nested.rotation = 0.2;
        let mut child = rect(65.);
        child.layout.parent = Some(nested.id);
        let mut destination = crate::layout::make_frame(Pt::new(250., 80.), Pt::splat(120.));
        destination.rotation = -0.5;
        let (root, leaf, target) = (nested.id, child.id, destination.id);
        *s.doc.layers[0].kind.shapes_mut().unwrap() = vec![outer, nested, child];
        s.doc.layers[1].kind.shapes_mut().unwrap().push(destination);
        let before = state(&s);
        let root_points = points(&s.doc, 0, root);
        let child_points = points(&s.doc, 0, leaf);
        assert!(s.drop_objects(
            &[(0, root), (0, leaf)],
            1,
            Some(target),
            TreePlacement::Inside
        ));
        assert_eq!(s.history.len(), 1);
        assert_eq!(s.doc.find_shape(1, leaf).unwrap().layout.parent, Some(root));
        same_points(&root_points, &points(&s.doc, 1, root));
        same_points(&child_points, &points(&s.doc, 1, leaf));
        s.undo();
        assert_eq!(state(&s), before);
        s.redo();
        assert!(s.drop_objects(&[(1, root)], 0, None, TreePlacement::Inside));
        same_points(&root_points, &points(&s.doc, 0, root));
        same_points(&child_points, &points(&s.doc, 0, leaf));
        assert_eq!(s.doc.find_shape(0, root).unwrap().layout.parent, None);
        assert!(s.doc.validate_hierarchy().is_ok());
    }

    #[test]
    fn multi_object_drop_keeps_order_and_selection_through_undo() {
        let mut s = studio();
        let objects = vec![rect(0.), rect(50.), rect(100.), rect(150.)];
        let ids: Vec<_> = objects.iter().map(|s| s.id).collect();
        *s.doc.layers[0].kind.shapes_mut().unwrap() = objects;
        s.selection = vec![(0, ids[2]), (0, ids[0])];
        let before = state(&s);
        assert!(s.drop_objects(&s.selection.clone(), 0, Some(ids[3]), TreePlacement::Above));
        assert_eq!(
            s.doc.layers[0]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            vec![ids[1], ids[3], ids[0], ids[2]]
        );
        s.undo();
        assert_eq!(state(&s), before);
        assert!(s.drop_objects(&s.selection.clone(), 1, None, TreePlacement::Inside));
        assert!(s.selection.iter().all(|s| s.0 == 1));
        s.undo();
        assert_eq!(state(&s), before);
        assert!(s.selection.iter().all(|s| s.0 == 0));
        s.redo();
        assert!(s.selection.iter().all(|s| s.0 == 1));
    }

    fn panel(s: &Studio) -> Vec<u64> {
        s.layer_panel_order(true)
            .into_iter()
            .map(|index| s.doc.layers[index].id)
            .collect()
    }

    #[test]
    fn craft_whole_rows_group_empty_groups_in_painter_order_and_restore_parents() {
        let mut s = studio();
        let low = s.doc.layers[0].id;
        let mid = s.doc.layers[1].id;
        let empty = Layer::group("Empty");
        let empty_id = empty.id;
        s.doc.layers.push(empty);
        let top = Layer::vector("Top");
        let top_id = top.id;
        s.doc.layers.push(top);
        s.select_layer_rows(0, false, false);
        s.select_layer_rows(2, false, true);
        s.select_layer_rows(3, false, true);
        let before = state(&s);
        s.group_named("Set");
        assert_eq!(s.history.len(), 1);
        let group_id = s.selected_layer.unwrap();
        let group = s.doc.layers.iter().find(|layer| layer.id == group_id).unwrap();
        assert_eq!(group.name, "Set");
        assert_eq!(group.parent, None);
        let children: Vec<u64> = s
            .layer_panel_order(true)
            .into_iter()
            .rev()
            .filter(|&index| s.doc.layers[index].parent == Some(group_id))
            .map(|index| s.doc.layers[index].id)
            .collect();
        assert_eq!(children, vec![low, empty_id, top_id]);
        let rows = panel(&s);
        let header = rows.iter().position(|id| *id == group_id).unwrap();
        assert_eq!(&rows[header + 1..header + 4], &[top_id, empty_id, low]);
        assert!(rows.iter().position(|id| *id == mid).unwrap() > header);
        assert!(s.doc.validate_hierarchy().is_ok());
        s.undo();
        assert_eq!(state(&s), before);
        assert_eq!(
            s.doc.layers.iter().find(|layer| layer.id == empty_id).unwrap().parent,
            None
        );
        s.redo();
        assert_eq!(s.selected_layer, Some(group_id));
    }

    #[test]
    fn craft_grouping_uses_the_highest_parent_and_prunes_selected_descendants() {
        let mut s = studio();
        let mut inner = Layer::vector("Inner");
        let outer = Layer::group("Outer");
        inner.parent = Some(outer.id);
        let (inner_id, outer_id) = (inner.id, outer.id);
        s.doc.layers.push(inner);
        s.doc.layers.push(outer);
        let mut high = Layer::vector("High child");
        let high_group = Layer::group("High");
        high.parent = Some(high_group.id);
        let (high_id, high_group_id) = (high.id, high_group.id);
        s.doc.layers.push(high);
        s.doc.layers.push(high_group);
        let inner_index = s.doc.layers.iter().position(|layer| layer.id == inner_id).unwrap();
        let high_index = s.doc.layers.iter().position(|layer| layer.id == high_id).unwrap();
        s.select_layer_rows(inner_index, false, false);
        s.select_layer_rows(high_index, false, true);
        s.group_selected();
        let group_id = s.selected_layer.unwrap();
        assert_eq!(
            s.doc.layers.iter().find(|layer| layer.id == group_id).unwrap().parent,
            Some(high_group_id)
        );
        assert_eq!(
            s.doc.layers.iter().find(|layer| layer.id == high_id).unwrap().parent,
            Some(group_id)
        );
        assert_eq!(
            s.doc.layers.iter().find(|layer| layer.id == inner_id).unwrap().parent,
            Some(group_id)
        );
        assert_eq!(
            s.doc.layers.iter().find(|layer| layer.id == outer_id).unwrap().parent,
            None
        );
        s.undo();
        let outer_index = s.doc.layers.iter().position(|layer| layer.id == outer_id).unwrap();
        s.select_layer_rows(outer_index, false, false);
        s.select_layer_rows(inner_index, false, true);
        s.group_selected();
        let wrapped = s.selected_layer.unwrap();
        assert_eq!(
            s.doc.layers.iter().find(|layer| layer.id == inner_id).unwrap().parent,
            Some(outer_id)
        );
        assert_eq!(
            s.doc.layers.iter().find(|layer| layer.id == outer_id).unwrap().parent,
            Some(wrapped)
        );
        assert_ne!(wrapped, outer_id);
    }

    #[test]
    fn craft_partial_objects_still_extract_beside_an_unselected_empty_group() {
        let mut s = studio();
        let empty = Layer::group("Empty");
        let empty_id = empty.id;
        s.doc.layers.push(empty);
        let objects = vec![rect(0.), rect(40.)];
        let kept = objects[1].clone();
        let moving = objects[0].id;
        *s.doc.layers[0].kind.shapes_mut().unwrap() = objects;
        s.selected_layer = None;
        s.selection = vec![(0, moving)];
        s.group_named("Extracted");
        let group = s.doc.layers.iter().find(|layer| layer.name == "Extracted").unwrap();
        let child = s.doc.layers.iter().find(|layer| layer.parent == Some(group.id)).unwrap();
        assert_eq!(child.kind.shapes().unwrap().len(), 1);
        assert_eq!(child.kind.shapes().unwrap()[0].id, moving);
        assert_eq!(s.doc.layers[0].kind.shapes().unwrap(), &[kept]);
        assert_eq!(
            s.doc.layers.iter().find(|layer| layer.id == empty_id).unwrap().parent,
            None
        );
    }

    #[test]
    fn craft_locked_rows_refuse_grouping_without_history() {
        let mut s = studio();
        s.doc.layers[1].locked = true;
        s.select_layer_rows(0, false, false);
        s.select_layer_rows(1, false, true);
        let before = state(&s);
        s.group_selected();
        assert_eq!(s.history.len(), 0);
        assert_eq!(state(&s), before);
        assert_eq!(s.status, "Unlock the layers before grouping");
    }

    #[test]
    fn craft_ungroup_keeps_the_sibling_slot_and_only_drops_group_animation() {
        let mut s = studio();
        let below = s.doc.layers[0].id;
        let above = s.doc.layers[1].id;
        let mut child_low = Layer::vector("Low");
        let mut shape = rect(5.);
        let shape_id = shape.id;
        child_low.kind.shapes_mut().unwrap().push(shape);
        let mut child_high = Layer::vector("High");
        let mut group = Layer::group("Wrap");
        child_low.parent = Some(group.id);
        child_high.parent = Some(group.id);
        let (low_id, high_id, group_id) = (child_low.id, child_high.id, group.id);
        let mut marker = rect(9.);
        let marker_id = marker.id;
        group.kind.shapes_mut().unwrap().push(marker);
        s.doc.layers = vec![
            s.doc.layers[0].clone(),
            child_low,
            child_high,
            group,
            s.doc.layers[1].clone(),
        ];
        s.doc.motion.tracks = vec![
            crate::motion::Track {
                shape: group_id,
                prop: crate::motion::Prop::Opacity,
                keys: vec![
                    crate::motion::Key::at(0., 1., crate::motion::Ease::Linear),
                    crate::motion::Key::at(1., 0.4, crate::motion::Ease::Linear),
                ],
            },
            crate::motion::Track {
                shape: marker_id,
                prop: crate::motion::Prop::Opacity,
                keys: vec![
                    crate::motion::Key::at(0., 1., crate::motion::Ease::Linear),
                    crate::motion::Key::at(1., 0.2, crate::motion::Ease::Linear),
                ],
            },
            crate::motion::Track {
                shape: low_id,
                prop: crate::motion::Prop::Opacity,
                keys: vec![
                    crate::motion::Key::at(0., 1., crate::motion::Ease::Linear),
                    crate::motion::Key::at(1., 0.8, crate::motion::Ease::Linear),
                ],
            },
        ];
        let child_before = s
            .doc
            .layers
            .iter()
            .find(|layer| layer.id == low_id)
            .unwrap()
            .clone();
        let group_index = s.doc.layers.iter().position(|layer| layer.id == group_id).unwrap();
        s.activate_layer_tree(group_index);
        let before = state(&s);
        s.ungroup_selected();
        assert!(!s.doc.layers.iter().any(|layer| layer.id == group_id));
        assert!(s.doc.layers.iter().all(|layer| layer.parent.is_none()));
        assert_eq!(panel(&s), vec![above, high_id, low_id, below]);
        let kept = s.doc.layers.iter().find(|layer| layer.id == low_id).unwrap();
        assert_eq!(kept.kind.shapes().unwrap()[0].id, shape_id);
        assert_eq!(
            serde_json::to_value(&kept.kind).unwrap(),
            serde_json::to_value(&child_before.kind).unwrap()
        );
        let tracks: Vec<u64> = s.doc.motion.tracks.iter().map(|track| track.shape).collect();
        assert_eq!(tracks, vec![low_id]);
        assert!(s.doc.validate_hierarchy().is_ok());
        s.undo();
        assert_eq!(state(&s), before);
        let mut animated = vec![group_id, low_id, marker_id];
        animated.sort_unstable();
        assert_eq!(s.doc.motion.shapes(), animated);
    }

    #[test]
    fn craft_group_dialog_request_refuses_a_changed_document_or_selection() {
        let mut s = studio();
        s.show_welcome = false;
        s.select_layer_rows(0, false, false);
        s.select_layer_rows(1, false, true);
        let request = s.capture_group_request().unwrap();
        s.mark();
        assert!(s.group_request_current(&request).is_err());
        assert_eq!(s.history.len(), 0);
        s.select_layer_rows(0, false, false);
        let request = s.capture_group_request().unwrap();
        s.select_layer_rows(1, false, true);
        assert!(s.group_request_current(&request).is_err());
        let request = s.capture_group_request().unwrap();
        assert!(s.group_request_current(&request).is_ok());
        s.group_named("  Kept  ");
        assert_eq!(
            s.doc.layers.iter().find(|layer| layer.is_group).unwrap().name,
            "Kept"
        );
    }
}

fn reorder_commands(order: &mut Vec<u64>, desired: &[u64], commands: &mut Vec<Cmd>) {
    for (to, id) in desired.iter().enumerate() {
        if order[to] != *id {
            let from = order.iter().position(|v| v == id).unwrap();
            order.remove(from);
            order.insert(to, *id);
            commands.push(Cmd::ReorderLayer { from, to });
        }
    }
}

fn shape_ancestors(doc: &Document, layer: usize, id: u64) -> Vec<&Shape> {
    let mut result = Vec::new();
    let mut parent = doc.find_shape(layer, id).and_then(|s| s.layout.parent);
    for _ in 0..64 {
        let Some(shape) = parent.and_then(|id| doc.find_shape(layer, id)) else {
            break;
        };
        if result.iter().any(|s: &&Shape| s.id == shape.id) {
            break;
        }
        result.push(shape);
        parent = shape.layout.parent;
    }
    result
}

impl Studio {
    /// Move whole frame subtrees, retaining world coordinates and stable object ids.
    pub fn drop_objects(
        &mut self,
        source: &[(usize, u64)],
        target_layer: usize,
        target: Option<u64>,
        place: TreePlacement,
    ) -> bool {
        if source.is_empty()
            || self.op.is_some()
            || !self.layer_unlocked(target_layer)
            || !self.doc.layer_visible(target_layer)
        {
            return false;
        }
        let target_shape = target.and_then(|id| self.doc.find_shape(target_layer, id));
        if target.is_some() && target_shape.is_none() {
            return false;
        }
        let parent = if place == TreePlacement::Inside {
            if let Some(shape) = target_shape {
                if !shape.layout.frame || shape.locked || !shape.visible {
                    return false;
                }
                Some(shape.id)
            } else {
                None
            }
        } else {
            target_shape.and_then(|s| s.layout.parent)
        };
        if let Some(shape) = target_shape {
            if shape.locked
                || shape_ancestors(&self.doc, target_layer, shape.id)
                    .iter()
                    .any(|s| s.locked)
            {
                return false;
            }
        }
        let mut after = self.doc.layout_snapshot();
        let mut commands = Vec::new();
        let destination = if self.doc.layers[target_layer].is_group {
            let mut child = Layer::vector("Objects");
            child.parent = Some(self.doc.layers[target_layer].id);
            // Insert at the group marker after vector updates, so their indices stay stable.
            after.layers.push(child);
            after.layers.len() - 1
        } else {
            if self.doc.layers[target_layer].kind.shapes().is_none() {
                return false;
            }
            target_layer
        };
        let selected: HashSet<_> = source.iter().copied().collect();
        let mut roots: Vec<_> = source
            .iter()
            .copied()
            .filter(|&(li, id)| {
                !shape_ancestors(&self.doc, li, id)
                    .iter()
                    .any(|s| selected.contains(&(li, s.id)))
            })
            .collect();
        roots.sort_by_key(|&(li, id)| {
            (
                li,
                self.doc
                    .layers
                    .get(li)
                    .and_then(|l| l.kind.shapes())
                    .and_then(|s| s.iter().position(|s| s.id == id))
                    .unwrap_or(0),
            )
        });
        roots.dedup();
        let mut moving = Vec::new();
        let mut root_ids = Vec::new();
        for (li, id) in roots {
            if !self.layer_unlocked(li) {
                return false;
            }
            let Some(shape) = self.doc.find_shape(li, id) else {
                return false;
            };
            let ids: HashSet<_> = std::iter::once(id)
                .chain(crate::layout::descendants(&self.doc, li, id))
                .collect();
            if target.is_some_and(|id| li == target_layer && ids.contains(&id)) {
                return false;
            }
            if shape_ancestors(&self.doc, li, id)
                .iter()
                .any(|s| s.locked || !s.visible)
            {
                return false;
            }
            let mut subtree: Vec<_> = self.doc.layers[li]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .filter(|s| ids.contains(&s.id))
                .cloned()
                .collect();
            if subtree.iter().any(|s| s.locked || !s.visible) {
                return false;
            }
            let centre = shape.geom.bbox().center();
            let mut world = centre;
            let mut rotation = 0.0;
            for frame in shape_ancestors(&self.doc, li, id) {
                world = world.rotate_about(frame.geom.bbox().center(), frame.rotation);
                rotation += frame.rotation;
            }
            let mut target_chain = parent
                .map(|id| {
                    let mut v = vec![self.doc.find_shape(target_layer, id).unwrap()];
                    v.extend(shape_ancestors(&self.doc, target_layer, id));
                    v
                })
                .unwrap_or_default();
            target_chain.reverse();
            for frame in target_chain {
                if frame.locked {
                    return false;
                }
                world = world.rotate_about(frame.geom.bbox().center(), -frame.rotation);
                rotation -= frame.rotation;
            }
            for item in &mut subtree {
                item.geom.translate(world - centre);
            }
            let root = subtree.iter_mut().find(|s| s.id == id).unwrap();
            root.rotation += rotation;
            root.layout.parent = parent;
            after.layers[li]
                .kind
                .shapes_mut()
                .unwrap()
                .retain(|s| !ids.contains(&s.id));
            root_ids.push(id);
            moving.extend(subtree);
        }
        let shapes = after.layers[destination].kind.shapes_mut().unwrap();
        let insertion = if let Some(target) = target.filter(|_| place != TreePlacement::Inside) {
            let Some(index) = shapes.iter().position(|s| s.id == target) else {
                return false;
            };
            index + usize::from(place == TreePlacement::Above)
        } else {
            shapes.len()
        };
        shapes.splice(insertion..insertion, moving);
        if crate::layout::validate_hierarchy(&after).is_err() {
            return false;
        }
        for (li, layer) in self.doc.layers.iter().enumerate() {
            if let (Some(before), Some(next)) =
                (layer.kind.shapes(), after.layers[li].kind.shapes())
            {
                if before != next {
                    commands.push(Cmd::SetVectorShapes {
                        layer: li,
                        before: before.to_vec(),
                        after: next.to_vec(),
                    });
                }
            }
        }
        let destination = if destination == self.doc.layers.len() {
            commands.push(Cmd::AddLayer {
                index: target_layer,
                layer: after.layers.pop().unwrap(),
            });
            target_layer
        } else {
            destination
        };
        if commands.is_empty() {
            return false;
        }
        self.commit_type_edit();
        self.commit(Cmd::Batch(commands));
        self.selected_layer = None;
        self.selection = root_ids.into_iter().map(|id| (destination, id)).collect();
        self.active_layer = Some(destination);
        self.layer_expanded.insert(self.doc.layers[destination].id);
        for ancestor in self.doc.layer_ancestors(destination) {
            self.layer_expanded.insert(self.doc.layers[ancestor].id);
        }
        self.status = "Objects moved".into();
        true
    }

    pub fn can_drop_layer(&self, source: usize, target: usize, place: TreePlacement) -> bool {
        source != target
            && self.op.is_none()
            && self.layer_unlocked(source)
            && self.layer_unlocked(target)
            && !self.layer_tree_indices(source).contains(&target)
            && self
                .layer_tree_indices(source)
                .iter()
                .all(|&i| !self.doc.layers[i].locked)
            && (place != TreePlacement::Inside || self.doc.layers[target].is_group)
            && self.doc.validate_hierarchy().is_ok()
    }

    pub fn drop_layer(&mut self, source: usize, target: usize, place: TreePlacement) -> bool {
        if !self.can_drop_layer(source, target, place) {
            return false;
        }
        let parent = if place == TreePlacement::Inside {
            Some(self.doc.layers[target].id)
        } else {
            self.doc.layers[target].parent
        };
        let mut order: Vec<_> = self.doc.layers.iter().map(|l| l.id).collect();
        let desired = self.layer_drop_order(source, target, place);
        let mut commands = Vec::new();
        if self.doc.layers[source].parent != parent {
            commands.push(Cmd::SetLayerParent {
                index: source,
                before: self.doc.layers[source].parent,
                after: parent,
            });
        }
        reorder_commands(&mut order, &desired, &mut commands);
        if commands.is_empty() {
            return false;
        }
        let command = Cmd::Batch(commands);
        if let Err(error) = layer_copy::validate(&self.doc, &command) { self.status = error; return false; }
        self.commit_type_edit();
        let before = self.history_target();
        self.commit(command);
        self.history.remember_target(before, self.history_target());
        if let Some(parent) = parent {
            self.layer_expanded.insert(parent);
        }
        self.status = "Layer moved".into();
        true
    }

    pub fn group_selected(&mut self) {
        self.group_named("Group");
    }

    /// Group the current explicit rows or object selection under one name.
    /// Args: group name. Returns nothing; a refused name, lock or hierarchy check adds no history.
    pub fn group_named(&mut self, name: impl Into<String>) {
        let name = name.into().trim().to_string();
        let step = if name == "Group" {
            crate::actions::Step::Group
        } else {
            crate::actions::Step::GroupNamed(name.clone())
        };
        self.with_action_step(step, |studio| studio.group_rows(&name));
    }

    fn group_rows(&mut self, name: &str) {
        crate::telemetry::count("feature.group");
        if self.op.is_some() {
            return;
        }
        if name.is_empty() || name.chars().count() > 1024 || name.chars().any(|c| c.is_control()) {
            self.status = "Name the group with 1 to 1024 characters".into();
            return;
        }
        self.commit_type_edit();
        if let Some(rows) = self.explicit_group_rows() {
            self.group_explicit_rows(name, &rows);
            return;
        }
        self.group_object_pieces(name);
    }

    fn group_object_pieces(&mut self, name: &str) {
        let mut whole = Vec::new();
        let mut pieces: Vec<(usize, Vec<Shape>, Vec<Shape>)> = Vec::new();
        if let Some(index) = self
            .selected_layer
            .and_then(|id| self.doc.layers.iter().position(|l| l.id == id))
        {
            whole.push(index);
        } else {
            let mut layers: Vec<_> = self.selection.iter().map(|s| s.0).collect();
            layers.sort_unstable();
            layers.dedup();
            for li in layers {
                if self.selection.contains(&(li, RASTER_ID)) {
                    whole.push(li);
                    continue;
                }
                let Some(shapes) = self.doc.layers[li].kind.shapes() else {
                    continue;
                };
                let mut ids: HashSet<_> = self
                    .selection
                    .iter()
                    .filter(|s| s.0 == li)
                    .map(|s| s.1)
                    .collect();
                for id in ids.clone() {
                    ids.extend(crate::layout::descendants(&self.doc, li, id));
                }
                let (mut moving, remaining): (Vec<_>, Vec<_>) =
                    shapes.iter().cloned().partition(|s| ids.contains(&s.id));
                if moving.is_empty() {
                    continue;
                }
                if moving.iter().any(|s| s.locked || !s.visible) {
                    self.status =
                        "Unlock selected objects and their children before grouping".into();
                    return;
                }
                if remaining.is_empty() {
                    whole.push(li);
                } else {
                    // Extracted roots no longer inherit their enclosing frame transforms.
                    let roots: Vec<_> = moving
                        .iter()
                        .filter(|s| s.layout.parent.is_some_and(|id| !ids.contains(&id)))
                        .map(|s| s.id)
                        .collect();
                    for root_id in roots {
                        let shape = self.doc.find_shape(li, root_id).unwrap();
                        let centre = shape.geom.bbox().center();
                        let mut point = centre;
                        let mut rotation = 0.0;
                        for frame in shape_ancestors(&self.doc, li, root_id) {
                            if frame.locked {
                                self.status = "Unlock the containing frame before grouping".into();
                                return;
                            }
                            point = point.rotate_about(frame.geom.bbox().center(), frame.rotation);
                            rotation += frame.rotation;
                        }
                        let descendants = crate::layout::descendants(&self.doc, li, root_id);
                        for item in &mut moving {
                            if item.id == root_id || descendants.contains(&item.id) {
                                item.geom.translate(point - centre);
                            }
                            if item.id == root_id {
                                item.rotation += rotation;
                                item.layout.parent = None;
                            }
                        }
                    }
                    pieces.push((li, moving, remaining));
                }
            }
        }
        let mut sources = whole.clone();
        sources.extend(pieces.iter().map(|p| p.0));
        if sources.is_empty() {
            self.status = "Select objects or a layer to group".into();
            return;
        }
        if sources.iter().any(|&i| !self.layer_unlocked(i))
            || whole.iter().any(|&i| {
                self.layer_tree_indices(i)
                    .iter()
                    .any(|&j| self.doc.layers[j].locked)
            })
        {
            self.status = "Unlock the layers before grouping".into();
            return;
        }
        // Find the nearest common layer group and insert after the affected branches.
        let mut candidates: Vec<_> = self
            .doc
            .layer_ancestors(sources[0])
            .into_iter()
            .map(|i| Some(self.doc.layers[i].id))
            .collect();
        candidates.push(None);
        let parent = candidates
            .into_iter()
            .find(|candidate| {
                candidate.is_none_or(|id| {
                    sources.iter().all(|&i| {
                        self.doc
                            .layer_ancestors(i)
                            .iter()
                            .any(|&a| self.doc.layers[a].id == id)
                    })
                })
            })
            .flatten();
        let insertion_top = sources
            .iter()
            .map(|&i| {
                let mut branch = i;
                for ancestor in self.doc.layer_ancestors(i) {
                    if Some(self.doc.layers[ancestor].id) == parent {
                        break;
                    }
                    branch = ancestor;
                }
                branch
            })
            .max()
            .unwrap();
        let mut group = Layer::group(name);
        group.parent = parent;
        let group_id = group.id;
        let mut commands = vec![Cmd::AddLayer {
            index: self.doc.layers.len(),
            layer: group,
        }];
        let mut order: Vec<_> = self.doc.layers.iter().map(|l| l.id).collect();
        order.push(group_id);
        let mut children: Vec<(usize, Vec<u64>)> = Vec::new();
        for &li in &whole {
            commands.push(Cmd::SetLayerParent {
                index: li,
                before: parent,
                after: Some(group_id),
            });
            children.push((
                li,
                self.layer_tree_indices(li)
                    .iter()
                    .map(|&i| self.doc.layers[i].id)
                    .collect(),
            ));
        }
        for (li, moving, remaining) in pieces {
            let mut child = self.doc.layers[li].clone();
            child.id = crate::document::next_id();
            child.name = if moving.len() == 1 {
                moving[0].name.clone()
            } else {
                "Objects".into()
            };
            child.parent = Some(group_id);
            *child.kind.shapes_mut().unwrap() = moving;
            commands.push(Cmd::SetVectorShapes {
                layer: li,
                before: self.doc.layers[li].kind.shapes().unwrap().to_vec(),
                after: remaining,
            });
            children.push((li, vec![child.id]));
            commands.push(Cmd::AddLayer {
                index: order.len(),
                layer: child.clone(),
            });
            order.push(child.id);
        }
        children.sort_by_key(|p| p.0);
        let moved: Vec<_> = children.iter().flat_map(|p| p.1.iter().copied()).collect();
        let top = insertion_top;
        let mut desired: Vec<_> = self.doc.layers[..=top]
            .iter()
            .map(|l| l.id)
            .filter(|id| !moved.contains(id))
            .collect();
        desired.extend(moved.iter().copied());
        desired.push(group_id);
        desired.extend(
            self.doc.layers[top + 1..]
                .iter()
                .map(|l| l.id)
                .filter(|id| !moved.contains(id)),
        );
        reorder_commands(&mut order, &desired, &mut commands);
        self.finish_group(group_id, Cmd::Batch(commands));
    }

    pub fn ungroup_selected(&mut self) {
        self.with_action_step(crate::actions::Step::Ungroup, |studio| studio.ungroup_selected_command());
    }

    fn ungroup_selected_command(&mut self) {
        let index = self
            .selected_layer
            .and_then(|id| {
                self.doc
                    .layers
                    .iter()
                    .position(|l| l.id == id && l.is_group)
            })
            .or_else(|| {
                self.explicit_group_rows().and_then(|rows| {
                    rows.into_iter()
                        .rev()
                        .find(|&i| self.doc.layers[i].is_group)
                })
            })
            .or_else(|| {
                self.primary()
                    .and_then(|(li, _)| self.doc.layer_ancestors(li).first().copied())
            })
            .or_else(|| {
                self.active_layer
                    .filter(|&i| self.doc.layers.get(i).is_some_and(|l| l.is_group))
            });
        let Some(index) = index else {
            self.status = "Select a layer group to ungroup".into();
            return;
        };
        if !self.layer_unlocked(index)
            || self
                .layer_tree_indices(index)
                .iter()
                .any(|&i| self.doc.layers[i].locked)
        {
            self.status = "Unlock the group and its layers before ungrouping".into();
            return;
        }
        let group = self.doc.layers[index].clone();
        let child_ids: Vec<u64> = self
            .layer_panel_order(true)
            .into_iter()
            .rev()
            .filter(|&i| self.doc.layers[i].parent == Some(group.id))
            .map(|i| self.doc.layers[i].id)
            .collect();
        let desired = self.ungrouped_storage(index);
        let mut commands = Vec::new();
        for (i, layer) in self.doc.layers.iter().enumerate() {
            if layer.parent == Some(group.id) {
                commands.push(Cmd::SetLayerParent {
                    index: i,
                    before: layer.parent,
                    after: group.parent,
                });
            }
        }
        let mut removed = vec![group.id];
        removed.extend(group.kind.shapes().unwrap_or(&[]).iter().map(|shape| shape.id));
        let before_motion = self.doc.motion.clone();
        let mut after_motion = before_motion.clone();
        after_motion.drop_shapes(&removed);
        if after_motion != before_motion {
            commands.push(Cmd::SetMotion {
                before: before_motion,
                after: after_motion,
            });
        }
        if self.doc.layers.len() == 1 {
            commands.push(Cmd::AddLayer {
                index: 1,
                layer: Layer::vector("Layer 1"),
            });
        }
        let mut order: Vec<u64> = self.doc.layers.iter().map(|layer| layer.id).collect();
        let position = order.iter().position(|id| *id == group.id).unwrap();
        commands.push(Cmd::RemoveLayer {
            index: position,
            layer: group.clone(),
        });
        order.remove(position);
        if self.doc.layers.len() > 1 {
            reorder_commands(&mut order, &desired, &mut commands);
        }
        let command = Cmd::Batch(commands);
        if let Err(error) = layer_copy::validate(&self.doc, &command) {
            self.status = error;
            return;
        }
        let before = self.history_target();
        self.commit(command);
        self.layer_expanded.retain(|id| *id != group.id);
        let released: Vec<usize> = self
            .layer_panel_order(true)
            .into_iter()
            .rev()
            .filter(|&i| child_ids.contains(&self.doc.layers[i].id))
            .collect();
        if let Some(&first) = released.first() {
            self.select_layer_rows(first, false, false);
            for index in released.into_iter().skip(1) {
                self.select_layer_rows(index, false, true);
            }
        } else if let Some(index) = self.doc.layers.len().checked_sub(1) {
            self.activate_layer_tree(index);
        }
        self.history.remember_target(before, self.history_target());
        self.status = "Group released".into();
    }

    /// Whole-layer rows chosen in the sidebar, independent of object hits.
    /// Args: none. Returns painter order, or nothing when the editor only has an object selection.
    pub(crate) fn explicit_group_rows(&self) -> Option<Vec<usize>> {
        if self.selected_layer.is_none() && self.current_layer_rows().is_none() {
            return None;
        }
        let rows = self.selected_layer_rows();
        (!rows.is_empty()).then_some(rows)
    }

    /// Whether Group has an explicit row or object selection to act on.
    /// Args: none. Returns false during another operation.
    pub(crate) fn can_group_selection(&self) -> bool {
        self.op.is_none()
            && !self.updates.freezing
            && (self.selected_layer.is_some()
                || self.current_layer_rows().is_some()
                || !self.selection.is_empty())
    }

    /// Capture the document revision and grouping target before a dialog edit.
    /// Args: none. Returns the current target, or a refusal when nothing can be grouped.
    pub(crate) fn capture_group_request(&self) -> Result<GroupRequest, String> {
        if self.show_welcome || self.op.is_some() || self.updates.freezing || self.file_dialog_pending()
        {
            return Err("Finish the current operation before grouping".into());
        }
        if !self.can_group_selection() {
            return Err("Select objects or a layer to group".into());
        }
        let rows = self.explicit_group_rows().map(|rows| {
            rows.iter()
                .map(|&index| self.doc.layers[index].id)
                .collect()
        });
        let objects = if rows.is_some() {
            Vec::new()
        } else {
            self.selection
                .iter()
                .filter_map(|&(index, id)| self.doc.layers.get(index).map(|layer| (layer.id, id)))
                .collect()
        };
        Ok(GroupRequest {
            document: self.swap_id.clone(),
            revision: self.last_input,
            rows,
            objects,
        })
    }

    /// Refuse a group dialog once its document or selection is no longer current.
    /// Args: request captured when the dialog opened. Returns an error without editing.
    pub(crate) fn group_request_current(&self, request: &GroupRequest) -> Result<(), String> {
        if self.swap_id != request.document || self.last_input != request.revision {
            return Err("The document changed. No layers were grouped".into());
        }
        if self.op.is_some() || self.updates.freezing {
            return Err("Finish the current operation before grouping".into());
        }
        match &request.rows {
            Some(rows) => {
                let now: Vec<u64> = self
                    .explicit_group_rows()
                    .unwrap_or_default()
                    .iter()
                    .map(|&index| self.doc.layers[index].id)
                    .collect();
                if now != *rows {
                    return Err("The selected layers changed. No layers were grouped".into());
                }
            }
            None => {
                if self.explicit_group_rows().is_some() {
                    return Err("The selected layers changed. No layers were grouped".into());
                }
                let now: Vec<_> = self
                    .selection
                    .iter()
                    .filter_map(|&(index, id)| {
                        self.doc.layers.get(index).map(|layer| (layer.id, id))
                    })
                    .collect();
                if now != request.objects {
                    return Err("The selected objects changed. No layers were grouped".into());
                }
            }
        }
        Ok(())
    }

    fn group_explicit_rows(&mut self, name: &str, rows: &[usize]) {
        let roots = group_roots(&self.doc, rows);
        if roots.is_empty() {
            self.status = "Select objects or a layer to group".into();
            return;
        }
        if roots.iter().any(|&index| {
            !self.layer_unlocked(index)
                || self
                    .layer_tree_indices(index)
                    .iter()
                    .any(|&child| self.doc.layers[child].locked)
        }) {
            self.status = "Unlock the layers before grouping".into();
            return;
        }
        let top = *roots.last().unwrap();
        let parent = self.doc.layers[top].parent;
        let mut group = Layer::group(name);
        group.parent = parent;
        let group_id = group.id;
        let mut commands = vec![Cmd::AddLayer {
            index: self.doc.layers.len(),
            layer: group,
        }];
        let mut order: Vec<u64> = self.doc.layers.iter().map(|layer| layer.id).collect();
        order.push(group_id);
        for &index in &roots {
            commands.push(Cmd::SetLayerParent {
                index,
                before: self.doc.layers[index].parent,
                after: Some(group_id),
            });
        }
        let desired = self.grouped_storage(&roots, group_id);
        reorder_commands(&mut order, &desired, &mut commands);
        self.finish_group(group_id, Cmd::Batch(commands));
    }

    fn finish_group(&mut self, group_id: u64, command: Cmd) {
        if let Err(error) = layer_copy::validate(&self.doc, &command) {
            self.status = error;
            return;
        }
        let before = self.history_target();
        self.commit(command);
        let index = self
            .doc
            .layers
            .iter()
            .position(|layer| layer.id == group_id)
            .unwrap();
        self.layer_expanded.insert(group_id);
        for ancestor in self.doc.layer_ancestors(index) {
            self.layer_expanded.insert(self.doc.layers[ancestor].id);
        }
        self.activate_layer_tree(index);
        self.history.remember_target(before, self.history_target());
        self.status = "Grouped · Ctrl+Shift+G to ungroup".into();
    }

    fn grouped_storage(&self, roots: &[usize], group_id: u64) -> Vec<u64> {
        let paint: Vec<usize> = self.layer_panel_order(true).into_iter().rev().collect();
        let root_ids: HashSet<u64> = roots.iter().map(|&index| self.doc.layers[index].id).collect();
        let moved: HashSet<u64> = paint
            .iter()
            .copied()
            .filter(|&index| {
                let id = self.doc.layers[index].id;
                root_ids.contains(&id)
                    || self
                        .doc
                        .layer_ancestors(index)
                        .iter()
                        .any(|&ancestor| root_ids.contains(&self.doc.layers[ancestor].id))
            })
            .map(|index| self.doc.layers[index].id)
            .collect();
        let top = *roots.last().unwrap();
        let top_ids: HashSet<u64> = self
            .layer_tree_indices(top)
            .into_iter()
            .map(|index| self.doc.layers[index].id)
            .collect();
        let start = paint
            .iter()
            .position(|&index| top_ids.contains(&self.doc.layers[index].id))
            .unwrap();
        let mut block: Vec<u64> = paint
            .iter()
            .copied()
            .filter(|&index| moved.contains(&self.doc.layers[index].id))
            .map(|index| self.doc.layers[index].id)
            .collect();
        block.push(group_id);
        let mut desired = Vec::with_capacity(paint.len() + 1);
        for (offset, &index) in paint.iter().enumerate() {
            if offset == start {
                desired.extend(block.iter().copied());
            }
            let id = self.doc.layers[index].id;
            if !moved.contains(&id) {
                desired.push(id);
            }
        }
        desired
    }

    fn ungrouped_storage(&self, group: usize) -> Vec<u64> {
        let paint: Vec<usize> = self.layer_panel_order(true).into_iter().rev().collect();
        let group_id = self.doc.layers[group].id;
        let members: HashSet<u64> = self
            .layer_tree_indices(group)
            .into_iter()
            .map(|index| self.doc.layers[index].id)
            .collect();
        let start = paint
            .iter()
            .position(|&index| members.contains(&self.doc.layers[index].id))
            .unwrap();
        let block: Vec<u64> = paint
            .iter()
            .copied()
            .filter(|&index| {
                members.contains(&self.doc.layers[index].id) && self.doc.layers[index].id != group_id
            })
            .map(|index| self.doc.layers[index].id)
            .collect();
        let mut desired = Vec::with_capacity(paint.len());
        for (offset, &index) in paint.iter().enumerate() {
            if offset == start {
                desired.extend(block.iter().copied());
            }
            let id = self.doc.layers[index].id;
            if !members.contains(&id) {
                desired.push(id);
            }
        }
        desired
    }
}

/// Sidebar row targets with no selected ancestor, in the order supplied.
fn group_roots(doc: &Document, rows: &[usize]) -> Vec<usize> {
    let ids: HashSet<u64> = rows
        .iter()
        .filter_map(|&index| doc.layers.get(index).map(|layer| layer.id))
        .collect();
    rows.iter()
        .copied()
        .filter(|&index| {
            doc.layers.get(index).is_some()
                && !doc
                    .layer_ancestors(index)
                    .iter()
                    .any(|&ancestor| ids.contains(&doc.layers[ancestor].id))
        })
        .collect()
}

/// Document revision and selection captured by the named group dialog.
#[derive(Clone)]
pub(crate) struct GroupRequest {
    pub document: String,
    pub revision: std::time::Instant,
    pub rows: Option<Vec<u64>>,
    pub objects: Vec<(u64, u64)>,
}
