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
    let mut pairs: Vec<_> = run
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
    for at in run.spans.iter().flat_map(|s|[s.start,s.end]).filter(|&at|at>0&&at<count) {
        let style=|index|run.spans.iter().find(|s|s.start<=index&&index<s.end);
        let mode=|index|style(index).and_then(|s|s.kerning);
        let scale=|index|style(index).map(|s|(s.hscale,s.vscale)).unwrap_or_default();
        if (mode(at-1)!=mode(at) && [mode(at-1),mode(at)].contains(&Some(crate::geom::KernMode::Optical)) || scale(at-1)!=scale(at)) && original.is_some_and(|l|l.clusters.iter().any(|r|r.start<at&&at<r.end)) {pairs.push(at);}
    }
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
    if let Some(value) = run
        .spans
        .iter()
        .find(|s| s.start <= at && at < s.end)
        .and_then(|s| s.leading)
    {
        return Some(value);
    }
    let flow = svg.flow.as_ref()?;
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

/// Apply accepted native pair and optical spacing to resolved source character positions.
/// Args: native run, unmodified SVG, source range start and visible Unicode. Returns editable SVG.
pub(super) fn pairs(run: &TypeRun, source: String, from: usize, content: &str) -> Option<String> {
    let leading:Vec<_>=run.spans.iter().filter(|_|run.svg_text.as_ref().is_some_and(|s|s.flow.is_none()))
        .filter_map(|s|{let a=s.start.max(from);let b=s.end.min(from+content.chars().count());(a<b).then(||s.leading.map(|v|(a-from,b-from,v))).flatten()}).collect();
    if run
        .manual_kern
        .range(from..from + content.chars().count())
        .next()
        .is_none() && !run.spans.iter().any(|s|s.start<from+content.chars().count()&&from<s.end&&s.kerning==Some(crate::geom::KernMode::Optical)) && leading.is_empty()
    {
        return Some(source);
    }
    let layout = layout_source(run, source.clone(), content)?;
    let optical=super::optical::adjustments(run,&layout,from);
    let count = layout.addressable_count;
    let mut dx = vec![0.; count];
    let mut dy = vec![0.; count];
    for &(at,x,y) in &layout.control_offsets {dx[at]=x;dy[at]=y;}
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
    let base_dx=dx.clone();let base_dy=dy.clone();
    for c in &layout.characters {if let Some(amount)=optical.get(&c.ordinal){let target=if c.vertical{&mut dy}else{&mut dx};target[c.ordinal]+=amount;}}
    if !leading.is_empty(){super::line_spacing::apply(run,&layout,from,&mut dx,&mut dy)?;}
    let ranges:Vec<_>=run.spans.iter().filter(|s|s.kerning==Some(crate::geom::KernMode::Optical)).filter_map(|s|{let a=s.start.max(from);let b=s.end.min(from+content.chars().count());(a<b).then_some([a-from,b-from])}).collect();
    let metadata=(!ranges.is_empty() || !leading.is_empty()).then(||serde_json::to_string(&Offsets{version:1,length:content.chars().count(),dx:base_dx,dy:base_dy,optical:ranges,leading}).ok()).flatten();
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
        format!(" dx=\"{}\" dy=\"{}\"{}", values(dx), values(dy),metadata.map_or(String::new(),|value|format!(" {META}=\"{}\"",escape(&value)))),
    ));
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut result = source;
    for (range, value) in edits {
        result.replace_range(range, &value);
    }
    Some(result)
}

pub(super) const META:&str="data-oma-character-offsets";
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Offsets {version:u8,length:usize,dx:Vec<f32>,dy:Vec<f32>,optical:Vec<[usize;2]>,#[serde(default,skip_serializing_if="Vec::is_empty")]leading:Vec<(usize,usize,crate::geom::Leading)>}

/// Restore accepted optical intent from bounded metadata while keeping standard SVG placement visible.
/// Args: captured native text. Returns an error before changing malformed source metadata.
pub(crate) fn restore_offsets(run:&mut TypeRun)->Result<(),String> {
    let Some(svg)=&run.svg_text else{return Ok(());};
    let TextNode::Element{attributes,..}=&svg.root else{return Ok(());};
    let Some(value)=attributes.get(META) else{return Ok(());};
    let count=run.content.chars().count();
    let bound=count.checked_mul(200).and_then(|n|n.checked_add(1024)).unwrap_or(usize::MAX).min(256*1024*1024);
    if value.len()>bound{return Err("SVG optical positioning metadata exceeds source bounds".into());}
    let offsets:Offsets=serde_json::from_str(value).map_err(|e|format!("Invalid SVG optical positioning: {e}"))?;
    if offsets.version!=1 || offsets.length!=count || offsets.dx.len()!=offsets.dy.len() || offsets.dx.len()>count || offsets.dx.iter().chain(&offsets.dy).any(|v|!v.is_finite()) || offsets.optical.len()>count || offsets.optical.iter().any(|[a,b]|a>=b||*b>count) || offsets.optical.windows(2).any(|p|p[0][1]>p[1][0]) {return Err("Invalid SVG optical positioning bounds".into());}
    if offsets.leading.len()>count || offsets.leading.iter().any(|(a,b,v)|{let(crate::geom::Leading::Auto(v)|crate::geom::Leading::Fixed(v))=v; a>=b || *b>count || !v.is_finite() || *v<0.}) || offsets.leading.windows(2).any(|p|p[0].1>p[1].0) {return Err("Invalid SVG line spacing bounds".into());}
    fn clear(node:&mut TextNode){if let TextNode::Element{attributes,children,..}=node {attributes.remove("dx");attributes.remove("dy");for child in children{clear(child);}}}
    let svg=run.svg_text.as_mut().unwrap();
    if svg.flow.is_none(){clear(&mut svg.root);}
    if let TextNode::Element{attributes,..}=&mut svg.root {attributes.remove(META);if svg.flow.is_none(){for(key,values)in[("dx",offsets.dx),("dy",offsets.dy)]{attributes.insert(key.into(),values.into_iter().map(|v|v.to_string()).collect::<Vec<_>>().join(" "));}}}
    for[a,b]in offsets.optical {run.set_character_style(a,b,|style|style.kerning=Some(crate::geom::KernMode::Optical));}
    for(a,b,value)in offsets.leading {run.set_character_style(a,b,|style|style.leading=Some(value));}
    Ok(())
}
