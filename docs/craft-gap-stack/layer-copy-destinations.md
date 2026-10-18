# Duplicate Layer destinations

The Pixel Layer menu and the layer row context menu open **Duplicate layer…**. Native name and document controls can copy the concrete layer or complete group into the current document, another open document, or a new document. The Layers copy icon and existing Duplicate selection shortcut keep their immediate-copy behavior.

Opening the dialog, changing its fields and cancelling do not edit either document. The source document, revision and layer identity are captured. Existing destinations are captured by document identity and revision, so a closed, replaced or edited destination is refused before switching tabs or committing history.

Same-document copies remain immediately above the source in its existing parent. Other-document copies retain their canvas positions and are inserted above the destination's active layer in that layer's parent. The destination becomes active and owns one Undo transaction. A new document keeps source dimensions, DPI, workspace, color settings and animation timing, with transparent empty space. It starts unsaved and excludes source collaboration state, comments and unrelated visible artwork.

Native pixel/mask buffers, editable geometry, linked stories, component overrides, tracks and expression owners use the existing layer-copy rules. Original source attachments and referenced design tokens travel with the copy; token identities and conflicting names are remapped. Raster authoring converts through the existing document color conversion only when source and destination settings differ. Mask samples stay scalar and are not color converted.

External live-mask sources, component families, text paths and prototype destinations are collected transitively. Their layers are retained in a hidden, locked **Copy dependencies** group with fresh object identities and remapped links. Source layers remain unchanged. The group is ordinary editable layer data, with no new serialized fields, schema, archive loader or renderer.

Cross-document expressions that explicitly call `property(...)` currently refuse before any mutation; this cut does not rewrite expression source or silently redirect a channel to an unrelated destination object. Missing linked resources and invalid layouts also refuse. Multiple independently selected layer destinations, cross-document expression channel remapping, clipping/merge workflows and complete Craft parity remain open.

Source validation and exact runtime receipts are recorded separately in the implementation handoff. Authored tests do not certify OS clipboard delivery, screen-reader announcements, visible desktop behavior or human acceptance. The running human QA window is untouched.
