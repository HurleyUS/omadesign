**Test:** `ui::layer_merge::tests::craft_merge_row_context_uses_its_concrete_owner_and_rename_keeps_its_native_keys`

**Panic:** `native Tab did not reach Merge Down; actual focus traversal: [...]` (`src/ui/welcome.rs:522`, `tab_to`).

Reproduce: `cargo test --lib -- craft_merge_row_context_uses_its_concrete_owner_and_rename_keeps_its_native_keys`

Tab traversal walks menu bar, mode rows and Merge UI window chrome but never lands on the `Merge Down` control. The merge-row concrete-owner and rename-native-keys branches at the end never execute.

Shared suspect with the sibling clipping/merge tab failures: the 0.6.8 native-focus work in `src/ui/welcome.rs`.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
