# Imported SVG pair spacing and flow leading

The existing Pair kerning and Leading controls now feed the retained SVG layout. Pair values use the resolved right character's font size, replace its existing inline offset, and follow horizontal, vertical or path writing. Explicit zero clears an authored offset. Original x/y anchors, source text, styles and transforms remain retained. Flow measurement includes pair edits before choosing wraps, and flow source metadata retains the native pair map for another editable SVG import.

Optional ligatures around explicit character offsets or rotation are disabled through ranged features in the existing shaper. Required shaping, full-context bidi, fallback fonts and unaffected ligatures stay on that path. The vendor source, original license files, package version and dependencies remain pinned.

Flow line advance reads accepted per-character Fixed/Auto leading and each resolved source font size. The inspector reports the inherited flow value. Standard positioned tspans carry rendered line placement, while the existing flow-source metadata retains line-height ranges for later edits.

Qualification is pending. Focused tests cover native fields, existing-source offsets, ligatures, mixed sizes, RTL, vertical and curved text, wrapping, per-line leading, original-source equality, whole-document Undo/Redo, native reopen and editable SVG reimport. Independent browser comparisons and a current QA build will follow actual native results. Horizontal/vertical scaling, optical kerning and non-flow authored line leading remain open under issue #391.
