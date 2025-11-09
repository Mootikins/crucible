//! Block Text Normalization for Content-Addressed Block Links
//!
//! This module provides text normalization for computing stable block hashes
//! used in block link validation. The normalization preserves markdown semantics
//! while removing formatting variability.
//!
//! ## Normalization Rules
//!
//! 1. **Whitespace Normalization**:
//!    - Strip leading/trailing whitespace
//!    - Collapse multiple spaces to single space
//!    - Normalize line endings to \n
//!
//! 2. **Markdown Preservation**:
//!    - Keep bold (**text**, __text__)
//!    - Keep italic (*text*, _text_)
//!    - Keep highlights (==text==)
//!    - Keep code (`code`)
//!    - Keep strikethrough (~~text~~)
//!
//! 3. **Stripped Elements**:
//!    - Leading/trailing whitespace per line
//!    - Empty lines
//!    - Bullet markers (-, *, +) for lists
//!    - Numbered list markers (1., 2., etc.)
//!    - Blockquote markers (>)
//!    - Heading markers (#)
//!
//! ## Example
//!
//! ```rust
//! use crucible_core::hashing::normalize_block_text;
//!
//! let text = "  This is **bold** and ==highlighted==  ";
//! let normalized = normalize_block_text(text);
//! assert_eq!(normalized, "This is **bold** and ==highlighted==");
//! ```

/// Normalize block text for stable content hashing
///
/// This function strips whitespace and leading characters while preserving
/// markdown formatting for LLM-friendly block link resolution.
///
/// # Arguments
///
/// * `text` - The raw block text to normalize
///
/// # Returns
///
/// Normalized text suitable for BLAKE3 hashing
///
/// # Examples
///
/// ```
/// use crucible_core::hashing::normalize_block_text;
///
/// // Whitespace normalization
/// assert_eq!(normalize_block_text("  hello  "), "hello");
/// assert_eq!(normalize_block_text("hello\n\n\nworld"), "hello\nworld");
///
/// // Markdown preservation
/// assert_eq!(normalize_block_text("**bold** text"), "**bold** text");
/// assert_eq!(normalize_block_text("==highlight=="), "==highlight==");
///
/// // Leading character removal
/// assert_eq!(normalize_block_text("- list item"), "list item");
/// assert_eq!(normalize_block_text("> quote"), "quote");
/// assert_eq!(normalize_block_text("# heading"), "heading");
/// ```
pub fn normalize_block_text(text: &str) -> String {
    text.lines()
        .map(|line| {
            // Strip leading/trailing whitespace
            let trimmed = line.trim();

            // Skip empty lines
            if trimmed.is_empty() {
                return String::new();
            }

            // Remove leading characters (list markers, blockquote, headings)
            let content = remove_leading_markers(trimmed);

            // Collapse multiple spaces to single space
            collapse_spaces(&content)
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Remove leading markdown markers (lists, blockquotes, headings)
fn remove_leading_markers(text: &str) -> String {
    let mut remaining = text;

    // Remove heading markers (# must be followed by space or another #)
    loop {
        if let Some(stripped) = remaining.strip_prefix('#') {
            if stripped.starts_with(|c: char| c.is_whitespace()) {
                // "# text" → " text" → trim → "text"
                remaining = stripped.trim_start();
                break;
            } else if stripped.is_empty() {
                // Just "#"
                remaining = "";
                break;
            } else {
                // "##..." continue loop
                remaining = stripped;
            }
        } else {
            break;
        }
    }

    // Save state after heading removal for blockquote fallback
    let after_headings = remaining;

    // Remove blockquote markers (> must be followed by space or another >)
    loop {
        if let Some(stripped) = remaining.strip_prefix('>') {
            if stripped.starts_with(|c: char| c.is_whitespace()) {
                // "> text" → " text" → trim → "text"
                remaining = stripped.trim_start();
                break;
            } else if stripped.is_empty() {
                // Just ">"
                remaining = "";
                break;
            } else if stripped.starts_with('>') {
                // ">>..." continue loop
                remaining = stripped;
            } else {
                // ">text" without space - NOT a blockquote, restore
                remaining = after_headings;
                break;
            }
        } else {
            break;
        }
    }

    // Handle list markers (-, *, +, or numbered)
    let remaining = remaining;

    // Unordered lists (with space or at end of line)
    if remaining.starts_with("- ") || remaining.starts_with("* ") || remaining.starts_with("+ ") {
        return remaining[2..].to_string();
    }
    if remaining == "-" || remaining == "*" || remaining == "+" {
        return String::new();
    }

    // Numbered lists (e.g., "1. ", "12. ")
    if let Some(dot_pos) = remaining.find('.') {
        if dot_pos > 0 && remaining[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
            if remaining.len() > dot_pos + 1 && remaining.chars().nth(dot_pos + 1) == Some(' ') {
                return remaining[dot_pos + 2..].to_string();
            }
        }
    }

    remaining.to_string()
}

/// Collapse multiple consecutive spaces to a single space
fn collapse_spaces(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last_was_space = false;

    for c in text.chars() {
        if c.is_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(c);
            last_was_space = false;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_whitespace_normalization() {
        assert_eq!(normalize_block_text("  hello  "), "hello");
        assert_eq!(normalize_block_text("hello\n\nworld"), "hello\nworld");
        assert_eq!(normalize_block_text("  spaced  out  "), "spaced out");
    }

    #[test]
    fn test_markdown_preservation() {
        assert_eq!(normalize_block_text("**bold** text"), "**bold** text");
        assert_eq!(normalize_block_text("*italic* text"), "*italic* text");
        assert_eq!(normalize_block_text("==highlighted=="), "==highlighted==");
        assert_eq!(normalize_block_text("`code`"), "`code`");
        assert_eq!(normalize_block_text("~~strikethrough~~"), "~~strikethrough~~");

        // Mixed formatting
        assert_eq!(
            normalize_block_text("**bold** and *italic* with ==highlight=="),
            "**bold** and *italic* with ==highlight=="
        );
    }

    #[test]
    fn test_list_marker_removal() {
        assert_eq!(normalize_block_text("- list item"), "list item");
        assert_eq!(normalize_block_text("* bullet point"), "bullet point");
        assert_eq!(normalize_block_text("+ another item"), "another item");
        assert_eq!(normalize_block_text("1. numbered"), "numbered");
        assert_eq!(normalize_block_text("42. answer"), "answer");
    }

    #[test]
    fn test_blockquote_removal() {
        assert_eq!(normalize_block_text("> quote"), "quote");
        assert_eq!(normalize_block_text(">> nested"), "nested");
        assert_eq!(normalize_block_text(">    spaced"), "spaced");
    }

    #[test]
    fn test_heading_removal() {
        assert_eq!(normalize_block_text("# Heading 1"), "Heading 1");
        assert_eq!(normalize_block_text("## Heading 2"), "Heading 2");
        assert_eq!(normalize_block_text("### Heading 3"), "Heading 3");
    }

    #[test]
    fn test_combined_markers() {
        assert_eq!(normalize_block_text("> - quote list"), "quote list");
        assert_eq!(normalize_block_text("# - heading list"), "heading list");
    }

    #[test]
    fn test_preserve_inline_formatting() {
        assert_eq!(
            normalize_block_text("- This is **important** and ==critical=="),
            "This is **important** and ==critical=="
        );
        assert_eq!(
            normalize_block_text("> Use `function()` for *processing*"),
            "Use `function()` for *processing*"
        );
    }

    #[test]
    fn test_empty_lines_removed() {
        assert_eq!(normalize_block_text("hello\n\n\nworld"), "hello\nworld");
        assert_eq!(normalize_block_text("\n\nhello\n\n"), "hello");
    }

    #[test]
    fn test_space_collapsing() {
        assert_eq!(normalize_block_text("hello    world"), "hello world");
        assert_eq!(normalize_block_text("too   many   spaces"), "too many spaces");
    }

    #[test]
    fn test_real_world_examples() {
        // Typical paragraph from note
        let input = "  This concept relates to [[Other Note]] and ==highlights== the **key insight**.  ";
        let expected = "This concept relates to [[Other Note]] and ==highlights== the **key insight**.";
        assert_eq!(normalize_block_text(input), expected);

        // List item with formatting
        let input = "- The **primary benefit** is `performance` and *reliability*";
        let expected = "The **primary benefit** is `performance` and *reliability*";
        assert_eq!(normalize_block_text(input), expected);

        // Blockquote with citation
        let input = "> Important: *always* validate ==user input==";
        let expected = "Important: *always* validate ==user input==";
        assert_eq!(normalize_block_text(input), expected);
    }

    #[test]
    fn test_multiline_normalization() {
        let input = "  First line  \n  Second line  \n\n  Third line  ";
        let expected = "First line\nSecond line\nThird line";
        assert_eq!(normalize_block_text(input), expected);
    }

    #[test]
    fn test_edge_cases() {
        // Empty input
        assert_eq!(normalize_block_text(""), "");
        assert_eq!(normalize_block_text("   "), "");

        // Only markers
        assert_eq!(normalize_block_text("- "), "");
        assert_eq!(normalize_block_text("> "), "");
        assert_eq!(normalize_block_text("# "), "");

        // Markers without space
        assert_eq!(normalize_block_text("-text"), "-text"); // Not a list
        assert_eq!(normalize_block_text(">text"), ">text"); // Not a blockquote
    }
}
