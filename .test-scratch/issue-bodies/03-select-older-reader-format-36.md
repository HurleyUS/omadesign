**Test:** `app::active_selection::tests::craft_active_selection_older_reader_refuses_the_new_version`

**Panic:** assertion `left == right` failed, left: Number(36), right: 40 (`src/app/active_selection_tests.rs:116`).

Reproduce: `cargo test --lib -- craft_active_selection_older_reader_refuses_the_new_version`

This is the "format 36" case from the failure sweep: the fixture doc has an active selection, so `encode` writes content-required version 36 (`ACTIVE_SELECTION`) while `crate::project::VERSION` is 40 (`COLOR_TAGS`, bumped in `0026878e` -- "Add Gray and Lab tagged documents ... (project version 40)"). The test intends a saved file with an active selection to be at the current format version so a one-version-older reader refuses it instead of silently dropping the selection. The test message is also stale: it still says "a version-35 reader must not silently drop it" from the era when VERSION was 36.

Same root cause as the save-and-reopen version issue and the OpenType version-12 issue: `crate::project::encode` saves the minimal required version while these tests expect the current version.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
