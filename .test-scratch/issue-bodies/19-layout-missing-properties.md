**Test:** `ui::layout::field_tests::keyboard_layout_inspector_empty_document_grid_named_property_and_variable_authoring_reopens`

**Panic:** `missing native widget Properties` (`src/ui/welcome.rs:499`, `named()`).

Reproduce: `cargo test --lib -- keyboard_layout_inspector_empty_document_grid_named_property_and_variable_authoring_reopens`

On the empty-document layout grid state, the layout inspector must expose its named `Properties` widget through the accesskit tree; the widget no longer paints its native label. Grid, named-property and variable-authoring branches later in the test never run.

Shared suspect with the other "missing native widget" failures (layout/svg/timeline/photo/font): the 0.6.8 native-focus and field-routing work around `src/ui/welcome.rs`.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
