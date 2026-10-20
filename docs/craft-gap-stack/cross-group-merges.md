# Selected layer merges across groups

Merge Layers accepts selected rows from different groups. Command/Ctrl-click selects individual rows; Layer > Merge Layers or Command/Ctrl+E with canvas focus combines them. Alt-click Merge Layers or Command/Ctrl+Alt+E creates a merged copy and keeps every source layer.

The group tree determines paint order, including documents whose flat layer storage is not contiguous. Selected descendants of a selected group are included once. The result occupies the highest selected row's parent. A destructive merge retains that row's identity and name; a stamp gets a fresh identity immediately above it.

Selected local contents are composited together. Styles from outside the selected roots are not baked into the result: destination-group opacity, blend and visibility apply afterward. This matches PhotoCraft's selected-node merge/stamp source behavior. Consequently, moving local contents between differently styled groups can change the full image. Explicitly selected contents under hidden ancestors remain operable; a locally hidden selected root remains nonpainting. Mixed-parent merges clear the output's former clipping relationship rather than bind it to another group's base.

Existing background render/cancel, captured revision/selection, destination lock and atomic command gates remain. Source data retain RGB/CMYK U8/U16/F32 depth, ICC, intent, alpha, HDR/negative values and ink ownership through the existing precise compositor. Existing unsupported native effects and other named refusals remain.

Document Undo/Redo and native save/reopen are covered. Restoring the original multi-row UI selection on Undo is still open and requires canonical history target metadata; no parallel history was added. Grayscale/Lab, semantic Actions, broader clipping interchange and the rest of the full Craft parity ledger remain open.

Reference: PhotoCraft `4e7a17cd5d5723313f459753663d0d155f047e19`, `crates/engine/src/layer_multi_cmds.rs` and `stamp_cmds.rs`. Source inspected locally; no reference implementation copied or reference executable run. No renderer math, model/schema, resource limits, dependencies, version, installer or public release change.

## Exact validation

Source `1ff355b6dc481f6be3bfb752ea8c3242e97a9b26`: 31 focused application/native-widget tests passed, zero failed or ignored, 122.98 seconds. The source tree was clean before and after; immutable compiler receipt `e53fc78459ea1e157ea922c29c06d7835ad06f9ffe72111433dbddda90d52c0d` binds the actual test executable. This includes the new cross-group routes and directly affected existing masks, groups, stamps, cancellation, stale ownership and clipping checks.

The genuine current desktop executable was built through the same immutable helper, receipt `6369a4cccc2b4b8bcf8833f8caf019f8f172f05b6043ea97a8a6d6013e2c7ca8`. Binary SHA-256: `e793135b8e2e9c9f06f16b04bcb0361e58042e9791a851af196f264fe0eb0bda`.

Independent Node readers checked 21 native before/after case pairs through 42 cold file conversions and one derived workspace preview. Checks cover native JSON/zstd float bits, CMYKA, ICC/intent, IDs, parents, untouched layers, full PNG CRC/filter pixels and TIFF strip samples. Destination-style cases use literal straight-color output oracles. An initial reader assertion incorrectly compared equal JSON arrays by object identity; it was corrected to exact deep array equality, retaining the first script/failure and reusing successful outputs. No product change or Rust replay was needed.

QA launcher: `dist/omadesign-cross-group-merges-qa-1ff355b6/run`. The agent did not launch a GUI or touch the current human QA window. Durable handoff: `implementation/craft-cross-group-merges.json`. Issue #419 / stacked PR #420.
