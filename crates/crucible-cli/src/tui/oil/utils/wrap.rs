//! Text wrapping utilities
//!
//! Canonical implementations for wrapping text to terminal width
//! while preserving ANSI escape codes.

pub use crucible_oil::ansi::wrap_styled_text;

/// Wrap text to fit within `max_width` visible columns.
///
/// Preserves ANSI escape codes across line breaks.
/// Respects existing newlines in the input.
///
/// # Arguments
/// * `s` - The string to wrap
/// * `max_width` - Maximum visible width in columns
///
/// # Returns
/// Wrapped text as a single string with newlines
///
/// # Examples
/// ```
/// use crucible_cli::tui::oil::utils::wrap::wrap_to_width;
///
/// let text = "This is a long line that needs to be wrapped";
/// let wrapped = wrap_to_width(text, 20);
/// assert!(wrapped.contains('\n'));
/// ```
pub fn wrap_to_width(s: &str, max_width: usize) -> String {
    if max_width == 0 || s.is_empty() {
        return s.to_string();
    }

    // Handle each line separately to preserve existing newlines
    let lines: Vec<String> = s
        .lines()
        .flat_map(|line| {
            if line.is_empty() {
                vec![String::new()]
            } else {
                // wrap_styled_text expects (text, ansi_codes) pairs
                // For plain text without pre-parsed styles, pass empty codes
                wrap_styled_text(&[(line.to_string(), String::new())], max_width)
            }
        })
        .collect();

    lines.join("\n")
}

/// Wrap with indent on continuation lines.
///
/// First line gets no indent, subsequent wrapped lines get `indent` spaces.
/// Useful for bulleted content where continuation should align.
///
/// # Arguments
/// * `s` - The string to wrap
/// * `max_width` - Maximum visible width in columns
/// * `indent` - Number of spaces to indent continuation lines
///
/// # Examples
/// ```
/// use crucible_cli::tui::oil::utils::wrap::wrap_to_width_indented;
///
/// let text = "This is content that will wrap to multiple lines";
/// let wrapped = wrap_to_width_indented(text, 20, 3);
/// // First line: "This is content that"
/// // Second line: "   will wrap to"  (3 space indent)
/// ```
pub fn wrap_to_width_indented(s: &str, max_width: usize, indent: usize) -> String {
    if max_width == 0 || s.is_empty() {
        return s.to_string();
    }

    let indent_str = " ".repeat(indent);

    // Handle each input line separately
    let result_lines: Vec<String> = s
        .lines()
        .flat_map(|line| {
            if line.is_empty() {
                vec![String::new()]
            } else {
                // First wrap to full width
                let first_wrap = wrap_styled_text(&[(line.to_string(), String::new())], max_width);

                if first_wrap.len() <= 1 {
                    // No wrapping needed
                    first_wrap
                } else {
                    // First line stays as-is, rest get indented
                    let mut result = Vec::with_capacity(first_wrap.len());
                    for (i, wrapped_line) in first_wrap.into_iter().enumerate() {
                        if i == 0 {
                            result.push(wrapped_line);
                        } else {
                            // Re-wrap continuation with reduced width and add indent
                            // For simplicity, just prepend indent (may exceed width slightly)
                            result.push(format!("{}{}", indent_str, wrapped_line.trim_start()));
                        }
                    }
                    result
                }
            }
        })
        .collect();

    result_lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::ansi::visible_width;

    #[test]
    fn wrap_short_line_unchanged() {
        let text = "hello";
        let result = wrap_to_width(text, 80);
        assert_eq!(result, "hello");
    }

    #[test]
    fn wrap_long_line() {
        let text = "this is a long line that needs wrapping";
        let result = wrap_to_width(text, 15);
        assert!(result.contains('\n'));
        for line in result.lines() {
            assert!(
                visible_width(line) <= 15,
                "Line too wide: {} ({})",
                line,
                visible_width(line)
            );
        }
    }

    #[test]
    fn wrap_preserves_existing_newlines() {
        let text = "line one\nline two";
        let result = wrap_to_width(text, 80);
        assert_eq!(result.lines().count(), 2);
    }

    #[test]
    fn wrap_empty_string() {
        assert_eq!(wrap_to_width("", 80), "");
    }

    #[test]
    fn wrap_zero_width_returns_original() {
        let text = "hello world";
        assert_eq!(wrap_to_width(text, 0), text);
    }

    #[test]
    fn wrap_indented_first_line_no_indent() {
        let text = "hello world test";
        let result = wrap_to_width_indented(text, 10, 3);
        let lines: Vec<&str> = result.lines().collect();
        // First line should not start with spaces (from indent)
        assert!(!lines[0].starts_with("   "));
    }

    #[test]
    fn wrap_indented_continuation_has_indent() {
        let text = "this is a long line that definitely needs wrapping to test indentation";
        let result = wrap_to_width_indented(text, 20, 4);
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines.len() > 1);
        // Continuation lines should start with indent
        for line in &lines[1..] {
            assert!(
                line.starts_with("    ") || line.is_empty(),
                "Line should be indented: {:?}",
                line
            );
        }
    }

    #[test]
    fn wrap_handles_blank_lines() {
        let text = "first\n\nlast";
        let result = wrap_to_width(text, 80);
        assert!(result.contains("\n\n") || result.lines().any(|l| l.is_empty()));
    }
}
