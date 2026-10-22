**Test:** `text::opentype::editing_tests::pending_features_clipboard_and_history_preserve_ranges`

**Panic:** `assertion failed: encoded.contains("\"version\":12")` (`src/text/opentype.rs:290`).

Reproduce: `cargo test --lib -- pending_features_clipboard_and_history_preserve_ranges`

The fixture creates a text layer, patches features (`liga`, `smcp`), exercises clipboard, commit, undo and redo, then asserts the encoded document contains `"version":12`. The encoder now writes the minimal content-required version (`features::required_version`), which for this document no longer lands on 12 -- this assertion froze encoder behavior from an older era.

Same underlying policy family as the two active-selection version issues: `encode` writes the minimal required version, this test expects a fixed absolute version. Needs either a rewritten expectation (assert a supported version and a decode round-trip) or an encoder policy decision.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
