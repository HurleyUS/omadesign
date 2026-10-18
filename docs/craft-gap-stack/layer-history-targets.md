# Layer selection targets in Undo history

Undo after a merge or stamp restores the original layer rows, active layer and Shift-selection anchor. Redo restores the accepted result row. Clicking another layer between those actions does not replace the historical target.

The metadata is stored with the actual in-memory command entry. Undo and Redo move the entry and its target together. Clear, the 200-entry limit, redo branching, clone, per-document tab history, staging and native field grouping retain their existing command behavior. The History pane iterates command references from those entries without cloning artwork.

Targets contain stable document/layer/object IDs, not renderer resources or widget focus. Restoration happens after the inverse/forward command and only for the owning document. The initial consumer is local layer merge/stamp. Other edit consumers, Live replica selection history and serialized reopening of an active selection remain separate work. Existing command-only History Undo/Redo callers keep their API.

No second history, document serialization, command/pixel wire, model/schema, reader gate, resource limit, compositor, dependency, app version or release change. PhotoCraft's edit/selection behavior was inspected from the existing local reference; no reference code was copied.

## Exact focused results

- `41f37940`: compilation refused the new index closure's missing `usize` annotation; no tests ran. Receipt `84cde4ae9826f9aa1e6320bcbb82ea9166d4e979954a3aec980d9911e78a068d` remains a compile failure.
- Production `67433362`: 26 passed / 1 failed / 0 ignored, 83.00 seconds. Native merge/stamp menu/click/Undo/Redo, tab ownership, canonical history limits/branch/clear/staging, Actions rollback and size-field checks passed. The new anchor test incorrectly expected the last-clicked row rather than the original first-clicked anchor. Receipt `e31e92705f31d2dba2123a463508e177b116bde037638563e88eb60b3b7b866d` remains a failed cohort.
- Test-only `18f31416`: the corrected range/group/stamp test and two existing object opacity tests passed; the legacy layer-opacity fixture failed before editing because it called the layer-list renderer instead of Layer settings. Result 3/1/0, 19.13 seconds; receipt `0044b8f909d98229b05c6a8bf7bb615e44304aadfb06e57193e5a8dbf97f7efb` remains failed.
- Test-only `5c50cfad`: the failed layer-opacity route and its other Layer-settings fixture consumer passed 2/0/0 in 12.11 seconds after the one-line call correction. Receipt `6b0dbd12f781739e6c84fe69b609f6625925c79202830413780bf0ce965c075b`. All drag/history assertions remained. Production bytes are identical to `67433362`; there is no single-head aggregate test receipt.

The original cross-group source `1ff355b6` retains its separate 31/0/0 result, genuine desktop receipt and 21-case / 43-process independent cold output proof. Render/export/source pixels are unchanged by this history layer. No previous proof is relabeled current.

Issue #421 / stacked PR #422. Durable handoff: `implementation/craft-layer-history-targets.json`. The agent does not launch the QA window; no existing desktop session is changed.

The genuine current executable built at `5c50cfad8b313b3a8def23014c82480275837be5`, receipt `af21daa086db165718afe15a7c72e508d6eb489d7ec7c850cfda404568fc9e12`. Binary SHA-256: `e6be16ae2253af10e9ef61b06ee8ea6b192362d705aa0c9427817c236a26e109`. One current cold native-document conversion matched the prior workspace PNG pixels exactly; the earlier21-case43-process proof stays pinned to its original source.

QA launcher: `dist/omadesign-layer-history-qa-5c50cfad/run`. No GUI launched.
