**Test:** `ui::layer_clipping::tests::craft_clipping_row_context_owner_and_shift_selected_stack_follow_actual_rows`

**Panic:** `native Tab did not reach Create Clipping Mask; actual focus traversal: [...]` (`src/ui/welcome.rs:522`, `tab_to`).

Reproduce: `cargo test --lib -- craft_clipping_row_context_owner_and_shift_selected_stack_follow_actual_rows`

Tab traversal from the document surface never reaches the `Create Clipping Mask` control: the captured sequence runs through menu bar, mode rows and window chrome but the clipping row target is unreachable within the 128-tab budget. The shift-selected stack assertions at the end of the test never run.

Shared suspect with the sibling clipping/merge tab failures: the 0.6.8 native-focus work in `src/ui/welcome.rs`.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
