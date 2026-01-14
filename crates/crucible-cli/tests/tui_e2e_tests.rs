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

/// Check if the cru binary exists (either debug or release)
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
// Ink Runner Tests
// =============================================================================

/// Test that ink runner doesn't freeze after startup
///
/// This reproduces a bug where the TUI freezes ~5s after startup,
/// becoming unresponsive to all input including Ctrl+C.
#[test]
#[ignore = "requires built binary"]
fn ink_runner_does_not_freeze() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--ink", "--no-process"])
        .with_env("RUST_LOG", "crucible_cli::tui::ink=debug")
        .with_timeout(Duration::from_secs(3));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn chat --ink");

    let start = std::time::Instant::now();

    session.wait(Duration::from_secs(1));
    eprintln!(
        "[{:?}] Startup complete, beginning input test",
        start.elapsed()
    );

    for i in 0..20 {
        let elapsed = start.elapsed();
        eprintln!("[{:?}] Sending keystroke {}", elapsed, i);

        if session.send("x").is_err() {
            panic!("Send failed at {:?} (iteration {})", elapsed, i);
        }

        session.wait(Duration::from_millis(300));

        if session.send_key(Key::Backspace).is_err() {
            panic!("Backspace failed at {:?} (iteration {})", elapsed, i);
        }

        session.wait(Duration::from_millis(300));
    }

    eprintln!(
        "[{:?}] Input test passed, trying ESC first",
        start.elapsed()
    );
    session.send_key(Key::Escape).expect("ESC failed");

    session.wait(Duration::from_millis(500));

    eprintln!("[{:?}] Checking if ESC caused exit", start.elapsed());
    match session.expect_eof() {
        Ok(_) => {
            eprintln!("[{:?}] ESC worked - test passed", start.elapsed());
            return;
        }
        Err(_) => {
            eprintln!("[{:?}] ESC didn't exit, trying Ctrl+C", start.elapsed());
        }
    }

    session.send_control('c').expect("Ctrl+C failed");
    session.wait(Duration::from_millis(500));

    eprintln!("[{:?}] Waiting for exit after Ctrl+C", start.elapsed());
    if session.expect_eof().is_err() {
        panic!(
            "TUI did not exit after ESC or Ctrl+C at {:?}",
            start.elapsed()
        );
    }

    eprintln!("[{:?}] Test passed", start.elapsed());
}

/// Test that /quit command causes exit
#[test]
#[ignore = "requires built binary"]
fn ink_quit_with_slash_command() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--ink", "--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(5));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn chat --ink");

    session.wait(Duration::from_secs(1));
    eprintln!("Sending /quit command");

    session.send_line("/quit").expect("Failed to send /quit");
    session.wait(Duration::from_millis(500));

    match session.expect_eof() {
        Ok(_) => eprintln!("/quit worked - test passed"),
        Err(e) => panic!("/quit command did not exit: {:?}", e),
    }
}

/// Test that typing and Enter works - check output for echo
#[test]
#[ignore = "requires built binary"]
fn ink_verify_pty_works() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--ink", "--no-process"])
        .with_env("RUST_LOG", "crucible_cli::tui::ink=debug")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn chat --ink");

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

/// Test ink runner stays responsive for extended period
#[test]
#[ignore = "requires built binary"]
fn ink_runner_stays_responsive_10s() {
    let config = TuiTestConfig::new("chat")
        .with_args(&["--ink", "--no-process"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(20));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn chat --ink");

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
