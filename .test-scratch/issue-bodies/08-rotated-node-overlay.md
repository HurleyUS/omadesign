**Test:** `ui::canvas::node_tests::rotated_nodes_draw_pick_and_drag_in_one_world_frame`

**Panic:** `node overlay must follow rotation` (`src/ui/canvas_node_tests.rs:101`; the overlay-hit loop over `world_anchors` at :94-100 finds no matching painted Rect at the expected window point).

Reproduce: `cargo test --lib -- rotated_nodes_draw_pick_and_drag_in_one_world_frame`

For rotated layers, the canvas node overlay must paint its resize/rotate handles in one world frame matching pick and drag. The painted overlay rectangle for the expected anchor is missing from painted shapes, so rotated-node hit regions have drifted away from the drawn frame. Later drag/pick assertions in the same test never execute because the first anchor check dies.

Starting points: `canvas_frame` overlay painting order vs `world_anchors` conversion (`studio.canvas_rect`, `studio.view`).

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
