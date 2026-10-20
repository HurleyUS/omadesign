// Copyright 2022 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::*;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

struct Shaper<'a> {
    text: &'a str,
    span: &'a TextSpan,
    spans: &'a [TextSpan],
}

impl Shaper<'_> {
    /// Shape one font run without discarding its original Unicode context.
    /// Args: selected face, original byte range, direction and font database. Returns real glyphs.
    fn piece(
        &self,
        font: Arc<ResolvedFont>,
        range: Range<usize>,
        ltr: bool,
        fontdb: &Database,
    ) -> Vec<Glyph> {
        let text = self.text;
        let variations = &self.span.font.variations;
        let font_size = self.span.font_size.get();
        let font_optical_sizing = self.span.font_optical_sizing;
        if range.is_empty() {
            return Vec::new();
        }
        fontdb
            .with_face_data(font.id, |font_data, face_index| -> Option<Vec<Glyph>> {
                let mut rb_font = rustybuzz::Face::from_slice(font_data, face_index)?;

                // Build the list of variations to apply
                let mut final_variations: Vec<rustybuzz::Variation> = variations
                    .iter()
                    .map(|v| rustybuzz::Variation {
                        tag: Tag::from_bytes(&v.tag),
                        value: v.value,
                    })
                    .collect();

                // Automatic optical sizing: if font-optical-sizing is auto and the font has
                // an 'opsz' axis that isn't explicitly set, auto-set it to match font size.
                // This matches browser behavior (CSS font-optical-sizing: auto).
                if font_optical_sizing == crate::FontOpticalSizing::Auto {
                    let has_explicit_opsz = variations.iter().any(|v| v.tag == *b"opsz");
                    if !has_explicit_opsz {
                        // Check if font has opsz axis using the already parsed rb_font
                        if let Some(axes) = rb_font.tables().fvar {
                            let has_opsz_axis = axes
                                .axes
                                .into_iter()
                                .any(|axis| axis.tag == ttf_parser::Tag::from_bytes(b"opsz"));
                            if has_opsz_axis {
                                final_variations.push(rustybuzz::Variation {
                                    tag: Tag::from_bytes(b"opsz"),
                                    value: font_size,
                                });
                            }
                        }
                    }
                }

                // Apply font variations for variable fonts
                if !final_variations.is_empty() {
                    rb_font.set_variations(&final_variations);
                }

                let mut glyphs = Vec::new();
                let sub_text = &text[range.clone()];
                let hb_direction = if ltr {
                    rustybuzz::Direction::LeftToRight
                } else {
                    rustybuzz::Direction::RightToLeft
                };

                let mut buffer = rustybuzz::UnicodeBuffer::new();
                buffer.push_str(sub_text);
                buffer.set_pre_context(&text[..range.start]);
                buffer.set_post_context(&text[range.end..]);
                let mut cursor =
                    unicode_segmentation::GraphemeCursor::new(range.start, text.len(), true);
                if cursor.is_boundary(text, 0) == Ok(false) {
                    buffer.set_flags(rustybuzz::BufferFlags::DO_NOT_INSERT_DOTTED_CIRCLE);
                }
                buffer.set_direction(hb_direction);

                let mut features = Vec::new();
                for span in self.spans {
                    let start = span.start.max(range.start);
                    let end = span.end.min(range.end);
                    if start >= end {
                        continue;
                    }
                    for (tag, value) in [
                        (*b"smcp", u32::from(span.small_caps)),
                        (*b"kern", u32::from(span.apply_kerning)),
                    ]
                    .into_iter()
                    .chain(span.font.features.iter().copied())
                    {
                        features.push(rustybuzz::Feature {
                            tag: Tag::from_bytes(&tag),
                            value,
                            start: (start - range.start) as u32,
                            end: (end - range.start) as u32,
                        });
                    }
                }
                let output = rustybuzz::shape(&rb_font, &features, buffer);

                let positions = output.glyph_positions();
                let infos = output.glyph_infos();
                let mut boundaries: Vec<_> = infos.iter().map(|info| info.cluster as usize).collect();
                boundaries.push(sub_text.len());
                boundaries.sort_unstable();
                boundaries.dedup();

                for i in 0..output.len() {
                    let pos = positions[i];
                    let info = infos[i];
                    let idx = range.start + info.cluster as usize;

                    let start = info.cluster as usize;

                    let repeated = if ltr {
                        i.checked_add(1)
                    } else {
                        i.checked_sub(1)
                    }
                    .and_then(|last| infos.get(last))
                    .is_some_and(|info| info.cluster as usize == start);
                    let end = if repeated {
                        start
                    } else {
                        boundaries[boundaries.binary_search(&start).unwrap() + 1]
                    };

                    glyphs.push(Glyph {
                        byte_idx: ByteIndex::new(idx),
                        cluster_len: end - start,
                        text: sub_text[start..end].to_string(),
                        id: GlyphId(info.glyph_id as u16),
                        dx: pos.x_offset,
                        dy: pos.y_offset,
                        width: pos.x_advance,
                        font: font.clone(),
                    });
                }

                Some(glyphs)
            })
            .flatten()
            .unwrap_or_default()
    }
}

#[derive(Clone)]
struct Run {
    bytes: Range<usize>,
    font: Arc<ResolvedFont>,
}

struct Choice {
    font: Arc<ResolvedFont>,
    named: bool,
}

fn selectors(text: &str) -> Vec<(char, char)> {
    let chars: Vec<_> = text.chars().collect();
    chars
        .windows(2)
        .filter_map(|p| {
            (matches!(p[1], '\u{fe0e}' | '\u{fe0f}')
                && !matches!(p[0], '\u{fe00}'..='\u{fe0f}' | '\u{e0100}'..='\u{e01ef}'))
            .then_some((p[0], p[1]))
        })
        .collect()
}

fn scripts(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = range.start;
    let mut previous = None;
    for (byte, cluster) in text[range.clone()].grapheme_indices(true) {
        let script = cluster.chars().map(|c| c.script()).find(|s| {
            !matches!(
                s,
                unicode_script::Script::Common | unicode_script::Script::Inherited
            )
        });
        if let Some(script) = script {
            let at = range.start + byte;
            if previous.is_some_and(|p| p != script) && start < at {
                out.push(start..at);
                start = at;
            }
            previous = Some(script);
        }
    }
    if start < range.end {
        out.push(start..range.end);
    }
    out
}

impl Shaper<'_> {
    fn visual(&self, runs: &[Run], bidi: &unicode_bidi::BidiInfo<'_>, db: &Database) -> Vec<Glyph> {
        let mut glyphs = Vec::new();
        for paragraph in &bidi.paragraphs {
            let (_, order) = bidi.visual_runs(paragraph, paragraph.range.clone());
            for visual in order {
                let ltr = bidi.levels[visual.start].is_ltr();
                let mut pieces = Vec::new();
                for run in runs {
                    let start = run.bytes.start.max(visual.start);
                    let end = run.bytes.end.min(visual.end);
                    if start < end {
                        for bytes in scripts(self.text, start..end) {
                            pieces.push((bytes, run.font.clone()));
                        }
                    }
                }
                if !ltr {
                    pieces.reverse();
                }
                for (bytes, font) in pieces {
                    glyphs.extend(self.piece(font, bytes, ltr, db));
                }
            }
        }
        glyphs
    }

    fn supported(
        &self,
        font: Arc<ResolvedFont>,
        range: Range<usize>,
        ltr: bool,
        variants: bool,
        db: &Database,
    ) -> bool {
        if variants {
            let pairs = selectors(&self.text[range.clone()]);
            if !db
                .with_face_data(font.id, |bytes, index| {
                    ttf_parser::Face::parse(bytes, index).is_ok_and(|f| {
                        pairs.iter().all(|&(base, selector)| {
                            f.glyph_variation_index(base, selector)
                                .is_some_and(|g| g.0 != 0)
                        })
                    })
                })
                .unwrap_or(false)
            {
                return false;
            }
        }
        let glyphs = self.piece(font, range, ltr, db);
        !glyphs.is_empty() && glyphs.iter().all(|g| !g.is_missing())
    }

    fn choose(
        &self,
        range: Range<usize>,
        ltr: bool,
        variants: bool,
        listed: &[Choice],
        primary: &Arc<ResolvedFont>,
        resolver: &FontResolver,
        db: &mut Arc<Database>,
    ) -> Option<Arc<ResolvedFont>> {
        let source = &self.text[range.clone()];
        let private=source.chars().any(|c|matches!(c,'\u{e000}'..='\u{f8ff}'|'\u{f0000}'..='\u{ffffd}'|'\u{100000}'..='\u{10fffd}'));
        let mut tried = vec![primary.id];
        for choice in listed {
            if !tried.contains(&choice.font.id) {
                tried.push(choice.font.id);
            }
            if private && !choice.named {
                continue;
            }
            if self.supported(choice.font.clone(), range.clone(), ltr, variants, db) {
                return Some(choice.font.clone());
            }
        }
        if private {
            return None;
        }
        let c = if variants {
            selectors(source).first().map(|p| p.0)
        } else {
            None
        }
        .or_else(|| {
            source.chars().find(|&c| {
                !matches!(c,'\u{200c}'|'\u{200d}'|'\u{fe00}'..='\u{fe0f}'|'\u{e0100}'..='\u{e01ef}')
                    && !db.has_char(primary.id, c)
            })
        })
        .or_else(|| source.chars().next())?;
        loop {
            let id = (resolver.select_fallback)(c, &tried, db)?;
            if tried.contains(&id) {
                return None;
            }
            tried.push(id);
            let Some(font) = db.load_font(id).map(Arc::new) else {
                continue;
            };
            if self.supported(font.clone(), range.clone(), ltr, variants, db) {
                return Some(font);
            }
        }
    }
}

fn push(runs: &mut Vec<Run>, bytes: Range<usize>, font: Arc<ResolvedFont>) {
    if let Some(last) = runs.last_mut()
        && last.bytes.end == bytes.start
        && last.font.id == font.id
    {
        last.bytes.end = bytes.end;
    } else {
        runs.push(Run { bytes, font });
    }
}

/// Preserve covered primary clusters while matching missing graphemes to real faces.
/// Args: Unicode, primary face, source style, existing resolver and database. Returns visual glyphs.
pub(super) fn shape(
    text: &str,
    primary: Arc<ResolvedFont>,
    span: &TextSpan,
    spans: &[TextSpan],
    resolver: &FontResolver,
    db: &mut Arc<Database>,
) -> Vec<Glyph> {
    if text.is_empty() {
        return Vec::new();
    }
    let shaper = Shaper { text, span, spans };
    let bidi = unicode_bidi::BidiInfo::new(text, Some(unicode_bidi::Level::ltr()));
    let original = shaper.visual(
        &[Run {
            bytes: 0..text.len(),
            font: primary.clone(),
        }],
        &bidi,
        db,
    );
    let mut missing = Vec::new();
    for (glyphs, byte) in GlyphClusters::new(&original) {
        let group = &original[glyphs];
        if group.iter().any(Glyph::is_missing) {
            let end = byte.value() + group.iter().map(|g| g.cluster_len).max().unwrap_or(0);
            missing.push(byte.value()..end);
        }
    }
    missing.sort_by_key(|r| r.start);
    let variation_needed = |source: &str| {
        !selectors(source).is_empty()
            && !db
                .with_face_data(primary.id, |data, index| {
                    ttf_parser::Face::parse(data, index).is_ok_and(|f| {
                        selectors(source)
                            .iter()
                            .all(|&(b, s)| f.glyph_variation_index(b, s).is_some_and(|g| g.0 != 0))
                    })
                })
                .unwrap_or(false)
    };
    if missing.is_empty() && !text.graphemes(true).any(variation_needed) {
        return original;
    }
    let mut listed = vec![Choice {
        font: primary.clone(),
        named: span
            .font
            .families
            .iter()
            .any(|f| matches!(f, crate::FontFamily::Named(_))),
    }];
    for family in &span.font.families {
        if let crate::FontFamily::Named(name) = family
            && !db
                .faces()
                .any(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)))
        {
            continue;
        }
        let mut style = span.font.clone();
        style.families = vec![family.clone()];
        if let Some(id) = (resolver.select_font)(&style, db)
            && !listed.iter().any(|c| c.font.id == id)
            && let Some(font) = db.load_font(id)
        {
            listed.push(Choice {
                font: Arc::new(font),
                named: matches!(family, crate::FontFamily::Named(_)),
            });
        }
    }
    let mut runs = Vec::new();
    let mut cache: HashMap<String, Option<Arc<ResolvedFont>>> = HashMap::new();
    for (start, cluster) in text.grapheme_indices(true) {
        let range = start..start + cluster.len();
        let at = missing.partition_point(|r| r.end <= start);
        let absent = missing.get(at).is_some_and(|r| r.start < range.end);
        let sequences = selectors(cluster);
        let variants = !sequences.is_empty();
        let needs_variant = variants
            && !db
                .with_face_data(primary.id, |data, index| {
                    ttf_parser::Face::parse(data, index).is_ok_and(|f| {
                        sequences
                            .iter()
                            .all(|&(b, s)| f.glyph_variation_index(b, s).is_some_and(|g| g.0 != 0))
                    })
                })
                .unwrap_or(false);
        if !absent && !needs_variant {
            push(&mut runs, range, primary.clone());
            continue;
        }
        let ltr = bidi.levels[start].is_ltr();
        let font = cache
            .entry(cluster.to_owned())
            .or_insert_with(|| {
                shaper
                    .choose(
                        range.clone(),
                        ltr,
                        variants,
                        &listed,
                        &primary,
                        resolver,
                        db,
                    )
                    .or_else(|| {
                        variants
                            .then(|| {
                                shaper.choose(
                                    range.clone(),
                                    ltr,
                                    false,
                                    &listed,
                                    &primary,
                                    resolver,
                                    db,
                                )
                            })
                            .flatten()
                    })
            })
            .clone();
        if let Some(font) = font {
            push(&mut runs, range, font);
            continue;
        }
        let boundaries: Vec<_> = cluster
            .char_indices()
            .map(|(b, _)| range.start + b)
            .chain(std::iter::once(range.end))
            .collect();
        let mut prefix = None;
        for &end in boundaries.iter().skip(1).rev() {
            if let Some(font) = listed
                .iter()
                .find(|c| shaper.supported(c.font.clone(), range.start..end, ltr, false, db))
                .map(|c| c.font.clone())
            {
                prefix = Some((end, font));
                break;
            }
        }
        let (end, font) = prefix.unwrap_or((boundaries[1], primary.clone()));
        push(&mut runs, range.start..end, font);
        for p in boundaries.windows(2).filter(|p| p[0] >= end) {
            let bytes = p[0]..p[1];
            let font = shaper
                .choose(bytes.clone(), ltr, false, &listed, &primary, resolver, db)
                .unwrap_or_else(|| primary.clone());
            push(&mut runs, bytes, font);
        }
    }
    let glyphs = shaper.visual(&runs, &bidi, db);
    for glyph in &glyphs {
        if glyph.is_missing() {
            log::warn!(
                "No fonts with a {}/U+{:X} character were found.",
                glyph.byte_idx.char_from(text),
                glyph.byte_idx.char_from(text) as u32
            );
        }
    }
    glyphs
}
