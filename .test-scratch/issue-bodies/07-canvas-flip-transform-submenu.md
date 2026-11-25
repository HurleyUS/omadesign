**Test:** `ui::canvas::node_tests::canvas_context_flip_targets_the_clicked_object_and_preserves_multi_selection`

**Panic:** `Transform submenu` (`src/ui/canvas_node_tests.rs:395`, `.expect` on the search over painted menu text).

Reproduce: `cargo test --lib -- canvas_context_flip_targets_the_clicked_object_and_preserves_multi_selection`

After right-press (pointer button primary) on a canvas node, the test opens the context menu and looks for a painted text entry named `Transform` to hover-open the submenu for flip/rotate. Earlier assertions (selection preserved, no stray node/artboard selection) pass; only the menu lookup fails -- the context menu content itself regressed.

Note: the multi-selection branch reached by this search tests flips applied to a multi-selection, so until the Transform submenu paints again, that whole acceptance branch is dead.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
