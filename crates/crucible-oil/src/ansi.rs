//! ANSI escape sequence utilities
//!
//! Functions for stripping ANSI codes and calculating visible width.

use crate::style::Style;
use unicode_width::UnicodeWidthStr;

/// Strip ANSI escape sequences from a string, returning only visible characters.
///
/// Handles CSI (`\x1b[`), OSC (`\x1b]`), APC (`\x1b_`), and DCS (`\x1bP`) sequences.
pub fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                // CSI: \x1b[ ... <letter>
                Some(&'[') => {
                    chars.next();
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                // OSC: \x1b] ... (terminated by BEL \x07 or ST \x1b\\)
                Some(&']') => {
                    chars.next();
                    skip_until_st_or_bel(&mut chars);
                }
                // APC: \x1b_ ... (terminated by ST \x1b\\)
                Some(&'_') => {
                    chars.next();
                    skip_until_st_or_bel(&mut chars);
                }
                // DCS: \x1bP ... (terminated by ST \x1b\\)
                Some(&'P') => {
                    chars.next();
                    skip_until_st_or_bel(&mut chars);
                }
                _ => {}
            }
        } else if c == '\x07' {
            // Stray BEL outside a sequence — skip it
        } else {
            result.push(c);
        }
    }

    result
}

/// Consume chars until String Terminator (ST = `\x1b\\`) or BEL (`\x07`).
fn skip_until_st_or_bel(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while let Some(c) = chars.next() {
        if c == '\x07' {
            return;
        }
        if c == '\x1b' && chars.peek() == Some(&'\\') {
            chars.next();
            return;
        }
    }
}

/// Calculate the visible width of a string (excluding ANSI codes).
pub fn visible_width(s: &str) -> usize {
    strip_ansi(s).width()
}

pub fn visual_rows(line: &str, terminal_width: usize) -> usize {
    if terminal_width == 0 {
        return 1;
    }
    let width = visible_width(line);
    if width == 0 {
        1
    } else {
        width.div_ceil(terminal_width)
    }
}

struct StyledSpan {
    start: usize,
    end: usize,
    codes: String,
}

pub fn wrap_styled_text(spans: &[(String, String)], width: usize) -> Vec<String> {
    if width == 0 {
        let combined: String = spans.iter().map(|(t, _)| t.as_str()).collect();
        return vec![combined];
    }

    let mut styled_spans: Vec<StyledSpan> = Vec::new();
    let mut plain_text = String::new();
    let mut pos = 0;

    for (text, codes) in spans {
        let start = pos;
        plain_text.push_str(text);
        pos += text.chars().count();
        if !codes.is_empty() {
            styled_spans.push(StyledSpan {
                start,
                end: pos,
                codes: codes.clone(),
            });
        }
    }

    if plain_text.is_empty() {
        return vec![String::new()];
    }

    use textwrap::{wrap, Options, WordSplitter};
    let options = Options::new(width).word_splitter(WordSplitter::NoHyphenation);
    let initial_wrapped: Vec<String> = wrap(&plain_text, options)
        .into_iter()
        .map(|cow| cow.into_owned())
        .collect();

    let mut wrapped_lines: Vec<String> = Vec::new();
    for line in initial_wrapped {
        let char_count = line.chars().count();
        if char_count <= width {
            wrapped_lines.push(line);
        } else {
            let chars: Vec<char> = line.chars().collect();
            for chunk in chars.chunks(width) {
                wrapped_lines.push(chunk.iter().collect());
            }
        }
    }

    let mut result: Vec<String> = Vec::new();
    let mut char_offset = 0;

    for line in wrapped_lines {
        let line_start = char_offset;
        let line_char_count = line.chars().count();
        let line_end = line_start + line_char_count;

        let mut output = String::new();
        let mut current_pos = line_start;

        for span in &styled_spans {
            if span.end <= line_start || span.start >= line_end {
                continue;
            }

            let overlap_start = span.start.max(line_start);
            let overlap_end = span.end.min(line_end);

            if overlap_start > current_pos {
                let prefix: String = line
                    .chars()
                    .skip(current_pos - line_start)
                    .take(overlap_start - current_pos)
                    .collect();
                output.push_str(&prefix);
            }

            output.push_str(&span.codes);
            let styled_part: String = line
                .chars()
                .skip(overlap_start - line_start)
                .take(overlap_end - overlap_start)
                .collect();
            output.push_str(&styled_part);
            output.push_str("\x1b[0m");

            current_pos = overlap_end;
        }

        if current_pos < line_end {
            let suffix: String = line.chars().skip(current_pos - line_start).collect();
            output.push_str(&suffix);
        }

        result.push(output);
        char_offset = line_end + 1;
    }

    if result.is_empty() {
        result.push(String::new());
    }

    result
}

/// Apply a Style to content, generating appropriate ANSI escape sequences.
///
/// Returns the content formatted with the given style using crossterm's StyledContent.
/// If the style is the default, returns the content unchanged.
pub fn apply_style(content: &str, style: &Style) -> String {
    if style == &Style::default() {
        return content.to_string();
    }

    use crossterm::style::StyledContent;
    let ct_style = style.to_crossterm();
    format!("{}", StyledContent::new(ct_style, content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_plain() {
        assert_eq!(strip_ansi("hello"), "hello");
    }

    #[test]
    fn test_strip_ansi_colored() {
        // Red text: \x1b[31mhello\x1b[0m
        let colored = "\x1b[31mhello\x1b[0m";
        assert_eq!(strip_ansi(colored), "hello");
    }

    #[test]
    fn test_strip_ansi_bold() {
        // Bold: \x1b[1mbold\x1b[0m
        let bold = "\x1b[1mbold\x1b[0m";
        assert_eq!(strip_ansi(bold), "bold");
    }

    #[test]
    fn test_strip_ansi_complex() {
        // Multiple styles: \x1b[1;31;4mstuff\x1b[0m
        let complex = "\x1b[1;31;4mstuff\x1b[0m";
        assert_eq!(strip_ansi(complex), "stuff");
    }

    #[test]
    fn strip_ansi_handles_osc_sequences() {
        let input = "hello\x1b]1337;File=inline=1:abc\x07world";
        assert_eq!(strip_ansi(input), "helloworld");
    }

    #[test]
    fn strip_ansi_handles_osc_with_st_terminator() {
        let input = "hello\x1b]0;title\x1b\\world";
        assert_eq!(strip_ansi(input), "helloworld");
    }

    #[test]
    fn strip_ansi_handles_apc_sequences() {
        let input = "hello\x1b_Gi=31,s=1,v=1;AAAA\x1b\\world";
        assert_eq!(strip_ansi(input), "helloworld");
    }

    #[test]
    fn strip_ansi_handles_dcs_sequences() {
        let input = "hello\x1bPq#0;2~-\x1b\\world";
        assert_eq!(strip_ansi(input), "helloworld");
    }

    #[test]
    fn visible_width_excludes_osc_apc_dcs() {
        let input = "hi\x1b]1337;abc\x07there";
        assert_eq!(visible_width(input), 7); // "hi" + "there"
    }

    #[test]
    fn test_visible_width_ascii() {
        assert_eq!(visible_width("hello"), 5);
    }

    #[test]
    fn test_visible_width_with_ansi() {
        let colored = "\x1b[31mhello\x1b[0m";
        assert_eq!(visible_width(colored), 5);
    }

    #[test]
    fn test_visible_width_unicode() {
        // CJK characters are typically double-width
        assert_eq!(visible_width("你好"), 4);
    }

    #[test]
    fn test_visible_width_emoji() {
        let width = visible_width("🐋");
        assert!(width > 0);
    }

    #[test]
    fn test_visual_rows_no_wrap() {
        assert_eq!(visual_rows("hello", 80), 1);
        assert_eq!(visual_rows("hello world", 80), 1);
    }

    #[test]
    fn test_visual_rows_exact_width() {
        assert_eq!(visual_rows("12345", 5), 1);
        assert_eq!(visual_rows("1234567890", 10), 1);
    }

    #[test]
    fn test_visual_rows_with_wrap() {
        assert_eq!(visual_rows("123456", 5), 2);
        assert_eq!(visual_rows("12345678901", 5), 3);
    }

    #[test]
    fn test_visual_rows_empty() {
        assert_eq!(visual_rows("", 80), 1);
    }

    #[test]
    fn test_visual_rows_zero_width() {
        assert_eq!(visual_rows("hello", 0), 1);
    }

    #[test]
    fn test_visual_rows_with_ansi() {
        let colored = "\x1b[31mhello\x1b[0m";
        assert_eq!(visual_rows(colored, 80), 1);
        assert_eq!(visual_rows(colored, 3), 2);
    }

    #[test]
    fn test_wrap_styled_text_preserves_styles() {
        let spans = vec![
            ("Hello ".to_string(), String::new()),
            ("bold".to_string(), "\x1b[1m".to_string()),
            (" world".to_string(), String::new()),
        ];
        let wrapped = wrap_styled_text(&spans, 80);
        assert_eq!(wrapped.len(), 1);
        assert!(wrapped[0].contains("\x1b[1m"), "Should contain bold code");
        assert!(wrapped[0].contains("bold"), "Should contain bold text");
        assert!(wrapped[0].contains("\x1b[0m"), "Should contain reset code");
    }

    #[test]
    fn test_wrap_styled_text_wraps_correctly() {
        let spans = vec![
            ("This is ".to_string(), String::new()),
            ("styled".to_string(), "\x1b[1m".to_string()),
            (" text that wraps".to_string(), String::new()),
        ];
        let wrapped = wrap_styled_text(&spans, 15);
        assert!(wrapped.len() > 1, "Should wrap to multiple lines");
        for line in &wrapped {
            assert!(
                visible_width(line) <= 15,
                "Line exceeds width: {}",
                visible_width(line)
            );
        }
    }

    #[test]
    fn test_wrap_styled_text_style_spans_lines() {
        let spans = vec![(
            "A very long styled phrase".to_string(),
            "\x1b[1m".to_string(),
        )];
        let wrapped = wrap_styled_text(&spans, 10);
        assert!(wrapped.len() > 1);
        for line in &wrapped {
            if !line.is_empty() && visible_width(line) > 0 {
                assert!(line.contains("\x1b[1m"), "Each line should have style");
                assert!(line.contains("\x1b[0m"), "Each line should have reset");
            }
        }
    }
}
