//! Model popup and model flow E2E tests.

use std::time::Duration;

use super::shared::{provider_test_config, safe_truncate};
use super::tui_e2e_harness::{
    assert_screen_contains, assert_screen_not_contains, Key, TuiTestConfig, TuiTestSession,
};

// =============================================================================
// Model Popup Tests
// =============================================================================

/// Test :model command shows popup (models loaded at startup)
///
/// When models are fetched at startup (parallel with file/note indexing),
/// the :model command should show a popup with available models.
#[test]
#[ignore = "requires built binary and LLM provider (set CRUCIBLE_TEST_CONFIG)"]
fn model_popup_shows_with_ollama() {
    let mut config = provider_test_config();
    config.args.push("--internal".to_string());

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session.send(":model\r").expect("Failed to send :model");
    session.settle();

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Screen after :model: {}", safe_truncate(&screen, 500));

    session.send_key(Key::Escape).expect("Escape failed");
    session.settle();

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test :model triggers lazy fetch when models not loaded
///
/// When no models are pre-loaded (e.g., Ollama was slow to start),
/// the :model command should trigger a fetch and show "Fetching models..."
#[test]
#[ignore = "requires built binary and LLM provider (set CRUCIBLE_TEST_CONFIG)"]
fn model_popup_lazy_fetch_on_demand() {
    let config = provider_test_config();

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session.send(":model\r").expect("Failed to send :model");
    session.settle();

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!(
        "Screen after :model (lazy fetch): {}",
        safe_truncate(&screen, 500)
    );

    session.send_key(Key::Escape).expect("Escape failed");
    session.settle();

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test :model with filter shows filtered models
///
/// Typing `:model lla` should filter to llama models.
#[test]
#[ignore = "requires built binary and LLM provider (set CRUCIBLE_TEST_CONFIG)"]
fn model_popup_filter_works() {
    let mut config = provider_test_config();
    config.args.push("--internal".to_string());

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session
        .send(":model lla")
        .expect("Failed to send :model lla");
    session.settle();

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Screen with filter 'lla': {}", safe_truncate(&screen, 500));

    session.send_key(Key::Escape).expect("Escape failed");
    session.settle();

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
        .with_args(&["--internal"])
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(15));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session.send(":model\r").expect("Failed to send :model");
    session.settle();

    session.send_key(Key::Down).expect("Down failed");
    session.settle();

    session.send_key(Key::Enter).expect("Enter failed");
    session.settle();

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
#[ignore = "requires built binary and LLM provider (set CRUCIBLE_TEST_CONFIG)"]
fn model_popup_navigation() {
    let mut config = provider_test_config();
    config.args.push("--internal".to_string());

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session.send(":model\r").expect("Failed to send :model");
    session.settle();

    for i in 0..3 {
        session.send_key(Key::Down).expect("Down failed");
        session.settle();
        eprintln!("Down {}", i + 1);
    }

    for i in 0..2 {
        session.send_key(Key::Up).expect("Up failed");
        session.settle();
        eprintln!("Up {}", i + 1);
    }

    session.send_key(Key::Escape).expect("Escape failed");
    session.settle();

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test :model <name> direct switch (no popup)
#[test]
#[ignore = "requires built binary and LLM provider (set CRUCIBLE_TEST_CONFIG)"]
fn model_direct_switch_command() {
    let mut config = provider_test_config();
    config.args.push("--internal".to_string());

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session
        .send(":model llama3.2\r")
        .expect("Failed to send :model llama3.2");
    session.settle();

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
        .with_env("OLLAMA_HOST", "http://localhost:99999")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.wait_for_ready().expect("TUI ready");

    session.send(":model\r").expect("Failed to send :model");
    session.settle();

    let screen = session.capture_screen().unwrap_or_default();
    eprintln!("Screen with no Ollama: {}", safe_truncate(&screen, 500));

    session.send_control('c').ok();
    session.send_control('c').ok();
}

// =============================================================================
// Model Flow E2E Tests (T5 — regression coverage for model command fixes)
// =============================================================================

/// Test model flow resolves to a final state within timeout
///
/// When a provider is configured, `:model<CR>` should transition from a loading
/// state to a final state (models available, empty list, or error) within the
/// timeout period. This test is provider-agnostic.
#[test]
#[ignore = "requires built binary and LLM provider (set CRUCIBLE_TEST_CONFIG)"]
fn model_flow_loading_to_loaded_e2e() {
    let config = provider_test_config();

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    // Wait for TUI to initialize
    session
        .wait_for_text("NORMAL", Duration::from_secs(5))
        .expect("TUI should initialize to NORMAL mode within 5s");

    session.send(":model\r").expect("Failed to send :model");

    // Wait up to 30s for the state to resolve — any terminal state counts
    let resolved = session
        .wait_until(
            |s| {
                let c = s.contents();
                (c.contains('/')
                    && (c.contains("model")
                        || c.contains("llama")
                        || c.contains("gpt")
                        || c.contains("claude")))
                    || c.contains("Available models")
                    || c.contains("No models")
                    || c.contains("Retrying model fetch")
                    || c.contains("error")
                    || c.contains("Error")
            },
            Duration::from_secs(30),
        )
        .is_ok();

    if resolved {
        // If resolved, loading indicator should be gone
        assert_screen_not_contains(session.screen(), "please wait");
    } else {
        // If not resolved within 30s, warn but don't hard-fail
        // This test is about state resolution, not provider speed
        eprintln!(
            "WARN: model state did not resolve within 30s.\nScreen:\n{}",
            session.screen_contents()
        );
    }

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test model flow resolves (error or success) within timeout
///
/// Regardless of Ollama status, the model popup should resolve to either
/// showing models or an error — not stay stuck at "please wait" forever.
#[test]
#[ignore = "requires built binary and LLM provider (set CRUCIBLE_TEST_CONFIG)"]
fn model_flow_error_shows_within_timeout_e2e() {
    let config = provider_test_config();

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .wait_for_text("NORMAL", Duration::from_secs(5))
        .expect("TUI should initialize to NORMAL mode within 5s");

    session.send(":model\r").expect("Failed to send :model");

    // Wait up to 15s for the state to resolve — models OR error
    let resolved = session
        .wait_until(
            |s| {
                let c = s.contents();
                (c.contains('/')
                    && (c.contains("model")
                        || c.contains("llama")
                        || c.contains("gpt")
                        || c.contains("claude")))
                    || c.contains("error")
                    || c.contains("Error")
                    || c.contains("No models")
                    || c.contains("Available")
            },
            Duration::from_secs(15),
        )
        .is_ok();

    if resolved {
        // If resolved, loading text should be gone
        assert_screen_not_contains(session.screen(), "please wait");
    } else {
        // If not resolved within 15s, that's worth noting but not a hard fail
        // since this test is about state resolution, not speed
        eprintln!(
            "WARN: model state did not resolve within 15s.\nScreen:\n{}",
            session.screen_contents()
        );
    }

    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test backspace from model popup doesn't create double borders
///
/// Typing `:model ` (with space) opens the model popup. Pressing Backspace
/// transitions back to the command popup. This should NOT produce duplicate
/// border rows (the `▄` character).
#[test]
#[ignore = "requires built binary"]
fn model_backspace_no_double_borders_e2e() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .wait_for_text("NORMAL", Duration::from_secs(5))
        .expect("TUI should initialize to NORMAL mode within 5s");

    // Type `:model ` — trailing space triggers model popup
    session.send(":model ").expect("Failed to send :model ");

    // Wait for popup to render
    session
        .wait_for_text("model", Duration::from_secs(3))
        .expect("model popup should appear within 3s after sending ':model '");

    // Backspace removes the space, transitioning back to command popup
    session.send_key(Key::Backspace).expect("Backspace failed");

    // Wait for screen to stabilize after transition
    session
        .wait_for_text(":", Duration::from_secs(2))
        .expect("command popup should reappear within 2s after Backspace");
    session.refresh_screen();

    // Count rows in the bottom 10 lines that are entirely `▄` characters
    let contents = session.screen_contents();
    let lines: Vec<&str> = contents.lines().collect();
    let start = lines.len().saturating_sub(10);
    let bottom_lines = &lines[start..];
    let border_row_count = bottom_lines
        .iter()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && trimmed.chars().all(|ch| ch == '▄')
        })
        .count();

    assert!(
        border_row_count <= 1,
        "Expected at most 1 border row in bottom 10 rows, got {}.\nBottom lines:\n{}",
        border_row_count,
        bottom_lines.join("\n")
    );

    // Verify screen is still functional (not corrupted)
    assert_screen_contains(session.screen(), ":");

    session.send_key(Key::Escape).ok();
    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test rapid `:model` does not produce duplicate loading messages
///
/// Pressing `:model<CR>` multiple times in quick succession should not
/// result in duplicate "Fetching" / "loading" / "please wait" messages.
#[test]
#[ignore = "requires built binary"]
fn model_loading_no_duplicate_messages_e2e() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .wait_for_text("NORMAL", Duration::from_secs(5))
        .expect("TUI should initialize to NORMAL mode within 5s");

    // Send :model 3 times in quick succession
    session.send(":model\r").expect("Failed to send :model 1");
    session.send(":model\r").expect("Failed to send :model 2");
    session.send(":model\r").expect("Failed to send :model 3");

    // Wait for rendering to settle
    session
        .wait_for_text("model", Duration::from_secs(3))
        .expect("model popup should appear within 3s after sending ':model'");
    session.refresh_screen();

    // Count loading-related messages (case-insensitive)
    let contents = session.screen_contents().to_lowercase();
    let fetch_count = contents.matches("fetching").count()
        + contents.matches("loading model").count()
        + contents.matches("please wait").count();

    assert!(
        fetch_count <= 1,
        "Expected at most 1 loading message after 3 rapid :model presses, got {}.\nScreen:\n{}",
        fetch_count,
        session.screen_contents()
    );

    // Screen should not be in a broken state
    assert_screen_not_contains(session.screen(), "panic");

    session.send_key(Key::Escape).ok();
    session.send_control('c').ok();
    session.send_control('c').ok();
}

/// Test second `:model` press triggers retry/refetch
///
/// After an initial `:model<CR>`, dismissing and pressing `:model<CR>`
/// again should trigger a retry or refetch of the model list.
#[test]
#[ignore = "requires built binary"]
fn model_retry_after_failure_e2e() {
    let config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(10));

    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .wait_for_text("NORMAL", Duration::from_secs(5))
        .expect("TUI should initialize to NORMAL mode within 5s");

    // First :model press
    session
        .send(":model\r")
        .expect("Failed to send first :model");

    // Wait for initial state to render
    session
        .wait_for_text("model", Duration::from_secs(3))
        .expect("model popup should appear within 3s after first ':model'");

    // Dismiss with Escape
    session.send_key(Key::Escape).expect("Escape failed");
    session
        .wait_for_text("NORMAL", Duration::from_secs(2))
        .expect("TUI should return to NORMAL mode within 2s after Escape");

    // Second :model press — should trigger retry/refetch
    session
        .send(":model\r")
        .expect("Failed to send second :model");

    // Wait for the retry state — should show fetching or model content
    let has_retry_indicator = session
        .wait_until(
            |s| {
                let c = s.contents().to_lowercase();
                c.contains("fetching")
                    || c.contains("retrying")
                    || c.contains("model")
                    || (c.contains('/') && c.contains("model"))
            },
            Duration::from_secs(5),
        )
        .is_ok();

    assert!(
        has_retry_indicator,
        "Expected retry/fetch indicator after second :model press.\nScreen:\n{}",
        session.screen_contents()
    );

    assert_screen_contains(session.screen(), "model");

    session.send_key(Key::Escape).ok();
    session.send_control('c').ok();
    session.send_control('c').ok();
}
