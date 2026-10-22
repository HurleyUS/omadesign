**Test:** `ui::selection::tests::inspector_routes_all_alignment_distribution_and_order_controls`

**Panic:** assertion `left == right` failed: single Align home, left: 0, right: 1 (`src/ui/selection_tests.rs:109`).

Reproduce: `cargo test --lib -- inspector_routes_all_alignment_distribution_and_order_controls`

The painted glyph list must contain exactly one painted instance of each alignment home glyph (the inspector routes each control glyph through a single home). For the first glyph (`ALIGN_LEFT`) zero instances paint, so the routing table for that branch broke; the remaining alignment/distribution/order glyph branches never run.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
