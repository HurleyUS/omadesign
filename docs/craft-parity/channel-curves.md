# Saved alpha Curves

The Channels pane opens Curves for the concrete saved alpha target. The graph edits 2–19 natural cubic control points or a 256-entry float drawing. Point inputs and outputs use the familiar 0–255 display scale; the actual channel remains float coverage. Arrow keys move the selected point, Page Up/Down changes selection, Insert adds a point, and Delete removes it. Tab reaches the native numeric controls. Draw mode supports continuous strokes and Shift continuation, smoothing and a bounded fit back to points.

Preview uses a proxy with a maximum edge of 512 pixels. Original toggles the displayed proxy. Neither touches artwork or history. Apply runs against the captured full-resolution float plane and pixel selection, then uses the existing channel command for one Undo step. Cancel retires its worker. Document, source or selection changes retire the dialog rather than applying to a new owner. Invalid or unordered points cannot apply.

The Pixel Color pane shows the actual foreground used by the brush (or the current fill tool). Foreground changes update tool settings rather than recoloring a selected object. Vector and text color controls keep their existing typed commands.

The implementation follows the local PhotoCraft reference's natural spline, point/drawing modes, smoothing and constant endpoints. Native evaluation uses original float inputs rather than an eight-bit lookup table. This layer does not implement color-channel Curves, reusable adjustment layers, eyedroppers, presets, curve-file interchange or full PhotoCraft parity. Existing Live/PSD refusal for saved channels remains.

Source and tests are being qualified at an exact committed revision. Draft source is not a runtime or human acceptance claim. Public release/version reconciliation remains separate.
