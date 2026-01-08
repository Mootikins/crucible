//! TUI state management

//!
//! Contains TuiState struct and related types for managing UI state.

// Note: Types are being migrated to state/types/ module
// See state/types/popup.rs and state/types/context.rs for new location

pub mod types;
pub mod actions;
pub mod navigation;

// Re-export types from types/ submodules for backward compatibility
pub use self::types::popup::*;
pub use self::types::context::*;

// Re-export action executor
pub use actions::ActionExecutor;

// Re-export navigation utilities
pub use navigation::{find_word_start_backward, find_word_start_forward};

use crate::tui::conversation_view::ViewState;
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

// Word boundary helpers moved to state/navigation.rs
// MessageRole imported from crucible_core::traits
// PopupKind, ContextKind, ContextAttachment now in state/types/

// PopupItem, PopupItemKind now in state/types/popup.rs
// All PopupItem impl methods moved to state/types/popup.rs

// All PopupItem implementations moved to state/types/popup.rs
// From<PopupItem> and PopupItem trait impls also in state/types/popup.rs

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
    /// ViewState owns ALL view-related fields (input, cursor, popup, conversation, etc.)
    pub view: ViewState,

    /// Non-view concerns owned by TuiState:
    pub streaming: Option<StreamingBuffer>,
    pub mode_name: String,
    pub pending_tools: Vec<ToolCallInfo>,
    pub last_seen_seq: u64,
    pub should_exit: bool,
    pub ctrl_c_count: u8,
    pub last_ctrl_c: Option<Instant>,
    pub status_error: Option<String>,
    /// Context attachments pending for the next message (files, notes)
    pub pending_context: Vec<ContextAttachment>,
    /// Accumulated reasoning from the current response (kept separate from ViewState)
    pub accumulated_reasoning: String,
    #[allow(clippy::type_complexity)] // Complex callback type, not worth a type alias
    output_fn: Option<Box<dyn Fn(&str) + Send + Sync>>,
}

impl TuiState {
    pub fn new(mode_id: impl Into<String>) -> Self {
        let mode_id = mode_id.into();
        let mode_name = crucible_core::traits::chat::mode_display_name(&mode_id).to_string();
        // Create ViewState with default dimensions (will be resized by runner)
        let view = ViewState::new(&mode_id, 80, 24);

        Self {
            view,
            streaming: None,
            mode_name,
            pending_tools: Vec::new(),
            last_seen_seq: 0,
            should_exit: false,
            ctrl_c_count: 0,
            last_ctrl_c: None,
            status_error: None,
            pending_context: Vec::new(),
            accumulated_reasoning: String::new(),
            output_fn: None,
        }
    }

    pub fn with_output<F: Fn(&str) + Send + Sync + 'static>(
        mode_id: impl Into<String>,
        output_fn: F,
    ) -> Self {
        let mode_id = mode_id.into();
        let mode_name = crucible_core::traits::chat::mode_display_name(&mode_id).to_string();
        // Create ViewState with default dimensions (will be resized by runner)
        let view = ViewState::new(&mode_id, 80, 24);

        Self {
            view,
            streaming: None,
            mode_name,
            pending_tools: Vec::new(),
            last_seen_seq: 0,
            should_exit: false,
            ctrl_c_count: 0,
            last_ctrl_c: None,
            status_error: None,
            pending_context: Vec::new(),
            accumulated_reasoning: String::new(),
            output_fn: Some(Box::new(output_fn)),
        }
    }

    // =========================================================================
    // Accessor Methods - delegate to ViewState
    // =========================================================================

    /// Get the input buffer text
    pub fn input(&self) -> &str {
        &self.view.input_buffer
    }

    /// Get mutable reference to input buffer
    pub fn input_mut(&mut self) -> &mut String {
        &mut self.view.input_buffer
    }

    /// Get cursor position
    pub fn cursor(&self) -> usize {
        self.view.cursor_position
    }

    /// Set cursor position
    pub fn set_cursor(&mut self, pos: usize) {
        self.view.cursor_position = pos;
    }

    /// Check if popup is currently shown
    pub fn has_popup(&self) -> bool {
        self.view.popup.is_some()
    }

    /// Get the current popup state
    pub fn popup(&self) -> Option<&crate::tui::components::generic_popup::PopupState> {
        self.view.popup.as_ref()
    }

    /// Get mutable reference to popup state
    pub fn popup_mut(&mut self) -> Option<&mut crate::tui::components::generic_popup::PopupState> {
        self.view.popup.as_mut()
    }

    /// Get mode_id
    pub fn mode_id(&self) -> &str {
        &self.view.mode_id
    }

    /// Get show_reasoning flag
    pub fn show_reasoning(&self) -> bool {
        self.view.show_reasoning
    }

    /// Set show_reasoning flag
    pub fn set_show_reasoning(&mut self, value: bool) {
        self.view.show_reasoning = value;
    }

    /// Get notifications
    pub fn notifications(&self) -> &NotificationState {
        &self.view.notifications
    }

    /// Get mutable notifications
    pub fn notifications_mut(&mut self) -> &mut NotificationState {
        &mut self.view.notifications
    }

    // =========================================================================
    // Reasoning Methods (for thinking models like Qwen3-thinking, DeepSeek-R1)
    // =========================================================================

    /// Append text to the accumulated reasoning buffer
    pub fn append_reasoning(&mut self, text: &str) {
        self.accumulated_reasoning.push_str(text);
    }

    /// Clear the accumulated reasoning buffer
    pub fn clear_reasoning(&mut self) {
        self.accumulated_reasoning.clear();
    }

    // =========================================================================
    // Context Attachment Methods
    // =========================================================================

    /// Add a context attachment, avoiding duplicates by path
    pub fn add_context(&mut self, attachment: ContextAttachment) {
        if !self
            .pending_context
            .iter()
            .any(|c| c.path == attachment.path)
        {
            self.pending_context.push(attachment);
        }
    }

    /// Clear and return all pending context attachments
    pub fn clear_pending_context(&mut self) -> Vec<ContextAttachment> {
        std::mem::take(&mut self.pending_context)
    }

    /// Remove a context attachment by index
    pub fn remove_context(&mut self, index: usize) {
        if index < self.pending_context.len() {
            self.pending_context.remove(index);
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
        self.view.mode_id = mode_id.clone();
        self.mode_name = mode_name;
    }

    /// Execute a non-view action (input-related actions moved to ViewState)
    ///
    /// Input-related actions (InsertChar, DeleteChar, MoveCursor, etc.) are now
    /// handled directly by the runner/harness which has access to ViewState.
    /// This method only handles actions that affect non-view state.
    ///
    /// This method delegates to ActionExecutor for cleaner separation of concerns.
    pub fn execute_action(&mut self, action: crate::tui::InputAction) -> Option<String> {
        ActionExecutor::execute_action(self, action)
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
}

// Popup trigger detection methods removed - these are now handled by the runner/harness
// which has access to ViewState (where input_buffer and cursor_position live).

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
        assert_eq!(s.mode_id(), "plan");
        assert_eq!(s.mode_id(), "plan");
        assert_eq!(s.mode_name, "Plan");
    }

    #[test]
    fn test_tui_state_mode_id_name() {
        assert_eq!(TuiState::new("plan").mode_id(), "plan");
        assert_eq!(TuiState::new("act").mode_id(), "act");
        assert_eq!(TuiState::new("auto").mode_id(), "auto");
    }

    #[test]
    fn test_set_mode_dynamic() {
        let mut s = TuiState::new("plan");
        s.set_mode_dynamic("act".into(), "Act".into());
        assert_eq!(s.mode_id(), "act");
        assert_eq!(s.mode_name, "Act");
        // Unknown mode ID still updates the display strings
        s.set_mode_dynamic("custom".into(), "Custom".into());
        assert_eq!(s.mode_id(), "custom");
        assert_eq!(s.mode_name, "Custom");
    }

    #[test]
    fn test_execute_action_cycle_mode() {
        let mut s = TuiState::new("plan");
        s.execute_action(InputAction::CycleMode);
        assert_eq!(s.mode_id(), "act");
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
        assert_eq!(s.mode_id(), "plan");
        assert_eq!(s.mode_id(), "plan");
        assert_eq!(s.mode_name, "Plan");
    }

    #[test]
    fn test_cycle_mode_updates_all_mode_fields() {
        let mut s = TuiState::new("plan");

        // Cycle Plan -> Act
        s.execute_action(InputAction::CycleMode);
        assert_eq!(s.mode_id(), "act");
        assert_eq!(s.mode_id(), "act");
        assert_eq!(s.mode_name, "Act");

        // Cycle Act -> Auto
        s.execute_action(InputAction::CycleMode);
        assert_eq!(s.mode_id(), "auto");
        assert_eq!(s.mode_id(), "auto");
        assert_eq!(s.mode_name, "Auto");

        // Cycle Auto -> Plan (wraps)
        s.execute_action(InputAction::CycleMode);
        assert_eq!(s.mode_id(), "plan");
        assert_eq!(s.mode_id(), "plan");
        assert_eq!(s.mode_name, "Plan");
    }

    // NOTE: Popup trigger detection removed - this is now handled by the runner
    // which has access to ViewState and checks for trigger characters.

    // NOTE: Popup selection and viewport tests moved to generic_popup.rs
    // The new PopupState handles selection wrapping and viewport management internally.

    #[test]
    fn test_tui_state_has_notifications() {
        let state = TuiState::new("plan");
        assert!(state.notifications().is_empty());
    }

    // =========================================================================
    // Readline Action Tests
    // =========================================================================

    #[test]
    fn test_delete_word_backward() {
        let mut s = TuiState::new("plan");
        *s.input_mut() = "hello world".into();
        s.set_cursor(11); // at end
        s.execute_action(InputAction::DeleteWordBackward);
        assert_eq!(s.input(), "hello ");
        assert_eq!(s.cursor(), 6);
    }

    #[test]
    fn test_delete_word_backward_multiple_spaces() {
        let mut s = TuiState::new("plan");
        *s.input_mut() = "hello   world".into();
        s.set_cursor( 13);
        s.execute_action(InputAction::DeleteWordBackward);
        assert_eq!(s.input(), "hello   ");
        assert_eq!(s.cursor(), 8);
    }

    #[test]
    fn test_delete_to_line_start() {
        let mut s = TuiState::new("plan");
        *s.input_mut() = "hello world".into();
        s.set_cursor( 6);
        s.execute_action(InputAction::DeleteToLineStart);
        assert_eq!(s.input(), "world");
        assert_eq!(s.cursor(), 0);
    }

    #[test]
    fn test_delete_to_line_end() {
        let mut s = TuiState::new("plan");
        *s.input_mut() = "hello world".into();
        s.set_cursor( 5);
        s.execute_action(InputAction::DeleteToLineEnd);
        assert_eq!(s.input(), "hello");
        assert_eq!(s.cursor(), 5);
    }

    #[test]
    fn test_move_cursor_to_start() {
        let mut s = TuiState::new("plan");
        *s.input_mut() = "hello world".into();
        s.set_cursor( 6);
        s.execute_action(InputAction::MoveCursorToStart);
        assert_eq!(s.cursor(), 0);
    }

    #[test]
    fn test_move_cursor_to_end() {
        let mut s = TuiState::new("plan");
        *s.input_mut() = "hello world".into();
        s.set_cursor( 0);
        s.execute_action(InputAction::MoveCursorToEnd);
        assert_eq!(s.cursor(), 11);
    }

    #[test]
    fn test_move_word_backward() {
        let mut s = TuiState::new("plan");
        *s.input_mut() = "hello world".into();
        s.set_cursor( 11);
        s.execute_action(InputAction::MoveWordBackward);
        assert_eq!(s.cursor(), 6); // After space, at "world"
    }

    #[test]
    fn test_move_word_forward() {
        let mut s = TuiState::new("plan");
        *s.input_mut() = "hello world foo".into();
        s.set_cursor( 0);
        s.execute_action(InputAction::MoveWordForward);
        assert_eq!(s.cursor(), 6); // After "hello ", at "world"
    }

    #[test]
    fn test_transpose_chars_middle() {
        let mut s = TuiState::new("plan");
        *s.input_mut() = "abcd".into();
        s.set_cursor(2); // Between b and c
        s.execute_action(InputAction::TransposeChars);
        assert_eq!(s.input(), "acbd");
        assert_eq!(s.cursor(), 3); // Cursor moves forward
    }

    #[test]
    fn test_transpose_chars_at_end() {
        let mut s = TuiState::new("plan");
        *s.input_mut() = "abcd".into();
        s.set_cursor(4); // At end
        s.execute_action(InputAction::TransposeChars);
        assert_eq!(s.input(), "abdc"); // Swaps last two
    }

    // =========================================================================
    // Context Attachment Tests
    // =========================================================================

    #[test]
    fn test_pending_context_initially_empty() {
        let state = TuiState::new("plan");
        assert!(state.pending_context.is_empty());
    }

    #[test]
    fn test_add_context_file() {
        let mut state = TuiState::new("plan");
        state.add_context(ContextAttachment {
            kind: ContextKind::File,
            path: "/test/file.rs".into(),
            display_name: "file.rs".into(),
        });
        assert_eq!(state.pending_context.len(), 1);
        assert_eq!(state.pending_context[0].kind, ContextKind::File);
        assert_eq!(state.pending_context[0].path, "/test/file.rs");
    }

    #[test]
    fn test_add_context_note() {
        let mut state = TuiState::new("plan");
        state.add_context(ContextAttachment {
            kind: ContextKind::Note,
            path: "Project/README".into(),
            display_name: "README".into(),
        });
        assert_eq!(state.pending_context.len(), 1);
        assert_eq!(state.pending_context[0].kind, ContextKind::Note);
    }

    #[test]
    fn test_add_context_deduplicates_by_path() {
        let mut state = TuiState::new("plan");
        state.add_context(ContextAttachment {
            kind: ContextKind::File,
            path: "/test/file.rs".into(),
            display_name: "file.rs".into(),
        });
        // Adding same path again should not duplicate
        state.add_context(ContextAttachment {
            kind: ContextKind::File,
            path: "/test/file.rs".into(),
            display_name: "file.rs (copy)".into(),
        });
        assert_eq!(state.pending_context.len(), 1);
    }

    #[test]
    fn test_add_multiple_contexts() {
        let mut state = TuiState::new("plan");
        state.add_context(ContextAttachment {
            kind: ContextKind::File,
            path: "/test/file1.rs".into(),
            display_name: "file1.rs".into(),
        });
        state.add_context(ContextAttachment {
            kind: ContextKind::File,
            path: "/test/file2.rs".into(),
            display_name: "file2.rs".into(),
        });
        state.add_context(ContextAttachment {
            kind: ContextKind::Note,
            path: "README".into(),
            display_name: "README".into(),
        });
        assert_eq!(state.pending_context.len(), 3);
    }

    #[test]
    fn test_clear_pending_context() {
        let mut state = TuiState::new("plan");
        state.add_context(ContextAttachment {
            kind: ContextKind::File,
            path: "/test/file.rs".into(),
            display_name: "file.rs".into(),
        });
        state.add_context(ContextAttachment {
            kind: ContextKind::Note,
            path: "README".into(),
            display_name: "README".into(),
        });

        let cleared = state.clear_pending_context();
        assert_eq!(cleared.len(), 2);
        assert!(state.pending_context.is_empty());
    }

    #[test]
    fn test_remove_context_by_index() {
        let mut state = TuiState::new("plan");
        state.add_context(ContextAttachment {
            kind: ContextKind::File,
            path: "/test/file1.rs".into(),
            display_name: "file1.rs".into(),
        });
        state.add_context(ContextAttachment {
            kind: ContextKind::File,
            path: "/test/file2.rs".into(),
            display_name: "file2.rs".into(),
        });

        state.remove_context(0);
        assert_eq!(state.pending_context.len(), 1);
        assert_eq!(state.pending_context[0].path, "/test/file2.rs");
    }

    #[test]
    fn test_remove_context_invalid_index() {
        let mut state = TuiState::new("plan");
        state.add_context(ContextAttachment {
            kind: ContextKind::File,
            path: "/test/file.rs".into(),
            display_name: "file.rs".into(),
        });

        // Should not panic with invalid index
        state.remove_context(100);
        assert_eq!(state.pending_context.len(), 1);
    }

    // =========================================================================
    // Reasoning Toggle Tests (TDD - RED PHASE)
    // =========================================================================

    #[test]
    fn test_show_reasoning_default_false() {
        // Reasoning should be hidden by default
        let state = TuiState::new("plan");
        assert!(!state.show_reasoning());
    }

    #[test]
    fn test_accumulated_reasoning_default_empty() {
        // Accumulated reasoning should start empty
        let state = TuiState::new("plan");
        assert!(state.accumulated_reasoning.is_empty());
    }

    #[test]
    fn test_toggle_reasoning_flips_state() {
        let mut state = TuiState::new("plan");
        assert!(!state.show_reasoning());

        state.execute_action(InputAction::ToggleReasoning);
        assert!(state.show_reasoning());

        state.execute_action(InputAction::ToggleReasoning);
        assert!(!state.show_reasoning());
    }

    #[test]
    fn test_append_reasoning() {
        let mut state = TuiState::new("plan");
        state.append_reasoning("Thinking about ");
        state.append_reasoning("the problem...");
        assert_eq!(state.accumulated_reasoning, "Thinking about the problem...");
    }

    #[test]
    fn test_clear_reasoning() {
        let mut state = TuiState::new("plan");
        state.append_reasoning("Some reasoning");
        assert!(!state.accumulated_reasoning.is_empty());

        state.clear_reasoning();
        assert!(state.accumulated_reasoning.is_empty());
    }

    // =========================================================================
    // PopupItem to PopupEntry Conversion Tests
    // =========================================================================

    #[test]
    fn test_popup_item_to_entry_conversion() {
        use crucible_core::types::PopupEntry;

        let cmd = PopupItem::cmd("search")
            .desc("Search the vault")
            .with_score(100);

        let entry: PopupEntry = cmd.into();

        assert_eq!(entry.label, "/search");
        assert_eq!(entry.description.as_deref(), Some("Search the vault"));
        // Data should contain kind information
        let data = entry.data.unwrap();
        assert_eq!(data["kind"], "command");
    }

    #[test]
    fn test_popup_item_agent_to_entry() {
        use crucible_core::types::PopupEntry;

        let agent = PopupItem::agent("coder")
            .desc("Coding assistant")
            .with_score(50);

        let entry: PopupEntry = agent.into();

        assert_eq!(entry.label, "@coder");
        assert_eq!(entry.description.as_deref(), Some("Coding assistant"));
        let data = entry.data.unwrap();
        assert_eq!(data["kind"], "agent");
    }

    #[test]
    fn test_popup_item_file_to_entry() {
        use crucible_core::types::PopupEntry;

        let file = PopupItem::file("src/main.rs").with_score(75);

        let entry: PopupEntry = file.into();

        assert_eq!(entry.label, "src/main.rs");
        assert!(entry.description.is_none());
        let data = entry.data.unwrap();
        assert_eq!(data["kind"], "file");
    }

    #[test]
    fn test_popup_item_note_to_entry() {
        use crucible_core::types::PopupEntry;

        let note = PopupItem::note("Project/README").with_score(80);

        let entry: PopupEntry = note.into();

        assert_eq!(entry.label, "Project/README");
        assert!(entry.description.is_none());
        let data = entry.data.unwrap();
        assert_eq!(data["kind"], "note");
    }

    #[test]
    fn test_popup_item_skill_to_entry() {
        use crucible_core::types::PopupEntry;

        let skill = PopupItem::skill("commit")
            .desc("Create git commit")
            .with_scope("user")
            .with_score(90);

        let entry: PopupEntry = skill.into();

        assert_eq!(entry.label, "skill:commit");
        assert_eq!(
            entry.description.as_deref(),
            Some("Create git commit (user)")
        );
        let data = entry.data.unwrap();
        assert_eq!(data["kind"], "skill");
    }

    #[test]
    fn test_popup_item_repl_to_entry() {
        use crucible_core::types::PopupEntry;

        let repl = PopupItem::repl("quit").desc("Exit the REPL").with_score(50);

        let entry: PopupEntry = repl.into();

        assert_eq!(entry.label, ":quit");
        assert_eq!(entry.description.as_deref(), Some("Exit the REPL"));
        let data = entry.data.unwrap();
        assert_eq!(data["kind"], "repl");
    }
}
