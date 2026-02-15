//! String truncation utilities
//!
//! Canonical implementations for truncating strings by visible width
//! or character count.

use std::borrow::Cow;

use unicode_width::UnicodeWidthChar;

// Re-export ANSI utilities that were previously in the utils module
pub use crate::ansi::{apply_style, strip_ansi, visible_width, visual_rows};

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
/// use crucible_oil::utils::truncate_to_width;
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
/// use crucible_oil::utils::truncate_to_chars;
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
        assert_eq!(truncate_to_width("hello world", 5, true), "hell…");
    }

    #[test]
    fn width_truncation_without_ellipsis() {
        assert_eq!(truncate_to_width("hello world", 5, false), "hello");
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
        assert!(matches!(
            truncate_to_width("hello", 5, false),
            Cow::Borrowed(_)
        ));
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
}
