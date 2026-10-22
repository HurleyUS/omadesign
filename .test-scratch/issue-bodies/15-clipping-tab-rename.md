**Test:** `ui::layer_clipping::tests::craft_clipping_native_canvas_chord_and_real_rename_field_keep_distinct_owners`

**Panic:** `native Tab did not reach Rename; actual focus traversal: [...]` (`src/ui/welcome.rs:522`, `tab_to`) after exhausting the 128-tab budget.

Reproduce: `cargo test --lib -- craft_clipping_native_canvas_chord_and_real_rename_field_keep_distinct_owners`

The captured traversal shows focus walking the menu bar (File, Edit, Image, Layer, Type, Select, Filter, View, Window, Plugins), then every mode row (Design, Pixel, Layout, Welcome, Photo, Motion, Agent, Inspect, Typography, Palettes, Brand assets), then the Clipping UI window chrome -- and stopping without ever reaching the rename text field inside the panel. The rename field dropped out of (or behind) the traversal.

Context: part of the native-focus/keyboard cluster that broke with the 0.6.8 focus work in `src/ui/welcome.rs` (deferred focus handoff, native field routing). The sibling clipping and merge tab-traversal tests fail the same way for their targets.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
