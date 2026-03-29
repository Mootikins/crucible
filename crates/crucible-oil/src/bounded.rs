//! Bounded render wrappers — cap rendered content to a max number of visible lines.
//!
//! These are **wrapper functions**, not new `Node` variants. They pre-render the inner
//! node to count lines, then build a new `Node` with capped lines + an overflow indicator.
//!
//! - [`bounded`]: Shows the **last** `max_lines` lines (tail). Overflow indicator at top.
//! - [`bounded_head`]: Shows the **first** `max_lines` lines (head). Overflow indicator at bottom.

use crate::node::{col, styled, text, Node};
use crate::render::render_to_plain_text;
use crate::style::Style;

/// Pre-render content and collect lines. Returns `None` if content fits within `max_lines`.
fn pre_render_lines(content: &Node, max_lines: usize) -> Option<(Vec<String>, usize)> {
    let plain = render_to_plain_text(content, 4096);
    let all_lines: Vec<String> = plain.lines().map(String::from).collect();
    let total = all_lines.len();
    if total <= max_lines {
        None
    } else {
        let hidden = total - max_lines;
        Some((all_lines, hidden))
    }
}

fn overflow_indicator(hidden: usize, indent: usize) -> Node {
    let pad = " ".repeat(indent);
    styled(format!("{pad}({hidden} more lines)"), Style::new().dim())
}

/// Detect leading whitespace of the first non-empty line in the tail.
fn detect_indent(lines: &[String]) -> usize {
    lines
        .iter()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .unwrap_or(0)
}

/// Cap rendered content to show the **last** `max_lines` visible lines (tail view).
///
/// If content exceeds `max_lines`, a dimmed `(N more lines)` indicator appears at the top.
pub fn bounded(content: Node, max_lines: usize) -> Node {
    if max_lines == 0 {
        return styled("(all content hidden)", Style::new().dim());
    }

    let Some((all_lines, hidden)) = pre_render_lines(&content, max_lines) else {
        return content;
    };

    let tail = &all_lines[hidden..];
    let indent = detect_indent(tail);
    let body = text(tail.join("\n"));
    col([overflow_indicator(hidden, indent), body])
}

/// Cap rendered content to show the **first** `max_lines` visible lines (head view).
///
/// If content exceeds `max_lines`, a dimmed `(N more lines)` indicator appears at the bottom.
pub fn bounded_head(content: Node, max_lines: usize) -> Node {
    if max_lines == 0 {
        return styled("(all content hidden)", Style::new().dim());
    }

    let Some((all_lines, hidden)) = pre_render_lines(&content, max_lines) else {
        return content;
    };

    let head = &all_lines[..max_lines];
    let indent = detect_indent(head);
    let body = text(head.join("\n"));
    col([body, overflow_indicator(hidden, indent)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render_to_plain_text;

    #[test]
    fn bounded_exact_fit_no_overflow() {
        let content = text("line1\nline2\nline3");
        let result = bounded(content, 3);
        let plain = render_to_plain_text(&result, 80);
        assert!(
            !plain.contains("more lines"),
            "no overflow indicator when content fits: {plain:?}"
        );
        assert!(plain.contains("line1"));
        assert!(plain.contains("line2"));
        assert!(plain.contains("line3"));
    }

    #[test]
    fn bounded_overflow_shows_indicator() {
        let content = text("line1\nline2\nline3\nline4\nline5");
        let result = bounded(content, 3);
        let plain = render_to_plain_text(&result, 80);
        assert!(
            plain.contains("(2 more lines)"),
            "should show overflow indicator: {plain:?}"
        );
        assert!(plain.contains("line3"));
        assert!(plain.contains("line4"));
        assert!(plain.contains("line5"));
        assert!(
            !plain.contains("line1"),
            "line1 should be hidden: {plain:?}"
        );
        assert!(
            !plain.contains("line2"),
            "line2 should be hidden: {plain:?}"
        );
    }

    #[test]
    fn bounded_one_line() {
        let content = text("line1\nline2\nline3");
        let result = bounded(content, 1);
        let plain = render_to_plain_text(&result, 80);
        assert!(
            plain.contains("(2 more lines)"),
            "should show 2 hidden: {plain:?}"
        );
        assert!(plain.contains("line3"), "only last line visible: {plain:?}");
        assert!(
            !plain.contains("line1"),
            "line1 should be hidden: {plain:?}"
        );
    }

    #[test]
    fn bounded_empty_content() {
        let result = bounded(Node::Empty, 5);
        let plain = render_to_plain_text(&result, 80);
        assert!(
            !plain.contains("more lines"),
            "no indicator for empty content: {plain:?}"
        );
    }

    #[test]
    fn bounded_head_overflow_shows_indicator_at_bottom() {
        let content = text("line1\nline2\nline3\nline4\nline5");
        let result = bounded_head(content, 3);
        let plain = render_to_plain_text(&result, 80);
        assert!(
            plain.contains("(2 more lines)"),
            "should show overflow indicator: {plain:?}"
        );
        assert!(plain.contains("line1"));
        assert!(plain.contains("line2"));
        assert!(plain.contains("line3"));
        assert!(
            !plain.contains("line4"),
            "line4 should be hidden: {plain:?}"
        );
        assert!(
            !plain.contains("line5"),
            "line5 should be hidden: {plain:?}"
        );
    }

    #[test]
    fn bounded_head_exact_fit_no_overflow() {
        let content = text("line1\nline2\nline3");
        let result = bounded_head(content, 3);
        let plain = render_to_plain_text(&result, 80);
        assert!(
            !plain.contains("more lines"),
            "no overflow indicator when content fits: {plain:?}"
        );
        assert!(plain.contains("line1"));
        assert!(plain.contains("line3"));
    }

    #[test]
    fn bounded_zero_max_lines() {
        let content = text("line1\nline2");
        let result = bounded(content, 0);
        let plain = render_to_plain_text(&result, 80);
        assert!(
            plain.contains("all content hidden"),
            "max_lines=0 shows hidden message: {plain:?}"
        );
    }

    #[test]
    fn bounded_indicator_directly_adjacent_to_content() {
        // Verify the overflow indicator sits directly next to content with no blank
        // line spacer. This is intentional: bounded views are space-constrained, and
        // the dim styling + parenthesized format provide sufficient visual separation.
        let lines: Vec<String> = (1..=10).map(|i| format!("content line {i}")).collect();
        let content = text(lines.join("\n"));
        let result = bounded(content, 3);
        let rendered = crate::render::render_to_string(&result, 80);
        let stripped = crate::ansi::strip_ansi(&rendered);
        let output_lines: Vec<&str> = stripped.lines().collect();

        // Expect exactly 4 lines: indicator + 3 content lines, no blank spacers
        assert_eq!(output_lines.len(), 4, "expected 4 lines, got: {output_lines:?}");
        assert!(output_lines[0].contains("(7 more lines)"));
        assert!(output_lines[1].contains("content line 8"));
        assert!(output_lines[2].contains("content line 9"));
        assert!(output_lines[3].contains("content line 10"));
    }

    #[test]
    fn bounded_head_indicator_directly_adjacent_to_content() {
        let lines: Vec<String> = (1..=10).map(|i| format!("content line {i}")).collect();
        let content = text(lines.join("\n"));
        let result = bounded_head(content, 3);
        let rendered = crate::render::render_to_string(&result, 80);
        let stripped = crate::ansi::strip_ansi(&rendered);
        let output_lines: Vec<&str> = stripped.lines().collect();

        assert_eq!(output_lines.len(), 4, "expected 4 lines, got: {output_lines:?}");
        assert!(output_lines[0].contains("content line 1"));
        assert!(output_lines[1].contains("content line 2"));
        assert!(output_lines[2].contains("content line 3"));
        assert!(output_lines[3].contains("(7 more lines)"));
    }

    #[test]
    fn bounded_max_usize_no_cap() {
        let content = text("line1\nline2\nline3");
        let result = bounded(content, usize::MAX);
        let plain = render_to_plain_text(&result, 80);
        assert!(!plain.contains("more lines"));
        assert!(plain.contains("line1"));
        assert!(plain.contains("line3"));
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use crate::render::render_to_plain_text;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn bounded_respects_max_lines(
            line_count in 0usize..50,
            max_lines in 1usize..20,
        ) {
            let lines: Vec<String> = (0..line_count).map(|i| format!("line{i}")).collect();
            let content = text(lines.join("\n"));
            let result = bounded(content, max_lines);
            let plain = render_to_plain_text(&result, 80);
            let rendered_lines = if plain.is_empty() { 0 } else { plain.lines().count() };
            prop_assert!(
                rendered_lines <= max_lines + 1,
                "rendered {} lines but max is {} (+1 indicator). content:\n{}",
                rendered_lines, max_lines, plain
            );
        }

        #[test]
        fn bounded_head_respects_max_lines(
            line_count in 0usize..50,
            max_lines in 1usize..20,
        ) {
            let lines: Vec<String> = (0..line_count).map(|i| format!("line{i}")).collect();
            let content = text(lines.join("\n"));
            let result = bounded_head(content, max_lines);
            let plain = render_to_plain_text(&result, 80);
            let rendered_lines = if plain.is_empty() { 0 } else { plain.lines().count() };
            prop_assert!(
                rendered_lines <= max_lines + 1,
                "rendered {} lines but max is {} (+1 indicator). content:\n{}",
                rendered_lines, max_lines, plain
            );
        }
    }
}
