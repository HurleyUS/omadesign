**Test:** `ui::timeline::keyboard_field_tests::keyboard_motion_native_timing_and_preset_authoring_undo_and_save_reopen`

**Panic:** `missing native widget Motion preset duration (s)` (`src/ui/welcome.rs:499`, `named()`).

Reproduce: `cargo test --lib -- keyboard_motion_native_timing_and_preset_authoring_undo_and_save_reopen`

The timeline preset panel lost its named `Motion preset duration (s)` field from the accesskit tree; the native timing and preset authoring branches (including undo and save/reopen) never run.

Shared suspect with the other "missing native widget" failures in this sweep; possibly related to `Enable native keyframe authoring through accepted motion commands` and the accepted-motion-fields changes in the same 0.6.8 stack.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
