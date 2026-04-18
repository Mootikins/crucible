//! Oil runner terminal size adaptation tests.

use std::time::Duration;

use super::tui_e2e_harness::{Key, TuiTestConfig, TuiTestSession};

// =============================================================================
// Oil Runner Terminal Size Tests
// =============================================================================

/// Test oil runner at narrow terminal width (60 cols)
/// NOTE: with_dimensions only affects the vt100 parser, not the actual PTY size.
/// The TUI still runs at 80x24. This test verifies the mode indicator renders.
#[test]
#[ignore = "requires built binary"]
fn oil_narrow_terminal_60_cols() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_dimensions(60, 24)
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .wait_for_text("NORMAL", Duration::from_secs(3))
        .expect("Should show mode indicator at 60 cols");

    session.send(":quit\r").ok();
}

/// Test oil runner at very narrow terminal width (40 cols)
#[test]
#[ignore = "requires built binary"]
fn ink_very_narrow_terminal_40_cols() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_dimensions(40, 24)
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session.send("test input\r").expect("Input failed");
    session.settle();

    session.send(":quit\r").ok();
}

/// Test oil runner at wide terminal width (120 cols)
#[test]
#[ignore = "requires built binary"]
fn oil_wide_terminal_120_cols() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_dimensions(120, 40)
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .wait_for_text("NORMAL", Duration::from_secs(3))
        .expect("Should show mode indicator at 120 cols");

    session.send_key(Key::F(1)).expect("F1 failed");
    session
        .wait_until(
            |s| {
                let c = s.contents();
                c.contains("semantic_search") || c.contains("tool")
            },
            Duration::from_secs(3),
        )
        .expect("Popup should render at wide width");

    session.send(":quit\r").ok();
}

/// Test oil runner at short terminal height (10 rows)
#[test]
#[ignore = "requires built binary"]
fn oil_short_terminal_10_rows() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_dimensions(80, 10)
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session.send("test\r").expect("Input failed");
    session.settle();

    session.send(":quit\r").ok();
}
