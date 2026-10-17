# Find Font

Type → Find Font lists authored font usage separately from rendered fallback. Find selects matching text, including hidden and locked stories. Change edits matching selected stories; Change All edits matching writable stories in the document. Both use the existing captured Type target and one history command.

The picker lists real family aliases and OpenType style names from the renderer's system database plus document font bytes. “Closest to original style” uses the source weight and slant. A chosen collection face is extracted to standalone SFNT and registered by content in the existing portable font loader. Opening, picking and cancelling do not edit text. Refresh rescans installed fonts; it never replaces a missing font with a similarly named one automatically.

The initial core has separate exact-source software receipts. Catalog, dialog and collection checks are pending. Missing-font folder discovery/addition, native desktop operation and cold recovery remain in progress. This is not a full Find Font or full product parity completion claim.
