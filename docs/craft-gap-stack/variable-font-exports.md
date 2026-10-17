# Variable-font instance exports

This layer starts from the native named-style work in #388. Native `.oma` keeps original font bytes and editable coordinates. The PDF writer derives a separate static glyph program for each rendered point through pinned `subsetter` 0.2.6, retains every glyph ID, and writes the existing CID/Unicode text route. One font instance is shared across pages and matching ranges. Native-first admission and all existing path/area/CFF fallback behavior remain in place.

The derived font retains the original character mapping, embedding flags, copyright/license names and ordinary substitution tables when those have no conditional variation records. Its family/style/weight describes the actual selected instance. The common OpenType table assembler also serves collection extraction, with checked offsets, full checksums and the existing portable font limit. Original font bytes and the native document are never replaced.

Five new pinned Rust packages are added; existing dependency versions stay fixed. Registry archive checksums, packaged MIT texts and the subsetter's complete third-party NOTICE are retained in the normal offline credits inventory. There is no new installed native library or application version.

The first coherent software group is unrun at this source commit. It includes both variable fonts and the complete existing PDF group, font permissions and native-first guards, independent Poppler/qpdf checks and a real cold project process. Embedded SVG text, imported-SVG axis reporting and full interchange parity remain open. Axis sliders are explicitly unfinished in the reference and are not a parity requirement. The current user QA window remains untouched; no public release or human acceptance is claimed.
