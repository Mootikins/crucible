//! TUI state management

//!
//! Contains TuiState struct and related types for managing UI state.

use crate::tui::streaming::StreamingBuffer;
use crate::tui::InputAction;
use crucible_core::events::SessionEvent;
use crucible_core::traits::chat::cycle_mode_id;
use crucible_rune::EventRing;
use serde_json::Value as JsonValue;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

/// Role of a message in the conversation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

/// Type of popup trigger
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupKind {
    Command,
    AgentOrFile,
}

/// Type of popup item
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupItemKind {
    Command,
    Agent,
    File,
    Note,
}

/// Popup entry displayed in the inline picker
#[derive(Debug, Clone)]
pub struct PopupItem {
    pub kind: PopupItemKind,
    pub title: String,
    pub subtitle: String,
    pub token: String,
    pub score: i32,
    pub available: bool,
}

/// Popup state for inline triggers (/ or @)
#[derive(Debug, Clone)]
pub struct PopupState {
    pub kind: PopupKind,
    pub query: String,
    pub items: Vec<PopupItem>,
    pub selected: usize,
    pub last_update: Instant,
}

impl PopupState {
    pub fn new(kind: PopupKind) -> Self {
        Self {
            kind,
            query: String::new(),
            items: Vec::new(),
            selected: 0,
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
                        .rev()
                        .next()
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
                        .rev()
                        .next()
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
            InputAction::ScrollUp
            | InputAction::ScrollDown
            | InputAction::PageUp
            | InputAction::PageDown
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
            PopupItem {
                kind: PopupItemKind::Command,
                title: "/a".into(),
                subtitle: String::new(),
                token: "/a ".into(),
                score: 1,
                available: true,
            },
            PopupItem {
                kind: PopupItemKind::Command,
                title: "/b".into(),
                subtitle: String::new(),
                token: "/b ".into(),
                score: 1,
                available: true,
            },
        ];
        state.popup = Some(popup);
        state.execute_action(InputAction::MovePopupSelection(-1));
        assert_eq!(state.popup.as_ref().unwrap().selected, 1);
        state.execute_action(InputAction::MovePopupSelection(1));
        assert_eq!(state.popup.as_ref().unwrap().selected, 0);
    }
}
