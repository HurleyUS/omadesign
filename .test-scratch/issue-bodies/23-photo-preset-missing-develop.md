**Test:** `ui::photo::workflow::keyboard_tests::keyboard_photo_workflow_preset_native_fields_save_reopen_and_stale_owner_refusal`

**Panic:** `missing native widget Photo develop` (`src/ui/welcome.rs:499`, `named()`).

Reproduce: `cargo test --lib -- keyboard_photo_workflow_preset_native_fields_save_reopen_and_stale_owner_refusal`

Same develop-panel label loss as the sibling photo failures: the photo develop widget is missing from the accesskit tree, so preset native-field authoring, save/reopen and stale-owner refusal branches never run.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
