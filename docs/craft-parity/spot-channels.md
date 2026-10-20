# Spot channels

A saved channel can carry a spot ink: an sRGB display color and a solidity. Open **Spot** in the Channels panel to create a spot channel from the pixel selection (or an empty one at canvas size when nothing is selected), make the selected alpha channel a spot channel, change its ink, or convert it back to alpha. Each is one ordinary Undo step. Spot channels are painted, filtered, curved and transformed with the existing saved-channel tools.

The canvas, still and motion exports and export previews draw each spot channel as a tinted overlay at its ink color, in channel order, with coverage × solidity. Alpha channels never tint.

Files gain native feature 37 only when a spot channel exists. Alpha-only files keep version 32, and readers at 36 refuse 37 instead of dropping inks.

PDF: the existing writer has no Separation color spaces, so spot inks are flattened to process color as a top image and the export reports a warning. PSD spot channels follow in the channel interchange slice.

## Decisions to review

- Ink is stored as display sRGB plus solidity, not as a named library ink with Lab values. The default ink is magenta-pink at full solidity.
- The overlay uses nearest channel samples at the output resolution and mixes in 8-bit, like Photoshop's on-screen spot preview, so flattened values round once.
- The float (native/deep-color) compositor does not draw spot overlays yet. Native documents already refuse the RGBA8 export paths.
- PDF flattening rasterizes the ink at one pixel per document unit.

Calculations and broader interchange remain open.
