//! Native Type content commands share the current story and ordinary text history.
use super::*;
use crate::{geom::CharSpan, text::CharacterRewrite};
use std::ops::Range;
#[path = "type_fonts.rs"]
pub(crate) mod fonts;
#[path = "type_search.rs"]
pub(crate) mod search;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextScope {
    Selection,
    Document,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextCase {
    Upper,
    Lower,
    Title,
    Sentence,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SmartPunctuation {
    pub quotes: bool,
    pub dashes: bool,
    pub ellipsis: bool,
    pub double: [char; 2],
    pub single: [char; 2],
}
impl Default for SmartPunctuation {
    fn default() -> Self {
        Self {
            quotes: true,
            dashes: true,
            ellipsis: true,
            double: ['“', '”'],
            single: ['‘', '’'],
        }
    }
}
#[derive(Clone)]
pub(crate) enum TypeCommand {
    Case(TextCase),
    Punctuation(SmartPunctuation),
    Insert(String),
    Replace(search::Replacement),
    Font(fonts::Replacement),
    /// Signed pixel change applied to each authored font size in the target.
    Size(f32),
    /// Track the first paragraph of area type so it fills the frame width.
    FitHeadline,
    /// Replace the story with stand-in copy. Area type fills its frame. Other type gets one sentence.
    Placeholder,
    /// Apply an installed family, a remembered file, or that family's own bold or italic face.
    Face(TypeFace),
}
/// A Type menu font or style choice. The face has to be one the family actually ships.
#[derive(Clone)]
pub(crate) enum TypeFace {
    /// The family's own regular face.
    Family(String),
    /// A remembered font file. A missing file stays missing.
    Recent(String),
    /// The family's own bold face, or back to its regular weight.
    Bold,
    /// The family's own italic face, or back upright.
    Italic,
}
pub(crate) const SPECIAL_CHARACTERS: &[(&str, &str)] = &[
    ("Bullet •", "•"),
    ("Copyright ©", "©"),
    ("Registered ®", "®"),
    ("Trademark ™", "™"),
    ("Ellipsis …", "…"),
    ("Paragraph mark ¶", "¶"),
    ("Section §", "§"),
    ("Em dash —", "—"),
    ("En dash –", "–"),
    ("Discretionary hyphen", "\u{00ad}"),
    ("Nonbreaking hyphen", "\u{2011}"),
    ("Opening double quote “", "“"),
    ("Closing double quote ”", "”"),
    ("Opening single quote ‘", "‘"),
    ("Closing single quote ’", "’"),
];
pub(crate) const WHITESPACE_CHARACTERS: &[(&str, &str)] = &[
    ("Em space", "\u{2003}"),
    ("En space", "\u{2002}"),
    ("Hair space", "\u{200a}"),
    ("Sixth space", "\u{2006}"),
    ("Thin space", "\u{2009}"),
    ("Nonbreaking space", "\u{00a0}"),
    ("Figure space", "\u{2007}"),
    ("Punctuation space", "\u{2008}"),
    ("Third space", "\u{2004}"),
    ("Quarter space", "\u{2005}"),
    ("Tab", "\t"),
];
pub(crate) const BREAK_CHARACTERS: &[(&str, &str)] =
    &[("Forced line break", "\n"), ("Paragraph return", "\n")];
#[derive(Clone, PartialEq)]
struct EditOwner {
    hit: (usize, u64),
    frame: (usize, u64),
    caret: usize,
    anchor: usize,
    pending: Option<CharSpan>,
    preedit: Option<(String, Option<Range<usize>>)>,
    interrupt: bool,
}
impl EditOwner {
    fn new(edit: &TypeEdit) -> Self {
        Self {
            hit: (edit.layer, edit.id),
            frame: edit.frame,
            caret: edit.caret,
            anchor: edit.anchor,
            pending: edit.pending_style.clone(),
            preedit: edit
                .ime
                .preedit
                .as_ref()
                .map(|p| (p.text.clone(), p.active.clone())),
            interrupt: edit.ime.interrupt,
        }
    }
}
#[derive(Clone)]
pub(crate) struct TypeCommandTarget {
    document: String,
    generation: u64,
    selection: Vec<(usize, u64)>,
    layer: Option<u64>,
    scope: TextScope,
    edit: Option<EditOwner>,
    hits: Vec<(usize, u64)>,
}

/// Map a caret through the same original character rewrites used for the content.
/// Args: original caret and ordered edits. Returns its new character position.
fn rewritten_caret(at: usize, edits: &[CharacterRewrite]) -> usize {
    let mut old = 0;
    let mut new = 0;
    for e in edits {
        if at <= e.start {
            break;
        }
        if at < e.end {
            return new + e.start - old + e.text.chars().count();
        }
        new += e.start - old + e.text.chars().count();
        old = e.end;
    }
    new + at - old
}

/// Split a style name into bold, regular weight, italic, and the other words.
/// Args: OpenType style name. Returns those four parts. A narrow face keeps the word "narrow".
fn style_parts(style: &str) -> (bool, bool, bool, Vec<String>) {
    const WEIGHTS: &[&str] = &[
        "thin",
        "hairline",
        "extralight",
        "ultralight",
        "light",
        "book",
        "medium",
        "semibold",
        "demibold",
        "demi",
        "bold",
        "extrabold",
        "ultrabold",
        "heavy",
        "black",
        "ultra",
    ];
    const PLAIN: &[&str] = &["regular", "normal", "roman", "plain", "upright"];
    let words: Vec<String> = style
        .split([' ', '-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| word.to_ascii_lowercase())
        .collect();
    let italic = words
        .iter()
        .any(|word| word == "italic" || word == "oblique");
    let bold = words.iter().any(|word| word == "bold");
    let regular = !words.iter().any(|word| WEIGHTS.contains(&word.as_str()));
    let mut rest: Vec<String> = words
        .into_iter()
        .filter(|word| {
            !WEIGHTS.contains(&word.as_str())
                && !PLAIN.contains(&word.as_str())
                && word != "italic"
                && word != "oblique"
        })
        .collect();
    rest.sort();
    (bold, regular, italic, rest)
}

/// Pick the family's own style name for a bold and italic choice.
/// Args: style names the family ships, whether bold is wanted, whether italic is wanted, and the style to keep width words from.
/// Returns the shortest matching name. A family without that face returns none.
fn pick_style<'a>(styles: &'a [String], bold: bool, italic: bool, like: &str) -> Option<&'a str> {
    let (.., keep) = style_parts(like);
    styles
        .iter()
        .map(String::as_str)
        .filter(|style| {
            let (is_bold, regular, is_italic, rest) = style_parts(style);
            is_italic == italic && (if bold { is_bold } else { regular }) && rest == keep
        })
        .min_by_key(|style| style.len())
}

/// Name the face Bold or Italic asked for.
/// Args: whether bold is wanted and whether italic is wanted. Returns the style words used in the refusal.
fn wanted_style(bold: bool, italic: bool) -> &'static str {
    match (bold, italic) {
        (true, true) => "Bold Italic",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (false, false) => "Regular",
    }
}

/// Read the family and style stored on a font reference.
/// Args: font file or project font id. Returns those names, or an error when the file is missing.
fn authored_face(reference: &str) -> Result<(String, String), String> {
    if reference.is_empty() {
        return Err("Choose an available font".into());
    }
    let info = crate::text::font_inventory::describe(
        &crate::text::font_inventory::Request::Stored(reference.into()),
        reference,
        &std::collections::BTreeSet::new(),
    );
    if info.status != crate::text::font_inventory::Status::Exact {
        return Err("Choose an available font".into());
    }
    Ok((info.family, info.style))
}

/// Bind one named style of a family to the portable font used by typed text.
/// Args: catalog, family and exact style name. Returns that face's project font id.
fn bind_style(
    catalog: &crate::text::font_catalog::Catalog,
    family: &str,
    style: &str,
) -> Result<String, String> {
    let choice = catalog.choice(
        family,
        Some(style),
        &crate::text::font_inventory::Request::Named {
            family: family.into(),
            weight: 400,
            slant: 0,
        },
    )?;
    catalog.reference(&choice)
}

/// Resolve Type > Font to the family's own regular face.
/// Args: catalog and family name. Returns the portable font id, or an error when that face is absent.
fn regular_reference(
    catalog: &crate::text::font_catalog::Catalog,
    family: &str,
) -> Result<String, String> {
    if family.is_empty()
        || !catalog
            .families()
            .iter()
            .any(|name| name.eq_ignore_ascii_case(family))
    {
        return Err("Choose an available font family".into());
    }
    let names: Vec<_> = catalog
        .styles(family)
        .into_iter()
        .map(|choice| choice.style)
        .collect();
    let style = pick_style(&names, false, false, "Regular")
        .ok_or_else(|| format!("{family} has no Regular style"))?;
    bind_style(catalog, family, style)
}

/// Resolve Type > Bold or Type > Italic from the story's own face.
/// Args: catalog, the story font, and bold or italic. Returns the portable font id.
fn style_reference(
    catalog: &crate::text::font_catalog::Catalog,
    reference: &str,
    face: &TypeFace,
) -> Result<String, String> {
    let (family, style) = authored_face(reference)?;
    let (bold, _, italic, _) = style_parts(&style);
    let (want_bold, want_italic) = match face {
        TypeFace::Bold => (!bold, italic),
        TypeFace::Italic => (bold, !italic),
        TypeFace::Family(_) | TypeFace::Recent(_) => {
            return Err("Choose an available font".into());
        }
    };
    let names: Vec<_> = catalog
        .styles(&family)
        .into_iter()
        .map(|choice| choice.style)
        .collect();
    let picked = pick_style(&names, want_bold, want_italic, &style).ok_or_else(|| {
        format!(
            "{family} has no {} style",
            wanted_style(want_bold, want_italic)
        )
    })?;
    bind_style(catalog, &family, picked)
}

/// Replace every authored face in a story with one available font.
/// Args: story and portable font id or file path. Returns whether the story changed.
fn replace_story_font(run: &mut TypeRun, to: &str) -> Result<bool, String> {
    let mut from = Vec::new();
    for usage in crate::text::font_inventory::uses(run)? {
        if !from.contains(&usage.request) {
            from.push(usage.request);
        }
    }
    crate::text::font_inventory::replace_at(run, &from, to, &Default::default())
}

/// Rewrite content while carrying word/sentence/quote context across styled intervals.
/// Args: source run, original range and command. Returns only actual character replacements.
fn rewrites(run: &TypeRun, range: Range<usize>, command: &TypeCommand) -> Vec<CharacterRewrite> {
    if matches!(
        command,
        TypeCommand::Font(_)
            | TypeCommand::Size(_)
            | TypeCommand::FitHeadline
            | TypeCommand::Placeholder
            | TypeCommand::Face(_)
    ) {
        return vec![];
    }
    if let TypeCommand::Replace(replacement) = command {
        return search::rewrites(run, range, replacement);
    }
    let chars: Vec<_> = run.content.chars().collect();
    let mut edges: HashSet<_> = run
        .character_style_runs(range.start, range.end)
        .iter()
        .flat_map(|s| [s.start, s.end])
        .collect();
    fn leaves(node: &crate::text::svg_type::TextNode, at: &mut usize, edges: &mut HashSet<usize>) {
        match node {
            crate::text::svg_type::TextNode::Characters { count } => {
                *at += count;
                edges.insert(*at);
            }
            crate::text::svg_type::TextNode::Element { children, .. } => {
                for child in children {
                    leaves(child, at, edges);
                }
            }
        }
    }
    if let Some(svg) = &run.svg_text {
        leaves(&svg.root, &mut 0, &mut edges);
    }
    let mut result = Vec::new();
    let mut previous = if matches!(command, TypeCommand::Punctuation(_)) {
        range
            .start
            .checked_sub(1)
            .and_then(|i| chars.get(i).copied())
    } else {
        None
    };
    let mut sentence = true;
    let mut index = range.start;
    while index < range.end {
        let c = chars[index];
        let mut end = index + 1;
        let text = match command {
            TypeCommand::Case(case) => match case {
                TextCase::Upper => c.to_uppercase().collect(),
                TextCase::Lower => c.to_lowercase().collect(),
                TextCase::Title => {
                    if previous.is_none_or(|p: char| {
                        p.is_whitespace() || matches!(p, '-' | '(' | '"' | '“' | '/')
                    }) {
                        c.to_uppercase().collect()
                    } else {
                        c.to_lowercase().collect()
                    }
                }
                TextCase::Sentence => {
                    let out = if c.is_alphabetic() && sentence {
                        sentence = false;
                        c.to_uppercase().collect()
                    } else {
                        c.to_lowercase().collect()
                    };
                    if matches!(c, '.' | '!' | '?' | '\n') {
                        sentence = true;
                    }
                    out
                }
            },
            TypeCommand::Punctuation(options) => {
                let matches = |token: &[char]| {
                    index + token.len() <= range.end
                        && chars[index..index + token.len()] == *token
                        && (index + 1..index + token.len()).all(|at| !edges.contains(&at))
                };
                if options.dashes && matches(&['-', '-', '-']) {
                    end += 2;
                    "—".into()
                } else if options.dashes && matches(&['-', '-']) {
                    end += 1;
                    "–".into()
                } else if options.ellipsis && matches(&['.', '.', '.']) {
                    end += 2;
                    "…".into()
                } else if options.quotes && matches!(c, '\'' | '"') {
                    let open = previous.is_none_or(|p| {
                        p.is_whitespace()
                            || matches!(p, '(' | '[' | '{' | '—' | '–')
                            || p == options.double[0]
                            || p == options.single[0]
                    });
                    let pair = if c == '"' {
                        options.double
                    } else {
                        options.single
                    };
                    pair[usize::from(!open)].to_string()
                } else {
                    c.to_string()
                }
            }
            TypeCommand::Insert(_)
            | TypeCommand::Replace(_)
            | TypeCommand::Font(_)
            | TypeCommand::Size(_)
            | TypeCommand::FitHeadline
            | TypeCommand::Placeholder
            | TypeCommand::Face(_) => {
                unreachable!()
            }
        };
        previous = text.chars().last().or(previous);
        if !chars[index..end].iter().copied().eq(text.chars()) {
            result.push(CharacterRewrite {
                start: index,
                end,
                text,
            });
        }
        index = end;
    }
    result
}

/// Step the authored font size of each character in a range.
/// Args: text run, character range, and signed pixel step. Returns whether a stored size changed.
/// A size already at 0.1 or 1296 pixels stays there. A non-finite size refuses before any character changes.
fn step_authored_size(run: &mut TypeRun, range: Range<usize>, delta: f32) -> Result<bool, String> {
    const MIN_PX: f32 = 0.1;
    const MAX_PX: f32 = 1296.;
    if !delta.is_finite() {
        return Err("Font size step must be a finite number of pixels".into());
    }
    let count = run.content.chars().count();
    let start = range.start.min(count);
    let end = range.end.min(count).max(start);
    if start == end {
        return Ok(false);
    }
    let sizes: Vec<f32> = (start..end)
        .map(|index| run.character_style(index).px.unwrap_or(run.px))
        .collect();
    if sizes.iter().any(|px| !px.is_finite()) {
        return Err("This text has no finite font size to change".into());
    }
    let mut changed = false;
    let mut index = 0;
    while index < sizes.len() {
        let px = sizes[index];
        let mut group = index + 1;
        while group < sizes.len() && (sizes[group] - px).abs() < 0.001 {
            group += 1;
        }
        let next = (px + delta).clamp(MIN_PX, MAX_PX);
        if (next - px).abs() >= 0.001 {
            run.set_character_style(start + index, start + group, |span| span.px = Some(next));
            changed = true;
        }
        index = group;
    }
    if changed {
        run.layout = None;
    }
    Ok(changed)
}

/// Area type is a text frame that is not following a path.
/// Args: text run. Returns whether Fit Headline can track it.
fn area_text(run: &TypeRun) -> bool {
    run.frame.is_some() && run.on_path.is_none()
}

/// Width of one paragraph measured as point type at a uniform tracking value.
/// Args: source run, character end of that paragraph, and tracking in thousandths of an em.
/// Returns the composed width. An empty paragraph has no width.
fn headline_width(run: &TypeRun, end: usize, tracking: f32) -> Option<f32> {
    if end == 0 || !tracking.is_finite() {
        return None;
    }
    let mut head = run.clone();
    head.content = run.content.chars().take(end).collect();
    head.wrap_width = None;
    head.frame = None;
    head.on_path = None;
    head.thread = None;
    head.layout = None;
    head.align = crate::geom::TextAlign::Start;
    if let Some(paragraph) = head.paragraphs.first_mut() {
        paragraph.align = crate::geom::TextAlign::Start;
    }
    head.set_character_style(0, end, |span| span.tracking = Some(tracking));
    Some(
        crate::text::compose(&head)
            .first()
            .map(|line| line.width)
            .unwrap_or(0.),
    )
}

/// Usable width of the first line already laid into the frame.
/// Args: area-text run. Returns that span in pixels, or a readable error when the frame is not finite.
fn headline_target(run: &TypeRun) -> Result<f32, String> {
    let frame = run.frame.as_ref().ok_or("Select area type")?;
    let inner = frame.size.x - frame.inset[1] - frame.inset[3];
    if !frame.size.x.is_finite()
        || !frame.size.y.is_finite()
        || frame.inset.iter().any(|value| !value.is_finite())
        || !inner.is_finite()
    {
        return Err("The type frame has no finite width".into());
    }
    let layout = crate::text_geometry::layout_frame(run, run, 0, &[], false);
    let laid = layout
        .export_lines
        .first()
        .and_then(|line| line.run.wrap_width)
        .filter(|width| width.is_finite() && *width > 0.);
    Ok(laid.unwrap_or(inner.max(0.)))
}

/// Track the first paragraph of area type until it fills the frame width.
/// Args: text run. Returns whether stored character tracking changed.
/// Point type, path type, an empty first line, and a zero-width frame stay unchanged.
/// Tracking remains between -1000 and 10000 thousandths of an em, a hair under the frame so the line does not wrap.
fn fit_authored_headline(run: &mut TypeRun) -> Result<bool, String> {
    if !area_text(run) {
        return Ok(false);
    }
    let end = run
        .content
        .chars()
        .position(|c| c == '\n')
        .unwrap_or(run.content.chars().count());
    if end == 0 {
        return Ok(false);
    }
    let target = f64::from(headline_target(run)?);
    if target <= 0. {
        return Ok(false);
    }
    let w0 = f64::from(headline_width(run, end, 0.).unwrap_or(0.));
    let w1 = f64::from(headline_width(run, end, 100.).unwrap_or(0.));
    let slope = (w1 - w0) / 100.;
    if slope <= 1e-9 {
        return Ok(false);
    }
    let tracking = (((target - w0) / slope) - 0.05).clamp(-1000., 10000.);
    if !tracking.is_finite() {
        return Err("Fit headline could not resolve a tracking value".into());
    }
    let tracking = tracking as f32;
    let needs = (0..end).any(|index| {
        run.spans
            .iter()
            .find(|span| span.start <= index && index < span.end)
            .and_then(|span| span.tracking)
            .is_none_or(|value| (value - tracking).abs() >= 0.001)
    });
    if !needs {
        return Ok(false);
    }
    run.set_character_style(0, end, |span| span.tracking = Some(tracking));
    run.layout = None;
    Ok(true)
}

/// Stand-in copy for Type > Fill with placeholder text.
/// Point type uses the first sentence. Area type repeats these words until the frame budget is full.
const PLACEHOLDER: &str = "Placeholder text sits here while the frame is still open. Replace it when the real words are ready. The stand-in shows size, spacing and color without pretending to be the finished line. Headlines and captions start the same way. Add words until the frame is full and the page feels settled.";

/// The one sentence used for point type and path type.
/// Args: none. Returns that sentence, including its period.
fn placeholder_sentence() -> String {
    let sentence = PLACEHOLDER.split(". ").next().unwrap_or(PLACEHOLDER);
    format!("{sentence}.")
}

/// Stored style of the first character, without a resolved font fallback.
/// Args: text run. Returns that style, or the empty style when the story has no characters.
fn first_stored_style(run: &TypeRun) -> crate::geom::CharSpan {
    let mut style = if run.content.is_empty() {
        crate::geom::CharSpan::default()
    } else {
        run.spans
            .iter()
            .find(|span| span.start == 0 && span.end > 0)
            .cloned()
            .unwrap_or_default()
    };
    style.start = 0;
    style.end = 0;
    style
}

/// Build the stand-in string for one story.
/// Args: text run. Returns the replacement. A non-finite area frame is an error and does not choose a string.
fn placeholder_copy(run: &TypeRun) -> Result<String, String> {
    if !area_text(run) {
        return Ok(placeholder_sentence());
    }
    let frame = run.frame.as_ref().expect("area text has a frame");
    if !frame.size.x.is_finite()
        || !frame.size.y.is_finite()
        || frame.inset.iter().any(|value| !value.is_finite())
    {
        return Err("The type frame has no finite size".into());
    }
    let metrics = crate::text::character_metrics(run, 0);
    let px = metrics.px.unwrap_or(run.px).max(0.1);
    let leading = metrics
        .leading
        .map(|mode| mode.pixels(px))
        .unwrap_or_else(|| run.line_height())
        .max(0.1);
    let per_line = (f64::from(frame.size.x) / (f64::from(px) * 0.5)).max(1.);
    let lines = (f64::from(frame.size.y) / f64::from(leading)).max(1.);
    let want = (per_line * lines).floor().max(1.) as usize;
    let mut out = String::new();
    for (index, word) in PLACEHOLDER.split(' ').cycle().enumerate() {
        if out.len() >= want || index >= 10_000 {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    Ok(out)
}

/// Replace a story with stand-in copy in the first character's style.
/// Args: text run. Returns whether the story text or its styles changed.
/// Area type repeats the copy up to the frame budget. Point type and path type get one sentence.
/// The same text and styles a second time do not count as a change.
fn fill_authored_placeholder(run: &mut TypeRun) -> Result<bool, String> {
    let text = placeholder_copy(run)?;
    let mut next = run.clone();
    let count = next.content.chars().count();
    next.replace_text(0, count, &text, Some(first_stored_style(run)));
    next.layout = None;
    if next.content == run.content
        && next.spans == run.spans
        && next.paragraphs == run.paragraphs
        && next.manual_kern == run.manual_kern
        && next.svg_text == run.svg_text
    {
        return Ok(false);
    }
    *run = next;
    Ok(true)
}

/// Keep a rewritten text object's existing rotation frame instead of moving its baseline.
/// Args: original shape and rewritten run. Returns live text with its visible anchor preserved.
fn placed_text(shape: &Shape, run: TypeRun) -> Geom {
    let mut geom = Geom::Text(run);
    crate::text::fill_contours(&mut geom);
    if shape.rotation.abs() > 1e-5 {
        let delta = shape.geom.bbox().center() - geom.bbox().center();
        let shift = delta - delta.rotate(shape.rotation);
        if shift.length() > 0.
            && let Geom::Text(run) = &mut geom
        {
            run.apply_affine([1., 0., 0., 1., shift.x, shift.y]);
        }
    }
    geom
}

impl Studio {
    fn type_commands_available(&self) -> bool {
        !self.show_welcome
            && self.persona != Persona::Photo
            && !self.file_dialog_pending()
            && !self.updates.freezing
            && self.pending_nav.is_none()
            && self.op.is_none()
            && self.free_transform.is_none()
            && self.deformation.is_none()
            && self.channel_target.is_none()
            && !self.quick_mask_painting()
    }
    fn type_command_writable(&self, hit: (usize, u64), hidden: bool) -> bool {
        let Some(shape) = self.doc.find_shape(hit.0, hit.1) else {
            return false;
        };
        let Geom::Text(run) = &shape.geom else {
            return false;
        };
        let story = run.thread.as_ref().map(|t| t.story);
        let members: Vec<_> = self.doc.layers.iter().enumerate().flat_map(|(layer, l)| {
            l.kind.shapes().unwrap_or(&[]).iter().filter_map(move |s| {
                let related = s.id == hit.1 || story.is_some_and(|id| s.id == id || matches!(&s.geom, Geom::Text(r) if r.thread.as_ref().is_some_and(|t| t.story == id)));
                related.then_some((layer, s))
            })
        }).collect();
        let ids: HashSet<_> = members.iter().map(|(_, s)| s.id).collect();
        if story.is_some_and(|id| !ids.contains(&id)) {
            return false;
        }
        members.into_iter().all(|(layer, s)| {
            if !self.layer_unlocked(layer) || (!hidden && !self.doc.layer_visible(layer)) {
                return false;
            }
            if let Geom::Text(r) = &s.geom
                && r.thread.as_ref().is_some_and(|t| {
                    t.prev.is_some_and(|id| !ids.contains(&id))
                        || t.next.is_some_and(|id| !ids.contains(&id))
                })
            {
                return false;
            }
            let mut next = Some(s.id);
            let mut seen = HashSet::new();
            while let Some(id) = next {
                if !seen.insert(id) || seen.len() > 64 {
                    return false;
                }
                let Some(s) = self.doc.find_shape(layer, id) else {
                    return false;
                };
                if s.locked || (!hidden && !s.visible) {
                    return false;
                }
                next = s.layout.parent;
            }
            true
        })
    }
    /// Capture actual selected stories or the whole document before opening a Type action.
    /// Args: scope. Returns a stale-checked target or a readable availability error.
    pub(crate) fn capture_type_command(
        &self,
        scope: TextScope,
    ) -> Result<TypeCommandTarget, String> {
        self.capture_type_target(scope, scope == TextScope::Document, false)
    }

    fn capture_type_target(
        &self,
        scope: TextScope,
        hidden: bool,
        skip_locked: bool,
    ) -> Result<TypeCommandTarget, String> {
        self.capture_type_target_filtered(scope, hidden, skip_locked, |_| Ok(true))
    }
    fn capture_type_target_filtered(
        &self,
        scope: TextScope,
        hidden: bool,
        skip_locked: bool,
        include: impl Fn(&TypeRun) -> Result<bool, String>,
    ) -> Result<TypeCommandTarget, String> {
        if !self.type_commands_available() {
            return Err("Finish the current operation before editing text".into());
        }
        let mut objects = Vec::new();
        if scope == TextScope::Document {
            for (layer, l) in self.doc.layers.iter().enumerate() {
                objects.extend(
                    l.kind
                        .shapes()
                        .unwrap_or(&[])
                        .iter()
                        .filter(|s| matches!(s.geom, Geom::Text(_)))
                        .map(|s| (layer, s.id)),
                );
            }
        } else if let Some(edit) = &self.type_edit {
            objects.push(edit.frame);
        } else {
            for &(layer, id) in &self.selection {
                objects.extend(
                    std::iter::once(id)
                        .chain(crate::layout::descendants(&self.doc, layer, id))
                        .map(|id| (layer, id)),
                );
            }
        }
        let mut hits = Vec::new();
        for hit in objects {
            if !self
                .doc
                .find_shape(hit.0, hit.1)
                .is_some_and(|s| matches!(s.geom, Geom::Text(_)))
            {
                continue;
            }
            let head = self.story_head(hit);
            let Some(Geom::Text(run)) =
                self.doc.find_shape(head.0, head.1).map(|shape| &shape.geom)
            else {
                continue;
            };
            if !include(run)? {
                continue;
            }
            if !self.type_command_writable(hit, hidden) {
                if skip_locked {
                    continue;
                }
                return Err(
                    "Unlock the text, its parent layers and every frame in its story".into(),
                );
            }
            if !hits.contains(&head) {
                hits.push(head);
            }
        }
        if hits.is_empty() {
            return Err("Select editable text".into());
        }
        Ok(self.type_command_snapshot(scope, hits))
    }
    /// Capture the common document, selection and live-draft owner of a Type action.
    /// Args: scope and actual story identities. Returns their immutable owner snapshot.
    fn type_command_snapshot(
        &self,
        scope: TextScope,
        hits: Vec<(usize, u64)>,
    ) -> TypeCommandTarget {
        TypeCommandTarget {
            document: self.swap_id.clone(),
            generation: self.canvas_gen,
            selection: self.selection.clone(),
            layer: self.selected_layer,
            scope,
            edit: self.type_edit.as_ref().map(EditOwner::new),
            hits,
        }
    }
    /// Check the document, selection, draft and writable story identities captured by a Type action.
    /// Args: target. Returns false after any stale document/edit owner or lock change.
    pub(crate) fn type_command_current(&self, target: &TypeCommandTarget) -> bool {
        self.type_command_owner_current(target)
            && target
                .hits
                .iter()
                .all(|&hit| self.type_command_writable(hit, target.scope == TextScope::Document))
    }
    /// Read the captured owner without granting permission to mutate its stories.
    /// Args: target snapshot. Returns whether the same document, selection and live draft still own it.
    fn type_command_owner_current(&self, target: &TypeCommandTarget) -> bool {
        self.type_commands_available()
            && target.document == self.swap_id
            && target.generation == self.canvas_gen
            && target.selection == self.selection
            && target.layer == self.selected_layer
            && target.edit == self.type_edit.as_ref().map(EditOwner::new)
    }
    /// Apply a Type content command through the current draft or one ordinary document batch.
    /// Args: captured owner and command. Returns changed story count; refusals and no-ops keep draft and redo intact.
    pub(crate) fn apply_type_command(
        &mut self,
        target: &TypeCommandTarget,
        command: TypeCommand,
    ) -> Result<usize, String> {
        if !self.type_command_current(target) {
            return Err("The text destination changed. Reopen this Type command".into());
        }
        if let TypeCommand::Replace(replacement) = &command {
            replacement.query.validate()?;
        }
        if let TypeCommand::Punctuation(p) = command
            && p.double.into_iter().chain(p.single).any(char::is_control)
        {
            return Err("Quotation marks must be printable characters".into());
        }
        if matches!(command, TypeCommand::FitHeadline)
            && !target.hits.iter().any(|&(layer, id)| {
                self.doc
                    .find_shape(layer, id)
                    .is_some_and(|shape| matches!(&shape.geom, Geom::Text(run) if area_text(run)))
            })
        {
            return Err("Select area type".into());
        }
        let menu_face = match &command {
            TypeCommand::Face(TypeFace::Family(family)) => {
                Some(regular_reference(&self.font_catalog(), family)?)
            }
            TypeCommand::Face(TypeFace::Recent(path)) => {
                authored_face(path)?;
                Some(path.clone())
            }
            _ => None,
        };
        let menu_catalog = matches!(
            command,
            TypeCommand::Face(TypeFace::Bold | TypeFace::Italic)
        )
        .then(|| self.font_catalog());
        let live = (target.scope == TextScope::Selection
            && !matches!(
                command,
                TypeCommand::FitHeadline | TypeCommand::Placeholder | TypeCommand::Face(_)
            ))
        .then_some(target.edit.as_ref())
        .flatten();
        if let TypeCommand::Size(_) = &command
            && let Some(edit) = live
            && edit.caret == edit.anchor
        {
            return Ok(0);
        }
        let mut plans = Vec::new();
        let mut selection = None;
        for &(layer, id) in &target.hits {
            let shape = self.doc.find_shape(layer, id).unwrap();
            let Geom::Text(source) = &shape.geom else {
                return Err("The text destination changed".into());
            };
            if matches!(command, TypeCommand::FitHeadline) && !area_text(source) {
                continue;
            }
            let mut run = source.clone();
            let count = run.content.chars().count();
            let range = live
                .filter(|e| e.hit == (layer, id))
                .map(|e| e.caret.min(e.anchor)..e.caret.max(e.anchor));
            if range.as_ref().is_some_and(|r| r.end > count) {
                return Err("The text selection changed".into());
            }
            let changed = if let TypeCommand::Size(delta) = &command {
                step_authored_size(&mut run, range.unwrap_or(0..count), *delta)?
            } else if let TypeCommand::FitHeadline = &command {
                fit_authored_headline(&mut run)?
            } else if let TypeCommand::Placeholder = &command {
                fill_authored_placeholder(&mut run)?
            } else if let TypeCommand::Face(face) = &command {
                let to = if let Some(to) = &menu_face {
                    to.clone()
                } else {
                    style_reference(menu_catalog.as_ref().unwrap(), &run.font, face)?
                };
                replace_story_font(&mut run, &to)?
            } else if let TypeCommand::Font(replacement) = &command {
                crate::text::font_inventory::replace_at(
                    &mut run,
                    &replacement.from,
                    &replacement.to,
                    &replacement.variations,
                )?
            } else if let TypeCommand::Insert(text) = &command {
                if text.is_empty() {
                    false
                } else {
                    let r = range.unwrap_or(count..count);
                    run.replace_text(r.start, r.end, text, live.and_then(|e| e.pending.clone()));
                    selection = Some((
                        r.start + text.chars().count(),
                        r.start + text.chars().count(),
                    ));
                    let changed = run != *source;
                    if changed {
                        run.layout = None;
                    }
                    changed
                }
            } else {
                let r = range.filter(|r| !r.is_empty()).unwrap_or(0..count);
                let edits = rewrites(&run, r, &command);
                if let Some(e) = live {
                    selection = Some((
                        rewritten_caret(e.caret, &edits),
                        rewritten_caret(e.anchor, &edits),
                    ));
                }
                if matches!(command, TypeCommand::Replace(_)) {
                    run.rewrite_text_ranges(&edits)?
                } else {
                    run.rewrite_characters(&edits)?
                }
            };
            if changed {
                plans.push(((layer, id), placed_text(shape, run)));
            }
        }
        let changed = plans.len();
        if changed == 0 {
            return Ok(0);
        }
        self.cancel_type_ime();
        self.type_paste_jobs.clear();
        if let Some(edit) = live {
            let geom = plans
                .into_iter()
                .find(|(hit, _)| *hit == edit.hit)
                .unwrap()
                .1;
            self.doc
                .find_shape_mut(edit.hit.0, edit.hit.1)
                .unwrap()
                .geom = geom;
            let edit = self.type_edit.as_mut().unwrap();
            if let Some((caret, anchor)) = selection {
                edit.caret = caret;
                edit.anchor = anchor;
            }
            edit.pending_style = None;
            self.reshape_live_type();
        } else {
            let mut commands = Vec::new();
            let pending = self.type_edit.take();
            if let Some(edit) = &pending
                && !plans.iter().any(|(hit, _)| *hit == (edit.layer, edit.id))
            {
                let shape = self
                    .doc
                    .find_shape(edit.layer, edit.id)
                    .ok_or("The active story disappeared")?;
                if shape.geom != edit.before {
                    commands.push(Cmd::SetGeom {
                        layer: edit.layer,
                        id: edit.id,
                        before: edit.before.clone(),
                        after: shape.geom.clone(),
                        rot_before: shape.rotation,
                        rot_after: shape.rotation,
                    });
                }
            }
            for ((layer, id), mut after) in plans {
                let shape = self.doc.find_shape(layer, id).unwrap();
                let before = pending
                    .as_ref()
                    .filter(|e| (e.layer, e.id) == (layer, id))
                    .map_or_else(|| shape.geom.clone(), |e| e.before.clone());
                crate::text::fill_contours(&mut after);
                commands.push(Cmd::SetGeom {
                    layer,
                    id,
                    before,
                    after,
                    rot_before: shape.rotation,
                    rot_after: shape.rotation,
                });
            }
            self.commit(Cmd::Batch(commands));
        }
        self.status = if changed == 1 {
            "Text updated".into()
        } else {
            format!("Updated {changed} text stories")
        };
        if let TypeCommand::Face(TypeFace::Recent(path)) = &command {
            self.font_recents.retain(|item| item != path);
            self.font_recents.insert(0, path.clone());
            self.font_recents.truncate(15);
        }
        Ok(changed)
    }
    /// Invoke an immediate selection command using the same captured-target path as the dialog.
    /// Args: command. Returns no value; any refusal is displayed in the ordinary status line.
    pub(crate) fn type_selection_command(&mut self, command: TypeCommand) {
        let result = self
            .capture_type_command(TextScope::Selection)
            .and_then(|target| self.apply_type_command(&target, command));
        if let Err(error) = result {
            self.status = error;
        }
    }
}

#[cfg(test)]
#[path = "type_commands_tests.rs"]
mod tests;
