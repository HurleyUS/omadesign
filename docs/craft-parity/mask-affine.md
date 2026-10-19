At `f66af369518a09f78da02b4015b5823953d88f36` that route passed 30 tests. Receipt `e75c5ca9d331acaff794ff368c82f1bc5f3af57cd7dab0fa72435e715fd3770c`.

# Affine mask editing

Saved alpha channels and the active pixel selection can be flipped horizontally or vertically and rotated 90° either way or 180°. Each edit turns around the world-space centre of the covered pixels. Use **Select ▸ Flip or rotate selection** for the selection or the Channels panel's ⟲ menu for the selected saved channel.

Only the stored affine placement changes. Coverage samples keep their shared owner and are never resampled, so soft edges stay exact at full depth, and reversing an edit restores the placement. A channel edit is one ordinary Undo step and saves without a new document feature, since channel placements were already stored. Selection edits, like Grow, Shrink and Feather, are not history. Missing, empty, stale or frozen targets are refused without change.

Free affine handles already exist through **Move / resize selection**. Mask filters, color/spot targets, calculations and interchange remain open.

## Qualification

