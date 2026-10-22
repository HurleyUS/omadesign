**Test:** `ui::svg_field_tests::keyboard_svg_filter_native_source_respects_graph_owner_lock_and_retires_undo_drafts`

**Panic:** `missing native widget Properties` (`src/ui/welcome.rs:499`, `named()`).

Reproduce: `cargo test --lib -- keyboard_svg_filter_native_source_respects_graph_owner_lock_and_retires_undo_drafts`

The SVG filter pane requires its named `Properties` field to be present in the accesskit tree; the widget lost its native label after the 0.6.8 field-routing changes, so the filter graph owner-lock and undo-draft retirement branches never run.

Shared suspect with the other "missing native widget" failures (layout/svg pattern/timeline/photo/font).

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
