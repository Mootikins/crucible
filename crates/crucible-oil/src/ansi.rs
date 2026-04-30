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

    // Build wrapped lines with their starting character offset in the original text.
    // textwrap consumes whitespace between words (+1 offset), but char-level chunking
    // of long words does NOT (+0 offset between chunks).
    // Build wrapped lines with their starting character offset in the original text.
    // textwrap may consume whitespace between words OR break long words directly.
    // We detect whitespace gaps by checking the original text.
    let plain_chars: Vec<char> = plain_text.chars().collect();
    let mut wrapped_lines: Vec<(String, usize)> = Vec::new();
    let mut char_offset = 0;
    for line in initial_wrapped {
        let char_count = line.chars().count();
        if char_count <= width {
            wrapped_lines.push((line, char_offset));
            char_offset += char_count;
        } else {
            let chars: Vec<char> = line.chars().collect();
            for chunk in chars.chunks(width) {
                let chunk_str: String = chunk.iter().collect();
                wrapped_lines.push((chunk_str, char_offset));
                char_offset += chunk.len();
            }
        }
        // Skip whitespace at the current position (consumed by textwrap between words).
        // Don't skip if we're at the end or if the next char isn't whitespace.
        if char_offset < plain_chars.len() && plain_chars[char_offset].is_whitespace() {
            char_offset += 1;
        }
    }

    let mut result: Vec<String> = Vec::new();

    for (line, line_start) in &wrapped_lines {
        let line_char_count = line.chars().count();
        let line_end = line_start + line_char_count;

        let mut output = String::new();
        let mut current_pos = *line_start;

        for span in &styled_spans {
            if span.end <= *line_start || span.start >= line_end {
                continue;
            }

            let overlap_start = span.start.max(*line_start);
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

/// Extract the background-color contribution of a (possibly compound) SGR
/// style string, returning a standalone escape sequence that, when prepended
/// to another style's content, restores the same bg.
///
/// Returns `None` if the style does not set a background color, OR if it ends
/// in a reset (`SGR 0` or `SGR 49`) that clears any prior bg.
///
/// Used by [`crate::cell_grid::CellGrid::blit_line`] to compose styles when a
/// new write does not specify its own bg — the prior cell's bg is preserved
/// so a parent Box's `style.bg` survives child renders, matching the CSS
/// background-color semantics.
pub fn extract_bg(style: &str) -> Option<String> {
    let mut last_bg: Option<String> = None;
    let mut i = 0;
    let bytes = style.as_bytes();

    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            let start = i;
            i += 2;
            // SGR parameter bytes are digits + ';' (0x30..=0x3F). Anything else
            // means this isn't an SGR escape — bail past `\x1b[` so we don't
            // walk into a following SGR's terminator and misparse them as one.
            let params_start = i;
            while i < bytes.len() && matches!(bytes[i], b'0'..=b'9' | b';') {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'm' {
                // Not SGR (cursor move, OSC, truncated, etc.). Resume scanning
                // immediately after `\x1b[`.
                i = params_start;
                continue;
            }
            i += 1; // consume 'm'
            let escape = &style[start..i];
            // Strip "\x1b[" prefix and "m" suffix, split on ';'
            let inner = &escape[2..escape.len() - 1];
            let params: Vec<i32> = if inner.is_empty() {
                vec![0]
            } else {
                inner
                    .split(';')
                    .map(|p| p.parse::<i32>().unwrap_or(-1))
                    .collect()
            };

            let mut p = 0;
            while p < params.len() {
                match params[p] {
                    0 | 49 => {
                        // Full reset OR bg-default — clears any prior bg.
                        last_bg = None;
                        p += 1;
                    }
                    40..=47 | 100..=107 => {
                        // Basic 8-color bg or bright bg — single param.
                        last_bg = Some(format!("\x1b[{}m", params[p]));
                        p += 1;
                    }
                    48 => {
                        // Extended bg: 48;5;N (256-color) or 48;2;R;G;B (RGB).
                        match params.get(p + 1) {
                            Some(&5) if params.len() > p + 2 => {
                                last_bg =
                                    Some(format!("\x1b[48;5;{}m", params[p + 2]));
                                p += 3;
                            }
                            Some(&2) if params.len() > p + 4 => {
                                last_bg = Some(format!(
                                    "\x1b[48;2;{};{};{}m",
                                    params[p + 2],
                                    params[p + 3],
                                    params[p + 4]
                                ));
                                p += 5;
                            }
                            _ => p += 1,
                        }
                    }
                    38 => {
                        // Extended fg — must be skipped over so its R/G/B
                        // components aren't misread as basic-bg codes.
                        match params.get(p + 1) {
                            Some(&5) if params.len() > p + 2 => p += 3,
                            Some(&2) if params.len() > p + 4 => p += 5,
                            _ => p += 1,
                        }
                    }
                    _ => p += 1,
                }
            }
        } else {
            i += 1;
        }
    }

    last_bg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_bg_returns_none_for_no_bg() {
        assert_eq!(extract_bg(""), None);
        assert_eq!(extract_bg("\x1b[38;2;100;200;50m"), None);
        assert_eq!(extract_bg("\x1b[1m"), None);
    }

    #[test]
    fn extract_bg_handles_rgb_bg() {
        assert_eq!(
            extract_bg("\x1b[48;2;40;44;52m").as_deref(),
            Some("\x1b[48;2;40;44;52m")
        );
    }

    #[test]
    fn extract_bg_handles_256color_bg() {
        assert_eq!(
            extract_bg("\x1b[48;5;236m").as_deref(),
            Some("\x1b[48;5;236m")
        );
    }

    #[test]
    fn extract_bg_handles_basic_bg() {
        assert_eq!(extract_bg("\x1b[42m").as_deref(), Some("\x1b[42m"));
        assert_eq!(extract_bg("\x1b[105m").as_deref(), Some("\x1b[105m"));
    }

    #[test]
    fn extract_bg_returns_last_bg_when_multiple() {
        let style = "\x1b[48;2;0;0;0m\x1b[48;2;255;255;255m";
        assert_eq!(
            extract_bg(style).as_deref(),
            Some("\x1b[48;2;255;255;255m")
        );
    }

    #[test]
    fn extract_bg_returns_none_after_full_reset() {
        let style = "\x1b[48;2;40;44;52m\x1b[0m";
        assert_eq!(extract_bg(style), None);
    }

    #[test]
    fn extract_bg_returns_none_after_bg_default() {
        // SGR 49 = default bg
        let style = "\x1b[48;2;40;44;52m\x1b[49m";
        assert_eq!(extract_bg(style), None);
    }

    #[test]
    fn extract_bg_skips_non_sgr_escapes() {
        // Cursor positioning escape followed by a real bg-setting SGR.
        // The non-SGR escape must not consume the SGR's terminator.
        let style = "\x1b[H\x1b[42m";
        assert_eq!(extract_bg(style).as_deref(), Some("\x1b[42m"));
    }

    #[test]
    fn extract_bg_handles_truncated_escape() {
        // Truncated SGR (no terminator) followed by a real bg.
        let style = "\x1b[31\x1b[42m";
        // First escape lacks `m`; the parser must skip it and find the second.
        assert_eq!(extract_bg(style).as_deref(), Some("\x1b[42m"));
    }

    #[test]
    fn extract_bg_handles_compound_fg_and_bg_in_one_escape() {
        // crossterm sometimes emits combined: \x1b[38;2;...;48;2;...m
        let style = "\x1b[38;2;255;255;255;48;2;40;44;52m";
        assert_eq!(
            extract_bg(style).as_deref(),
            Some("\x1b[48;2;40;44;52m")
        );
    }

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
