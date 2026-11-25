**Test:** `ui::selection::tests::inspector_single_layer_and_shift_align_are_live_and_arrange_menu_is_gone`

**Panic:** `missing ` with an empty render-out label at `src/ui/selection_tests.rs:74` (`panic!("missing {label}")` inside `click()` -- the missing target is one of the inspector's painted glyph labels; the actual failure is the search finding no painted rectangle for the required click target).

Reproduce: `cargo test --lib -- inspector_single_layer_and_shift_align_are_live_and_arrange_menu_is_gone`

The single-layer painter must keep the alignment home(s) live so a click advances through align/distribute/order pairs and the old Arrange menu stays gone. The click target was not found among painted labels, which suggests the target glyph (an icon-font paint, e.g. `\u{e136}`) changed id or vanished from the inspector surface after the 0.6.8 inspector work.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
