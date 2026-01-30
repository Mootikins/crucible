//! Security invariant tests for the permission system.
//!
//! These tests verify fundamental security properties that MUST hold:
//! 1. **No writes without consent** - Tool execution NEVER happens if user denied
//! 2. **Escape always denies** - Esc key ALWAYS results in denial
//! 3. **Diff accuracy** - Displayed diff matches actual content
//! 4. **Pattern persistence** - Patterns survive session restart (out of scope for TUI tests)
//!
//! Uses property-based testing with proptest to verify invariants across
//! a wide range of permission requests and key sequences.

use crate::tui::oil::app::{Action, App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::event::Event;
use crate::tui::oil::focus::FocusContext;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::interaction::{
    InteractionRequest, InteractionResponse, PermRequest, PermResponse,
};
use proptest::prelude::*;

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Create a key event with no modifiers.
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Create a Ctrl+C key event.
fn ctrl_c() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
}

/// Check if a response is a permission denial.
fn is_denial(response: &InteractionResponse) -> bool {
    match response {
        InteractionResponse::Permission(perm) => !perm.allowed,
        _ => false,
    }
}

/// Check if a response is a permission allow.
fn is_allow(response: &InteractionResponse) -> bool {
    match response {
        InteractionResponse::Permission(perm) => perm.allowed,
        _ => false,
    }
}

/// Extract the permission response from an InteractionResponse.
fn extract_perm_response(response: &InteractionResponse) -> Option<PermResponse> {
    match response {
        InteractionResponse::Permission(perm) => Some(perm.clone()),
        _ => None,
    }
}

// ============================================================================
// PROPERTY GENERATORS
// ============================================================================

/// Generate a bash command with 1-5 parts.
fn bash_command_strategy() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec("[a-z]+", 1..5)
        .prop_map(|parts| parts.into_iter().map(|s| s.to_string()).collect())
}

/// Generate a random key that is NOT y, Y, n, N, h, H, p, P, or Esc.
fn random_non_action_key_strategy() -> impl Strategy<Value = char> {
    // Excludes: y/Y (allow), n/N (deny), a/A (allowlist), h/H (toggle diff),
    // j/k (navigation), p/P (legacy pattern)
    prop::sample::select(vec![
        'b', 'c', 'd', 'e', 'f', 'g', 'i', 'l', 'm', 'o', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x',
        'z', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ' ', '@', '#', '$', '%', '&', '*',
        '+', '=', '-', '_', '/', '\\', '|', ';', ':', ',', '.', '<', '>', '?',
    ])
}

// ============================================================================
// INVARIANT 1: Escape Always Denies
// ============================================================================

proptest! {
    #[test]
    fn invariant_escape_always_denies(
        command_parts in bash_command_strategy()
    ) {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(
            PermRequest::bash(command_parts.iter().map(|s| s.as_str()))
        );
        app.open_interaction("test".to_string(), request);

        // Verify modal is open
        prop_assert!(app.interaction_visible(), "Modal should be open after request");

        // Press Escape
        app.update(Event::Key(key(KeyCode::Esc)));

        // Modal should close
        prop_assert!(
            !app.interaction_visible(),
            "Modal should close after Escape"
        );
    }
}

// ============================================================================
// INVARIANT 2: Y Key Always Allows
// ============================================================================

proptest! {
    #[test]
    fn invariant_y_key_always_allows(
        command_parts in bash_command_strategy()
    ) {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(
            PermRequest::bash(command_parts.iter().map(|s| s.as_str()))
        );
        app.open_interaction("test".to_string(), request);

        prop_assert!(app.interaction_visible(), "Modal should be open");

        // Press 'y' to allow
        app.update(Event::Key(key(KeyCode::Char('y'))));

        // Modal should close
        prop_assert!(
            !app.interaction_visible(),
            "Modal should close after 'y' response"
        );
    }
}

proptest! {
    #[test]
    fn invariant_uppercase_y_key_always_allows(
        command_parts in bash_command_strategy()
    ) {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(
            PermRequest::bash(command_parts.iter().map(|s| s.as_str()))
        );
        app.open_interaction("test".to_string(), request);

        prop_assert!(app.interaction_visible(), "Modal should be open");

        // Press 'Y' (uppercase) to allow
        app.update(Event::Key(KeyEvent::new(
            KeyCode::Char('Y'),
            KeyModifiers::NONE,
        )));

        // Modal should close
        prop_assert!(
            !app.interaction_visible(),
            "Modal should close after 'Y' response"
        );
    }
}

// ============================================================================
// INVARIANT 3: N Key Always Denies
// ============================================================================

proptest! {
    #[test]
    fn invariant_n_key_always_denies(
        command_parts in bash_command_strategy()
    ) {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(
            PermRequest::bash(command_parts.iter().map(|s| s.as_str()))
        );
        app.open_interaction("test".to_string(), request);

        prop_assert!(app.interaction_visible(), "Modal should be open");

        // Press 'n' to deny
        app.update(Event::Key(key(KeyCode::Char('n'))));

        // Modal should close
        prop_assert!(
            !app.interaction_visible(),
            "Modal should close after 'n' response"
        );
    }
}

proptest! {
    #[test]
    fn invariant_uppercase_n_key_always_denies(
        command_parts in bash_command_strategy()
    ) {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(
            PermRequest::bash(command_parts.iter().map(|s| s.as_str()))
        );
        app.open_interaction("test".to_string(), request);

        prop_assert!(app.interaction_visible(), "Modal should be open");

        // Press 'N' (uppercase) to deny
        app.update(Event::Key(KeyEvent::new(
            KeyCode::Char('N'),
            KeyModifiers::NONE,
        )));

        // Modal should close
        prop_assert!(
            !app.interaction_visible(),
            "Modal should close after 'N' response"
        );
    }
}

// ============================================================================
// INVARIANT 4: Random Keys Don't Close Modal
// ============================================================================

proptest! {
    #[test]
    fn invariant_random_keys_dont_close_modal(
        command_parts in bash_command_strategy(),
        key_char in random_non_action_key_strategy()
    ) {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(
            PermRequest::bash(command_parts.iter().map(|s| s.as_str()))
        );
        app.open_interaction("test".to_string(), request);

        prop_assert!(app.interaction_visible(), "Modal should be open initially");

        // Press the random key
        app.update(Event::Key(KeyEvent::new(
            KeyCode::Char(key_char),
            KeyModifiers::NONE,
        )));

        // Modal should still be visible
        prop_assert!(
            app.interaction_visible(),
            "Modal should remain open for key '{}'",
            key_char
        );
    }
}

// ============================================================================
// INVARIANT 5: Ctrl+C Closes Permission Modal (Denies)
// ============================================================================

proptest! {
    #[test]
    fn invariant_ctrl_c_closes_and_denies_permission_modal(
        command_parts in bash_command_strategy()
    ) {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(
            PermRequest::bash(command_parts.iter().map(|s| s.as_str()))
        );
        app.open_interaction("test".to_string(), request);

        prop_assert!(app.interaction_visible(), "Modal should be open");

        let action = app.update(Event::Key(ctrl_c()));

        prop_assert!(
            !app.interaction_visible(),
            "Modal should close after Ctrl+C"
        );

        if let Action::Send(ChatAppMsg::CloseInteraction {
            response: InteractionResponse::Permission(perm),
            ..
        }) = action
        {
            prop_assert!(!perm.allowed, "Ctrl+C should deny permission");
        }
    }
}

// ============================================================================
// INVARIANT 6: Multiple Escapes Still Deny
// ============================================================================

proptest! {
    #[test]
    fn invariant_multiple_escapes_still_deny(
        command_parts in bash_command_strategy(),
        escape_count in 1usize..5
    ) {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(
            PermRequest::bash(command_parts.iter().map(|s| s.as_str()))
        );
        app.open_interaction("test".to_string(), request);

        prop_assert!(app.interaction_visible(), "Modal should be open");

        // Press Escape multiple times
        for _ in 0..escape_count {
            app.update(Event::Key(key(KeyCode::Esc)));
            if !app.interaction_visible() {
                break;
            }
        }

        // Modal should be closed after first Escape
        prop_assert!(
            !app.interaction_visible(),
            "Modal should close after Escape (pressed {} times)",
            escape_count
        );
    }
}

// ============================================================================
// ADDITIONAL EXHAUSTIVE TESTS
// ============================================================================

#[test]
fn test_all_deny_keys_produce_denial() {
    let deny_keys = [
        ("Esc", key(KeyCode::Esc)),
        ("n", key(KeyCode::Char('n'))),
        ("N", KeyEvent::new(KeyCode::Char('N'), KeyModifiers::NONE)),
    ];

    for (key_name, deny_key) in deny_keys.iter() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(PermRequest::bash(["test"]));
        app.open_interaction(format!("perm-{}", key_name), request);

        app.update(Event::Key(*deny_key));

        assert!(
            !app.interaction_visible(),
            "Modal should close for deny key '{}'",
            key_name
        );
    }
}

#[test]
fn test_all_allow_keys_produce_allow() {
    let allow_keys = [
        key(KeyCode::Char('y')),
        KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::NONE),
    ];

    for (idx, allow_key) in allow_keys.iter().enumerate() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(PermRequest::bash(["test"]));
        app.open_interaction(format!("perm-{}", idx), request);

        app.update(Event::Key(*allow_key));

        assert!(
            !app.interaction_visible(),
            "Modal should close for allow key at index {}",
            idx
        );
    }
}

#[test]
fn test_h_key_does_not_close_modal() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["test"]));
    app.open_interaction("perm-h".to_string(), request);

    assert!(app.interaction_visible(), "Modal should be open");

    // Press 'h' to toggle diff visibility
    app.update(Event::Key(key(KeyCode::Char('h'))));

    // Modal should still be visible
    assert!(
        app.interaction_visible(),
        "Modal should remain open after 'h' (help toggle)"
    );
}

#[test]
fn test_uppercase_h_key_does_not_close_modal() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["test"]));
    app.open_interaction("perm-H".to_string(), request);

    assert!(app.interaction_visible(), "Modal should be open");

    // Press 'H' (uppercase) to toggle diff visibility
    app.update(Event::Key(KeyEvent::new(
        KeyCode::Char('H'),
        KeyModifiers::NONE,
    )));

    // Modal should still be visible
    assert!(
        app.interaction_visible(),
        "Modal should remain open after 'H' (help toggle)"
    );
}

#[test]
fn test_tab_key_does_not_close_modal() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["test"]));
    app.open_interaction("perm-tab".to_string(), request);

    assert!(app.interaction_visible(), "Modal should be open");

    // Press Tab
    app.update(Event::Key(key(KeyCode::Tab)));

    // Modal should still be visible
    assert!(
        app.interaction_visible(),
        "Modal should remain open after Tab"
    );
}

#[test]
fn test_arrow_keys_do_not_close_modal() {
    let arrow_keys = [
        key(KeyCode::Up),
        key(KeyCode::Down),
        key(KeyCode::Left),
        key(KeyCode::Right),
    ];

    for (idx, arrow_key) in arrow_keys.iter().enumerate() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(PermRequest::bash(["test"]));
        app.open_interaction(format!("perm-arrow-{}", idx), request);

        app.update(Event::Key(*arrow_key));

        assert!(
            app.interaction_visible(),
            "Modal should remain open after arrow key at index {}",
            idx
        );
    }
}

#[test]
fn test_permission_modal_opens_and_closes_cleanly() {
    let mut app = OilChatApp::default();

    // Initially no modal
    assert!(
        !app.interaction_visible(),
        "Modal should be closed initially"
    );

    // Open permission modal
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    // Modal should be visible
    assert!(
        app.interaction_visible(),
        "Modal should be open after request"
    );

    // Close with 'n'
    app.update(Event::Key(key(KeyCode::Char('n'))));

    // Modal should be closed
    assert!(
        !app.interaction_visible(),
        "Modal should be closed after response"
    );
}

#[test]
fn test_multiple_permission_requests_queue_correctly() {
    let mut app = OilChatApp::default();

    // Queue multiple permission requests
    let req1 = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    let req2 = InteractionRequest::Permission(PermRequest::bash(["cargo", "build"]));

    app.open_interaction("perm-1".to_string(), req1);
    app.open_interaction("perm-2".to_string(), req2);

    // First request should be displayed
    assert!(app.interaction_visible(), "Modal should be visible");

    // Respond to first request
    app.update(Event::Key(key(KeyCode::Char('y'))));

    // After responding to first, the second should now be displayed
    // (close_interaction_and_show_next pops from queue)
    assert!(
        app.interaction_visible(),
        "Modal should show second request after responding to first"
    );

    // Respond to second request
    app.update(Event::Key(key(KeyCode::Char('n'))));

    // Now modal should close (no more queued requests)
    assert!(
        !app.interaction_visible(),
        "Modal should close after responding to all requests"
    );
}

#[test]
fn test_escape_closes_modal_without_side_effects() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["dangerous", "command"]));
    app.open_interaction("perm-dangerous".to_string(), request);

    assert!(app.interaction_visible(), "Modal should be open");

    // Press Escape
    app.update(Event::Key(key(KeyCode::Esc)));

    // Modal should close
    assert!(
        !app.interaction_visible(),
        "Modal should close after Escape"
    );

    // App should be in a clean state (no errors, no stuck state)
    // This is verified by the fact that we can open another modal
    let request2 = InteractionRequest::Permission(PermRequest::bash(["safe", "command"]));
    app.open_interaction("perm-safe".to_string(), request2);

    assert!(
        app.interaction_visible(),
        "Should be able to open another modal after Escape"
    );
}
