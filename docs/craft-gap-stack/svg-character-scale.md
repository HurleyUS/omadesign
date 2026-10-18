# Editable SVG character scale

The scale adapter keeps the original source and typed character overrides. A temporary standard SVG span uses font size for the cross-axis factor and textLength with spacingAndGlyphs for its inline factor. Tracking and word spacing are compensated before inline adjustment; actual original advances, writing orientation and chunk boundaries come from the existing shaped glyph layout. Upright vertical glyphs exchange the font-size/inline roles. Auto optical sizing retains the original size through explicit variation coordinates.

Bounded scale records retain the span's prior attributes and accepted native scale. Editable import restores those attributes before rendering the accepted scale again. Flow source keeps its original story; measured scaled widths take part in line wrapping. No second font loader, compositor, schema or model is added.

This is an unqualified source candidate. Native geometry, mixed ranges, upright/rotated/path/RTL text, original glyph identity, field keyboard/history, malformed metadata and exact editable roundtrip checks are authored. Runtime, browser and packaged QA evidence are pending. Non-flow leading, authored bidi controls and full product parity remain open.
