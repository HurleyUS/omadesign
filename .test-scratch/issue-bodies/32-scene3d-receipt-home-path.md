**Test:** `scene3d::render::qualification_tests::independent_output_receipt_and_native_disk_reopen`

**Panic:** `assertion failed: output.starts_with("/home/")` (`src/scene3d/render.rs:1186`).

This is an environment-sensitive failure. The receipt must be written to a scratch directory rooted under `/home`, sourced from `TMPDIR`. The repo's serialized-cargo helper (`scripts/serialized-cargo.mjs`) sets `TMPDIR`/`TMP`/`TEMP` to its `/home` scratch and the test asserts absolute paths under /home, so with TMPDIR unset (bare cargo test in a shell whose TMPDIR is absent or /tmp) the assert fails; with TMPDIR set under /home the test passes.

Options: accept `std::env::temp_dir()` as a valid root (assert it exists and is writable, no /home prefix) or document/require the helper invocation.

Reproduce (panics): `TMPDIR` unset + `cargo test --lib -- independent_output_receipt_and_native_disk_reopen`
Reproduce (passes): `TMPDIR=/home/michael/Projects/omadesign/.test-scratch cargo test --lib -- ...`

Run on master `4cb4afd1` (Release 0.6.8: Craft parity).
