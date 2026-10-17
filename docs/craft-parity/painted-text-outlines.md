# Painted text outlines

The existing object conversion kept one object fill and lost native character colours. The new Type > Create outlines route prepares real editable fill paths and a final whole-object stroke, inside the existing frame appearance group. The original object ID, layer/painter position, parent, rotation and whole-object mask/opacity/effects stay on that group. Numeric gradient and retained pattern placement are rebased from the original paint bounds to each path's bounds.

Selected text and text inside selected frames are preflighted together. Linked stories use the existing typed break commands before their captured visible paths replace text. One ordinary history transaction reverses the accepted conversion; no new model or history mechanism is introduced. Non-text selected objects stay unchanged. The existing simple point-text conversion stays on its exact curve route.

This source layer is unqualified pending its focused checks. Imported SVG text trees, shape-distance paint, animated paint/reveal, contour-wrapping sources, placed text images and text already serving as a layout frame need further adapters; they refuse before ending the current draft or changing redo. These remain required work, not full Type/outline parity. No model/schema, font loader, renderer, dependency or version change.
