**Test:** `ui::browsers::keyboard_tests::keyboard_browser_places_bundled_shape_and_undo_saves_reopens_without_pointer_setup`

**Panic:** `Closed browser must restore a native panel` (`src/ui/browser_keyboard_tests.rs:121`, `.expect(...)` on `ctx.memory(focused)`).

Reproduce: `cargo test --lib -- keyboard_browser_places_bundled_shape_and_undo_saves_reopens_without_pointer_setup`

After placing a bundled browser shape on the canvas with native keys, undoing, saving and reopening, the test closes the browser with the Escape/keyboard path and asserts focus lands on a real native panel widget. At :121 no widget is focused at all, so the keyboard-only flow has nowhere to continue into.

Context: part of the native-focus/keyboard cluster that broke with the 0.6.8 focus work in `src/ui/welcome.rs` (deferred focus handoff, accepted-mixed-field routing, "Target actual native panel controls and supported Motion creation in acceptance"). The layer-merge and layer-clipping tab-traversal failures share this suspect introduction.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
