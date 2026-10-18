//! CSS OpenType feature settings retain tags, values and declaration-level validity.
use std::{iter::Peekable, str::Chars};

fn space(input: &mut Peekable<Chars<'_>>) -> Option<bool> {
    let mut spaced = false;
    loop {
        while input.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            input.next();
            spaced = true;
        }
        let mut probe = input.clone();
        if probe.next() != Some('/') || probe.next() != Some('*') {
            return Some(spaced);
        }
        *input = probe;
        spaced = true;
        loop {
            let c = input.next()?;
            if c == '*' && input.peek() == Some(&'/') {
                input.next();
                break;
            }
        }
    }
}

/// Parse one CSS font-feature-settings declaration.
/// Args: inherited CSS value. Returns the complete requested feature list, or no overrides for an invalid declaration.
pub(super) fn parse(value: &str) -> Option<Vec<([u8; 4], u32)>> {
    fn settings(value: &str) -> Option<Vec<([u8; 4], u32)>> {
        let mut input = value.chars().peekable();
        let mut features = Vec::new();
        space(&mut input)?;
        let mut keyword = input.clone();
        let name = keyword
            .by_ref()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect::<String>();
        if name.eq_ignore_ascii_case("normal") || name.eq_ignore_ascii_case("initial") {
            let mut keyword = input.clone();
            for _ in name.chars() {
                keyword.next();
            }
            space(&mut keyword)?;
            if keyword.next().is_none() {
                return Some(features);
            }
            return None;
        }
        loop {
            let quote = input.next()?;
            if !matches!(quote, '\'' | '"') {
                return None;
            }
            let mut tag = Vec::new();
            loop {
                let mut c = input.next()?;
                if c == quote {
                    break;
                }
                if c == '\\' {
                    c = input.next()?;
                    if c == '\r' || c == '\n' || c == '\u{c}' {
                        if c == '\r' && input.peek() == Some(&'\n') {
                            input.next();
                        }
                        continue;
                    }
                    if c.is_ascii_hexdigit() {
                        let mut number = c.to_digit(16)?;
                        for _ in 1..6 {
                            let Some(digit) = input.peek().and_then(|c| c.to_digit(16)) else {
                                break;
                            };
                            input.next();
                            number = number * 16 + digit;
                        }
                        if input.peek().is_some_and(|c| c.is_ascii_whitespace()) {
                            input.next();
                        }
                        c = char::from_u32(number)?;
                    }
                }
                if !c.is_ascii() || !(0x20..=0x7e).contains(&(c as u8)) || tag.len() == 4 {
                    return None;
                }
                tag.push(c as u8);
            }
            let tag: [u8; 4] = tag.try_into().ok()?;
            space(&mut input)?;
            let mut value = String::new();
            while input.peek().is_some_and(|c| *c != ',') {
                if space(&mut input)? && !value.is_empty() {
                    value.push(' ');
                }
                if input.peek().is_none_or(|c| *c == ',') {
                    break;
                }
                value.push(input.next()?);
            }
            let number = match value.trim() {
                "" => 1,
                v if v.eq_ignore_ascii_case("on") => 1,
                v if v.eq_ignore_ascii_case("off") => 0,
                v => v.parse().ok()?,
            };
            features.push((tag, number));
            if input.next().is_none() {
                return Some(features);
            }
            space(&mut input)?;
        }
    }
    settings(value)
}
