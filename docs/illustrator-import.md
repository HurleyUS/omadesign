# Illustrator editing data

Omadesign reads the editable artwork that Illustrator stores separately from its
PDF page. PDF-compatible AI and AI saved without its PDF preview use the same
`PieceInfo /Illustrator /Private` reader. Numbered `AIPrivateData` streams are
joined in numeric order; `AIPDFPrivateData` is accepted when the first form is
absent. Plain streams, PDF Flate compression, Illustrator zlib and Zstandard
editing-data compression are supported. EPS private data supports comment-line
ASCII85, zlib and Zstandard. Legacy Illustrator PostScript is parsed directly;
no PostScript program runs.

Native import reconstructs named layers and nested groups, artboards in their
original layout, editable cubic and compound paths, fill winding, clipping
groups, RGB paint, RGB equivalents of CMYK/spot paint, stroke caps/joins/dashes,
linear/radial gradients, visibility, locks, opacity and blend modes. Point type
from the Illustrator text engine and legacy AI type remain editable. Area type
keeps its text and rectangular frame bounds. Embedded raster samples and placed
picture previews are decoded. Local linked pictures are read only within the
source document's directory; their original bytes are retained once as a source
attachment and their placed layers reference that attachment.

Import notes are saved in the native document. Patterns, symbols, meshes,
unsupported operators, live brushes/effects/multiple appearances, unavailable
fonts, irregular/threaded area type, sheared type and mixed ATE font/paint runs
are labelled when encountered. Live appearance definitions remain in the
retained source. Mixed type currently keeps all editable characters with the
first run's font and paint. CMYK and spot paint uses an RGB equivalent; overprint
and named-ink separation are unsupported. These are fidelity limits, not a claim
of full Illustrator feature support. Source recovery is distinct from exporting
new Illustrator editing data after changing native artwork.

The tokenizer, postfix dictionaries and text-engine key mappings are adapted
from [VectorCraft at b12a52f](https://github.com/storytold/vectorcraft/tree/b12a52f7249e0aa73f030587e05ed9ce0c73ab67/crates/eps/src/import).
Its [MIT notice](../src/formats/illustrator/LICENSE-MIT) is retained.

## Verification

Generated tests exercise compressed and uncompressed private streams, numeric
block ordering, AI and AIPDF stream names, preview-independent editable art,
EPS ASCII85, legacy editable text, group hierarchy, multiple artboards,
compound paths, unsupported-feature notes, malformed input, linked source
deduplication and native save/reopen.

The ignored `illustrator_independent_` tests read a directory given by
`OMADESIGN_ILLUSTRATOR_FIXTURE`. They require the
following independent, real Illustrator files. They are deliberately absent
from this repository:

- `Test-Inkscape-LineIssue.ai` and `.svg`, supplied by their author in
  [Ink/Stitch issue 1623](https://github.com/inkstitch/inkstitch/issues/1623).
  The test compares stroke placement and widths against the author's SVG parsed
  directly by usvg, independently of Omadesign's SVG import conversions.
  The attachment has no explicit redistribution license.
- `font_template.ai` from
  [fontpixel/font-template at 6b613e2](https://github.com/fontpixel/font-template/blob/6b613e2d73d7189b754db04fb5c62e198594d882/font_template.ai),
  under the repository's MIT license, copyright Tom Chen. It contains 208
  artboards and 416 text-engine stories.
- `reuters-dynamicText.ai`, downloaded as `dynamicText.ai` from
  [Reuters ai2svelte at ba5675c](https://github.com/reuters-graphics/ai2svelte/blob/ba5675cc4378acf77902709c08b744ca5796a052/public/examples/dynamicText.ai).
  Its text engine contains 107 stories. The root README names ISC while the
  subdirectory's license is MIT; this fixture is used only for local, read-only
  QA without assuming redistribution rights.
  Its editing data has five artboards (`xs`, `sm`, `md`, `lg`, `xl`); the single
  PDF page previews `xl`. The test checks all five names and placements.

The test removes every PDF page's `Contents`, confirms identical editing data,
and compares the resulting native geometry, paint and artboards with the
original import. It requires actual editable text for both text fixtures and
checks native save/reopen. The generated and independent real-file checks are
separate. Passing software checks does not establish a native Illustrator
opening/saving session, human visual acceptance, or a new Illustrator-data
writer roundtrip.
