use super::*;

/// Split optional ligatures only when an edited pair lies inside an existing glyph.
/// Args: native text, visible range and original layout. Returns styled intervals and split flags.
pub(super) fn spans(
    run: &TypeRun,
    start: usize,
    end: usize,
    original: Option<&Layout>,
) -> Vec<(crate::geom::CharSpan, bool)> {
    let count = run.content.chars().count();
    let pairs: Vec<_> = run
        .manual_kern
        .iter()
        .filter_map(|(&at, &value)| {
            (at > 0
                && at < count
                && value != 0.
                && original.is_some_and(|l| l.clusters.iter().any(|r| r.start < at && at < r.end)))
            .then_some(at)
        })
        .collect();
    let mut result = Vec::new();
    for span in run.character_style_runs(start, end) {
        let mut ends = vec![span.start, span.end];
        for &at in &pairs {
            for boundary in [at - 1, at + 1] {
                if span.start < boundary && boundary < span.end {
                    ends.push(boundary);
                }
            }
        }
        ends.sort_unstable();
        ends.dedup();
        for range in ends.windows(2) {
            let mut style = span.clone();
            style.start = range[0];
            style.end = range[1];
            let split = pairs
                .iter()
                .any(|&at| style.start < at + 1 && at - 1 < style.end);
            result.push((style, split));
        }
    }
    result
}

/// Resolve the selected flow character's source or accepted native line spacing.
/// Args: text run and logical character index. Returns its existing line-spacing mode.
pub(crate) fn leading(run: &TypeRun, at: usize) -> Option<crate::geom::Leading> {
    use crate::geom::Leading;
    let svg = run.svg_text.as_ref()?;
    let flow = svg.flow.as_ref()?;
    if let Some(value) = run
        .spans
        .iter()
        .find(|s| s.start <= at && at < s.end)
        .and_then(|s| s.leading)
    {
        return Some(value);
    }
    fn find<'a>(
        node: &'a TextNode,
        cursor: &mut usize,
        at: usize,
        inherited: Option<&'a str>,
    ) -> Option<&'a str> {
        match node {
            TextNode::Characters { count } => {
                let start = *cursor;
                *cursor += count;
                (start <= at && at < *cursor).then_some(inherited).flatten()
            }
            TextNode::Element {
                attributes,
                children,
                ..
            } => {
                let own = attributes
                    .get("line-height")
                    .map(String::as_str)
                    .filter(|v| !matches!(*v, "inherit" | "unset"))
                    .or(inherited);
                children.iter().find_map(|c| find(c, cursor, at, own))
            }
        }
    }
    let value = find(&svg.root, &mut 0, at, None);
    Some(match value {
        Some("normal" | "initial") => Leading::Auto(120.),
        Some(value) if value.ends_with('%') => {
            Leading::Auto(value.trim_end_matches('%').parse().unwrap_or(120.))
        }
        Some(value) if value.ends_with("em") => {
            Leading::Auto(value.trim_end_matches("em").parse::<f32>().unwrap_or(1.2) * 100.)
        }
        Some(value) if value.parse::<f32>().is_ok() => {
            Leading::Auto(value.parse::<f32>().unwrap() * 100.)
        }
        Some(value) => Leading::Fixed(length(value, svg.base_px).unwrap_or(flow.leading)),
        None => Leading::Fixed(flow.leading),
    })
}

/// Resolve a pair's editable offset in thousandths of the current character's em.
/// Args: authoritative text and caret index. Returns the native override or source offset.
pub(crate) fn pair_kerning(run: &TypeRun, at: usize) -> f32 {
    if let Some(value) = run.manual_kern.get(&at) {
        return *value;
    }
    layout(run)
        .and_then(|layout| {
            layout
                .characters
                .iter()
                .find(|c| c.index == at)
                .map(|c| (if c.vertical { c.dy } else { c.dx }) / c.px * 1000.)
        })
        .unwrap_or(0.)
}

/// Apply accepted native pair edits to the source's resolved character positions.
/// Args: native run, unmodified SVG, source range start and visible Unicode. Returns editable SVG.
pub(super) fn pairs(run: &TypeRun, source: String, from: usize, content: &str) -> Option<String> {
    if run
        .manual_kern
        .range(from..from + content.chars().count())
        .next()
        .is_none()
    {
        return Some(source);
    }
    let layout = layout_source(run, source.clone(), content)?;
    let count = layout.characters.len();
    let mut dx = vec![0.; count];
    let mut dy = vec![0.; count];
    for c in &layout.characters {
        dx[c.ordinal] = c.dx;
        dy[c.ordinal] = c.dy;
        let at = from + c.index;
        if at == 0 || run.svg_text.as_ref().is_some_and(|s| s.flow.is_some()) && c.line_start {
            continue;
        }
        if let Some(value) = run.manual_kern.get(&at) {
            let target = if c.vertical { &mut dy } else { &mut dx };
            target[c.ordinal] = value * c.px / 1000.;
        }
    }
    let xml = roxmltree::Document::parse(&source).ok()?;
    let nodes: Vec<_> = xml
        .descendants()
        .filter(|n| {
            n.is_element()
                && matches!(n.tag_name().name(), "text" | "tspan" | "textPath")
                && !n.ancestors().any(|a| a.has_tag_name("defs"))
        })
        .collect();
    let root = nodes.iter().find(|n| n.has_tag_name("text"))?;
    let mut edits = Vec::new();
    for node in &nodes {
        for attr in node
            .attributes()
            .filter(|a| matches!(a.name(), "dx" | "dy"))
        {
            edits.push((attr.range(), String::new()));
        }
    }
    let values = |v: Vec<f32>| {
        v.into_iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let at = root.range().start + source[root.range()].find('>')?;
    edits.push((
        at..at,
        format!(" dx=\"{}\" dy=\"{}\"", values(dx), values(dy)),
    ));
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut result = source;
    for (range, value) in edits {
        result.replace_range(range, &value);
    }
    Some(result)
}
