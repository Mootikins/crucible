//! TUI Snapshot Tests

#![allow(clippy::field_reassign_with_default)]

//!
//! Uses insta for snapshot testing of TUI rendering.
//! Run `cargo insta review` to interactively review snapshot changes.

use crucible_cli::tui::render::render;
use crucible_cli::tui::state::{PopupItem, PopupItemKind, PopupKind, PopupState, TuiState};
use crucible_cli::tui::streaming::StreamingBuffer;
use insta::assert_snapshot;
use ratatui::{backend::TestBackend, Terminal};
use std::time::Instant;

// Test utilities (duplicated here since testing.rs is cfg(test) only within the crate)

const TEST_WIDTH: u16 = 80;
const TEST_HEIGHT: u16 = 24;

fn test_terminal() -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(TEST_WIDTH, TEST_HEIGHT)).unwrap()
}

struct TestStateBuilder {
    mode_id: String,
    input_buffer: String,
    cursor_position: usize,
    popup: Option<PopupState>,
    streaming_content: Option<String>,
    status_error: Option<String>,
}

impl TestStateBuilder {
    fn new(mode: &str) -> Self {
        Self {
            mode_id: mode.to_string(),
            input_buffer: String::new(),
            cursor_position: 0,
            popup: None,
            streaming_content: None,
            status_error: None,
        }
    }

    fn with_input(mut self, text: &str) -> Self {
        self.input_buffer = text.to_string();
        self.cursor_position = text.len();
        self
    }

    fn with_popup_items(mut self, kind: PopupKind, items: Vec<PopupItem>) -> Self {
        self.popup = Some(PopupState {
            kind,
            query: String::new(),
            items,
            selected: 0,
            viewport_offset: 0,
            last_update: Instant::now(),
        });
        self
    }

    fn with_popup_selected(mut self, index: usize) -> Self {
        if let Some(ref mut popup) = self.popup {
            popup.selected = index.min(popup.items.len().saturating_sub(1));
        }
        self
    }

    fn with_streaming(mut self, content: &str) -> Self {
        self.streaming_content = Some(content.to_string());
        self
    }

    fn with_error(mut self, error: &str) -> Self {
        self.status_error = Some(error.to_string());
        self
    }

    fn build(self) -> TuiState {
        let mut state = TuiState::new(&self.mode_id);
        state.input_buffer = self.input_buffer;
        state.cursor_position = self.cursor_position;
        state.popup = self.popup;
        state.status_error = self.status_error;

        if let Some(content) = self.streaming_content {
            let mut buf = StreamingBuffer::new();
            buf.append(&content);
            state.streaming = Some(buf);
        }

        state
    }
}

fn popup_item_command(name: &str, desc: &str) -> PopupItem {
    PopupItem {
        kind: PopupItemKind::Command,
        title: format!("/{}", name),
        subtitle: desc.to_string(),
        token: format!("/{} ", name),
        score: 0,
        available: true,
    }
}

fn popup_item_agent(id: &str, desc: &str) -> PopupItem {
    PopupItem {
        kind: PopupItemKind::Agent,
        title: format!("@{}", id),
        subtitle: desc.to_string(),
        token: format!("@{}", id),
        score: 0,
        available: true,
    }
}

fn popup_item_file(path: &str, subtitle: &str) -> PopupItem {
    PopupItem {
        kind: PopupItemKind::File,
        title: path.to_string(),
        subtitle: subtitle.to_string(),
        token: path.to_string(),
        score: 0,
        available: true,
    }
}

fn popup_item_note(path: &str, subtitle: &str) -> PopupItem {
    PopupItem {
        kind: PopupItemKind::Note,
        title: path.to_string(),
        subtitle: subtitle.to_string(),
        token: path.to_string(),
        score: 0,
        available: true,
    }
}

// =============================================================================
// Popup Visibility Tests
// =============================================================================

#[test]
fn popup_hidden_when_none() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("plan").with_input("/help").build(); // popup: None

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("popup_hidden", terminal.backend());
}

#[test]
fn popup_visible_with_commands() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("plan")
        .with_input("/")
        .with_popup_items(
            PopupKind::Command,
            vec![
                popup_item_command("help", "Show help information"),
                popup_item_command("exit", "Exit the chat"),
                popup_item_command("clear", "Clear the screen"),
            ],
        )
        .build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("popup_commands_visible", terminal.backend());
}

#[test]
fn popup_visible_with_agents_and_files() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("plan")
        .with_input("@")
        .with_popup_items(
            PopupKind::AgentOrFile,
            vec![
                popup_item_agent("dev-helper", "Developer assistant"),
                popup_item_agent("test-runner", "Test automation"),
                popup_item_file("src/main.rs", "workspace"),
            ],
        )
        .build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("popup_agents_files", terminal.backend());
}

// =============================================================================
// Popup Selection Tests
// =============================================================================

#[test]
fn popup_first_item_selected_by_default() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("plan")
        .with_input("/")
        .with_popup_items(
            PopupKind::Command,
            vec![
                popup_item_command("help", "Show help"),
                popup_item_command("exit", "Exit"),
            ],
        )
        .build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("popup_selection_first", terminal.backend());
}

#[test]
fn popup_second_item_selected() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("plan")
        .with_input("/")
        .with_popup_items(
            PopupKind::Command,
            vec![
                popup_item_command("help", "Show help"),
                popup_item_command("exit", "Exit"),
                popup_item_command("clear", "Clear screen"),
            ],
        )
        .with_popup_selected(1)
        .build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("popup_selection_second", terminal.backend());
}

#[test]
fn popup_last_item_selected() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("plan")
        .with_input("@")
        .with_popup_items(
            PopupKind::AgentOrFile,
            vec![
                popup_item_agent("agent1", "First agent"),
                popup_item_agent("agent2", "Second agent"),
                popup_item_file("README.md", "workspace"),
            ],
        )
        .with_popup_selected(2)
        .build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("popup_selection_last", terminal.backend());
}

// =============================================================================
// Popup Type Labels Tests
// =============================================================================

#[test]
fn popup_shows_mixed_type_labels() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("plan")
        .with_input("@")
        .with_popup_items(
            PopupKind::AgentOrFile,
            vec![
                popup_item_agent("helper", "AI Helper"),
                popup_item_file("README.md", "workspace"),
                popup_item_note("note:project/todo.md", "note"),
            ],
        )
        .build();

    terminal.draw(|f| render(f, &state)).unwrap();
    // Snapshot should show [agent], [file], [note] labels
    assert_snapshot!("popup_mixed_types", terminal.backend());
}

#[test]
fn popup_command_type_labels() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("act")
        .with_input("/s")
        .with_popup_items(
            PopupKind::Command,
            vec![
                popup_item_command("search", "Search files"),
                popup_item_command("stats", "Show statistics"),
            ],
        )
        .build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("popup_command_labels", terminal.backend());
}

// =============================================================================
// Popup Height/Truncation Tests
// =============================================================================

#[test]
fn popup_truncates_to_max_five_items() {
    let mut terminal = test_terminal();
    let items: Vec<_> = (0..10)
        .map(|i| popup_item_command(&format!("cmd{i}"), &format!("Command {i}")))
        .collect();

    let state = TestStateBuilder::new("plan")
        .with_input("/")
        .with_popup_items(PopupKind::Command, items)
        .build();

    terminal.draw(|f| render(f, &state)).unwrap();
    // Should only show 5 items (render.rs:30 has .min(5))
    assert_snapshot!("popup_max_five_items", terminal.backend());
}

#[test]
fn popup_single_item() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("plan")
        .with_input("/exit")
        .with_popup_items(PopupKind::Command, vec![popup_item_command("exit", "Exit")])
        .build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("popup_single_item", terminal.backend());
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
// Combined State Tests
// =============================================================================

#[test]
fn popup_with_streaming() {
    let mut terminal = test_terminal();
    let state = TestStateBuilder::new("act")
        .with_input("/")
        .with_popup_items(
            PopupKind::Command,
            vec![
                popup_item_command("help", "Show help"),
                popup_item_command("exit", "Exit"),
            ],
        )
        .with_streaming("Processing your request...")
        .build();

    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!("popup_with_streaming", terminal.backend());
}

// =============================================================================
// NEW: Conversation View Tests (target design)
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
    conv.set_status(StatusKind::Generating { token_count: 127, prev_token_count: 0, spinner_frame: 0 });

    render_conversation_view(&mut terminal, &conv, "", "act", Some(127), "Generating");
    assert_snapshot!("conv_generating_tokens", terminal.backend());
}

#[test]
fn conversation_tool_running() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("Find auth files");
    conv.push_assistant_message("Let me search for authentication-related files.");
    conv.push_tool_running("grep \"auth\" --type rs");

    render_conversation_view(&mut terminal, &conv, "", "act", None, "Tool running");
    assert_snapshot!("conv_tool_running", terminal.backend());
}

#[test]
fn conversation_tool_with_output() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("Find auth files");
    conv.push_tool_running("glob **/*auth*.rs");
    conv.update_tool_output(
        "glob **/*auth*.rs",
        "src/auth/mod.rs\nsrc/auth/jwt.rs\nsrc/auth/session.rs",
    );
    conv.complete_tool("glob **/*auth*.rs", Some("3 files".to_string()));

    render_conversation_view(&mut terminal, &conv, "", "plan", None, "Ready");
    assert_snapshot!("conv_tool_complete", terminal.backend());
}

#[test]
fn conversation_tool_error() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_tool_running("read /nonexistent");
    conv.error_tool("read /nonexistent", "file not found");

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
    conv.push_tool_running("grep pattern");
    conv.complete_tool("grep pattern", Some("12 matches".to_string()));

    // Second tool - running
    conv.push_tool_running("read src/main.rs");
    conv.update_tool_output(
        "read src/main.rs",
        "fn main() {\n    println!(\"Hello\");\n}",
    );

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

    conv.push_tool_running("grep auth");
    conv.complete_tool("grep auth", Some("found in 3 files".to_string()));

    conv.push_tool_running("glob **/*auth*.rs");
    conv.update_tool_output("glob **/*auth*.rs", "src/auth/mod.rs\nsrc/auth/jwt.rs");
    conv.complete_tool("glob **/*auth*.rs", Some("2 files".to_string()));

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

use crucible_cli::tui::content_block::ContentBlock;

#[test]
fn streaming_partial_prose() {
    let mut terminal = test_terminal();
    let mut conv = ConversationState::new();
    conv.push_user_message("Explain this");
    conv.start_assistant_streaming();
    conv.append_streaming_blocks(vec![ContentBlock::prose_partial("Hello, I'm here to hel")]);

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
        ContentBlock::prose("Here's the code:"),
        ContentBlock::code_partial(Some("rust".into()), "fn main() {\n    println!(\"Hel"),
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
        ContentBlock::prose("First paragraph complete."),
        ContentBlock::code(Some("rust".into()), "fn example() {}"),
        ContentBlock::prose_partial("Now continuin"),
    ]);

    render_conversation_view(&mut terminal, &conv, "", "act", Some(80), "Generating");
    assert_snapshot!("conv_streaming_mixed_blocks", terminal.backend());
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
    let mut terminal = test_terminal();
    let mut dialog = DialogState::confirm("Delete File", "This action cannot be undone.");
    // Move focus to cancel button
    dialog.focus_index = 1;

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
    let mut terminal = test_terminal();
    let dialog = DialogState::select(
        "Choose Mode",
        vec!["plan".into(), "act".into(), "auto".into()],
    );
    // Select second item
    let mut dialog = dialog;
    if let crucible_cli::tui::dialog::DialogKind::Select { selected, .. } = &mut dialog.kind {
        *selected = 1;
        dialog.focus_index = 1;
    }

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
