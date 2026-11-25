**Test:** `ui::studios::font_picker_tests::keyboard_font_native_effect_blend_retirement_and_layer_owner_guards`

**Panic:** `missing native widget Layer blend` (`src/ui/welcome.rs:499`, `named()`).

Reproduce: `cargo test --lib -- keyboard_font_native_effect_blend_retirement_and_layer_owner_guards`

The font/effect pane must expose its named `Layer blend` widget via accesskit; the widget no longer paints its native label after the 0.6.8 field routing changes, so effect/blend retirement and layer-owner-guard branches never run.

Shared suspect with the other "missing native widget" failures in this sweep (layout/svg/timeline/photo).

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
