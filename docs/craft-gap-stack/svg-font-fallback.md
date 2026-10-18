# SVG font fallback

Issue #395 corrects whole-chunk fallback replacement. Covered primary glyphs remain in their original face while missing graphemes use the ordered author families, then the existing system resolver. Rustybuzz retains original joining context and canonical composition. Real glyphs and original Unicode remain authoritative. Repeated resolver IDs terminate.

Native SVG source keeps its full family list rather than writing each computed glyph face into permanent character overrides. Subsequent typing can return to the primary family. Runtime field metrics still report the actual resolved face. The existing portable font namespace and archive collect unused secondary faces, stored component baselines and retained collaboration family lists. Library import copies the same dependencies, including withdrawn source. Font permissions and existing byte limits remain enforced.

The two existing SVG delivery policies remain separate. Embedded-font delivery writes actual face programs and complete family lists, including retained flow metadata. Original program family names follow all embedded identities for readers that ignore webfonts. Plain delivery uses a human installed family only after exact program-byte identity is established; unavailable portable faces retain the existing outlined delivery policy. Explicit outlines still outline.

Tests use the original OFL EBGaramond fixture and installed Liberation/Noto programs. Derived cmap fixtures retain all real outline/shaping tables and recalculate SFNT checksums; they test canonical composition and Unicode variation mappings without fabricated glyphs. Non-presentation variation selectors do not change family selection, but the selected font can apply its own cmap variation. U+FE0E/U+FE0F use the presentation matching route. Reference: https://www.w3.org/TR/css-fonts-4/#font-variant-emoji-prop

Native files and portable SVG must reopen with identical pixels. Independent Chromium comparisons use both original source and emitted SVG with the unchanged pixel threshold. Frozen failed receipts and browser outputs remain in the source handoff; they are not promoted.

This cut does not complete character scale, optical kerning, cross-chunk textLength or the remaining non-flow leading work in #391. It does not claim human or physical-device acceptance.

Plain installed-font delivery records the current resolved face in an export-only clone so a fallback Medium face is not accidentally reopened as the same family's Regular face. This preserves the existing installed-face delivery policy; it does not freeze the native author source. Native and embedded-font delivery keep full author family lists. Library copy and save/recovery now share the same bounded logical-cell font visitor, so withdrawn property-path writes are not missed by a second key-name scanner.

## Qualified development build

Production source is `9f7b2b5c5a63b1735a00d279b09c49f8df6a841c`. The final affected group passed 11/11; the broader 66 passes remain recorded at `6e92c0a9` with its four failures separately retained and corrected. This is not a single-source 70/70 claim. The real current executable then passed all 19 packaged native cold opens plus seven original-source renders in 45 fresh CLI processes. Fourteen fresh Chromium comparisons against current native renders passed; the maximum mean error was 0.771277 against the unchanged 1.5 limit. Original emitted SVG identities remain pinned to `6e92c0a9`.

Private QA launcher: `dist/omadesign-svg-fallback-qa-9f7b2b5c/run`. The existing human QA window was left untouched. The new bundle uses the native savewriter and contains its actual font sidecar and licenses. The earlier pair/leading QA folder also received its missing font sidecar and passed twelve real cold opens without changing its executable or documents.

Durable handoff: `/home/michael/Projects/omadesign/.artifacts/craft-gap-stack/implementation/craft-svg-font-fallback.json`, SHA-256 `95e45dfe03e54a0ac0529470d92d5246f9d47316021a25ea07443b20dc0d079f`. Its 336-file manifest preserves all failed/intermediate receipts and final proofs. Full parity remains open.
