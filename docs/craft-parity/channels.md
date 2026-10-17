# Channels

Saved alpha channels are document-owned scalar planes, not copies of layer RGBA or thumbnail pixels. Their float samples use the existing admitted native sample codec and retain their grid and document placement. This preserves a selection from a placed or rotated raster without making it a different coordinate system.

The pane selects actual document color/transparency sources or saved alpha planes. Load uses full-resolution compositor data and the existing pixel-selection path. Replace, Add, Subtract and Intersect retain soft coverage; subtract/intersect require an existing selection. Saving a selection, naming, duplicating, inverting and deleting channels use ordinary typed commands and Undo. Duplicate shares immutable samples until an edit creates a new allocation. Busy, stale-document and stale-selection guards reject late worker output.

Pixel selections now retain their shared allocation owner while active, in an inactive tab or captured by a selection editor. Restart keeps its existing compatibility wire and admits restored masks before replacing current tabs. Selection preview algorithms and actual paint paths remain the existing routes.

Empty documents omit the new metadata. Stored channels require native document feature 32 across current, embedded and retained/deleted data; the application version does not change. Live protocol 3 cannot preserve saved channels, so the wire snapshot refuses before expanding their sample strings. PSD/PSB channel interchange is not implemented in this cut and refuses these documents rather than dropping their saved channels. PNG/JPEG and other ordinary flattened artwork exports do not paint auxiliary alpha planes into visible artwork.

This source is uncompiled and unrun. Native brush/Curves targeting, editable color/spot channels, channel options/visibility, Apply Image/Calculations, Quick Mask and Photoshop channel interchange still need implementation and validation. This is not full Channels or full Craft parity.
