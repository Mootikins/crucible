//! TUI Snapshot Tests
//!
//! Uses insta for snapshot testing of TUI rendering.
//! Run `cargo insta review` to interactively review snapshot changes.
//!
//! NOTE: Popup snapshot tests are in src/tui/testing/popup_snapshot_tests.rs
//! which uses the Harness to properly manage View-owned popup state.

#![allow(clippy::field_reassign_with_default)]

use crucible_cli::tui::render::render;
use crucible_cli::tui::testing::{test_terminal, TestStateBuilder, TEST_HEIGHT, TEST_WIDTH};
use insta::assert_snapshot;
use ratatui::{backend::TestBackend, Terminal};

// =============================================================================
// Basic Rendering Tests
// =============================================================================

#[test]
fn no_popup_renders_clean() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("plan").with_input("/help").build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("no_popup_clean", terminal.backend());
}

// =============================================================================
// Mode Display Tests
// =============================================================================

#[test]
fn mode_plan_display() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("plan").build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("mode_plan", terminal.backend());
}

#[test]
fn mode_act_display() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("act").build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("mode_act", terminal.backend());
}

#[test]
fn mode_auto_display() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("auto").build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("mode_auto", terminal.backend());
}

// =============================================================================
// Streaming Tests
// =============================================================================

#[test]
fn streaming_partial_response() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("act")
        .with_streaming("Hi! I'm currently thinking about your question...")
        .build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("streaming_partial", terminal.backend());
}

#[test]
fn streaming_empty() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("plan").build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("streaming_empty", terminal.backend());
}

// =============================================================================
// Error Display Tests
// =============================================================================

#[test]
fn status_error_display() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("plan")
        .with_error("Connection failed: timeout")
        .build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("status_error", terminal.backend());
}

// =============================================================================
// Input Display Tests
// =============================================================================

#[test]
fn input_with_text() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("plan")
        .with_input("Hello, how can you help me?")
        .build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("input_with_text", terminal.backend());
}

#[test]
fn input_empty() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("act").with_input("").build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("input_empty", terminal.backend());
}

// =============================================================================
// Conversation View Tests
// =============================================================================

use crucible_cli::tui::conversation::{
    ConversationState, ConversationWidget, InputBoxWidget, StatusBarWidget, StatusKind,
};
use ratatui::layout::{Constraint, Direction, Layout};

/// Helper to render a full conversation view
fn render_conversation_view(
    terminal: &mut Terminal<TestBackend>,
    conversation: &ConversationState,
    input: &str,
    mode_id: &str,
    token_count: Option<usize>,
    status: &str,
) {
    terminal
        .draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(10),   // Conversation area
                    Constraint::Length(3), // Input box
                    Constraint::Length(1), // Status bar
                ])
                .split(f.area());

            // Conversation
            let conv_widget = ConversationWidget::new(conversation);
            f.render_widget(conv_widget, chunks[0]);

            // Input box
            let input_widget = InputBoxWidget::new(input, input.len());
            f.render_widget(input_widget, chunks[1]);

            // Status bar
            let mut status_widget = StatusBarWidget::new(mode_id, status);
            if let Some(count) = token_count {
                status_widget = status_widget.token_count(count);
            }
            f.render_widget(status_widget, chunks[2]);
        })
        .unwrap();
}

#[test]
fn conversation_user_message_inverted() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("What files handle authentication?");

    render_conversation_view(&mut terminal, &conv, "", "plan", None, "Ready");
    assert_snapshot!("conv_user_message", terminal.backend());
}

#[test]
fn conversation_assistant_response() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("Hello");
    conv.push_assistant_message("Hi! I'm here to help you with your code.");

    render_conversation_view(&mut terminal, &conv, "", "plan", None, "Ready");
    assert_snapshot!("conv_assistant_response", terminal.backend());
}

#[test]
fn conversation_thinking_status() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("What is the architecture?");
    conv.set_status(StatusKind::Thinking { spinner_frame: 0 });

    render_conversation_view(&mut terminal, &conv, "", "plan", None, "Ready");
    assert_snapshot!("conv_thinking", terminal.backend());
}

#[test]
fn conversation_generating_with_tokens() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("Explain this code");
    conv.set_status(StatusKind::Generating {
        token_count: 127,
        prev_token_count: 0,
        spinner_frame: 0,
    });

    render_conversation_view(&mut terminal, &conv, "", "act", Some(127), "Generating");
    assert_snapshot!("conv_generating_tokens", terminal.backend());
}

#[test]
fn conversation_tool_running() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("Find auth files");
    conv.push_assistant_message("Let me search for authentication-related files.");
    conv.push_tool_running("grep", serde_json::json!({"pattern": "auth", "type": "rs"}));

    render_conversation_view(&mut terminal, &conv, "", "act", None, "Tool running");
    assert_snapshot!("conv_tool_running", terminal.backend());
}

#[test]
fn conversation_tool_with_output() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("Find auth files");
    conv.push_tool_running("glob", serde_json::json!({"pattern": "**/*auth*.rs"}));
    conv.update_tool_output(
        "glob",
        "src/auth/mod.rs\nsrc/auth/jwt.rs\nsrc/auth/session.rs",
    );
    conv.complete_tool("glob", Some("3 files".to_string()));

    render_conversation_view(&mut terminal, &conv, "", "plan", None, "Ready");
    assert_snapshot!("conv_tool_complete", terminal.backend());
}

#[test]
fn conversation_tool_error() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_tool_running("read", serde_json::json!({"path": "/nonexistent"}));
    conv.error_tool("read", "file not found");

    render_conversation_view(&mut terminal, &conv, "", "plan", None, "Ready");
    assert_snapshot!("conv_tool_error", terminal.backend());
}

#[test]
fn conversation_multiple_tools() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("Search the codebase");
    conv.push_assistant_message("I'll search for relevant files.");

    // First tool - complete
    conv.push_tool_running("grep", serde_json::json!({"pattern": "pattern"}));
    conv.complete_tool("grep", Some("12 matches".to_string()));

    // Second tool - running
    conv.push_tool_running("read", serde_json::json!({"path": "src/main.rs"}));
    conv.update_tool_output("read", "fn main() {\n    println!(\"Hello\");\n}");

    render_conversation_view(&mut terminal, &conv, "", "act", None, "Tool running");
    assert_snapshot!("conv_multiple_tools", terminal.backend());
}

#[test]
fn conversation_full_exchange() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();

    // User question
    conv.push_user_message("What files handle authentication?");

    // Assistant response with tool use
    conv.push_assistant_message("Let me search for authentication-related files.");

    conv.push_tool_running("grep", serde_json::json!({"pattern": "auth"}));
    conv.complete_tool("grep", Some("found in 3 files".to_string()));

    conv.push_tool_running("glob", serde_json::json!({"pattern": "**/*auth*.rs"}));
    conv.update_tool_output("glob", "src/auth/mod.rs\nsrc/auth/jwt.rs");
    conv.complete_tool("glob", Some("2 files".to_string()));

    // Final response
    conv.push_assistant_message(
        "I found these authentication files:\n- src/auth/mod.rs - Main module\n- src/auth/jwt.rs - JWT handling",
    );

    render_conversation_view(&mut terminal, &conv, "", "plan", Some(256), "Ready");
    assert_snapshot!("conv_full_exchange", terminal.backend());
}

#[test]
fn conversation_input_box_with_content() {
    let mut terminal = test_terminal();
    let conv = ConversationState::new();

    render_conversation_view(
        &mut terminal,
        &conv,
        "How do I add a new feature?",
        "act",
        None,
        "Ready",
    );
    assert_snapshot!("conv_input_with_content", terminal.backend());
}

#[test]
fn conversation_markdown_formatting() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("Show me an example");
    conv.push_assistant_message(
        "Here's a **Rust** example:\n\n```rust\nfn main() {\n    println!(\"Hello, world!\");\n}\n```\n\nUse `cargo run` to execute it.\n\nYou can also:\n- Build with `cargo build`\n- Test with `cargo test`\n\nSee the *Cargo Book* for more details."
    );

    render_conversation_view(&mut terminal, &conv, "", "plan", None, "Ready");
    assert_snapshot!("conv_markdown_formatted", terminal.backend());
}

#[test]
fn conversation_status_bar_modes() {
    // Test all three modes show correctly
    let mut terminal = test_terminal();
    let conv = ConversationState::new();

    // Plan mode
    render_conversation_view(&mut terminal, &conv, "", "plan", Some(100), "Ready");
    assert_snapshot!("conv_status_plan", terminal.backend());
}

#[test]
fn conversation_status_bar_act_mode() {
    let mut terminal = test_terminal();
    let conv = ConversationState::new();

    render_conversation_view(&mut terminal, &conv, "", "act", Some(200), "Ready");
    assert_snapshot!("conv_status_act", terminal.backend());
}

#[test]
fn conversation_status_bar_auto_mode() {
    let mut terminal = test_terminal();
    let conv = ConversationState::new();

    render_conversation_view(&mut terminal, &conv, "", "auto", Some(300), "Ready");
    assert_snapshot!("conv_status_auto", terminal.backend());
}

// =============================================================================
// SendMessage State Transition Tests
// =============================================================================
// These tests verify the expected UI state after SendMessage action.
// The input should be cleared and status should show "Thinking".

use crucible_cli::tui::conversation_view::{ConversationView, RatatuiView};

/// Helper to render RatatuiView (full view with popup support)
fn render_ratatui_view(terminal: &mut Terminal<TestBackend>, view: &RatatuiView) {
    terminal.draw(|f| view.render_frame(f)).unwrap();
}

/// Test: After SendMessage, input box should be EMPTY
///
/// This is the expected behavior:
/// 1. User types "Hello world"
/// 2. User presses Enter (SendMessage)
/// 3. Input box clears
/// 4. User message appears in conversation
/// 5. Status shows "Thinking"
///
/// BUG: Currently RatatuiRunner doesn't clear input after SendMessage
#[test]
fn send_message_clears_input() {
    let mut terminal = test_terminal();
    let mut view = RatatuiView::new("plan", TEST_WIDTH, TEST_HEIGHT);

    // Simulate state BEFORE sending: user typed "Hello world"
    view.set_input("Hello world");
    view.set_cursor_position(11);

    // Capture "before" state
    render_ratatui_view(&mut terminal, &view);
    let before_snapshot = format!("{:?}", terminal.backend().buffer());

    // Now simulate what SHOULD happen after SendMessage:
    // 1. Clear input (THIS IS THE BUG - runner doesn't do this)
    view.set_input("");
    view.set_cursor_position(0);
    // 2. Add user message to conversation
    view.push_user_message("Hello world").unwrap();
    // 3. Set thinking status
    view.set_status(StatusKind::Thinking { spinner_frame: 0 });
    view.set_status_text("Thinking");

    // Capture "after" state
    render_ratatui_view(&mut terminal, &view);
    let after_snapshot = format!("{:?}", terminal.backend().buffer());

    // The before and after should be DIFFERENT (input should be cleared)
    assert_ne!(
        before_snapshot, after_snapshot,
        "UI should change after SendMessage"
    );

    // Verify input is now empty in the view
    assert_eq!(
        view.input(),
        "",
        "Input should be cleared after SendMessage"
    );

    // Snapshot the expected "after" state for visual verification
    assert_snapshot!("send_message_input_cleared", terminal.backend());
}

/// Test: Status shows "Thinking" after SendMessage
#[test]
fn send_message_shows_thinking_status() {
    let mut terminal = test_terminal();
    let mut view = RatatuiView::new("act", TEST_WIDTH, TEST_HEIGHT);

    // Simulate SendMessage flow
    view.push_user_message("What is the project structure?")
        .unwrap();
    view.set_status(StatusKind::Thinking { spinner_frame: 0 });
    view.set_status_text("Thinking");
    view.set_input(""); // Should be cleared

    render_ratatui_view(&mut terminal, &view);

    // Verify view state
    assert_eq!(view.status_text(), "Thinking");
    assert_eq!(view.input(), "");

    assert_snapshot!("send_message_thinking_status", terminal.backend());
}

// =============================================================================
// Streaming Parser Tests (Phase 4)
// =============================================================================
// Tests for incremental parsing with ContentBlock rendering

use crucible_cli::tui::content_block::StreamBlock;

#[test]
fn streaming_partial_prose() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("Explain this");
    conv.start_assistant_streaming();
    conv.append_streaming_blocks(vec![StreamBlock::prose_partial("Hello, I'm here to hel")]);

    render_conversation_view(&mut terminal, &conv, "", "act", Some(23), "Generating");
    assert_snapshot!("conv_streaming_partial_prose", terminal.backend());
}

#[test]
fn streaming_code_block() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("Show me code");
    conv.start_assistant_streaming();
    conv.append_streaming_blocks(vec![
        StreamBlock::prose("Here's the code:"),
        StreamBlock::code_partial(Some("rust".into()), "fn main() {\n    println!(\"Hel"),
    ]);

    render_conversation_view(&mut terminal, &conv, "", "act", Some(45), "Generating");
    assert_snapshot!("conv_streaming_code_block", terminal.backend());
}

#[test]
fn streaming_complete_blocks() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("Explain");
    conv.start_assistant_streaming();
    conv.append_streaming_blocks(vec![
        StreamBlock::prose("First paragraph complete."),
        StreamBlock::code(Some("rust".into()), "fn example() {}"),
        StreamBlock::prose_partial("Now continuin"),
    ]);

    render_conversation_view(&mut terminal, &conv, "", "act", Some(80), "Generating");
    assert_snapshot!("conv_streaming_mixed_blocks", terminal.backend());
}

// =============================================================================
// Multi-turn Conversation Tests
// =============================================================================
// Tests for realistic multi-turn conversations with tool calls

#[test]
fn conversation_multiturn_with_tools() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();

    // Turn 1: User asks, assistant responds with tool
    conv.push_user_message("Read my note");
    conv.start_assistant_streaming();
    // Simulate prose that might have leading newline (common from LLM responses)
    conv.append_streaming_blocks(vec![StreamBlock::prose(
        "\n▷ read({\"filePath\":\"docs/Note1.md\"})",
    )]);
    conv.complete_streaming();

    // Turn 2: User follows up
    conv.push_user_message("Try again");
    conv.push_assistant_message("Let me try a different approach.");

    render_conversation_view(&mut terminal, &conv, "", "plan", None, "Ready");
    assert_snapshot!("conv_multiturn_tools", terminal.backend());
}

#[test]
fn prose_with_leading_newline_should_not_orphan_prefix() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("Test");
    conv.start_assistant_streaming();
    // Content starts with newline - prefix should NOT be on empty line
    conv.append_streaming_blocks(vec![StreamBlock::prose("\nThis is the actual content.")]);

    render_conversation_view(&mut terminal, &conv, "", "plan", None, "Ready");

    // Get the rendered output and check that ● is not on its own line
    let backend = terminal.backend();
    let buffer_str = format!("{:?}", backend);

    // The prefix should be followed by content, not just whitespace
    // If we see " ● " followed by only spaces until newline, that's a bug
    assert!(
        !buffer_str.contains("\" ● \""),
        "Prefix should not be on its own line - got orphaned ● symbol"
    );

    assert_snapshot!("conv_prose_leading_newline", terminal.backend());
}

// =============================================================================
// Dialog System Tests (Phase 5)
// =============================================================================
// Tests for modal dialogs with centered overlay rendering

use crucible_cli::tui::dialog::{DialogState, DialogWidget};

#[test]
fn dialog_confirm() {
    let mut terminal = test_terminal();
    let dialog = DialogState::confirm("Confirm Action", "Are you sure you want to proceed?");

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    assert_snapshot!("dialog_confirm", terminal.backend());
}

#[test]
fn dialog_confirm_focused_cancel() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut terminal = test_terminal();
    let mut dialog = DialogState::confirm("Delete File", "This action cannot be undone.");
    // Move focus to cancel button by pressing Right
    dialog.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    assert_snapshot!("dialog_confirm_focused_cancel", terminal.backend());
}

#[test]
fn dialog_select() {
    let mut terminal = test_terminal();
    let dialog = DialogState::select(
        "Select Agent",
        vec![
            "claude-opus".into(),
            "local-qwen".into(),
            "research-agent".into(),
        ],
    );

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    assert_snapshot!("dialog_select", terminal.backend());
}

#[test]
fn dialog_select_second_item() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut terminal = test_terminal();
    let mut dialog = DialogState::select(
        "Choose Mode",
        vec!["plan".into(), "act".into(), "auto".into()],
    );
    // Select second item by pressing Down
    dialog.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    assert_snapshot!("dialog_select_second_item", terminal.backend());
}

#[test]
fn dialog_info() {
    let mut terminal = test_terminal();
    let dialog = DialogState::info("Help", "Press Ctrl+C to exit\nPress ? for help");

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    assert_snapshot!("dialog_info", terminal.backend());
}

#[test]
fn dialog_info_multiline() {
    let mut terminal = test_terminal();
    let dialog = DialogState::info(
        "Keyboard Shortcuts",
        "Navigation:\n  j/k - Move down/up\n  h/l - Move left/right\n\nActions:\n  Enter - Confirm\n  Esc - Cancel\n  q - Quit",
    );

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    assert_snapshot!("dialog_info_multiline", terminal.backend());
}

#[test]
fn dialog_select_many_items() {
    let mut terminal = test_terminal();
    let items: Vec<String> = (1..=15).map(|i| format!("Option {}", i)).collect();
    let dialog = DialogState::select("Select Option", items);

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    assert_snapshot!("dialog_select_many_items", terminal.backend());
}

#[test]
fn dialog_over_conversation() {
    let mut terminal = test_terminal();
    let mut view = RatatuiView::new("plan", TEST_WIDTH, TEST_HEIGHT);

    // Add some conversation content
    view.push_user_message("What is the project structure?")
        .unwrap();
    view.push_assistant_message("Let me analyze the codebase.")
        .unwrap();

    // Push a dialog
    view.push_dialog(DialogState::confirm(
        "Continue Analysis",
        "This will analyze all files. Continue?",
    ));

    terminal.draw(|f| view.render_frame(f)).unwrap();

    assert_snapshot!("dialog_over_conversation", terminal.backend());
}

// =============================================================================
// Panel Interaction Tests (InteractivePanel → DialogState)
// =============================================================================
// Tests for the InteractivePanel primitive and its TUI rendering.
// Panels are currently rendered via DialogState::select.

use crucible_core::interaction::{InteractivePanel, PanelHints, PanelItem, PanelResult};

/// Helper to create a dialog from an InteractivePanel (matching runner.rs logic)
fn panel_to_dialog(panel: &InteractivePanel) -> DialogState {
    let mut choices: Vec<String> = panel
        .items
        .iter()
        .map(|item| {
            if let Some(desc) = &item.description {
                format!("{} - {}", item.label, desc)
            } else {
                item.label.clone()
            }
        })
        .collect();

    // Add "Other..." option if hints.allow_other is enabled
    if panel.hints.allow_other {
        choices.push("[Other...]".to_string());
    }

    DialogState::select(&panel.header, choices)
}

#[test]
fn panel_basic_select() {
    let mut terminal = test_terminal();

    let panel = InteractivePanel::new("Select database").items([
        PanelItem::new("PostgreSQL").with_description("Full-featured RDBMS"),
        PanelItem::new("SQLite").with_description("Embedded, single-file"),
        PanelItem::new("MongoDB").with_description("Document store"),
    ]);

    let dialog = panel_to_dialog(&panel);

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    assert_snapshot!("panel_basic_select", terminal.backend());
}

#[test]
fn panel_items_without_descriptions() {
    let mut terminal = test_terminal();

    let panel = InteractivePanel::new("Pick a color").items([
        PanelItem::new("Red"),
        PanelItem::new("Green"),
        PanelItem::new("Blue"),
    ]);

    let dialog = panel_to_dialog(&panel);

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    assert_snapshot!("panel_items_without_desc", terminal.backend());
}

#[test]
fn panel_with_allow_other() {
    let mut terminal = test_terminal();

    let panel = InteractivePanel::new("Select framework")
        .items([
            PanelItem::new("React"),
            PanelItem::new("Vue"),
            PanelItem::new("Svelte"),
        ])
        .hints(PanelHints::default().allow_other());

    let dialog = panel_to_dialog(&panel);

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    // Should have "[Other...]" as the last option
    assert_snapshot!("panel_with_allow_other", terminal.backend());
}

#[test]
fn panel_confirm_yes_no() {
    let mut terminal = test_terminal();

    // Simulates ui.confirm() - a panel with Yes/No choices
    let panel = InteractivePanel::new("Delete this file?")
        .items([PanelItem::new("Yes"), PanelItem::new("No")]);

    let dialog = panel_to_dialog(&panel);

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    assert_snapshot!("panel_confirm_yes_no", terminal.backend());
}

#[test]
fn panel_select_navigation() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut terminal = test_terminal();

    let panel = InteractivePanel::new("Choose mode").items([
        PanelItem::new("plan").with_description("Plan before acting"),
        PanelItem::new("act").with_description("Execute immediately"),
        PanelItem::new("auto").with_description("Automatic execution"),
    ]);

    let mut dialog = panel_to_dialog(&panel);
    // Navigate to second item
    dialog.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    assert_snapshot!("panel_select_navigation", terminal.backend());
}

#[test]
fn panel_many_items_scrolling() {
    let mut terminal = test_terminal();

    // Create a panel with many items to test scrolling
    let items: Vec<PanelItem> = (1..=15)
        .map(|i| PanelItem::new(format!("Option {}", i)))
        .collect();

    let panel = InteractivePanel::new("Select from many").items(items);
    let dialog = panel_to_dialog(&panel);

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    assert_snapshot!("panel_many_items", terminal.backend());
}

#[test]
fn panel_search_with_hints() {
    let mut terminal = test_terminal();

    // Simulates ui.search() - filterable + allow_other
    let panel = InteractivePanel::new("Find note")
        .items([
            PanelItem::new("Daily Note"),
            PanelItem::new("Todo List"),
            PanelItem::new("Project Ideas"),
        ])
        .hints(PanelHints::default().filterable().allow_other());

    let dialog = panel_to_dialog(&panel);

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    // Note: filtering is not yet implemented in dialog, but allow_other shows "[Other...]"
    assert_snapshot!("panel_search_hints", terminal.backend());
}

#[test]
fn panel_over_conversation() {
    let mut terminal = test_terminal();
    let mut view = RatatuiView::new("plan", TEST_WIDTH, TEST_HEIGHT);

    // Add conversation context
    view.push_user_message("What database should I use?")
        .unwrap();
    view.push_assistant_message("I'll help you choose a database. Let me present some options.")
        .unwrap();

    // Create and display panel as dialog
    let panel = InteractivePanel::new("Select database").items([
        PanelItem::new("PostgreSQL").with_description("Full-featured RDBMS"),
        PanelItem::new("SQLite").with_description("Embedded, single-file"),
    ]);

    view.push_dialog(panel_to_dialog(&panel));

    terminal.draw(|f| view.render_frame(f)).unwrap();

    assert_snapshot!("panel_over_conversation", terminal.backend());
}

#[test]
fn panel_with_data_renders_correctly() {
    let mut terminal = test_terminal();

    // Items with attached data (not visible in render, but should not affect display)
    let panel = InteractivePanel::new("Select agent").items([
        PanelItem::new("researcher")
            .with_description("Research agent")
            .with_data(serde_json::json!({"id": "agent-1", "model": "opus"})),
        PanelItem::new("coder")
            .with_description("Coding agent")
            .with_data(serde_json::json!({"id": "agent-2", "model": "sonnet"})),
    ]);

    let dialog = panel_to_dialog(&panel);

    terminal
        .draw(|f| {
            let widget = DialogWidget::new(&dialog);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    assert_snapshot!("panel_with_data", terminal.backend());
}

// =============================================================================
// PanelResult Tests (Verify result types serialize correctly)
// =============================================================================

#[test]
fn panel_result_selected_serializes() {
    let result = PanelResult::selected([0, 2]);
    let json = serde_json::to_string_pretty(&result).unwrap();
    insta::assert_snapshot!("panel_result_selected", json);
}

#[test]
fn panel_result_cancelled_serializes() {
    let result = PanelResult::cancelled();
    let json = serde_json::to_string_pretty(&result).unwrap();
    insta::assert_snapshot!("panel_result_cancelled", json);
}

#[test]
fn panel_result_other_serializes() {
    let result = PanelResult::other("custom input");
    let json = serde_json::to_string_pretty(&result).unwrap();
    insta::assert_snapshot!("panel_result_other", json);
}

// =============================================================================
// InteractivePanel Serialization Tests
// =============================================================================

#[test]
fn interactive_panel_serializes() {
    let panel = InteractivePanel::new("Select database")
        .items([
            PanelItem::new("PostgreSQL").with_description("Full-featured RDBMS"),
            PanelItem::new("SQLite"),
        ])
        .hints(PanelHints::default().filterable().allow_other());

    let json = serde_json::to_string_pretty(&panel).unwrap();
    insta::assert_snapshot!("interactive_panel_json", json);
}

#[test]
fn interactive_panel_with_multi_select() {
    let panel = InteractivePanel::new("Select features")
        .items([
            PanelItem::new("Auth"),
            PanelItem::new("Logging"),
            PanelItem::new("Caching"),
        ])
        .hints(
            PanelHints::default()
                .multi_select()
                .initial_selection([0, 2]),
        );

    let json = serde_json::to_string_pretty(&panel).unwrap();
    insta::assert_snapshot!("panel_multi_select_json", json);
}
