# SVG font fallback

Issue #395 corrects whole-chunk fallback replacement. Covered primary glyphs remain in their original face while missing graphemes use the ordered author families, then the existing system resolver. Rustybuzz retains original joining context and canonical composition. Real glyphs and original Unicode remain authoritative. Repeated resolver IDs terminate.

Native SVG source keeps its full family list rather than writing each computed glyph face into permanent character overrides. Subsequent typing can return to the primary family. Runtime field metrics still report the actual resolved face. The existing portable font namespace and archive collect unused secondary faces, stored component baselines and retained collaboration family lists. Library import copies the same dependencies, including withdrawn source. Font permissions and existing byte limits remain enforced.

The two existing SVG delivery policies remain separate. Embedded-font delivery writes actual face programs and complete family lists, including retained flow metadata. Original program family names follow all embedded identities for readers that ignore webfonts. Plain delivery uses a human installed family only after exact program-byte identity is established; unavailable portable faces retain the existing outlined delivery policy. Explicit outlines still outline.

Tests use the original OFL EBGaramond fixture and installed Liberation/Noto programs. Derived cmap fixtures retain all real outline/shaping tables and recalculate SFNT checksums; they test canonical composition and Unicode variation mappings without fabricated glyphs. Non-presentation variation selectors do not change family selection, but the selected font can apply its own cmap variation. U+FE0E/U+FE0F use the presentation matching route. Reference: https://www.w3.org/TR/css-fonts-4/#font-variant-emoji-prop

Native files and portable SVG must reopen with identical pixels. Independent Chromium comparisons use both original source and emitted SVG with the unchanged pixel threshold. Frozen failed receipts and browser outputs remain in the source handoff; they are not promoted.

This cut does not complete character scale, optical kerning, cross-chunk textLength or the remaining non-flow leading work in #391. It does not claim human or physical-device acceptance.
