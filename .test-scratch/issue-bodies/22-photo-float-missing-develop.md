**Test:** `ui::photo::keyboard_field_tests::keyboard_photo_workspace_native_float_close_restore_edit_and_book_reopen`

**Panic:** `missing native widget Photo develop` (`src/ui/welcome.rs:499`, `named()`).

Reproduce: `cargo test --lib -- keyboard_photo_workspace_native_float_close_restore_edit_and_book_reopen`

The photo workspace float-cycle test can no longer find the named `Photo develop` widget in the accesskit tree; the develop panel lost its native label after the 0.6.8 workspace/field changes. The float close/restore, edit and book-reopen branches never run.

Shared suspect with the other photo-workflow "missing native widget Photo develop" failures in this sweep.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
