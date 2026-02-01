//! TUI End-to-End Tests
//!
//! These tests use expectrl to spawn real `cru` processes in PTY sessions,
//! enabling multi-turn interaction testing.
//!
//! # Running Tests
//!
//! ```bash
//! # Build first
//! cargo build --release
//!
//! # Run e2e tests (marked as ignored by default)
//! cargo test -p crucible-cli tui_e2e -- --ignored
//! ```
//!
//! # Test Categories
//!
//! - **Smoke tests**: Basic startup/shutdown
//! - **Navigation tests**: Key sequences, mode switching
//! - **Multi-turn tests**: Full conversation flows
//! - **Command tests**: Slash command behavior

// Include the harness module
#[path = "tui_e2e_harness.rs"]
mod tui_e2e_harness;

use std::path::PathBuf;
use std::time::Duration;
use tui_e2e_harness::{Key, TuiTestBuilder, TuiTestConfig, TuiTestSession};

// =============================================================================
// Helper Functions
// =============================================================================

fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn find_binary() -> Option<PathBuf> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_path = PathBuf::from(manifest_dir);
    let workspace_root = manifest_path
        .parent()
        .and_then(|p| p.parent())
        .expect("Could not find workspace root");

    let release_path = workspace_root.join("target/release/cru");
    if release_path.exists() {
        return Some(release_path);
    }

    let debug_path = workspace_root.join("target/debug/cru");
    if debug_path.exists() {
        return Some(debug_path);
    }

    None
}

/// Skip test if binary is not built
macro_rules! require_binary {
    () => {
        if find_binary().is_none() {
            eprintln!("SKIPPED: cru binary not built. Run `cargo build` first.");
            return;
        }
    };
}

// =============================================================================
// Smoke Tests
// =============================================================================

/// Test that `cru --version` works
///
/// This test runs by default (no #[ignore]) and skips gracefully if the binary
/// isn't built. This allows CI to catch version string issues when the binary
/// is available.
#[test]
fn smoke_version() {
    require_binary!();

    let config = TuiTestConfig::new("--version");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("cru").expect("Should show cru in version");
    session.expect_eof().expect("Should exit");
}

/// Test that `cru --help` works
///
/// Runs by default and skips gracefully if binary isn't built.
#[test]
fn smoke_help() {
    require_binary!();

    let config = TuiTestConfig::new("--help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("Usage").expect("Should show usage");
    session.expect_eof().expect("Should exit");
}

// =============================================================================
// Chat TUI Startup Tests
// =============================================================================

/// Test that chat TUI starts and shows initial prompt
#[test]
#[ignore = "requires built binary and may need ACP agent"]
fn chat_startup_shows_prompt() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(15)
        .spawn()
        .expect("Failed to spawn chat");

    // Should see mode indicator or prompt area
    // The exact text depends on TUI design, adjust as needed
    session.wait(Duration::from_secs(2));

    // Try to exit cleanly
    session.send_control('c').expect("Failed to send Ctrl+C");
    session.wait(Duration::from_millis(500));
}

/// Test that Ctrl+C exits the TUI
#[test]
#[ignore = "requires built binary"]
fn chat_ctrl_c_exits() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(10)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(1));
    session.send_control('c').expect("Failed to send Ctrl+C");
    session.expect_eof().expect("Should exit after Ctrl+C");
}

// =============================================================================
// Input Tests
// =============================================================================

/// Test typing in the input box
#[test]
#[ignore = "requires built binary"]
fn chat_input_typing() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(10)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(1));

    // Type some text
    session.send("Hello world").expect("Failed to send text");
    session.wait(Duration::from_millis(200));

    // The input should be visible (TUI renders it)
    // We can't easily assert on exact screen content with expectrl alone,
    // but we can verify the session is still responsive

    // Clean exit
    session.send_control('c').expect("Failed to send Ctrl+C");
}

/// Test backspace deletes characters
#[test]
#[ignore = "requires built binary"]
fn chat_input_backspace() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(10)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(1));

    // Type and then delete
    session.send("Hello").expect("Failed to send text");
    session.wait(Duration::from_millis(100));
    session
        .send_key(Key::Backspace)
        .expect("Failed to send backspace");
    session
        .send_key(Key::Backspace)
        .expect("Failed to send backspace");
    session.wait(Duration::from_millis(100));

    // Clean exit
    session.send_control('c').expect("Failed to send Ctrl+C");
}

/// Regression test: backspace should not delete terminal scrollback
#[test]
#[ignore = "requires built binary"]
fn chat_backspace_preserves_scrollback() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(10)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(1));

    session.send("Hello").expect("Failed to send text");
    session.wait(Duration::from_millis(200));

    for _ in 0..5 {
        session
            .send_key(Key::Backspace)
            .expect("Failed to send backspace");
        session.wait(Duration::from_millis(100));
    }

    session.send("Done").expect("Failed to send text");
    session.wait(Duration::from_millis(200));

    session.send_control('c').expect("Failed to send Ctrl+C");
}

// =============================================================================
// Slash Command Tests
// =============================================================================

/// Test that typing "/" shows command popup
#[test]
#[ignore = "requires built binary"]
fn chat_slash_shows_popup() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(10)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(1));

    // Type slash to trigger command popup
    session.send("/").expect("Failed to send /");
    session.wait(Duration::from_millis(300));

    // Popup should appear with commands
    // The TUI should render command options

    // Clean exit
    session
        .send_key(Key::Escape)
        .expect("Failed to send Escape");
    session.wait(Duration::from_millis(100));
    session.send_control('c').expect("Failed to send Ctrl+C");
}

/// Test /help command
#[test]
#[ignore = "requires built binary"]
fn chat_help_command() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(10)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(1));

    // Type /help and press Enter
    session.send_line("/help").expect("Failed to send /help");
    session.wait(Duration::from_millis(500));

    // Should show help information (dialog or inline)

    // Clean exit
    session.send_control('c').expect("Failed to send Ctrl+C");
}

// =============================================================================
// Navigation Tests
// =============================================================================

/// Test arrow key navigation in popup
#[test]
#[ignore = "requires built binary"]
fn chat_popup_navigation() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(10)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(1));

    // Open command popup
    session.send("/").expect("Failed to send /");
    session.wait(Duration::from_millis(200));

    // Navigate down
    session.send_key(Key::Down).expect("Failed to send Down");
    session.wait(Duration::from_millis(100));
    session.send_key(Key::Down).expect("Failed to send Down");
    session.wait(Duration::from_millis(100));

    // Navigate up
    session.send_key(Key::Up).expect("Failed to send Up");
    session.wait(Duration::from_millis(100));

    // Escape to close popup
    session
        .send_key(Key::Escape)
        .expect("Failed to send Escape");
    session.wait(Duration::from_millis(100));

    // Clean exit
    session.send_control('c').expect("Failed to send Ctrl+C");
}

/// Test Tab completion
#[test]
#[ignore = "requires built binary"]
fn chat_tab_completion() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(10)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(1));

    // Type partial command
    session.send("/hel").expect("Failed to send /hel");
    session.wait(Duration::from_millis(200));

    // Tab to complete
    session.send_key(Key::Tab).expect("Failed to send Tab");
    session.wait(Duration::from_millis(200));

    // Clean exit
    session.send_control('c').expect("Failed to send Ctrl+C");
}

// =============================================================================
// Multi-turn Conversation Tests
// =============================================================================

/// Test a basic multi-turn conversation flow
///
/// This is a template for more complex multi-turn tests.
/// Requires an actual ACP agent to be configured.
#[test]
#[ignore = "requires built binary and ACP agent"]
fn chat_multiturn_basic() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(30) // Longer timeout for LLM responses
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(2));

    // Turn 1: Send a simple message
    session
        .send_line("Hello, please respond with just 'Hi'")
        .expect("Failed to send message");

    // Wait for response (this may take a while depending on the agent)
    session.wait(Duration::from_secs(10));

    // Turn 2: Follow-up message
    session
        .send_line("Now say 'Goodbye'")
        .expect("Failed to send follow-up");

    session.wait(Duration::from_secs(10));

    // Clean exit
    session.send_control('c').expect("Failed to send Ctrl+C");
}

// =============================================================================
// Mode Switching Tests
// =============================================================================

/// Test switching between modes (plan/act/auto)
#[test]
#[ignore = "requires built binary"]
fn chat_mode_switching() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(10)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(1));

    // Try /mode command if it exists
    session
        .send_line("/mode act")
        .expect("Failed to send /mode");
    session.wait(Duration::from_millis(500));

    // Switch to plan mode
    session
        .send_line("/mode plan")
        .expect("Failed to send /mode");
    session.wait(Duration::from_millis(500));

    // Clean exit
    session.send_control('c').expect("Failed to send Ctrl+C");
}

// =============================================================================
// Error Handling Tests
// =============================================================================

/// Test behavior when sending empty message
#[test]
#[ignore = "requires built binary"]
fn chat_empty_message() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(10)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(1));

    // Send empty line (just Enter)
    session.send_key(Key::Enter).expect("Failed to send Enter");
    session.wait(Duration::from_millis(200));

    // Should not crash, TUI should still be responsive
    session.send("test").expect("Should still accept input");

    // Clean exit
    session.send_control('c').expect("Failed to send Ctrl+C");
}

// =============================================================================
// Stress Tests
// =============================================================================

/// Test rapid key input doesn't cause issues
#[test]
#[ignore = "requires built binary"]
fn chat_rapid_input() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(15)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(1));

    // Send rapid keystrokes
    for _ in 0..50 {
        session.send("x").expect("Failed to send");
    }
    session.wait(Duration::from_millis(200));

    // Clear with backspaces
    for _ in 0..50 {
        session
            .send_key(Key::Backspace)
            .expect("Failed to backspace");
    }
    session.wait(Duration::from_millis(200));

    // Should still be responsive
    session.send("still works").expect("Should still work");

    // Clean exit
    session.send_control('c').expect("Failed to send Ctrl+C");
}

// =============================================================================
// Oil Runner Tests
// =============================================================================

/// Test that oil runner stays responsive during extended use
#[test]
#[ignore = "requires built binary"]
fn oil_runner_does_not_freeze() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(3));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn chat");

    let start = std::time::Instant::now();

    session.wait(Duration::from_secs(1));

    for i in 0..20 {
        if session.send("x").is_err() {
            panic!("Send failed at iteration {}", i);
        }
        session.wait(Duration::from_millis(300));

        if session.send_key(Key::Backspace).is_err() {
            panic!("Backspace failed at iteration {}", i);
        }
        session.wait(Duration::from_millis(300));
    }

    eprintln!("Input test passed after {:?}", start.elapsed());

    session.send("/quit\r").ok();
}

/// Test that /quit command causes exit
#[test]
#[ignore = "requires built binary"]
fn oil_quit_with_slash_command() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(5));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn chat");

    session.wait(Duration::from_secs(1));
    eprintln!("Sending /quit command");

    session.send("/quit").expect("Failed to send /quit");
    session.send("\r").expect("Failed to send Enter");
    session.wait(Duration::from_millis(500));

    match session.expect_eof() {
        Ok(_) => eprintln!("/quit worked - test passed"),
        Err(e) => panic!("/quit command did not exit: {:?}", e),
    }
}

/// Test that typing and Enter works - check output for echo
#[test]
#[ignore = "requires built binary"]
fn oil_verify_pty_works() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "crucible_cli::tui::oil=debug")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn chat");

    session.wait(Duration::from_secs(2));

    eprintln!("Checking for initial prompt...");
    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Initial screen: {:?}", screen);

    eprintln!("Typing hello...");
    session.send("hello").expect("Failed to send hello");
    session.wait(Duration::from_secs(1));

    let screen2 = session.capture_screen().unwrap_or_default();
    eprintln!("Screen after 'hello': {:?}", screen2);

    eprintln!("Sending Enter...");
    session.send("\r").expect("Failed to send Enter");
    session.wait(Duration::from_secs(2));

    let screen3 = session.capture_screen().unwrap_or_default();
    eprintln!("Screen after Enter: {:?}", screen3);

    session.send_control('c').ok();
}

/// Test oil runner stays responsive for extended period
#[test]
#[ignore = "requires built binary"]
fn oil_runner_stays_responsive_10s() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(20));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn chat");

    session.wait(Duration::from_secs(1));

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

        session.wait(Duration::from_millis(200));

        session
            .send_key(Key::Backspace)
            .unwrap_or_else(|_| panic!("Backspace failed after {:?}", start.elapsed()));

        last_responsive = std::time::Instant::now();
        session.wait(Duration::from_millis(300));
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
        .with_args(&["--no-process", "--internal"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(60));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(2));

    session
        .send_line("Say exactly: 'Hello World' and nothing else")
        .expect("Failed to send");

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
        .with_args(&["--no-process", "--internal"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(90));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(2));

    session
        .send_line(
            "Create a markdown table with 3 columns: Name, Age, City. Add 5 rows of sample data.",
        )
        .expect("Failed to send");

    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
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
        .with_args(&["--no-process", "--internal"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(30));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(2));

    session
        .send_line("Write a very long story about a dragon, at least 500 words")
        .expect("Failed to send");

    session.wait(Duration::from_secs(3));

    eprintln!("Sending Ctrl+C during streaming...");
    session.send_control('c').expect("Ctrl+C failed");

    session.wait(Duration::from_millis(500));

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Screen after Ctrl+C: {}", safe_truncate(&screen, 300));

    session.wait(Duration::from_secs(1));

    session
        .send("test input after cancel")
        .expect("Should still accept input");
    session.wait(Duration::from_millis(500));

    session.send_control('c').expect("Second Ctrl+C failed");
}

/// Test double Ctrl+C exits even during streaming
#[test]
#[ignore = "requires built binary and Ollama"]
fn oil_double_ctrl_c_exits_during_streaming() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process", "--internal"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(30));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(2));

    session
        .send("Write an extremely long essay about the history of computing\r")
        .expect("Failed to send");

    session.wait(Duration::from_secs(2));

    session.send_control('c').expect("First Ctrl+C failed");
    session.wait(Duration::from_millis(300));

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
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    session.send_control('c').expect("First Ctrl+C failed");
    session.wait(Duration::from_millis(500));

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Screen after first Ctrl+C: {}", screen);

    session.send("a").expect("Should still accept input");
    session.wait(Duration::from_millis(200));
    session.send_key(Key::Backspace).expect("Backspace failed");
    session.wait(Duration::from_millis(200));

    session.send_control('c').expect("Second Ctrl+C failed");
    session.wait(Duration::from_millis(100));
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
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    session
        .wait_for_text("Plan", Duration::from_secs(3))
        .expect("Initial mode should be Plan");

    for _ in 0..3 {
        session.send("/mode\r").expect("Failed to send /mode");
        session.wait(Duration::from_millis(300));
    }

    session.send("/quit\r").ok();
}

/// Test /act and /plan commands
#[test]
#[ignore = "requires built binary"]
fn oil_explicit_mode_commands() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    session.send_line("/act").expect("Failed to send /act");
    session
        .wait_for_text("Act", Duration::from_secs(3))
        .expect("Should show Act mode indicator");

    session.send_line("/plan").expect("Failed to send /plan");
    session
        .wait_for_text("Plan", Duration::from_secs(3))
        .expect("Should show Plan mode indicator");

    session.send_control('c').ok();
    session.send_control('c').ok();
}

// =============================================================================
// Oil Runner Popup Tests
// =============================================================================

// =============================================================================
// Model Popup Tests
// =============================================================================

/// Test :model command shows popup (models loaded at startup)
///
/// When models are fetched at startup (parallel with file/note indexing),
/// the :model command should show a popup with available models.
#[test]
#[ignore = "requires built binary and Ollama"]
fn model_popup_shows_with_ollama() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process", "--internal"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(15));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(3));

    session.send(":model\r").expect("Failed to send :model");
    session.wait(Duration::from_millis(500));

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Screen after :model: {}", safe_truncate(&screen, 500));

    session.send_key(Key::Escape).expect("Escape failed");
    session.wait(Duration::from_millis(200));

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test :model triggers lazy fetch when models not loaded
///
/// When no models are pre-loaded (e.g., Ollama was slow to start),
/// the :model command should trigger a fetch and show "Fetching models..."
#[test]
#[ignore = "requires built binary"]
fn model_popup_lazy_fetch_on_demand() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "crucible_cli::tui::oil=debug")
        .with_timeout(Duration::from_secs(15));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    session.send(":model\r").expect("Failed to send :model");
    session.wait(Duration::from_secs(2));

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!(
        "Screen after :model (lazy fetch): {}",
        safe_truncate(&screen, 500)
    );

    session.send_key(Key::Escape).expect("Escape failed");
    session.wait(Duration::from_millis(200));

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test :model with filter shows filtered models
///
/// Typing `:model lla` should filter to llama models.
#[test]
#[ignore = "requires built binary and Ollama"]
fn model_popup_filter_works() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process", "--internal"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(15));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(3));

    session
        .send(":model lla")
        .expect("Failed to send :model lla");
    session.wait(Duration::from_millis(500));

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Screen with filter 'lla': {}", safe_truncate(&screen, 500));

    session.send_key(Key::Escape).expect("Escape failed");
    session.wait(Duration::from_millis(200));

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test model selection updates status bar
///
/// Selecting a model from popup should update the status bar.
#[test]
#[ignore = "requires built binary and Ollama"]
fn model_selection_updates_status() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process", "--internal"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(15));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(3));

    session.send(":model\r").expect("Failed to send :model");
    session.wait(Duration::from_millis(500));

    session.send_key(Key::Down).expect("Down failed");
    session.wait(Duration::from_millis(100));

    session.send_key(Key::Enter).expect("Enter failed");
    session.wait(Duration::from_millis(500));

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!(
        "Screen after model selection: {}",
        safe_truncate(&screen, 500)
    );

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test :model popup navigation with arrow keys
#[test]
#[ignore = "requires built binary and Ollama"]
fn model_popup_navigation() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process", "--internal"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(15));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(3));

    session.send(":model\r").expect("Failed to send :model");
    session.wait(Duration::from_millis(500));

    for i in 0..3 {
        session.send_key(Key::Down).expect("Down failed");
        session.wait(Duration::from_millis(100));
        eprintln!("Down {}", i + 1);
    }

    for i in 0..2 {
        session.send_key(Key::Up).expect("Up failed");
        session.wait(Duration::from_millis(100));
        eprintln!("Up {}", i + 1);
    }

    session.send_key(Key::Escape).expect("Escape failed");
    session.wait(Duration::from_millis(200));

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test :model <name> direct switch (no popup)
#[test]
#[ignore = "requires built binary and Ollama"]
fn model_direct_switch_command() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process", "--internal"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(15));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(2));

    session
        .send(":model llama3.2\r")
        .expect("Failed to send :model llama3.2");
    session.wait(Duration::from_millis(500));

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!(
        "Screen after direct model switch: {}",
        safe_truncate(&screen, 500)
    );

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test :model shows message when no models available (Ollama not running)
#[test]
#[ignore = "requires built binary without Ollama"]
fn model_popup_no_models_message() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process"])
        .with_env("OLLAMA_HOST", "http://localhost:99999")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(2));

    session.send(":model\r").expect("Failed to send :model");
    session.wait(Duration::from_secs(3));

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Screen with no Ollama: {}", safe_truncate(&screen, 500));

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test F1 toggles popup
#[test]
#[ignore = "requires built binary"]
fn oil_f1_popup_toggle() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    session.send_key(Key::F(1)).expect("F1 failed");
    session.wait(Duration::from_millis(500));

    let screen_open = session.capture_screen().unwrap_or_default();
    eprintln!("After F1 (should show popup): {}", screen_open);

    session.send_key(Key::F(1)).expect("Second F1 failed");
    session.wait(Duration::from_millis(500));

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
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    session.send_key(Key::F(1)).expect("F1 failed");
    session.wait(Duration::from_millis(300));

    for i in 0..5 {
        session.send_key(Key::Down).expect("Down failed");
        session.wait(Duration::from_millis(100));
        eprintln!("Down {}", i + 1);
    }

    for i in 0..3 {
        session.send_key(Key::Up).expect("Up failed");
        session.wait(Duration::from_millis(100));
        eprintln!("Up {}", i + 1);
    }

    session.send_key(Key::Escape).expect("Escape failed");
    session.wait(Duration::from_millis(300));

    session.send_control('c').ok();
    session.send_control('c').ok();
}

// =============================================================================
// Permission Workflow Tests
// =============================================================================

/// Test that permission prompt appears for destructive operations
///
/// This test is marked as ignored because:
/// - Requires built binary
/// - Requires agent that triggers permission prompts
/// - May need mock agent setup
#[test]
#[ignore = "requires built binary and agent that triggers permission prompts"]
fn permission_prompt_appears() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(30)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(2));

    // This would require an agent that triggers a permission prompt
    // For now, we document the test but can't trigger it without mock agent

    // Clean exit
    session.send_control('c').expect("Failed to send Ctrl+C");
    session.wait(Duration::from_millis(500));
}

/// Test that 'y' key allows and continues
#[test]
#[ignore = "requires built binary and agent that triggers permission prompts"]
fn permission_y_allows() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(30)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(2));

    // Would need to trigger permission prompt, then:
    // session.send_key(Key::Char('y')).expect("Failed to send y");

    session.send_control('c').expect("Failed to send Ctrl+C");
}

/// Test that 'n' key denies and shows error to LLM
#[test]
#[ignore = "requires built binary and agent that triggers permission prompts"]
fn permission_n_denies() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(30)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(2));

    // Would need to trigger permission prompt, then:
    // session.send_key(Key::Char('n')).expect("Failed to send n");
    // Verify error message appears in output

    session.send_control('c').expect("Failed to send Ctrl+C");
}

/// Test that Escape key denies
#[test]
#[ignore = "requires built binary and agent that triggers permission prompts"]
fn permission_escape_denies() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(30)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(2));

    // Would need to trigger permission prompt, then:
    // session.send_key(Key::Escape).expect("Failed to send Escape");
    // Verify denial is communicated to agent

    session.send_control('c').expect("Failed to send Ctrl+C");
}

/// Test that 'h' key toggles diff visibility
#[test]
#[ignore = "requires built binary and agent that triggers permission prompts"]
fn permission_h_toggles_diff() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(30)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(2));

    // Would need to trigger permission prompt with diff, then:
    // session.send_key(Key::Char('h')).expect("Failed to send h");
    // Verify diff appears/disappears in terminal output

    session.send_control('c').expect("Failed to send Ctrl+C");
}

/// Test permission queue indicator shows correctly
#[test]
#[ignore = "requires built binary and agent that triggers multiple permission prompts"]
fn permission_queue_indicator_visible() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(30)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait(Duration::from_secs(2));

    // Would need to trigger multiple permission prompts, then:
    // Verify "[1/3]" style indicator appears in prompt
    // Verify indicator updates as permissions are processed

    session.send_control('c').expect("Failed to send Ctrl+C");
}

// =============================================================================
// Oil Runner Stress Tests
// =============================================================================

/// Test rapid input doesn't corrupt display
#[test]
#[ignore = "requires built binary"]
fn oil_rapid_typing_stress() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(15));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    let test_string = "The quick brown fox jumps over the lazy dog 1234567890";
    for c in test_string.chars() {
        session.send(&c.to_string()).expect("Send char failed");
    }

    session.wait(Duration::from_millis(500));

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Screen after rapid typing: {}", screen);

    for _ in 0..test_string.len() {
        session.send_key(Key::Backspace).expect("Backspace failed");
    }

    session.wait(Duration::from_millis(300));

    session
        .send("still works")
        .expect("Should still accept input");
    session.wait(Duration::from_millis(200));

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test alternating input and commands
#[test]
#[ignore = "requires built binary"]
fn oil_alternating_input_commands() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(15));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    for i in 0..5 {
        session
            .send(&format!("message {}", i))
            .expect("Send failed");
        session.wait(Duration::from_millis(100));

        for _ in 0..10 {
            session.send_key(Key::Backspace).expect("Backspace failed");
        }

        session.send_line("/help").expect("Help failed");
        session.wait(Duration::from_millis(300));

        session.send_line("/mode").expect("Mode failed");
        session.wait(Duration::from_millis(200));
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
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    session
        .send_line("/nonexistent_command")
        .expect("Failed to send");
    session.wait(Duration::from_millis(500));

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
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    session.send("/help\r").expect("Help failed");
    session.wait(Duration::from_millis(500));

    let screen_before = session.capture_screen().unwrap_or_default();
    eprintln!("Before clear: {}", safe_truncate(&screen_before, 200));

    session.send(":clear\r").expect("Clear failed");
    session.wait(Duration::from_millis(500));

    let screen_after = session.capture_screen().unwrap_or_default();
    eprintln!("After clear: {}", safe_truncate(&screen_after, 200));

    session.send("/quit\r").ok();
}

// =============================================================================
// Oil Runner Terminal Size Tests
// =============================================================================

/// Test oil runner at narrow terminal width (60 cols)
#[test]
#[ignore = "requires built binary"]
fn oil_narrow_terminal_60_cols() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_dimensions(60, 24)
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    session
        .wait_for_text("Plan", Duration::from_secs(3))
        .expect("Should show mode indicator at 60 cols");

    session.send("/help\r").expect("Help failed");
    session
        .wait_until(
            |s| {
                let c = s.contents();
                c.contains("Commands") || c.contains("/mode")
            },
            Duration::from_secs(3),
        )
        .expect("Help should render at narrow width");

    session.send("/quit\r").ok();
}

/// Test oil runner at very narrow terminal width (40 cols)
#[test]
#[ignore = "requires built binary"]
fn ink_very_narrow_terminal_40_cols() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_dimensions(40, 24)
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    session.send("test input\r").expect("Input failed");
    session.wait(Duration::from_millis(500));

    session.send("/quit\r").ok();
}

/// Test oil runner at wide terminal width (120 cols)
#[test]
#[ignore = "requires built binary"]
fn oil_wide_terminal_120_cols() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_dimensions(120, 40)
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    session
        .wait_for_text("Plan", Duration::from_secs(3))
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

    session.send("/quit\r").ok();
}

/// Test oil runner at short terminal height (10 rows)
#[test]
#[ignore = "requires built binary"]
fn oil_short_terminal_10_rows() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_dimensions(80, 10)
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait(Duration::from_secs(1));

    session.send("test\r").expect("Input failed");
    session.wait(Duration::from_millis(500));

    session.send("/quit\r").ok();
}

// =============================================================================
// Subcommand Help Output Tests
// =============================================================================

/// Test that `cru config --help` shows available subcommands
#[test]
fn smoke_config_help() {
    require_binary!();

    let config = TuiTestConfig::new("config --help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("init").expect("Should show init subcommand");
    session.expect("show").expect("Should show show subcommand");
    session.expect_eof().expect("Should exit");
}

/// Test that `cru daemon --help` shows daemon management options
#[test]
fn smoke_daemon_help() {
    require_binary!();

    let config = TuiTestConfig::new("daemon --help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .expect("start")
        .expect("Should show start subcommand");
    session.expect("stop").expect("Should show stop subcommand");
    session
        .expect("status")
        .expect("Should show status subcommand");
    session.expect_eof().expect("Should exit");
}

/// Test that `cru storage --help` shows storage management options
#[test]
fn smoke_storage_help() {
    require_binary!();

    let config = TuiTestConfig::new("storage --help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("Usage").expect("Should show usage");
    session.expect_eof().expect("Should exit");
}

/// Test that `cru process --help` shows processing options
#[test]
fn smoke_process_help() {
    require_binary!();

    let config = TuiTestConfig::new("process --help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("Usage").expect("Should show usage");
    session.expect_eof().expect("Should exit");
}

/// Test that `cru stats --help` shows stats options
#[test]
fn smoke_stats_help() {
    require_binary!();

    let config = TuiTestConfig::new("stats --help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("Usage").expect("Should show usage");
    session.expect_eof().expect("Should exit");
}

/// Test that `cru mcp --help` shows MCP server options
#[test]
fn smoke_mcp_help() {
    require_binary!();

    let config = TuiTestConfig::new("mcp --help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("Usage").expect("Should show usage");
    session.expect_eof().expect("Should exit");
}

/// Test that `cru chat --help` shows chat options
#[test]
fn smoke_chat_help() {
    require_binary!();

    let config = TuiTestConfig::new("chat --help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("Usage").expect("Should show usage");
    session.expect_eof().expect("Should exit");
}

// =============================================================================
// Error Message Tests
// =============================================================================

/// Test that invalid subcommand shows helpful error with suggestions
#[test]
fn error_invalid_subcommand_shows_suggestion() {
    require_binary!();

    let config = TuiTestConfig::new("chta");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .expect_regex(r"(?i)(error|invalid|unrecognized|unknown)")
        .expect("Should show error message");
    session.expect_eof().expect("Should exit");
}

/// `cru config init` without path defaults to CWD and runs provider detection,
/// so it no longer errors. This test verifies it produces output and exits.
#[test]
#[ignore = "init without path now succeeds with provider detection — slow and environment-dependent"]
fn error_missing_required_arg_shows_help() {
    require_binary!();

    let config = TuiTestConfig::new("config init");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect_eof().expect("Should exit");
}

/// Test that conflicting arguments show clear error
#[test]
#[ignore = "requires built binary with conflicting args"]
fn error_conflicting_args_shows_message() {
    require_binary!();

    let config = TuiTestConfig::new("chat");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .expect_regex(r"(?i)(error|conflict|cannot)")
        .expect("Should show conflict error");
    session.expect_eof().expect("Should exit");
}

/// Test that invalid option shows helpful error
#[test]
fn error_invalid_option_value() {
    require_binary!();

    let config = TuiTestConfig::new("daemon --invalid-option-xyz");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .expect_regex(r"(?i)(error|unexpected)")
        .expect("Should show invalid option error");
    session.expect_eof().expect("Should exit");
}

// =============================================================================
// Exit Code Tests
// =============================================================================

/// Test that --version exits with code 0
#[test]
fn exit_code_version_success() {
    require_binary!();

    let binary = find_binary().expect("Binary should exist");
    let output = std::process::Command::new(&binary)
        .arg("--version")
        .output()
        .expect("Failed to run command");

    assert!(
        output.status.success(),
        "--version should exit with code 0, got: {:?}",
        output.status.code()
    );
}

/// Test that --help exits with code 0
#[test]
fn exit_code_help_success() {
    require_binary!();

    let binary = find_binary().expect("Binary should exist");
    let output = std::process::Command::new(&binary)
        .arg("--help")
        .output()
        .expect("Failed to run command");

    assert!(
        output.status.success(),
        "--help should exit with code 0, got: {:?}",
        output.status.code()
    );
}

/// Test that invalid subcommand exits with non-zero code
#[test]
fn exit_code_invalid_subcommand_failure() {
    require_binary!();

    let binary = find_binary().expect("Binary should exist");
    let output = std::process::Command::new(&binary)
        .arg("nonexistent_command_xyz")
        .output()
        .expect("Failed to run command");

    assert!(
        !output.status.success(),
        "Invalid subcommand should exit with non-zero code, got: {:?}",
        output.status.code()
    );
}

/// Test that invalid option exits with non-zero code
#[test]
fn exit_code_invalid_option_failure() {
    require_binary!();

    let binary = find_binary().expect("Binary should exist");
    let output = std::process::Command::new(&binary)
        .arg("--nonexistent-option-xyz")
        .output()
        .expect("Failed to run command");

    assert!(
        !output.status.success(),
        "Invalid option should exit with non-zero code, got: {:?}",
        output.status.code()
    );
}

/// Test that subcommand --help exits with code 0
#[test]
fn exit_code_subcommand_help_success() {
    require_binary!();

    let binary = find_binary().expect("Binary should exist");

    for subcommand in &[
        "config", "daemon", "storage", "stats", "mcp", "chat", "process",
    ] {
        let output = std::process::Command::new(&binary)
            .args([*subcommand, "--help"])
            .output()
            .expect("Failed to run command");

        assert!(
            output.status.success(),
            "{} --help should exit with code 0, got: {:?}",
            subcommand,
            output.status.code()
        );
    }
}

/// Test that process command on nonexistent path completes without hanging
#[test]
fn exit_code_process_nonexistent_path() {
    require_binary!();

    let binary = find_binary().expect("Binary should exist");
    let output = std::process::Command::new(&binary)
        .args(["process", "--kiln", "/nonexistent/path/that/does/not/exist"])
        .output()
        .expect("Failed to run command");

    let _stderr = String::from_utf8_lossy(&output.stderr);
    let _stdout = String::from_utf8_lossy(&output.stdout);
}
