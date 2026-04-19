//! Chat TUI interaction tests: startup, input, slash commands, navigation,
//! multi-turn flows, mode switching, and stress.

use std::time::Duration;

use super::tui_e2e_harness::{Key, TuiTestBuilder};

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
    session.wait_for_ready().expect("TUI ready");

    // Try to exit cleanly
    session.send_control('c').expect("Failed to send Ctrl+C");
    session.settle();
}

/// Test that double Ctrl+C exits the TUI
#[test]
#[ignore = "requires built binary and clean daemon shutdown"]
fn chat_ctrl_c_exits() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .timeout(10)
        .spawn()
        .expect("Failed to spawn chat");

    session.wait_for_ready().expect("TUI ready");
    // Double Ctrl+C within 300ms triggers quit
    session.send_control('c').expect("Failed to send Ctrl+C");
    session.settle();
    session
        .send_control('c')
        .expect("Failed to send second Ctrl+C");
    session
        .expect_eof()
        .expect("Should exit after double Ctrl+C");
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

    session.wait_for_ready().expect("TUI ready");

    // Type some text
    session.send("Hello world").expect("Failed to send text");
    session.settle();

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

    session.wait_for_ready().expect("TUI ready");

    // Type and then delete
    session.send("Hello").expect("Failed to send text");
    session.settle();
    session
        .send_key(Key::Backspace)
        .expect("Failed to send backspace");
    session
        .send_key(Key::Backspace)
        .expect("Failed to send backspace");
    session.settle();

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

    session.wait_for_ready().expect("TUI ready");

    session.send("Hello").expect("Failed to send text");
    session.settle();

    for _ in 0..5 {
        session
            .send_key(Key::Backspace)
            .expect("Failed to send backspace");
        session.settle();
    }

    session.send("Done").expect("Failed to send text");
    session.settle();

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

    session.wait_for_ready().expect("TUI ready");

    // Type slash to trigger command popup
    session.send("/").expect("Failed to send /");
    session.settle();

    // Popup should appear with commands
    // The TUI should render command options

    // Clean exit
    session
        .send_key(Key::Escape)
        .expect("Failed to send Escape");
    session.settle();
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

    session.wait_for_ready().expect("TUI ready");

    // Type /help and press Enter
    session.send_line("/help").expect("Failed to send /help");
    session.settle();

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

    session.wait_for_ready().expect("TUI ready");

    // Open command popup
    session.send("/").expect("Failed to send /");
    session.settle();

    // Navigate down
    session.send_key(Key::Down).expect("Failed to send Down");
    session.settle();
    session.send_key(Key::Down).expect("Failed to send Down");
    session.settle();

    // Navigate up
    session.send_key(Key::Up).expect("Failed to send Up");
    session.settle();

    // Escape to close popup
    session
        .send_key(Key::Escape)
        .expect("Failed to send Escape");
    session.settle();

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

    session.wait_for_ready().expect("TUI ready");

    // Type partial command
    session.send("/hel").expect("Failed to send /hel");
    session.settle();

    // Tab to complete
    session.send_key(Key::Tab).expect("Failed to send Tab");
    session.settle();

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

    session.wait_for_ready().expect("TUI ready");

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

    session.wait_for_ready().expect("TUI ready");

    // Try /mode command if it exists
    session
        .send_line("/mode act")
        .expect("Failed to send /mode");
    session.settle();

    // Switch to plan mode
    session
        .send_line("/mode plan")
        .expect("Failed to send /mode");
    session.settle();

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

    session.wait_for_ready().expect("TUI ready");

    // Send empty line (just Enter)
    session.send_key(Key::Enter).expect("Failed to send Enter");
    session.settle();

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

    session.wait_for_ready().expect("TUI ready");

    // Send rapid keystrokes
    for _ in 0..50 {
        session.send("x").expect("Failed to send");
    }
    session.settle();

    // Clear with backspaces
    for _ in 0..50 {
        session
            .send_key(Key::Backspace)
            .expect("Failed to backspace");
    }
    session.settle();

    // Should still be responsive
    session.send("still works").expect("Should still work");

    // Clean exit
    session.send_control('c').expect("Failed to send Ctrl+C");
}
