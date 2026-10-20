# SVG text with authored bidi controls

The source character map retains authored embedding and isolate controls. These controls have zero visible advance but still consume SVG character-position slots. Removing them from the native map caused range and pair edits to target a later or earlier letter.

The existing direction normalizer now reports exactly where it inserted runtime controls. The mapper excludes only those insertions, including adjacent identical authored and runtime controls. Original source offsets and chunk boundaries remain attached to the original logical slots. No alternate font loader, text model or renderer is added.

The affected native group at75b59c0c ran109pass/1fail: the new scale fixture assumed style-batch order was character order. The correction compares identities by source index and independently runs rotated and wrapped cases; both pass at989d818c with identical application bytes. The109 passing results remain separate. They cover range baseline editing, authored control offsets, nested RTL/entity mapping, native keyboard/history, fonts, Optical/pair/scale routes, paint and native/editable reopen. Exact receipts and the original borrowing refusal remain in `implementation/craft-svg-authored-bidi.json`.

Actual Chromium comparison passes6/7 new editable examples and4/4 separately authored baseline controls at the unchanged1.5 image limit. Rotated scale with authored bidi controls fails4.12434. Caret diagnostics also differ on authored controls and a scaled RTL letter; they are not full caret qualification. The visible baseline changes address exactly the intended source letter.

A genuine989d818c desktop binary and seven native samples are available in `dist/omadesign-svg-bidi-candidate-989d818c/run`. All seven saved and cold-reopened in14 fresh headless CLI processes with exact native pixels. This remains a private development candidate. The current human QA window and earlier candidates are untouched; PR402 stays draft.

Full bidi caret/navigation remains open. Existing mixed-affine scale, three independent scale raster comparisons, non-flow leading and full Craft parity remain open. This change does not certify those routes.
