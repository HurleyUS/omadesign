//! Hidden-character marks and symbols that flow as one character.
use super::compose;
use crate::document::{Document, Fill, Shape};
use crate::geom::{Bounds, Geom, InlinePaint, InlineSymbol, Pt, TypeRun};

/// A mark drawn over type while hidden characters are showing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HiddenMark {
    /// Filled dot above a space.
    Dot(Pt),
    /// Paragraph mark after a line that breaks.
    Paragraph(Pt),
    /// Story mark after the last line.
    Story(Pt),
}

/// Place hidden-character marks for one story.
/// Args: text run. Returns document-space marks. Path type returns none. The marks are view paint, not ink.
pub fn hidden_marks(run: &TypeRun) -> Vec<HiddenMark> {
    if run.on_path.is_some() {
        return Vec::new();
    }
    let lines = compose(run);
    let chars: Vec<char> = run.content.chars().collect();
    let mut marks = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        for glyph in &line.glyphs {
            if chars.get(glyph.cluster).copied() != Some(' ') {
                continue;
            }
            marks.push(HiddenMark::Dot(run.map_point(Pt::new(
                run.origin.x + glyph.x + glyph.advance * 0.5,
                run.origin.y + glyph.y - 3.,
            ))));
        }
        let last = index + 1 == lines.len() && line.end >= chars.len();
        let breaks = chars.get(line.end).copied() == Some('\n');
        if !last && !breaks {
            continue;
        }
        let x1 = line
            .glyphs
            .iter()
            .map(|glyph| glyph.x + glyph.advance)
            .fold(line.offset, f32::max);
        let at = run.map_point(Pt::new(
            run.origin.x + x1 + 1.,
            run.origin.y + line.baseline,
        ));
        marks.push(if last {
            HiddenMark::Story(at)
        } else {
            HiddenMark::Paragraph(at)
        });
    }
    marks
}

/// Rebuild every story's symbol ink from the current symbols.
/// Args: document. Returns nothing. A missing symbol leaves an empty slot. Point type rebuilds its letter outlines.
pub fn refresh_document(doc: &mut Document) {
    let art = symbol_art(doc);
    for layer in &mut doc.layers {
        let Some(shapes) = layer.kind.shapes_mut() else {
            continue;
        };
        for shape in shapes {
            let Geom::Text(run) = &mut shape.geom else {
                continue;
            };
            if run.inlines.is_empty() {
                run.inline_paint.clear();
                continue;
            }
            let next = paint_from(&art, run);
            if next == run.inline_paint {
                continue;
            }
            run.inline_paint = next;
            run.layout = None;
            if run.frame.is_none() && run.on_path.is_none() && run.svg_text.is_none() {
                let contours = super::shape(run);
                run.contours = contours;
            }
        }
    }
}

/// Fill one story's symbol ink before its outlines are rebuilt.
/// Args: document and story. Returns nothing. The saved record stays the symbol id and scale.
pub fn paint_run(doc: &Document, run: &mut TypeRun) {
    if run.inlines.is_empty() {
        run.inline_paint.clear();
        return;
    }
    run.inline_paint = paint_from(&symbol_art(doc), run);
}

/// Draw each inline symbol in its own fill.
/// Args: text run. Returns document-space fill paths. Area type, path type and a missing symbol draw nothing.
pub fn inline_paths(run: &TypeRun) -> Vec<(Fill, tiny_skia::Path)> {
    if run.inlines.is_empty()
        || run.frame.is_some()
        || run.on_path.is_some()
        || run.inline_paint.is_empty()
    {
        return Vec::new();
    }
    let mut drawn = Vec::new();
    for line in compose(run).iter() {
        for glyph in &line.glyphs {
            let Some(paint) = run
                .inline_paint
                .iter()
                .find(|paint| paint.at == glyph.cluster)
            else {
                continue;
            };
            if paint.pieces.is_empty() || paint.advance <= 0. || paint.height <= 0. {
                continue;
            }
            let top = glyph.y - paint.px * 0.35 - paint.shift - paint.height * 0.5;
            for (fill, contours) in &paint.pieces {
                let mut path = tiny_skia::PathBuilder::new();
                for contour in contours {
                    if contour.len() < 2 {
                        continue;
                    }
                    let point = |unit: Pt| {
                        run.map_point(Pt::new(
                            run.origin.x + glyph.x + unit.x * paint.advance,
                            run.origin.y + top + unit.y * paint.height,
                        ))
                    };
                    let first = point(contour[0]);
                    path.move_to(first.x, first.y);
                    for unit in &contour[1..] {
                        let placed = point(*unit);
                        path.line_to(placed.x, placed.y);
                    }
                    path.close();
                }
                if let Some(path) = path.finish() {
                    drawn.push((fill.clone(), path));
                }
            }
        }
    }
    drawn
}

/// Keep inline records on the characters that survived a rewrite.
/// Args: records, and each replacement as old start, old end, and the new character index of that start. Returns the records still in the story.
pub(crate) fn shift_rewrites(
    inlines: &mut Vec<InlineSymbol>,
    mapped: &[(usize, usize, usize, usize)],
    position: impl Fn(usize) -> usize,
) {
    let mut kept = Vec::new();
    for mut inline in std::mem::take(inlines) {
        if mapped
            .iter()
            .any(|&(start, end, _, _)| inline.at >= start && inline.at < end)
        {
            continue;
        }
        inline.at = position(inline.at);
        kept.push(inline);
    }
    *inlines = kept;
}

/// Keep inline records across a direct character replacement.
/// Args: records, replaced range and the number of characters written in its place. Returns nothing.
pub(crate) fn shift_replacement(
    inlines: &mut Vec<InlineSymbol>,
    start: usize,
    end: usize,
    inserted: usize,
) {
    let removed = end.saturating_sub(start);
    inlines.retain(|inline| inline.at < start || inline.at >= end);
    for inline in inlines.iter_mut() {
        if inline.at >= end {
            inline.at = inline.at - removed + inserted;
        }
    }
}

struct SymbolArt {
    id: u64,
    bounds: Bounds,
    pieces: Vec<(Fill, Vec<Vec<Pt>>)>,
}

fn symbol_art(doc: &Document) -> Vec<SymbolArt> {
    crate::layout_components::definitions(doc)
        .into_iter()
        .filter_map(|(_, id, _, _)| {
            let (layer, root) = locate(doc, id)?;
            let bounds = root.geom.bbox();
            if bounds.width() < 0.01 || bounds.height() < 0.01 {
                return Some(SymbolArt {
                    id,
                    bounds,
                    pieces: Vec::new(),
                });
            }
            let mut pieces = Vec::new();
            for shape in member_shapes(doc, layer, id) {
                if shape.id == id || shape.style.fill.is_none() {
                    continue;
                }
                let contours = shape
                    .world_contours(24)
                    .into_iter()
                    .map(|contour| {
                        contour
                            .into_iter()
                            .map(|point| {
                                Pt::new(
                                    (point.x - bounds.min.x) / bounds.width(),
                                    (point.y - bounds.min.y) / bounds.height(),
                                )
                            })
                            .collect()
                    })
                    .filter(|contour: &Vec<Pt>| contour.len() >= 2)
                    .collect();
                pieces.push((shape.style.fill.clone(), contours));
            }
            Some(SymbolArt { id, bounds, pieces })
        })
        .collect()
}

fn locate(doc: &Document, id: u64) -> Option<(usize, &Shape)> {
    doc.layers.iter().enumerate().find_map(|(layer, item)| {
        item.kind
            .shapes()?
            .iter()
            .find(|shape| shape.id == id)
            .map(|shape| (layer, shape))
    })
}

fn member_shapes(doc: &Document, layer: usize, root: u64) -> Vec<Shape> {
    let mut ids = vec![root];
    ids.extend(crate::layout::descendants(doc, layer, root));
    let Some(shapes) = doc.layers.get(layer).and_then(|item| item.kind.shapes()) else {
        return Vec::new();
    };
    shapes
        .iter()
        .filter(|shape| ids.contains(&shape.id))
        .cloned()
        .collect()
}

fn paint_from(art: &[SymbolArt], run: &TypeRun) -> Vec<InlinePaint> {
    run.inlines
        .iter()
        .map(|inline| {
            let px = run.character_style(inline.at).px.unwrap_or(run.px).max(1.);
            let scale = if inline.scale.is_finite() && inline.scale > 0. && inline.scale <= 100. {
                inline.scale
            } else {
                1.
            };
            let height = px * scale;
            let symbol = art.iter().find(|item| item.id == inline.symbol);
            let advance = symbol
                .filter(|item| item.bounds.height() >= 0.01)
                .map(|item| height * item.bounds.width() / item.bounds.height())
                .filter(|advance| advance.is_finite() && *advance >= 0.)
                .unwrap_or(px);
            InlinePaint {
                at: inline.at,
                advance,
                height,
                shift: inline.shift,
                px,
                pieces: symbol.map(|item| item.pieces.clone()).unwrap_or_default(),
            }
        })
        .collect()
}
