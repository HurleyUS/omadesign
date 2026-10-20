use super::*;

/// Keep source character slots distinct from renderer-inserted direction controls.
/// Args: validated native SVG source. Returns runtime SVG and inserted scalar positions for each text root.
pub(super) fn indexed(source: &str) -> (String, Vec<Vec<usize>>) {
    let mut indices = Vec::new();
    let source = render(source, Some(&mut indices));
    (source, indices)
}

/// Normalize direction without changing the source character protocol.
/// Args: SVG source. Returns equivalent renderer input.
pub(super) fn source(source: &str) -> String {
    render(source, None)
}

fn positions(
    node: roxmltree::Node<'_, '_>,
    controls: &[(usize, char)],
    text_ends: &[(usize, usize)],
) -> Vec<usize> {
    let before = |at| {
        text_ends
            .partition_point(|(end, _)| *end <= at)
            .checked_sub(1)
            .map_or(0, |i| text_ends[i].1)
    };
    let base = before(node.range().start);
    let start = controls.partition_point(|(at, _)| *at < node.range().start);
    let end = controls.partition_point(|(at, _)| *at < node.range().end);
    controls[start..end]
        .iter()
        .map(|(at, _)| before(*at) - base)
        .collect()
}

fn render(source: &str, mapping: Option<&mut Vec<Vec<usize>>>) -> String {
    let Ok(xml) = roxmltree::Document::parse(source) else {
        return source.into();
    };
    let nodes: Vec<_> = xml
        .descendants()
        .filter(|n| n.is_element() && matches!(n.tag_name().name(), "text" | "tspan" | "textPath"))
        .collect();
    let inherited = |node: roxmltree::Node<'_, '_>, key: &str, initial: &str| {
        node.ancestors()
            .filter_map(|n| n.attribute(key))
            .find(|v| !matches!(*v, "inherit" | "unset"))
            .unwrap_or(initial)
            .to_string()
    };
    let mut controls = Vec::new();
    let mut edits = Vec::new();
    for node in &nodes {
        let rtl = inherited(*node, "direction", "ltr") == "rtl";
        if rtl {
            let anchor = inherited(*node, "text-anchor", "start");
            let anchor = match anchor.as_str() {
                "start" => "end",
                "end" => "start",
                other => other,
            };
            if let Some(attr) = node.attributes().find(|a| a.name() == "text-anchor") {
                edits.push((attr.range_value(), anchor.into()));
            } else if let Some(end) = source[node.range()].find('>') {
                let at = node.range().start + end;
                edits.push((at..at, format!(" text-anchor=\"{anchor}\"")));
            }
        }
        let mode = inherited(*node, "unicode-bidi", "normal");
        if (node.has_tag_name("text")
            || node.attribute("unicode-bidi").is_some()
            || node.attribute("x").is_some()
            || node.attribute("y").is_some())
            && (rtl || mode != "normal")
            && node.children().next().is_some()
        {
            let isolate = mode.contains("isolate");
            let prefix = if isolate {
                if rtl { '\u{2067}' } else { '\u{2066}' }
            } else if mode.contains("override") {
                if rtl { '\u{202e}' } else { '\u{202d}' }
            } else if rtl {
                '\u{202b}'
            } else {
                '\u{202a}'
            };
            if let Some(open) = source[node.range()].find('>')
                && let Some(close) = source[node.range()].rfind("</")
            {
                controls.push((node.range().start + open + 1, prefix));
                controls.push((
                    node.range().start + close,
                    if isolate { '\u{2069}' } else { '\u{202c}' },
                ));
            }
        }
    }
    controls.sort_by_key(|(at, _)| *at);
    let mut total = 0usize;
    let text_ends: Vec<_> = xml
        .descendants()
        .filter(|n| n.is_text())
        .map(|n| {
            total += n.text().unwrap_or("").chars().count();
            (n.range().end, total)
        })
        .collect();
    if let Some(mapping) = mapping {
        *mapping = nodes
            .iter()
            .filter(|n| n.has_tag_name("text") && !n.ancestors().any(|a| a.has_tag_name("defs")))
            .map(|node| {
                positions(*node, &controls, &text_ends)
                    .into_iter()
                    .enumerate()
                    .map(|(i, p)| p + i)
                    .collect()
            })
            .collect();
    }
    for node in nodes {
        let positions = positions(node, &controls, &text_ends);
        for attr in node
            .attributes()
            .filter(|a| matches!(a.name(), "x" | "y" | "dx" | "dy" | "rotate"))
        {
            let mut values: Vec<_> = attr
                .value()
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .collect();
            let mut added = 0;
            for position in &positions {
                if matches!(attr.name(), "x" | "y") && *position == 0 && values.len() == 1 {
                    continue;
                }
                let at = position + added;
                if at < values.len() {
                    let dummy = if matches!(attr.name(), "x" | "y") {
                        values[at].clone()
                    } else {
                        "0".into()
                    };
                    values.insert(at, dummy);
                    added += 1;
                }
            }
            if added > 0 {
                edits.push((attr.range_value(), values.join(" ")));
            }
        }
    }
    edits.extend(controls.into_iter().map(|(at, c)| (at..at, c.to_string())));
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut result = source.to_owned();
    for (range, value) in edits {
        result.replace_range(range, &value);
    }
    result
}

pub(super) struct Characters {
    pub order: Vec<VisualCharacter>,
    pub count: usize,
    pub offsets: Vec<(usize, f32, f32)>,
    pub chunks: BTreeMap<(usize, usize), usize>,
}

/// Match visible glyph owners to actual source scalars, retaining authored control slots.
/// Args: rendered text roots, native story and inserted control indices. Returns a lossless character map, or no map for inconsistent source.
pub(super) fn characters(
    nodes: &[usvg::Text],
    content: &str,
    mapping: &[Vec<usize>],
) -> Option<Characters> {
    let normalized = content.replace(['\n', '\r', '\t'], " ");
    let mut result = Characters {
        order: Vec::new(),
        count: 0,
        offsets: Vec::new(),
        chunks: BTreeMap::new(),
    };
    let mut cursor = 0;
    for (node_index, node) in nodes.iter().enumerate() {
        let inserted = mapping.get(node_index).map_or(&[][..], Vec::as_slice);
        let project = |at: usize| at - inserted.partition_point(|p| *p < at);
        let mut offset = 0;
        let base_ordinal = result.count;
        for (chunk_index, chunk) in node.chunks().iter().enumerate() {
            let text = chunk.text();
            let bytes: Vec<_> = text.char_indices().map(|(b, _)| b).collect();
            if bytes.is_empty() {
                continue;
            }
            let authored: String = text
                .chars()
                .enumerate()
                .filter(|(i, _)| inserted.binary_search(&(offset + i)).is_err())
                .map(|(_, c)| c)
                .collect();
            let start = cursor + normalized[cursor..].find(&authored)?;
            let native_start = content[..start].chars().count();
            let projected_start = project(offset);
            result
                .chunks
                .insert((node_index, chunk_index), base_ordinal + projected_start);
            for (scalar, c) in text.chars().enumerate() {
                let at = offset + scalar;
                if bidi_control(c) && inserted.binary_search(&at).is_err() {
                    result.offsets.push((
                        base_ordinal + project(at),
                        node.dx().get(at).copied().unwrap_or(0.),
                        node.dy().get(at).copied().unwrap_or(0.),
                    ));
                }
            }
            let first_visible = text.chars().position(|c| !bidi_control(c));
            let bidi = unicode_bidi::BidiInfo::new(text, Some(unicode_bidi::Level::ltr()));
            for paragraph in &bidi.paragraphs {
                let (levels, runs) = bidi.visual_runs(paragraph, paragraph.range.clone());
                for range in runs {
                    let rtl = levels[range.start].is_rtl();
                    let first = bytes.partition_point(|b| *b < range.start);
                    let end = bytes.partition_point(|b| *b < range.end);
                    for i in 0..end - first {
                        let scalar = if rtl { end - 1 - i } else { first + i };
                        let byte = bytes[scalar];
                        if text[byte..].chars().next().is_some_and(bidi_control) {
                            continue;
                        }
                        let runtime = offset + scalar;
                        if inserted.binary_search(&runtime).is_ok() {
                            return None;
                        }
                        let ordinal = project(runtime);
                        let span = chunk
                            .spans()
                            .iter()
                            .find(|s| s.start() <= byte && byte < s.end());
                        result.order.push(VisualCharacter {
                            node: node_index,
                            chunk: chunk_index,
                            text_index: runtime,
                            index: native_start + ordinal - projected_start,
                            ordinal: base_ordinal + ordinal,
                            dx: node.dx().get(runtime).copied().unwrap_or(0.),
                            dy: node.dy().get(runtime).copied().unwrap_or(0.),
                            vertical: node.writing_mode() == usvg::WritingMode::TopToBottom
                                && matches!(chunk.text_flow(), usvg::TextFlow::Linear),
                            line_start: first_visible == Some(scalar),
                            rtl,
                            px: span.map_or(16., |s| s.font_size().get()),
                            color: solid(span.and_then(|s| s.fill())),
                        });
                    }
                }
            }
            cursor = start + authored.len();
            offset += bytes.len();
        }
        result.count += project(offset);
    }
    Some(result)
}
