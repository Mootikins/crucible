//! Bounded matching with JavaScript replacement-string semantics.
use anyhow::{ensure, Result};
use fancy_regex::{Captures, Regex, RegexBuilder};

pub(super) struct Pattern {
    regex: Regex,
    global: bool,
    sticky: bool,
}
impl Pattern {
    pub fn new(pattern: &str, flags: &str) -> Result<Self> {
        let mut seen = std::collections::BTreeSet::new();
        ensure!(
            flags
                .chars()
                .all(|c| "dgimsuy".contains(c) && seen.insert(c)),
            "Invalid or duplicate regular expression flags"
        );
        let regex = RegexBuilder::new(pattern)
            .case_insensitive(flags.contains('i'))
            .multi_line(flags.contains('m'))
            .dot_matches_new_line(flags.contains('s'))
            .backtrack_limit(100_000)
            .delegate_size_limit(1_000_000)
            .build()?;
        Ok(Self {
            regex,
            global: flags.contains('g'),
            sticky: flags.contains('y'),
        })
    }
    pub fn matches(&self, text: &str) -> Result<bool> {
        Ok(self
            .regex
            .find(text)?
            .is_some_and(|m| !self.sticky || m.start() == 0))
    }
    pub fn replace(&self, text: &str, replacement: &str) -> Result<String> {
        let mut out = String::new();
        let mut end = 0;
        for captures in self.regex.captures_iter(text) {
            let captures = captures?;
            let found = captures.get(0).expect("full match");
            if self.sticky && found.start() != end {
                break;
            }
            out.push_str(&text[end..found.start()]);
            expand(
                &mut out,
                text,
                &captures,
                replacement,
                self.regex.capture_names().any(|name| name.is_some()),
            );
            ensure!(
                out.len() <= 16_000_000,
                "Regex replacement exceeds allocation limit"
            );
            end = found.end();
            if !self.global {
                break;
            }
        }
        out.push_str(&text[end..]);
        Ok(out)
    }
    pub fn split(&self, text: &str, limit: usize) -> Result<Vec<String>> {
        let mut out = vec![];
        let mut end = 0;
        for captures in self.regex.captures_iter(text) {
            let captures = captures?;
            let m = captures.get(0).expect("full match");
            if m.start() == m.end() && (m.start() == end || m.end() == text.len()) {
                continue;
            }
            out.push(text[end..m.start()].to_owned());
            for i in 1..captures.len() {
                out.push(captures.get(i).map_or("", |m| m.as_str()).to_owned());
            }
            end = m.end();
            if out.len() >= limit {
                out.truncate(limit);
                return Ok(out);
            }
        }
        out.push(text[end..].to_owned());
        out.truncate(limit);
        Ok(out)
    }
}
fn expand(out: &mut String, text: &str, captures: &Captures<'_>, replacement: &str, named: bool) {
    let m = captures.get(0).expect("full match");
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('$') => {
                chars.next();
                out.push('$');
            }
            Some('&') => {
                chars.next();
                out.push_str(m.as_str());
            }
            Some('`') => {
                chars.next();
                out.push_str(&text[..m.start()]);
            }
            Some('\'') => {
                chars.next();
                out.push_str(&text[m.end()..]);
            }
            Some(c @ '0'..='9') => {
                let mut index = c.to_digit(10).unwrap() as usize;
                let mut digits = 1;
                let next = chars.clone().nth(1).and_then(|c| c.to_digit(10));
                if let Some(next) = next {
                    let two = index * 10 + next as usize;
                    if two > 0 && two < captures.len() {
                        index = two;
                        digits = 2;
                    }
                }
                if index == 0 || index >= captures.len() {
                    out.push('$');
                    continue;
                }
                for _ in 0..digits {
                    chars.next();
                }
                if let Some(group) = captures.get(index) {
                    out.push_str(group.as_str());
                }
            }
            Some('<') if named => {
                let tail = chars.clone().collect::<String>();
                if let Some(close) = tail.find('>') {
                    let name = &tail[1..close];
                    if let Some(group) = captures.name(name) {
                        out.push_str(group.as_str());
                    }
                    for _ in 0..=close {
                        chars.next();
                    }
                    continue;
                }
                out.push('$');
            }
            _ => out.push('$'),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bases_regex_stops_adversarial_backtracking() {
        let pattern = Pattern::new(r"^(a+)+\1$", "").unwrap();
        let input = format!("{}!", "a".repeat(40));
        assert!(pattern.matches(&input).is_err());
    }
}
