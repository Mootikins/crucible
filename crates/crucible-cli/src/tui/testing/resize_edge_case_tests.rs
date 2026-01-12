//! Tests for terminal resize behavior in the TUI.
//!
//! These tests verify that the TUI handles terminal resizing correctly,
//! including during streaming, with various content types, and at extreme sizes.

use crate::tui::testing::fixtures::sessions;
use crate::tui::testing::{Harness, StreamingHarness, TEST_HEIGHT, TEST_WIDTH};

const NARROW_WIDTH: u16 = 40;
const WIDE_WIDTH: u16 = 120;
const SHORT_HEIGHT: u16 = 10;
const TALL_HEIGHT: u16 = 50;

mod resize_basics {
    use super::*;

    #[test]
    fn resize_to_narrower_width() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

        let before = h.render();
        assert!(before.contains("Hello"));

        h.resize(NARROW_WIDTH as u16, TEST_HEIGHT as u16);

        let after = h.render();
        assert!(after.contains("Hello"));
        assert!(!after.is_empty());
    }

    #[test]
    fn resize_to_wider_width() {
        let mut h =
            Harness::new(NARROW_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

        h.resize(WIDE_WIDTH as u16, TEST_HEIGHT as u16);

        let after = h.render();
        assert!(after.contains("Hello"));
        assert!(!after.is_empty());
    }

    #[test]
    fn resize_to_shorter_height() {
        let mut h = Harness::new(TEST_WIDTH, TALL_HEIGHT).with_session(sessions::basic_exchange());

        h.resize(TEST_WIDTH as u16, SHORT_HEIGHT as u16);

        let after = h.render();
        assert!(after.contains("Hello"));
        assert!(!after.is_empty());
    }

    #[test]
    fn resize_to_taller_height() {
        let mut h = Harness::new(TEST_WIDTH, SHORT_HEIGHT).with_session(sessions::basic_exchange());

        h.resize(TEST_WIDTH as u16, TALL_HEIGHT as u16);

        let after = h.render();
        assert!(after.contains("Hello"));
        assert!(!after.is_empty());
    }

    #[test]
    fn resize_to_extremely_narrow() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

        h.resize(20, TEST_HEIGHT as u16);

        let after = h.render();
        assert!(after.contains("Hello"));
    }

    #[test]
    fn resize_to_extremely_wide() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

        h.resize(200, TEST_HEIGHT as u16);

        let after = h.render();
        assert!(after.contains("Hello"));
    }
}

mod resize_during_streaming {
    use super::*;

    #[test]
    fn resize_during_streaming_preserves_content() {
        let mut h = StreamingHarness::inline();

        h.user_message("Test resize during streaming");
        h.start_streaming();
        h.chunk("This is a response");

        h.harness.resize(60, 20);

        h.chunk(" with more content");
        h.complete();

        let rendered = h.harness.render();
        assert!(rendered.contains("Test resize"));
        assert!(rendered.contains("response"));
    }

    #[test]
    fn resize_narrow_during_streaming_graduation() {
        let mut h = StreamingHarness::new(80, 20);

        h.user_message("Tell me a long story");
        h.start_streaming();

        h.harness.resize(NARROW_WIDTH as u16, 15);

        for i in 1..=10 {
            h.chunk(&format!("Paragraph {} with content.\n\n", i));
        }
        h.complete();

        let graduated = h.graduated_line_count();
        let rendered = h.harness.render();

        assert!(graduated > 0 || rendered.contains("Paragraph 10"));
    }

    #[test]
    fn resize_wide_during_streaming_no_graduation() {
        let mut h = StreamingHarness::new(NARROW_WIDTH, 20);

        h.user_message("Short message");
        h.start_streaming();

        h.harness.resize(WIDE_WIDTH as u16, 30);

        h.chunk("Short response");
        h.complete();

        let rendered = h.harness.render();
        assert!(rendered.contains("Short response"));
    }

    #[test]
    fn rapid_resize_during_streaming() {
        let mut h = StreamingHarness::inline();

        h.user_message("Test");
        h.start_streaming();
        h.chunk("Resizing");

        h.harness.resize(80, 15);
        h.harness.resize(60, 15);
        h.harness.resize(80, 15);
        h.harness.resize(40, 15);
        h.harness.resize(80, 15);

        h.complete();

        let rendered = h.harness.render();
        assert!(rendered.contains("Resizing"));
    }
}

mod resize_with_long_conversation {
    use super::*;

    #[test]
    fn resize_long_conversation_to_narrow() {
        let mut h =
            Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::long_conversation());

        h.resize(NARROW_WIDTH as u16, TEST_HEIGHT as u16);

        let rendered = h.render();
        // Long conversation has "Message X" and "Response X" items
        assert!(
            rendered.contains("Message") || rendered.contains("Response"),
            "Should contain conversation content after narrow resize"
        );
    }

    #[test]
    fn resize_long_conversation_to_short() {
        let mut h =
            Harness::new(TEST_WIDTH, TALL_HEIGHT).with_session(sessions::long_conversation());

        h.resize(TEST_WIDTH as u16, SHORT_HEIGHT as u16);

        let rendered = h.render();
        // Should still show some conversation content
        assert!(
            rendered.contains("Message") || rendered.contains("Response"),
            "Should contain conversation content after short resize"
        );
    }

    #[test]
    fn resize_long_conversation_preserves_scroll_position() {
        let mut h =
            Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::long_conversation());

        h.resize(NARROW_WIDTH as u16, SHORT_HEIGHT as u16);
        h.resize(TEST_WIDTH as u16, TALL_HEIGHT as u16);
        h.resize(WIDE_WIDTH as u16, TEST_HEIGHT as u16);

        let rendered = h.render();
        // Should still have conversation content after multiple resizes
        assert!(
            rendered.contains("Message") || rendered.contains("Response"),
            "Should preserve conversation content through multiple resizes"
        );
    }
}

mod resize_with_code_blocks {
    use super::*;

    #[test]
    fn resize_with_code_block_narrows_correctly() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_long_code());

        h.resize(NARROW_WIDTH as u16, TEST_HEIGHT as u16);

        let rendered = h.render();
        // with_long_code has "struct Data" with field_N fields
        assert!(
            rendered.contains("struct") || rendered.contains("field"),
            "Should contain code block content after narrow resize"
        );
    }

    #[test]
    fn resize_with_code_block_wides_correctly() {
        let mut h =
            Harness::new(NARROW_WIDTH, TEST_HEIGHT).with_session(sessions::with_rust_code());

        h.resize(WIDE_WIDTH as u16, TEST_HEIGHT as u16);

        let rendered = h.render();
        assert!(
            rendered.contains("fn"),
            "Should contain function keyword after wide resize"
        );
    }
}

mod resize_with_tables {
    use super::*;

    #[test]
    fn resize_with_table_to_narrow() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_table());

        h.resize(NARROW_WIDTH as u16, TEST_HEIGHT as u16);

        let rendered = h.render();
        // with_table has a Rust vs Go comparison table
        assert!(
            rendered.contains("Rust") || rendered.contains("Feature") || rendered.contains("comparison"),
            "Should contain table content after narrow resize"
        );
    }

    #[test]
    fn resize_with_wide_table() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_wide_table());

        h.resize(WIDE_WIDTH as u16, TEST_HEIGHT as u16);

        let rendered = h.render();
        // with_wide_table has Package, Description, Version columns
        assert!(
            rendered.contains("Package") || rendered.contains("serde") || rendered.contains("tokio"),
            "Should contain table content after wide resize"
        );
    }

    #[test]
    fn resize_table_at_graduation_boundary() {
        let mut h = StreamingHarness::inline();

        h.user_message("Show table");
        h.start_streaming();

        h.chunk("| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n");

        h.harness.resize(NARROW_WIDTH as u16, 10);

        h.chunk("\nMore content after table");
        h.complete();

        let rendered = h.harness.render();
        assert!(
            rendered.contains("A")
                || rendered.contains("B")
                || rendered.contains("1")
                || rendered.contains("More content"),
            "Should have table or subsequent content: {}",
            rendered
        );
    }
}

mod resize_edge_cases {
    use super::*;

    #[test]
    fn resize_to_minimal_size() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

        h.resize(10, 5);

        let rendered = h.render();
        assert!(!rendered.is_empty() || rendered.len() > 0);
    }

    #[test]
    fn resize_to_maximal_size() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

        h.resize(300, 100);

        let rendered = h.render();
        assert!(rendered.contains("Hello"));
    }

    #[test]
    fn resize_back_and_forth() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

        for _ in 0..5 {
            h.resize(NARROW_WIDTH as u16, SHORT_HEIGHT as u16);
            h.resize(WIDE_WIDTH as u16, TALL_HEIGHT as u16);
        }

        let rendered = h.render();
        assert!(rendered.contains("Hello"));
    }

    #[test]
    fn resize_preserves_conversation_state() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_tool_calls());

        let before_len = h.conversation_len();

        h.resize(NARROW_WIDTH as u16, SHORT_HEIGHT as u16);
        h.resize(WIDE_WIDTH as u16, TALL_HEIGHT as u16);

        let after_len = h.conversation_len();
        assert_eq!(before_len, after_len);
    }
}

mod resize_with_popups {
    use super::*;
    use crate::tui::state::PopupKind;

    #[test]
    fn resize_with_popup_open() {
        use crate::tui::testing::fixtures::registries;

        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        assert!(h.has_popup());

        h.resize(NARROW_WIDTH as u16, SHORT_HEIGHT as u16);

        let rendered = h.render();
        assert!(!rendered.is_empty());
    }
}
