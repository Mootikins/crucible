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
/// Optimized to:
/// 1. First check if graduation is needed using cached line counts
/// 2. Only render lines if graduation is actually needed
pub fn check_graduation(
    conversation: &ConversationState,
    graduated_count: usize,
    viewport_capacity: usize,
    content_width: usize,
) -> (Vec<Line<'static>>, Option<GraduationResult>) {
    // First, efficiently calculate total line count using cache
    let total_lines = conversation.calculate_total_line_count(content_width);

    // Check if graduation is needed (cheap calculation)
    let result = calculate_graduation(total_lines, graduated_count, viewport_capacity);

    if result.is_none() {
        // No graduation needed - return empty vec to avoid rendering
        return (Vec::new(), None);
    }

    // Graduation needed - render using cache where available
    let all_lines = conversation.render_all_lines_cached(content_width);

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
}
