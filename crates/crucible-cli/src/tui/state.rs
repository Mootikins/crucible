//! TUI state management

//!
//! Contains TuiState struct and related types for managing UI state.

use crate::tui::notification::NotificationState;
use crate::tui::streaming::StreamingBuffer;
use crate::tui::InputAction;
use crucible_core::events::SessionEvent;
use crucible_core::traits::chat::cycle_mode_id;
use crucible_core::traits::MessageRole;
use crucible_rune::EventRing;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::time::Instant;

// =============================================================================
// Word boundary helpers for readline-style editing
// =============================================================================

/// Find the byte position of the start of the previous word.
/// Skips trailing whitespace, then finds where the word begins.
pub(crate) fn find_word_start_backward(s: &str) -> usize {
    let mut chars = s.char_indices().rev().peekable();

    // Skip trailing whitespace
    while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
        chars.next();
    }

    // Skip word characters
    while chars.peek().is_some_and(|(_, c)| !c.is_whitespace()) {
        chars.next();
    }

    // Return position after the whitespace (start of the word we skipped)
    chars.next().map(|(i, c)| i + c.len_utf8()).unwrap_or(0)
}

/// Find the byte offset to the start of the next word.
/// Skips current word, then whitespace.
pub(crate) fn find_word_start_forward(s: &str) -> usize {
    let mut chars = s.char_indices().peekable();

    // Skip current word
    while chars.peek().is_some_and(|(_, c)| !c.is_whitespace()) {
        chars.next();
    }

    // Skip whitespace
    while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
        chars.next();
    }

    chars.peek().map(|(i, _)| *i).unwrap_or(s.len())
}

// MessageRole imported from crucible_core::traits

/// Type of popup trigger
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupKind {
    Command,
    AgentOrFile,
}

/// Popup entry displayed in the inline picker
///
/// Each variant contains only the data relevant to that item type.
/// Use the accessor methods (`title()`, `token()`, etc.) for uniform access.
#[derive(Debug, Clone, PartialEq)]
pub enum PopupItem {
    /// Slash command: `/name`
    Command {
        name: String,
        description: String,
        /// Argument hint shown as faded text (e.g., "<query>" for /search)
        argument_hint: Option<String>,
        score: i32,
        available: bool,
    },
    /// Agent mention: `@id`
    Agent {
        id: String,
        description: String,
        score: i32,
        available: bool,
    },
    /// Workspace file reference
    File {
        path: String,
        score: i32,
        available: bool,
    },
    /// Note reference: `note:path`
    Note {
        path: String,
        score: i32,
        available: bool,
    },
    /// Skill invocation: `skill:name`
    Skill {
        name: String,
        description: String,
        scope: String,
        score: i32,
        available: bool,
    },
}

impl PopupItem {
    // =========================================================================
    // Constructors - create items with sensible defaults
    // =========================================================================

    /// Create a new command popup item: `/name`
    pub fn cmd(name: impl Into<String>) -> Self {
        PopupItem::Command {
            name: name.into(),
            description: String::new(),
            argument_hint: None,
            score: 0,
            available: true,
        }
    }

    /// Create a new agent popup item: `@id`
    pub fn agent(id: impl Into<String>) -> Self {
        PopupItem::Agent {
            id: id.into(),
            description: String::new(),
            score: 0,
            available: true,
        }
    }

    /// Create a new file popup item
    pub fn file(path: impl Into<String>) -> Self {
        PopupItem::File {
            path: path.into(),
            score: 0,
            available: true,
        }
    }

    /// Create a new note popup item
    pub fn note(path: impl Into<String>) -> Self {
        PopupItem::Note {
            path: path.into(),
            score: 0,
            available: true,
        }
    }

    /// Create a new skill popup item
    pub fn skill(name: impl Into<String>) -> Self {
        PopupItem::Skill {
            name: name.into(),
            description: String::new(),
            scope: String::new(),
            score: 0,
            available: true,
        }
    }

    // =========================================================================
    // Builder methods - chain after constructor
    // =========================================================================

    /// Builder: set description (for Command, Agent, Skill)
    pub fn desc(mut self, description: impl Into<String>) -> Self {
        let d = description.into();
        match &mut self {
            PopupItem::Command { description, .. } => *description = d,
            PopupItem::Agent { description, .. } => *description = d,
            PopupItem::Skill { description, .. } => *description = d,
            PopupItem::File { .. } | PopupItem::Note { .. } => {}
        }
        self
    }

    /// Builder: set argument hint (Command only)
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        if let PopupItem::Command { argument_hint, .. } = &mut self {
            *argument_hint = Some(hint.into());
        }
        self
    }

    /// Builder: set scope (Skill only)
    pub fn with_scope(mut self, s: impl Into<String>) -> Self {
        if let PopupItem::Skill { scope, .. } = &mut self {
            *scope = s.into();
        }
        self
    }

    /// Builder: set score
    pub fn with_score(mut self, s: i32) -> Self {
        match &mut self {
            PopupItem::Command { score, .. } => *score = s,
            PopupItem::Agent { score, .. } => *score = s,
            PopupItem::File { score, .. } => *score = s,
            PopupItem::Note { score, .. } => *score = s,
            PopupItem::Skill { score, .. } => *score = s,
        }
        self
    }

    /// Builder: set availability
    pub fn with_available(mut self, a: bool) -> Self {
        match &mut self {
            PopupItem::Command { available, .. } => *available = a,
            PopupItem::Agent { available, .. } => *available = a,
            PopupItem::File { available, .. } => *available = a,
            PopupItem::Note { available, .. } => *available = a,
            PopupItem::Skill { available, .. } => *available = a,
        }
        self
    }

    // =========================================================================
    // Accessors - uniform interface across variants
    // =========================================================================

    /// Display title (e.g., "/search", "@agent", "src/main.rs")
    pub fn title(&self) -> String {
        match self {
            PopupItem::Command { name, .. } => format!("/{}", name),
            PopupItem::Agent { id, .. } => format!("@{}", id),
            PopupItem::File { path, .. } => path.clone(),
            PopupItem::Note { path, .. } => format!("note:{}", path),
            PopupItem::Skill { name, .. } => format!("skill:{}", name),
        }
    }

    /// Subtitle/description text
    pub fn subtitle(&self) -> &str {
        match self {
            PopupItem::Command { description, .. } => description,
            PopupItem::Agent { description, .. } => description,
            PopupItem::File { .. } => "workspace",
            PopupItem::Note { .. } => "note",
            PopupItem::Skill {
                description,
                scope: _,
                ..
            } => {
                // For skills, we want "description (scope)" but we can't allocate here
                // Return just description; caller can format with scope if needed
                description
            }
        }
    }

    /// Token to insert when selected
    pub fn token(&self) -> String {
        match self {
            PopupItem::Command { name, .. } => format!("/{} ", name),
            PopupItem::Agent { id, .. } => format!("@{}", id),
            PopupItem::File { path, .. } => path.clone(),
            PopupItem::Note { path, .. } => path.clone(),
            PopupItem::Skill { name, .. } => format!("skill:{} ", name),
        }
    }

    /// Kind label for display (e.g., "cmd", "agent")
    pub fn kind_label(&self) -> &'static str {
        match self {
            PopupItem::Command { .. } => "cmd",
            PopupItem::Agent { .. } => "agent",
            PopupItem::File { .. } => "file",
            PopupItem::Note { .. } => "note",
            PopupItem::Skill { .. } => "skill",
        }
    }

    /// Score for sorting/filtering
    pub fn score(&self) -> i32 {
        match self {
            PopupItem::Command { score, .. } => *score,
            PopupItem::Agent { score, .. } => *score,
            PopupItem::File { score, .. } => *score,
            PopupItem::Note { score, .. } => *score,
            PopupItem::Skill { score, .. } => *score,
        }
    }

    /// Whether item is available/enabled
    pub fn is_available(&self) -> bool {
        match self {
            PopupItem::Command { available, .. } => *available,
            PopupItem::Agent { available, .. } => *available,
            PopupItem::File { available, .. } => *available,
            PopupItem::Note { available, .. } => *available,
            PopupItem::Skill { available, .. } => *available,
        }
    }

    /// Argument hint (Command only)
    pub fn argument_hint(&self) -> Option<&str> {
        match self {
            PopupItem::Command { argument_hint, .. } => argument_hint.as_deref(),
            _ => None,
        }
    }

    /// Skill scope (Skill only)
    pub fn scope(&self) -> Option<&str> {
        match self {
            PopupItem::Skill { scope, .. } => Some(scope),
            _ => None,
        }
    }

    // =========================================================================
    // Compatibility - for code that still uses old field access patterns
    // =========================================================================

    /// Check if this is a Command variant
    pub fn is_command(&self) -> bool {
        matches!(self, PopupItem::Command { .. })
    }

    /// Check if this is an Agent variant
    pub fn is_agent(&self) -> bool {
        matches!(self, PopupItem::Agent { .. })
    }

    /// Check if this is a File variant
    pub fn is_file(&self) -> bool {
        matches!(self, PopupItem::File { .. })
    }

    /// Check if this is a Note variant
    pub fn is_note(&self) -> bool {
        matches!(self, PopupItem::Note { .. })
    }

    /// Check if this is a Skill variant
    pub fn is_skill(&self) -> bool {
        matches!(self, PopupItem::Skill { .. })
    }
}

/// Legacy type alias for code that still references PopupItemKind
///
/// Use pattern matching on PopupItem variants instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupItemKind {
    Command,
    Agent,
    File,
    Note,
    Skill,
}

impl PopupItem {
    /// Get the kind as an enum (for compatibility with old code)
    ///
    /// Prefer pattern matching on PopupItem directly.
    pub fn kind(&self) -> PopupItemKind {
        match self {
            PopupItem::Command { .. } => PopupItemKind::Command,
            PopupItem::Agent { .. } => PopupItemKind::Agent,
            PopupItem::File { .. } => PopupItemKind::File,
            PopupItem::Note { .. } => PopupItemKind::Note,
            PopupItem::Skill { .. } => PopupItemKind::Skill,
        }
    }
}

/// Popup state for inline triggers (/ or @)
#[derive(Debug, Clone)]
pub struct PopupState {
    pub kind: PopupKind,
    pub query: String,
    pub items: Vec<PopupItem>,
    pub selected: usize,
    pub viewport_offset: usize,
    pub last_update: Instant,
}

impl PopupState {
    pub fn new(kind: PopupKind) -> Self {
        Self {
            kind,
            query: String::new(),
            items: Vec::new(),
            selected: 0,
            viewport_offset: 0,
            last_update: Instant::now(),
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.items.is_empty() {
            self.selected = 0;
            return;
        }
        let len = self.items.len() as isize;
        let new_idx = (self.selected as isize + delta).rem_euclid(len);
        self.selected = new_idx as usize;
    }

    /// Update viewport offset to keep selection visible
    /// Call this after changing `selected`
    pub fn update_viewport(&mut self, visible_count: usize) {
        // If selection is above viewport, scroll up
        if self.selected < self.viewport_offset {
            self.viewport_offset = self.selected;
        }
        // If selection is below viewport, scroll down
        else if self.selected >= self.viewport_offset + visible_count {
            self.viewport_offset = self.selected - visible_count + 1;
        }
    }
}

/// A message formatted for display
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: u64,
    pub tool_calls: Vec<ToolCallInfo>,
}

impl DisplayMessage {
    pub fn from_event(event: &SessionEvent) -> Option<Self> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        match event {
            SessionEvent::MessageReceived {
                content,
                participant_id,
            } => {
                let role = if participant_id == "user" {
                    MessageRole::User
                } else if participant_id == "system" {
                    MessageRole::System
                } else {
                    MessageRole::User
                };
                Some(Self {
                    role,
                    content: content.clone(),
                    timestamp: now,
                    tool_calls: Vec::new(),
                })
            }
            SessionEvent::AgentResponded {
                content,
                tool_calls,
            } => {
                let calls = tool_calls
                    .iter()
                    .map(|tc| ToolCallInfo {
                        name: tc.name.clone(),
                        args: tc.args.clone(),
                        call_id: tc.call_id.clone(),
                        completed: false,
                        result: None,
                        error: None,
                    })
                    .collect();
                Some(Self {
                    role: MessageRole::Assistant,
                    content: content.clone(),
                    timestamp: now,
                    tool_calls: calls,
                })
            }
            SessionEvent::ToolCompleted {
                name,
                result,
                error,
            } => Some(Self {
                role: MessageRole::Tool,
                content: if let Some(err) = error {
                    format!("[{}] Error: {}", name, err)
                } else {
                    format!("[{}] {}", name, result)
                },
                timestamp: now,
                tool_calls: Vec::new(),
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub name: String,
    pub args: JsonValue,
    pub call_id: Option<String>,
    pub completed: bool,
    pub result: Option<String>,
    pub error: Option<String>,
}

impl ToolCallInfo {
    pub fn from_event(event: &SessionEvent) -> Option<Self> {
        match event {
            SessionEvent::ToolCalled { name, args } => Some(Self {
                name: name.clone(),
                args: args.clone(),
                call_id: None,
                completed: false,
                result: None,
                error: None,
            }),
            _ => None,
        }
    }

    pub fn apply_completion(&mut self, event: &SessionEvent) -> bool {
        match event {
            SessionEvent::ToolCompleted {
                name,
                result,
                error,
            } => {
                if &self.name == name {
                    self.completed = true;
                    self.result = Some(result.clone());
                    self.error = error.clone();
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub fn format_display(&self) -> String {
        if self.completed {
            if let Some(ref error) = self.error {
                format!("Tool error: {} -> Error: {}", self.name, error)
            } else if let Some(ref result) = self.result {
                format!(
                    "Tool complete: {} -> {}",
                    self.name,
                    truncate_string_safe(result, 50)
                )
            } else {
                format!("Tool complete: {}", self.name)
            }
        } else {
            format_tool_call(Some(&self.name), &self.args)
        }
    }
}

fn truncate_string_safe(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!(
            "{}...",
            s.chars()
                .take(max_chars.saturating_sub(3))
                .collect::<String>()
        )
    }
}

pub struct TuiState {
    pub input_buffer: String,
    pub cursor_position: usize,
    pub streaming: Option<StreamingBuffer>,
    pub mode_id: String,
    pub mode_name: String,
    pub pending_tools: Vec<ToolCallInfo>,
    pub last_seen_seq: u64,
    pub should_exit: bool,
    pub ctrl_c_count: u8,
    pub last_ctrl_c: Option<Instant>,
    pub status_error: Option<String>,
    // Inline popup for slash commands / agents / files/notes
    pub popup: Option<PopupState>,
    // Notification state for file watch events
    pub notifications: NotificationState,
    #[allow(clippy::type_complexity)] // Complex callback type, not worth a type alias
    output_fn: Option<Box<dyn Fn(&str) + Send + Sync>>,
}

impl TuiState {
    pub fn new(mode_id: impl Into<String>) -> Self {
        let mode_id = mode_id.into();
        let mode_name = crucible_core::traits::chat::mode_display_name(&mode_id).to_string();
        Self {
            input_buffer: String::new(),
            cursor_position: 0,
            streaming: None,
            mode_id,
            mode_name,
            pending_tools: Vec::new(),
            last_seen_seq: 0,
            should_exit: false,
            ctrl_c_count: 0,
            last_ctrl_c: None,
            status_error: None,
            popup: None,
            notifications: NotificationState::new(),
            output_fn: None,
        }
    }

    pub fn with_output<F: Fn(&str) + Send + Sync + 'static>(
        mode_id: impl Into<String>,
        output_fn: F,
    ) -> Self {
        let mode_id = mode_id.into();
        let mode_name = crucible_core::traits::chat::mode_display_name(&mode_id).to_string();
        Self {
            input_buffer: String::new(),
            cursor_position: 0,
            streaming: None,
            mode_id,
            mode_name,
            pending_tools: Vec::new(),
            last_seen_seq: 0,
            should_exit: false,
            ctrl_c_count: 0,
            last_ctrl_c: None,
            status_error: None,
            popup: None,
            notifications: NotificationState::new(),
            output_fn: Some(Box::new(output_fn)),
        }
    }

    fn print_output(&self, content: &str) {
        if let Some(ref f) = self.output_fn {
            f(content);
        }
    }

    pub fn handle_agent_error(&mut self, error: &str) {
        if let Some(mut buf) = self.streaming.take() {
            let r = buf.finalize();
            if !r.is_empty() {
                self.print_output(&r);
            }
            self.print_output(&format!("\\n\\x1b[31m[Error: {}]\\x1b[0m\\n", error));
        }
        self.status_error = Some(error.to_string());
    }

    pub fn set_mode_dynamic(&mut self, mode_id: String, mode_name: String) {
        self.mode_id = mode_id.clone();
        self.mode_name = mode_name;
    }

    pub fn execute_action(&mut self, action: crate::tui::InputAction) -> Option<String> {
        match action {
            InputAction::SendMessage(msg) => {
                self.popup = None;
                self.input_buffer.clear();
                self.cursor_position = 0;
                self.status_error = None;
                Some(msg)
            }
            InputAction::InsertNewline => {
                self.input_buffer.insert(self.cursor_position, '\n');
                self.cursor_position += 1;
                None
            }
            InputAction::InsertChar(c) => {
                self.input_buffer.insert(self.cursor_position, c);
                self.cursor_position += c.len_utf8();
                self.update_popup_on_edit();
                None
            }
            InputAction::DeleteChar => {
                if self.cursor_position > 0 {
                    let prev_boundary = self.input_buffer[..self.cursor_position]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.input_buffer.remove(prev_boundary);
                    self.cursor_position = prev_boundary;
                    self.update_popup_on_edit();
                }
                None
            }
            InputAction::MoveCursorLeft => {
                if self.cursor_position > 0 {
                    self.cursor_position = self.input_buffer[..self.cursor_position]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                }
                None
            }
            InputAction::MoveCursorRight => {
                if self.cursor_position < self.input_buffer.len() {
                    self.cursor_position = self.input_buffer[self.cursor_position..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| self.cursor_position + i)
                        .unwrap_or(self.input_buffer.len());
                    self.update_popup_on_edit();
                }
                None
            }
            InputAction::MovePopupSelection(delta) => {
                if let Some(ref mut popup) = self.popup {
                    popup.move_selection(delta);
                }
                None
            }
            InputAction::ConfirmPopup => {
                // The actual resolution of popup items is handled externally in the runner
                // where data sources are available. Here we just signal that a popup confirm
                // occurred when a popup is present.
                None
            }
            InputAction::CycleMode => {
                let new_mode_id = cycle_mode_id(&self.mode_id);
                self.mode_id = new_mode_id.to_string();
                self.mode_name =
                    crucible_core::traits::chat::mode_display_name(new_mode_id).to_string();
                None
            }
            InputAction::Exit => {
                self.should_exit = true;
                None
            }
            InputAction::Cancel => {
                self.input_buffer.clear();
                self.cursor_position = 0;
                // Track Ctrl+C for double-press detection
                self.ctrl_c_count += 1;
                self.last_ctrl_c = Some(Instant::now());
                self.popup = None;
                None
            }
            InputAction::ExecuteSlashCommand(_cmd) => {
                // TODO: Slash command execution
                None
            }
            // Readline-style editing (emacs mode)
            InputAction::DeleteWordBackward => {
                if self.cursor_position > 0 {
                    let before = &self.input_buffer[..self.cursor_position];
                    let word_start = find_word_start_backward(before);
                    self.input_buffer.drain(word_start..self.cursor_position);
                    self.cursor_position = word_start;
                    self.update_popup_on_edit();
                }
                None
            }
            InputAction::DeleteToLineStart => {
                if self.cursor_position > 0 {
                    self.input_buffer.drain(..self.cursor_position);
                    self.cursor_position = 0;
                    self.update_popup_on_edit();
                }
                None
            }
            InputAction::DeleteToLineEnd => {
                if self.cursor_position < self.input_buffer.len() {
                    self.input_buffer.truncate(self.cursor_position);
                    self.update_popup_on_edit();
                }
                None
            }
            InputAction::MoveCursorToStart => {
                self.cursor_position = 0;
                None
            }
            InputAction::MoveCursorToEnd => {
                self.cursor_position = self.input_buffer.len();
                None
            }
            InputAction::MoveWordBackward => {
                if self.cursor_position > 0 {
                    let before = &self.input_buffer[..self.cursor_position];
                    self.cursor_position = find_word_start_backward(before);
                }
                None
            }
            InputAction::MoveWordForward => {
                if self.cursor_position < self.input_buffer.len() {
                    let after = &self.input_buffer[self.cursor_position..];
                    self.cursor_position += find_word_start_forward(after);
                }
                None
            }
            InputAction::TransposeChars => {
                // Swap the character before cursor with the one at cursor
                // If at end, swap the two chars before cursor
                let len = self.input_buffer.chars().count();
                if len >= 2 && self.cursor_position > 0 {
                    let chars: Vec<char> = self.input_buffer.chars().collect();
                    let char_pos = self.input_buffer[..self.cursor_position].chars().count();

                    let (i, j) = if char_pos >= len {
                        // At end: swap last two chars
                        (len - 2, len - 1)
                    } else {
                        // Swap char before cursor with char at cursor
                        (char_pos - 1, char_pos)
                    };

                    let mut new_chars = chars.clone();
                    new_chars.swap(i, j);
                    self.input_buffer = new_chars.into_iter().collect();

                    // Move cursor forward (or stay at end)
                    if char_pos < len {
                        self.cursor_position = self
                            .input_buffer
                            .char_indices()
                            .nth(char_pos + 1)
                            .map(|(idx, _)| idx)
                            .unwrap_or(self.input_buffer.len());
                    }
                }
                None
            }
            InputAction::ScrollUp
            | InputAction::ScrollDown
            | InputAction::PageUp
            | InputAction::PageDown
            | InputAction::HalfPageUp
            | InputAction::HalfPageDown
            | InputAction::ScrollToTop
            | InputAction::ScrollToBottom
            | InputAction::HistoryPrev
            | InputAction::HistoryNext
            | InputAction::None => None,
        }
    }

    /// Poll events from the ring buffer and process them.
    ///
    /// Returns the finalized assistant response content when AgentResponded is received,
    /// allowing the runner to render it with markdown.
    pub fn poll_events(&mut self, ring: &Arc<EventRing<SessionEvent>>) -> Option<String> {
        let events: Vec<_> = ring
            .range(self.last_seen_seq, ring.write_sequence())
            .collect();
        self.last_seen_seq = ring.write_sequence();
        let mut finalized_content: Option<String> = None;

        for event in events {
            match &*event {
                SessionEvent::TextDelta { delta, .. } => {
                    if self.streaming.is_none() {
                        self.streaming = Some(StreamingBuffer::new());
                    }
                    if let Some(ref mut buf) = self.streaming {
                        if let Some(output) = buf.append(delta) {
                            self.print_output(&output);
                        }
                    }
                }
                SessionEvent::AgentResponded { content, .. } => {
                    // Finalize streaming buffer and return content for markdown rendering
                    if let Some(mut buf) = self.streaming.take() {
                        let remaining = buf.finalize();
                        // Return the full accumulated content for markdown rendering
                        finalized_content = Some(buf.all_content().to_string());
                        if !remaining.is_empty() {
                            // Note: remaining is the unflushed portion, but we want all content
                            // for proper markdown rendering
                        }
                    } else {
                        // No streaming - use the content directly
                        finalized_content = Some(content.clone());
                    }
                }
                SessionEvent::ToolCalled { name, args } => {
                    self.pending_tools.push(ToolCallInfo {
                        name: name.clone(),
                        args: args.clone(),
                        call_id: None,
                        completed: false,
                        result: None,
                        error: None,
                    });
                }
                SessionEvent::ToolCompleted {
                    name,
                    result,
                    error,
                } => {
                    for tool in &mut self.pending_tools {
                        if &tool.name == name && !tool.completed {
                            tool.completed = true;
                            tool.result = Some(result.clone());
                            tool.error = error.clone();
                            break;
                        }
                    }
                }
                _ => {}
            }
        }

        finalized_content
    }

    fn detect_popup_trigger(&self) -> Option<PopupKind> {
        let trimmed = self.input_buffer.trim_start();
        if trimmed.starts_with('/') {
            Some(PopupKind::Command)
        } else if trimmed.starts_with('@') {
            Some(PopupKind::AgentOrFile)
        } else {
            None
        }
    }

    fn current_query(&self) -> String {
        let trimmed = self.input_buffer.trim_start();
        if let Some(rest) = trimmed.strip_prefix('/') {
            rest.to_string()
        } else if let Some(rest) = trimmed.strip_prefix('@') {
            rest.to_string()
        } else {
            String::new()
        }
    }

    /// Update popup state based on current input buffer and cursor
    fn update_popup_on_edit(&mut self) {
        if let Some(kind) = self.detect_popup_trigger() {
            let query = self.current_query();
            let needs_refresh = match &self.popup {
                Some(p) => p.kind != kind,
                None => true,
            };

            if needs_refresh {
                self.popup = Some(PopupState::new(kind));
            }

            if let Some(ref mut popup) = self.popup {
                popup.query = query;
                popup.last_update = Instant::now();
                // Actual item population happens externally via a provider
            }
        } else {
            self.popup = None;
        }
    }
}

fn format_tool_call(name: Option<&str>, args: &JsonValue) -> String {
    format!(
        "Running tool: {}({})",
        name.unwrap_or("unknown"),
        truncate_args_preview(args, 40)
    )
}

fn truncate_args_preview(args: &JsonValue, max_len: usize) -> String {
    let f = args.to_string();
    if f.len() <= max_len {
        f
    } else {
        format!("{}...", &f[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::InputAction;

    #[test]
    fn test_tui_state_new() {
        let s = TuiState::new("plan");
        assert_eq!(s.mode_id, "plan");
        assert_eq!(s.mode_id, "plan");
        assert_eq!(s.mode_name, "Plan");
    }

    #[test]
    fn test_tui_state_mode_id_name() {
        assert_eq!(TuiState::new("plan").mode_id, "plan");
        assert_eq!(TuiState::new("act").mode_id, "act");
        assert_eq!(TuiState::new("auto").mode_id, "auto");
    }

    #[test]
    fn test_set_mode_dynamic() {
        let mut s = TuiState::new("plan");
        s.set_mode_dynamic("act".into(), "Act".into());
        assert_eq!(s.mode_id, "act");
        assert_eq!(s.mode_name, "Act");
        // Unknown mode ID still updates the display strings
        s.set_mode_dynamic("custom".into(), "Custom".into());
        assert_eq!(s.mode_id, "custom");
        assert_eq!(s.mode_name, "Custom");
    }

    #[test]
    fn test_execute_action_cycle_mode() {
        let mut s = TuiState::new("plan");
        s.execute_action(InputAction::CycleMode);
        assert_eq!(s.mode_id, "act");
        assert_eq!(s.mode_name, "Act");
    }

    #[test]
    fn test_truncate_string_safe() {
        assert_eq!(truncate_string_safe("hello", 10), "hello");
        assert_eq!(truncate_string_safe("hello world", 8), "hello...");
    }

    #[test]
    fn test_format_tool_call() {
        let a = serde_json::json!({"file": "test.rs"});
        assert!(format_tool_call(Some("read"), &a).contains("read"));
    }

    #[test]
    fn test_display_message_from_event() {
        let e = SessionEvent::MessageReceived {
            content: "Hello".into(),
            participant_id: "user".into(),
        };
        let m = DisplayMessage::from_event(&e).unwrap();
        assert_eq!(m.role, MessageRole::User);
    }

    #[test]
    fn test_cycle_mode_wraps_around() {
        let mut s = TuiState::new("auto");
        // From Auto -> Plan (wraps around)
        s.execute_action(InputAction::CycleMode);
        assert_eq!(s.mode_id, "plan");
        assert_eq!(s.mode_id, "plan");
        assert_eq!(s.mode_name, "Plan");
    }

    #[test]
    fn test_cycle_mode_updates_all_mode_fields() {
        let mut s = TuiState::new("plan");

        // Cycle Plan -> Act
        s.execute_action(InputAction::CycleMode);
        assert_eq!(s.mode_id, "act");
        assert_eq!(s.mode_id, "act");
        assert_eq!(s.mode_name, "Act");

        // Cycle Act -> Auto
        s.execute_action(InputAction::CycleMode);
        assert_eq!(s.mode_id, "auto");
        assert_eq!(s.mode_id, "auto");
        assert_eq!(s.mode_name, "Auto");

        // Cycle Auto -> Plan (wraps)
        s.execute_action(InputAction::CycleMode);
        assert_eq!(s.mode_id, "plan");
        assert_eq!(s.mode_id, "plan");
        assert_eq!(s.mode_name, "Plan");
    }

    #[test]
    fn test_popup_trigger_detection() {
        let mut s = TuiState::new("plan");
        s.input_buffer = "/search".into();
        s.update_popup_on_edit();
        assert!(matches!(
            s.popup.as_ref().map(|p| p.kind),
            Some(PopupKind::Command)
        ));

        s.input_buffer = "@dev".into();
        s.update_popup_on_edit();
        assert!(matches!(
            s.popup.as_ref().map(|p| p.kind),
            Some(PopupKind::AgentOrFile)
        ));

        s.input_buffer = "hello".into();
        s.update_popup_on_edit();
        assert!(s.popup.is_none());
    }

    #[test]
    fn test_popup_selection_wraps() {
        let mut state = TuiState::new("plan");
        let mut popup = PopupState::new(PopupKind::Command);
        popup.items = vec![
            PopupItem::cmd("a").with_score(1),
            PopupItem::cmd("b").with_score(1),
        ];
        state.popup = Some(popup);
        state.execute_action(InputAction::MovePopupSelection(-1));
        assert_eq!(state.popup.as_ref().unwrap().selected, 1);
        state.execute_action(InputAction::MovePopupSelection(1));
        assert_eq!(state.popup.as_ref().unwrap().selected, 0);
    }

    #[test]
    fn test_tui_state_has_notifications() {
        let state = TuiState::new("plan");
        assert!(state.notifications.is_empty());
    }

    // ============================================================================
    // Viewport Offset Tests (TDD Phase 1A - These tests SHOULD FAIL to compile)
    // ============================================================================

    #[test]
    fn test_popup_viewport_initial_state() {
        // New popup should have viewport_offset: 0
        let popup = PopupState::new(PopupKind::Command);
        assert_eq!(popup.viewport_offset, 0);
    }

    /// Helper to create 10 test popup items
    fn make_ten_items() -> Vec<PopupItem> {
        (0..10)
            .map(|i| PopupItem::cmd(i.to_string()).with_score(1))
            .collect()
    }

    #[test]
    fn test_popup_viewport_follows_selection_down() {
        // With 10 items and 5 visible, selecting item 6 should shift offset to 2
        // (so items 2-6 are visible, with 6 selected)
        let mut popup = PopupState::new(PopupKind::Command);
        popup.items = make_ten_items();

        // Select item 6 (index 6)
        popup.selected = 6;
        popup.update_viewport(5); // 5 visible items

        // With 5 visible items and selected=6, we want the selection in the bottom slot
        // Visible window should be [2, 3, 4, 5, 6] with 6 selected
        assert_eq!(popup.viewport_offset, 2);
    }

    #[test]
    fn test_popup_viewport_follows_selection_up() {
        // With offset at 3, selecting item 0 should shift offset to 0
        let mut popup = PopupState::new(PopupKind::Command);
        popup.items = make_ten_items();

        popup.viewport_offset = 3;
        popup.selected = 0;
        popup.update_viewport(5); // 5 visible items

        // Selecting item 0 should force viewport to start at 0
        assert_eq!(popup.viewport_offset, 0);
    }

    #[test]
    fn test_popup_viewport_stable_within_window() {
        // Selecting item 2 when offset is 0 should keep offset at 0
        let mut popup = PopupState::new(PopupKind::Command);
        popup.items = make_ten_items();

        popup.viewport_offset = 0;
        popup.selected = 2;
        popup.update_viewport(5); // 5 visible items

        // Item 2 is within visible window [0-4], so offset should stay at 0
        assert_eq!(popup.viewport_offset, 0);
    }

    #[test]
    fn test_popup_viewport_wrap_to_end() {
        // Wrapping selection from 0 to last item should jump viewport
        let mut popup = PopupState::new(PopupKind::Command);
        popup.items = make_ten_items();

        popup.viewport_offset = 0;
        popup.selected = 9; // Last item (wrapped from 0)
        popup.update_viewport(5); // 5 visible items

        // Last item should be visible at bottom of window
        // Visible window should be [5, 6, 7, 8, 9] with 9 selected
        assert_eq!(popup.viewport_offset, 5);
    }

    #[test]
    fn test_popup_viewport_wrap_to_start() {
        // Wrapping selection from last to 0 should reset viewport to 0
        let mut popup = PopupState::new(PopupKind::Command);
        popup.items = make_ten_items();

        popup.viewport_offset = 5;
        popup.selected = 0; // Wrapped from 9 back to 0
        popup.update_viewport(5); // 5 visible items

        // Selecting first item should reset viewport to 0
        assert_eq!(popup.viewport_offset, 0);
    }

    // =========================================================================
    // Readline Action Tests
    // =========================================================================

    #[test]
    fn test_delete_word_backward() {
        let mut s = TuiState::new("plan");
        s.input_buffer = "hello world".into();
        s.cursor_position = 11; // at end
        s.execute_action(InputAction::DeleteWordBackward);
        assert_eq!(s.input_buffer, "hello ");
        assert_eq!(s.cursor_position, 6);
    }

    #[test]
    fn test_delete_word_backward_multiple_spaces() {
        let mut s = TuiState::new("plan");
        s.input_buffer = "hello   world".into();
        s.cursor_position = 13;
        s.execute_action(InputAction::DeleteWordBackward);
        assert_eq!(s.input_buffer, "hello   ");
        assert_eq!(s.cursor_position, 8);
    }

    #[test]
    fn test_delete_to_line_start() {
        let mut s = TuiState::new("plan");
        s.input_buffer = "hello world".into();
        s.cursor_position = 6;
        s.execute_action(InputAction::DeleteToLineStart);
        assert_eq!(s.input_buffer, "world");
        assert_eq!(s.cursor_position, 0);
    }

    #[test]
    fn test_delete_to_line_end() {
        let mut s = TuiState::new("plan");
        s.input_buffer = "hello world".into();
        s.cursor_position = 5;
        s.execute_action(InputAction::DeleteToLineEnd);
        assert_eq!(s.input_buffer, "hello");
        assert_eq!(s.cursor_position, 5);
    }

    #[test]
    fn test_move_cursor_to_start() {
        let mut s = TuiState::new("plan");
        s.input_buffer = "hello world".into();
        s.cursor_position = 6;
        s.execute_action(InputAction::MoveCursorToStart);
        assert_eq!(s.cursor_position, 0);
    }

    #[test]
    fn test_move_cursor_to_end() {
        let mut s = TuiState::new("plan");
        s.input_buffer = "hello world".into();
        s.cursor_position = 0;
        s.execute_action(InputAction::MoveCursorToEnd);
        assert_eq!(s.cursor_position, 11);
    }

    #[test]
    fn test_move_word_backward() {
        let mut s = TuiState::new("plan");
        s.input_buffer = "hello world".into();
        s.cursor_position = 11;
        s.execute_action(InputAction::MoveWordBackward);
        assert_eq!(s.cursor_position, 6); // After space, at "world"
    }

    #[test]
    fn test_move_word_forward() {
        let mut s = TuiState::new("plan");
        s.input_buffer = "hello world foo".into();
        s.cursor_position = 0;
        s.execute_action(InputAction::MoveWordForward);
        assert_eq!(s.cursor_position, 6); // After "hello ", at "world"
    }

    #[test]
    fn test_transpose_chars_middle() {
        let mut s = TuiState::new("plan");
        s.input_buffer = "abcd".into();
        s.cursor_position = 2; // Between b and c
        s.execute_action(InputAction::TransposeChars);
        assert_eq!(s.input_buffer, "acbd");
        assert_eq!(s.cursor_position, 3); // Cursor moves forward
    }

    #[test]
    fn test_transpose_chars_at_end() {
        let mut s = TuiState::new("plan");
        s.input_buffer = "abcd".into();
        s.cursor_position = 4; // At end
        s.execute_action(InputAction::TransposeChars);
        assert_eq!(s.input_buffer, "abdc"); // Swaps last two
    }
}
