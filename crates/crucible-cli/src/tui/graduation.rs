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
/// When `is_streaming` is true, reserves a buffer of lines at the bottom
/// to prevent volatile streaming content from being graduated. This prevents
/// the "disappearing lines" artifact during rapid content updates.
pub fn check_graduation(
    conversation: &ConversationState,
    graduated_count: usize,
    viewport_capacity: usize,
    content_width: usize,
    is_streaming: bool,
) -> (Vec<Line<'static>>, Option<GraduationResult>) {
    // Render all content for viewport display
    let all_lines = conversation.render_for_graduation(content_width);
    let total_rendered = all_lines.len();

    // During streaming, reserve some lines to handle content volatility.
    // The last few lines may change as partial content is parsed/rendered.
    // Reserve ~20% of viewport or minimum 3 lines as a buffer.
    let graduatable_lines = if is_streaming {
        let buffer = (viewport_capacity / 5).max(3);
        total_rendered.saturating_sub(buffer)
    } else {
        total_rendered
    };

    // Check if graduation is needed
    let result = calculate_graduation(graduatable_lines, graduated_count, viewport_capacity);

    (all_lines, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Property-based tests for graduation invariants
    // =========================================================================

    mod proptest_graduation {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Property: Graduated count should never exceed total lines
            #[test]
            fn graduation_never_exceeds_total(
                total_lines in 1..1000usize,
                graduated in 0..500usize,
                viewport in 5..50usize
            ) {
                if let Some(result) = calculate_graduation(total_lines, graduated, viewport) {
                    prop_assert!(
                        result.new_graduated_count <= total_lines,
                        "new_graduated_count ({}) exceeded total_lines ({})",
                        result.new_graduated_count, total_lines
                    );
                }
            }

            /// Property: Graduation range should never start before current graduated count
            #[test]
            fn graduation_range_starts_at_graduated(
                total_lines in 1..1000usize,
                graduated in 0..500usize,
                viewport in 5..50usize
            ) {
                if let Some(result) = calculate_graduation(total_lines, graduated, viewport) {
                    prop_assert!(
                        result.lines_to_graduate.start >= graduated,
                        "Range start ({}) < graduated count ({})",
                        result.lines_to_graduate.start, graduated
                    );
                }
            }

            /// Property: After graduation, visible lines should fit in viewport
            #[test]
            fn graduation_makes_visible_fit(
                total_lines in 1..1000usize,
                graduated in 0..500usize,
                viewport in 5..50usize
            ) {
                if let Some(result) = calculate_graduation(total_lines, graduated, viewport) {
                    let new_visible = total_lines.saturating_sub(result.new_graduated_count);
                    prop_assert!(
                        new_visible <= viewport,
                        "After graduation, visible ({}) > viewport ({})",
                        new_visible, viewport
                    );
                }
            }

            /// Property: Graduation should be monotonic (count never decreases)
            #[test]
            fn graduation_is_monotonic(
                total_lines in 100..1000usize,
                viewport in 10..50usize
            ) {
                let mut graduated = 0;
                let mut iterations = 0;
                const MAX_ITERATIONS: usize = 100;

                while iterations < MAX_ITERATIONS {
                    if let Some(result) = calculate_graduation(total_lines, graduated, viewport) {
                        prop_assert!(
                            result.new_graduated_count >= graduated,
                            "Monotonicity violated: new ({}) < old ({})",
                            result.new_graduated_count, graduated
                        );
                        graduated = result.new_graduated_count;
                    } else {
                        break;
                    }
                    iterations += 1;
                }
            }

            /// Property: Graduation should be idempotent (same inputs = same outputs)
            #[test]
            fn graduation_is_idempotent(
                total_lines in 1..1000usize,
                graduated in 0..500usize,
                viewport in 5..50usize
            ) {
                let result1 = calculate_graduation(total_lines, graduated, viewport);
                let result2 = calculate_graduation(total_lines, graduated, viewport);
                prop_assert_eq!(result1, result2, "Idempotency violated");
            }

            /// Property: Range end should equal new_graduated_count
            #[test]
            fn graduation_range_end_equals_count(
                total_lines in 1..1000usize,
                graduated in 0..500usize,
                viewport in 5..50usize
            ) {
                if let Some(result) = calculate_graduation(total_lines, graduated, viewport) {
                    prop_assert_eq!(
                        result.lines_to_graduate.end,
                        result.new_graduated_count,
                        "Range end ({}) != new_graduated_count ({})",
                        result.lines_to_graduate.end, result.new_graduated_count
                    );
                }
            }

            /// Property: No graduation needed when visible lines fit
            #[test]
            fn no_graduation_when_fits(
                total_lines in 0..100usize,
                viewport in 100..200usize
            ) {
                // When viewport >= total, no graduation should happen
                let result = calculate_graduation(total_lines, 0, viewport);
                prop_assert!(
                    result.is_none(),
                    "Graduation should be None when total ({}) <= viewport ({})",
                    total_lines, viewport
                );
            }

            /// Property: Streaming buffer should never graduate more than non-streaming
            #[test]
            fn streaming_buffer_reduces_graduation(
                total_lines in 50..500usize,
                viewport in 10..30usize
            ) {
                // Simulate streaming vs non-streaming graduation
                let non_streaming_result = calculate_graduation(total_lines, 0, viewport);

                // Streaming reserves ~20% buffer
                let buffer = (viewport / 5).max(3);
                let graduatable = total_lines.saturating_sub(buffer);
                let streaming_result = calculate_graduation(graduatable, 0, viewport);

                // Streaming should graduate same or fewer lines
                match (non_streaming_result, streaming_result) {
                    (Some(ns), Some(s)) => {
                        prop_assert!(
                            s.new_graduated_count <= ns.new_graduated_count,
                            "Streaming graduated more ({}) than non-streaming ({})",
                            s.new_graduated_count, ns.new_graduated_count
                        );
                    }
                    (Some(_), None) => {
                        // OK: streaming decided not to graduate
                    }
                    (None, Some(s)) => {
                        prop_assert!(
                            false,
                            "Streaming graduated ({}) when non-streaming didn't",
                            s.new_graduated_count
                        );
                    }
                    (None, None) => {
                        // OK: neither graduated
                    }
                }
            }
        }
    }

    // =========================================================================
    // Unit tests
    // =========================================================================

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
