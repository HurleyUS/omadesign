# Editable SVG and artwork masks

Issue: https://github.com/michaelmonetized/omadesign/issues/210

Right-click a source in Layers, choose **Use as mask…**, then click its target. The target's menu selects the source, switches transparency/luminance interpretation, and releases the binding. Editing source geometry updates the target. Pixel selection and mask painting remain separate operations.

`editable.oma` is a small, editable luminance-mask example. `input.svg` is its source; `export.svg` preserves vector paths. `preview.png` is the native 4× render. Opening the native example requires project-version-13 support. The original accepted 0.5.0 QA evidence is unchanged.

## Software verification

- Nine focused regressions cover twelve SVG fixtures at 1×/4×, 25 source/target combinations, shared definitions, source edits and moves, undo/redo, rotated placement, selected-frame boundaries, and PSD/PDF/OpenRaster round trips.
- SVG references are rendered directly with resvg. Comparison limits are a maximum five-byte channel difference and a mean channel difference below 0.4, covering native rendering and exported/reimported SVG. Vector fixtures require zero pixel-mask buffers and zero embedded raster images.
- SVG and geometry follow-up suites passed 48 and 17 tests. Mask sources using SVG bounding-box units have document-sized nodes, and tiny cubic curves retain their geometry and precision.
- The compositor suite passed 68 tests; one existing timing benchmark stayed ignored. Effect distances now follow output scale. Cache checks retain their comparison with full-surface rendering.
- The built CLI completed SVG → OMA → SVG/PNG. `cli-receipt.json` records native version 13, one live mask, zero pixel masks, zero embedded raster images, and zero differing pixels against the resvg reference and SVG round trip. The independent reference uses the transformed, axis-aligned clip fixture; its 4× reference downsampled with box filtering matches the 1× render exactly.
- Agent context still excludes pixel-mask buffers and now describes artwork-mask references.

Raster interchange renders only affected layers/groups and reports the conversion. `.oma` and SVG retain editable sources. Lottie rejects unsupported live masks and points to animated SVG.

Library qualification accounts for all 1,045 cases: 1,036 passed across the broad run and follow-ups; nine existing cases remained ignored. No failures or untested cases remain in that accounting.

The full library run and isolated subprocess reruns are recorded in `qualification.json` and compressed logs. Rebuilding the test executable during the first run removed the pathname used by self-relaunching tests. Those cases were rerun against the final frozen executable; the original failures remain in the receipt.

## Reproduce

From the project checkout, keep temporary and build files on `/home`:

```sh
mkdir -p target/tmp
TMPDIR="$PWD/target/tmp" cargo test --lib artwork_mask -- --test-threads=4
TMPDIR="$PWD/target/tmp" cargo test --lib compositor:: -- --test-threads=4
TMPDIR="$PWD/target/tmp" cargo test --lib -- --test-threads=4
TMPDIR="$PWD/target/tmp" cargo build --bin omadesign
```

Use `--convert INPUT --output OUTPUT` and `--inspect FILE` for headless checks. The CLI currently has no `--help` handler.

This is local ARM64 software evidence. The installed app was not replaced. Native pointer/node editing has not received human QA for this change.
