use super::*;
use crate::tui::oil::chat_container::ChatContainer;
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::render::render_to_string;
use crucible_core::traits::chat::PrecognitionNoteInfo;

#[test]
fn test_mode_cycle() {
    assert_eq!(ChatMode::Normal.cycle(), ChatMode::Plan);
    assert_eq!(ChatMode::Plan.cycle(), ChatMode::Auto);
    assert_eq!(ChatMode::Auto.cycle(), ChatMode::Normal);
}

#[test]
fn test_mode_from_str() {
    assert_eq!(ChatMode::parse("normal"), ChatMode::Normal);
    assert_eq!(ChatMode::parse("default"), ChatMode::Normal);
    assert_eq!(ChatMode::parse("plan"), ChatMode::Plan);
    assert_eq!(ChatMode::parse("auto"), ChatMode::Auto);
    assert_eq!(ChatMode::parse("unknown"), ChatMode::Normal);
}

#[test]
fn test_app_init() {
    let app = OilChatApp::init();
    assert!(app.container_list().is_empty());
    assert!(!app.is_streaming());
    assert_eq!(app.mode, ChatMode::Normal);
}

#[test]
fn test_user_message() {
    let mut app = OilChatApp::init();
    app.add_user_message("Hello".to_string());

    assert_eq!(app.container_list().len(), 1);
    if let ChatContainer::UserMessage { content, .. } = &app.container_list().all_containers()[0] {
        assert_eq!(content, "Hello");
    } else {
        panic!("Expected UserMessage");
    }
}

#[test]
fn test_streaming_flow() {
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::TextDelta("Hello ".to_string()));
    assert!(app.is_streaming());

    app.on_message(ChatAppMsg::TextDelta("World".to_string()));
    assert!(app.is_streaming());

    app.on_message(ChatAppMsg::StreamComplete);
    assert!(!app.is_streaming());

    // Verify content via container list
    let containers = app.container_list().all_containers();
    assert_eq!(containers.len(), 1);
    if let ChatContainer::AssistantResponse { blocks, .. } = &containers[0] {
        let combined = blocks.join("");
        assert_eq!(combined, "Hello World");
    } else {
        panic!("Expected AssistantResponse");
    }
}

#[test]
fn test_tool_call_flow() {
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::ToolCall {
        name: "Read".to_string(),
        args: r#"{"path":"file.md","offset":10}"#.to_string(),
        call_id: None,
        description: None,
        source: None,
    });
    let tool = app.container_list().find_tool("Read").unwrap();
    assert_eq!(tool.name.as_ref(), "Read");
    assert!(!tool.complete);

    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "Read".to_string(),
        delta: "line 1\n".to_string(),
        call_id: None,
    });
    let tool = app.container_list().find_tool("Read").unwrap();
    assert_eq!(tool.result(), "line 1");

    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "Read".to_string(),
        delta: "line 2\n".to_string(),
        call_id: None,
    });
    let tool = app.container_list().find_tool("Read").unwrap();
    assert_eq!(tool.result(), "line 1\nline 2");

    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "Read".to_string(),
        call_id: None,
    });
    let tool = app.container_list().find_tool("Read").unwrap();
    assert!(tool.complete);
}

#[test]
fn test_slash_commands() {
    let mut app = OilChatApp::init();

    assert_eq!(app.mode, ChatMode::Normal);
    app.handle_slash_command("/mode");
    assert_eq!(app.mode, ChatMode::Plan);

    app.handle_slash_command("/normal");
    assert_eq!(app.mode, ChatMode::Normal);
}

#[test]
fn test_clear_repl_command() {
    let mut app = OilChatApp::init();

    app.add_user_message("test".to_string());
    assert_eq!(app.container_list().len(), 1);

    let action = app.handle_repl_command(":clear");
    // reset_session() is now called by the runner after confirming non-ACP
    assert_eq!(app.container_list().len(), 1);
    assert!(matches!(action, Action::Send(ChatAppMsg::ClearHistory)));
}

#[test]
fn test_messages_command_toggles_notification_area() {
    let mut app = OilChatApp::init();
    assert!(!app.notification_area.is_visible());

    app.handle_repl_command(":messages");
    assert!(app.notification_area.is_visible());

    app.handle_repl_command(":messages");
    assert!(!app.notification_area.is_visible());

    app.handle_repl_command(":msgs");
    assert!(app.notification_area.is_visible());
}

#[test]
fn test_toggle_messages_msg() {
    let mut app = OilChatApp::init();
    assert!(!app.notification_area.is_visible());

    app.on_message(ChatAppMsg::ToggleMessages);
    assert!(app.notification_area.is_visible());

    app.on_message(ChatAppMsg::ToggleMessages);
    assert!(!app.notification_area.is_visible());
}

#[test]
fn test_quit_command() {
    let mut app = OilChatApp::init();
    let action = app.handle_repl_command(":quit");
    assert!(action.is_quit());
}

#[test]
fn test_autocomplete_messages_command() {
    let mut app = OilChatApp::init();
    app.popup.kind = AutocompleteKind::ReplCommand;
    app.popup.filter = "mes".to_string();
    let items = app.get_popup_items();
    assert!(items.iter().any(|item| item.label == ":messages"));
}

#[test]
fn test_autocomplete_reload_command() {
    let mut app = OilChatApp::init();
    app.popup.kind = AutocompleteKind::ReplCommand;
    app.popup.filter = "rel".to_string();
    let items = app.get_popup_items();
    assert!(items.iter().any(|item| item.label == ":reload"));
}

#[test]
fn test_view_renders() {
    use crate::tui::oil::focus::FocusContext;

    let mut app = OilChatApp::init();
    app.add_user_message("Hello".to_string());
    app.on_message(ChatAppMsg::TextDelta("Hi there".to_string()));

    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let _node = app.view(&ctx);
}

#[test]
fn test_tool_call_renders_with_result() {
    use crate::tui::oil::focus::FocusContext;
    use crate::tui::oil::render::render_to_string;

    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"README.md","offset":1,"limit":200}"#.to_string(),
        call_id: None,
        description: None,
        source: None,
    });

    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let node = app.view(&ctx);
    let output = render_to_string(&node, 80);

    assert!(output.contains("Read File"), "should show tool name");
    assert!(output.contains("path="), "should show args");

    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "# README\nThis is the content.".to_string(),
        call_id: None,
    });

    let node = app.view(&ctx);
    let output = render_to_string(&node, 80);
    assert!(
        output.contains("README") || output.contains("content"),
        "should show streaming output while running"
    );

    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: None,
    });

    let node = app.view(&ctx);
    let output = render_to_string(&node, 80);
    assert!(output.contains("✓"), "should show checkmark when complete");
    assert!(
        output.contains("2 lines"),
        "should show line count for read_file when complete"
    );
}

#[test]
fn test_context_usage_updates() {
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::ContextUsage {
        used: 64000,
        total: 128000,
    });

    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let output = render_to_string(&tree, 80);

    assert!(output.contains("50%"), "Should show 50% context usage");
}

#[test]
fn test_context_display_unknown_total() {
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::ContextUsage {
        used: 5000,
        total: 0,
    });

    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("5k tok"),
        "Should show token count when total is unknown: {}",
        output
    );
    assert!(
        !output.contains("%"),
        "Should not show percentage when total is unknown"
    );
}

#[test]
fn test_context_display_no_usage_shows_placeholder() {
    let app = OilChatApp::init();

    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("— ctx"),
        "Should show placeholder when no context data: {}",
        output
    );
}

#[test]
fn test_context_percentage_calculation() {
    let test_cases: Vec<(usize, usize, &str)> = vec![
        (0, 100000, "0%"),
        (50000, 100000, "50%"),
        (100000, 100000, "100%"),
        (1000, 100000, "1%"),
        (99999, 100000, "100%"),
        (33333, 100000, "33%"),
        (66666, 100000, "67%"),
    ];

    for (used, total, expected_pct) in test_cases {
        let mut app = OilChatApp::init();
        app.on_message(ChatAppMsg::ContextUsage { used, total });

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        assert!(
            output.contains(expected_pct),
            "For used={}, total={}: expected '{}' in output: {}",
            used,
            total,
            expected_pct,
            output
        );
        assert!(
            output.contains("ctx"),
            "For used={}, total={}: should contain 'ctx'",
            used,
            total
        );
    }
}

#[test]
fn test_status_shows_mode_indicator() {
    let mut app = OilChatApp::init();
    app.set_mode(ChatMode::Plan);

    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let output = render_to_string(&tree, 80);

    assert!(output.contains("PLAN"), "Status should show PLAN mode");
}

#[test]
fn test_error_message_clears_streaming() {
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::TextDelta("partial response".to_string()));
    assert!(app.is_streaming());

    app.on_message(ChatAppMsg::Error("Connection lost".to_string()));
    assert!(!app.is_streaming(), "Error should stop streaming");
}

#[test]
fn test_ctrl_t_toggles_thinking_during_streaming() {
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::TextDelta("streaming...".to_string()));
    assert!(app.is_streaming());

    let initial_show_thinking = app.show_thinking;

    let ctrl_t = crossterm::event::KeyEvent::new(
        KeyCode::Char('t'),
        crossterm::event::KeyModifiers::CONTROL,
    );
    let action = app.handle_key(ctrl_t);

    assert!(
        matches!(action, Action::Continue),
        "Ctrl+T should return Continue, not cancel stream"
    );
    assert!(app.is_streaming(), "Stream should still be active");
    assert_ne!(
        app.show_thinking, initial_show_thinking,
        "Ctrl+T should toggle show_thinking"
    );
    assert!(
        !app.notification_area.is_empty(),
        "Notification should be added to store"
    );
}

#[test]
fn test_shell_history_ring_buffer_evicts_oldest() {
    let mut app = OilChatApp::init();

    for i in 0..(MAX_SHELL_HISTORY + 5) {
        app.push_shell_history(format!("cmd {}", i));
    }

    assert_eq!(app.shell_history.shell_history.len(), MAX_SHELL_HISTORY);
    assert_eq!(app.shell_history.shell_history.front().unwrap(), "cmd 5");
    assert_eq!(
        app.shell_history.shell_history.back().unwrap(),
        &format!("cmd {}", MAX_SHELL_HISTORY + 4)
    );
}

#[test]
fn test_interaction_modal_open_close_cycle() {
    use crucible_core::interaction::AskRequest;

    let mut app = OilChatApp::init();
    assert!(!app.interaction_visible());

    let request = InteractionRequest::Ask(AskRequest::new("Choose an option"));
    app.open_interaction("req-123".to_string(), request);

    assert!(app.interaction_visible());
    let modal = app.interaction_modal.as_ref().unwrap();
    assert_eq!(modal.request_id, "req-123");
    assert_eq!(modal.selected, 0);
    assert!(modal.filter.is_empty());
    assert!(modal.other_text.is_empty());
    assert_eq!(modal.mode, InteractionMode::Selecting);

    app.close_interaction();
    assert!(!app.interaction_visible());
    assert!(app.interaction_modal.is_none());
}

#[test]
fn test_interaction_modal_replaces_previous() {
    use crucible_core::interaction::AskRequest;

    let mut app = OilChatApp::init();

    let request1 = InteractionRequest::Ask(AskRequest::new("First question"));
    app.open_interaction("req-1".to_string(), request1);
    assert_eq!(app.interaction_modal.as_ref().unwrap().request_id, "req-1");

    let request2 = InteractionRequest::Ask(AskRequest::new("Second question"));
    app.open_interaction("req-2".to_string(), request2);
    assert_eq!(app.interaction_modal.as_ref().unwrap().request_id, "req-2");
    assert!(app.interaction_visible());
}

#[test]
fn test_interaction_modal_close_when_none_is_noop() {
    let mut app = OilChatApp::init();
    assert!(!app.interaction_visible());

    app.close_interaction();
    assert!(!app.interaction_visible());
}

#[test]
fn test_perm_request_bash_renders() {
    use crucible_core::interaction::PermRequest;

    let mut app = OilChatApp::init();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install", "lodash"]));
    app.open_interaction("perm-1".to_string(), request);

    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("PERMISSION"),
        "Should show PERMISSION badge"
    );
    assert!(output.contains("BASH"), "Should show BASH type label");
    assert!(
        output.contains("npm install lodash"),
        "Should show command tokens"
    );
    assert!(output.contains("y"), "Should show allow key");
    assert!(output.contains("n"), "Should show deny key");
    assert!(output.contains("Esc"), "Should show cancel key");
}

#[test]
fn test_perm_request_write_renders() {
    use crucible_core::interaction::PermRequest;

    let mut app = OilChatApp::init();
    let request = InteractionRequest::Permission(PermRequest::write([
        "home", "user", "project", "src", "main.rs",
    ]));
    app.open_interaction("perm-2".to_string(), request);

    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let output = render_to_string(&tree, 80);

    assert!(output.contains("WRITE"), "Should show WRITE type label");
    assert!(
        output.contains("/home/user/project/src/main.rs"),
        "Should show path segments"
    );
}

#[test]
fn test_perm_request_y_allows() {
    use crossterm::event::{KeyEvent, KeyModifiers};
    use crucible_core::interaction::PermRequest;

    let mut app = OilChatApp::init();
    let request = InteractionRequest::Permission(PermRequest::bash(["ls", "-la"]));
    app.open_interaction("perm-3".to_string(), request);

    let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    let action = app.handle_key(key);

    assert!(!app.interaction_visible(), "Modal should close after y");
    match action {
        Action::Send(ChatAppMsg::CloseInteraction { response, .. }) => match response {
            InteractionResponse::Permission(perm) => {
                assert!(perm.allowed, "Should be allowed");
            }
            _ => panic!("Expected Permission response"),
        },
        _ => panic!("Expected CloseInteraction action"),
    }
}

#[test]
fn test_perm_request_n_denies() {
    use crossterm::event::{KeyEvent, KeyModifiers};
    use crucible_core::interaction::PermRequest;

    let mut app = OilChatApp::init();
    let request = InteractionRequest::Permission(PermRequest::bash(["rm", "-rf", "/"]));
    app.open_interaction("perm-4".to_string(), request);

    let key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
    let action = app.handle_key(key);

    assert!(!app.interaction_visible(), "Modal should close after n");
    match action {
        Action::Send(ChatAppMsg::CloseInteraction { response, .. }) => match response {
            InteractionResponse::Permission(perm) => {
                assert!(!perm.allowed, "Should be denied");
            }
            _ => panic!("Expected Permission response"),
        },
        _ => panic!("Expected CloseInteraction action"),
    }
}

#[test]
fn test_perm_request_escape_denies() {
    use crossterm::event::{KeyEvent, KeyModifiers};
    use crucible_core::interaction::PermRequest;

    let mut app = OilChatApp::init();
    let request = InteractionRequest::Permission(PermRequest::read(["etc", "passwd"]));
    app.open_interaction("perm-5".to_string(), request);

    let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let action = app.handle_key(key);

    assert!(
        !app.interaction_visible(),
        "Modal should close after Escape"
    );
    match action {
        Action::Send(ChatAppMsg::CloseInteraction { response, .. }) => match response {
            InteractionResponse::Permission(perm) => {
                assert!(!perm.allowed, "Escape should deny permission");
            }
            _ => panic!("Expected Permission response"),
        },
        _ => panic!("Expected CloseInteraction action"),
    }
}

#[test]
fn test_perm_request_h_toggles_diff_collapsed() {
    use crossterm::event::{KeyEvent, KeyModifiers};
    use crucible_core::interaction::PermRequest;

    let mut app = OilChatApp::init();
    let request = InteractionRequest::Permission(PermRequest::write(["home", "user", "file.txt"]));
    app.open_interaction("perm-6".to_string(), request);

    assert!(app.interaction_visible(), "Modal should be visible");

    let key = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
    let action = app.handle_key(key);

    assert!(
        app.interaction_visible(),
        "Modal should remain visible after h"
    );
    assert!(
        matches!(action, Action::Continue),
        "h should return Continue, not close modal"
    );
}

#[test]
fn test_perm_request_a_saves_pattern_and_allows() {
    use crossterm::event::{KeyEvent, KeyModifiers};
    use crucible_core::interaction::PermRequest;

    let mut app = OilChatApp::init();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install"]));
    app.open_interaction("perm-7".to_string(), request);

    let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    let action = app.handle_key(key);

    assert!(
        !app.interaction_visible(),
        "Modal should close after a (pattern saved)"
    );
    match action {
        Action::Send(ChatAppMsg::CloseInteraction { response, .. }) => match response {
            InteractionResponse::Permission(perm) => {
                assert!(perm.allowed, "a should allow");
                assert!(perm.pattern.is_some(), "a should set a pattern");
                assert_eq!(
                    perm.pattern.as_deref(),
                    Some("npm *"),
                    "pattern should match suggested pattern"
                );
            }
            _ => panic!("Expected Permission response"),
        },
        _ => panic!("Expected Send(CloseInteraction) action"),
    }
}

#[test]
fn test_perm_request_other_keys_ignored() {
    use crossterm::event::{KeyEvent, KeyModifiers};
    use crucible_core::interaction::PermRequest;

    let mut app = OilChatApp::init();
    let request = InteractionRequest::Permission(PermRequest::bash(["ls", "-la"]));
    app.open_interaction("perm-8".to_string(), request);

    for c in ['b', 'x', 'z', '1', '!'] {
        let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        let action = app.handle_key(key);

        assert!(
            app.interaction_visible(),
            "Modal should remain visible after '{}'",
            c
        );
        assert!(
            matches!(action, Action::Continue),
            "'{}' should be ignored and return Continue",
            c
        );
    }
}

#[test]
fn test_perm_queue_second_request_queued_when_first_pending() {
    use crucible_core::interaction::PermRequest;

    let mut app = OilChatApp::init();

    let request1 = InteractionRequest::Permission(PermRequest::bash(["ls"]));
    app.open_interaction("perm-1".to_string(), request1);
    assert!(app.interaction_visible());
    assert_eq!(app.permission.permission_queue.len(), 0);

    let request2 = InteractionRequest::Permission(PermRequest::bash(["cat", "file.txt"]));
    app.open_interaction("perm-2".to_string(), request2);

    assert!(app.interaction_visible());
    assert_eq!(app.interaction_modal.as_ref().unwrap().request_id, "perm-1");
    assert_eq!(app.permission.permission_queue.len(), 1);
}

#[test]
fn test_perm_queue_shows_next_after_response() {
    use crossterm::event::{KeyEvent, KeyModifiers};
    use crucible_core::interaction::PermRequest;

    let mut app = OilChatApp::init();

    let request1 = InteractionRequest::Permission(PermRequest::bash(["ls"]));
    app.open_interaction("perm-1".to_string(), request1);

    let request2 = InteractionRequest::Permission(PermRequest::bash(["cat"]));
    app.open_interaction("perm-2".to_string(), request2);

    let request3 = InteractionRequest::Permission(PermRequest::bash(["rm"]));
    app.open_interaction("perm-3".to_string(), request3);

    assert_eq!(app.permission.permission_queue.len(), 2);

    let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    app.handle_key(key);

    assert!(app.interaction_visible());
    assert_eq!(app.interaction_modal.as_ref().unwrap().request_id, "perm-2");
    assert_eq!(app.permission.permission_queue.len(), 1);

    let key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
    app.handle_key(key);

    assert!(app.interaction_visible());
    assert_eq!(app.interaction_modal.as_ref().unwrap().request_id, "perm-3");
    assert_eq!(app.permission.permission_queue.len(), 0);

    let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    app.handle_key(key);

    assert!(!app.interaction_visible());
    assert_eq!(app.permission.permission_queue.len(), 0);
}

#[test]
fn test_perm_queue_indicator_shows_in_header() {
    use crucible_core::interaction::PermRequest;

    let mut app = OilChatApp::init();

    let request1 = InteractionRequest::Permission(PermRequest::bash(["ls"]));
    app.open_interaction("perm-1".to_string(), request1);

    let request2 = InteractionRequest::Permission(PermRequest::bash(["cat"]));
    app.open_interaction("perm-2".to_string(), request2);

    let request3 = InteractionRequest::Permission(PermRequest::bash(["rm"]));
    app.open_interaction("perm-3".to_string(), request3);

    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("[1/3]"),
        "Should show queue indicator [1/3], got: {}",
        output
    );
}

#[test]
fn test_perm_queue_no_indicator_for_single_request() {
    use crucible_core::interaction::PermRequest;

    let mut app = OilChatApp::init();

    let request = InteractionRequest::Permission(PermRequest::bash(["ls"]));
    app.open_interaction("perm-1".to_string(), request);

    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let output = render_to_string(&tree, 80);

    assert!(
        !output.contains("[1/1]"),
        "Should not show queue indicator for single request"
    );
    assert!(output.contains("BASH"), "Should show BASH type label");
}

#[test]
fn messages_drawer_closes_on_escape() {
    let mut app = OilChatApp::init();
    app.notification_area
        .add(crucible_core::types::Notification::toast("test"));
    app.notification_area.show();
    assert!(app.notification_area.is_visible());

    app.update(Event::Key(crossterm::event::KeyEvent::new(
        KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    )));
    assert!(!app.notification_area.is_visible());
}

#[test]
fn messages_drawer_closes_on_q() {
    let mut app = OilChatApp::init();
    app.notification_area
        .add(crucible_core::types::Notification::toast("test"));
    app.notification_area.show();
    assert!(app.notification_area.is_visible());

    app.update(Event::Key(crossterm::event::KeyEvent::new(
        KeyCode::Char('q'),
        crossterm::event::KeyModifiers::NONE,
    )));
    assert!(!app.notification_area.is_visible());
}

#[test]
fn add_notification_does_not_open_drawer() {
    let mut app = OilChatApp::init();
    app.add_notification(crucible_core::types::Notification::toast("test"));
    assert!(
        !app.notification_area.is_visible(),
        "Adding a notification should not open the drawer"
    );
}

#[test]
fn error_adds_notification_without_opening_drawer() {
    let mut app = OilChatApp::init();
    assert!(!app.notification_area.is_visible());

    app.on_message(ChatAppMsg::Error("Connection lost".to_string()));

    assert!(!app.notification_area.is_visible());
    assert!(!app.notification_area.is_empty());
}

#[test]
fn notify_toast_does_not_open_drawer() {
    let mut app = OilChatApp::init();
    app.notify_toast("test toast");
    assert!(
        !app.notification_area.is_visible(),
        "notify_toast should not open the drawer"
    );
}

#[test]
fn drawer_any_key_dismisses_without_fallthrough() {
    let mut app = OilChatApp::init();
    app.notification_area
        .add(crucible_core::types::Notification::toast("test"));
    app.notification_area.show();
    assert!(app.notification_area.is_visible());

    // Press 'a' — should dismiss drawer but NOT insert 'a' into input
    app.update(Event::Key(crossterm::event::KeyEvent::new(
        KeyCode::Char('a'),
        crossterm::event::KeyModifiers::NONE,
    )));
    assert!(!app.notification_area.is_visible());
    assert!(
        !app.input.content().contains('a'),
        "Key should not fall through to input after dismissing drawer"
    );
}

#[test]
fn messages_drawer_closes_on_permission() {
    let mut app = OilChatApp::init();
    app.notification_area
        .add(crucible_core::types::Notification::toast("test"));
    app.notification_area.show();
    assert!(app.notification_area.is_visible());

    app.open_interaction(
        "req-1".to_string(),
        InteractionRequest::Permission(PermRequest::bash(["ls", "-la"])),
    );
    assert!(!app.notification_area.is_visible());
    assert!(app.interaction_visible());
}

#[test]
fn messages_command_works_during_streaming() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::TextDelta("streaming...".to_string()));
    assert!(app.is_streaming());

    app.notification_area
        .add(crucible_core::types::Notification::toast("test"));

    // Type :messages and submit
    for c in ":messages".chars() {
        app.update(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char(c),
            crossterm::event::KeyModifiers::NONE,
        )));
    }
    app.update(Event::Key(crossterm::event::KeyEvent::new(
        KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    )));

    assert!(
        app.notification_area.is_visible(),
        ":messages should open drawer even during streaming"
    );
    assert!(app.is_streaming(), "Stream should still be active");
}

#[test]
fn test_mode_change_does_not_duplicate_in_status() {
    let mut app = OilChatApp::init();

    for mode in [ChatMode::Plan, ChatMode::Auto, ChatMode::Normal] {
        app.set_mode(mode);
        app.status = "Ready".to_string();

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        let output = render_to_string(&tree, 80);

        let label = mode.as_str().to_uppercase();
        let count = output.matches(&label).count();
        assert_eq!(
            count, 1,
            "Mode '{}' should appear exactly once in status bar, found {} times in: {}",
            label, count, output
        );

        assert!(
            !output.contains(&format!("Mode: {}", mode.as_str())),
            "Status text should not contain 'Mode: {}' (duplicate indicator)",
            mode.as_str()
        );
    }
}

#[test]
fn mode_cycling_works_during_streaming() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::TextDelta("streaming...".to_string()));
    assert!(app.is_streaming());
    assert_eq!(app.mode, ChatMode::Normal);

    app.update(Event::Key(crossterm::event::KeyEvent::new(
        KeyCode::BackTab,
        crossterm::event::KeyModifiers::NONE,
    )));

    assert_ne!(
        app.mode,
        ChatMode::Normal,
        "BackTab should cycle mode during streaming"
    );
    assert!(app.is_streaming(), "Stream should still be active");
}

#[test]
fn export_command_returns_export_session_action() {
    let mut app = OilChatApp::init();
    app.set_session_dir(PathBuf::from("/tmp/test-session"));
    let action = app.handle_export_command("/tmp/test.md");
    match action {
        Action::Send(ChatAppMsg::ExportSession(path)) => {
            assert_eq!(path, PathBuf::from("/tmp/test.md"));
        }
        other => panic!("Expected ExportSession action, got {:?}", other),
    }
}

#[test]
fn export_command_empty_path_sets_error() {
    let mut app = OilChatApp::init();
    let action = app.handle_export_command("");
    assert!(matches!(action, Action::Continue));
    assert!(app.notification_area.active_toast().is_some());
}

#[test]
fn export_command_no_session_sets_error() {
    let mut app = OilChatApp::init();
    let action = app.handle_export_command("/tmp/test.md");
    assert!(matches!(action, Action::Continue));
    let toast = app.notification_area.active_toast();
    assert!(toast.is_some());
    assert!(toast.unwrap().0.contains("No active session"));
}

#[test]
fn precognition_default_on() {
    let app = OilChatApp::init();
    assert!(app.precognition());
}

#[test]
fn precognition_toggle_via_set_command() {
    let mut app = OilChatApp::init();
    assert!(app.precognition());

    app.handle_set_command("set noprecognition");
    assert!(!app.precognition());

    app.handle_set_command("set precognition");
    assert!(app.precognition());

    app.handle_set_command("set precognition!");
    assert!(!app.precognition());
}

#[test]
fn precognition_result_shows_system_message() {
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::PrecognitionResult {
        notes_count: 3,
        notes: vec![],
    });

    let containers = app.container_list().all_containers();
    assert_eq!(containers.len(), 1);
    if let ChatContainer::SystemMessage { content, .. } = &containers[0] {
        assert_eq!(content, "Found 3 relevant notes");
    } else {
        panic!("Expected SystemMessage, got {:?}", containers[0]);
    }
}

#[test]
fn precognition_result_zero_notes_no_message() {
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::PrecognitionResult {
        notes_count: 0,
        notes: vec![],
    });

    assert!(app.container_list().is_empty());
}

#[test]
fn precognition_result_single_note_primary_kiln() {
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::PrecognitionResult {
        notes_count: 1,
        notes: vec![PrecognitionNoteInfo {
            title: "Authentication Guide".to_string(),
            kiln_label: None,
        }],
    });

    let containers = app.container_list().all_containers();
    assert_eq!(containers.len(), 1);
    if let ChatContainer::SystemMessage { content, .. } = &containers[0] {
        assert!(content.contains("Found 1 relevant notes:"));
        assert!(content.contains("\u{00B7} Authentication Guide"));
        assert!(!content.contains('['));
    } else {
        panic!("Expected SystemMessage, got {:?}", containers[0]);
    }
}

#[test]
fn precognition_result_mixed_kilns() {
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::PrecognitionResult {
        notes_count: 3,
        notes: vec![
            PrecognitionNoteInfo {
                title: "Auth Module".to_string(),
                kiln_label: None,
            },
            PrecognitionNoteInfo {
                title: "Security Patterns".to_string(),
                kiln_label: Some("docs".to_string()),
            },
            PrecognitionNoteInfo {
                title: "OAuth2 Flow".to_string(),
                kiln_label: Some("reference".to_string()),
            },
        ],
    });

    let containers = app.container_list().all_containers();
    assert_eq!(containers.len(), 1);
    if let ChatContainer::SystemMessage { content, .. } = &containers[0] {
        assert!(content.contains("Found 3 relevant notes:"));
        assert!(content.contains("\u{00B7} Auth Module"));
        assert!(!content.contains("Auth Module ["));
        assert!(content.contains("\u{00B7} Security Patterns [docs]"));
        assert!(content.contains("\u{00B7} OAuth2 Flow [reference]"));
    } else {
        panic!("Expected SystemMessage, got {:?}", containers[0]);
    }
}

#[test]
fn autoconfirm_session_skips_modal_and_returns_allow() {
    let mut app = OilChatApp::init();
    app.handle_set_command("set perm.autoconfirm_session");

    let request = InteractionRequest::Permission(PermRequest::bash(["rm", "-rf", "/"]));
    let action = app.open_interaction("perm-auto".to_string(), request);

    assert!(!app.interaction_visible(), "Modal should not open");
    match action {
        Action::Send(ChatAppMsg::CloseInteraction {
            request_id,
            response,
        }) => {
            assert_eq!(request_id, "perm-auto");
            match response {
                InteractionResponse::Permission(perm) => assert!(perm.allowed),
                other => panic!("Expected Permission response, got {:?}", other),
            }
        }
        other => panic!("Expected CloseInteraction, got {:?}", other),
    }
}

#[test]
fn autoconfirm_session_does_not_affect_ask_requests() {
    let mut app = OilChatApp::init();
    app.handle_set_command("set perm.autoconfirm_session");

    let request = InteractionRequest::Ask(AskRequest {
        question: "Pick one".to_string(),
        choices: Some(vec!["a".to_string(), "b".to_string()]),
        allow_other: false,
        multi_select: false,
    });
    let action = app.open_interaction("ask-auto".to_string(), request);

    assert!(app.interaction_visible(), "Ask modal should still open");
    assert!(matches!(action, Action::Continue));
}

#[test]
fn autoconfirm_off_shows_modal_normally() {
    let mut app = OilChatApp::init();
    assert!(!app.perm_autoconfirm_session());

    let request = InteractionRequest::Permission(PermRequest::bash(["ls"]));
    let action = app.open_interaction("perm-normal".to_string(), request);

    assert!(
        app.interaction_visible(),
        "Modal should open when autoconfirm off"
    );
    assert!(matches!(action, Action::Continue));
}

#[test]
fn plugins_command_no_plugins_shows_message() {
    let mut app = OilChatApp::init();
    let action = app.handle_repl_command(":plugins");
    assert!(matches!(action, Action::Continue));
    let containers = app.container_list().all_containers();
    assert_eq!(containers.len(), 1);
    if let ChatContainer::SystemMessage { content, .. } = &containers[0] {
        assert!(content.contains("No plugins found"));
    } else {
        panic!("Expected SystemMessage, got {:?}", containers[0]);
    }
}

#[test]
fn plugins_command_shows_status() {
    let mut app = OilChatApp::init();
    app.set_plugin_status(vec![
        PluginStatusEntry {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            state: "Active".to_string(),
            error: None,
        },
        PluginStatusEntry {
            name: "broken-plugin".to_string(),
            version: "0.1.0".to_string(),
            state: "Error".to_string(),
            error: Some("syntax error".to_string()),
        },
    ]);
    let action = app.handle_repl_command(":plugins");
    assert!(matches!(action, Action::Continue));
    let containers = app.container_list().all_containers();
    assert_eq!(containers.len(), 1);
    if let ChatContainer::SystemMessage { content, .. } = &containers[0] {
        assert!(content.contains("Plugins (2):"), "Header with count");
        assert!(
            content.contains("✓ test-plugin v1.0.0 (active)"),
            "Active plugin"
        );
        assert!(
            content.contains("✗ broken-plugin v0.1.0 (error: syntax error)"),
            "Error plugin with message"
        );
    } else {
        panic!("Expected SystemMessage, got {:?}", containers[0]);
    }
}

#[test]
fn plugin_status_loaded_stores_entries_without_duplicate_notifications() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::PluginStatusLoaded(vec![
        PluginStatusEntry {
            name: "good-plugin".to_string(),
            version: "1.0.0".to_string(),
            state: "Active".to_string(),
            error: None,
        },
        PluginStatusEntry {
            name: "bad-plugin".to_string(),
            version: String::new(),
            state: "Error".to_string(),
            error: Some("file not found".to_string()),
        },
    ]));
    assert_eq!(app.plugin_status.len(), 2);
    assert!(
        app.notification_area.is_empty(),
        "PluginStatusLoaded should not create notifications (runner init handles that)"
    );
}

#[test]
fn tool_result_error_strips_prefix_chains() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::ToolCall {
        name: "read".to_string(),
        args: "{}".to_string(),
        call_id: Some("c1".to_string()),
        description: None,
        source: None,
    });
    app.on_message(ChatAppMsg::ToolResultError {
        name: "read".to_string(),
        error: "ToolCallError: ToolCallError: ToolCallError: file not found".to_string(),
        call_id: Some("c1".to_string()),
    });
    let tool = app.container_list().find_tool("read").unwrap();
    assert_eq!(
        tool.error.as_deref(),
        Some("file not found"),
        "Should strip nested ToolCallError prefixes"
    );
}

#[test]
fn tool_result_error_strips_mixed_prefixes() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::ToolCall {
        name: "search".to_string(),
        args: "{}".to_string(),
        call_id: None,
        description: None,
        source: None,
    });
    app.on_message(ChatAppMsg::ToolResultError {
        name: "search".to_string(),
        error: "ToolCallError: MCP gateway error: connection refused".to_string(),
        call_id: None,
    });
    let tool = app.container_list().find_tool("search").unwrap();
    assert_eq!(
        tool.error.as_deref(),
        Some("connection refused"),
        "Should strip mixed prefix chain"
    );
}

#[test]
fn tool_result_error_preserves_clean_errors() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".to_string(),
        args: "{}".to_string(),
        call_id: None,
        description: None,
        source: None,
    });
    app.on_message(ChatAppMsg::ToolResultError {
        name: "bash".to_string(),
        error: "command not found".to_string(),
        call_id: None,
    });
    let tool = app.container_list().find_tool("bash").unwrap();
    assert_eq!(
        tool.error.as_deref(),
        Some("command not found"),
        "Clean errors should pass through unchanged"
    );
}
