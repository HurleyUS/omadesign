**Test:** `ui::layer_merge::tests::craft_stamp_native_alt_menu_and_canvas_keys_preserve_real_source_and_field_owners`

**Panic:** `native Tab did not reach Rename; actual focus traversal: [...]` (`src/ui/welcome.rs:522`, `tab_to`).

Reproduce: `cargo test --lib -- craft_stamp_native_alt_menu_and_canvas_keys_preserve_real_source_and_field_owners`

Same traversal break as the sibling tab failures: menu bar, mode rows, Merge UI window chrome, then dead -- the rename target never receives focus inside the 128-tab budget. The Alt-stamp flow (merge via canvas keys with the real source owner preserved) never runs.

Shared suspect with the sibling clipping/merge tab failures: the 0.6.8 native-focus work in `src/ui/welcome.rs`.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
