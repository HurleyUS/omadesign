**Test:** `ui::motion_compositions::keyboard_tests::keyboard_composition_invalid_native_drafts_and_external_revisions_do_not_cross_owners`

**Panic:** assertion failed at `src/ui/welcome.rs:577` -- "The actual error for Composition start must paint once" — painted count 0, expected 1. The error text is `Enter a finite number within the shown range` and the field bounds resolve (`Rect { x0: 600.0, y0: 756.15625, x1: 771.8125, y1: 790.15625 }`), but the visible paint list no longer contains that error string.

Reproduce: `cargo test --lib -- keyboard_composition_invalid_native_drafts_and_external_revisions_do_not_cross_owners`

The composition `start` field shows an accesskit error message but does not paint the error visually once; the visible error label vanished while the accessibility annotation still fires (`error.bounds()` is non-None). Related first: `Reveal rejected native fields and fit named property rows` in `welcome.rs` history, and the visibly deduped/rejected field changes.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
