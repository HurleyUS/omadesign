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
Required compiler records include strict helper/module/config SHA-256 identities and
the original and actual structured Cargo arguments, with `--locked` before any test
argument separator. Missing identities or mismatched executed arguments fail.

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

The reviewed `b3041919` dependency now consumes exactly `cr3-canon-m50-raw`:

```
node scripts/qualify-suite.mjs raw-evidence --raw-producer-bindings /home/.../m50-suite-bindings-b3041919.json
node scripts/qualify-suite.mjs gate --lane candidate --raw-producer-bindings /home/.../m50-suite-bindings-b3041919.json --out /home/.../NEW_GATE
```

The private producer binding supplies the existing producer checkout, packet,
case bindings and original compiler/receipt locations. No corpus or outputs are
copied. The consumer verifies the exact committed tree and eleven source blobs,
including inspector blob `9ce83d61` (31,380 bytes, SHA `2a87c7b4`). It imports only
the verified Node inspector's read-only validation functions. It never invokes
`inspectCase`, which would open tool executables, or any qualification runner.
The reviewed diff from `870ef7a3` changes aggregation and the helper path; inspector
case/manifest validation is unchanged. Only the M50 manifest case changed.

Actual RAW input and reference pixel data stream through a 512 KiB buffer. SHA,
length, regular-file identity and before/after stat checks must match; sensor FNV
is over each little-endian byte, including masked borders. `62261f0ba81cfcd2` is
attributed to the independent reference recipe, not the native qualification code.
Current data checks are distinct from frozen coordinator observations. Missing,
changed or truncated data cannot pass using an old observation.

The original `c9de158a` and workflow `0c73d1c2` compiler records bind the same
`b115c236` test executable, exact clean producer source, Cargo arguments and
helper hashes. The consumer checks these records, the actual logged native packet,
one executed test, strict selected/checked summary, seven workflow checks, required
nonzero comparison samples and the pinned producer host hash. The host contract is
SHA-256 of nonempty trimmed hostname bytes. No product, decoder or reference-tool
executable is opened, posthashed, replayed or executed. Later checkout/head/dirty
observations are reported separately and never relabel the original binary.

Both commands exit 1 while 79 required cases remain unqualified. A valid import
reports one exact M50 case, not an 80-case pass. DNGLab independently proves masked
sensor values; RGB normalization uses LibRaw integration with the same decoder
implementation. Firmware is unknown. Other modes/cameras, RAW document placement,
100 MP, native-window/device/physical/human proof remain unqualified. Process VmHWM
includes the lifetime of reference/workflow work and is not decoder-only peak RSS.
The native cancellation 283.503 ms / 25-check proof is separate from this case.
The previous `870ef7a3` contract and `60eab7da` observations remain in history.

Each UI case requires separate app edit, undo, native save/reopen, keyboard, focus,
mixed, invalid, error, preservation and appearance checks. The source generator is
pinned; currently absent appearance/font/theme pins are explicit unavailable
dependencies. `builds[FIXTURE_ID].ui_receipts` maps semantic check IDs to compact
owner packets `{check,status,revision,binary_sha256,fixture_sha256,host_sha256,artifact:{path,
sha256,bytes}}`. The referenced artifact must exist and match. `ui_captures` maps
each scale to `{path,revision,binary_sha256,fixture_sha256,font_sha256,theme_sha256,
font_path,theme_path,host_sha256}`. Captures must bind the current app source,
executable and fixture. The runner independently hashes the actual nonempty font
and theme dependency bytes; matching claimed hashes alone cannot pass. Private
dependency paths and licensed font content stay local.
All 100/150/200 percent captures
must exactly match their pinned PNG bytes and font/theme identity. This conservative
appearance check is independent of semantic proof and establishes no native-window,
physical or human acceptance. Existing accepted 0.5.0 history is not reused here.

`owner_gates` retains each product owner's stricter acceptance. `--owner-receipts`
points to packets named `GAP-ID.PRODUCT.json` containing `{version:1,id,product,
revision,tree,binary_sha256,host_sha256,status,checks:[{id,status,artifact:{path,sha256,bytes}}]}`.
Every named check, actual artifact and current executable/source must match. A
packet's `passed` means the owner asserts its complete acceptance, including its
stricter required native/product gates; a narrow partial test log is insufficient.
The shared baseline cannot imply all 25 features or approved follow-ons qualify.
Historical receipts, including native plugin and audio safety checks, stay useful
only at their original revisions/executables until the owner qualifies integration.

Read reviewed partial owner evidence without launching any product:

```
node scripts/qualify-suite.mjs evidence --producer-roots /home/.../PRIVATE_ROOTS.json
```

`owner-sources.json` pins the original AUDIO04, stereo routing and video packets,
immutable compiler records, logs, authored native inputs, outputs, references and
inspectors. Reviewed JSON packets are retained by content hash locally; later edits
to a producer's handoff do not rewrite these observations. New packet pins require
review. The private roots map supplies `evidence` (the craft artifact root),
`video` (the producer receipt directory), `system` (`/usr`), `omatainer` and
`editbay` (existing Git checkouts containing the pinned commits). It creates no
corpus, exports or binary copies. JSON output contains logical evidence IDs and
hashes; private bindings and packet contents remain local.

This command verifies retained bytes, committed Git trees, original clean compiler
records, actual recorded invocations/counts, helper/module/queue/config bytes,
source generators, native input manifests and reference tool/font dependencies.
It reads no shared product target. Compiler-captured binary identities are original
records, never a hash taken after another job could overwrite the target. Test
counts exclude nested fresh-process summaries; editorial and caption routes share
one generator group each. Clippy has no fixture-execution credit. Inspection binds
its own Node executable, host hash, runner Git source and current checkout dirtiness.

`integrity_passed` means these reviewed records verified. It does **not** qualify a
candidate. Missing or corrupt records leave named cases/routes unavailable and
exit nonzero. Generated AUDIO04 input bytes that were not retained stay unavailable;
their recorded hashes are not present-byte verification. The abrupt-edge result
stays `characterized`, with certified true-peak ceiling false. The 7cf/d72 routing
jobs remain failed despite specific passing cases. The 3f8 correction proves only
its four UI checks, including when the owner packet names a later candidate.
Editorial6 and captions34 tests retain their exact original sources/executables.
Retained SRT/VTT timing/Unicode and numeric oracles do not establish independent
FFmpeg codec conformance. Original editorial/ffprobe inspection tool hashes absent
from their records and original compiler-host identity remain explicit gaps.

Pass `--producer-roots` to `gate` to attach the same inspection. A failed evidence
import also fails that gate; a valid partial import cannot satisfy or replace any
required owner acceptance check. All 80 RAW cases remain required (79 unavailable
after the reviewed M50 pin), along with UI appearance/common fixture/font/theme,
complete current caption delivery, scopes/multicam/viewer acceptance, stereo
full-cohort/surround and stricter product gates remain required and unavailable.
The eleven baseline fixtures/52 checks remain historical at Omadesign123dd7c5,
Omatainer23d0b3ee and EditBay29238c2. Future execution still requires actual binary
SHA verification immediately before launch under the global lock.

The bounded video reader consumes e7 scope-worker and 7f native-delivery records:

```
node scripts/qualify-suite.mjs video-evidence --producer-roots PRIVATE_ROOTS.json
```

It uses only `evidence`, `video` and `editbay` roots from the map above. It reads
pinned JSON, logs, helper source, Git objects and retained fixture/font/master
data, bounded to 8 MiB per asset with before/after file checks. It never opens
product/decoder/tool executables or replays tests. Node version and current
inspection host/source are recorded; its executable hash is deliberately null.
Missing source/helper/input/reference/artifact, changed executed arguments and
zero or changed test counts make required routes unavailable and exit nonzero.

Numeric scopes14 and native viewer1 remain separate generators at e7. The viewer
records eleven scope reports, four offscreen completions, paused/prepared refresh,
stale serial/owner rejection and worker join. Multicam core12 and app10 (one
ignored scope test) at 7f serve two routes through one group. One separate native
delivery test produced four masters, each with ten frames and 20,000 mono samples;
caption pixels, retained VTT, program scopes and preservation assertions belong to
that same execution. Four masters do not become four executed tests. Original
editorial9a and caption2d observations retain their sources; stale route prose
does not supersede the qualified caption packet.

Compiler receipts do not embed runtime packets. Original source assertions,
actual execution logs, separately pinned producer/coordinator packets and current
data hashes form the retained join. Producer CLI hashes join separate CLI build
records; app-test identity joins the original compiler/log. Native project and
numeric reference hashes absent from producer fields remain coordinator
observations. Original output-time observations are preserved in those packets,
not refreshed execution. The separate ffprobe inspection records its executable
hash but lacks version/full invocation. It shares FFmpeg and cannot establish
independent codec conformance. Numeric decoder oracles remain original results;
this reader only checks retained bytes and report arithmetic. No window, sound
clock, HDR display, device, physical or human acceptance follows.

Every imported route still has `current_candidate_status: unavailable` and
`public_candidate: false`. VIDEO08 d709 and phase2 audio761b remain unavailable.
The 055 M50 pins/one-camera qualification and 79 unavailable camera cases, frozen
60e audio evidence and historical eleven baseline fixtures/52 checks are unchanged.
