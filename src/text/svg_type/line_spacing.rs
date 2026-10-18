use super::*;
use crate::geom::Leading;

struct Row<'a> {
    characters: Vec<&'a VisualCharacter>,
    cross: f32,
    anchor: f32,
    vertical: bool,
    node: usize,
    previous: Option<usize>,
    next: Option<usize>,
}

fn rows(layout: &Layout) -> Vec<Row<'_>> {
    let mut nodes = Vec::new();
    texts(layout.tree.root(), &mut nodes);
    let glyphs: BTreeMap<_, _> = layout
        .glyphs
        .iter()
        .map(|g| ((g.node, g.chunk, g.index), g))
        .collect();
    let mut characters: Vec<_> = layout.characters.iter().collect();
    characters.sort_by_key(|c| c.ordinal);
    let mut result = Vec::<Row<'_>>::new();
    let mut previous = None::<usize>;
    for c in characters {
        let Some(g) = glyphs.get(&(c.node, c.chunk, c.index)) else {
            if let Some(i) = previous
                && result[i]
                    .characters
                    .last()
                    .is_some_and(|last| last.node == c.node && last.chunk == c.chunk)
            {
                result[i].characters.push(c);
            }
            continue;
        };
        if !g.source_linear {
            previous = None;
            continue;
        }
        let cross = if c.vertical {
            g.authored.x
        } else {
            g.authored.y
        };
        let measured_anchor = if c.vertical {
            g.authored.y
        } else {
            g.authored.x
        };
        let chunk = nodes
            .get(c.node)
            .and_then(|text| text.chunks().get(c.chunk));
        let anchor = chunk
            .and_then(|chunk| if c.vertical { chunk.y() } else { chunk.x() })
            .unwrap_or(measured_anchor);
        if let Some(i) = previous {
            let row = &result[i];
            if row.cross == cross && row.vertical == c.vertical && row.node == c.node {
                result[i].characters.push(c);
                continue;
            }
            if row.vertical != c.vertical
                || row.node != c.node
                || row.previous.is_some_and(|before| {
                    let direction = (row.cross - result[before].cross).signum();
                    (cross - row.cross).signum() != direction && anchor != row.anchor
                })
            {
                previous = None;
            }
        }
        let i = result.len();
        if let Some(before) = previous {
            result[before].next = Some(i);
        }
        result.push(Row {
            characters: vec![c],
            cross,
            anchor,
            vertical: c.vertical,
            node: c.node,
            previous,
            next: None,
        });
        previous = Some(i);
    }
    result
}

fn gap(rows: &[Row<'_>], i: usize) -> Option<f32> {
    let row = &rows[i];
    row.next
        .map(|next| (rows[next].cross - row.cross).abs())
        .or_else(|| {
            row.previous
                .map(|previous| (row.cross - rows[previous].cross).abs())
        })
}

/// Measure real line spacing without confusing superscripts with separate lines.
/// Args: canonical glyph layout. Returns each source character's authored baseline distance.
pub(super) fn measured(layout: &Layout) -> BTreeMap<usize, Leading> {
    let rows = rows(layout);
    let mut result = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        if let Some(value) = gap(&rows, i) {
            for c in &row.characters {
                result.insert(c.index, Leading::Fixed(value));
            }
        }
    }
    result
}

/// Move authored rows through standard SVG offsets while preserving absolute resets.
/// Args: accepted native intent, original layout, source range and resolved offsets. Returns no value, or no result for a non-finite placement.
pub(super) fn apply(
    run: &TypeRun,
    layout: &Layout,
    from: usize,
    dx: &mut [f32],
    dy: &mut [f32],
) -> Option<()> {
    if run.svg_text.as_ref()?.flow.is_some() {
        return Some(());
    }
    let rows = rows(layout);
    let mut targets: Vec<_> = rows.iter().map(|r| r.cross).collect();
    let mut changes = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        if let Some(before) = row.previous {
            let previous = &rows[before];
            let distance = (row.cross - previous.cross).abs();
            let leading = previous
                .characters
                .iter()
                .map(|c| {
                    run.spans
                        .iter()
                        .find(|s| s.start <= from + c.index && from + c.index < s.end)
                        .and_then(|s| s.leading)
                        .or_else(|| (run.leading > 0.5).then_some(Leading::Fixed(run.leading)))
                        .map_or(distance, |v| v.pixels(c.px))
                })
                .reduce(f32::max)
                .unwrap_or(distance);
            targets[i] = targets[before] + (row.cross - previous.cross).signum() * leading;
        }
        let offset = targets[i] - row.cross;
        if !offset.is_finite() {
            return None;
        }
        for c in &row.characters {
            changes.insert(c.ordinal, offset);
        }
    }
    let mut nodes = Vec::new();
    texts(layout.tree.root(), &mut nodes);
    let mut chunks = Vec::new();
    for ((node, chunk), start) in &layout.chunk_starts {
        let text = nodes.get(*node)?;
        let chunk = text.chunks().get(*chunk)?;
        let vertical = text.writing_mode() == usvg::WritingMode::TopToBottom;
        chunks.push((
            *start,
            vertical,
            matches!(chunk.text_flow(), usvg::TextFlow::Linear),
            if vertical {
                chunk.x().is_some()
            } else {
                chunk.y().is_some()
            },
        ));
    }
    chunks.sort_by_key(|c| c.0);
    let mut reset = vec![false; dx.len()];
    let mut linear = vec![false; dx.len()];
    let mut vertical = vec![false; dx.len()];
    for (i, &(start, is_vertical, is_linear, absolute)) in chunks.iter().enumerate() {
        let end = chunks.get(i + 1).map_or(dx.len(), |c| c.0);
        if start < reset.len() {
            reset[start] |= absolute;
        }
        for at in start..end {
            linear[at] = is_linear;
            vertical[at] = is_vertical;
        }
    }
    let mut desired = 0.;
    let mut carried = 0.;
    for at in 0..dx.len() {
        if !linear[at] {
            desired = 0.;
            carried = 0.;
            continue;
        }
        if let Some(value) = changes.get(&at) {
            desired = *value;
        }
        let added = desired - if reset[at] { 0. } else { carried };
        let offset = if vertical[at] {
            &mut dx[at]
        } else {
            &mut dy[at]
        };
        *offset += added;
        if !offset.is_finite() {
            return None;
        }
        carried = desired;
    }
    Some(())
}
