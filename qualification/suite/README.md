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
This is software qualification only. Memory readings, if absent, remain explicit
unknowns; this contract adds no unmeasured resource-performance claim.
