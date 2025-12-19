//! TUI Snapshot Tests
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
