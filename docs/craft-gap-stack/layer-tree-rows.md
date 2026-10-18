# Layer tree rows, selection and Arrange

The Layers pane displays the actual group tree, with a group header immediately above its children. Storage order no longer splits a valid group's displayed contents. Shift ranges use those visible rows, with the reference's expanded-tree fallback when a programmatic endpoint is inside a collapsed group.

Command/Ctrl-click keeps a final targeted row. Adding or removing a row keeps the active row inside the selection and updates the range anchor to that active row. Entering row selection from only some objects in a layer establishes an explicit whole-layer target. None of these selection changes edit artwork or add history.

Arrange and layer drag/drop move a complete logical subtree through the existing parent/reorder commands, including valid noncontiguous storage. A semantic no-op does not normalize storage or create an edit. An actual move may order the stored tree consistently while preserving each layer's complete data; one Undo restores the exact original document. Cycles, locked drops and excessive depth refuse before mutation. Arrange, drag/drop and clipping changes retain their stable selection targets with the existing canonical history entry.

Clipping selection now uses logical bottom-to-top order rather than raw indices. Merge menu dispatch recognizes every explicitly selected row, including an active descendant whose selected ancestor already represents it in render roots. The existing typed merge worker, cancellation and revision guards remain.

No document model, renderer math, resource, native pixel/command wire, project schema, dependency, version, installer or public release changes. All other shared inspector bytes remain unchanged apart from the one row-order loop. Reference source: PhotoCraft `4e7a17cd5d5723313f459753663d0d155f047e19`, `layer_multi_cmds.rs` / `DocState::selected_layers`; inspected locally without running or copying the reference implementation.

## Exact evidence

Initial `94bc21be`: 42 passed / 1 failed / 0 ignored in 200.57 seconds, receipt `4f8e3fbe100d744ddfb3ea29ba5618ebc2d3f0435679e0645d47e0649b0a652a`. All Arrange/drop data cases, row/toggle tests, inherited hierarchy/selection/drag/clipping controls passed. The real native Shift-selected merge failed because the menu tested only pruned render-root membership. That failed cohort remains failed.

Corrected `acb2f57c`: all five directly affected tests passed, zero failed or ignored, in 21.83 seconds, receipt `52b41c8101829dc9fc5ef3d6d611fe1a33a64a5aeb551197ba812eaee86b5f69`. Coverage includes the failed native menu/range/history route, context owner behavior, and command/native partial-object final-row entry. Prior 42 passes were not repeated or presented as a combined single-head result.

Genuine current executable: build receipt `0c1a785ab86a2ddd03976b8ed4455716470689ce6e4b997b01d487576dd48bf8`, SHA-256 `c24f4130c2540c20c6c6544f5e247138f28fb35f4e1124029d64dbce86f7e911`. Nineteen saved cases emitted by passing `94bc` tests cover four native color/depth combinations × four Arrange operations and three subtree-drop placements. Their emission source remains `94bc`; protected producing bodies are byte-identical in `acb`.

Independent Node tree/data readers plus 38 current cold PNG/TIFF conversions verify exact original layer data, parent changes, full output samples outside the changed overlap and literal RGB/CMYKA values inside it. One derived workspace preview adds the 39th current CLI process. The first demo oracle sampled x170 as red, but the red rectangle ends at x170 exclusive; the reader now retains x170 as green and adds x160 as a red interior point. Original script/log/failure are retained, with all 39 successful outputs reused and no Rust replay.

QA: `dist/omadesign-layer-tree-qa-acb2f57c/run`. Issue #423 / PR #424. Durable packet: `implementation/craft-layer-tree-rows.json`. No GUI was launched and the existing human QA window was not touched.

Grouping explicitly selected empty groups still needs the object-derived grouping path adapted. Further command targets, saved selection, Gray/Lab, Actions, clipping interchange and the rest of the Craft parity ledger remain open. This layer does not close that goal.
