//! Tests for terminal resize behavior.
//!
//! These tests verify that TUI content reflows correctly when the terminal
//! is resized. Uses snapshot testing to capture before/after states.

use super::fixtures::sessions;
use super::{Harness, TEST_HEIGHT, TEST_WIDTH};
use insta::assert_snapshot;

/// Test that content reflows correctly when terminal becomes wider.
#[test]
fn resize_wider_reflows_content() {
    let mut h = Harness::new(40, TEST_HEIGHT).with_session(sessions::basic_exchange());

    assert_snapshot!("narrow_before", h.render());

    h.resize(80, TEST_HEIGHT);
    assert_snapshot!("wide_after", h.render());
}

/// Test that content wraps correctly when terminal becomes narrower.
#[test]
fn resize_narrower_wraps_content() {
    let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

    assert_snapshot!("wide_before", h.render());

    h.resize(40, TEST_HEIGHT);
    assert_snapshot!("narrow_after", h.render());
}

/// Test that vertical resize handles content overflow.
#[test]
fn resize_shorter_handles_overflow() {
    let mut h = Harness::new(TEST_WIDTH, 24).with_session(sessions::multi_turn());

    assert_snapshot!("tall_before", h.render());

    h.resize(TEST_WIDTH, 12);
    assert_snapshot!("short_after", h.render());
}

/// Test that resize works with multiline messages containing code blocks.
#[test]
fn resize_with_code_blocks() {
    let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::multiline_messages());

    assert_snapshot!("code_blocks_wide", h.render());

    h.resize(50, TEST_HEIGHT);
    assert_snapshot!("code_blocks_narrow", h.render());
}

/// Test resize behavior with long conversation history.
#[test]
fn resize_long_conversation() {
    let mut h = Harness::new(TEST_WIDTH, 20).with_session(sessions::long_conversation());

    assert_snapshot!("long_conversation_initial", h.render());

    // Make terminal shorter
    h.resize(TEST_WIDTH, 10);
    assert_snapshot!("long_conversation_short", h.render());

    // Make terminal wider and taller
    h.resize(120, 30);
    assert_snapshot!("long_conversation_large", h.render());
}

/// Test resize with tool call displays.
#[test]
fn resize_with_tool_calls() {
    let mut h =
        Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::interleaved_prose_and_tools());

    assert_snapshot!("tool_calls_standard", h.render());

    h.resize(50, TEST_HEIGHT);
    assert_snapshot!("tool_calls_narrow", h.render());
}
