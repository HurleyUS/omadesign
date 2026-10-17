# Editable variable fonts

Native text and character spans retain OpenType user coordinates alongside the unchanged original face reference. The same validated settings feed the existing Rustybuzz shaper, ab_glyph outlines and metrics, optical spacing, frame/path composition and their caches. Changing to another ordinary face resets old coordinates. Native styled clipboard data explicitly carries the source point so destination defaults cannot leak into pasted text.

Default coordinates add no project fields. Nonempty native coordinates require project reader 34, including stored instance baselines, embedded Motion documents and retained cells. Axis data is bounded to 64 unique printable tags with finite coordinates. Existing SVG variable CSS remains supported through its current rich-text representation.

This is an in-progress foundation. The first software group is not run yet. Named-instance enumeration and Find Font binding still need implementation. Native SVG uses the existing glyph-outline delivery route for native variable text. PDF, IDML and Live protocol 3 refuse native axis data until their routes preserve it; those refusals are unfinished parity work, not completion. No native desktop, human QA, release, version or install claim.
