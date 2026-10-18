# Pixel Layer menu

Pixel mode now exposes native layer ordering and complete layer-mask commands in the top Layer menu. Arrange moves whole layer groups among their siblings; it preserves descendants, stable identities and existing selection/history remapping. Layer Mask offers reveal/hide/from-selection, painting, inversion, application and removal through the same typed operations as the Layers panel. Existing New pixel layer, New group and Delete layer commands stay available. Duplicate is explicitly the existing selection action.

Menu state belongs to the current document and concrete layer. Mask actions require the layer and its ancestors to be unlocked and visible; Arrange respects sibling bounds; deletion also rejects locked descendants. An active operation or update freeze disables mutations. This is an app UI route, without another layer model, compositor or mask algorithm.

Native tests open the actual menu and activate commands with native keys. They check complete group order and stable target, HDR/signed samples during mask application, source pixel preservation, one-step full-document Undo/Redo/native reopen, new/group/delete operations, and disabled missing/locked/ancestor-locked/hidden/frozen targets. Existing mask/group and Pixel menu regression cohorts accompany the new paths.

This bounded menu work does not provide Merge/Flatten, clipping-group or broad image/canvas commands. Full Craft parity remains open. Source receipts and earlier failed fixtures remain distinct. No human desktop interaction, version, install or public release is part of this cut.

At exact6e645d21, the focused cohort ran18pass/1fail. The sole failed creation/deletion fixture tried Delete after Undo had correctly retired the newly created layer's selection. The correction gives each command a real selected starting layer; exact8b03e403 passes that isolated route1/1. Product bytes are unchanged between those revisions. The earlier71ea compile refusal from using a nonexistent History::is_empty test helper is retained separately. These are separate exact receipts, not a single-head19/19 claim.
