# Drag to collect

Dragging Layers rows (objects or a whole layer) and releasing them over the Asset Export pane collects them through the same capture and one-Undo command as the Collect buttons. The dropped items become the selection, as in Craft. Each object becomes its own asset, and Shift collects them as one. A drag from another document, a release outside the pane, Live mode and pending unsaved row edits are ignored.

## Decisions to review

- Dropping replaces the current object selection with the dropped items, so the pane and canvas agree.
- Only Layers-panel drags are accepted. Canvas drags still move artwork and don't collect.
- Dropping while row edits are unsaved is ignored, not auto-saved, to avoid mixing two transactions.

Symbols remain open.
