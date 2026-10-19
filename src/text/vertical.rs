//! Vertical writing. Columns run down, the next column starts to the left, and path text stays on its path.
use super::{LayoutGlyph, LayoutLine, character_metrics, compose, glyph_contours};
use crate::geom::{Pt, TypeRun};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Sideways,
    Upright,
    Tate,
}

struct Unit {
    glyphs: Vec<LayoutGlyph>,
    lead: f32,
    height: f32,
    widths: Vec<f32>,
    squeeze: f32,
    upright: bool,
}

struct Column {
    x: f32,
    height: f32,
    glyphs: Vec<LayoutGlyph>,
    carets: Vec<(usize, f32)>,
    start: usize,
    end: usize,
    paragraph: usize,
}

/// Stack point type into columns. A newline starts the column to the left.
/// Args: the vertical run and its horizontal shaping, one line per paragraph. Returns column lines.
pub(super) fn stack_point(run: &TypeRun, lines: &[LayoutLine]) -> Vec<LayoutLine> {
    let pitch = run.px.max(1.);
    let top = run.origin.y;
    let bottom = run
        .wrap_width
        .filter(|width| width.is_finite() && *width > 0.)
        .map(|width| top + width);
    let (columns, _) = flow(
        run,
        lines,
        0,
        run.origin,
        pitch,
        top,
        bottom,
        run.origin.x + pitch * 0.5,
        None,
    );
    columns_to_lines(run, &columns, pitch)
}

/// Lay a vertical story into a frame, starting at the top right.
/// Args: frame geometry, story and first character. Returns paint, carets and overflow for that frame.
pub(crate) fn layout_vertical_frame(
    frame_run: &TypeRun,
    story: &TypeRun,
    start: usize,
) -> crate::text_geometry::TextGeometryLayout {
    let mut layout = crate::text_geometry::TextGeometryLayout::default();
    let total = story.content.chars().count();
    let start = start.min(total);
    layout.visible_start = start;
    layout.visible_end = start;
    layout.source = story.content.clone();
    let Some(bounds) = crate::text_geometry::frame_bounds(frame_run) else {
        layout.overflow = start < total;
        return layout;
    };
    let frame = frame_run.frame.as_ref();
    let inset = frame.map(|frame| frame.inset).unwrap_or([0.; 4]);
    let inner_min = bounds.min + Pt::new(inset[3], inset[0]);
    let inner_max = bounds.max - Pt::new(inset[1], inset[2]);
    if !inner_min.x.is_finite()
        || !inner_min.y.is_finite()
        || !inner_max.x.is_finite()
        || !inner_max.y.is_finite()
        || inner_max.x <= inner_min.x
        || inner_max.y <= inner_min.y
    {
        layout.overflow = start < total;
        layout
            .carets
            .push((start, frame_run.origin, Pt::new(0., 1.)));
        return layout;
    }
    let mut flat = story.clone();
    flat.vertical = false;
    flat.wrap_width = None;
    flat.frame = None;
    flat.on_path = None;
    flat.layout = None;
    let lines = compose(&flat);
    let pitch = story.px.max(1.);
    let (columns, overflow) = flow(
        story,
        &lines,
        start,
        frame_run.origin,
        pitch,
        inner_min.y,
        Some(inner_max.y),
        inner_max.x,
        Some(inner_min.x),
    );
    let mut paint = story.clone();
    paint.origin = frame_run.origin;
    paint.transform = None;
    for column in &columns {
        for glyph in &column.glyphs {
            let fill = story.character_style(glyph.cluster).fill;
            let contours = glyph_contours(&paint, glyph);
            if contours.is_empty() {
                continue;
            }
            let from = layout.contours.len();
            layout.contours.extend(contours);
            let to = layout.contours.len();
            if let Some((previous, range)) = layout.paint_ranges.last_mut()
                && *previous == fill
                && range.end == from
            {
                range.end = to;
            } else {
                layout.paint_ranges.push((fill, from..to));
            }
            layout.visible_end = layout.visible_end.max(glyph.cluster + 1);
        }
        for &(index, y) in &column.carets {
            layout.carets.push((
                index,
                Pt::new(frame_run.origin.x + column.x, frame_run.origin.y + y),
                Pt::new(0., 1.),
            ));
            layout.caret_heights.push((index, pitch));
        }
        let text: String = story
            .content
            .chars()
            .skip(column.start)
            .take(column.end.saturating_sub(column.start))
            .collect();
        let mut line_run = paint.clone();
        line_run.wrap_width = None;
        line_run.content = text.clone();
        layout
            .export_lines
            .push(crate::text_geometry::PositionedTextLine {
                run: Box::new(line_run),
                line: LayoutLine {
                    text,
                    hyphenated: false,
                    start: column.start,
                    end: column.end,
                    paragraph: column.paragraph,
                    last: false,
                    baseline: 0.,
                    height: column.height,
                    offset: column.x,
                    width: pitch,
                    natural_width: pitch,
                    glyphs: column.glyphs.clone(),
                    carets: column.carets.clone(),
                },
                origin: frame_run.origin,
            });
    }
    if layout.carets.is_empty() {
        layout.carets.push((
            start,
            Pt::new(inner_max.x - pitch * 0.5, inner_min.y),
            Pt::new(0., 1.),
        ));
    }
    layout.overflow = overflow || layout.visible_end < total;
    layout.visible_text = story
        .content
        .chars()
        .skip(start)
        .take(layout.visible_end.saturating_sub(start))
        .collect();
    layout
}

fn columns_to_lines(run: &TypeRun, columns: &[Column], pitch: f32) -> Vec<LayoutLine> {
    if columns.is_empty() {
        return vec![LayoutLine {
            text: String::new(),
            hyphenated: false,
            start: 0,
            end: 0,
            paragraph: 0,
            last: true,
            baseline: 0.,
            height: 0.,
            offset: 0.,
            width: pitch,
            natural_width: pitch,
            glyphs: Vec::new(),
            carets: vec![(0, 0.)],
        }];
    }
    let last = columns.len() - 1;
    columns
        .iter()
        .enumerate()
        .map(|(index, column)| LayoutLine {
            text: run
                .content
                .chars()
                .skip(column.start)
                .take(column.end.saturating_sub(column.start))
                .collect(),
            hyphenated: false,
            start: column.start,
            end: column.end,
            paragraph: column.paragraph,
            last: index == last,
            baseline: 0.,
            height: column.height,
            offset: column.x,
            width: pitch,
            natural_width: pitch,
            glyphs: column.glyphs.clone(),
            carets: column.carets.clone(),
        })
        .collect()
}

fn flow(
    run: &TypeRun,
    lines: &[LayoutLine],
    start: usize,
    origin: Pt,
    pitch: f32,
    top: f32,
    bottom: Option<f32>,
    right: f32,
    left: Option<f32>,
) -> (Vec<Column>, bool) {
    let mut columns = Vec::new();
    let mut index = 0usize;
    let mut column = Column::open(index, right, pitch, origin);
    let mut pen = top;
    let mut overflow = false;
    let mut placed = false;
    for (_, line) in lines.iter().enumerate() {
        let units = units(run, &line.glyphs, start);
        if units.is_empty() {
            continue;
        }
        if placed && !column.glyphs.is_empty() {
            if !open_next(
                &mut columns,
                &mut column,
                &mut index,
                &mut pen,
                right,
                pitch,
                origin,
                top,
                left,
            ) {
                overflow = true;
                break;
            }
        }
        for unit in units {
            if column.glyphs.is_empty() {
                column.paragraph = line.paragraph;
            }
            let span = unit.lead + unit.height;
            if let Some(bottom) = bottom
                && !column.glyphs.is_empty()
                && pen + span > bottom + 0.01
            {
                if !open_next(
                    &mut columns,
                    &mut column,
                    &mut index,
                    &mut pen,
                    right,
                    pitch,
                    origin,
                    top,
                    left,
                ) {
                    overflow = true;
                    break;
                }
            }
            pen += unit.lead;
            let cell = pen;
            pen += unit.height;
            place_unit(&mut column, &unit, cell, origin);
            placed = true;
        }
        if overflow {
            break;
        }
    }
    if !column.glyphs.is_empty() || columns.is_empty() {
        column.height = (pen - top).max(0.);
        if column.carets.is_empty() {
            column.carets.push((start, top - origin.y));
        }
        columns.push(column);
    }
    let consumed = columns
        .iter()
        .filter_map(|column| column.glyphs.last().map(|glyph| glyph.cluster + 1))
        .max()
        .unwrap_or(start);
    let pending = lines
        .iter()
        .flat_map(|line| &line.glyphs)
        .any(|glyph| glyph.cluster >= start && glyph.cluster + 1 > consumed);
    (columns, overflow || pending)
}

fn open_next(
    columns: &mut Vec<Column>,
    column: &mut Column,
    index: &mut usize,
    pen: &mut f32,
    right: f32,
    pitch: f32,
    origin: Pt,
    top: f32,
    left: Option<f32>,
) -> bool {
    let next = *index + 1;
    let center = right - pitch * 0.5 - next as f32 * pitch;
    if left.is_some_and(|left| center < left - 0.01) {
        return false;
    }
    column.height = (*pen - top).max(0.);
    columns.push(std::mem::replace(
        column,
        Column::open(next, right, pitch, origin),
    ));
    *index = next;
    *pen = top;
    true
}

fn place_unit(column: &mut Column, unit: &Unit, cell_top: f32, origin: Pt) {
    let local_top = cell_top - origin.y;
    if column.glyphs.is_empty() {
        column.start = unit.glyphs[0].cluster;
        column.carets.push((column.start, local_top));
    }
    let center_y = cell_top + unit.height * 0.5 - origin.y;
    if unit.upright && unit.glyphs.len() > 1 {
        let mut x = column.x - unit.widths.iter().sum::<f32>() * unit.squeeze * 0.5;
        for (glyph, width) in unit.glyphs.iter().zip(&unit.widths) {
            let scaled = width * unit.squeeze;
            let mut placed = glyph.clone();
            placed.turn = 1;
            placed.hscale *= unit.squeeze;
            placed.x = x + scaled * 0.5;
            placed.y = center_y;
            placed.advance = 0.;
            x += scaled;
            column.end = placed.cluster + 1;
            column.glyphs.push(placed);
        }
        if let Some(first) = column.glyphs.iter_mut().rev().nth(unit.glyphs.len() - 1) {
            first.advance = unit.height;
        }
    } else if let Some(glyph) = unit.glyphs.first() {
        let mut placed = glyph.clone();
        placed.turn = if unit.upright { 1 } else { 2 };
        placed.x = column.x;
        placed.y = center_y;
        placed.advance = unit.height;
        column.end = placed.cluster + 1;
        column.glyphs.push(placed);
    }
    column.carets.push((column.end, local_top + unit.height));
    column.height = local_top + unit.height;
}

impl Column {
    fn open(index: usize, right: f32, pitch: f32, origin: Pt) -> Self {
        Self {
            x: right - pitch * 0.5 - index as f32 * pitch - origin.x,
            height: 0.,
            glyphs: Vec::new(),
            carets: Vec::new(),
            start: 0,
            end: 0,
            paragraph: 0,
        }
    }
}

fn units(run: &TypeRun, glyphs: &[LayoutGlyph], start: usize) -> Vec<Unit> {
    let role = roles(&run.content);
    let mut units = Vec::new();
    let mut index = 0;
    while index < glyphs.len() {
        if glyphs[index].cluster < start {
            index += 1;
            continue;
        }
        let kind = role
            .get(glyphs[index].cluster)
            .copied()
            .unwrap_or(Role::Sideways);
        if kind == Role::Tate {
            let mut group = vec![glyphs[index].clone()];
            let mut next = index + 1;
            while next < glyphs.len()
                && glyphs[next].cluster >= start
                && role.get(glyphs[next].cluster).copied() == Some(Role::Tate)
                && glyphs[next].cluster == glyphs[next - 1].cluster + 1
            {
                group.push(glyphs[next].clone());
                next += 1;
            }
            units.push(tate_unit(run, &glyphs, index, group));
            index = next;
        } else {
            units.push(single_unit(run, glyphs, index, kind == Role::Upright));
            index += 1;
        }
    }
    units
}

fn single_unit(run: &TypeRun, glyphs: &[LayoutGlyph], index: usize, upright: bool) -> Unit {
    let glyph = glyphs[index].clone();
    let metrics = character_metrics(run, glyph.cluster);
    let em = metrics.px.unwrap_or(run.px).max(1.);
    let tracking = metrics.tracking.unwrap_or(0.) * em / 1000.;
    let object = run.inlines.iter().any(|item| item.at == glyph.cluster);
    let height = if upright || object {
        (em + tracking).max(em * 0.05)
    } else {
        glyph.advance.max(em * 0.05)
    };
    Unit {
        glyphs: vec![glyph],
        lead: lead(glyphs, index),
        height,
        widths: vec![height],
        squeeze: 1.,
        upright,
    }
}

fn tate_unit(run: &TypeRun, glyphs: &[LayoutGlyph], index: usize, group: Vec<LayoutGlyph>) -> Unit {
    let metrics = character_metrics(run, group[0].cluster);
    let em = metrics.px.unwrap_or(run.px).max(1.);
    let tracking = metrics.tracking.unwrap_or(0.) * em / 1000.;
    let widths: Vec<f32> = group
        .iter()
        .map(|glyph| {
            let tracking =
                character_metrics(run, glyph.cluster).tracking.unwrap_or(0.) * em / 1000.;
            (glyph.advance - tracking).max(em * 0.05)
        })
        .collect();
    let total = widths.iter().sum::<f32>().max(0.01);
    Unit {
        glyphs: group,
        lead: lead(glyphs, index),
        height: (em + tracking).max(em * 0.05),
        widths,
        squeeze: if total > em { em / total } else { 1. },
        upright: true,
    }
}

fn lead(glyphs: &[LayoutGlyph], index: usize) -> f32 {
    if index == 0 {
        return 0.;
    }
    let previous = &glyphs[index - 1];
    let gap = glyphs[index].x - (previous.x + previous.advance);
    if gap.is_finite() { gap.max(0.) } else { 0. }
}

fn roles(content: &str) -> Vec<Role> {
    let chars: Vec<char> = content.chars().collect();
    let mut role = vec![Role::Sideways; chars.len()];
    let mut index = 0;
    while index < chars.len() {
        if chars[index].is_ascii_digit() {
            let mut end = index + 1;
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
            if (2..=3).contains(&(end - index)) {
                for slot in &mut role[index..end] {
                    *slot = Role::Tate;
                }
            }
            index = end;
            continue;
        }
        role[index] = if stands_upright(chars[index]) {
            Role::Upright
        } else {
            Role::Sideways
        };
        index += 1;
    }
    role
}

/// Marks that stand like CJK. Latin letters, ASCII digits and ASCII punctuation lie on their side.
/// Args: one character. Returns whether its ink keeps the horizontal orientation.
fn stands_upright(ch: char) -> bool {
    if ch.is_ascii()
        || ('\u{00C0}'..='\u{024F}').contains(&ch)
        || ('\u{0370}'..='\u{052F}').contains(&ch)
    {
        return false;
    }
    if ('\u{1100}'..='\u{11FF}').contains(&ch)
        || ('\u{2E80}'..='\u{9FFF}').contains(&ch)
        || ('\u{AC00}'..='\u{D7AF}').contains(&ch)
        || ('\u{F900}'..='\u{FAFF}').contains(&ch)
        || ('\u{FE10}'..='\u{FE6F}').contains(&ch)
        || ('\u{FF00}'..='\u{FFEF}').contains(&ch)
        || ('\u{20000}'..='\u{3FFFD}').contains(&ch)
    {
        return true;
    }
    matches!(ch, '§' | '×' | '÷' | '°' | '¶' | '†' | '‡' | '•' | '·')
}
