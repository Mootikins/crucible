//! Property-based tests for viewport scrolling and buffer management
//!
//! Uses proptest to verify invariants that should hold for ANY valid input.

#[cfg(test)]
mod proptest_viewport {
    use crate::tui::viewport::{ContentKind, ViewportBlock, ViewportState};
    use proptest::prelude::*;

    // ==========================================================================
    // Height Calculation Properties
    // ==========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn height_is_always_positive(
            content in "[a-zA-Z0-9 ]{1,200}",
            width in 10u16..200
        ) {
            let mut block = ViewportBlock::new(1, ContentKind::UserMessage(content));
            let height = block.height(width);
            prop_assert!(height >= 1, "Height should be at least 1, got {}", height);
        }

        #[test]
        fn narrower_width_produces_taller_output(
            content in "[a-zA-Z0-9 ]{20,100}",
            narrow in 15u16..30,
            wide in 60u16..120
        ) {
            let mut block1 = ViewportBlock::new(1, ContentKind::UserMessage(content.clone()));
            let mut block2 = ViewportBlock::new(2, ContentKind::UserMessage(content));

            let narrow_height = block1.height(narrow);
            let wide_height = block2.height(wide);

            prop_assert!(
                narrow_height >= wide_height,
                "Narrow width {} should produce >= height than wide {}: {} vs {}",
                narrow, wide, narrow_height, wide_height
            );
        }

        #[test]
        fn cache_invalidation_allows_recalculation(
            content in "[a-zA-Z]{10,50}",
            width1 in 20u16..40,
            width2 in 60u16..100
        ) {
            let mut block = ViewportBlock::new(1, ContentKind::UserMessage(content));

            let h1 = block.height(width1);
            block.invalidate_height();
            let h2 = block.height(width2);

            // Different widths may produce different heights
            // but both should be valid (>= 1)
            prop_assert!(h1 >= 1 && h2 >= 1);
        }

        #[test]
        fn multiline_content_counts_all_lines(
            lines in prop::collection::vec("[a-zA-Z]{5,20}", 1..10)
        ) {
            let content = lines.join("\n");
            let line_count = lines.len();

            let mut block = ViewportBlock::new(1, ContentKind::UserMessage(content));
            let height = block.height(200); // Wide enough to not wrap

            prop_assert!(
                height >= line_count as u16,
                "Height {} should be >= line count {} for content with newlines",
                height, line_count
            );
        }
    }

    // ==========================================================================
    // Buffer Management Properties
    // ==========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn push_increments_count(
            messages in prop::collection::vec("[a-zA-Z]{1,20}", 1..20)
        ) {
            let mut viewport = ViewportState::new(80, 24);

            for (i, msg) in messages.iter().enumerate() {
                viewport.push_user_message(msg);
                prop_assert_eq!(
                    viewport.content_count(),
                    i + 1,
                    "Count should match number of pushed messages"
                );
            }
        }

        #[test]
        fn messages_maintain_insertion_order(
            messages in prop::collection::vec("[a-zA-Z0-9]{3,10}", 2..10)
        ) {
            let mut viewport = ViewportState::new(80, 24);

            for msg in &messages {
                viewport.push_user_message(msg);
            }

            let retrieved: Vec<&str> = viewport
                .content_blocks()
                .map(|b| b.content_text())
                .collect();

            prop_assert_eq!(
                retrieved.len(),
                messages.len(),
                "Should retrieve same number of messages"
            );

            for (i, (expected, actual)) in messages.iter().zip(retrieved.iter()).enumerate() {
                prop_assert_eq!(
                    expected.as_str(),
                    *actual,
                    "Message {} should match", i
                );
            }
        }

        #[test]
        fn all_blocks_have_unique_ids(
            count in 2usize..50
        ) {
            let mut viewport = ViewportState::new(80, 24);

            for i in 0..count {
                viewport.push_user_message(format!("msg{}", i));
            }

            let ids: Vec<u64> = viewport.content_blocks().map(|b| b.id).collect();
            let unique: std::collections::HashSet<_> = ids.iter().collect();

            prop_assert_eq!(
                ids.len(),
                unique.len(),
                "All block IDs should be unique"
            );
        }

        #[test]
        fn content_zone_height_is_terminal_minus_chrome(
            height in 10u16..100
        ) {
            let viewport = ViewportState::new(80, height);
            let zone = viewport.content_zone_height();

            // Content zone = terminal height - input area - status bar
            // INPUT_HEIGHT = 3, STATUS_HEIGHT = 1
            let expected = height.saturating_sub(4);

            prop_assert_eq!(
                zone, expected,
                "Content zone should be terminal height minus chrome"
            );
        }
    }

    // ==========================================================================
    // Overflow Properties
    // ==========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(30))]

        #[test]
        fn overflow_preserves_message_order(
            messages in prop::collection::vec("[a-zA-Z]{5,15}", 5..15)
        ) {
            // Use small terminal to force overflow
            let mut viewport = ViewportState::new(80, 8); // zone = 8 - 4 = 4 lines

            for msg in &messages {
                viewport.push_user_message(msg);
            }

            let overflow = viewport.maybe_overflow_to_scrollback();

            // If there's overflow, first overflowed should be oldest
            if !overflow.is_empty() {
                let first_overflow_content = overflow[0].content_text();
                prop_assert_eq!(
                    first_overflow_content,
                    messages[0].as_str(),
                    "First overflow should be first message"
                );
            }
        }

        #[test]
        fn total_messages_equals_buffer_plus_overflow(
            messages in prop::collection::vec("[a-z]{5,10}", 5..20)
        ) {
            let mut viewport = ViewportState::new(80, 10); // zone = 6 lines

            for msg in &messages {
                viewport.push_user_message(msg);
            }

            let overflow = viewport.maybe_overflow_to_scrollback();
            let remaining = viewport.content_count();

            prop_assert_eq!(
                overflow.len() + remaining,
                messages.len(),
                "Overflow + remaining should equal total messages"
            );
        }

        #[test]
        fn no_overflow_when_content_fits(
            messages in prop::collection::vec("[a-z]{3,8}", 1..4)
        ) {
            // Large terminal, few messages
            let mut viewport = ViewportState::new(80, 50); // zone = 46 lines

            for msg in &messages {
                viewport.push_user_message(msg);
            }

            let overflow = viewport.maybe_overflow_to_scrollback();

            prop_assert!(
                overflow.is_empty(),
                "Should not overflow with {} messages in zone of 46 lines",
                messages.len()
            );
        }
    }

    // ==========================================================================
    // Format Properties
    // ==========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn user_message_has_you_prefix(content in "[a-zA-Z]{1,30}") {
            let block = ViewportBlock::new(1, ContentKind::UserMessage(content.clone()));
            let formatted = block.format_for_viewport();

            prop_assert!(
                formatted.starts_with("You: "),
                "User message should start with 'You: ', got: {}", formatted
            );
            prop_assert!(
                formatted.contains(&content),
                "Formatted should contain original content"
            );
        }

        #[test]
        fn assistant_message_has_assistant_prefix(content in "[a-zA-Z]{1,30}") {
            let block = ViewportBlock::new(
                1,
                ContentKind::AssistantMessage { content: content.clone(), complete: true }
            );
            let formatted = block.format_for_viewport();

            prop_assert!(
                formatted.starts_with("Assistant: "),
                "Assistant message should start with 'Assistant: '"
            );
            prop_assert!(
                formatted.contains(&content),
                "Formatted should contain original content"
            );
        }

        #[test]
        fn system_message_has_asterisk_prefix(content in "[a-zA-Z]{1,30}") {
            let block = ViewportBlock::new(1, ContentKind::System(content.clone()));
            let formatted = block.format_for_viewport();

            prop_assert!(
                formatted.starts_with("* "),
                "System message should start with '* '"
            );
            prop_assert!(
                formatted.contains(&content),
                "Formatted should contain original content"
            );
        }
    }
}
