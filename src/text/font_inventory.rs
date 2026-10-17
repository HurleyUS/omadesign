//! Find Font reads authored requests and the existing composer's actual font data.
use super::*;
use std::{collections::BTreeSet, ops::Range};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Request {
    Stored(String),
    Variable {
        source: Box<Request>,
        coordinates: Vec<([u8; 4], u32)>,
    },
    Named {
        family: String,
        weight: u16,
        slant: u8,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Use {
    pub range: Range<usize>,
    pub request: Request,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Exact,
    Substitute,
    Missing,
}
#[derive(Clone, Debug)]
pub(crate) struct Info {
    pub identity: Option<String>,
    pub family: String,
    pub style: String,
    pub version: Option<String>,
    pub status: Status,
    pub resolved: Option<String>,
    pub missing_glyphs: usize,
}

pub(super) fn name(face: &rustybuzz::ttf_parser::Face<'_>, ids: &[u16]) -> Option<String> {
    ids.iter().find_map(|id| {
        face.names()
            .into_iter()
            .filter(|n| {
                n.name_id == *id
                    && n.language() == rustybuzz::ttf_parser::Language::English_UnitedStates
            })
            .chain(face.names().into_iter().filter(|n| n.name_id == *id))
            .filter_map(|n| n.to_string())
            .map(|n| n.trim().to_owned())
            .find(|n| !n.is_empty())
    })
}
pub(super) fn metadata(
    data: &[u8],
    index: u32,
    chars: &BTreeSet<char>,
) -> Option<(String, String, Option<String>, usize)> {
    let face = rustybuzz::ttf_parser::Face::parse(data, index).ok()?;
    let family = name(&face, &[16, 1])?;
    let style = name(&face, &[17, 2]).unwrap_or_else(|| "Regular".into());
    let version = name(&face, &[5]);
    let missing = chars
        .iter()
        .filter(|c| {
            !c.is_whitespace()
                && !c.is_control()
                && face.glyph_index(**c).is_none_or(|id| id.0 == 0)
        })
        .count();
    Some((family, style, version, missing))
}
fn stored(id: &str, chars: &BTreeSet<char>) -> Option<(String, String, Option<String>, usize)> {
    let run = TypeRun {
        font: id.into(),
        ..Default::default()
    };
    metadata(&font_bytes_for(&run)?, 0, chars)
}
fn present(id: &str) -> bool {
    id.is_empty() && default_path().is_some()
        || id.starts_with("omatype:") && project_font_loaded(id)
        || !id.is_empty() && Path::new(id).is_file()
}
fn family_query(family: &str) -> usvg::fontdb::Family<'_> {
    use usvg::fontdb::Family;
    match family.to_ascii_lowercase().as_str() {
        "serif" | "ui-serif" => Family::Serif,
        "sans-serif" | "system-ui" | "ui-sans-serif" | "ui-rounded" => Family::SansSerif,
        "monospace" | "ui-monospace" => Family::Monospace,
        "cursive" => Family::Cursive,
        "fantasy" => Family::Fantasy,
        _ => Family::Name(family),
    }
}
/// Describe a requested face, keeping unavailable names distinct from rendered fallback data.
/// Args: authored request, the run's existing fallback reference and unique text characters.
/// Returns real font names, version, substitution status and missing-glyph count.
pub(crate) fn describe(request: &Request, fallback: &str, chars: &BTreeSet<char>) -> Info {
    let catalog = super::font_catalog::Catalog::new([fallback.to_owned()]);
    describe_with(&catalog, request, fallback, chars)
}
pub(crate) fn describe_with(
    catalog: &super::font_catalog::Catalog,
    request: &Request,
    fallback: &str,
    chars: &BTreeSet<char>,
) -> Info {
    let absent = |family: String, style: String| {
        let resolved = stored(fallback, chars);
        Info {
            identity: None,
            family,
            style,
            version: None,
            status: Status::Missing,
            resolved: resolved.as_ref().map(|(f, s, _, _)| format!("{f} {s}")),
            missing_glyphs: resolved.map_or_else(
                || {
                    chars
                        .iter()
                        .filter(|c| !c.is_control() && !c.is_whitespace())
                        .count()
                },
                |(_, _, _, n)| n,
            ),
        }
    };
    match request {
        Request::Variable {
            source,
            coordinates,
        } => {
            let mut info = describe_with(catalog, source, fallback, chars);
            let shown = coordinates
                .iter()
                .map(|(tag, value)| {
                    format!(
                        "{} {}",
                        String::from_utf8_lossy(tag),
                        f32::from_bits(*value)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let named = match source.as_ref() {
                Request::Stored(reference) => font_bytes_for(&TypeRun { font:reference.clone(), ..Default::default() })
                    .and_then(|bytes| {
                        let face=rustybuzz::ttf_parser::Face::parse(&bytes,0).ok()?;
                        let settings=super::variations::FontVariations::new(coordinates.iter().map(|(t,v)|(*t,f32::from_bits(*v))).collect()).ok()?;
                        super::font_instances::read(&face).1.into_iter().find(|i| super::font_instances::same_point(&face,&settings,&i.variations)).map(|i| i.style)
                    }),
                _=>None,
            };
            info.style = format!("{} ({shown})", named.as_deref().unwrap_or(&info.style));
            info.identity = info.identity.map(|id| format!("{id}:{coordinates:?}"));
            if let Some(resolved) = &mut info.resolved {
                *resolved = format!("{resolved} ({shown})");
            }
            info
        }
        Request::Stored(id) => {
            if present(id)
                && let Some((family, style, version, missing_glyphs)) = stored(id, chars)
            {
                let data = font_bytes_for(&TypeRun {
                    font: id.clone(),
                    ..Default::default()
                })
                .unwrap();
                return Info {
                    identity: Some(format!("{:032x}:0", crate::typography::fingerprint(&data))),
                    resolved: Some(format!("{family} {style}")),
                    family,
                    style,
                    version,
                    status: Status::Exact,
                    missing_glyphs,
                };
            }
            absent(
                if id.starts_with("omatype:") {
                    label_for(id)
                } else {
                    Path::new(id)
                        .file_stem()
                        .map_or_else(|| id.clone(), |s| s.to_string_lossy().into_owned())
                },
                String::new(),
            )
        }
        Request::Named {
            family,
            weight,
            slant,
        } => {
            let db = &catalog.db;
            let style = match slant {
                1 => usvg::fontdb::Style::Italic,
                2 => usvg::fontdb::Style::Oblique,
                _ => usvg::fontdb::Style::Normal,
            };
            if let Some(id) = db.query(&usvg::fontdb::Query {
                families: &[family_query(family)],
                weight: usvg::fontdb::Weight(*weight),
                style,
                ..Default::default()
            }) && let Some(info) = db.face(id)
                && let Some((resolved_family, resolved_style, version, missing_glyphs)) = db
                    .with_face_data(id, |data, index| {
                        metadata(data, index, chars).map(|m| {
                            (
                                m,
                                format!("{:032x}:{index}", crate::typography::fingerprint(data)),
                            )
                        })
                    })
                    .flatten()
                    .map(|(m, key)| (m.0, m.1, m.2, (m.3, key)))
            {
                let (missing_glyphs, key) = missing_glyphs;
                return Info {
                    identity: Some(key),
                    family: if info.weight.0 == *weight && info.style == style {
                        resolved_family.clone()
                    } else {
                        family.clone()
                    },
                    style: if info.weight.0 == *weight && info.style == style {
                        resolved_style.clone()
                    } else {
                        requested_style(*weight, *slant)
                    },
                    version,
                    status: if info.weight.0 == *weight && info.style == style {
                        Status::Exact
                    } else {
                        Status::Substitute
                    },
                    resolved: Some(format!("{resolved_family} {resolved_style}")),
                    missing_glyphs,
                };
            }
            absent(family.clone(), requested_style(*weight, *slant))
        }
    }
}
fn requested_style(weight: u16, slant: u8) -> String {
    let weight = match weight {
        100 => "Thin".into(),
        200 => "Extra Light".into(),
        300 => "Light".into(),
        400 => "Regular".into(),
        500 => "Medium".into(),
        600 => "Semi Bold".into(),
        700 => "Bold".into(),
        800 => "Extra Bold".into(),
        900 => "Black".into(),
        n => n.to_string(),
    };
    format!(
        "{weight}{}",
        match slant {
            1 => " Italic",
            2 => " Oblique",
            _ => "",
        }
    )
}
fn native_request(font: &str, axes: &super::variations::FontVariations) -> Request {
    let source = Request::Stored(font.into());
    if axes.is_empty() {
        source
    } else {
        Request::Variable {
            source: Box::new(source),
            coordinates: axes.key(),
        }
    }
}
fn first_family(value: &str) -> String {
    let mut quote = None;
    let mut escaped = false;
    let mut out = String::new();
    for c in value.chars() {
        if escaped {
            out.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if quote == Some(c) {
            quote = None;
        } else if quote.is_none() && matches!(c, '\'' | '"') {
            quote = Some(c);
        } else if quote.is_none() && c == ',' {
            break;
        } else {
            out.push(c);
        }
    }
    out.trim().to_owned()
}
fn named(attributes: &std::collections::BTreeMap<String, String>, inherited: &Request) -> Request {
    let (mut family, mut weight, mut slant) = match inherited {
        Request::Named {
            family,
            weight,
            slant,
        } => (family.clone(), *weight, *slant),
        _ => (String::new(), 400, 0),
    };
    if let Some(value) = attributes
        .get("font-family")
        .filter(|v| !matches!(v.as_str(), "inherit" | "unset"))
    {
        family = first_family(value);
    }
    if let Some(value) = attributes.get("font-weight") {
        weight = match value.as_str() {
            "normal" | "initial" => 400,
            "bold" => 700,
            "bolder" => {
                if weight < 350 {
                    400
                } else if weight < 550 {
                    700
                } else {
                    900
                }
            }
            "lighter" => {
                if weight < 550 {
                    100
                } else if weight < 750 {
                    400
                } else {
                    700
                }
            }
            _ => value
                .parse()
                .ok()
                .filter(|v| (1..=1000).contains(v))
                .unwrap_or(weight),
        };
    }
    if let Some(value) = attributes.get("font-style") {
        slant = match value.as_str() {
            "normal" | "initial" => 0,
            "italic" => 1,
            "oblique" => 2,
            _ => slant,
        };
    }
    if family.is_empty() {
        inherited.clone()
    } else {
        Request::Named {
            family,
            weight,
            slant,
        }
    }
}
/// Read effective authored font intervals without confusing SVG fallback faces for their requests.
/// Args: native rich text. Returns complete bounded scalar intervals or a malformed-style error.
pub(crate) fn uses(run: &TypeRun) -> Result<Vec<Use>, String> {
    let count = run.content.chars().count();
    let mut last = 0;
    for span in &run.spans {
        if span.start < last || span.start > span.end || span.end > count {
            return Err("Text has invalid font style ranges".into());
        }
        last = span.end;
    }
    let base = native_request(&run.font, &run.font_variations);
    let mut source = vec![];
    if let Some(svg) = &run.svg_text
        && run.font == svg.base_font
        && run.font_variations.is_empty()
    {
        fn walk(
            node: &svg_type::TextNode,
            inherited: &Request,
            at: &mut usize,
            limit: usize,
            out: &mut Vec<Use>,
        ) -> Result<(), String> {
            match node {
                svg_type::TextNode::Characters { count } => {
                    let end = at
                        .checked_add(*count)
                        .filter(|n| *n <= limit)
                        .ok_or("SVG font ranges exceed the text")?;
                    if end > *at {
                        out.push(Use {
                            range: *at..end,
                            request: inherited.clone(),
                        });
                    }
                    *at = end;
                }
                svg_type::TextNode::Element {
                    attributes,
                    children,
                    ..
                } => {
                    let request = named(attributes, inherited);
                    for child in children {
                        walk(child, &request, at, limit, out)?;
                    }
                }
            }
            Ok(())
        }
        let mut at = 0;
        walk(&svg.root, &base, &mut at, count, &mut source)?;
        if at != count {
            return Err("SVG font ranges do not cover the text".into());
        }
    } else {
        source.push(Use {
            range: 0..count,
            request: base.clone(),
        });
    }
    if count == 0 {
        let request = run
            .svg_text
            .as_ref()
            .filter(|svg| run.font == svg.base_font && run.font_variations.is_empty())
            .and_then(|svg| match &svg.root {
                svg_type::TextNode::Element { attributes, .. } => Some(named(attributes, &base)),
                _ => None,
            })
            .unwrap_or(base);
        return Ok(vec![Use {
            range: 0..0,
            request,
        }]);
    }
    let mut edges: BTreeSet<_> = source
        .iter()
        .flat_map(|u| [u.range.start, u.range.end])
        .collect();
    edges.extend(run.spans.iter().flat_map(|s| [s.start, s.end]));
    let edges: Vec<_> = edges.into_iter().collect();
    let mut span_at = 0;
    let mut source_at = 0;
    Ok(edges
        .windows(2)
        .filter_map(|pair| {
            let range = pair[0]..pair[1];
            if range.is_empty() {
                return None;
            }
            while run.spans.get(span_at).is_some_and(|s| s.end <= range.start) {
                span_at += 1;
            }
            while source
                .get(source_at)
                .is_some_and(|s| s.range.end <= range.start)
            {
                source_at += 1;
            }
            let request = run
                .spans
                .get(span_at)
                .filter(|s| s.start <= range.start && range.start < s.end)
                .filter(|s| s.font.is_some() || s.font_variations.is_some())
                .map(|s| {
                    native_request(
                        s.font.as_deref().unwrap_or(&run.font),
                        &super::variations::at(run, range.start),
                    )
                })
                .or_else(|| {
                    source
                        .get(source_at)
                        .filter(|u| u.range.start <= range.start && range.start < u.range.end)
                        .map(|u| u.request.clone())
                })?;
            Some(Use { range, request })
        })
        .collect())
}
/// Replace one reported face through native font overrides without touching other character metadata.
/// Args: text, exact authored source requests and available replacement reference. Returns whether text changed.
pub(crate) fn replace(run: &mut TypeRun, from: &[Request], to: &str) -> Result<bool, String> {
    replace_at(run, from, to, &Default::default())
}
/// Replace matching authored font intervals with an original face at one editable design-space point.
/// Args: native text, source requests, available font reference and validated coordinates.
/// Returns whether text changed, preserving unmatched character metadata and ranges.
pub(crate) fn replace_at(run: &mut TypeRun, from: &[Request], to: &str, point: &super::variations::FontVariations) -> Result<bool, String> {
    if !present(to) || stored(to, &BTreeSet::new()).is_none() {
        return Err("Choose an available font".into());
    }
    let uses = uses(run)?;
    let ranges: Vec<_> = uses
        .iter()
        .filter(|u| from.contains(&u.request) && u.request != native_request(to, point))
        .map(|u| u.range.clone())
        .collect();
    if ranges.is_empty() {
        return Ok(false);
    }
    let before = run.clone();
    fn has_axis_css(node: &svg_type::TextNode) -> bool {
        match node {
            svg_type::TextNode::Characters { .. } => false,
            svg_type::TextNode::Element { attributes, children, .. } => {
                attributes.get("font-variation-settings")
                    .is_some_and(|value| !matches!(value.trim(), "" | "normal"))
                    || children.iter().any(has_axis_css)
            }
        }
    }
    let reset_svg = run.svg_text.as_ref().is_some_and(|svg| {
        !run.font_variations.is_empty() || has_axis_css(&svg.root)
    });
    if run
        .svg_text
        .as_ref()
        .is_none_or(|svg| run.font != svg.base_font)
        && from.contains(&native_request(&run.font, &run.font_variations))
        && !run.spans.iter().any(|s| s.font_variations.is_some())
        && !reset_svg
    {
        run.font = to.into();
        run.font_variations = point.clone();
        for span in &mut run.spans {
            if span
                .font
                .as_ref()
                .is_some_and(|f| from.contains(&Request::Stored(f.clone())))
            {
                span.font = Some(to.into());
                span.font_variations = (!point.is_empty()).then(|| point.clone());
            }
        }
    } else {
        let count = run.content.chars().count();
        if count == 0 {
            run.font = to.into();
            run.font_variations = point.clone();
        } else {
            let original = run.character_style_runs(0, count);
            let mut span_at = 0;
            let mut changed = Vec::with_capacity(uses.len());
            for usage in uses {
                while original[span_at].end <= usage.range.start {
                    span_at += 1;
                }
                let mut span = original[span_at].clone();
                span.start = usage.range.start;
                span.end = usage.range.end;
                if from.contains(&usage.request) && usage.request != native_request(to, point) {
                    span.font = Some(to.into());
                    span.font_variations = (reset_svg || !point.is_empty()).then(|| point.clone());
                }
                changed.push(span);
            }
            run.replace_character_styles(0, count, changed);
        }
    }
    if run == &before {
        return Ok(false);
    }
    run.layout = None;
    Ok(true)
}
