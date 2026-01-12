//! Tests for code block rendering with syntax highlighting.
//!
//! These tests capture and verify how code blocks are displayed in the TUI,
//! including various languages, untagged blocks, and multi-block scenarios.

use super::fixtures::sessions;
use super::{Harness, TEST_HEIGHT, TEST_WIDTH};
use insta::assert_snapshot;

// =============================================================================
// Snapshot Tests - Code Block Rendering via Harness
// =============================================================================

mod snapshots {
    use super::*;

    #[test]
    fn rust_code_block_renders() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_rust_code());
        assert_snapshot!("code_block_rust", h.render());
    }

    #[test]
    fn multiple_code_blocks_render() {
        let h =
            Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_multi_lang_code());
        assert_snapshot!("code_blocks_multi_lang", h.render());
    }

    #[test]
    fn long_code_block_renders() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_long_code());
        assert_snapshot!("code_block_long", h.render());
    }

    #[test]
    fn untagged_code_block_renders() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_untagged_code());
        assert_snapshot!("code_block_untagged", h.render());
    }

    #[test]
    fn inline_code_renders() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_inline_code());
        assert_snapshot!("code_inline", h.render());
    }

    #[test]
    fn multiline_messages_with_code() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::multiline_messages());
        assert_snapshot!("code_block_multiline", h.render());
    }
}

// =============================================================================
// Content Verification Tests
// =============================================================================

mod padding_tests {
    use super::*;
    use crate::tui::content_block::StreamBlock;
    use crate::tui::testing::fixtures::sessions::{assistant_blocks, user};

    /// Test that code blocks have blank lines before and after for visual separation
    #[test]
    fn code_block_has_padding_lines() {
        // Create a session with prose before and after a code block
        let session = vec![
            user("Show me code"),
            assistant_blocks(vec![
                StreamBlock::prose("Here's the code:"),
                StreamBlock::code(Some("rust".to_string()), "fn main() {}"),
                StreamBlock::prose("That's it!"),
            ]),
        ];

        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(session);
        let output = h.render();

        // The code block should be separated from prose by blank lines
        // Check that there's a blank line between "Here's the code:" and "fn main()"
        let lines: Vec<&str> = output.lines().collect();

        // Find the line with "Here's the code:"
        let prose_before_idx = lines
            .iter()
            .position(|l| l.contains("Here's the code:"))
            .expect("Should find 'Here's the code:' line");

        // Find the line with "fn main()"
        let code_idx = lines
            .iter()
            .position(|l| l.contains("fn main()"))
            .expect("Should find 'fn main()' line");

        // There should be at least one blank line between them
        assert!(
            code_idx > prose_before_idx + 1,
            "Code block should have blank line before it. Prose at {}, code at {}",
            prose_before_idx,
            code_idx
        );

        // Find the line with "That's it!"
        let prose_after_idx = lines
            .iter()
            .position(|l| l.contains("That's it!"))
            .expect("Should find 'That's it!' line");

        // There should be at least one blank line between code and following prose
        assert!(
            prose_after_idx > code_idx + 1,
            "Code block should have blank line after it. Code at {}, prose at {}",
            code_idx,
            prose_after_idx
        );
    }

    /// Test that code blocks with a language tag show the language label
    #[test]
    fn code_block_shows_language_label() {
        let session = vec![
            user("Show me rust"),
            assistant_blocks(vec![StreamBlock::code(
                Some("rust".to_string()),
                "fn main() {}",
            )]),
        ];

        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(session);
        let output = h.render();

        // The language label "rust" should appear before the code
        assert!(
            output.contains("rust"),
            "Should show 'rust' language label. Output:\n{}",
            output
        );

        // Find positions to verify order
        let lines: Vec<&str> = output.lines().collect();
        let label_idx = lines
            .iter()
            .position(|l| l.contains("rust") && !l.contains("fn"));
        let code_idx = lines.iter().position(|l| l.contains("fn main()"));

        assert!(
            label_idx.is_some() && code_idx.is_some(),
            "Should find both label and code"
        );
        assert!(
            label_idx.unwrap() < code_idx.unwrap(),
            "Language label should appear before code"
        );
    }
}

mod content_tests {
    use super::*;

    #[test]
    fn rust_code_block_content() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_rust_code());
        let output = h.render();

        assert!(output.contains("fn main()"), "Should show fn main()");
        assert!(output.contains("println!"), "Should show println!");
    }

    #[test]
    fn multi_lang_blocks_all_visible() {
        let h =
            Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_multi_lang_code());
        let output = h.render();

        assert!(output.contains("print"), "Should show Python print");
        assert!(output.contains("println!"), "Should show Rust println!");
    }

    #[test]
    fn inline_code_visible() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_inline_code());
        let output = h.render();

        assert!(
            output.contains("println!"),
            "Should show println! from inline code"
        );
    }
}
