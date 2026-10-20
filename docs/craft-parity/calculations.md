# Calculations

**Calc** in the Channels panel blends two scalar sources into a new alpha channel or the pixel selection. Sources can be Gray (composite luminance), Transparency, each color channel or any saved alpha/spot channel, and each can be inverted. Blending modes are Normal, Multiply, Screen, Overlay, Add, Subtract, Difference, Darken and Lighten, with opacity. The result is mix(source 2, blend(source 1, source 2), opacity), in source 2's grid. Source 1 is resampled when the placements differ.

A new channel is one Undo step. A selection result is not history, matching the other selection edits. Stale owners and missing sources are refused.

At `8025677c972297bed04486eb0d1893c11ce7aa6b` that route passed 20 tests. Receipt `f4acd0fca569608193dfd1059262bf02d637a632b2069a3edd5a4c3537407cff`.

## Decisions to review

- Source 1 is the top layer and source 2 the bottom, as in Photoshop. Subtract is bottom − top with no scale/offset, and Add has no scale/offset either.
- No mask input and no "result into a new document" option. The two outputs are a new channel and the selection.
- Calculations always reads full-resolution sources, synchronously on the UI thread, with no live preview.
- There is no separate Apply Image dialog. Apply Image's layer blends stay in the existing raster filters.

Further interchange remains open.
