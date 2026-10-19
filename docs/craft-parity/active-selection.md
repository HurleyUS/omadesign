# Active selection persistence

An ordinary active pixel selection now stays with the saved document. Saving writes the selection's full-depth soft coverage, grid and placement into the file. Opening moves it back into the editor exactly, including partial coverage and a translated or rotated placement. The selection is not history: saving changes neither the document, the Undo history nor the dirty state, and opening adds no Undo step. Recovery snapshots and Live are unaffected because the stored selection exists only while the file is written.

A document that has a selection is written as native document feature 36. A document without one keeps its older feature version, and older readers refuse feature 36 rather than drop the selection silently. Damaged stored coverage is refused when the file is read. When Quick Mask is on, the mask already carries the selection, so nothing extra is stored. In Live, the selection is not written.

At `3249c71a622bcdc43ee37c2bb743a6bfc92b9ff2` that route passed 20 tests. Receipt `c59a2475dc92617a6f7784ce71290e17523f828406b6ba1cbe029bacdf1538c0`.

No new dependency or application version change. Affine mask editing, mask filters, color and spot targets, calculations and interchange remain open.
