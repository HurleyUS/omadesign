//! SVG text consumes the same placed lines and glyph clusters as the native renderer.
use super::*;
use crate::{
    document::Shape,
    geom::{Bounds, Pt},
    text::{self, LayoutLine},
};
use tiny_skia::Transform;

pub(super) enum Issue {
    Outline(String),
    Invalid(String),
}
impl From<String> for Issue {
    fn from(value: String) -> Self {
        Self::Invalid(value)
    }
}
impl From<&str> for Issue {
    fn from(value: &str) -> Self {
        Self::Invalid(value.into())
    }
}

struct Paints<'a> {
    id: u64,
    bounds: Bounds,
    contours: &'a [Vec<Pt>],
    inverse: Transform,
    definitions: String,
    seen: BTreeMap<String, Fill>,
}
impl Paints<'_> {
    fn fill(&mut self, fill: &Fill) -> Result<String, String> {
        let reference = match fill {
            Fill::Solid(color) => color.css(),
            Fill::None => "none".into(),
            _ => {
                let key = serde_json::to_vec(fill).map_err(|e| e.to_string())?;
                let id = format!(
                    "oma-embedded-paint-{}-{:032x}",
                    self.id,
                    crate::typography::fingerprint(&key)
                );
                if let Some(old) = self.seen.get(&id) {
                    if old != fill {
                        return Err("SVG paint fingerprint collision".into());
                    }
                } else {
                    self.definitions
                        .push_str(&crate::svg::type_paint_definition(
                            fill,
                            self.bounds,
                            &id,
                            self.inverse,
                            self.contours,
                        ));
                    self.seen.insert(id.clone(), fill.clone());
                }
                format!("url(#{id})")
            }
        };
        Ok(format!(
            " fill=\"{}\" fill-opacity=\"{}\"",
            text::svg_type::escape(&reference),
            crate::svg::paint_opacity(fill)
        ))
    }
}

impl Embedded {
    /// Serialize native text using its existing placed lines and original font programs.
    /// Args: source shape with current native geometry. Returns standard editable SVG text without mutating artwork.
    pub(super) fn native_layout(&mut self, shape: &Shape) -> Result<String, Issue> {
        let Geom::Text(run) = &shape.geom else {
            return Err("SVG type export requires text".into());
        };
        let m = run.transform.unwrap_or([1., 0., 0., 1., 0., 0.]);
        let matrix = Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5]);
        let mut paints = Paints {
            id: shape.id,
            bounds: shape.geom.bbox(),
            contours: &run.contours,
            inverse: matrix.invert().unwrap_or_default(),
            definitions: String::new(),
            seen: Default::default(),
        };
        let fill = paints.fill(&shape.style.fill)?;
        let mut body = format!("<text xml:space=\"preserve\" stroke=\"none\"{fill}>");
        if run.on_path.is_some() {
            body.push_str(&self.path_text(run, shape.id, &mut paints)?);
        } else if run.frame.is_some() {
            let computed;
            let layout = if let Some(layout) = run.layout.as_ref() {
                layout
            } else {
                computed = crate::text_geometry::layout_frame(run, run, 0, &[], false);
                &computed
            };
            for placed in &layout.export_lines {
                body.push_str(&self.line(&placed.run, &placed.line, placed.origin, &mut paints)?);
            }
        } else {
            for line in text::compose(run).iter() {
                body.push_str(&self.line(run, line, run.origin, &mut paints)?);
            }
        }
        body.push_str("</text>");
        if let Some(frame) = run.frame.as_ref() {
            if frame.overflow != crate::geom::Overflow::Visible
                || run.thread.as_ref().is_some_and(|t| t.next.is_some())
            {
                let id = format!("oma-embedded-frame-{}", shape.id);
                paints.definitions.push_str(&format!("<clipPath id=\"{id}\" clipPathUnits=\"userSpaceOnUse\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>",run.origin.x,run.origin.y-run.px*0.85,frame.size.x.max(0.),frame.size.y.max(0.)));
                body = format!("<g clip-path=\"url(#{id})\">{body}</g>");
            }
        }
        Ok(format!(
            "<g transform=\"{}\"><defs>{}</defs>{body}</g>",
            crate::svg_source::matrix(matrix),
            paints.definitions
        ))
    }
    fn attributes(
        &mut self,
        run: &TypeRun,
        at: usize,
        scaled: bool,
        paints: &mut Paints<'_>,
    ) -> Result<String, String> {
        let metrics = text::character_metrics(run, at);
        let font = metrics.font.as_ref().unwrap_or(&run.font);
        let bytes = text::font_bytes_for(&TypeRun {
            font: font.clone(),
            ..Default::default()
        })
        .ok_or("The font needed for SVG export is unavailable")?;
        let face =
            rustybuzz::ttf_parser::Face::parse(&bytes, 0).map_err(|_| "Invalid SVG export font")?;
        let authored = text::variations::at(run, at);
        let axes = text::variations::FontVariations::new(
            face.variation_axes()
                .into_iter()
                .map(|a| {
                    let value = authored
                        .settings()
                        .iter()
                        .find(|(t, _)| *t == a.tag.to_bytes())
                        .map_or(a.def_value, |(_, v)| *v);
                    (a.tag.to_bytes(), value.clamp(a.min_value, a.max_value))
                })
                .collect(),
        )?;
        let weight = face.weight().to_number();
        let italic = face.is_italic();
        let family = self.add(bytes)?;
        let px = metrics.px.unwrap_or(run.px).max(1.)
            * if scaled {
                metrics.vscale.unwrap_or(100.) / 100.
            } else {
                1.
            };
        let mut tags = std::collections::BTreeSet::from([
            *b"kern", *b"liga", *b"clig", *b"tnum", *b"smcp", *b"c2sc", *b"calt",
        ]);
        tags.extend(run.features.iter().map(|(t, _)| *t));
        tags.extend(metrics.features.iter().map(|(t, _)| *t));
        let features = tags
            .into_iter()
            .map(|tag| {
                let value =
                    if tag == *b"kern" && metrics.kerning != Some(crate::geom::KernMode::Metrics) {
                        0
                    } else {
                        text::feature_value(run, at, tag)
                    };
                (tag, value)
            })
            .collect::<Vec<_>>();
        let css = format!(
            "font-optical-sizing:none;font-variation-settings:{};font-feature-settings:{};font-kerning:{};",
            if axes.is_empty() {
                "normal".into()
            } else {
                text::variations::css(&axes)
            },
            text::feature_css(&features),
            if metrics.kerning == Some(crate::geom::KernMode::Metrics) && run.kern {
                "normal"
            } else {
                "none"
            }
        );
        let mut out = format!(
            " font-family=\"{family}\" font-size=\"{px}\" font-weight=\"{weight}\" font-style=\"{}\" style=\"{}\"",
            if italic { "italic" } else { "normal" },
            text::svg_type::escape(&css)
        );
        if let Some(fill) = &metrics.fill {
            out.push_str(&paints.fill(fill)?);
        }
        Ok(out)
    }
    fn line(
        &mut self,
        run: &TypeRun,
        line: &LayoutLine,
        origin: Pt,
        paints: &mut Paints<'_>,
    ) -> Result<String, Issue> {
        let paragraph = text::paragraph_style(run, line.start);
        let positioned = !run.manual_kern.is_empty()
            || paragraph.word_spacing[1] != 100.
            || paragraph.letter_spacing[1] != 0.
            || matches!(paragraph.align, crate::geom::TextAlign::Justify { .. })
            || (line.start..line.end).any(|at| {
                let m = text::character_metrics(run, at);
                m.hscale != Some(100.)
                    || m.vscale != Some(100.)
                    || m.tracking != Some(0.)
                    || m.kerning == Some(crate::geom::KernMode::Optical)
            });
        if positioned {
            return self.clusters(run, line, origin, paints);
        }
        let mut out = String::new();
        for range in run.character_style_runs(line.start, line.end) {
            let mut text: String = run
                .content
                .chars()
                .skip(range.start)
                .take(range.end - range.start)
                .filter(|c| *c != '\u{ad}')
                .collect();
            if line.hyphenated && range.end == line.end {
                text.push('-');
            }
            let attrs = self.attributes(run, range.start, false, paints)?;
            let m = text::character_metrics(run, range.start);
            out.push_str(&format!(
                "<tspan x=\"{}\" y=\"{}\"{attrs}>{}</tspan>",
                origin.x + line.caret_x(range.start),
                origin.y + line.baseline - m.baseline_shift.unwrap_or(0.),
                text::svg_type::escape(&text)
            ));
        }
        Ok(out)
    }
    fn clusters(
        &mut self,
        run: &TypeRun,
        line: &LayoutLine,
        origin: Pt,
        paints: &mut Paints<'_>,
    ) -> Result<String, Issue> {
        let mut groups = Vec::<(usize, Vec<&text::LayoutGlyph>)>::new();
        for glyph in &line.glyphs {
            if let Some((at, g)) = groups.last_mut().filter(|(at, _)| *at == glyph.cluster) {
                let _ = at;
                g.push(glyph);
            } else {
                groups.push((glyph.cluster, vec![glyph]));
            }
        }
        let mut out = String::new();
        for (index, (start, glyphs)) in groups.iter().enumerate() {
            let end = groups.get(index + 1).map_or(line.end, |(at, _)| *at);
            let mut content: String = run
                .content
                .chars()
                .skip(*start)
                .take(end.saturating_sub(*start))
                .filter(|c| *c != '\u{ad}')
                .collect();
            if line.hyphenated && index + 1 == groups.len() {
                content.push('-');
            }
            if content.is_empty() {
                continue;
            }
            let metrics = text::character_metrics(run, *start);
            let mut probe = run.clone();
            probe.origin = Pt::ZERO;
            probe.transform = None;
            probe.layout = None;
            probe.contours.clear();
            probe.frame = None;
            probe.on_path = None;
            probe.thread = None;
            probe.wrap_width = None;
            probe.align = crate::geom::TextAlign::Start;
            probe.paragraphs.clear();
            probe.manual_kern.clear();
            probe.content = content.clone();
            let mut style = metrics.clone();
            style.start = 0;
            style.end = content.chars().count();
            style.tracking = Some(0.);
            style.baseline_shift = Some(0.);
            probe.spans = vec![style];
            let shaped = text::compose(&probe);
            let isolated = shaped.first().ok_or("Cannot shape an SVG text cluster")?;
            if !isolated
                .glyphs
                .iter()
                .map(|g| g.id)
                .eq(glyphs.iter().map(|g| g.id))
            {
                return Err(Issue::Outline(
                    "Context-dependent text was outlined to preserve its original glyphs".into(),
                ));
            }
            let first = glyphs[0];
            let intrinsic = isolated
                .glyphs
                .first()
                .ok_or("Cannot position an empty text cluster")?;
            for (a, b) in isolated.glyphs.iter().zip(glyphs) {
                if (a.x - intrinsic.x - (b.x - first.x)).abs() > 0.01
                    || (a.y - intrinsic.y - (b.y - first.y)).abs() > 0.01
                {
                    return Err(Issue::Outline("Context-dependent text positioning was outlined to preserve its original glyphs".into()));
                }
            }
            let attrs = self.attributes(run, *start, true, paints)?;
            let mut width = String::new();
            if (metrics.hscale.unwrap_or(100.) - metrics.vscale.unwrap_or(100.)).abs() > 0.001 {
                let advance = isolated.glyphs.iter().map(|g| g.advance).sum::<f32>();
                if advance > 0. {
                    width = format!(" textLength=\"{advance}\" lengthAdjust=\"spacingAndGlyphs\"");
                }
            }
            out.push_str(&format!(
                "<tspan x=\"{}\" y=\"{}\"{attrs}{width}>{}</tspan>",
                origin.x + first.x - intrinsic.x,
                origin.y + first.y - intrinsic.y,
                text::svg_type::escape(&content)
            ));
        }
        Ok(out)
    }
    fn path_text(
        &mut self,
        run: &TypeRun,
        id: u64,
        paints: &mut Paints<'_>,
    ) -> Result<String, Issue> {
        let on = run.on_path.as_ref().unwrap();
        let path = on.cache.as_ref().ok_or("Text path is missing its guide")?;
        let (_, span) =
            crate::text_geometry::path_interval(on).ok_or("Text path has no visible interval")?;
        let mut plain = run.clone();
        plain.origin = Pt::ZERO;
        plain.transform = None;
        plain.frame = None;
        plain.thread = None;
        plain.on_path = None;
        plain.layout = None;
        plain.wrap_width = None;
        plain.content = plain.content.replace('\n', " ");
        plain.tracking += on.spacing;
        for style in &mut plain.spans {
            if let Some(t) = &mut style.tracking {
                *t += on.spacing / plain.px.max(1.) * 1000.;
            }
        }
        let lines = text::compose(&plain);
        let width = lines.iter().map(|l| l.width).fold(0., f32::max);
        let offset = match text::paragraph_style(run, 0).align {
            crate::geom::TextAlign::Center => (span - width).max(0.) * 0.5,
            crate::geom::TextAlign::End => (span - width).max(0.),
            _ => 0.,
        };
        let mut visible = 0;
        for line in lines.iter() {
            let mut starts = line.glyphs.iter().map(|g| g.cluster).collect::<Vec<_>>();
            starts.push(line.end);
            starts.sort_unstable();
            starts.dedup();
            for g in &line.glyphs {
                let center = offset + g.x + g.advance * 0.5;
                if center >= 0. && center + g.advance * 0.5 <= span + 0.001 {
                    visible = visible.max(
                        starts
                            .iter()
                            .copied()
                            .find(|i| *i > g.cluster)
                            .unwrap_or(line.end),
                    );
                }
            }
        }
        let cubics = if on.flip {
            path.cubics
                .iter()
                .rev()
                .map(|c| [c[3], c[2], c[1], c[0]])
                .collect::<Vec<_>>()
        } else {
            path.cubics.clone()
        };
        let cubics = if path.closed {
            cubics
                .iter()
                .copied()
                .cycle()
                .take(cubics.len() * 2)
                .collect::<Vec<_>>()
        } else {
            cubics
        };
        let mut data = String::new();
        for (i, c) in cubics.iter().enumerate() {
            if i == 0 {
                data.push_str(&format!("M {} {} ", c[0].x, c[0].y));
            }
            data.push_str(&format!(
                "C {} {} {} {} {} {} ",
                c[1].x, c[1].y, c[2].x, c[2].y, c[3].x, c[3].y
            ));
        }
        if path.closed {
            data.push('Z');
        }
        let guide = format!("oma-embedded-guide-{id}");
        paints
            .definitions
            .push_str(&format!("<path id=\"{guide}\" d=\"{data}\"/>"));
        let start = if on.flip {
            path.length - on.start * path.length - span + offset
        } else {
            on.start * path.length + offset
        };
        let start = if path.closed {
            start.rem_euclid(path.length.max(f32::EPSILON))
        } else {
            start
        };
        let align = match on.align {
            crate::geom::PathAlign::Baseline => 0.,
            crate::geom::PathAlign::Center => run.px * 0.35,
            crate::geom::PathAlign::Top => run.px * 0.8,
            crate::geom::PathAlign::Bottom => -run.px * 0.2,
        };
        let mut out = format!("<textPath href=\"#{guide}\" startOffset=\"{start}\">");
        let mut shift = 0.;
        for range in plain.character_style_runs(0, visible) {
            let metrics = text::character_metrics(&plain, range.start);
            if metrics.hscale != Some(100.)
                || metrics.vscale != Some(100.)
                || metrics.kerning == Some(crate::geom::KernMode::Optical)
                || !plain.manual_kern.is_empty()
            {
                return Err(Issue::Outline(
                    "Advanced text on a path was outlined to preserve its native placement".into(),
                ));
            }
            let dy = align - on.baseline_shift - metrics.baseline_shift.unwrap_or(0.);
            let attrs = self.attributes(&plain, range.start, false, paints)?;
            out.push_str(&format!(
                "<tspan{attrs} dy=\"{}\" letter-spacing=\"{}\">{}</tspan>",
                dy - shift,
                metrics.tracking.unwrap_or(0.) * plain.px / 1000.,
                text::svg_type::escape(
                    &plain
                        .content
                        .chars()
                        .skip(range.start)
                        .take(range.end - range.start)
                        .collect::<String>()
                )
            ));
            shift = dy;
        }
        out.push_str("</textPath>");
        Ok(out)
    }
}
