**Test:** `ui::color_guide::library::tests::craft_color_theme_library_preserves_corrupt_future_and_concurrently_changed_files`

**Panic:** `Run through the serialized helper: NotPresent` (`src/ui/color_guide.rs:878`, `std::env::var("TMPDIR").expect(...)`).

This is an environment-sensitive failure. The test's `context()` helper (`src/ui/color_guide.rs:873-886`) requires `TMPDIR` to point somewhere writable; the repo's canonical invocation (`scripts/serialized-cargo.mjs` per `docs/raw-camera-qualification.md:160-167`) sets `TMPDIR`/`TMP`/`TEMP` to its `/home` scratch directory. In a bare `cargo test --lib` shell TMPDIR is unset, so the helper panics; with TMPDIR set under `/home` this test passes. The assert message itself acknowledges the dependency ("Run through the serialized helper").

Options: default the helper to a writable location (e.g. `std::env::temp_dir()` with a `/home` fallback) instead of panicking, or document that these cases must run through the helper.

Reproduce (panics): `TMPDIR` unset + `cargo test --lib -- craft_color_theme_library_preserves_...`
Reproduce (passes): `TMPDIR=/home/michael/Projects/omadesign/.test-scratch cargo test --lib -- ...`

Run on master `4cb4afd1` (Release 0.6.8: Craft parity). Sibling issues cover the other four color-guide TMPDIR cases.
