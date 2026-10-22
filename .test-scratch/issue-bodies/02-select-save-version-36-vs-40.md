**Test:** `app::active_selection::tests::craft_active_selection_save_and_reopen_keeps_exact_soft_coverage`

**Panic:** assertion `left == right` failed, left: 36, right: 40 (`src/app/active_selection_tests.rs:42`).

Reproduce: `cargo test --lib -- craft_active_selection_save_and_reopen_keeps_exact_soft_coverage`

The saved `.oma` file for a document with a soft pixel selection carries `"version": 36` while the current format version is 40 (`src/project/features.rs:35`, `COLOR_TAGS`). The test asserts the saved version equals `crate::project::VERSION`.

Root cause: `crate::project::encode` writes `features::required_version(doc)` (`src/project.rs:60`) -- the minimal version that preserves the document -- so a doc with an `active_selection` (feature `ACTIVE_SELECTION = 36`) but no color tag saves as 36, not the current 40. Either the encoder needs a policy change (gate files that must round-trip on the current version) or the test expectations need to match the minimal-version policy.

Shared root cause with the OpenType version-12 issue and the older-reader issue; filed separately per request.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
