//! Rune bindings for SessionEvent.
//!
//! This module provides Rune-compatible wrappers for `SessionEvent` and related types.
//! Rather than adding Rune as a dependency to crucible-core, we create wrapper types
//! here that implement the necessary Rune traits.
//!
//! ## Design
//!
//! - `RuneSessionEvent` wraps the core `SessionEvent` enum
//! - Implements `#[derive(Any)]` for Rune integration
//! - Provides constructors, getters, and protocol implementations
//!
//! ## Usage from Rune
//!
//! ```rune
//! // Check event type
//! if event.event_type() == "message_received" {
//!     println!("Got message: {}", event.content());
//! }
//!
//! // Construction
//! let event = RuneSessionEvent::custom("my_event", #{});
//! ctx.emit(event);
//! ```

use rune::alloc::fmt::TryWrite;
use rune::runtime::{Formatter, Value, VmResult};
use rune::{Any, ContextError, Module};
use serde_json::Value as JsonValue;
use std::path::PathBuf;

use crate::mcp_types::rune_to_json;
use crucible_core::events::{FileChangeKind, NoteChangeType, SessionEvent};

/// Rune-compatible wrapper for SessionEvent.
///
/// This wrapper allows Rune scripts to:
/// - Construct new events
/// - Access event fields via getter methods
/// - Display events for debugging
#[derive(Debug, Clone, Any)]
#[rune(item = ::crucible)]
pub struct RuneSessionEvent {
    /// The wrapped core SessionEvent.
    inner: SessionEvent,
}

impl RuneSessionEvent {
    /// Create a new RuneSessionEvent from a core SessionEvent.
    pub fn new(event: SessionEvent) -> Self {
        Self { inner: event }
    }

    /// Get the inner SessionEvent.
    pub fn into_inner(self) -> SessionEvent {
        self.inner
    }

    /// Get a reference to the inner SessionEvent.
    pub fn inner(&self) -> &SessionEvent {
        &self.inner
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Constructors (Rust implementation - for tests)
    // ─────────────────────────────────────────────────────────────────────────

    /// Create a MessageReceived event (impl).
    pub fn message_received_impl(content: String, participant_id: String) -> Self {
        Self::new(SessionEvent::MessageReceived {
            content,
            participant_id,
        })
    }

    /// Create an AgentResponded event (impl).
    pub fn agent_responded_impl(content: String) -> Self {
        Self::new(SessionEvent::AgentResponded {
            content,
            tool_calls: vec![],
        })
    }

    /// Create an AgentThinking event (impl).
    pub fn agent_thinking_impl(thought: String) -> Self {
        Self::new(SessionEvent::AgentThinking { thought })
    }

    /// Create a ToolCalled event (Rust version with JSON args).
    pub fn tool_called_json(name: String, args: serde_json::Value) -> Self {
        Self::new(SessionEvent::ToolCalled { name, args })
    }

    /// Create a ToolCompleted event (impl).
    pub fn tool_completed_impl(name: String, result: String) -> Self {
        Self::new(SessionEvent::ToolCompleted {
            name,
            result,
            error: None,
        })
    }

    /// Create a ToolCompleted event with error (impl).
    pub fn tool_error_impl(name: String, result: String, error: String) -> Self {
        Self::new(SessionEvent::ToolCompleted {
            name,
            result,
            error: Some(error),
        })
    }

    /// Create a Custom event (Rust version with JSON payload).
    pub fn custom_json(name: String, payload: serde_json::Value) -> Self {
        Self::new(SessionEvent::Custom { name, payload })
    }

    /// Create a TextDelta event (impl).
    pub fn text_delta_impl(delta: String, seq: u64) -> Self {
        Self::new(SessionEvent::TextDelta { delta, seq })
    }

    /// Create a SessionEnded event (impl).
    pub fn session_ended_impl(reason: String) -> Self {
        Self::new(SessionEvent::SessionEnded { reason })
    }

    /// Create a NoteCreated event (impl).
    pub fn note_created_impl(path: String, title: Option<String>) -> Self {
        Self::new(SessionEvent::NoteCreated {
            path: PathBuf::from(path),
            title,
        })
    }

    /// Create a NoteModified event (impl).
    pub fn note_modified_impl(path: String) -> Self {
        Self::new(SessionEvent::NoteModified {
            path: PathBuf::from(path),
            change_type: NoteChangeType::Content,
        })
    }

    /// Create a FileChanged event (impl).
    pub fn file_changed_impl(path: String, kind: FileChangeKind) -> Self {
        Self::new(SessionEvent::FileChanged {
            path: PathBuf::from(path),
            kind,
        })
    }

    /// Create a FileDeleted event (impl).
    pub fn file_deleted_impl(path: String) -> Self {
        Self::new(SessionEvent::FileDeleted {
            path: PathBuf::from(path),
        })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Constructors (Rune bindings - registered with function_meta)
    // ─────────────────────────────────────────────────────────────────────────

    /// Create a MessageReceived event.
    #[rune::function(path = Self::message_received)]
    pub fn message_received(content: String, participant_id: String) -> Self {
        Self::message_received_impl(content, participant_id)
    }

    /// Create an AgentResponded event.
    #[rune::function(path = Self::agent_responded)]
    pub fn agent_responded(content: String) -> Self {
        Self::agent_responded_impl(content)
    }

    /// Create an AgentThinking event.
    #[rune::function(path = Self::agent_thinking)]
    pub fn agent_thinking(thought: String) -> Self {
        Self::agent_thinking_impl(thought)
    }

    /// Create a ToolCalled event.
    #[rune::function(path = Self::tool_called)]
    pub fn tool_called(name: String, args: Value) -> Self {
        let json_args = value_to_json(args);
        Self::new(SessionEvent::ToolCalled {
            name,
            args: json_args,
        })
    }

    /// Create a ToolCompleted event (success).
    #[rune::function(path = Self::tool_completed)]
    pub fn tool_completed(name: String, result: String) -> Self {
        Self::tool_completed_impl(name, result)
    }

    /// Create a ToolCompleted event (with error).
    #[rune::function(path = Self::tool_error)]
    pub fn tool_error(name: String, result: String, error: String) -> Self {
        Self::tool_error_impl(name, result, error)
    }

    /// Create a Custom event.
    #[rune::function(path = Self::custom)]
    pub fn custom(name: String, payload: Value) -> Self {
        let json_payload = value_to_json(payload);
        Self::new(SessionEvent::Custom {
            name,
            payload: json_payload,
        })
    }

    /// Create a TextDelta event (streaming).
    #[rune::function(path = Self::text_delta)]
    pub fn text_delta(delta: String, seq: i64) -> Self {
        Self::text_delta_impl(delta, seq as u64)
    }

    /// Create a SessionEnded event.
    #[rune::function(path = Self::session_ended)]
    pub fn session_ended(reason: String) -> Self {
        Self::session_ended_impl(reason)
    }

    /// Create a NoteCreated event.
    #[rune::function(path = Self::note_created)]
    pub fn note_created(path: String, title: Option<String>) -> Self {
        Self::note_created_impl(path, title)
    }

    /// Create a NoteModified event.
    #[rune::function(path = Self::note_modified)]
    pub fn note_modified(path: String) -> Self {
        Self::note_modified_impl(path)
    }

    /// Create a FileChanged event.
    #[rune::function(path = Self::file_changed)]
    pub fn file_changed(path: String, kind: String) -> Self {
        let change_kind = match kind.as_str() {
            "created" => FileChangeKind::Created,
            _ => FileChangeKind::Modified,
        };
        Self::file_changed_impl(path, change_kind)
    }

    /// Create a FileDeleted event.
    #[rune::function(path = Self::file_deleted)]
    pub fn file_deleted(path: String) -> Self {
        Self::file_deleted_impl(path)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Getters (for field access) - Rune bindings
    // ─────────────────────────────────────────────────────────────────────────

    /// Get the event type name (Rune binding).
    #[rune::function(instance)]
    pub fn event_type(&self) -> String {
        self.get_event_type()
    }

    /// Check if this is a tool event (Rune binding).
    #[rune::function(instance)]
    pub fn is_tool_event(&self) -> bool {
        self.get_is_tool_event()
    }

    /// Check if this is a note event (Rune binding).
    #[rune::function(instance)]
    pub fn is_note_event(&self) -> bool {
        self.get_is_note_event()
    }

    /// Check if this is a lifecycle event (Rune binding).
    #[rune::function(instance)]
    pub fn is_lifecycle_event(&self) -> bool {
        self.get_is_lifecycle_event()
    }

    /// Get the content field (Rune binding).
    #[rune::function(instance)]
    pub fn content(&self) -> Option<String> {
        self.get_content()
    }

    /// Get the participant_id field (Rune binding).
    #[rune::function(instance)]
    pub fn participant_id(&self) -> Option<String> {
        self.get_participant_id()
    }

    /// Get the tool name field (Rune binding).
    #[rune::function(instance)]
    pub fn tool_name(&self) -> Option<String> {
        self.get_tool_name()
    }

    /// Get the path field (Rune binding).
    #[rune::function(instance)]
    pub fn path(&self) -> Option<String> {
        self.get_path()
    }

    /// Get the custom event name (Rune binding).
    #[rune::function(instance)]
    pub fn custom_name(&self) -> Option<String> {
        self.get_custom_name()
    }

    /// Get the thought field (Rune binding).
    #[rune::function(instance)]
    pub fn thought(&self) -> Option<String> {
        self.get_thought()
    }

    /// Get the error field (Rune binding).
    #[rune::function(instance)]
    pub fn error(&self) -> Option<String> {
        self.get_error()
    }

    /// Get a debug string representation (Rune binding).
    #[rune::function(instance)]
    pub fn debug_string(&self) -> String {
        format!("{:?}", self.inner)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Getters (for Rust access) - Implementation methods
    // ─────────────────────────────────────────────────────────────────────────

    /// Get the event type name.
    pub fn get_event_type(&self) -> String {
        self.inner.event_type().to_string()
    }

    /// Check if this is a tool event.
    pub fn get_is_tool_event(&self) -> bool {
        self.inner.is_tool_event()
    }

    /// Check if this is a note event.
    pub fn get_is_note_event(&self) -> bool {
        self.inner.is_note_event()
    }

    /// Check if this is a lifecycle event.
    pub fn get_is_lifecycle_event(&self) -> bool {
        self.inner.is_lifecycle_event()
    }

    /// Get the content field (if present).
    pub fn get_content(&self) -> Option<String> {
        match &self.inner {
            SessionEvent::MessageReceived { content, .. } => Some(content.clone()),
            SessionEvent::AgentResponded { content, .. } => Some(content.clone()),
            _ => None,
        }
    }

    /// Get the participant_id field (if present).
    pub fn get_participant_id(&self) -> Option<String> {
        match &self.inner {
            SessionEvent::MessageReceived { participant_id, .. } => Some(participant_id.clone()),
            _ => None,
        }
    }

    /// Get the tool name field (if present).
    pub fn get_tool_name(&self) -> Option<String> {
        match &self.inner {
            SessionEvent::ToolCalled { name, .. } => Some(name.clone()),
            SessionEvent::ToolCompleted { name, .. } => Some(name.clone()),
            SessionEvent::ToolDiscovered { name, .. } => Some(name.clone()),
            _ => None,
        }
    }

    /// Get the path field (if present).
    pub fn get_path(&self) -> Option<String> {
        match &self.inner {
            SessionEvent::NoteParsed { path, .. } => Some(path.display().to_string()),
            SessionEvent::NoteCreated { path, .. } => Some(path.display().to_string()),
            SessionEvent::NoteModified { path, .. } => Some(path.display().to_string()),
            SessionEvent::FileChanged { path, .. } => Some(path.display().to_string()),
            SessionEvent::FileDeleted { path, .. } => Some(path.display().to_string()),
            SessionEvent::FileMoved { to, .. } => Some(to.display().to_string()),
            _ => None,
        }
    }

    /// Get the custom event name (if Custom variant).
    pub fn get_custom_name(&self) -> Option<String> {
        match &self.inner {
            SessionEvent::Custom { name, .. } => Some(name.clone()),
            _ => None,
        }
    }

    /// Get the thought field (if AgentThinking).
    pub fn get_thought(&self) -> Option<String> {
        match &self.inner {
            SessionEvent::AgentThinking { thought } => Some(thought.clone()),
            _ => None,
        }
    }

    /// Get the error field (if present).
    pub fn get_error(&self) -> Option<String> {
        match &self.inner {
            SessionEvent::ToolCompleted { error, .. } => error.clone(),
            SessionEvent::SubagentFailed { error, .. } => Some(error.clone()),
            SessionEvent::EmbeddingFailed { error, .. } => Some(error.clone()),
            _ => None,
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Protocol implementations
    // ─────────────────────────────────────────────────────────────────────────

    /// Implement DISPLAY_FMT protocol for debugging.
    #[rune::function(protocol = DISPLAY_FMT)]
    fn display_fmt(&self, f: &mut Formatter) -> VmResult<()> {
        if f.try_write_str("SessionEvent::").is_err() {
            return VmResult::Ok(());
        }
        let _ = f.try_write_str(self.inner.event_type());
        VmResult::Ok(())
    }

    /// Implement PARTIAL_EQ protocol for comparisons.
    #[rune::function(protocol = PARTIAL_EQ)]
    fn partial_eq(&self, other: &Self) -> bool {
        // Compare event types for basic equality
        self.inner.event_type() == other.inner.event_type()
    }
}

impl From<SessionEvent> for RuneSessionEvent {
    fn from(event: SessionEvent) -> Self {
        Self::new(event)
    }
}

impl From<RuneSessionEvent> for SessionEvent {
    fn from(wrapper: RuneSessionEvent) -> Self {
        wrapper.into_inner()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper types
// ─────────────────────────────────────────────────────────────────────────────

/// Rune-compatible wrapper for FileChangeKind.
#[derive(Debug, Clone, Copy, Any)]
#[rune(item = ::crucible)]
pub struct RuneFileChangeKind {
    inner: FileChangeKind,
}

impl RuneFileChangeKind {
    /// Create Created variant (impl).
    pub fn created_impl() -> Self {
        Self {
            inner: FileChangeKind::Created,
        }
    }

    /// Create Modified variant (impl).
    pub fn modified_impl() -> Self {
        Self {
            inner: FileChangeKind::Modified,
        }
    }

    /// Create Created variant.
    #[rune::function(path = Self::created)]
    pub fn created() -> Self {
        Self::created_impl()
    }

    /// Create Modified variant.
    #[rune::function(path = Self::modified)]
    pub fn modified() -> Self {
        Self::modified_impl()
    }

    /// Get the inner value.
    pub fn into_inner(self) -> FileChangeKind {
        self.inner
    }

    /// Get the string representation (impl).
    pub fn to_string_impl(&self) -> String {
        match self.inner {
            FileChangeKind::Created => "created".to_string(),
            FileChangeKind::Modified => "modified".to_string(),
        }
    }

    /// Get the string representation.
    #[rune::function(path = Self::to_string)]
    pub fn to_string(&self) -> String {
        self.to_string_impl()
    }

    #[rune::function(protocol = DISPLAY_FMT)]
    fn display_fmt(&self, f: &mut Formatter) -> VmResult<()> {
        let s = match self.inner {
            FileChangeKind::Created => "created",
            FileChangeKind::Modified => "modified",
        };
        let _ = f.try_write_str(s);
        VmResult::Ok(())
    }
}

/// Rune-compatible wrapper for NoteChangeType.
#[derive(Debug, Clone, Copy, Any)]
#[rune(item = ::crucible)]
pub struct RuneNoteChangeType {
    inner: NoteChangeType,
}

impl RuneNoteChangeType {
    /// Create Content variant (impl).
    pub fn content_impl() -> Self {
        Self {
            inner: NoteChangeType::Content,
        }
    }

    /// Create Frontmatter variant (impl).
    pub fn frontmatter_impl() -> Self {
        Self {
            inner: NoteChangeType::Frontmatter,
        }
    }

    /// Create Links variant (impl).
    pub fn links_impl() -> Self {
        Self {
            inner: NoteChangeType::Links,
        }
    }

    /// Create Tags variant (impl).
    pub fn tags_impl() -> Self {
        Self {
            inner: NoteChangeType::Tags,
        }
    }

    /// Create Content variant.
    #[rune::function(path = Self::content)]
    pub fn content() -> Self {
        Self::content_impl()
    }

    /// Create Frontmatter variant.
    #[rune::function(path = Self::frontmatter)]
    pub fn frontmatter() -> Self {
        Self::frontmatter_impl()
    }

    /// Create Links variant.
    #[rune::function(path = Self::links)]
    pub fn links() -> Self {
        Self::links_impl()
    }

    /// Create Tags variant.
    #[rune::function(path = Self::tags)]
    pub fn tags() -> Self {
        Self::tags_impl()
    }

    /// Get the inner value.
    pub fn into_inner(self) -> NoteChangeType {
        self.inner
    }

    /// Get the string representation (impl).
    pub fn to_string_impl(&self) -> String {
        match self.inner {
            NoteChangeType::Content => "content".to_string(),
            NoteChangeType::Frontmatter => "frontmatter".to_string(),
            NoteChangeType::Links => "links".to_string(),
            NoteChangeType::Tags => "tags".to_string(),
        }
    }

    /// Get the string representation.
    #[rune::function(path = Self::to_string)]
    pub fn to_string(&self) -> String {
        self.to_string_impl()
    }

    #[rune::function(protocol = DISPLAY_FMT)]
    fn display_fmt(&self, f: &mut Formatter) -> VmResult<()> {
        let s = match self.inner {
            NoteChangeType::Content => "content",
            NoteChangeType::Frontmatter => "frontmatter",
            NoteChangeType::Links => "links",
            NoteChangeType::Tags => "tags",
        };
        let _ = f.try_write_str(s);
        VmResult::Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rune context for event emission
// ─────────────────────────────────────────────────────────────────────────────

/// Rune-compatible event context for handlers.
///
/// Provides the `emit()` method for Rune handlers to emit events.
#[derive(Debug, Clone, Any)]
#[rune(item = ::crucible)]
pub struct RuneEventContext {
    /// Accumulated events to emit.
    emitted: Vec<RuneSessionEvent>,
    /// Metadata storage.
    metadata: std::collections::HashMap<String, JsonValue>,
}

impl RuneEventContext {
    /// Create a new empty context.
    pub fn new() -> Self {
        Self {
            emitted: Vec::new(),
            metadata: std::collections::HashMap::new(),
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Implementation methods (for Rust tests)
    // ─────────────────────────────────────────────────────────────────────────

    /// Emit an event (impl).
    pub fn emit_impl(&mut self, event: RuneSessionEvent) {
        self.emitted.push(event);
    }

    /// Get the number of emitted events (impl).
    pub fn emitted_count_impl(&self) -> i64 {
        self.emitted.len() as i64
    }

    /// Take all emitted events.
    pub fn take_emitted(&mut self) -> Vec<SessionEvent> {
        self.emitted.drain(..).map(|e| e.into_inner()).collect()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Rune bindings
    // ─────────────────────────────────────────────────────────────────────────

    /// Emit an event.
    #[rune::function(path = Self::emit)]
    pub fn emit(&mut self, event: RuneSessionEvent) {
        self.emit_impl(event);
    }

    /// Emit a custom event by name and payload.
    #[rune::function(path = Self::emit_custom)]
    pub fn emit_custom(&mut self, name: String, payload: Value) {
        let json_payload = value_to_json(payload);
        let event = RuneSessionEvent::new(SessionEvent::Custom {
            name,
            payload: json_payload,
        });
        self.emitted.push(event);
    }

    /// Set metadata.
    #[rune::function(path = Self::set)]
    pub fn set(&mut self, key: String, value: Value) {
        let json_value = value_to_json(value);
        self.metadata.insert(key, json_value);
    }

    /// Get metadata.
    #[rune::function(path = Self::get)]
    pub fn get(&self, key: &str) -> Option<String> {
        self.metadata.get(key).map(|v| v.to_string())
    }

    /// Get the number of emitted events.
    #[rune::function(path = Self::emitted_count)]
    pub fn emitted_count(&self) -> i64 {
        self.emitted_count_impl()
    }

    #[rune::function(protocol = DISPLAY_FMT)]
    fn display_fmt(&self, f: &mut Formatter) -> VmResult<()> {
        let _ = f.try_write_str("EventContext(emitted=");
        let _ = f.try_write_str(&self.emitted.len().to_string());
        let _ = f.try_write_str(")");
        VmResult::Ok(())
    }
}

impl Default for RuneEventContext {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Module registration
// ─────────────────────────────────────────────────────────────────────────────

/// Install the SessionEvent module into a Rune context.
pub fn module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate("crucible")?;

    // Register RuneSessionEvent
    module.ty::<RuneSessionEvent>()?;
    module.function_meta(RuneSessionEvent::message_received)?;
    module.function_meta(RuneSessionEvent::agent_responded)?;
    module.function_meta(RuneSessionEvent::agent_thinking)?;
    module.function_meta(RuneSessionEvent::tool_called)?;
    module.function_meta(RuneSessionEvent::tool_completed)?;
    module.function_meta(RuneSessionEvent::tool_error)?;
    module.function_meta(RuneSessionEvent::custom)?;
    module.function_meta(RuneSessionEvent::text_delta)?;
    module.function_meta(RuneSessionEvent::session_ended)?;
    module.function_meta(RuneSessionEvent::note_created)?;
    module.function_meta(RuneSessionEvent::note_modified)?;
    module.function_meta(RuneSessionEvent::file_changed)?;
    module.function_meta(RuneSessionEvent::file_deleted)?;
    module.function_meta(RuneSessionEvent::event_type)?;
    module.function_meta(RuneSessionEvent::is_tool_event)?;
    module.function_meta(RuneSessionEvent::is_note_event)?;
    module.function_meta(RuneSessionEvent::is_lifecycle_event)?;
    module.function_meta(RuneSessionEvent::content)?;
    module.function_meta(RuneSessionEvent::participant_id)?;
    module.function_meta(RuneSessionEvent::tool_name)?;
    module.function_meta(RuneSessionEvent::path)?;
    module.function_meta(RuneSessionEvent::custom_name)?;
    module.function_meta(RuneSessionEvent::thought)?;
    module.function_meta(RuneSessionEvent::error)?;
    module.function_meta(RuneSessionEvent::debug_string)?;
    module.function_meta(RuneSessionEvent::display_fmt)?;
    module.function_meta(RuneSessionEvent::partial_eq)?;

    // Register helper types
    module.ty::<RuneFileChangeKind>()?;
    module.function_meta(RuneFileChangeKind::created)?;
    module.function_meta(RuneFileChangeKind::modified)?;
    module.function_meta(RuneFileChangeKind::to_string)?;
    module.function_meta(RuneFileChangeKind::display_fmt)?;

    module.ty::<RuneNoteChangeType>()?;
    module.function_meta(RuneNoteChangeType::content)?;
    module.function_meta(RuneNoteChangeType::frontmatter)?;
    module.function_meta(RuneNoteChangeType::links)?;
    module.function_meta(RuneNoteChangeType::tags)?;
    module.function_meta(RuneNoteChangeType::to_string)?;
    module.function_meta(RuneNoteChangeType::display_fmt)?;

    // Register RuneEventContext
    module.ty::<RuneEventContext>()?;
    module.function_meta(RuneEventContext::emit)?;
    module.function_meta(RuneEventContext::emit_custom)?;
    module.function_meta(RuneEventContext::set)?;
    module.function_meta(RuneEventContext::get)?;
    module.function_meta(RuneEventContext::emitted_count)?;
    module.function_meta(RuneEventContext::display_fmt)?;

    Ok(module)
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper functions
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a Rune Value to serde_json::Value.
fn value_to_json(value: Value) -> JsonValue {
    // Use the existing conversion from mcp_types
    rune_to_json(&value).unwrap_or(JsonValue::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rune_session_event_from_core() {
        let core_event = SessionEvent::MessageReceived {
            content: "Hello".into(),
            participant_id: "user".into(),
        };
        let rune_event = RuneSessionEvent::from(core_event);

        assert_eq!(rune_event.get_event_type(), "message_received");
        assert_eq!(rune_event.get_content(), Some("Hello".to_string()));
        assert_eq!(rune_event.get_participant_id(), Some("user".to_string()));
    }

    #[test]
    fn test_rune_session_event_constructors() {
        let event = RuneSessionEvent::message_received_impl("Hi".into(), "user".into());
        assert_eq!(event.get_event_type(), "message_received");

        let event = RuneSessionEvent::agent_thinking_impl("Processing...".into());
        assert_eq!(event.get_event_type(), "agent_thinking");
        assert_eq!(event.get_thought(), Some("Processing...".to_string()));

        let event = RuneSessionEvent::tool_completed_impl("search".into(), "results".into());
        assert_eq!(event.get_event_type(), "tool_completed");
        assert_eq!(event.get_tool_name(), Some("search".to_string()));
    }

    #[test]
    fn test_rune_session_event_into_inner() {
        let rune_event = RuneSessionEvent::message_received_impl("Test".into(), "user".into());
        let core_event: SessionEvent = rune_event.into();

        match core_event {
            SessionEvent::MessageReceived {
                content,
                participant_id,
            } => {
                assert_eq!(content, "Test");
                assert_eq!(participant_id, "user");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_rune_event_context() {
        let mut ctx = RuneEventContext::new();
        assert_eq!(ctx.emitted_count_impl(), 0);

        ctx.emit_impl(RuneSessionEvent::agent_thinking_impl("Thinking...".into()));
        assert_eq!(ctx.emitted_count_impl(), 1);

        let events = ctx.take_emitted();
        assert_eq!(events.len(), 1);
        assert_eq!(ctx.emitted_count_impl(), 0);
    }

    #[test]
    fn test_file_change_kind() {
        let kind = RuneFileChangeKind::created_impl();
        assert_eq!(kind.to_string_impl(), "created");

        let kind = RuneFileChangeKind::modified_impl();
        assert_eq!(kind.to_string_impl(), "modified");
    }

    #[test]
    fn test_note_change_type() {
        let kind = RuneNoteChangeType::content_impl();
        assert_eq!(kind.to_string_impl(), "content");

        let kind = RuneNoteChangeType::frontmatter_impl();
        assert_eq!(kind.to_string_impl(), "frontmatter");

        let kind = RuneNoteChangeType::links_impl();
        assert_eq!(kind.to_string_impl(), "links");

        let kind = RuneNoteChangeType::tags_impl();
        assert_eq!(kind.to_string_impl(), "tags");
    }

    #[test]
    fn test_module_creation() {
        let m = module();
        assert!(m.is_ok(), "Module should be created successfully");
    }
}
