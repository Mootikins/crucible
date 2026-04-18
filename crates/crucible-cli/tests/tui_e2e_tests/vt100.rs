//! vt100 exemplar tests demonstrating the recommended screen-assertion pattern.

use std::time::Duration;

use super::tui_e2e_harness::{
    assert_screen_contains, assert_screen_not_contains, Key, TuiTestConfig, TuiTestSession,
};

// =============================================================================
// vt100 Exemplar Tests
// =============================================================================

/// Exemplar: verify screen content with `wait_for_text` and `assert_screen_contains`.
///
/// Demonstrates the basic vt100 assertion flow:
/// 1. `wait_for_text()` polls until content appears (or times out)
/// 2. `assert_screen_contains()` checks the parsed screen (no ANSI noise)
#[test]
#[ignore = "requires built binary"]
fn vt100_exemplar_screen_content_verification() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .wait_for_text("NORMAL", Duration::from_secs(5))
        .expect("TUI should render mode indicator on startup");

    assert_screen_contains(session.screen(), "NORMAL");

    session.send(":quit\r").ok();
}

/// Exemplar: popup lifecycle — open, verify content, close, verify gone.
///
/// Demonstrates asserting popup presence and absence using the vt100 screen:
/// 1. Open popup with a command
/// 2. `wait_for_text()` confirms popup appeared
/// 3. Dismiss popup
/// 4. `assert_screen_not_contains()` confirms popup dismissed
#[test]
#[ignore = "requires built binary"]
fn vt100_exemplar_popup_lifecycle() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .wait_for_text("NORMAL", Duration::from_secs(5))
        .expect("TUI ready");

    // F1 opens the command palette popup
    session.send_key(Key::F(1)).expect("Failed to send F1");
    session
        .wait_until(
            |s| {
                let c = s.contents();
                c.contains("Commands") || c.contains("/mode") || c.contains("palette")
            },
            Duration::from_secs(3),
        )
        .expect("Command palette popup should appear");

    session.send_key(Key::Escape).expect("Escape failed");
    session.settle();
    session.refresh_screen();

    // After dismissing, the help popup title should be gone.
    // Note: "Commands" may still appear in the input area, so this checks
    // the popup-specific content is no longer rendered.
    assert_screen_not_contains(session.screen(), "Commands");

    session.send(":quit\r").ok();
}

/// Exemplar: mode switching verified through the parsed screen.
///
/// Demonstrates using `wait_for_text()` to track mode indicator changes
/// across multiple mode switches, replacing fragile raw string matching.
#[test]
#[ignore = "requires built binary"]
fn vt100_exemplar_mode_indicator() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .wait_for_text("NORMAL", Duration::from_secs(5))
        .expect("Should start in NORMAL mode");

    session.send("/auto\r").expect("Failed to send /auto");
    session
        .wait_for_text("AUTO", Duration::from_secs(3))
        .expect("Should switch to AUTO mode");

    assert_screen_contains(session.screen(), "AUTO");

    session.send("/plan\r").expect("Failed to send /plan");
    session
        .wait_for_text("PLAN", Duration::from_secs(3))
        .expect("Should switch back to PLAN mode");

    assert_screen_contains(session.screen(), "PLAN");

    session.send(":quit\r").ok();
}

/// Exemplar: terminal size adaptation — same interaction at different widths.
///
/// Demonstrates using `wait_until()` with a custom predicate to verify content
/// renders correctly at a non-default terminal size.
#[test]
#[ignore = "requires built binary"]
fn vt100_exemplar_terminal_size_adaptation() {
    for (cols, rows, label) in [(60, 24, "narrow"), (120, 40, "wide")] {
        let config = TuiTestConfig::new("chat")
            .with_env("RUST_LOG", "warn")
            .with_dimensions(cols, rows)
            .with_timeout(Duration::from_secs(10));

        let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

        session
            .wait_for_text("NORMAL", Duration::from_secs(5))
            .unwrap_or_else(|e| {
                panic!(
                    "Mode indicator should render at {} ({cols}x{rows}): {e}",
                    label
                )
            });

        assert_screen_contains(session.screen(), "NORMAL");

        session.send(":quit\r").ok();
    }
}
