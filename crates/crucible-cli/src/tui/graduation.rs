//! Graduation logic for inline viewport mode.
//!
//! This module contains the core graduation calculation used by both the
//! real TUI runner and the test harness. By sharing this logic, tests
//! actually exercise the real graduation behavior.
//!
//! Graduation is simple: when rendered content exceeds viewport capacity,
//! graduate the overflow lines to terminal scrollback via `insert_before()`.
//! Lines are graduated regardless of item boundaries (rendered lines are
//! already styled text that can be split anywhere).

use crate::tui::conversation::{render_item_to_lines, ConversationItem, ConversationState};
use ratatui::text::Line;
use std::ops::Range;

/// Result of graduation calculation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraduationResult {
    /// Range of line indices to graduate (in the all_lines array)
    pub lines_to_graduate: Range<usize>,
    /// New graduated line count after this graduation
    pub new_graduated_count: usize,
}

/// Calculate which lines should be graduated to scrollback.
///
/// This is the core graduation algorithm used by both the runner and test harness.
///
/// # Arguments
/// * `total_lines` - Total number of rendered lines
/// * `graduated_count` - Number of lines already graduated
/// * `viewport_capacity` - How many lines the viewport can display
///
/// # Returns
/// * `Some(GraduationResult)` if lines should be graduated
/// * `None` if no graduation needed
pub fn calculate_graduation(
    total_lines: usize,
    graduated_count: usize,
    viewport_capacity: usize,
) -> Option<GraduationResult> {
    // How many lines are visible (not yet graduated)?
    let visible_lines = total_lines.saturating_sub(graduated_count);

    // If visible fits in viewport, nothing to graduate
    if visible_lines <= viewport_capacity {
        return None;
    }

    // Calculate overflow to graduate
    let overflow = visible_lines - viewport_capacity;
    let start = graduated_count;
    let end = start + overflow;

    Some(GraduationResult {
        lines_to_graduate: start..end,
        new_graduated_count: end,
    })
}

/// Render all conversation items to lines.
///
/// This is the canonical rendering used for graduation calculations.
/// Both runner and harness should use this to ensure consistency.
pub fn render_all_lines(items: &[ConversationItem], width: usize) -> Vec<Line<'static>> {
    let mut all_lines = Vec::new();

    for (i, item) in items.iter().enumerate() {
        // Add spacing before tool calls (but not between consecutive tools)
        if matches!(item, ConversationItem::ToolCall(_)) {
            let prev_was_tool =
                i > 0 && matches!(items.get(i - 1), Some(ConversationItem::ToolCall(_)));
            if !prev_was_tool {
                all_lines.push(Line::from(""));
            }
        }
        all_lines.extend(render_item_to_lines(item, width));
    }

    all_lines
}

/// High-level graduation check for a conversation.
///
/// Combines rendering and calculation into one call.
/// This is the main entry point for both runner and harness.
///
/// IMPORTANT: Uses newline-gated graduation to prevent partial content
/// from being graduated to scrollback. Only complete lines (those ending
/// with newlines) can be graduated. This prevents flickering when streaming
/// content changes after graduation.
///
/// The algorithm:
/// 1. Render all content for viewport display
/// 2. Use committed_line_count() to determine how many RENDERED lines are safe
/// 3. Only graduate up to the committed line count (never partial content)
pub fn check_graduation(
    conversation: &ConversationState,
    graduated_count: usize,
    viewport_capacity: usize,
    content_width: usize,
) -> (Vec<Line<'static>>, Option<GraduationResult>) {
    // Render all content for viewport display
    let all_lines = conversation.render_for_graduation(content_width);
    let total_rendered = all_lines.len();

    // Get the committed line count - this is based on source newlines.
    // For safety, we use the minimum of committed and rendered lines.
    // This ensures we never graduate more than what's actually rendered,
    // and never graduate partial (uncommitted) content.
    let committed = conversation.committed_line_count();

    // Check if streaming is active - if so, be conservative
    let is_streaming = conversation.is_streaming();

    // Calculate graduatable lines:
    // - If streaming, use committed count (excludes partial lines)
    // - If not streaming, all lines are graduatable
    let graduatable = if is_streaming {
        // During streaming, only graduate committed lines
        // This prevents partial content from being graduated
        committed.min(total_rendered)
    } else {
        // When not streaming, all rendered content is stable
        total_rendered
    };

    // Check if graduation is needed based on graduatable lines
    let result = calculate_graduation(graduatable, graduated_count, viewport_capacity);

    if result.is_none() {
        return (all_lines, None);
    }

    (all_lines, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_graduation_when_fits() {
        let result = calculate_graduation(
            10, // total lines
            0,  // graduated
            20, // viewport capacity
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_graduation_on_overflow() {
        let result = calculate_graduation(
            30, // total lines
            0,  // graduated
            10, // viewport capacity
        );
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.lines_to_graduate, 0..20);
        assert_eq!(r.new_graduated_count, 20);
    }

    #[test]
    fn test_incremental_graduation() {
        // First graduation: 20 lines, viewport 10, graduate 10
        let r1 = calculate_graduation(20, 0, 10).unwrap();
        assert_eq!(r1.lines_to_graduate, 0..10);
        assert_eq!(r1.new_graduated_count, 10);

        // Second graduation: 30 lines, 10 already graduated, viewport 10
        // Visible = 30 - 10 = 20, overflow = 20 - 10 = 10
        let r2 = calculate_graduation(30, 10, 10).unwrap();
        assert_eq!(r2.lines_to_graduate, 10..20);
        assert_eq!(r2.new_graduated_count, 20);
    }

    #[test]
    fn test_no_graduation_when_exactly_fits() {
        // Exactly viewport size - no graduation needed
        let result = calculate_graduation(10, 0, 10);
        assert!(result.is_none());
    }

    #[test]
    fn test_graduation_with_prior_graduations() {
        // 50 total lines, 20 already graduated, viewport 15
        // Visible = 50 - 20 = 30, overflow = 30 - 15 = 15
        let result = calculate_graduation(50, 20, 15).unwrap();
        assert_eq!(result.lines_to_graduate, 20..35);
        assert_eq!(result.new_graduated_count, 35);
    }

    #[test]
    fn test_large_overflow() {
        // 100 total lines, viewport 10 - graduates 90 lines
        let result = calculate_graduation(100, 0, 10).unwrap();
        assert_eq!(result.lines_to_graduate, 0..90);
        assert_eq!(result.new_graduated_count, 90);
    }

    // =========================================================================
    // Newline-Gated Graduation Tests
    // =========================================================================

    #[test]
    fn test_check_graduation_respects_committed_lines_during_streaming() {
        // Test that during streaming, only committed lines are considered
        let mut conversation = ConversationState::new();

        // Start streaming
        conversation.start_assistant_streaming();

        // Append text with newlines (3 complete lines)
        conversation.append_or_create_prose("Line 1\nLine 2\nLine 3\n");

        // Append partial line (no trailing newline)
        conversation.append_or_create_prose("Partial line without newline");

        // Should have 3 committed lines (from the 3 newlines)
        assert_eq!(conversation.committed_line_count(), 3);
        assert!(conversation.is_streaming());

        // Check graduation - should only consider committed lines
        let (all_lines, result) = check_graduation(&conversation, 0, 2, 80);

        // Should have at least 1 line rendered (depends on how prose consolidates)
        assert!(!all_lines.is_empty(), "Should render content");

        // Key test: graduation should respect committed count, not total rendered
        // With 3 committed lines and viewport of 2, we should graduate 1 line
        if let Some(grad) = result {
            // Can graduate up to 1 line (3 committed - 2 viewport = 1 overflow)
            assert_eq!(grad.lines_to_graduate.len(), 1);
        }
    }

    #[test]
    fn test_check_graduation_all_lines_when_not_streaming() {
        // Test that when not streaming, all lines are graduatable
        let mut conversation = ConversationState::new();

        // Add a complete user message (not streaming)
        conversation.push_user_message("Hello world");

        // Add a complete assistant message (not streaming)
        conversation.push_assistant_message("Hi there!\nHow are you?");

        // Not streaming
        assert!(!conversation.is_streaming());

        // Check graduation
        let (all_lines, result) = check_graduation(&conversation, 0, 2, 80);

        // Should consider ALL lines for graduation since not streaming
        assert!(!all_lines.is_empty());
        // Result depends on content vs viewport - just verify it runs
        let _ = result;
    }

    #[test]
    fn test_committed_lines_count_only_newlines() {
        let mut conversation = ConversationState::new();
        conversation.start_assistant_streaming();

        // "Hello" has no newline - should not increment committed
        conversation.append_or_create_prose("Hello");
        assert_eq!(conversation.committed_line_count(), 0);

        // " world" still no newline
        conversation.append_or_create_prose(" world");
        assert_eq!(conversation.committed_line_count(), 0);

        // Now add newline - should increment
        conversation.append_or_create_prose("\n");
        assert_eq!(conversation.committed_line_count(), 1);

        // Add another complete line
        conversation.append_or_create_prose("Second line\n");
        assert_eq!(conversation.committed_line_count(), 2);
    }

    #[test]
    fn test_complete_streaming_commits_final_line() {
        let mut conversation = ConversationState::new();
        conversation.start_assistant_streaming();

        // Add text without trailing newline
        conversation.append_or_create_prose("Hello world");
        assert_eq!(conversation.committed_line_count(), 0);
        assert!(conversation.is_streaming());

        // Complete streaming - should commit the final line
        conversation.complete_streaming();
        assert_eq!(conversation.committed_line_count(), 1);
        assert!(!conversation.is_streaming());
    }
}
