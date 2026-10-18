# SVG text with authored bidi controls

The source character map retains authored embedding and isolate controls. These controls have zero visible advance but still consume SVG character-position slots. Removing them from the native map caused range and pair edits to target a later or earlier letter.

The existing direction normalizer now reports exactly where it inserted runtime controls. The mapper excludes only those insertions, including adjacent identical authored and runtime controls. Original source offsets and chunk boundaries remain attached to the original logical slots. No alternate font loader, text model or renderer is added.

The affected native group covers range baseline editing, authored control offsets, nested RTL/entity mapping, glyph identity, rotated/wrapped scale, native fields, editable/native reopen and document Undo. Exact source-bound results are recorded in `implementation/craft-svg-authored-bidi.json`; the initial borrowing refusal and later fixture batch-order failure are retained separately.

Full bidi caret/navigation remains open. Existing mixed-affine scale, three independent scale raster comparisons, non-flow leading and full Craft parity remain open. This change does not certify those routes.
