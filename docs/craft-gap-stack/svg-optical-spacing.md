# Imported SVG optical spacing

The existing Optical kerning choice uses the actual selected glyph programs and normalized native outline cache. Pair measurements include real font size, variation/optical-size coordinates, glyph transforms and the inline baseline. The existing native optical target and bounded adjustment remain; authored spacing and manual offsets stay separate. Cursive scripts retain the existing no-extra-letter-spacing policy. Optional ligatures split only across a changed native metric boundary, while required shaping and untouched ligatures remain.

Standard dx/dy lists carry visible results. A bounded native metadata record retains the original offsets and optical ranges for another editable import; it restores the original source before applying the mode again, preventing double spacing. Flow source metadata retains its original story and line-spacing owner. Unknown or malformed positioning metadata refuses before replacing artwork.

The new positioned-glyph measurements are ephemeral and leave the normal vendor layout unchanged. They also provide the actual advance/baseline measurements needed by the still-open horizontal/vertical scale work in #391. Span transforms are not a valid substitute: SVG text spans are inline elements and transforms do not affect them. Reference: https://www.w3.org/TR/SVG2/text.html#TextLayoutAlgorithm

Source is not qualified until the native and independent checks complete. No model/schema, dependency, app-version, install, public release or existing human-window change is part of this cut. Full Craft parity remains open.
