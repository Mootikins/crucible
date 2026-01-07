//! Tests for inline viewport mode
//!
//! Verifies that inline viewport mode renders content from the top,
//! with the input prompt sliding down as messages are added.

use super::fixtures::sessions;
use super::Harness;
use crucible_config::ViewportMode;
use insta::assert_snapshot;

/// Inline mode: empty conversation shows input at top
#[test]
fn inline_empty_input_at_top() {
    let h = Harness::new(60, 15).with_viewport_mode(ViewportMode::Inline);
    assert_snapshot!(h.render());
}

/// Inline mode: basic exchange pushes input down
#[test]
fn inline_basic_exchange_input_slides_down() {
    let h = Harness::new(60, 15)
        .with_viewport_mode(ViewportMode::Inline)
        .with_session(sessions::basic_exchange());
    assert_snapshot!(h.render());
}

/// Inline mode: multiple exchanges approach bottom
#[test]
fn inline_multi_turn_fills_viewport() {
    let h = Harness::new(60, 20)
        .with_viewport_mode(ViewportMode::Inline)
        .with_session(sessions::multi_turn());
    assert_snapshot!(h.render());
}

/// Inline mode: multiline messages affect height calculation
#[test]
fn inline_multiline_messages() {
    let h = Harness::new(60, 20)
        .with_viewport_mode(ViewportMode::Inline)
        .with_session(sessions::multiline_messages());
    assert_snapshot!(h.render());
}

/// Fullscreen mode (default): input stays at bottom regardless of content
#[test]
fn fullscreen_empty_input_at_bottom() {
    let h = Harness::new(60, 15).with_viewport_mode(ViewportMode::Fullscreen);
    assert_snapshot!(h.render());
}

/// Fullscreen mode: content fills from top, input stays at bottom
#[test]
fn fullscreen_with_content_input_at_bottom() {
    let h = Harness::new(60, 15)
        .with_viewport_mode(ViewportMode::Fullscreen)
        .with_session(sessions::basic_exchange());
    assert_snapshot!(h.render());
}
