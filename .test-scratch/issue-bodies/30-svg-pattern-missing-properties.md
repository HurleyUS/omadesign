**Test:** `ui::svg_field_tests::keyboard_svg_pattern_native_selection_scalars_draft_apply_errors_undo_and_reopen`

**Panic:** `missing native widget Properties` (`src/ui/welcome.rs:499`, `named()`).

Reproduce: `cargo test --lib -- keyboard_svg_pattern_native_selection_scalars_draft_apply_errors_undo_and_reopen`

Identical member to the sibling SVG filter case: the pattern pane requires its named `Properties` field in the accesskit tree and the widget is absent, so pattern scalar draft apply/error/undo/reopen branches never run.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
