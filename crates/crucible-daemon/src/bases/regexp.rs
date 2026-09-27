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
        let regex = RegexBuilder::new(&ascii_classes(pattern))
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
/// JavaScript `\d` and `\w` (and their negations) are ASCII, with or
/// without the `u` flag. The Rust engine reads them as Unicode classes, so
/// they become the engine's ASCII classes.
fn ascii_classes(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut class = false;
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(e @ ('d' | 'D' | 'w' | 'W')) => {
                    let name = match e {
                        'd' => ":digit:",
                        'D' => ":^digit:",
                        'w' => ":word:",
                        _ => ":^word:",
                    };
                    if class {
                        out.push_str(&format!("[{name}]"));
                    } else {
                        out.push_str(&format!("[[{name}]]"));
                    }
                }
                Some(e) => {
                    out.push('\\');
                    out.push(e);
                }
                None => out.push('\\'),
            },
            '[' if !class => {
                class = true;
                out.push(c);
            }
            ']' if class => {
                class = false;
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}
/// Expands a JavaScript replacement template: `$$`, `$&`, `` $` ``, `$'`,
/// `$n`, `$nn` and `$<name>`.
fn expand(out: &mut String, text: &str, captures: &Captures<'_>, replacement: &str, named: bool) {
    let m = captures.get(0).expect("full match");
    let bytes = replacement.as_bytes();
    let mut i = 0;
    while i < replacement.len() {
        let rest = &replacement[i..];
        if !rest.starts_with('$') {
            let c = rest.chars().next().expect("non-empty rest");
            out.push(c);
            i += c.len_utf8();
            continue;
        }
        match bytes.get(i + 1) {
            Some(b'$') => {
                out.push('$');
                i += 2;
            }
            Some(b'&') => {
                out.push_str(m.as_str());
                i += 2;
            }
            Some(b'`') => {
                out.push_str(&text[..m.start()]);
                i += 2;
            }
            Some(b'\'') => {
                out.push_str(&text[m.end()..]);
                i += 2;
            }
            Some(d @ b'0'..=b'9') => {
                let one = usize::from(d - b'0');
                let two = bytes
                    .get(i + 2)
                    .filter(|b| b.is_ascii_digit())
                    .map(|b| one * 10 + usize::from(b - b'0'))
                    .filter(|n| *n > 0 && *n < captures.len());
                let (index, digits) = match two {
                    Some(n) => (n, 2),
                    None => (one, 1),
                };
                if index == 0 || index >= captures.len() {
                    out.push('$');
                    i += 1;
                    continue;
                }
                if let Some(group) = captures.get(index) {
                    out.push_str(group.as_str());
                }
                i += 1 + digits;
            }
            Some(b'<') if named => match rest.find('>') {
                Some(close) => {
                    if let Some(group) = captures.name(&rest[2..close]) {
                        out.push_str(group.as_str());
                    }
                    i += close + 1;
                }
                None => {
                    out.push('$');
                    i += 1;
                }
            },
            _ => {
                out.push('$');
                i += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bases_named_replacement_reads_the_name_by_bytes() {
        let pattern = Pattern::new("(?<é>b)", "").unwrap();
        assert_eq!(pattern.replace("abc", "[$<é>X]").unwrap(), "a[bX]c");
        assert_eq!(pattern.replace("abc", "$&$$$`$'").unwrap(), "ab$acc");
    }

    #[test]
    fn bases_digit_and_word_classes_are_ascii() {
        for (source, text, matches) in [
            (r"^\d$", "٣", false),
            (r"^\d$", "7", true),
            (r"^\w$", "é", false),
            (r"^[\w.]+$", "a.b_1", true),
            (r"^[\d]$", "٣", false),
            (r"^\W$", "é", true),
            (r"^\D$", "٣", true),
        ] {
            for flags in ["", "u"] {
                assert_eq!(
                    Pattern::new(source, flags).unwrap().matches(text).unwrap(),
                    matches,
                    "{source} /{flags} {text}"
                );
            }
        }
    }

    #[test]
    fn bases_regex_stops_adversarial_backtracking() {
        let pattern = Pattern::new(r"^(a+)+\1$", "").unwrap();
        let input = format!("{}!", "a".repeat(40));
        assert!(pattern.matches(&input).is_err());
    }
}
