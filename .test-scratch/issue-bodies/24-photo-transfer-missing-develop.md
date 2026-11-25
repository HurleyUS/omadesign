**Test:** `ui::photo::workflow::keyboard_tests::keyboard_photo_workflow_transfers_adjustments_and_undo_without_pointer_or_preselection`

**Panic:** `missing native widget Photo develop` (`src/ui/welcome.rs:499`, `named()`).

Reproduce: `cargo test --lib -- keyboard_photo_workflow_transfers_adjustments_and_undo_without_pointer_or_preselection`

Same develop-panel label loss as the sibling photo failures: `Photo develop` is absent from the accesskit tree, so the adjustments transfer and undo branches never run.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
