# Contributor review — October 13, 2026

Fred Nix submitted PRs #202–#205 on October 12. There were no open issues when this review began.

PRs #202, #203, and #204 apply together to master `f969a0b2`. Their combined library suite passed **1,014 tests**, with **9 ignored**, in an isolated checkout and isolated user directories. This checks code behavior; it does not establish native UI or physical-device acceptance.

## #202 — file loading and PDF export

**Integrated with the correction in #207.** `Document::ensure_ids` now calls `migrate_artboards` before advancing the ID allocator past the IDs loaded from disk. Migrating an old artboard with ID zero can reuse a loaded object's ID.

A fresh-process regression produced an artboard ID and shape ID both equal to `6`. The test failed on the submitted changes. Moving migration after validation and `bump_id(max)` passes the collision regression and the contributor's extreme-ID rejection regression.

The correction is in [pr-202-id-order-correction.patch](pr-202-id-order-correction.patch). Apply it on top of #202. The exact regression used a normal document, one rectangle, an existing artboard whose ID was set to zero, and a rectangle ID assigned to the allocator's next value; `ensure_ids()` must allocate a different artboard ID.

## #203 — geometry, painting, effects and animated SVG

No blocking finding in the submitted changes. The supplied pixel, winding, open-path, and SVG regressions pass in the combined suite. The PR explicitly leaves similar compound-contour handling in motion/Lottie unchanged; those routes were not qualified by this review.

## #204 — cloud identity and photo presets

No blocking finding. Failed identity writes remove only the temporary file created by that attempt. Preset collision matching now uses the same trimmed Unicode-lowercase names as validation. Both supplied regressions pass.

## #205 — README banner

Superseded by the current media refresh in #207. The image changes the official wordmark, introduces “The creative studio Linux never had,” and uses older Design/Pixel/Photo/Type screenshots. Use the official mark, established site copy, and current screenshots before adding it to the README.

## Hosted checks

All four PRs report a Vercel authorization failure for the contributor. That is not a native test result. CodeRabbit reports success. The three code histories are integrated into #207 with original authorship. #205 is superseded by current canonical branding and native media.
