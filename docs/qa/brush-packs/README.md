# Brush-pack qualification

The source-drive scan reached m1Pro13 over SSH and inspected its five mounted
external volumes. HDD320 contains no remaining user files. The recovered art
folder's brush directory is empty. Existing recovery catalogs and a targeted
read-only scan found JaxGrass, the 20 Grass set, Rodeo, and a tire-brush EPS archive
in the recovered MyBook documents. Files were copied; originals were not changed.
The broad Shared/Music search was stopped after the sources were located, so this
is not a claim that every file inside every disk image or archive was examined.
One historical compressed catalog was truncated; live source reads succeeded.

Public assets are original Omadesign CC0 tips. Rodeo's creator prohibits resource
redistribution, including derivatives. The grass sources list Brusheezy Community
and Standard licenses, rather than MIT or CC BY-SA. Converted copies are kept
private and excluded from Git. Their published source terms:

- https://thevectorlab.com/products/rodeo-rope-brush
- https://www.brusheezy.com/brushes/12625-jax-grass-brushes
- https://www.brusheezy.com/brushes/62805-20-grass-texture-ps-brushes-abr-vol-1

Focused local qualification uses:

```sh
OMADESIGN_BRUSH_FIXTURES=/absolute/private/source-directory \
  cargo test --lib brush -- --include-ignored --nocapture
cargo run --bin brush_qa -- /absolute/private/source-directory/jaxgrass.abr \
  "$PWD/artifacts/brush-packs/native"
```

The real-source test is explicitly ignored unless requested and supplied with
private source files. It decodes every supported tip, paints it, and checks exact
pack roundtrips. Source coverage: ABR v6.2, ABR v10.2, zlib Illustrator private
brush patterns, Zstandard Illustrator private patterns, and legacy PostScript.
Photoshop-specific dynamics and editable vector brush behavior are not claimed.

Public previews are actual strokes from the native painter:

```sh
node scripts/generate-brush-packs.mjs
cargo run --bin brush_preview -- assets/brushes/essentials.omabrush \
  site/public/brushes/essentials.svg
cargo run --bin brush_preview -- assets/brushes/texture.omabrush \
  site/public/brushes/texture.svg
```

Native WGPU run passed with real egui pointer events: 7,178 painted pixels,
exact undo/redo, exact save/reopen, and three packs after reloading the isolated
profile. `native-result.json` and `native-library.png` preserve that receipt.
The library preview cache avoids nested context locks, and only visible rows
allocate textures. The screenshot contains original Omadesign tips.

The website passed local typecheck and production build. At 1440px and 390px,
previews loaded and horizontal overflow was zero. A real browser download of
Essentials completed (74,654 bytes), matched the bundled pack byte for byte, and
imported/exported with identical names, settings, and alpha samples. JSON number
formatting is canonicalized on export. The staged production route and hosted
pack were checked through authenticated `vercel curl`.

The first complete desktop run returned 989 passes, eight failures, and nine
intentional ignores. Seven failures were Unix socket paths exceeding SUN_LEN in
an overly long project-local TMPDIR. The eighth required updating license
inventory for the new Zstandard dependencies. Final checks use the shorter
project-local `/home/michael/Projects/omadesign/.tmp-brush` scratch directory.

Final complete desktop run: **998 passed, 0 failed, 9 intentional ignores**.
The separate private-source run passed all five supplied ABR/AI libraries. The
EPS tire artwork also converted successfully through the existing Ghostscript
reader. Converted external tip counts are JaxGrass 6, Grass 20, Rodeo 8, Tire 1.
Additional installed Adobe fixtures qualified 428 Photoshop presets (sampled and
computed; unsupported tool/bristle presets are reported) and 19 Illustrator tips.

The website is live at https://omadesign.app/brushes on production deployment
`dpl_zK1uhWBkpw3DFVoMpjuQCdy3n54m`. At that initial website publication, desktop stable was v0.6.3 and packs required
the feature build. The first brush-pack desktop nightly is documented in
[0.6.3-nightly.20261011.0001](../../releases/0.6.3-nightly.20261011.0001.md). The original accepted 0.5.0 evidence and
installed public desktop build were preserved.
