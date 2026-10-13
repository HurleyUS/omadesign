# Current product media

All active product screenshots and recordings were refreshed on October 13, 2026 from the PR #207 candidate. Captures use the actual native WGPU app on an isolated Wayland display. They show current UI, not pre-product mockups. Artwork exports are separate from screenshots.

The editable Fieldwork poster, type study, thirteen-tip brush sheet, and pulse animation live in [examples/product-media](../examples/product-media/README.md). Shapes and text remain editable; photograph and brush samples are raster layers. Fonts use Liberation Sans. The responsive Layout example remains an editable native document.

The official O mark and wordmark come from `assets/omadesign-mark.svg` and `assets/omadesign-wordmark.svg`. The wide lockup changes only their placement and framing. Path geometry and letterforms are unchanged. The O uses the approved green from the full lockup; the wordmark adapts for light backgrounds. Site assets use stable names under `public/media/brand/`.

The photograph is the **CC0-1.0** Fujifilm X-T30 II sample from [raw.pixls.us](https://raw.pixls.us/getfile.php/5064/nice/Fujifilm%20-%20X-T30%20II%20-%2014bit%2014bit%20compressed%20%283%3A2%29.RAF), SHA-256 `5e2678038377cc7f146ceb7693c08535fa1af162cec705d06b0446c3d3017d47`. Omadesign decodes the untouched camera original; the media project uses a reduced JPEG derived from that decode. No private photograph is published.

The 40-second silent product film uses fresh native UI replays with actual pointer and key input. It shows design, responsive layout, gradients, brushes, chroma key, photo adjustments, motion, portable libraries, and composing an Agent brief. The Agent clip stops before sending: it makes no claim about a provider response. The homepage film and carousel keep their separate interactions, explicit playback, reduced-motion behavior, and offscreen pausing.

[The media manifest](public/media/manifest.json) records the exact capture binary, source and output hashes, clip duration, renderer, and unresolved input targets. Historical release QA evidence under `docs/qa/` remains unchanged. Superseded public marks, mocks, and clips are preserved in Git history.

To regenerate, build `omadesign`, `capture_studios`, and the `product_media` example. Set `OMADESIGN_BIN`, `OMADESIGN_RECORDER`, `XDG_RUNTIME_DIR`, and `WAYLAND_DISPLAY` to the built tools and an isolated display under /home, then run `node scripts/refresh-product-media.mjs`. Build outputs, temporary profiles, and raw takes stay in `artifacts/media-refresh/`.
