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
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_multi_lang_code());
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
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_multi_lang_code());
        let output = h.render();

        assert!(output.contains("print"), "Should show Python print");
        assert!(output.contains("println!"), "Should show Rust println!");
    }

    #[test]
    fn inline_code_visible() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_inline_code());
        let output = h.render();

        assert!(output.contains("println!"), "Should show println! from inline code");
    }
}
