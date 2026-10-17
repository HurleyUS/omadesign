# Full-depth selection coverage

This layer replaces the editor's authoritative byte selection with admitted float coverage. Saved channels and Quick Mask share their original full-depth owners when the grids match. Coordinate changes sample that original coverage in float rather than rendering through an RGBA8 mask. Brush, eraser, connected Fill, scalar Curves, native raster filters and selection combinations consume those float weights. A display overlay or actual byte image output may quantize its own pixels; it does not replace the selection's samples.

The existing selection grid, captured target guards, soft-coverage semantics and ordinary Quick Mask history path remain. New restart snapshots retain float coverage and still accept legacy byte selection snapshots. Saving a selection as an alpha channel or active Quick Mask uses the existing native scalar archive without a new file feature or app version.

Source is not qualified yet. This is a layer toward full Craft parity, not a completion claim. Persisting an ordinary active selection directly in the document, more selection commands and mask filters, broader Actions/clipboard/interchange, and the remaining Craft workflows stay open. The current human QA window must remain open and untouched.
