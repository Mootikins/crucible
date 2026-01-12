//! Test harness for TUI testing
//!
//! Provides a simulated TUI environment for testing component behavior
//! without a real terminal.

use crate::tui::components::generic_popup::PopupState;
use crate::tui::content_block::ParseEvent;
use crate::tui::conversation::ConversationItem;
use crate::tui::conversation_view::{ConversationView, RatatuiView};
use crate::tui::popup::PopupProvider;
use crate::tui::state::types::{PopupItem, PopupKind};
use crate::tui::state::TuiState;
use crate::tui::streaming_channel::StreamingEvent;
use crate::tui::streaming_parser::StreamingParser;
use crate::tui::StreamBlock;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::collections::VecDeque;
use std::sync::Arc;

/// A static provider that returns a fixed list of items
struct StaticItemsProvider {
    kind: PopupKind,
    items: Vec<PopupItem>,
}

impl PopupProvider for StaticItemsProvider {
    fn provide(&self, kind: PopupKind, _query: &str) -> Vec<PopupItem> {
        if kind == self.kind {
            self.items.clone()
        } else {
            Vec::new()
        }
    }
}

/// An empty provider for testing (returns no items)
struct EmptyProvider;

impl PopupProvider for EmptyProvider {
    fn provide(&self, _kind: PopupKind, _query: &str) -> Vec<PopupItem> {
        Vec::new()
    }
}

/// Test harness for TUI components
///
/// Provides a simulated environment for testing TUI behavior:
/// - Simulates key presses and input
/// - Injects streaming events
/// - Captures rendered output for snapshot testing
///
/// # Single Source of Truth
///
/// The view owns ALL state (conversation, dimensions, etc.).
/// This ensures tests exercise the same code paths as production.
/// Dimensions are accessed via `view.state().width` / `view.state().height`.
///
/// # Example
///
/// ```ignore
/// let mut h = Harness::new(80, 24);
/// h.keys("hello");
/// assert_eq!(h.input_text(), "hello");
///
/// h.key(KeyCode::Char('/'));
/// assert!(h.popup().is_some());
/// ```
pub struct Harness {
    /// Main TUI state (input, popup, etc.)
    pub state: TuiState,
    /// Ratatui view (owns ALL state - conversation, dimensions, etc.)
    pub view: RatatuiView,
    /// Streaming parser (same as production runner uses)
    streaming_parser: StreamingParser,
}

impl Default for Harness {
    fn default() -> Self {
        Self::new(80, 24)
    }
}

impl Harness {
    /// Create a new harness with given dimensions
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            state: TuiState::new("plan"),
            view: RatatuiView::new("plan", width, height),
            streaming_parser: StreamingParser::new(),
        }
    }

    /// Get viewport width (from view state - single source of truth)
    pub fn width(&self) -> u16 {
        self.view.state().width
    }

    /// Get viewport height (from view state - single source of truth)
    pub fn height(&self) -> u16 {
        self.view.state().height
    }

    /// Builder: set initial conversation
    ///
    /// Populates the view's conversation state (single source of truth).
    pub fn with_session(mut self, items: Vec<ConversationItem>) -> Self {
        for item in items {
            self.view.state_mut().conversation.push(item);
        }
        self
    }

    /// Resize the terminal viewport
    ///
    /// Updates the view's dimensions (single source of truth).
    pub fn resize(&mut self, width: u16, height: u16) {
        let _ = self.view.handle_resize(width, height);
    }

    /// Builder: set popup items
    ///
    /// Uses PopupState with PopupRenderer for proper rendering.
    /// Also sets the input buffer to the trigger character (/, @, or :) so that
    /// subsequent typing preserves the popup.
    pub fn with_popup_items(mut self, kind: PopupKind, items: Vec<PopupItem>) -> Self {
        // Create a static provider with the given items
        let provider = Arc::new(StaticItemsProvider {
            kind,
            items: items.clone(),
        });

        // Create PopupState and populate it
        let mut popup = PopupState::new(kind, provider);
        popup.update_query(""); // Load items from provider

        // Set on the view
        self.view.set_popup(Some(popup));
        self.sync_popup_to_state(); // Sync to state's view

        // Set input buffer to trigger character so update_popup() keeps it open
        let trigger = match kind {
            PopupKind::Command => "/",
            PopupKind::AgentOrFile => "@",
            PopupKind::ReplCommand => ":",
            PopupKind::Session => "/resume", // Session popup is triggered by /resume command
            PopupKind::Model => ":model",    // Model popup is triggered by :model command
        };
        *self.state.input_mut() = trigger.to_string();
        self.state.set_cursor(trigger.len());

        self
    }

    // =========================================================================
    // Input simulation
    // =========================================================================

    /// Simulate a key press
    pub fn key(&mut self, code: KeyCode) {
        self.key_with_modifiers(code, KeyModifiers::NONE);
    }

    /// Simulate a key press with modifiers
    pub fn key_with_modifiers(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let event = KeyEvent::new(code, modifiers);
        self.handle_key_event(event);
    }

    /// Simulate Ctrl+key
    pub fn key_ctrl(&mut self, c: char) {
        self.key_with_modifiers(KeyCode::Char(c), KeyModifiers::CONTROL);
    }

    /// Simulate Alt+key
    pub fn key_alt(&mut self, c: char) {
        self.key_with_modifiers(KeyCode::Char(c), KeyModifiers::ALT);
        // Handle Alt+T for reasoning toggle
        // Note: show_reasoning is now accessed via a method
        // The actual toggle will be handled by InputAction::ToggleReasoning
        // This manual toggle may need to be removed or updated
        if c == 't' {
            // self.state.show_reasoning = !self.state.show_reasoning; // No longer directly accessible
            // The toggle will be handled through the action system
        }
    }

    /// Type a string (simulates key-by-key input)
    pub fn keys(&mut self, input: &str) {
        for c in input.chars() {
            self.key(KeyCode::Char(c));
        }
    }

    /// Handle a key event (internal)
    fn handle_key_event(&mut self, event: KeyEvent) {
        // Handle Alt+T for reasoning toggle
        if event.modifiers.contains(KeyModifiers::ALT) {
            if let KeyCode::Char('t') = event.code {
                self.state.set_show_reasoning(!self.state.show_reasoning());
                return;
            }
        }

        // Handle Ctrl modifiers first
        if event.modifiers.contains(KeyModifiers::CONTROL) {
            match event.code {
                KeyCode::Char('a') => {
                    self.state.set_cursor(0);
                    return;
                }
                KeyCode::Char('e') => {
                    self.state.set_cursor(self.state.input().len());
                    return;
                }
                KeyCode::Char('u') => {
                    let cursor_pos = self.state.cursor();
                    self.state.input_mut().drain(..cursor_pos);
                    self.state.set_cursor(0);
                    return;
                }
                KeyCode::Char('k') => {
                    let cursor_pos = self.state.cursor();
                    self.state.input_mut().truncate(cursor_pos);
                    return;
                }
                KeyCode::Char('w') => {
                    let cursor_pos = self.state.cursor();
                    let new_pos = crate::tui::state::find_word_start_backward(
                        &self.state.input()[..cursor_pos],
                    );
                    self.state.input_mut().drain(new_pos..cursor_pos);
                    self.state.set_cursor(new_pos);
                    return;
                }
                _ => {}
            }
        }

        // Handle popup navigation (Esc, Up, Down, Enter) - these don't modify input
        // Char and Backspace fall through to normal handling which updates input_buffer
        // and then update_popup() derives the query from input (matches runner behavior)
        if self.state.has_popup() {
            match event.code {
                KeyCode::Esc => {
                    // Matches runner Cancel behavior: clear input AND close popup
                    self.view.set_popup(None);
                    self.sync_popup_to_state(); // Sync popup state
                    self.state.input_mut().clear();
                    self.state.set_cursor(0);
                    return;
                }
                KeyCode::Up => {
                    if let Some(popup) = self.view.popup_mut() {
                        popup.move_selection(-1);
                    }
                    self.sync_popup_to_state();
                    return;
                }
                KeyCode::Down => {
                    if let Some(popup) = self.view.popup_mut() {
                        popup.move_selection(1);
                    }
                    self.sync_popup_to_state();
                    return;
                }
                KeyCode::Enter => {
                    if let Some(popup) = self.view.popup_mut() {
                        if let Some(item) = popup.selected_item() {
                            let token = item.token();
                            self.view.set_popup(None);
                            self.sync_popup_to_state(); // Sync popup state
                                                        // Token already includes trailing space for most items
                            *self.state.input_mut() = token;
                            self.state.set_cursor(self.state.input().len());
                        }
                    }
                    return;
                }
                // Char and Backspace fall through to normal handling
                _ => {}
            }
        }

        // Normal input handling
        match event.code {
            KeyCode::Char(c) => {
                let cursor_pos = self.state.cursor();
                self.state.input_mut().insert(cursor_pos, c);
                self.state.set_cursor(cursor_pos + c.len_utf8());
                // Update popup based on input prefix (matches runner behavior)
                self.update_popup();
            }
            KeyCode::Backspace => {
                if self.state.cursor() > 0 {
                    let prev_char_boundary = self.state.input()[..self.state.cursor()]
                        .char_indices()
                        .last()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.state.input_mut().remove(prev_char_boundary);
                    self.state.set_cursor(prev_char_boundary);
                    // Update popup after deletion (matches runner behavior)
                    self.update_popup();
                }
            }
            KeyCode::Left => {
                if self.state.cursor() > 0 {
                    self.state.set_cursor(
                        self.state.input()[..self.state.cursor()]
                            .char_indices()
                            .last()
                            .map(|(i, _)| i)
                            .unwrap_or(0),
                    );
                }
            }
            KeyCode::Right => {
                if self.state.cursor() < self.state.input().len() {
                    self.state.set_cursor(
                        self.state.input()[self.state.cursor()..]
                            .char_indices()
                            .nth(1)
                            .map(|(i, _)| self.state.cursor() + i)
                            .unwrap_or(self.state.input().len()),
                    );
                }
            }
            _ => {}
        }
    }

    /// Update popup based on current input (matches TuiRunner::update_popup behavior)
    fn update_popup(&mut self) {
        let trimmed = self.state.input().trim_start();

        // Detect popup trigger prefix and corresponding kind
        let trigger = [
            ('/', PopupKind::Command),
            ('@', PopupKind::AgentOrFile),
            (':', PopupKind::ReplCommand),
        ]
        .into_iter()
        .find(|(prefix, _)| trimmed.starts_with(*prefix));

        if let Some((prefix, kind)) = trigger {
            let query = trimmed.strip_prefix(prefix).unwrap_or("").to_string();
            // Create popup if needed (wrong kind or none)
            if !self.state.has_popup() || self.view.popup().map(|p| p.kind()) != Some(kind) {
                let popup =
                    PopupState::new(kind, Arc::new(EmptyProvider) as Arc<dyn PopupProvider>);
                self.view.set_popup(Some(popup));
                self.sync_popup_to_state();
            }
            if let Some(popup) = self.view.popup_mut() {
                popup.update_query(&query);
                // Sync after updating query
                self.sync_popup_to_state();
            }
        } else {
            self.view.set_popup(None);
            self.sync_popup_to_state();
        }
    }

    /// Sync popup state from view to state
    /// This is needed because the harness has two separate ViewState objects
    fn sync_popup_to_state(&mut self) {
        // Sync popup by using EmptyProvider and maintaining query/selection state
        if let Some(view_popup) = self.view.popup() {
            let kind = view_popup.kind();
            let query = view_popup.query();

            // Use EmptyProvider for both ViewStates in the harness
            let provider = Arc::new(EmptyProvider) as Arc<dyn PopupProvider>;

            // Create new popup and sync its state
            let mut new_popup = PopupState::new(kind, provider);
            new_popup.update_query(query);

            // Sync selection by moving from 0 to current index
            let current_idx = view_popup.selected_index();
            if current_idx > 0 {
                new_popup.move_selection(current_idx as isize);
            }

            self.state.view.popup = Some(new_popup);
        } else {
            self.state.view.popup = None;
        }
    }

    // =========================================================================
    // Event injection
    // =========================================================================

    /// Inject a streaming event
    ///
    /// For Delta events, uses the same streaming parser as the production runner
    /// to ensure tests exercise the actual code path including code fence detection.
    pub fn event(&mut self, event: StreamingEvent) {
        match event {
            StreamingEvent::Delta { text, seq } => {
                self.state.last_seen_seq = seq;
                // Use streaming parser - SAME as production runner
                let parse_events = self.streaming_parser.feed(&text);
                self.apply_parse_events(parse_events);
            }
            StreamingEvent::Done { full_response: _ } => {
                // Finalize parser to flush any remaining content - SAME as production runner
                let final_events = self.streaming_parser.finalize();
                self.apply_parse_events(final_events);
                // Reset parser for next streaming session
                self.streaming_parser = StreamingParser::new();

                self.view.complete_assistant_streaming();
                // Clear reasoning buffer when response completes
                self.state.clear_reasoning();
            }
            StreamingEvent::Error { message } => {
                self.state.status_error = Some(message);
            }
            StreamingEvent::ToolCall { name, args, .. } => {
                // Track in state
                self.state
                    .pending_tools
                    .push(crate::tui::state::ToolCallInfo {
                        name: name.clone(),
                        args: args.clone(),
                        call_id: None,
                        completed: false,
                        result: None,
                        error: None,
                    });
                // Also push to view for rendering
                self.view.push_tool_running(&name, args);
            }
            StreamingEvent::ToolCompleted {
                name,
                result,
                error,
            } => {
                // Mark matching tool as completed in state
                if let Some(tool) = self
                    .state
                    .pending_tools
                    .iter_mut()
                    .find(|t| t.name == name && !t.completed)
                {
                    tool.completed = true;
                    tool.result = Some(result.clone());
                    tool.error = error.clone();
                }
                // Update view with completion
                if let Some(err) = &error {
                    self.view.error_tool(&name, err);
                } else {
                    let summary = if result.len() > 50 {
                        Some(format!("{}...", &result[..47]))
                    } else if !result.is_empty() {
                        Some(result)
                    } else {
                        None
                    };
                    self.view.complete_tool(&name, summary);
                }
            }
            StreamingEvent::Reasoning { text, seq } => {
                // Accumulate reasoning in state for display
                self.state.append_reasoning(&text);
                self.state.last_seen_seq = seq;
            }
        }
    }

    /// Inject multiple events in sequence
    pub fn events(&mut self, events: Vec<StreamingEvent>) {
        for event in events {
            self.event(event);
        }
    }

    /// Apply parse events to the conversation view.
    ///
    /// This is the SAME logic as the production runner's apply_parse_events,
    /// ensuring tests exercise the real code path for code blocks, prose, etc.
    fn apply_parse_events(&mut self, events: Vec<ParseEvent>) {
        for event in events {
            match event {
                ParseEvent::Text(text) => {
                    // Append to existing prose block if possible, otherwise create new
                    self.view.append_or_create_prose(&text);
                }
                ParseEvent::CodeBlockStart { lang } => {
                    // Start a new partial code block
                    self.view
                        .append_streaming_blocks(vec![StreamBlock::code_partial(lang, "")]);
                }
                ParseEvent::CodeBlockContent(content) => {
                    // Append to the existing code block in the view
                    self.view.append_to_last_block(&content);
                }
                ParseEvent::CodeBlockEnd => {
                    // Mark the code block as complete
                    self.view.complete_last_block();
                }
            }
        }
    }

    // =========================================================================
    // State accessors
    // =========================================================================

    /// Get current input text
    pub fn input_text(&self) -> &str {
        self.state.input()
    }

    /// Get cursor position
    pub fn cursor_position(&self) -> usize {
        self.state.cursor()
    }

    /// Get current popup state
    pub fn popup(&self) -> Option<&PopupState> {
        self.view.popup()
    }

    /// Check if popup is open
    pub fn has_popup(&self) -> bool {
        self.view.popup().is_some()
    }

    /// Get popup query
    pub fn popup_query(&self) -> Option<&str> {
        self.view.popup().map(|p| p.query())
    }

    /// Get popup selected index
    pub fn popup_selected(&self) -> Option<usize> {
        self.view.popup().map(|p| p.selected_index())
    }

    /// Get conversation items (from view's state - single source of truth)
    pub fn conversation_items(&self) -> &VecDeque<ConversationItem> {
        self.view.state().conversation.items()
    }

    /// Get number of conversation items
    pub fn conversation_len(&self) -> usize {
        self.view.state().conversation.items().len()
    }

    /// Check if there's an error
    pub fn has_error(&self) -> bool {
        self.state.status_error.is_some()
    }

    /// Get error message
    pub fn error(&self) -> Option<&str> {
        self.state.status_error.as_deref()
    }

    /// Check if reasoning display is enabled
    pub fn show_reasoning(&self) -> bool {
        self.state.show_reasoning()
    }

    /// Get accumulated reasoning text
    pub fn reasoning(&self) -> &str {
        &self.state.accumulated_reasoning
    }

    // =========================================================================
    // Rendering
    // =========================================================================

    /// Render to a test terminal and return it for snapshot testing
    pub fn render_terminal(&self) -> Terminal<TestBackend> {
        let mut terminal =
            Terminal::new(TestBackend::new(self.width(), self.height())).expect("create terminal");
        terminal
            .draw(|frame| {
                self.view.render_frame(frame);
            })
            .expect("draw frame");
        terminal
    }

    /// Render to a string for snapshot testing
    pub fn render(&self) -> String {
        let terminal = self.render_terminal();
        let backend = terminal.backend();
        let buffer = backend.buffer();
        buffer_to_string(buffer)
    }

    /// Render just the input area as a string
    pub fn render_input(&self) -> String {
        let cursor_marker = if self.state.cursor() == self.state.input().len() {
            format!("{}|", self.state.input())
        } else {
            let (before, after) = self.state.input().split_at(self.state.cursor());
            format!("{}|{}", before, after)
        };
        cursor_marker
    }
}

/// Convert a ratatui Buffer to a string for snapshot testing
fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
    let mut output = String::new();
    let area = buffer.area;

    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buffer.cell((x, y)) {
                output.push_str(cell.symbol());
            }
        }
        output.push('\n');
    }

    // Trim trailing empty lines but keep structure
    while output.ends_with("\n\n") {
        output.pop();
    }

    output
}

// =============================================================================
// StreamingHarness - Testing progressive graduation and scrolling
// =============================================================================

/// Default viewport height for inline mode (matches runner.rs)
const DEFAULT_VIEWPORT_HEIGHT: u16 = 15;

/// Test harness for streaming scenarios with graduation tracking
///
/// Extends `Harness` to simulate inline viewport behavior:
/// - Tracks content that would be "graduated" to terminal scrollback
/// - Calculates rendered line counts after each event
/// - Provides combined snapshots (scrollback + viewport)
///
/// # Example
///
/// ```ignore
/// let mut h = StreamingHarness::new(80, 15);
/// h.user_message("Hello");
/// h.start_streaming();
/// h.chunk("Line 1\nLine 2\nLine 3\n");
/// assert_eq!(h.graduated_line_count(), 0); // Nothing overflowed yet
///
/// h.chunk("Line 4\n...\nLine 20\n");
/// assert!(h.graduated_line_count() > 0); // Content graduated to scrollback
///
/// assert_snapshot!(h.full_state());
/// ```
pub struct StreamingHarness {
    /// Inner harness for rendering and state
    pub harness: Harness,
    /// Sequence counter for streaming events
    seq: u64,
    /// Whether currently in streaming mode
    is_streaming: bool,
    /// Timeline of snapshots (if recording enabled)
    timeline: Vec<TimelineEntry>,
    /// Whether to record timeline
    record_timeline: bool,
}

/// A snapshot in the timeline
#[derive(Debug, Clone)]
pub struct TimelineEntry {
    /// Event that triggered this snapshot
    pub event: String,
    /// Rendered viewport at this point
    pub viewport: String,
    /// Scrollback content at this point
    pub scrollback: Vec<String>,
    /// Total content lines at this point
    pub content_lines: usize,
    /// Graduated lines at this point
    pub graduated_lines: usize,
}

impl StreamingHarness {
    /// Create a new streaming harness with given dimensions
    ///
    /// Height should typically be small (e.g., 15) to simulate inline viewport.
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            harness: Harness::new(width, height),
            seq: 0,
            is_streaming: false,
            timeline: Vec::new(),
            record_timeline: false,
        }
    }

    /// Create with default inline viewport dimensions (80x15)
    pub fn inline() -> Self {
        Self::new(80, DEFAULT_VIEWPORT_HEIGHT)
    }

    /// Enable timeline recording for debugging
    pub fn with_timeline(mut self) -> Self {
        self.record_timeline = true;
        self
    }

    /// Add a user message to the conversation
    pub fn user_message(&mut self, content: &str) {
        let _ = self.harness.view.push_user_message(content);
        self.maybe_record("user_message");
        self.check_graduation();
    }

    /// Start streaming mode (creates empty assistant message)
    pub fn start_streaming(&mut self) {
        self.harness.view.start_assistant_streaming();
        self.is_streaming = true;
        self.seq = 0u64;
        self.maybe_record("start_streaming");
    }

    /// Send a streaming text chunk
    pub fn chunk(&mut self, text: &str) {
        self.seq += 1;
        self.harness.event(StreamingEvent::Delta {
            text: text.to_string(),
            seq: self.seq,
        });
        self.maybe_record(&format!(
            "chunk:{}",
            text.chars().take(20).collect::<String>()
        ));
        self.check_graduation();
    }

    /// Complete streaming
    pub fn complete(&mut self) {
        self.harness.event(StreamingEvent::Done {
            full_response: String::new(),
        });
        self.is_streaming = false;
        self.maybe_record("complete");
        // Graduate any remaining content
        self.check_graduation();
    }

    /// Check if content exceeds viewport and graduate overflow.
    ///
    /// Uses the SAME graduation logic as the real runner to ensure tests
    /// exercise actual production code.
    fn check_graduation(&mut self) {
        use crate::tui::constants::UiConstants;
        use crate::tui::graduation::check_graduation;

        // Use view's state width - SAME as production runner
        let terminal_width = self.harness.view.state().width;
        let content_width = UiConstants::content_width(terminal_width);
        let current_graduated = self.harness.view.state().graduated_line_count;

        // Use the view's actual viewport height calculation - SAME as production runner!
        // This accounts for input box height, reasoning panel, popups, etc.
        let viewport_capacity = self.harness.view.conversation_viewport_height();

        // Use the shared graduation logic (same as runner)
        // Pass streaming flag to reserve buffer for volatile content
        let (_all_lines, result) = check_graduation(
            &self.harness.view.state().conversation,
            current_graduated,
            viewport_capacity,
            content_width,
            self.is_streaming,
        );

        if let Some(grad) = result {
            // Update the view's graduated_line_count - scrollback is derived from this
            self.harness.view.state_mut().graduated_line_count = grad.new_graduated_count;
        }
    }

    /// Calculate total rendered content lines (using same logic as graduation).
    ///
    /// Uses `render_for_graduation` which excludes Status items, matching
    /// what the actual graduation logic uses.
    pub fn content_line_count(&self) -> usize {
        self.render_content_lines().len()
    }

    /// Render content to lines for scrollback (using same logic as graduation).
    ///
    /// Uses `render_for_graduation` which excludes Status items, matching
    /// what the actual graduation logic uses. This ensures scrollback
    /// content matches what would actually be graduated in production.
    fn render_content_lines(&self) -> Vec<String> {
        use crate::tui::constants::UiConstants;

        // Use view's state width - SAME as production runner
        let terminal_width = self.harness.view.state().width;
        let content_width = UiConstants::content_width(terminal_width);

        // Use render_for_graduation (excludes Status) - same as production graduation
        self.harness
            .view
            .state()
            .conversation
            .render_for_graduation(content_width)
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    /// Record a timeline entry
    fn maybe_record(&mut self, event: &str) {
        if !self.record_timeline {
            return;
        }
        self.timeline.push(TimelineEntry {
            event: event.to_string(),
            viewport: self.harness.render(),
            scrollback: self.scrollback(),
            content_lines: self.content_line_count(),
            graduated_lines: self.graduated_line_count(),
        });
    }

    // =========================================================================
    // Accessors
    // =========================================================================

    /// Get number of lines graduated to scrollback.
    ///
    /// Uses the view's graduated_line_count as the source of truth.
    pub fn graduated_line_count(&self) -> usize {
        self.harness.view.state().graduated_line_count
    }

    /// Get scrollback content as lines.
    ///
    /// Derives scrollback from the graduated lines - the first N lines that have
    /// been graduated to terminal scrollback, where N = graduated_line_count.
    pub fn scrollback(&self) -> Vec<String> {
        let all_lines = self.render_content_lines();
        let graduated = self.harness.view.state().graduated_line_count;

        if graduated == 0 {
            return Vec::new();
        }

        all_lines.into_iter().take(graduated).collect()
    }

    /// Check if currently streaming
    pub fn is_streaming(&self) -> bool {
        self.is_streaming
    }

    /// Get timeline (if recording enabled)
    pub fn timeline(&self) -> &[TimelineEntry] {
        &self.timeline
    }

    /// Render just the viewport
    pub fn render_viewport(&self) -> String {
        self.harness.render()
    }

    /// Render scrollback content as string.
    ///
    /// Derives scrollback from the graduated lines - the first N lines that have
    /// been graduated to terminal scrollback, where N = graduated_line_count.
    pub fn render_scrollback(&self) -> String {
        // Get all rendered lines and take the first graduated_line_count
        let all_lines = self.render_content_lines();
        let graduated = self.harness.view.state().graduated_line_count;

        if graduated == 0 || all_lines.is_empty() {
            return String::new();
        }

        all_lines
            .into_iter()
            .take(graduated)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Render full state: scrollback + viewport
    ///
    /// This represents what the user would see in their terminal:
    /// - Scrollback: content graduated via insert_before (above viewport)
    /// - Viewport: current inline viewport content
    ///
    /// Uses the view's graduated_line_count as the source of truth, ensuring
    /// tests verify the exact same logic as production.
    pub fn full_state(&self) -> String {
        let mut output = String::new();

        let scrollback = self.render_scrollback();
        if !scrollback.is_empty() {
            output.push_str("═══ SCROLLBACK ═══\n");
            output.push_str(&scrollback);
            output.push_str("\n═══ VIEWPORT ═══\n");
        }
        output.push_str(&self.harness.render());

        output
    }

    /// Access inner harness for additional operations
    pub fn inner(&self) -> &Harness {
        &self.harness
    }

    /// Access inner harness mutably
    pub fn inner_mut(&mut self) -> &mut Harness {
        &mut self.harness
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::testing::fixtures::sessions;

    #[test]
    fn harness_default_is_empty() {
        let h = Harness::default();
        assert_eq!(h.input_text(), "");
        assert!(!h.has_popup());
        assert_eq!(h.conversation_len(), 0);
    }

    #[test]
    fn harness_accepts_input() {
        let mut h = Harness::new(80, 24);
        h.keys("hello");
        assert_eq!(h.input_text(), "hello");
    }

    #[test]
    fn harness_cursor_movement() {
        let mut h = Harness::new(80, 24);
        h.keys("hello");
        assert_eq!(h.cursor_position(), 5);

        h.key(KeyCode::Left);
        assert_eq!(h.cursor_position(), 4);

        h.key_ctrl('a');
        assert_eq!(h.cursor_position(), 0);

        h.key_ctrl('e');
        assert_eq!(h.cursor_position(), 5);
    }

    #[test]
    fn harness_slash_opens_popup() {
        let mut h = Harness::new(80, 24);
        h.key(KeyCode::Char('/'));
        assert!(h.has_popup());
        assert_eq!(h.input_text(), "/");
    }

    #[test]
    fn harness_escape_closes_popup() {
        let mut h = Harness::new(80, 24);
        h.key(KeyCode::Char('/'));
        assert!(h.has_popup());

        h.key(KeyCode::Esc);
        assert!(!h.has_popup());
    }

    #[test]
    fn harness_with_session() {
        let h = Harness::new(80, 24).with_session(sessions::basic_exchange());
        assert_eq!(h.conversation_len(), 2);
    }

    #[test]
    fn harness_ctrl_w_deletes_word() {
        let mut h = Harness::new(80, 24);
        h.keys("hello world");
        h.key_ctrl('w');
        assert_eq!(h.input_text(), "hello ");
    }

    #[test]
    fn harness_ctrl_u_clears_to_start() {
        let mut h = Harness::new(80, 24);
        h.keys("hello world");
        h.key_ctrl('u');
        assert_eq!(h.input_text(), "");
    }

    #[test]
    fn harness_render_returns_string() {
        let h = Harness::new(40, 10);
        let output = h.render();
        assert!(!output.is_empty());
    }

    // =========================================================================
    // Reasoning Toggle Tests (TDD - RED PHASE)
    // =========================================================================

    #[test]
    fn harness_reasoning_default_hidden() {
        let h = Harness::new(80, 24);
        assert!(!h.show_reasoning());
        assert!(h.reasoning().is_empty());
    }

    #[test]
    fn harness_alt_t_toggles_reasoning() {
        let mut h = Harness::new(80, 24);
        assert!(!h.show_reasoning());

        h.key_alt('t');
        assert!(h.show_reasoning());

        h.key_alt('t');
        assert!(!h.show_reasoning());
    }

    #[test]
    fn harness_reasoning_event_accumulates() {
        let mut h = Harness::new(80, 24);
        h.event(StreamingEvent::Reasoning {
            text: "Thinking about ".to_string(),
            seq: 0,
        });
        h.event(StreamingEvent::Reasoning {
            text: "the problem".to_string(),
            seq: 1,
        });
        assert_eq!(h.reasoning(), "Thinking about the problem");
    }

    #[test]
    fn harness_reasoning_clears_on_done() {
        let mut h = Harness::new(80, 24);
        h.event(StreamingEvent::Reasoning {
            text: "Some thinking".to_string(),
            seq: 0,
        });
        assert!(!h.reasoning().is_empty());

        h.event(StreamingEvent::Done {
            full_response: "Done".to_string(),
        });
        assert!(h.reasoning().is_empty());
    }

    // =========================================================================
    // StreamingHarness Tests
    // =========================================================================

    #[test]
    fn streaming_harness_tracks_content_lines() {
        let mut h = StreamingHarness::new(80, 15);

        // Empty state
        assert_eq!(h.content_line_count(), 0);
        assert_eq!(h.graduated_line_count(), 0);

        // Add user message
        h.user_message("Hello");
        assert!(h.content_line_count() > 0);
    }

    #[test]
    fn streaming_harness_no_graduation_when_fits() {
        let mut h = StreamingHarness::new(80, 20); // Large viewport

        h.user_message("Hello");
        h.start_streaming();
        h.chunk("Short response");
        h.complete();

        // Content should fit in viewport, no graduation needed
        assert_eq!(h.graduated_line_count(), 0);
        assert!(h.scrollback().is_empty());
    }

    #[test]
    fn streaming_harness_graduates_overflow() {
        // Very small viewport to force graduation (8 lines total, ~4 for content)
        let mut h = StreamingHarness::new(80, 8);

        h.user_message("Hello");
        h.start_streaming();

        // Use double newlines to create markdown paragraphs (rendered as separate lines)
        // Single newlines in markdown are treated as soft breaks
        h.chunk("Paragraph 1\n\n");
        h.chunk("Paragraph 2\n\n");
        h.chunk("Paragraph 3\n\n");
        h.chunk("Paragraph 4\n\n");
        h.chunk("Paragraph 5\n\n");
        h.chunk("Paragraph 6\n\n");
        h.chunk("Paragraph 7\n\n");
        h.chunk("Paragraph 8\n\n");
        h.chunk("Paragraph 9\n\n");
        h.chunk("Paragraph 10\n\n");
        h.complete();

        let content_lines = h.content_line_count();
        let viewport = h.harness.view.conversation_viewport_height();

        // With 8-line terminal, viewport is ~4 lines for content
        // User message + 10 paragraphs should definitely overflow
        assert!(
            content_lines > viewport,
            "Content ({} lines) should exceed viewport ({} lines)",
            content_lines,
            viewport
        );

        // Should have graduated the overflow
        assert!(
            h.graduated_line_count() > 0,
            "Expected graduation but got 0. Content: {}, Viewport: {}, Scrollback: {:?}",
            content_lines,
            viewport,
            h.scrollback()
        );
    }

    #[test]
    fn streaming_harness_timeline_records_events() {
        let mut h = StreamingHarness::new(80, 15).with_timeline();

        h.user_message("Hello");
        h.start_streaming();
        h.chunk("Response");
        h.complete();

        let timeline = h.timeline();
        assert!(timeline.len() >= 4, "Expected at least 4 timeline entries");

        // Check events are recorded
        assert!(timeline.iter().any(|e| e.event == "user_message"));
        assert!(timeline.iter().any(|e| e.event == "start_streaming"));
        assert!(timeline.iter().any(|e| e.event.starts_with("chunk:")));
        assert!(timeline.iter().any(|e| e.event == "complete"));
    }

    #[test]
    fn streaming_harness_full_state_includes_scrollback() {
        let mut h = StreamingHarness::new(80, 15);

        h.user_message("Hello");
        h.start_streaming();
        for i in 1..=20 {
            h.chunk(&format!("Line {}\n", i));
        }
        h.complete();

        let full = h.full_state();

        // If there's scrollback, it should be marked
        if h.graduated_line_count() > 0 {
            assert!(
                full.contains("SCROLLBACK"),
                "Full state should include SCROLLBACK marker when lines are graduated"
            );
        }
    }

    #[test]
    fn streaming_harness_inline_creates_small_viewport() {
        let h = StreamingHarness::inline();
        assert_eq!(h.harness.height(), 15);
        // Content viewport is dynamically calculated by the view
        // With height=15, input=3, spacer=1, status=1: 15-5=10
        let viewport = h.harness.view.conversation_viewport_height();
        assert_eq!(viewport, 10);
    }

    /// Test that prose after a code block is correctly parsed and rendered.
    ///
    /// This is a regression test for the bug where code blocks break subsequent
    /// markdown formatting (e.g., **bold** shows as literal asterisks).
    #[test]
    fn streaming_prose_after_code_block_parses_correctly() {
        use crate::tui::conversation::ConversationItem;

        let mut h = StreamingHarness::inline();

        h.start_streaming();

        // Stream content with code block followed by prose with markdown
        h.chunk("Here is some code:\n\n");
        h.chunk("```rust\n");
        h.chunk("fn main() {}\n");
        h.chunk("```\n");
        h.chunk("\nAnd here is **bold** text after the code block.\n");

        h.complete();

        // Get the assistant message blocks
        let items = h.harness.view.state().conversation.items();
        let assistant_msg = items.iter().find(|item| {
            matches!(item, ConversationItem::AssistantMessage { .. })
        });

        assert!(assistant_msg.is_some(), "Should have assistant message");

        if let Some(ConversationItem::AssistantMessage { blocks, .. }) = assistant_msg {
            // Should have multiple blocks: prose -> code -> prose
            assert!(
                blocks.len() >= 3,
                "Should have at least 3 blocks (prose, code, prose), got {}: {:?}",
                blocks.len(),
                blocks.iter().map(|b| b.text()).collect::<Vec<_>>()
            );

            // First block should be prose
            assert!(blocks[0].is_prose(), "First block should be prose");

            // Second block should be code
            assert!(blocks[1].is_code(), "Second block should be code");
            assert_eq!(blocks[1].text(), "fn main() {}");

            // Third block should be prose with the bold text
            assert!(blocks[2].is_prose(), "Third block should be prose");
            assert!(
                blocks[2].text().contains("**bold**"),
                "Third block should contain markdown: {:?}",
                blocks[2].text()
            );
        }
    }

    /// Test that the streaming parser is actually being used by the harness.
    ///
    /// This ensures we don't regress to direct append_or_create_prose calls.
    #[test]
    fn streaming_harness_uses_parser_for_code_blocks() {
        use crate::tui::conversation::ConversationItem;

        let mut h = StreamingHarness::inline();

        h.start_streaming();
        h.chunk("```\ncode\n```\n");
        h.complete();

        let items = h.harness.view.state().conversation.items();
        let assistant_msg = items.iter().find(|item| {
            matches!(item, ConversationItem::AssistantMessage { .. })
        });

        if let Some(ConversationItem::AssistantMessage { blocks, .. }) = assistant_msg {
            // If parser is working, we should have a CODE block, not prose
            let has_code_block = blocks.iter().any(|b| b.is_code());
            assert!(
                has_code_block,
                "Should have code block (parser should be active). Blocks: {:?}",
                blocks.iter().map(|b| (b.is_prose(), b.is_code(), b.text())).collect::<Vec<_>>()
            );
        } else {
            panic!("Should have assistant message");
        }
    }
}
