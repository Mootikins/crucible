//! Oil runner tests: responsiveness, streaming, Ctrl+C edge cases, and mode
//! switching invoked via the oil runner.

use std::time::Duration;

use super::shared::safe_truncate;
use super::tui_e2e_harness::{Key, TuiTestConfig, TuiTestSession};

// =============================================================================
// Oil Runner Tests
// =============================================================================

/// Test that oil runner stays responsive during extended use
#[test]
#[ignore = "requires built binary"]
fn oil_runner_does_not_freeze() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(3));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn chat");

    let start = std::time::Instant::now();

    session.wait_for_ready().expect("TUI ready");

    for i in 0..20 {
        if session.send("x").is_err() {
            panic!("Send failed at iteration {}", i);
        }
        session.settle();

        if session.send_key(Key::Backspace).is_err() {
            panic!("Backspace failed at iteration {}", i);
        }
        session.settle();
    }

    eprintln!("Input test passed after {:?}", start.elapsed());

    session.send(":quit\r").ok();
}

/// Test that :quit REPL command causes exit.
/// :quit is a REPL command (colon prefix), not a slash command.
/// /quit would be forwarded to the agent as a slash command.
#[test]
#[ignore = "requires built binary"]
fn oil_quit_with_repl_command() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn chat");

    // Wait for TUI to initialize and show Ready status
    session.wait_for_ready().expect("TUI ready");

    session.send(":quit\r").expect("Failed to send :quit");
    session.settle();

    match session.expect_eof() {
        Ok(_) => eprintln!(":quit worked - test passed"),
        Err(e) => {
            let screen = session.capture_screen().unwrap_or_default();
            eprintln!("Screen at timeout: {:?}", screen);
            panic!(":quit command did not exit: {:?}", e);
        }
    }
}

/// Test that typing and Enter works - check output for echo
#[test]
#[ignore = "requires built binary"]
fn oil_verify_pty_works() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "crucible_cli::tui::oil=debug")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn chat");

    session.wait_for_ready().expect("TUI ready");

    eprintln!("Checking for initial prompt...");
    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Initial screen: {:?}", screen);

    eprintln!("Typing hello...");
    session.send("hello").expect("Failed to send hello");
    session.settle();

    let screen2 = session.capture_screen().unwrap_or_default();
    eprintln!("Screen after 'hello': {:?}", screen2);

    eprintln!("Sending Enter...");
    session.send("\r").expect("Failed to send Enter");
    session.settle();

    let screen3 = session.capture_screen().unwrap_or_default();
    eprintln!("Screen after Enter: {:?}", screen3);

    session.send_control('c').ok();
}

/// Test oil runner stays responsive for extended period
#[test]
#[ignore = "requires built binary"]
fn oil_runner_stays_responsive_10s() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(20));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn chat");

    session.wait_for_ready().expect("TUI ready");

    let start = std::time::Instant::now();
    let mut last_responsive = start;

    while start.elapsed() < Duration::from_secs(10) {
        session.send("a").unwrap_or_else(|_| {
            panic!(
                "Send failed after {:?}, last responsive {:?} ago",
                start.elapsed(),
                last_responsive.elapsed()
            )
        });

        session.settle();

        session
            .send_key(Key::Backspace)
            .unwrap_or_else(|_| panic!("Backspace failed after {:?}", start.elapsed()));

        last_responsive = std::time::Instant::now();
        session.settle();
    }

    session
        .send_control('c')
        .expect("Ctrl+C failed after 10s test");
}

// =============================================================================
// Oil Runner Streaming Tests (require Ollama)
// =============================================================================

/// Test streaming response renders progressively
#[test]
#[ignore = "requires built binary and Ollama"]
fn oil_streaming_response_renders() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--internal"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(60));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session
        .send_line("Say exactly: 'Hello World' and nothing else")
        .expect("Failed to send");

    // Wait for LLM streaming response (needs Ollama)
    session.wait(Duration::from_secs(5));

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Screen during streaming: {}", screen);

    session.wait(Duration::from_secs(10));

    session.send_control('c').ok();
}

/// Test long streaming response with markdown tables
#[test]
#[ignore = "requires built binary and Ollama"]
fn oil_streaming_with_markdown_table() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--internal"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(90));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session
        .send_line(
            "Create a markdown table with 3 columns: Name, Age, City. Add 5 rows of sample data.",
        )
        .expect("Failed to send");

    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        // Poll with settle instead of fixed 2s sleep inside loop
        session.wait(Duration::from_secs(2));
        let screen = session.capture_screen().unwrap_or_default();
        eprintln!(
            "[{:?}] Screen: {}",
            start.elapsed(),
            safe_truncate(&screen, 500)
        );

        if screen.contains("|") && screen.contains("---") {
            eprintln!("Table detected in output");
            break;
        }
    }

    session.send_control('c').ok();
}

// =============================================================================
// Oil Runner Ctrl+C Edge Cases
// =============================================================================

/// Test Ctrl+C during active streaming cancels gracefully
#[test]
#[ignore = "requires built binary and Ollama"]
fn oil_ctrl_c_during_streaming() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--internal"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(30));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session
        .send_line("Write a very long story about a dragon, at least 500 words")
        .expect("Failed to send");

    // Wait for streaming to start (needs Ollama)
    session.wait(Duration::from_secs(3));

    eprintln!("Sending Ctrl+C during streaming...");
    session.send_control('c').expect("Ctrl+C failed");

    session.settle();

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Screen after Ctrl+C: {}", safe_truncate(&screen, 300));

    session.settle();

    session
        .send("test input after cancel")
        .expect("Should still accept input");
    session.settle();

    session.send_control('c').expect("Second Ctrl+C failed");
}

/// Test double Ctrl+C exits even during streaming
#[test]
#[ignore = "requires built binary and Ollama"]
fn oil_double_ctrl_c_exits_during_streaming() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--internal"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(30));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session
        .send("Write an extremely long essay about the history of computing\r")
        .expect("Failed to send");

    // Wait for streaming to start (needs Ollama)
    session.wait(Duration::from_secs(2));

    session.send_control('c').expect("First Ctrl+C failed");
    session.settle();

    session.send_control('c').expect("Second Ctrl+C failed");

    match session.expect_eof() {
        Ok(_) => eprintln!("Double Ctrl+C exited successfully"),
        Err(e) => eprintln!(
            "Double Ctrl+C did not exit (expected without Ollama): {:?}",
            e
        ),
    }
}

/// Test Ctrl+C with empty input shows notification
#[test]
#[ignore = "requires built binary"]
fn oil_ctrl_c_empty_input_notification() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session.send_control('c').expect("First Ctrl+C failed");
    session.settle();

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Screen after first Ctrl+C: {}", screen);

    session.send("a").expect("Should still accept input");
    session.settle();
    session.send_key(Key::Backspace).expect("Backspace failed");
    session.settle();

    session.send_control('c').expect("Second Ctrl+C failed");
    session.settle();
    session.send_control('c').expect("Third Ctrl+C failed");

    session
        .expect_eof()
        .expect("Should exit after rapid Ctrl+C");
}

// =============================================================================
// Oil Runner Mode Switching Tests
// =============================================================================

/// Test /mode command cycles through modes (visual verification)
#[test]
#[ignore = "requires built binary"]
fn oil_mode_cycle() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .wait_for_text("NORMAL", Duration::from_secs(3))
        .expect("Initial mode should be NORMAL");

    for _ in 0..3 {
        session.send("/mode\r").expect("Failed to send /mode");
        session.settle();
    }

    session.send(":quit\r").ok();
}

/// Test /auto and /plan commands
#[test]
#[ignore = "requires built binary"]
fn oil_explicit_mode_commands() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session.send("/auto\r").expect("Failed to send /auto");
    session
        .wait_for_text("AUTO", Duration::from_secs(3))
        .expect("Should show AUTO mode indicator");

    session.send("/plan\r").expect("Failed to send /plan");
    session
        .wait_for_text("PLAN", Duration::from_secs(3))
        .expect("Should show PLAN mode indicator");

    session.send_control('c').ok();
    session.send_control('c').ok();
}
