//! String truncation utilities
//!
//! `truncate_to_width` and `truncate_to_chars` live in `crucible_oil::utils`;
//! this module re-exports them and adds the line-count variants.

use std::borrow::Cow;

pub use crucible_oil::utils::{truncate_to_chars, truncate_to_width};

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

#[cfg(test)]
mod tests {
    use super::*;

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
