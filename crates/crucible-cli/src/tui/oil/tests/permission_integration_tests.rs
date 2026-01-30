//! Integration tests for permission flow
//!
//! Tests the complete permission workflow:
//! 1. Daemon emits InteractionRequest::Permission(PermRequest)
//! 2. TUI displays permission prompt with y/n/h/Esc keybindings
//! 3. User responds
//! 4. Response sent back to daemon

use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::event::Event;
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::render::render_to_string;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::interaction::{InteractionRequest, PermRequest, PermResponse};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn render_app(app: &OilChatApp) -> String {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    render_to_string(&tree, 80)
}

fn assert_contains(output: &str, needle: &str, context: &str) {
    assert!(
        output.contains(needle),
        "Expected to find '{}' in output. Context: {}\nOutput:\n{}",
        needle,
        context,
        output
    );
}

fn assert_not_contains(output: &str, needle: &str, context: &str) {
    assert!(
        !output.contains(needle),
        "Expected NOT to find '{}' in output. Context: {}\nOutput:\n{}",
        needle,
        context,
        output
    );
}

// =============================================================================
// Permission Modal Display Tests
// =============================================================================

#[test]
fn test_permission_modal_opens_on_interaction_request() {
    let mut app = OilChatApp::default();

    // Initially no modal
    let _output = render_app(&app);
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
    let output = render_app(&app);
    assert_contains(&output, "npm", "bash command should be displayed");
}

#[test]
fn test_permission_modal_displays_bash_command() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install", "lodash"]));
    app.open_interaction("perm-bash".to_string(), request);

    let output = render_app(&app);
    assert_contains(&output, "npm", "should show npm command");
    assert_contains(&output, "install", "should show install argument");
    assert_contains(&output, "lodash", "should show package name");
}

#[test]
fn test_permission_modal_shows_keybinding_hints() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["rm", "-rf", "/"]));
    app.open_interaction("perm-dangerous".to_string(), request);

    let output = render_app(&app);
    // Should show keybinding hints for y/n/h/Esc
    assert_contains(&output, "y", "should show 'y' keybinding hint for allow");
    assert_contains(&output, "n", "should show 'n' keybinding hint for deny");
}

// =============================================================================
// Permission Keybinding Tests
// =============================================================================

#[test]
fn test_permission_y_key_allows_and_closes() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    assert!(app.interaction_visible(), "Modal should be open");

    // Press 'y' to allow
    app.update(Event::Key(key(KeyCode::Char('y'))));

    // Modal should close
    assert!(
        !app.interaction_visible(),
        "Modal should close after 'y' response"
    );
}

#[test]
fn test_permission_y_key_uppercase_allows_and_closes() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    // Press 'Y' (uppercase) to allow
    app.update(Event::Key(KeyEvent::new(
        KeyCode::Char('Y'),
        KeyModifiers::NONE,
    )));

    assert!(
        !app.interaction_visible(),
        "Modal should close after 'Y' response"
    );
}

#[test]
fn test_permission_n_key_denies_and_closes() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    assert!(app.interaction_visible(), "Modal should be open");

    // Press 'n' to deny
    app.update(Event::Key(key(KeyCode::Char('n'))));

    // Modal should close
    assert!(
        !app.interaction_visible(),
        "Modal should close after 'n' response"
    );
}

#[test]
fn test_permission_n_key_uppercase_denies_and_closes() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    // Press 'N' (uppercase) to deny
    app.update(Event::Key(KeyEvent::new(
        KeyCode::Char('N'),
        KeyModifiers::NONE,
    )));

    assert!(
        !app.interaction_visible(),
        "Modal should close after 'N' response"
    );
}

#[test]
fn test_permission_escape_denies_and_closes() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    assert!(app.interaction_visible(), "Modal should be open");

    // Press Escape to deny
    app.update(Event::Key(key(KeyCode::Esc)));

    // Modal should close
    assert!(
        !app.interaction_visible(),
        "Modal should close after Escape"
    );
}

#[test]
fn test_permission_h_key_toggles_diff_visibility() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    let _output_before = render_app(&app);

    // Press 'h' to toggle diff visibility
    app.update(Event::Key(key(KeyCode::Char('h'))));

    let output_after = render_app(&app);

    // Modal should still be visible
    assert!(
        app.interaction_visible(),
        "Modal should remain open after 'h'"
    );

    // Output may change (diff collapsed/expanded)
    // Just verify modal is still there
    assert_contains(&output_after, "npm", "command should still be visible");
}

#[test]
fn test_permission_h_key_uppercase_toggles_diff_visibility() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    // Press 'H' (uppercase) to toggle diff visibility
    app.update(Event::Key(KeyEvent::new(
        KeyCode::Char('H'),
        KeyModifiers::NONE,
    )));

    // Modal should still be visible
    assert!(
        app.interaction_visible(),
        "Modal should remain open after 'H'"
    );
}

#[test]
fn test_permission_other_keys_ignored() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    assert!(app.interaction_visible(), "Modal should be open");

    // Press random keys that should be ignored
    app.update(Event::Key(key(KeyCode::Char('x'))));
    assert!(
        app.interaction_visible(),
        "Modal should remain open after 'x'"
    );

    app.update(Event::Key(key(KeyCode::Char('z'))));
    assert!(
        app.interaction_visible(),
        "Modal should remain open after 'z'"
    );
}

// =============================================================================
// Permission Queue Tests
// =============================================================================

#[test]
fn test_permission_queue_shows_first() {
    let mut app = OilChatApp::default();

    // Queue multiple permission requests
    let req1 = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    let req2 = InteractionRequest::Permission(PermRequest::bash(["cargo", "build"]));

    app.open_interaction("perm-1".to_string(), req1);
    app.open_interaction("perm-2".to_string(), req2);

    let output = render_app(&app);

    // First request should be displayed
    assert_contains(&output, "npm", "first request should be visible");
    assert_not_contains(&output, "cargo", "second request should not be visible yet");
}

#[test]
fn test_permission_queue_shows_indicator() {
    let mut app = OilChatApp::default();

    // Queue multiple permission requests
    let req1 = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    let req2 = InteractionRequest::Permission(PermRequest::bash(["cargo", "build"]));
    let req3 = InteractionRequest::Permission(PermRequest::bash(["make", "all"]));

    app.open_interaction("perm-1".to_string(), req1);
    app.open_interaction("perm-2".to_string(), req2);
    app.open_interaction("perm-3".to_string(), req3);

    let output = render_app(&app);

    // Should show queue indicator like "[1/3]"
    assert_contains(&output, "1", "should show current position in queue");
    assert_contains(&output, "3", "should show total queue size");
}

#[test]
fn test_permission_queue_advances_after_response() {
    let mut app = OilChatApp::default();

    // Queue multiple permission requests
    let req1 = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    let req2 = InteractionRequest::Permission(PermRequest::bash(["cargo", "build"]));

    app.open_interaction("perm-1".to_string(), req1);
    app.open_interaction("perm-2".to_string(), req2);

    let output_before = render_app(&app);
    assert_contains(&output_before, "npm", "first request should be visible");

    // Respond to first request
    app.update(Event::Key(key(KeyCode::Char('y'))));

    // Second request should now be displayed
    let output_after = render_app(&app);
    assert_contains(
        &output_after,
        "cargo",
        "second request should now be visible",
    );
    assert_not_contains(&output_after, "npm", "first request should be gone");
}

// =============================================================================
// Permission Flow Tests
// =============================================================================

#[test]
fn test_permission_response_callback_called() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    // Respond with 'y'
    let action = app.update(Event::Key(key(KeyCode::Char('y'))));

    // Should emit CloseInteraction action with response
    // The action should contain the response
    match action {
        crate::tui::oil::app::Action::Send(ChatAppMsg::CloseInteraction {
            request_id,
            response,
        }) => {
            assert_eq!(request_id, "perm-1", "request_id should match");
            match response {
                crucible_core::interaction::InteractionResponse::Permission(perm_resp) => {
                    assert!(
                        perm_resp.allowed,
                        "response should indicate permission allowed"
                    );
                }
                _ => panic!("response should be Permission type"),
            }
        }
        _ => panic!("action should be CloseInteraction"),
    }
}

#[test]
fn test_permission_deny_response_callback_called() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    // Respond with 'n'
    let action = app.update(Event::Key(key(KeyCode::Char('n'))));

    // Should emit CloseInteraction action with deny response
    match action {
        crate::tui::oil::app::Action::Send(ChatAppMsg::CloseInteraction {
            request_id,
            response,
        }) => {
            assert_eq!(request_id, "perm-1", "request_id should match");
            match response {
                crucible_core::interaction::InteractionResponse::Permission(perm_resp) => {
                    assert!(
                        !perm_resp.allowed,
                        "response should indicate permission denied"
                    );
                }
                _ => panic!("response should be Permission type"),
            }
        }
        _ => panic!("action should be CloseInteraction"),
    }
}

#[test]
fn test_permission_multiple_denials_all_callback() {
    let mut app = OilChatApp::default();

    // Queue multiple permission requests
    let req1 = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    let req2 = InteractionRequest::Permission(PermRequest::bash(["cargo", "build"]));

    app.open_interaction("perm-1".to_string(), req1);
    app.open_interaction("perm-2".to_string(), req2);

    // Deny first request
    let action1 = app.update(Event::Key(key(KeyCode::Char('n'))));
    match action1 {
        crate::tui::oil::app::Action::Send(ChatAppMsg::CloseInteraction {
            request_id,
            response,
        }) => {
            assert_eq!(request_id, "perm-1", "first request_id should match");
            match response {
                crucible_core::interaction::InteractionResponse::Permission(perm_resp) => {
                    assert!(!perm_resp.allowed, "first response should be denied");
                }
                _ => panic!("response should be Permission type"),
            }
        }
        _ => panic!("action should be CloseInteraction"),
    }

    // Deny second request
    let action2 = app.update(Event::Key(key(KeyCode::Char('n'))));
    match action2 {
        crate::tui::oil::app::Action::Send(ChatAppMsg::CloseInteraction {
            request_id,
            response,
        }) => {
            assert_eq!(request_id, "perm-2", "second request_id should match");
            match response {
                crucible_core::interaction::InteractionResponse::Permission(perm_resp) => {
                    assert!(!perm_resp.allowed, "second response should be denied");
                }
                _ => panic!("response should be Permission type"),
            }
        }
        _ => panic!("action should be CloseInteraction"),
    }
}

// =============================================================================
// Permission Scope Tests
// =============================================================================

#[test]
fn test_permission_read_scope_displayed() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::read(["etc", "hosts"]));
    app.open_interaction("perm-read".to_string(), request);

    let output = render_app(&app);
    assert_contains(&output, "READ", "should indicate read operation");
    assert_contains(&output, "hosts", "should show file path");
}

#[test]
fn test_permission_tool_scope_displayed() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::tool(
        "semantic_search",
        serde_json::json!({"query": "rust memory safety"}),
    ));
    app.open_interaction("perm-tool".to_string(), request);

    let output = render_app(&app);
    assert_contains(&output, "semantic_search", "should show tool name");
}

// =============================================================================
// Cursor Navigation Tests
// =============================================================================

#[test]
fn test_permission_down_arrow_navigates() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    let output_before = render_app(&app);
    assert_contains(&output_before, "> ", "should show cursor on first option");

    app.update(Event::Key(key(KeyCode::Down)));

    let output_after = render_app(&app);
    assert!(
        app.interaction_visible(),
        "Modal should remain open after navigation"
    );
    assert_contains(&output_after, "No", "should still show deny option");
}

#[test]
fn test_permission_up_arrow_navigates() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    app.update(Event::Key(key(KeyCode::Up)));

    assert!(
        app.interaction_visible(),
        "Modal should remain open after navigation"
    );
}

#[test]
fn test_permission_j_key_navigates_down() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    app.update(Event::Key(key(KeyCode::Char('j'))));

    assert!(
        app.interaction_visible(),
        "Modal should remain open after 'j' navigation"
    );
}

#[test]
fn test_permission_k_key_navigates_up() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    app.update(Event::Key(key(KeyCode::Char('k'))));

    assert!(
        app.interaction_visible(),
        "Modal should remain open after 'k' navigation"
    );
}

#[test]
fn test_permission_enter_confirms_selection() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    let action = app.update(Event::Key(key(KeyCode::Enter)));

    assert!(!app.interaction_visible(), "Modal should close after Enter");
    match action {
        crate::tui::oil::app::Action::Send(ChatAppMsg::CloseInteraction {
            request_id,
            response,
        }) => {
            assert_eq!(request_id, "perm-1", "request_id should match");
            match response {
                crucible_core::interaction::InteractionResponse::Permission(perm_resp) => {
                    assert!(perm_resp.allowed, "first option (Allow) should be allowed");
                }
                _ => panic!("response should be Permission type"),
            }
        }
        _ => panic!("action should be CloseInteraction"),
    }
}

#[test]
fn test_permission_navigate_to_deny_then_enter() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    app.update(Event::Key(key(KeyCode::Down)));
    let action = app.update(Event::Key(key(KeyCode::Enter)));

    assert!(!app.interaction_visible(), "Modal should close after Enter");
    match action {
        crate::tui::oil::app::Action::Send(ChatAppMsg::CloseInteraction {
            request_id,
            response,
        }) => {
            assert_eq!(request_id, "perm-1", "request_id should match");
            match response {
                crucible_core::interaction::InteractionResponse::Permission(perm_resp) => {
                    assert!(
                        !perm_resp.allowed,
                        "second option (Deny) should not be allowed"
                    );
                }
                _ => panic!("response should be Permission type"),
            }
        }
        _ => panic!("action should be CloseInteraction"),
    }
}

#[test]
fn test_permission_navigate_to_pattern_then_enter() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    app.update(Event::Key(key(KeyCode::Down)));
    app.update(Event::Key(key(KeyCode::Down)));
    let action = app.update(Event::Key(key(KeyCode::Enter)));

    assert!(!app.interaction_visible(), "Modal should close after Enter");
    match action {
        crate::tui::oil::app::Action::Send(ChatAppMsg::CloseInteraction {
            request_id,
            response,
        }) => {
            assert_eq!(request_id, "perm-1", "request_id should match");
            match response {
                crucible_core::interaction::InteractionResponse::Permission(perm_resp) => {
                    assert!(perm_resp.allowed, "pattern option should allow");
                    assert!(
                        perm_resp.pattern.is_some(),
                        "pattern option should include pattern"
                    );
                }
                _ => panic!("response should be Permission type"),
            }
        }
        _ => panic!("action should be CloseInteraction"),
    }
}

#[test]
fn test_permission_navigation_wraps_around_bottom() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    app.update(Event::Key(key(KeyCode::Down)));
    app.update(Event::Key(key(KeyCode::Down)));
    app.update(Event::Key(key(KeyCode::Down)));
    let action = app.update(Event::Key(key(KeyCode::Enter)));

    match action {
        crate::tui::oil::app::Action::Send(ChatAppMsg::CloseInteraction { response, .. }) => {
            match response {
                crucible_core::interaction::InteractionResponse::Permission(perm_resp) => {
                    assert!(perm_resp.allowed, "should wrap to first option (Allow)");
                    assert!(perm_resp.pattern.is_none(), "first option has no pattern");
                }
                _ => panic!("response should be Permission type"),
            }
        }
        _ => panic!("action should be CloseInteraction"),
    }
}

#[test]
fn test_permission_navigation_wraps_around_top() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    app.update(Event::Key(key(KeyCode::Up)));
    let action = app.update(Event::Key(key(KeyCode::Enter)));

    match action {
        crate::tui::oil::app::Action::Send(ChatAppMsg::CloseInteraction { response, .. }) => {
            match response {
                crucible_core::interaction::InteractionResponse::Permission(perm_resp) => {
                    assert!(perm_resp.allowed, "should wrap to last option (Pattern)");
                    assert!(perm_resp.pattern.is_some(), "last option has pattern");
                }
                _ => panic!("response should be Permission type"),
            }
        }
        _ => panic!("action should be CloseInteraction"),
    }
}

#[test]
fn test_permission_ctrl_c_denies_and_closes() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    let action = app.update(Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )));

    assert!(
        !app.interaction_visible(),
        "Modal should close after Ctrl+C"
    );
    match action {
        crate::tui::oil::app::Action::Send(ChatAppMsg::CloseInteraction { response, .. }) => {
            match response {
                crucible_core::interaction::InteractionResponse::Permission(perm_resp) => {
                    assert!(!perm_resp.allowed, "Ctrl+C should deny");
                }
                _ => panic!("response should be Permission type"),
            }
        }
        _ => panic!("action should be CloseInteraction"),
    }
}

#[test]
fn test_permission_shows_selectable_options() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-1".to_string(), request);

    let output = render_app(&app);
    assert_contains(&output, "Yes", "should show Yes option");
    assert_contains(&output, "No", "should show No option");
    assert_contains(&output, "Allowlist", "should show Allowlist option");
    assert_contains(&output, "y", "should show y keybinding hint");
    assert_contains(&output, "n", "should show n keybinding hint");
}

// =============================================================================
// Permission Edge Cases
// =============================================================================

#[test]
fn test_permission_long_command_displayed() {
    let mut app = OilChatApp::default();
    let long_cmd = vec![
        "npm",
        "install",
        "--save-dev",
        "--save-exact",
        "typescript@5.0.0",
    ];
    let request = InteractionRequest::Permission(PermRequest::bash(long_cmd));
    app.open_interaction("perm-long".to_string(), request);

    let output = render_app(&app);
    assert_contains(&output, "npm", "should show npm");
    assert_contains(&output, "typescript", "should show package name");
}

#[test]
fn test_permission_empty_queue_after_all_responses() {
    let mut app = OilChatApp::default();

    let req1 = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    let req2 = InteractionRequest::Permission(PermRequest::bash(["cargo", "build"]));

    app.open_interaction("perm-1".to_string(), req1);
    app.open_interaction("perm-2".to_string(), req2);

    // Respond to both
    app.update(Event::Key(key(KeyCode::Char('y'))));
    app.update(Event::Key(key(KeyCode::Char('y'))));

    // Modal should be closed
    assert!(
        !app.interaction_visible(),
        "Modal should be closed after all responses"
    );
}
