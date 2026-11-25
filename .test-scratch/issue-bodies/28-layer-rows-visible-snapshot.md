**Test:** `ui::studios::layer_rows::tests::large_expanded_layers_only_snapshot_visible_rows_and_a_live_rename`

**Panic:** assertion `left == right` failed, left: `[4, 5, 6, 7, 1999]`, right: `[5, 6, 7, 1999]` (`src/ui/layer_rows.rs:94`).

Reproduce: `cargo test --lib -- large_expanded_layers_only_snapshot_visible_rows_and_a_live_rename`

With a 2000-row expanded layer list, `snapshot()` must only clone rows that intersect the clip range; row with `visual_index = 4` (offscreen above the visible clip `y0: 180.0` row band) leaks into the snapshot. The visible-row isolation contract broke (one extra row cloned), so the live-rename assertion at the end (`rows[3].name == "Object 0"` on masked row 1994) also failed to prove isolation.

Starting points: `src/ui/layer_rows.rs` snapshot/clip math and the decoded raster row crop caching in the same commit neighborhood.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
