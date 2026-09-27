//! Character-indexed edits preserve paragraph styles and normalized range overrides.
use crate::geom::{CharSpan, ParagraphStyle, TypeRun};
impl TypeRun {
    pub fn character_style(&self, index: usize) -> CharSpan {
        let mut style = self
            .spans
            .iter()
            .find(|s| s.start <= index && index < s.end)
            .cloned()
            .unwrap_or_default();
        style.start = 0;
        style.end = 0;
        style
    }
    pub fn set_character_style(
        &mut self,
        start: usize,
        end: usize,
        mut update: impl FnMut(&mut CharSpan),
    ) {
        let mut styles: Vec<_> = (0..self.content.chars().count())
            .map(|i| self.character_style(i))
            .collect();
        for style in styles.iter_mut().take(end).skip(start) {
            update(style);
        }
        self.assign_character_styles(styles);
    }
    fn assign_character_styles(&mut self, styles: Vec<CharSpan>) {
        self.spans.clear();
        for (i, mut style) in styles.into_iter().enumerate() {
            style.start=0;style.end=0;
            if style == CharSpan::default() {
                continue;
            }
            if let Some(last) = self.spans.last_mut() {
                let mut previous = last.clone();
                previous.start = 0;
                previous.end = 0;
                if last.end == i && previous == style {
                    last.end += 1;
                    continue;
                }
            }
            self.spans.push(CharSpan {
                start: i,
                end: i + 1,
                ..style
            });
        }
    }
    pub fn replace_text(
        &mut self,
        start: usize,
        end: usize,
        text: &str,
        pending: Option<CharSpan>,
    ) {
        let count = self.content.chars().count();
        let start = start.min(count);
        let end = end.max(start).min(count);
        let mut chars: Vec<_> = (0..count).map(|i| self.character_style(i)).collect();
        let mut paras: Vec<_> = (0..=count)
            .map(|i| super::paragraph_style(self, i))
            .collect();
        let inherited = pending.unwrap_or_else(|| self.character_style(start.saturating_sub(1)));
        let para = super::paragraph_style(self, start);
        let n = text.chars().count();
        chars.splice(start..end, std::iter::repeat_n(inherited, n));
        paras.splice(start..end, std::iter::repeat_n(para, n));
        let a = super::char_to_byte(&self.content, start);
        let b = super::char_to_byte(&self.content, end);
        self.manual_kern=std::mem::take(&mut self.manual_kern).into_iter().filter_map(|(at,value)| {
            if start<end && at>=start && at<=end {None} else {Some((if at>=end{at-(end-start)+n}else{at},value))}
        }).filter(|(at,_)|*at>0).collect();
        self.content.replace_range(a..b, text);
        self.assign_character_styles(chars);
        self.paragraphs.clear();
        let starts = std::iter::once(0).chain(
            self.content
                .chars()
                .enumerate()
                .filter_map(|(i, c)| (c == '\n').then_some(i + 1)),
        );
        for at in starts {
            let mut p = paras.get(at).cloned().unwrap_or_default();
            p.start = at;
            if p != (ParagraphStyle {
                start: at,
                align: self.align,
                ..Default::default()
            }) {
                self.paragraphs.push(p);
            }
        }
    }
    pub fn scale_character_metrics(&mut self,ratio:f32) {
        for span in &mut self.spans {
            if let Some(crate::geom::Leading::Fixed(value))=&mut span.leading {*value*=ratio.abs();}
            if let Some(shift)=&mut span.baseline_shift {*shift*=ratio;}
        }
    }
    pub fn update_paragraphs(
        &mut self,
        start: usize,
        end: usize,
        mut update: impl FnMut(&mut ParagraphStyle),
    ) {
        let starts: Vec<_> = std::iter::once(0)
            .chain(
                self.content
                    .chars()
                    .enumerate()
                    .filter_map(|(i, c)| (c == '\n').then_some(i + 1)),
            )
            .collect();
        let first = super::paragraph_style(self, start).start;
        for at in starts
            .into_iter()
            .filter(|at| *at >= first && (*at < end || *at == first))
        {
            let mut p = super::paragraph_style(self, at);
            update(&mut p);
            if let Some(old) = self.paragraphs.iter_mut().find(|p| p.start == at) {
                *old = p;
            } else {
                self.paragraphs.push(p);
            }
        }
        self.paragraphs.sort_by_key(|p| p.start);
    }
}
