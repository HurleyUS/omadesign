# Type command menus

The top Type menu contains the existing area/point conversion, text-inside-shape, threading and path-text commands. Object no longer duplicates that submenu. Character, Paragraph, OpenType, Glyphs and the Typography library remain available as explicitly named pane/library entries.

Converting a linked frame to point text and attaching area text to a path retain the existing typed commands and reflow order, collected into one ordinary Undo entry. The current text draft is committed normally before the structural operation. Busy or scalar-channel contexts and locked, hidden or missing story members reject structural changes before committing that draft or disturbing redo. Native fields and the document model are unchanged.

This layer is source-only pending the focused text/readiness and actual top-menu workflow checks. It does not establish full Type parity. Paint-preserving rich text outlines, direct font/style menus, further multi-object formatting and the remaining reference commands still require work. Existing outline conversion is not promoted as lossless rich-type support by this menu change. No dependency, schema, writer or app-version change.
