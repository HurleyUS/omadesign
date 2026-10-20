# Whole-row grouping

Explicit layer rows group on their own, including empty groups. A selected descendant of a selected row stays inside that row. The new group takes the parent of the topmost selected root and occupies that root's sibling slot. Children keep painter order, with the group header above them. One Undo restores each moved layer's original parent.

Ungroup removes only that group. The children return to its sibling slot in their previous order. Tracks and expressions owned by the removed group, including shapes stored on that group, drop. Child layers and their animation stay. A document whose only layer is the group still receives the existing replacement layer.

Group from Layers opens a native name dialog. The shortcut and the plain name Group record `Group`. Any other trimmed name records `GroupNamed`. Cancel leaves the document alone. A changed document, revision, row selection, or object selection keeps the dialog open and adds no history. Locked rows and a busy editor refuse before history. Partial objects still extract on the existing path when no whole row is selected.

Qualified source `b3c7de1e81940d286d5f58fbac1ef7baed520502`, tree `fb25c2d45bdb64de1362c7cf4fc9a3db9dabe7ec`. Two receipts on that exact commit:

- `578c9b32888437860d21a27a3214851dc714bb4cdac2133d5f473a1cdf2fe688`: 26 passed, 0 failed, 204.56s. Filters `hierarchy_edit::tests`, `craft_group_dialog_native_name`, `app::layer_groups`, `craft_pixel_layer`. Lib test SHA-256 `83af7bccd658492c948536844fdf9b425fc34e1ae129b9a84c1d656290ba7408`.
- `0672c5a2efbfd91beaf71d3fcca346281179dce3ad6d6a503b7d48b45ebfa866`: 1 passed, 0 failed, 4.56s. Filter `craft_pixel_duplicate_layer`, same executable, fresh cache hit.

Cargo.toml, Cargo.lock, and the document schema are unchanged. No GUI was launched. Saved selection, Find Font, Gray/Lab, Actions, and the rest of Craft parity remain open. Packet: `implementation/craft-layer-grouping.json`.
