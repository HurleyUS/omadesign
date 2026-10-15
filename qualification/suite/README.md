# Suite fixture qualification

The pinned candidate cohort is `index.json:candidate`. An affected PR selects only its
mapped fixtures. Every candidate runs the complete cohort. A future change to the
cohort is a reviewed source change, never a CLI exclusion or an aggregate floor.

The tiny SVG and PDF in this directory are original MIT fixtures authored from
the published format specifications. They contain only rectangles, clip/mask
geometry and colors; no fonts, profiles or client material. The PSD/PSB and ORA
inputs remain in their existing fixture directories with their original generator
and license notes. The small JSON oracles state independent known pixels and
layer facts. They are not renders captured from Omadesign. SVG appearance uses
librsvg/Cairo; PDF appearance uses Poppler, both at a fixed 72 DPI and transparent
or white backgrounds respectively. These are independent rendering paths; layered
fixture spec pixels are a mathematical oracle, not independent artist acceptance.

```
node --test scripts/suite/contract.test.mjs
node scripts/qualify-suite.mjs plan --lane affected --base origin/stack/craft-0.6.5
node scripts/qualify-suite.mjs plan --lane candidate
node scripts/qualify-suite.mjs run --lane candidate --config BUILD_CONFIG --builds LOCAL_BUILDS
node scripts/qualify-suite.mjs gate --lane candidate --builds LOCAL_BUILDS
```

Use `node scripts/suite/build.mjs CONFIG LOCKED_CARGO_RECEIPT /home/.../BUILD.json
[EXACT_ARTIFACT_SHA]` to consume the central helper's immutable compiler-artifact
receipt. It does not start Cargo or hash a possibly overwritten target later.
The suite checks the actual executable against that locked digest before use.
A build binding, including `--no-run`, is explicitly not fixture execution proof.

Cross-product adapter qualification uses one held global lock for the entire
build/bind/execute group:

```
node scripts/suite/serialized-adapters.mjs plan CONFIG omatainer FULL_SHA
node scripts/suite/serialized-adapters.mjs run CONFIG omatainer FULL_SHA /home/.../NEW_OUTPUT
node scripts/suite/serialized-adapters.mjs run CONFIG editbay FULL_SHA /home/.../NEW_OUTPUT
```

The recipes select Omatainer's binary test target and exact EditBay media/core
packages. They call the central helper with `--inside` under that existing lock,
consume compiler-emitted targets from its immutable receipt, and execute the real
adapters before releasing the same lock. There is no cache glob or later binary
hash used to reconstruct compilation. The internal path refuses execution without
an actual outer `flock` ancestor. This explicitly qualifies only the baseline
developer lane; the complete public candidate still includes UI/RAW/owner gates.

`--roots LOCAL_JSON` binds product IDs to local checkouts. `--builds LOCAL_JSON`
binds each product to `{revision,tree,binary,sha256,driver,tools}`; tools are keyed
by manifest tool ID and contain `{path,sha256,version}`. These files, inputs and
process logs belong in ignored project-local `.artifacts` directories. Do not
commit private bindings, local paths or licensed/client media. Source metadata
uses logical IDs and public provenance. All run scratch and temp variables stay
under the selected output directory, which must be under `/home`.

The runner measures actual committed source/tree and dirty status, executable,
Node, adapter, runner, tool, input/reference and output hashes. A build record must
name the current source/tree and match the executable bytes. Its compiler origin
is a build receipt, not something an adapter can establish. A Node-only adapter
explicitly records that it has no product executable. It cannot establish native
playback, device, display, physical or human proof.

Each fixture reports appearance, editable structure, undo, native save/reopen,
failure behavior and source preservation independently. Expected loss is reviewed
per fixture. A missing required output artifact, input or reference fails. Every
required fixture and tier must pass; improvements cannot offset a regression.
Native-window, device, physical and human evidence are named optional evidence
in this initial headless cohort. Their absence is unqualified, never a pass or a
claim of acceptance. Product owners retain their stricter native candidate gates.

Receipts are content-addressed immutable JSON alongside their hashed artifacts.
Old source/binary/fixture-contract passes remain historical. A later failure for
the same binding defeats an earlier pass. Missing/corrupt historical artifacts
are integrity errors; retain the evidence or move the entire historical receipt
and artifacts together into an explicitly separate archive. Never rewrite a
failure to a pass. The runner does not import adapter-provided source bindings.

Run operations share the canonical global build lock. Process groups have a
deadline and bounded output, and are killed when their leader exits or cancels.
This is software qualification only. Parent and process-tree RSS are sampled at
20 ms; short peaks may be missed. Allocation budgets and measured RSS are separate,
and this contract adds no benchmark or unmeasured resource-performance claim.

The complete candidate is the eleven baseline fixtures, one required UI case per
app, every required case in the pinned PHOTO-03 manifest, and the named original
and approved follow-on owner gates. CI runs Node integrity tests and writes an
affected or complete candidate **plan**. CI planning is not product qualification.
Actual execution and candidate decisions use local exact binaries and artifacts.

`--diff-product omatainer|editbay --roots LOCAL_JSON` selects changed paths from
that product's Git tree. Deleted and renamed paths are both considered. Unknown
mappings select the complete affected product lane. `--lane baseline --product
PRODUCT` is an explicit developer lane and cannot qualify a candidate.

PHOTO-03 is a pinned dependency, not a second corpus or camera-mode inventory.
`raw-camera` in the private roots map points to the integrated clean camera source.
`--raw-bindings`, `--raw-corpus` and `--raw-receipts` select existing local inputs and
per-case receipts. The import-safe upstream inspector streams actual source,
reference and external executable hashes without acquiring or decoding anything.
Every required comparison must have nonzero samples and match its representation,
reference/tool hashes and tolerances. Workflow math cannot replace independent
comparisons. Missing cameras/references remain unqualified. Updating the upstream
manifest requires a reviewed pin update; old receipts remain source-bound.

Each UI case requires separate app edit, undo, native save/reopen, keyboard, focus,
mixed, invalid, error, preservation and appearance checks. The source generator is
pinned; currently absent appearance/font/theme pins are explicit unavailable
dependencies. `builds[FIXTURE_ID].ui_receipts` maps semantic check IDs to compact
owner packets `{check,status,revision,binary_sha256,fixture_sha256,artifact:{path,
sha256,bytes}}`. The referenced artifact must exist and match. `ui_captures` maps
each scale to `{path,font_sha256,theme_sha256}`. All 100/150/200 percent captures
must exactly match their pinned PNG bytes and font/theme identity. This conservative
appearance check is independent of semantic proof and establishes no native-window,
physical or human acceptance. Existing accepted 0.5.0 history is not reused here.

`owner_gates` retains each product owner's stricter acceptance. `--owner-receipts`
points to packets named `GAP-ID.PRODUCT.json` containing `{version:1,id,product,
revision,tree,binary_sha256,status,checks:[{id,status,artifact:{path,sha256,bytes}}]}`.
Every named check, actual artifact and current executable/source must match. A
packet's `passed` means the owner asserts its complete acceptance, including its
stricter required native/product gates; a narrow partial test log is insufficient.
The shared baseline cannot imply all 25 features or approved follow-ons qualify.
Historical receipts, including native plugin and audio safety checks, stay useful
only at their original revisions/executables until the owner qualifies integration.
