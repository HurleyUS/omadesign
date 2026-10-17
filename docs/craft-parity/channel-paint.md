# Paint saved alpha channels

Selecting a saved Channels row targets its actual scalar plane. The canvas shows that plane in grayscale at its stored placement. Brush and Eraser use the existing round or library tip, spacing, pressure and dynamics; the existing soft pixel selection is mapped into that grid before painting. Float source samples outside the stroke remain exact. A stroke changes one alpha plane, not any layer pixels, color profile or other saved channel.

Release commits one ordinary SetChannels command. Escape/Undo during a gesture restore its source allocation. Document, selection and target changes retire the gesture. Targets travel with their own document tabs and are transient across native reopen. Layer-mask selection and switching to a different editing tool return to the normal target. Hand/Zoom preserve the channel. Delete clears only the targeted plane using the current soft selection.

Canvas display textures are bounded previews and never feed the stored plane or selection. The source plane, brush coverage, working samples and mapped selection retain their existing shared-pool allocation owners. Far off-grid segments are clipped before visiting dabs, with spacing phase retained; the brush engine is unchanged.

This source has not compiled or run yet. Color/transparency channel editing, spot options, Curves/other channel adjustments, channel clipboard transfer, Quick Mask, calculations and PSD/PSB/Live channel interoperability remain open. Unsupported channel clipboard/adjustment actions refuse rather than edit layer pixels. The native wire remains feature32; no app version or dependency changes.
