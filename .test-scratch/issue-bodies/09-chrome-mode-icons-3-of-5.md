**Test:** `ui::chrome::tests::mode_icons_stay_centered_and_document_previews_form_a_64px_left_column`

**Panic:** assertion `left == right` failed: all modes must remain directly available, left: 3, right: 5 (`src/ui/chrome.rs:1485`).

Reproduce: `cargo test --lib -- mode_icons_stay_centered_and_document_previews_form_a_64px_left_column`

The chrome dock must expose all five studio modes directly (Design/Pixel/Layout/Photo/Motion) with the mode group centered. The scan of painted mode labels found only 3, so two mode entries are missing their painted labels -- subsequent centering and the document-preview 64px-left-column assertions never run.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
