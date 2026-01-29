//! String truncation utilities
//!
//! Canonical implementations for truncating strings by visible width,
//! character count, or line count.

use std::borrow::Cow;

use unicode_width::UnicodeWidthChar;

/// Truncate a string to fit within `max_width` visible columns.
///
/// Accounts for ANSI escape codes (they don't consume width).
/// Optionally adds ellipsis (`…`) if truncated.
///
/// # Arguments
/// * `s` - The string to truncate
/// * `max_width` - Maximum visible width in columns
/// * `ellipsis` - Whether to add `…` when truncated (consumes 1 column)
///
/// # Returns
/// * `Cow::Borrowed` if no truncation needed (avoids allocation)
/// * `Cow::Owned` if truncated
///
/// # Examples
/// ```
/// use crucible_cli::tui::oil::utils::truncate::truncate_to_width;
///
/// assert_eq!(truncate_to_width("hello", 10, false), "hello");
/// assert_eq!(truncate_to_width("hello world", 5, true), "hell…");
/// assert_eq!(truncate_to_width("hello world", 5, false), "hello");
/// ```
pub fn truncate_to_width(s: &str, max_width: usize, ellipsis: bool) -> Cow<'_, str> {
    if max_width == 0 {
        return if ellipsis {
            Cow::Borrowed("…")
        } else {
            Cow::Borrowed("")
        };
    }

    // Quick check: if string is short enough, return borrowed
    // This is a heuristic - visible width <= byte length
    if s.len() <= max_width && !s.contains('\x1b') {
        // Still need to check actual width for wide chars
        let width = visible_width_simple(s);
        if width <= max_width {
            return Cow::Borrowed(s);
        }
    }

    let effective_max = if ellipsis {
        max_width.saturating_sub(1)
    } else {
        max_width
    };

    let mut result = String::new();
    let mut current_width = 0;
    let mut chars = s.chars().peekable();
    let mut truncated = false;

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Start of ANSI escape sequence - include but don't count width
            result.push(c);
            if chars.peek() == Some(&'[') {
                result.push(chars.next().unwrap());
                // Consume until we hit the command character (a letter)
                while let Some(&next) = chars.peek() {
                    result.push(chars.next().unwrap());
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            let char_width = c.width().unwrap_or(0);
            if current_width + char_width > effective_max {
                truncated = true;
                break;
            }
            result.push(c);
            current_width += char_width;
        }
    }

    // Check if we consumed everything (no truncation actually needed)
    if !truncated && chars.peek().is_none() {
        // We might have modified the string (removed invalid escapes), so check
        if result == s {
            return Cow::Borrowed(s);
        }
    }

    if truncated && ellipsis {
        result.push('…');
    }

    Cow::Owned(result)
}

/// Truncate to character count (not width).
///
/// For cases where we want N chars regardless of display width
/// (e.g., IDs, hashes, fixed-format fields).
///
/// # Arguments
/// * `s` - The string to truncate
/// * `max_chars` - Maximum number of characters
/// * `ellipsis` - Whether to add `…` when truncated
///
/// # Examples
/// ```
/// use crucible_cli::tui::oil::utils::truncate::truncate_to_chars;
///
/// assert_eq!(truncate_to_chars("hello", 10, false), "hello");
/// assert_eq!(truncate_to_chars("hello world", 5, true), "hell…");
/// assert_eq!(truncate_to_chars("你好世界", 2, true), "你…");
/// ```
pub fn truncate_to_chars(s: &str, max_chars: usize, ellipsis: bool) -> Cow<'_, str> {
    if max_chars == 0 {
        return if ellipsis {
            Cow::Borrowed("…")
        } else {
            Cow::Borrowed("")
        };
    }

    let char_count = s.chars().count();
    if char_count <= max_chars {
        return Cow::Borrowed(s);
    }

    let effective_max = if ellipsis {
        max_chars.saturating_sub(1)
    } else {
        max_chars
    };

    let truncated: String = s.chars().take(effective_max).collect();
    if ellipsis {
        Cow::Owned(format!("{}…", truncated))
    } else {
        Cow::Owned(truncated)
    }
}

/// Truncate multi-line content to max_lines.
///
/// Adds "[+N more lines]" suffix when truncated.
/// Used for tool output preview.
///
/// # Arguments
/// * `s` - The multi-line string to truncate
/// * `max_lines` - Maximum number of lines to keep
///
/// # Examples
/// ```
/// use crucible_cli::tui::oil::utils::truncate::truncate_lines;
///
/// let text = "line1\nline2\nline3\nline4\nline5";
/// assert_eq!(truncate_lines(text, 3), "line1\nline2\nline3\n[+2 more lines]");
/// assert_eq!(truncate_lines(text, 10), "line1\nline2\nline3\nline4\nline5");
/// ```
pub fn truncate_lines(s: &str, max_lines: usize) -> Cow<'_, str> {
    if max_lines == 0 {
        let total = s.lines().count();
        if total > 0 {
            return Cow::Owned(format!("[+{} more lines]", total));
        } else {
            return Cow::Borrowed("");
        }
    }

    let lines: Vec<&str> = s.lines().collect();
    let total = lines.len();

    if total <= max_lines {
        return Cow::Borrowed(s);
    }

    let kept: Vec<&str> = lines.into_iter().take(max_lines).collect();
    let remaining = total - max_lines;

    Cow::Owned(format!(
        "{}\n[+{} more {}]",
        kept.join("\n"),
        remaining,
        if remaining == 1 { "line" } else { "lines" }
    ))
}

/// Extract first line and truncate to width.
///
/// Convenience function for single-line display of potentially multi-line content.
/// Combines extracting the first line with width truncation.
///
/// # Arguments
/// * `s` - The potentially multi-line string
/// * `max_width` - Maximum visible width for the first line
/// * `ellipsis` - Whether to add `…` when truncated
pub fn truncate_first_line(s: &str, max_width: usize, ellipsis: bool) -> Cow<'_, str> {
    let first_line = s.lines().next().unwrap_or(s);
    truncate_to_width(first_line, max_width, ellipsis)
}

/// Simple visible width calculation without ANSI handling.
/// For internal use when we know there are no ANSI codes.
fn visible_width_simple(s: &str) -> usize {
    s.chars().map(|c| c.width().unwrap_or(0)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== truncate_to_width tests ====================

    #[test]
    fn width_no_truncation_returns_borrowed() {
        let result = truncate_to_width("hello", 10, false);
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, "hello");
    }

    #[test]
    fn width_truncation_with_ellipsis() {
        assert_eq!(
            truncate_to_width("hello world", 5, true),
            "hell…"
        );
    }

    #[test]
    fn width_truncation_without_ellipsis() {
        assert_eq!(
            truncate_to_width("hello world", 5, false),
            "hello"
        );
    }

    #[test]
    fn width_zero_with_ellipsis() {
        assert_eq!(truncate_to_width("hello", 0, true), "…");
    }

    #[test]
    fn width_zero_without_ellipsis() {
        assert_eq!(truncate_to_width("hello", 0, false), "");
    }

    #[test]
    fn width_empty_string() {
        assert_eq!(truncate_to_width("", 10, false), "");
        assert_eq!(truncate_to_width("", 10, true), "");
    }

    #[test]
    fn width_preserves_ansi_codes() {
        let styled = "\x1b[31mhello world\x1b[0m";
        let result = truncate_to_width(styled, 5, true);
        // Should contain the color code and truncated text
        assert!(result.contains("\x1b[31m"));
        assert!(result.contains("hell"));
        assert!(result.ends_with('…'));
        // Should NOT contain "world"
        assert!(!result.contains("world"));
    }

    #[test]
    fn width_handles_wide_chars() {
        // CJK characters are typically 2 columns wide
        let cjk = "你好世界";
        // "你好" = 4 columns, so max_width=5 should fit "你好" + partial
        let result = truncate_to_width(cjk, 5, true);
        // Should truncate since 你好世界 = 8 columns
        assert!(result.ends_with('…'));
    }

    #[test]
    fn width_exact_fit_no_ellipsis() {
        assert_eq!(truncate_to_width("hello", 5, false), "hello");
        assert!(matches!(truncate_to_width("hello", 5, false), Cow::Borrowed(_)));
    }

    // ==================== truncate_to_chars tests ====================

    #[test]
    fn chars_no_truncation_returns_borrowed() {
        let result = truncate_to_chars("hello", 10, false);
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, "hello");
    }

    #[test]
    fn chars_truncation_with_ellipsis() {
        assert_eq!(truncate_to_chars("hello world", 5, true), "hell…");
    }

    #[test]
    fn chars_truncation_without_ellipsis() {
        assert_eq!(truncate_to_chars("hello world", 5, false), "hello");
    }

    #[test]
    fn chars_handles_multibyte_utf8() {
        // Each character is one char, regardless of byte length
        let utf8 = "café☕test";
        assert_eq!(truncate_to_chars(utf8, 6, true), "café☕…");
    }

    #[test]
    fn chars_handles_cjk() {
        let cjk = "你好世界";
        assert_eq!(truncate_to_chars(cjk, 2, true), "你…");
    }

    #[test]
    fn chars_zero_with_ellipsis() {
        assert_eq!(truncate_to_chars("hello", 0, true), "…");
    }

    #[test]
    fn chars_zero_without_ellipsis() {
        assert_eq!(truncate_to_chars("hello", 0, false), "");
    }

    // ==================== truncate_lines tests ====================

    #[test]
    fn lines_no_truncation_returns_borrowed() {
        let text = "line1\nline2";
        let result = truncate_lines(text, 5);
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, text);
    }

    #[test]
    fn lines_truncation_adds_count() {
        let text = "line1\nline2\nline3\nline4\nline5";
        let result = truncate_lines(text, 2);
        assert_eq!(result, "line1\nline2\n[+3 more lines]");
    }

    #[test]
    fn lines_truncation_singular() {
        let text = "line1\nline2";
        let result = truncate_lines(text, 1);
        assert_eq!(result, "line1\n[+1 more line]");
    }

    #[test]
    fn lines_zero_shows_total() {
        let text = "line1\nline2\nline3";
        let result = truncate_lines(text, 0);
        assert_eq!(result, "[+3 more lines]");
    }

    #[test]
    fn lines_empty_string() {
        assert_eq!(truncate_lines("", 5), "");
    }

    #[test]
    fn lines_single_line_no_truncation() {
        assert_eq!(truncate_lines("single", 5), "single");
    }

    // ==================== truncate_first_line tests ====================

    #[test]
    fn first_line_extracts_and_truncates() {
        let text = "hello world\nsecond line\nthird";
        assert_eq!(truncate_first_line(text, 5, true), "hell…");
    }

    #[test]
    fn first_line_no_newlines() {
        assert_eq!(truncate_first_line("hello", 10, false), "hello");
    }

    #[test]
    fn first_line_empty() {
        assert_eq!(truncate_first_line("", 10, false), "");
    }
}
