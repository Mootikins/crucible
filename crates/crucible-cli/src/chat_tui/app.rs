//! ChatApp state management
//!
//! Contains the main application state for the chat TUI including mode,
//! render state, and dirty flag tracking.

use super::completion::{CompletionItem, CompletionState, CompletionType};
use super::input::{ChatAction, ChatInput};
use super::messages::{calculate_message_height, render_message, ChatMessageDisplay};
use ratatui::backend::Backend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Chat interaction mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatMode {
    /// Planning mode - agent explains before acting
    #[default]
    Plan,
    /// Action mode - agent acts immediately
    Act,
    /// Auto mode - agent decides
    Auto,
}

impl ChatMode {
    /// Get the display name for the mode
    pub fn name(&self) -> &'static str {
        match self {
            ChatMode::Plan => "plan",
            ChatMode::Act => "act",
            ChatMode::Auto => "auto",
        }
    }
}

/// Tracks what needs to be re-rendered
#[derive(Debug, Default)]
pub struct RenderState {
    dirty: Arc<AtomicBool>,
}

impl RenderState {
    /// Create a new render state
    pub fn new() -> Self {
        Self {
            dirty: Arc::new(AtomicBool::new(true)), // Initial render needed
        }
    }

    /// Check if re-render is needed
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::SeqCst)
    }

    /// Mark as needing re-render
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Clear dirty flag after render
    pub fn clear(&self) {
        self.dirty.store(false, Ordering::SeqCst);
    }
}

impl Clone for RenderState {
    fn clone(&self) -> Self {
        Self {
            dirty: Arc::clone(&self.dirty),
        }
    }
}

/// Main chat application state
pub struct ChatApp {
    /// Current chat mode
    pub mode: ChatMode,

    /// Text input state
    pub input: ChatInput,

    /// Active completion state (if showing popup)
    pub completion: Option<CompletionState>,

    /// Render state for dirty tracking
    pub render_state: RenderState,

    /// Whether agent is currently streaming a response
    pub is_streaming: bool,

    /// Whether to exit the event loop
    pub should_exit: bool,
}

impl ChatApp {
    /// Create a new chat application
    pub fn new() -> Self {
        Self {
            mode: ChatMode::default(),
            input: ChatInput::new(),
            completion: None,
            render_state: RenderState::new(),
            is_streaming: false,
            should_exit: false,
        }
    }

    /// Set the chat mode
    pub fn set_mode(&mut self, mode: ChatMode) {
        if self.mode != mode {
            self.mode = mode;
            self.render_state.mark_dirty();
        }
    }

    /// Set streaming state
    pub fn set_streaming(&mut self, streaming: bool) {
        if self.is_streaming != streaming {
            self.is_streaming = streaming;
            self.render_state.mark_dirty();
        }
    }

    /// Request exit
    pub fn request_exit(&mut self) {
        self.should_exit = true;
    }

    /// Check if render is needed
    pub fn needs_render(&self) -> bool {
        self.render_state.is_dirty()
    }

    /// Submit the current input, returning the message if non-empty
    pub fn submit_input(&mut self) -> Option<String> {
        let content = self.input.content();
        if !content.trim().is_empty() {
            self.input.clear();
            self.render_state.mark_dirty();
            Some(content)
        } else {
            None
        }
    }

    /// Input mode for key handling
    pub fn input_mode(&self) -> InputMode {
        if self.completion.is_some() {
            InputMode::Completion
        } else {
            InputMode::Normal
        }
    }

    /// Handle a key event, returning any message to send
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<String> {
        // Global bindings (always active)
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if self.completion.is_some() {
                self.cancel_completion();
            } else {
                self.request_exit();
            }
            return None;
        }

        match self.input_mode() {
            InputMode::Normal => self.handle_normal_key(key),
            InputMode::Completion => {
                self.handle_completion_key(key);
                None
            }
        }
    }

    /// Handle key in normal input mode
    fn handle_normal_key(&mut self, key: KeyEvent) -> Option<String> {
        let action = self.input.handle_key(key);
        self.render_state.mark_dirty();

        match action {
            ChatAction::Send(msg) => Some(msg),
            ChatAction::TriggerCommandCompletion => {
                self.show_command_completion();
                None
            }
            ChatAction::TriggerFileCompletion => {
                self.show_file_completion();
                None
            }
            ChatAction::None => None,
        }
    }

    /// Handle key in completion mode
    fn handle_completion_key(&mut self, key: KeyEvent) {
        let Some(completion) = self.completion.as_mut() else {
            return;
        };

        match key.code {
            // Navigation
            KeyCode::Up | KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                completion.select_prev();
            }
            KeyCode::Down | KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                completion.select_next();
            }
            KeyCode::Up => completion.select_prev(),
            KeyCode::Down => completion.select_next(),

            // Selection
            KeyCode::Enter | KeyCode::Tab => {
                self.confirm_completion();
                return;
            }
            KeyCode::Char(' ') if completion.multi_select => {
                completion.toggle_selection();
            }

            // Cancel
            KeyCode::Esc => {
                self.cancel_completion();
                return;
            }

            // Filter - pass characters to query
            KeyCode::Char(c) => {
                completion.query.push(c);
                completion.refilter();
            }
            KeyCode::Backspace => {
                if completion.query.pop().is_none() {
                    self.cancel_completion();
                    return;
                }
                completion.refilter();
            }

            _ => {}
        }

        self.render_state.mark_dirty();
    }

    /// Show command completion popup
    pub fn show_command_completion(&mut self) {
        // Placeholder - will be populated by completion sources later
        let items = vec![
            CompletionItem::new("clear", Some("Clear conversation".into()), CompletionType::Command),
            CompletionItem::new("help", Some("Show help".into()), CompletionType::Command),
            CompletionItem::new("mode", Some("Change mode".into()), CompletionType::Command),
            CompletionItem::new("exit", Some("Exit chat".into()), CompletionType::Command),
        ];
        self.completion = Some(CompletionState::new(items, CompletionType::Command));
        self.render_state.mark_dirty();
    }

    /// Show file completion popup
    pub fn show_file_completion(&mut self) {
        // Placeholder - will be populated by file sources later
        let items = vec![
            CompletionItem::new("README.md", None, CompletionType::File),
            CompletionItem::new("CLAUDE.md", None, CompletionType::File),
        ];
        let mut state = CompletionState::new(items, CompletionType::File);
        state.multi_select = true; // Files support multi-select
        self.completion = Some(state);
        self.render_state.mark_dirty();
    }

    /// Confirm the current completion selection
    fn confirm_completion(&mut self) {
        if let Some(completion) = self.completion.take() {
            // Get selected item(s) and insert into input
            let items = completion.selected_items();

            // Clear the partial query from input
            self.input.clear();

            // Insert all selected items
            let trigger = match completion.completion_type {
                CompletionType::Command => "/",
                CompletionType::File | CompletionType::Agent => "@",
            };

            for item in items {
                self.input.insert_str(&format!("{}{} ", trigger, item.text));
            }
        }
        self.render_state.mark_dirty();
    }

    /// Cancel the completion popup
    fn cancel_completion(&mut self) {
        self.completion = None;
        self.render_state.mark_dirty();
    }

    /// Insert a message into the scrollback (call from event loop)
    ///
    /// This pushes a chat message into the terminal scrollback buffer using
    /// `terminal.insert_before()`, which preserves the inline viewport at the
    /// bottom while allowing the user to scroll up to see message history.
    ///
    /// # Arguments
    /// * `terminal` - The ratatui terminal instance
    /// * `msg` - The message to display
    ///
    /// # Returns
    /// `Ok(())` on success, or an IO error if rendering fails
    ///
    /// # Example
    /// ```no_run
    /// use crucible_cli::chat_tui::{ChatApp, ChatMessageDisplay, MessageRole};
    /// use ratatui::backend::TestBackend;
    /// use ratatui::Terminal;
    ///
    /// let mut app = ChatApp::new();
    /// let backend = TestBackend::new(80, 24);
    /// let mut terminal = Terminal::new(backend).unwrap();
    ///
    /// let msg = ChatMessageDisplay::user("Hello!");
    /// ChatApp::insert_message(&mut terminal, &msg).unwrap();
    /// ```
    pub fn insert_message<B: Backend>(
        terminal: &mut Terminal<B>,
        msg: &ChatMessageDisplay,
    ) -> io::Result<()> {
        let width = terminal.size()?.width;
        let height = calculate_message_height(msg, width);

        terminal.insert_before(height, |buf| {
            render_message(buf, msg);
        })
    }
}

/// Input mode for key handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal text input
    Normal,
    /// Completion popup active
    Completion,
}

impl Default for ChatApp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_mode_names() {
        assert_eq!(ChatMode::Plan.name(), "plan");
        assert_eq!(ChatMode::Act.name(), "act");
        assert_eq!(ChatMode::Auto.name(), "auto");
    }

    #[test]
    fn test_render_state_dirty_tracking() {
        let state = RenderState::new();

        // Initially dirty (needs first render)
        assert!(state.is_dirty());

        // Clear after render
        state.clear();
        assert!(!state.is_dirty());

        // Mark dirty on state change
        state.mark_dirty();
        assert!(state.is_dirty());
    }

    #[test]
    fn test_chat_app_mode_change_marks_dirty() {
        let mut app = ChatApp::new();
        app.render_state.clear();
        assert!(!app.render_state.is_dirty());

        app.set_mode(ChatMode::Act);
        assert!(app.render_state.is_dirty());
    }

    #[test]
    fn test_chat_app_streaming_marks_dirty() {
        let mut app = ChatApp::new();
        app.render_state.clear();

        app.set_streaming(true);
        assert!(app.render_state.is_dirty());
    }

    #[test]
    fn test_input_mode_normal_by_default() {
        let app = ChatApp::new();
        assert_eq!(app.input_mode(), InputMode::Normal);
    }

    #[test]
    fn test_input_mode_completion_when_popup_active() {
        let mut app = ChatApp::new();
        app.show_command_completion();
        assert_eq!(app.input_mode(), InputMode::Completion);
    }

    #[test]
    fn test_command_completion_trigger() {
        let mut app = ChatApp::new();

        // Type '/' to trigger completion
        let key = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        let result = app.handle_key(key);

        assert!(result.is_none());
        assert!(app.completion.is_some());
        assert_eq!(app.input_mode(), InputMode::Completion);
    }

    #[test]
    fn test_file_completion_trigger() {
        let mut app = ChatApp::new();

        // Type '@' to trigger file completion
        let key = KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE);
        let result = app.handle_key(key);

        assert!(result.is_none());
        assert!(app.completion.is_some());
        let completion = app.completion.as_ref().unwrap();
        assert!(completion.multi_select); // Files support multi-select
    }

    #[test]
    fn test_completion_navigation() {
        let mut app = ChatApp::new();
        app.show_command_completion();

        // Navigate down
        let key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        app.handle_key(key);

        let completion = app.completion.as_ref().unwrap();
        assert_eq!(completion.selected_index, 1);

        // Navigate up
        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        app.handle_key(key);

        let completion = app.completion.as_ref().unwrap();
        assert_eq!(completion.selected_index, 0);
    }

    #[test]
    fn test_completion_cancel_on_escape() {
        let mut app = ChatApp::new();
        app.show_command_completion();
        assert!(app.completion.is_some());

        // Press Escape
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        app.handle_key(key);

        assert!(app.completion.is_none());
    }

    #[test]
    fn test_completion_filter() {
        let mut app = ChatApp::new();
        app.show_command_completion();

        // Type 'c' to filter
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
        app.handle_key(key);

        let completion = app.completion.as_ref().unwrap();
        assert_eq!(completion.query, "c");
        // Should filter to "clear" primarily
        assert!(!completion.filtered_items.is_empty());
    }

    #[test]
    fn test_ctrl_c_exits_in_normal_mode() {
        let mut app = ChatApp::new();
        assert!(!app.should_exit);

        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        app.handle_key(key);

        assert!(app.should_exit);
    }

    #[test]
    fn test_ctrl_c_cancels_completion() {
        let mut app = ChatApp::new();
        app.show_command_completion();
        assert!(app.completion.is_some());

        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        app.handle_key(key);

        // Should cancel completion, not exit
        assert!(app.completion.is_none());
        assert!(!app.should_exit);
    }

    #[test]
    fn test_submit_message() {
        let mut app = ChatApp::new();

        // Type a message
        app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));

        // Submit with Enter
        let result = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(result, Some("hi".to_string()));
        assert!(app.input.content().is_empty()); // Input cleared after send
    }

    // Comprehensive completion navigation tests (T11)

    #[test]
    fn test_completion_nav_down_wraps() {
        let mut app = ChatApp::new();
        app.show_command_completion();

        let completion = app.completion.as_ref().unwrap();
        let item_count = completion.filtered_items.len();
        assert!(item_count > 0, "Need items to test wrapping");

        // Navigate to last item
        for _ in 0..item_count - 1 {
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }

        let completion = app.completion.as_ref().unwrap();
        assert_eq!(completion.selected_index, item_count - 1);

        // One more down should wrap to first
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        let completion = app.completion.as_ref().unwrap();
        assert_eq!(completion.selected_index, 0, "Down at last item should wrap to first");
    }

    #[test]
    fn test_completion_nav_up_wraps() {
        let mut app = ChatApp::new();
        app.show_command_completion();

        let completion = app.completion.as_ref().unwrap();
        let item_count = completion.filtered_items.len();
        assert!(item_count > 0, "Need items to test wrapping");

        // Start at first item (index 0)
        assert_eq!(completion.selected_index, 0);

        // Up arrow at first item should wrap to last
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));

        let completion = app.completion.as_ref().unwrap();
        assert_eq!(completion.selected_index, item_count - 1, "Up at first item should wrap to last");
    }

    #[test]
    fn test_completion_nav_ctrl_j_k() {
        let mut app = ChatApp::new();
        app.show_command_completion();

        // Initially at index 0
        let completion = app.completion.as_ref().unwrap();
        assert_eq!(completion.selected_index, 0);

        // Ctrl+J moves down
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
        let completion = app.completion.as_ref().unwrap();
        assert_eq!(completion.selected_index, 1, "Ctrl+J should move down");

        // Ctrl+J again
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
        let completion = app.completion.as_ref().unwrap();
        assert_eq!(completion.selected_index, 2, "Ctrl+J should move down again");

        // Ctrl+K moves up
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
        let completion = app.completion.as_ref().unwrap();
        assert_eq!(completion.selected_index, 1, "Ctrl+K should move up");

        // Ctrl+K again
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
        let completion = app.completion.as_ref().unwrap();
        assert_eq!(completion.selected_index, 0, "Ctrl+K should move up again");
    }

    #[test]
    fn test_completion_confirm_enter() {
        let mut app = ChatApp::new();
        app.show_command_completion();

        // Select second item
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        let completion = app.completion.as_ref().unwrap();
        let selected_text = completion.selected_item().unwrap().text.clone();

        // Confirm with Enter
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // Completion should be closed
        assert!(app.completion.is_none(), "Completion should close after Enter");

        // Input should contain the selected item
        let input = app.input.content();
        assert!(input.contains(&selected_text), "Input should contain selected item: '{}' in '{}'", selected_text, input);
    }

    #[test]
    fn test_completion_confirm_tab() {
        let mut app = ChatApp::new();
        app.show_command_completion();

        // Select first item
        let completion = app.completion.as_ref().unwrap();
        let selected_text = completion.selected_item().unwrap().text.clone();

        // Confirm with Tab
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

        // Completion should be closed
        assert!(app.completion.is_none(), "Completion should close after Tab");

        // Input should contain the selected item
        let input = app.input.content();
        assert!(input.contains(&selected_text), "Input should contain selected item: '{}' in '{}'", selected_text, input);
    }

    #[test]
    fn test_completion_filter_updates_selection() {
        let mut app = ChatApp::new();
        app.show_command_completion();

        // Navigate down a few items
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        let completion = app.completion.as_ref().unwrap();
        assert_eq!(completion.selected_index, 2, "Should be at index 2 before filtering");

        // Type a filter character
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));

        // Selection should reset to 0 after filtering
        let completion = app.completion.as_ref().unwrap();
        assert_eq!(completion.selected_index, 0, "Filtering should reset selection to 0");
        assert_eq!(completion.query, "c", "Query should be updated");
    }

    #[test]
    fn test_completion_insert_into_input() {
        let mut app = ChatApp::new();

        // Trigger command completion
        app.show_command_completion();

        // Select "help" command (navigate to find it)
        let mut found_help = false;
        for _ in 0..10 {
            let completion = app.completion.as_ref().unwrap();
            if let Some(item) = completion.selected_item() {
                if item.text == "help" {
                    found_help = true;
                    break;
                }
            }
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        assert!(found_help, "Should find 'help' command in completion list");

        // Confirm selection
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // Check that input contains the completed command with trigger
        let input = app.input.content();
        assert!(input.contains("/help"), "Input should contain '/help' after completion: '{}'", input);

        // Check that completion is closed
        assert!(app.completion.is_none(), "Completion should be closed after confirmation");
    }

    #[test]
    fn test_completion_nav_empty_list() {
        let mut app = ChatApp::new();
        app.show_command_completion();

        // Filter to get empty list
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));

        let completion = app.completion.as_ref().unwrap();
        if completion.filtered_items.is_empty() {
            // Navigation should not panic on empty list
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
            app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));

            // Should still have completion state
            assert!(app.completion.is_some(), "Completion should remain active even with empty list");
        }
    }

    #[test]
    fn test_completion_nav_single_item() {
        let mut app = ChatApp::new();
        app.show_command_completion();

        // Filter to get single item (e.g., "clear")
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

        let completion = app.completion.as_ref().unwrap();
        if completion.filtered_items.len() == 1 {
            // Navigation on single item should keep selection at 0
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
            let completion = app.completion.as_ref().unwrap();
            assert_eq!(completion.selected_index, 0, "Single item: down should wrap to 0");

            app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
            let completion = app.completion.as_ref().unwrap();
            assert_eq!(completion.selected_index, 0, "Single item: up should wrap to 0");
        }
    }

    // Comprehensive multi-select checkbox tests (T12)

    #[test]
    fn test_multi_select_space_toggles() {
        let mut app = ChatApp::new();

        // Trigger file completion (which is multi-select)
        app.handle_key(KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE));

        assert!(app.completion.is_some());
        let completion = app.completion.as_ref().unwrap();
        assert!(completion.multi_select, "File completion should be multi-select");

        // Initially nothing selected
        assert!(!completion.is_selected(0), "First item should not be selected initially");

        // Press Space to toggle selection
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

        let completion = app.completion.as_ref().unwrap();
        assert!(completion.is_selected(0), "First item should be selected after Space");

        // Press Space again to toggle off
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

        let completion = app.completion.as_ref().unwrap();
        assert!(!completion.is_selected(0), "First item should be deselected after second Space");
    }

    #[test]
    fn test_multi_select_confirm_returns_all_selected() {
        let mut app = ChatApp::new();

        // Trigger file completion
        app.handle_key(KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE));

        let completion = app.completion.as_ref().unwrap();
        assert!(completion.multi_select, "File completion should be multi-select");
        let item_count = completion.filtered_items.len();
        assert!(item_count >= 2, "Need at least 2 items to test multi-select");

        // Select first item
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

        // Navigate to second item and select it
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

        let completion = app.completion.as_ref().unwrap();
        assert!(completion.is_selected(0), "First item should be selected");
        assert!(completion.is_selected(1), "Second item should be selected");

        // Get selected items before confirmation
        let selected_items: Vec<String> = completion
            .selected_items()
            .iter()
            .map(|item| item.text.clone())
            .collect();
        assert_eq!(selected_items.len(), 2, "Should have 2 selected items");

        // Confirm with Enter
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // Completion should be closed
        assert!(app.completion.is_none(), "Completion should close after Enter");

        // Input should contain both selected files
        let input = app.input.content();
        for item_text in &selected_items {
            assert!(
                input.contains(item_text),
                "Input should contain selected item '{}': '{}'",
                item_text,
                input
            );
        }
    }

    #[test]
    fn test_multi_select_not_available_for_commands() {
        let mut app = ChatApp::new();

        // Trigger command completion
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));

        assert!(app.completion.is_some());
        let completion = app.completion.as_ref().unwrap();
        assert!(!completion.multi_select, "Command completion should NOT be multi-select");
        assert_eq!(completion.completion_type, CompletionType::Command);

        // Space should not toggle selection in single-select mode
        let initial_selections = completion.selections.len();

        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

        let completion = app.completion.as_ref().unwrap();
        // In single-select mode, space is treated as a filter character
        assert_eq!(
            completion.query, " ",
            "Space should be added to query in single-select mode"
        );
        assert_eq!(
            completion.selections.len(),
            initial_selections,
            "Selections should not change in single-select mode"
        );
    }

    #[test]
    fn test_file_completion_is_multi_select() {
        let mut app = ChatApp::new();

        // Trigger file completion with @
        app.show_file_completion();

        assert!(app.completion.is_some());
        let completion = app.completion.as_ref().unwrap();
        assert!(completion.multi_select, "File completion should have multi_select = true");
        assert_eq!(completion.completion_type, CompletionType::File);
    }

    #[test]
    fn test_multi_select_multiple_toggles() {
        let mut app = ChatApp::new();

        // Trigger file completion
        app.handle_key(KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE));

        let completion = app.completion.as_ref().unwrap();
        let item_count = completion.filtered_items.len();
        assert!(item_count >= 2, "Need at least 2 items for this test");

        // Select first item
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(app.completion.as_ref().unwrap().is_selected(0));

        // Move down and select second item
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(app.completion.as_ref().unwrap().is_selected(1));

        // Both should still be selected
        let completion = app.completion.as_ref().unwrap();
        assert!(completion.is_selected(0), "First item should remain selected");
        assert!(completion.is_selected(1), "Second item should be selected");

        // Move back to first and deselect
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

        let completion = app.completion.as_ref().unwrap();
        assert!(!completion.is_selected(0), "First item should be deselected");
        assert!(completion.is_selected(1), "Second item should still be selected");
    }

    #[test]
    fn test_multi_select_preserves_selections_during_navigation() {
        let mut app = ChatApp::new();

        // Trigger file completion
        app.handle_key(KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE));

        // Select first item
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

        // Navigate around
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));

        // First item should still be selected
        let completion = app.completion.as_ref().unwrap();
        assert!(
            completion.is_selected(0),
            "Selection should persist during navigation"
        );
    }
}
