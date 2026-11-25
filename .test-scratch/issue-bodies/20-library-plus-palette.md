**Test:** `ui::library::tests::palette_sidebar_creates_edits_applies_and_saves_transparent_colours`

**Panic:** `missing visible label "+ Palette": [...]` (`src/ui/library_tests.rs:360`) with a full label dump of the sidebar surface.

Reproduce: `cargo test --lib -- palette_sidebar_creates_edits_applies_and_saves_transparent_colours`

The library sidebar must show a painted `+ Palette` affordance to create a new palette; the dump shows the sidebar painted (Appearance panel, Swatches, Fill/Stroke entries and all inspector chrome) but no `+ Palette` label anywhere. Create/edit/apply/save palette branches never run.

The label dump confirms the inspector panels still paint, so this is a sidebar-body change (the palette section collapsed or moved) rather than a whole-panel rendering loss.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
