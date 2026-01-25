//! Snapshot tests for ChatApp visual states

use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, ChatMode, InkChatApp};
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::planning::FramePlanner;
use crate::tui::oil::render::render_to_string;
use crate::tui::oil::test_harness::AppHarness;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;

fn render_app(app: &InkChatApp) -> String {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let mut planner = FramePlanner::new(80, 24);
    let snapshot = planner.plan(&tree);
    strip_ansi(&snapshot.viewport_with_overlays(80))
}

fn render_app_raw(app: &InkChatApp) -> String {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let mut planner = FramePlanner::new(80, 24);
    let snapshot = planner.plan(&tree);
    snapshot.viewport_with_overlays(80)
}

#[test]
fn snapshot_empty_chat_view() {
    let app = InkChatApp::default();
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_single_user_message() {
    let mut app = InkChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Hello, how are you?".to_string()));
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_user_and_assistant_exchange() {
    let mut app = InkChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("What is 2+2?".to_string()));
    app.on_message(ChatAppMsg::TextDelta("The answer is ".to_string()));
    app.on_message(ChatAppMsg::TextDelta("4.".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_streaming_in_progress() {
    let mut app = InkChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Tell me a story".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Once upon a time".to_string()));
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_streaming_with_spinner() {
    let mut app = InkChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Generate something".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Working on it...".to_string()));

    // Tick to advance spinner
    for _ in 0..3 {
        app.update(crate::tui::oil::event::Event::Tick);
    }

    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_tool_call_pending() {
    let mut app = InkChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Read a file".to_string()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"README.md","offset":1,"limit":100}"#.to_string(),
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_tool_call_complete() {
    let mut app = InkChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Read a file".to_string()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"README.md"}"#.to_string(),
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "# README\n\nThis is the content.\n".to_string(),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_error_displayed() {
    let mut app = InkChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Do something".to_string()));
    app.on_message(ChatAppMsg::Error("Connection failed: timeout".to_string()));
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_popup_open() {
    let mut app = InkChatApp::default();

    // Open popup with F1
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::F(1),
        KeyModifiers::NONE,
    )));

    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_popup_with_selection_moved() {
    let mut app = InkChatApp::default();

    // Open popup with F1
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::F(1),
        KeyModifiers::NONE,
    )));

    // Move selection down twice
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Down,
        KeyModifiers::NONE,
    )));
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Down,
        KeyModifiers::NONE,
    )));

    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_status_bar_plan_mode() {
    let mut app = InkChatApp::default();
    app.set_mode(ChatMode::Plan);
    app.on_message(ChatAppMsg::ContextUsage {
        used: 5000,
        total: 128000,
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_status_bar_normal_mode() {
    let mut app = InkChatApp::default();
    app.set_mode(ChatMode::Normal);
    app.on_message(ChatAppMsg::ContextUsage {
        used: 64000,
        total: 128000,
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_status_bar_auto_mode() {
    let mut app = InkChatApp::default();
    app.set_mode(ChatMode::Auto);
    app.on_message(ChatAppMsg::ContextUsage {
        used: 100000,
        total: 128000,
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_notification_visible() {
    let mut app = InkChatApp::default();

    // Press Ctrl+C to show notification
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )));

    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_multi_turn_conversation() {
    let mut app = InkChatApp::default();

    // First exchange
    app.on_message(ChatAppMsg::UserMessage("What is Rust?".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "Rust is a systems programming language.".to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    // Second exchange
    app.on_message(ChatAppMsg::UserMessage(
        "What about memory safety?".to_string(),
    ));
    app.on_message(ChatAppMsg::TextDelta(
        "Rust ensures memory safety through its ownership system.".to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_system_message() {
    let mut app = InkChatApp::default();

    // Trigger help command which adds system message
    for c in "/help".chars() {
        app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )));
    }
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));

    assert_snapshot!(render_app(&app));
}

mod composer_stability_snapshots {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn snapshot_popup_hidden_baseline() {
        let mut app = InkChatApp::default();
        app.set_workspace_files(vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "Cargo.toml".to_string(),
        ]);
        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_popup_visible_same_height() {
        let mut app = InkChatApp::default();
        app.set_workspace_files(vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "Cargo.toml".to_string(),
        ]);

        app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char('@'))));

        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_input_empty() {
        let app = InkChatApp::default();
        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_input_short_text() {
        let mut app = InkChatApp::default();
        for c in "Hello".chars() {
            app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char(c))));
        }
        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_input_long_text_clamped() {
        let mut app = InkChatApp::default();
        let long_text = "x".repeat(300);
        for c in long_text.chars() {
            app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char(c))));
        }
        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn verify_input_height_grows_with_content() {
        use crate::tui::oil::chat_app::INPUT_MAX_CONTENT_LINES;

        let app_empty = InkChatApp::default();

        let mut app_long = InkChatApp::default();
        for c in "x".repeat(300).chars() {
            app_long.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char(c))));
        }

        let empty_output = render_app(&app_empty);
        let long_output = render_app(&app_long);

        let empty_lines = empty_output.lines().count();
        let long_lines = long_output.lines().count();

        assert!(
            empty_lines < long_lines,
            "Long input ({} lines) should have more lines than empty ({} lines)",
            long_lines,
            empty_lines
        );

        let max_input_height = INPUT_MAX_CONTENT_LINES + 2;
        assert!(
            long_lines <= empty_lines + max_input_height,
            "Long input growth should be bounded by max content lines"
        );
    }
}

// =============================================================================
// Interaction Modal Snapshot Tests
// =============================================================================

mod interaction_modal_snapshots {
    use super::*;
    use crucible_core::interaction::{AskRequest, InteractionRequest, PermRequest};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Snapshot test: AskRequest with 3 choices, first selected (default)
    #[test]
    fn snapshot_ask_modal_with_choices_first_selected() {
        let mut app = InkChatApp::default();
        let request = InteractionRequest::Ask(
            AskRequest::new("Which programming language do you prefer?").choices([
                "Rust",
                "Python",
                "TypeScript",
            ]),
        );
        app.open_interaction("ask-1".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: AskRequest with 3 choices, second selected
    #[test]
    fn snapshot_ask_modal_with_choices_second_selected() {
        let mut app = InkChatApp::default();
        let request = InteractionRequest::Ask(
            AskRequest::new("Which programming language do you prefer?").choices([
                "Rust",
                "Python",
                "TypeScript",
            ]),
        );
        app.open_interaction("ask-2".to_string(), request);

        // Navigate down to select second option
        app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Down)));

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: AskRequest with allow_other showing "Other..." option
    #[test]
    fn snapshot_ask_modal_with_allow_other() {
        let mut app = InkChatApp::default();
        let request = InteractionRequest::Ask(
            AskRequest::new("Select your favorite or enter custom:")
                .choices(["Option A", "Option B"])
                .allow_other(),
        );
        app.open_interaction("ask-3".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: AskRequest free-text only (no choices)
    #[test]
    fn snapshot_ask_modal_free_text_only() {
        let mut app = InkChatApp::default();
        let request =
            InteractionRequest::Ask(AskRequest::new("Enter your custom value:").allow_other());
        app.open_interaction("ask-4".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: PermRequest for bash command
    #[test]
    fn snapshot_perm_modal_bash_command() {
        let mut app = InkChatApp::default();
        let request =
            InteractionRequest::Permission(PermRequest::bash(["npm", "install", "lodash"]));
        app.open_interaction("perm-bash".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: PermRequest for file write
    #[test]
    fn snapshot_perm_modal_file_write() {
        let mut app = InkChatApp::default();
        let request = InteractionRequest::Permission(PermRequest::write([
            "home", "user", "project", "src", "main.rs",
        ]));
        app.open_interaction("perm-write".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: PermRequest for file read
    #[test]
    fn snapshot_perm_modal_file_read() {
        let mut app = InkChatApp::default();
        let request = InteractionRequest::Permission(PermRequest::read(["etc", "hosts"]));
        app.open_interaction("perm-read".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: PermRequest for tool execution
    #[test]
    fn snapshot_perm_modal_tool() {
        let mut app = InkChatApp::default();
        let request = InteractionRequest::Permission(PermRequest::tool(
            "semantic_search",
            serde_json::json!({"query": "rust memory safety", "limit": 10}),
        ));
        app.open_interaction("perm-tool".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: AskRequest with many choices (scrolling)
    #[test]
    fn snapshot_ask_modal_many_choices() {
        let mut app = InkChatApp::default();
        let request = InteractionRequest::Ask(AskRequest::new("Select an option:").choices([
            "First option",
            "Second option",
            "Third option",
            "Fourth option",
            "Fifth option",
            "Sixth option",
            "Seventh option",
            "Eighth option",
        ]));
        app.open_interaction("ask-many".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: AskRequest after navigating to last choice
    #[test]
    fn snapshot_ask_modal_last_selected() {
        let mut app = InkChatApp::default();
        let request = InteractionRequest::Ask(
            AskRequest::new("Pick one:").choices(["Alpha", "Beta", "Gamma", "Delta"]),
        );
        app.open_interaction("ask-last".to_string(), request);

        // Navigate to last option
        for _ in 0..3 {
            app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Down)));
        }

        assert_snapshot!(render_app(&app));
    }
}
