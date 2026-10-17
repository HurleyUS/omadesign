# Editable variable fonts

Native Type stores original OpenType font bytes and validated design-space coordinates. The existing Rustybuzz composer and ab_glyph outlines use the same point for metrics, curves, frame text, text on a path and optical spacing. Character overrides and rich clipboard data retain the point with its face.

Find Font reads real `fvar` named-instance records, including default-instance aliases and localized style names. Its existing family/style controls pass the original portable face and exact named coordinates through the captured Type command. Changing a style preserves unrelated character metadata and ordinary Undo. Counts are bounded to 64 axes and 512 named instances; damaged records are ignored without inventing styles.

Reader 34 protects typed coordinates in current artwork, stored instance baselines, embedded Motion documents and every retained write. Coordinates must be finite with unique printable tags. Defaults preserve historical native serialization. Original SVG variable CSS continues to work when no native axis override exists; an explicit span reset stays distinct from an inherited setting.

The first source group at `8edea5d0` compiled and ran 53 checks: 51 passed; two fixture setup errors are preserved. Their corrections and the named-instance UI checks await the next exact-source group. Native SVG/HTML currently use the existing outlined artwork delivery route. PDF, IDML and Live 3 refuse native axis data until their readers/writers preserve it. Numeric axis controls, complete interchange and full Find Font parity remain open. No native desktop, human acceptance, public release or version claim is implied.
