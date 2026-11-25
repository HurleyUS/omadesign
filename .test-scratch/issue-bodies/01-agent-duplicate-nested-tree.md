**Test:** `agent::all_modes_tests::duplicate_nested_layer_tree_keeps_external_parent_and_remaps_children`

**Panic:** assertion `left == right` failed, left: 4, right: 5 (`src/agent/all_modes_tests.rs:533`).

Reproduce: `cargo test --lib -- duplicate_nested_layer_tree_keeps_external_parent_and_remaps_children`

The agent-mode editor action `duplicate_layer` on an outer group containing an inner group that has a vector child must duplicate the whole subtree (5 layers total), but the result has only 4 layers -- a nested layer is dropped. The test further verifies the external-parent remap (the copied inner group must still hang off the original `outer` group) and that the copied child gets fresh shape ids.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
