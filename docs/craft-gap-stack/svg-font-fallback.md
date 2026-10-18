# SVG font fallback

Issue #395 corrects whole-chunk fallback replacement. Covered primary glyphs remain in their original face while missing graphemes use the ordered author families, then the existing system resolver. Rustybuzz retains original joining context; variation sequences and canonical composition use the actual font programs. Unknown or repeatedly returned fallback faces terminate through the existing missing-glyph route.

Native SVG source keeps its full family list rather than writing each computed glyph face into permanent character overrides. Subsequent typing can return to the primary family. The existing portable font namespace and archive collect unused secondary faces, stored component baselines, and retained collaboration family lists. SVG export embeds those actual faces, including retained flow source. Font permissions and existing byte limits stay enforced.

Tests use the original OFL EBGaramond fixture and installed Liberation/Noto programs. Derived cmap fixtures retain all real outline/shaping tables and recalculate SFNT checksums; they test canonical composition and Unicode variation mappings without fabricated glyphs. Native files and portable SVG must reopen with identical pixels. Independent Chromium comparisons use both the original source and emitted SVG; their existing pixel threshold remains unchanged.

This cut does not complete character scale, optical kerning, cross-chunk textLength, or the remaining non-flow leading work in #391. It does not claim human or physical-device acceptance.

The first actual Chromium run exposed an incorrect non-presentation variation fallback. CSS Fonts 4 states that selectors other than U+FE0E/U+FE0F do not change family selection; the selected face can still apply its own cmap variation. Both covered-primary and missing-base/selected-secondary cases now have separate original-source pixel checks. Original failed output and the 91849203 61-pass/8-fail/2-ignored receipt remain retained. The broader run also caught default-family bold inheritance and the legacy non-embedded writer outlining newly portable source; capture now preserves the original requested default family before rebinding, and the existing embedded writer handles portable rich source without dropping editability.

Reference: https://www.w3.org/TR/css-fonts-4/#font-variant-emoji-prop
