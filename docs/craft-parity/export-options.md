# Export options and bleed

Each Asset Export row now has three new options:

- **Bleed:** extra area in document units on every side of the asset, artboard or document. Artwork stays in place.
- **Quality:** JPEG quality 1–100.
- **Variant:** for PNG rows, PNG-8, WebP or GIF.

Rows can be reordered with ▲/▼. Variants and quality are encoded from the same rendered PNG, inside the existing staging and no-replace publication, and staging files never leak. Files gain native feature 38 only when a row uses one of these options. Older readers refuse it, and invalid values are refused.

## Decisions to review

- WebP is lossless only, because the bundled codec has no lossy encoder. GIF uses the codec's built-in quantizer, with binary transparency.
- PNG-8 uses an in-house median cut to at most 256 RGBA colors with no dithering, written as indexed PNG with tRNS.
- Bleed is measured in document units (not pixels) and scales with the row size. It applies to raster and vector rows alike.
- Quality JPEG mattes on white and keeps PPI, like the shared JPEG writer. Without a quality setting, the shared writer's 95 still applies.

Drag-to-collect remains open.
