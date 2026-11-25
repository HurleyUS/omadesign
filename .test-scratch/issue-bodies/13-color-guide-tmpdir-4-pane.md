**Test:** `ui::color_guide::tests::craft_color_pane_body_can_open_another_pane_in_the_actual_workspace`

**Panic:** `Run through the serialized helper: NotPresent` (`src/ui/color_guide.rs:878`, `std::env::var("TMPDIR").expect(...)`).

Same shared root cause as the sibling color-guide TMPDIR issues: the test `context()` helper requires the `TMPDIR` that the repo's serialized helper (`scripts/serialized-cargo.mjs`) sets; in a bare `cargo test --lib` shell it is unset and the helper panics. With `TMPDIR` set under `/home` this test passes.

Reproduce (panics): `TMPDIR` unset + `cargo test --lib -- craft_color_pane_body_can_open_another_pane_in_the_actual_workspace`
Reproduce (passes): `TMPDIR=/home/michael/Projects/omadesign/.test-scratch cargo test --lib -- ...`

Run on master `4cb4afd1` (Release 0.6.8: Craft parity).
