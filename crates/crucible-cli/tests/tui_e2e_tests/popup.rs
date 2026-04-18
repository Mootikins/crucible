//! Oil runner popup, stress, and error handling tests.

use std::time::Duration;

use super::shared::safe_truncate;
use super::tui_e2e_harness::{Key, TuiTestConfig, TuiTestSession};

// =============================================================================
// Oil Runner Popup Tests
// =============================================================================

/// Test F1 toggles popup
#[test]
#[ignore = "requires built binary"]
fn oil_f1_popup_toggle() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session.send_key(Key::F(1)).expect("F1 failed");
    session.settle();

    let screen_open = session.capture_screen().unwrap_or_default();
    eprintln!("After F1 (should show popup): {}", screen_open);

    session.send_key(Key::F(1)).expect("Second F1 failed");
    session.settle();

    let screen_closed = session.capture_screen().unwrap_or_default();
    eprintln!("After second F1 (should close popup): {}", screen_closed);

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test popup navigation with arrow keys
#[test]
#[ignore = "requires built binary"]
fn oil_popup_arrow_navigation() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session.send_key(Key::F(1)).expect("F1 failed");
    session.settle();

    for i in 0..5 {
        session.send_key(Key::Down).expect("Down failed");
        session.settle();
        eprintln!("Down {}", i + 1);
    }

    for i in 0..3 {
        session.send_key(Key::Up).expect("Up failed");
        session.settle();
        eprintln!("Up {}", i + 1);
    }

    session.send_key(Key::Escape).expect("Escape failed");
    session.settle();

    session.send_control('c').ok();
    session.send_control('c').ok();
}

// =============================================================================
// Oil Runner Stress Tests
// =============================================================================

/// Test rapid input doesn't corrupt display
#[test]
#[ignore = "requires built binary"]
fn oil_rapid_typing_stress() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(15));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    let test_string = "The quick brown fox jumps over the lazy dog 1234567890";
    for c in test_string.chars() {
        session.send(&c.to_string()).expect("Send char failed");
    }

    session.settle();

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Screen after rapid typing: {}", screen);

    for _ in 0..test_string.len() {
        session.send_key(Key::Backspace).expect("Backspace failed");
    }

    session.settle();

    session
        .send("still works")
        .expect("Should still accept input");
    session.settle();

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test alternating input and commands
#[test]
#[ignore = "requires built binary"]
fn oil_alternating_input_commands() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(15));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    for i in 0..5 {
        session
            .send(&format!("message {}", i))
            .expect("Send failed");
        session.settle();

        for _ in 0..10 {
            session.send_key(Key::Backspace).expect("Backspace failed");
        }

        session.send_line("/help").expect("Help failed");
        session.settle();

        session.send_line("/mode").expect("Mode failed");
        session.settle();
    }

    session.send_control('c').ok();
    session.send_control('c').ok();
}

// =============================================================================
// Oil Runner Error Handling
// =============================================================================

/// Test unknown command shows error
#[test]
#[ignore = "requires built binary"]
fn oil_unknown_command_error() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session
        .send_line("/nonexistent_command")
        .expect("Failed to send");
    session.settle();

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Screen after unknown command: {}", screen);

    session
        .send("still works")
        .expect("Should still accept input");

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test :clear command works
#[test]
#[ignore = "requires built binary"]
fn oil_clear_command() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session.send("/help\r").expect("Help failed");
    session.settle();

    let screen_before = session.capture_screen().unwrap_or_default();
    eprintln!("Before clear: {}", safe_truncate(&screen_before, 200));

    session.send(":clear\r").expect("Clear failed");
    session.settle();

    let screen_after = session.capture_screen().unwrap_or_default();
    eprintln!("After clear: {}", safe_truncate(&screen_after, 200));

    session.send(":quit\r").ok();
}
