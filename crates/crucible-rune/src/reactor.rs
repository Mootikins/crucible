//! Reactor trait for session-level event processing.
//!
//! The `Reactor` trait defines the interface for pluggable session behaviors.
//! It sits above the EventBus and RingHandler infrastructure, handling
//! session lifecycle events and coordinating the context tree flow.
//!
//! ## Architecture
//!
//! ```text
//! Session
//!    │
//!    ├── Ring Buffer (storage/transport)
//!    │
//!    ├── EventBus (pub/sub, Rune handlers)
//!    │
//!    └── Reactor (context tree flow) ◄── This trait
//!           │
//!           └── Kiln (persistence)
//! ```
//!
//! ## Design Principles
//!
//! - **Single implementation for now**: `LinearReactor` handles simple
//!   sequential context flows. DAG topology emerges from subagent
//!   relationships, not from special reactor types.
//!
//! - **SOLID compliance**: Trait defines the interface; concrete
//!   implementations can be added without changing consumers.
//!
//! - **Pluggable behaviors**: Different session behaviors come from:
//!   - Rune handlers registered on EventBus
//!   - Configuration passed to reactor
//!   - Subagents (separate sessions linked via wikilinks)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::reactor::{Reactor, ReactorConfig};
//! use crucible_rune::event_bus::EventContext;
//! use async_trait::async_trait;
//!
//! struct LinearReactor {
//!     config: ReactorConfig,
//! }
//!
//! #[async_trait]
//! impl Reactor for LinearReactor {
//!     async fn handle_event(
//!         &self,
//!         ctx: &mut EventContext,
//!         event: SessionEvent,
//!     ) -> ReactorResult<SessionEvent> {
//!         // Process event, potentially emitting new events via ctx
//!         Ok(event)
//!     }
//! }
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::event_bus::EventContext;

// Re-export event types from crucible-core (canonical location)
pub use crucible_core::events::{
    EntityType, ToolProvider, FileChangeKind, NoteChangeType, Priority, SessionEvent,
    SessionEventConfig, ToolCall,
};

/// Result type for reactor operations.
pub type ReactorResult<T> = Result<T, ReactorError>;

/// Errors that can occur during reactor execution.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ReactorError {
    /// Event processing failed.
    #[error("Event processing failed: {message}")]
    ProcessingFailed { message: String },

    /// Session initialization failed.
    #[error("Session initialization failed: {message}")]
    InitializationFailed { message: String },

    /// Compaction failed.
    #[error("Compaction failed: {message}")]
    CompactionFailed { message: String },

    /// Storage error.
    #[error("Storage error: {message}")]
    Storage { message: String },

    /// Configuration error.
    #[error("Configuration error: {message}")]
    Configuration { message: String },
}

impl ReactorError {
    /// Create a processing failure error.
    pub fn processing_failed(message: impl Into<String>) -> Self {
        Self::ProcessingFailed {
            message: message.into(),
        }
    }

    /// Create an initialization failure error.
    pub fn init_failed(message: impl Into<String>) -> Self {
        Self::InitializationFailed {
            message: message.into(),
        }
    }

    /// Create a compaction failure error.
    pub fn compaction_failed(message: impl Into<String>) -> Self {
        Self::CompactionFailed {
            message: message.into(),
        }
    }

    /// Create a storage error.
    pub fn storage(message: impl Into<String>) -> Self {
        Self::Storage {
            message: message.into(),
        }
    }

    /// Create a configuration error.
    pub fn config(message: impl Into<String>) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }
}

/// Reactor session configuration for event handler execution.
///
/// Contains session identity, folder paths, and context limits for reactor processing.
/// This is distinct from `crucible_core::types::acp::SessionConfig` which is for
/// ACP protocol parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Unique session identifier.
    pub session_id: String,

    /// Session folder path in the kiln.
    pub folder: PathBuf,

    /// Maximum context tokens before compaction.
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: usize,

    /// Optional system prompt.
    #[serde(default)]
    pub system_prompt: Option<String>,

    /// Custom configuration values.
    #[serde(default)]
    pub custom: JsonValue,
}

fn default_max_context_tokens() -> usize {
    100_000
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            folder: PathBuf::new(),
            max_context_tokens: default_max_context_tokens(),
            system_prompt: None,
            custom: JsonValue::Null,
        }
    }
}

impl SessionConfig {
    /// Create a new session config with the given ID and folder.
    pub fn new(session_id: impl Into<String>, folder: impl Into<PathBuf>) -> Self {
        Self {
            session_id: session_id.into(),
            folder: folder.into(),
            ..Default::default()
        }
    }

    /// Set the maximum context tokens.
    pub fn with_max_context_tokens(mut self, tokens: usize) -> Self {
        self.max_context_tokens = tokens;
        self
    }

    /// Set the system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set custom configuration.
    pub fn with_custom(mut self, custom: JsonValue) -> Self {
        self.custom = custom;
        self
    }
}

/// Context passed to reactors during event processing.
///
/// `ReactorContext` bridges the EventBus pub/sub layer with the Reactor processing layer.
/// It provides:
///
/// - **Session configuration**: Immutable access to session settings
/// - **Event emission**: Ability to emit new events (collected for dispatch)
/// - **Metadata storage**: Cross-handler state passing via key-value pairs
/// - **Token tracking**: Current context size for compaction decisions
/// - **EventBus context**: Access to the underlying EventBus context
///
/// ## Lifecycle
///
/// A new `ReactorContext` is created for each event entering the reactor.
/// After processing:
/// 1. Emitted events are dispatched via the EventBus
/// 2. Metadata may be persisted or passed to subsequent handlers
/// 3. Token count informs compaction decisions
///
/// ## Example
///
/// ```rust,ignore
/// async fn handle_event(
///     &self,
///     ctx: &mut ReactorContext,
///     event: SessionEvent,
/// ) -> ReactorResult<SessionEvent> {
///     // Access session config
///     let session_id = ctx.config().session_id.clone();
///
///     // Store cross-handler metadata
///     ctx.set_metadata("processed_by", json!(self.name()));
///
///     // Track token usage
///     ctx.add_tokens(estimate_tokens(&event));
///
///     // Emit a follow-up event
///     ctx.emit(SessionEvent::AgentThinking {
///         thought: "Processing...".into(),
///     });
///
///     Ok(event)
/// }
/// ```
#[derive(Debug)]
pub struct ReactorContext {
    /// Session configuration (immutable reference).
    config: Arc<SessionConfig>,

    /// Events emitted during processing.
    emitted_events: Vec<SessionEvent>,

    /// Cross-handler metadata storage.
    metadata: HashMap<String, JsonValue>,

    /// Current token count for compaction tracking.
    token_count: usize,

    /// Whether compaction has been requested.
    compaction_requested: bool,

    /// The underlying EventBus context (for low-level access).
    event_context: EventContext,

    /// Sequence number of the current event being processed.
    current_seq: Option<u64>,

    /// Parent session ID for subagent tracking.
    parent_session_id: Option<String>,
}

impl ReactorContext {
    /// Create a new reactor context with the given session configuration.
    pub fn new(config: Arc<SessionConfig>) -> Self {
        Self {
            config,
            emitted_events: Vec::new(),
            metadata: HashMap::new(),
            token_count: 0,
            compaction_requested: false,
            event_context: EventContext::new(),
            current_seq: None,
            parent_session_id: None,
        }
    }

    /// Create a context for a subagent session.
    pub fn for_subagent(config: Arc<SessionConfig>, parent_session_id: impl Into<String>) -> Self {
        Self {
            config,
            emitted_events: Vec::new(),
            metadata: HashMap::new(),
            token_count: 0,
            compaction_requested: false,
            event_context: EventContext::new(),
            current_seq: None,
            parent_session_id: Some(parent_session_id.into()),
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Configuration access
    // ─────────────────────────────────────────────────────────────────────────

    /// Get the session configuration.
    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    /// Get the session ID.
    pub fn session_id(&self) -> &str {
        &self.config.session_id
    }

    /// Get the session folder path.
    pub fn folder(&self) -> &PathBuf {
        &self.config.folder
    }

    /// Get the maximum context tokens before compaction.
    pub fn max_context_tokens(&self) -> usize {
        self.config.max_context_tokens
    }

    /// Get the system prompt, if any.
    pub fn system_prompt(&self) -> Option<&str> {
        self.config.system_prompt.as_deref()
    }

    /// Get the parent session ID (for subagents).
    pub fn parent_session_id(&self) -> Option<&str> {
        self.parent_session_id.as_deref()
    }

    /// Check if this is a subagent context.
    pub fn is_subagent(&self) -> bool {
        self.parent_session_id.is_some()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Event emission
    // ─────────────────────────────────────────────────────────────────────────

    /// Emit a new session event.
    ///
    /// Emitted events are collected and dispatched after the current event
    /// completes processing.
    pub fn emit(&mut self, event: SessionEvent) {
        self.emitted_events.push(event);
    }

    /// Emit a custom event with the given name and payload.
    pub fn emit_custom(&mut self, name: impl Into<String>, payload: JsonValue) {
        self.emit(SessionEvent::Custom {
            name: name.into(),
            payload,
        });
    }

    /// Take all emitted events, leaving the context empty.
    pub fn take_emitted(&mut self) -> Vec<SessionEvent> {
        std::mem::take(&mut self.emitted_events)
    }

    /// Get the number of emitted events.
    pub fn emitted_count(&self) -> usize {
        self.emitted_events.len()
    }

    /// Check if any events have been emitted.
    pub fn has_emitted(&self) -> bool {
        !self.emitted_events.is_empty()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Metadata storage
    // ─────────────────────────────────────────────────────────────────────────

    /// Store a metadata value.
    pub fn set_metadata(&mut self, key: impl Into<String>, value: JsonValue) {
        self.metadata.insert(key.into(), value);
    }

    /// Get a metadata value by key.
    pub fn get_metadata(&self, key: &str) -> Option<&JsonValue> {
        self.metadata.get(key)
    }

    /// Remove and return a metadata value.
    pub fn remove_metadata(&mut self, key: &str) -> Option<JsonValue> {
        self.metadata.remove(key)
    }

    /// Check if a metadata key exists.
    pub fn has_metadata(&self, key: &str) -> bool {
        self.metadata.contains_key(key)
    }

    /// Get all metadata as a reference.
    pub fn metadata(&self) -> &HashMap<String, JsonValue> {
        &self.metadata
    }

    /// Clear all metadata.
    pub fn clear_metadata(&mut self) {
        self.metadata.clear();
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Token tracking
    // ─────────────────────────────────────────────────────────────────────────

    /// Get the current token count.
    pub fn token_count(&self) -> usize {
        self.token_count
    }

    /// Add tokens to the count.
    pub fn add_tokens(&mut self, count: usize) {
        self.token_count = self.token_count.saturating_add(count);
    }

    /// Set the token count directly.
    pub fn set_token_count(&mut self, count: usize) {
        self.token_count = count;
    }

    /// Reset the token count to zero.
    pub fn reset_token_count(&mut self) {
        self.token_count = 0;
    }

    /// Check if the token count exceeds the maximum.
    pub fn should_compact(&self) -> bool {
        self.token_count >= self.config.max_context_tokens
    }

    /// Request compaction explicitly.
    pub fn request_compaction(&mut self) {
        self.compaction_requested = true;
    }

    /// Check if compaction has been requested.
    pub fn compaction_requested(&self) -> bool {
        self.compaction_requested || self.should_compact()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Sequence tracking
    // ─────────────────────────────────────────────────────────────────────────

    /// Get the current event sequence number.
    pub fn current_seq(&self) -> Option<u64> {
        self.current_seq
    }

    /// Set the current event sequence number.
    pub fn set_current_seq(&mut self, seq: u64) {
        self.current_seq = Some(seq);
    }

    /// Clear the current sequence number.
    pub fn clear_current_seq(&mut self) {
        self.current_seq = None;
    }

    // ─────────────────────────────────────────────────────────────────────────
    // EventBus context access
    // ─────────────────────────────────────────────────────────────────────────

    /// Get a reference to the underlying EventBus context.
    pub fn event_context(&self) -> &EventContext {
        &self.event_context
    }

    /// Get a mutable reference to the underlying EventBus context.
    pub fn event_context_mut(&mut self) -> &mut EventContext {
        &mut self.event_context
    }

    /// Bridge events from EventContext to ReactorContext.
    ///
    /// This converts any events emitted via the EventBus context into
    /// SessionEvents and adds them to the reactor's emitted events.
    pub fn bridge_event_context(&mut self) {
        // Take events from EventContext and convert to Custom SessionEvents
        for event in self.event_context.take_emitted() {
            self.emit(SessionEvent::Custom {
                name: event.identifier,
                payload: event.payload,
            });
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Reset / lifecycle
    // ─────────────────────────────────────────────────────────────────────────

    /// Reset the context for processing a new event.
    ///
    /// This clears emitted events and resets the compaction flag,
    /// but preserves metadata and token count.
    pub fn reset_for_event(&mut self) {
        self.emitted_events.clear();
        self.compaction_requested = false;
        self.current_seq = None;
    }

    /// Fully reset the context.
    ///
    /// This clears everything including metadata and token count.
    pub fn reset(&mut self) {
        self.emitted_events.clear();
        self.metadata.clear();
        self.token_count = 0;
        self.compaction_requested = false;
        self.current_seq = None;
    }
}

impl Default for ReactorContext {
    fn default() -> Self {
        Self::new(Arc::new(SessionConfig::default()))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SessionEvent conversions (crucible-rune specific)
// ─────────────────────────────────────────────────────────────────────────────

/// Map a SessionEvent to an EventType for handler matching.
///
/// This provides backwards compatibility with handlers registered by EventType.
pub fn session_event_to_event_type(event: &SessionEvent) -> crate::event_bus::EventType {
    use crate::event_bus::EventType;
    match event {
        SessionEvent::ToolCalled { .. } => EventType::ToolBefore,
        SessionEvent::ToolCompleted { error: None, .. } => EventType::ToolAfter,
        SessionEvent::ToolCompleted { error: Some(_), .. } => EventType::ToolError,
        SessionEvent::ToolDiscovered { .. } => EventType::ToolDiscovered,
        SessionEvent::NoteParsed { .. } => EventType::NoteParsed,
        SessionEvent::NoteCreated { .. } => EventType::NoteCreated,
        SessionEvent::NoteModified { .. } => EventType::NoteModified,
        SessionEvent::McpAttached { .. } => EventType::McpAttached,
        // All other events map to Custom
        _ => EventType::Custom,
    }
}

/// Convert an EventBus Event to a SessionEvent.
///
/// This is used during the consolidation of EventBus events into the
/// SessionEvent system. Note/MCP/Tool events are converted to their
/// specific SessionEvent variants; other events become Custom.
pub fn event_to_session_event(event: crate::event_bus::Event) -> SessionEvent {
    use crate::event_bus::EventType;

    match event.event_type {
        EventType::ToolBefore => SessionEvent::ToolCalled {
            name: event.identifier,
            args: event.payload,
        },
        EventType::ToolAfter => SessionEvent::ToolCompleted {
            name: event.identifier,
            result: event.payload.to_string(),
            error: None,
        },
        EventType::ToolError => SessionEvent::ToolCompleted {
            name: event.identifier,
            result: String::new(),
            error: Some(event.payload.to_string()),
        },
        EventType::ToolDiscovered => SessionEvent::ToolDiscovered {
            name: event.identifier,
            source: ToolProvider::Rune, // Default source
            schema: Some(event.payload),
        },
        EventType::NoteParsed => SessionEvent::NoteParsed {
            path: std::path::PathBuf::from(&event.identifier),
            block_count: event
                .payload
                .get("block_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize,
            payload: None, // Payload extracted from legacy event doesn't include NotePayload
        },
        EventType::NoteCreated => SessionEvent::NoteCreated {
            path: std::path::PathBuf::from(&event.identifier),
            title: event
                .payload
                .get("title")
                .and_then(|v| v.as_str())
                .map(String::from),
        },
        EventType::NoteModified => SessionEvent::NoteModified {
            path: std::path::PathBuf::from(&event.identifier),
            change_type: NoteChangeType::Content, // Default
        },
        EventType::FileDeleted => SessionEvent::FileDeleted {
            path: std::path::PathBuf::from(&event.identifier),
        },
        EventType::McpAttached => SessionEvent::McpAttached {
            server: event.identifier,
            tool_count: event
                .payload
                .get("tool_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize,
        },
        EventType::Custom => SessionEvent::Custom {
            name: event.identifier,
            payload: event.payload,
        },
    }
}

/// Convert a SessionConfig to a SessionEventConfig.
impl From<&SessionConfig> for SessionEventConfig {
    fn from(config: &SessionConfig) -> Self {
        SessionEventConfig {
            session_id: config.session_id.clone(),
            folder: Some(config.folder.clone()),
            max_context_tokens: config.max_context_tokens,
            system_prompt: config.system_prompt.clone(),
        }
    }
}

/// The Reactor trait for session-level event processing.
///
/// Reactors handle the high-level session flow, processing events and
/// coordinating with storage, LLM providers, and subagents.
///
/// ## Lifecycle
///
/// 1. `on_session_start` - Called when session begins
/// 2. `on_event` or `handle_event` - Called for each event (may be called many times)
/// 3. `on_before_compact` - Called before context compaction
/// 4. `on_session_end` - Called when session ends
///
/// ## Event Flow (Ring Buffer Model)
///
/// ```text
/// User message
///     ↓
/// Push to Ring Buffer → get sequence number
///     ↓
/// Reactor::on_event(ctx, seq)
///     ↓
/// Handler chain processes event
///     ↓
/// Emitted events pushed back to ring
/// ```
///
/// ## Event Flow (Direct Model)
///
/// ```text
/// User message
///     ↓
/// SessionEvent::MessageReceived
///     ↓
/// Reactor::handle_event()
///     ↓
/// May emit new events via ctx.emit()
///     ↓
/// Persist to kiln
/// ```
#[async_trait]
pub trait Reactor: Send + Sync {
    /// Process an event by sequence number from the ring buffer.
    ///
    /// This is the primary entry point for the ring buffer model. The event
    /// is already stored in the ring buffer at the given sequence number.
    /// The reactor should:
    ///
    /// 1. Get the event from the ring by sequence
    /// 2. Process it through the handler chain
    /// 3. Push emitted events back to the ring
    ///
    /// # Arguments
    ///
    /// * `ctx` - Reactor context with session config, ring reference, etc.
    /// * `seq` - Sequence number of the event in the ring buffer
    ///
    /// # Returns
    ///
    /// Sequence numbers of any events emitted during processing.
    async fn on_event(&self, ctx: &mut ReactorContext, seq: u64) -> ReactorResult<Vec<u64>> {
        // Default implementation: do nothing, return empty vec
        let _ = (ctx, seq);
        Ok(Vec::new())
    }

    /// Handle a session event, potentially emitting new events.
    ///
    /// This is an alternative entry point for direct event handling (without
    /// the ring buffer). Use `on_event` for the ring buffer model.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Event context for emitting events and storing metadata
    /// * `event` - The session event to process
    ///
    /// # Returns
    ///
    /// The (possibly modified) event after processing.
    async fn handle_event(
        &self,
        ctx: &mut EventContext,
        event: SessionEvent,
    ) -> ReactorResult<SessionEvent>;

    /// Called when session starts.
    ///
    /// Use for initialization. Default implementation does nothing.
    async fn on_session_start(&self, _config: &SessionConfig) -> ReactorResult<()> {
        Ok(())
    }

    /// Called before context compaction.
    ///
    /// Returns a summary string to include in the new context file.
    /// Default returns an empty string.
    async fn on_before_compact(&self, _events: &[SessionEvent]) -> ReactorResult<String> {
        Ok(String::new())
    }

    /// Called when session ends.
    ///
    /// Use for cleanup. Default implementation does nothing.
    async fn on_session_end(&self, _reason: &str) -> ReactorResult<()> {
        Ok(())
    }

    /// Get reactor metadata for introspection.
    ///
    /// Default returns empty metadata.
    fn metadata(&self) -> ReactorMetadata {
        ReactorMetadata::default()
    }
}

/// Metadata about a reactor for introspection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReactorMetadata {
    /// Reactor name/type.
    pub name: String,
    /// Version string.
    pub version: String,
    /// Description.
    pub description: String,
    /// Custom metadata.
    #[serde(default)]
    pub custom: JsonValue,
}

impl ReactorMetadata {
    /// Create new metadata with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set the version.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}

/// Boxed reactor for type erasure.
pub type BoxedReactor = Box<dyn Reactor>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    /// Cross-platform test path helper
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

    #[test]
    fn test_session_config_default() {
        let config = SessionConfig::default();
        assert!(config.session_id.is_empty());
        assert_eq!(config.max_context_tokens, 100_000);
        assert!(config.system_prompt.is_none());
    }

    #[test]
    fn test_session_config_builder() {
        let folder = test_path("session");
        let config = SessionConfig::new("test-session", folder.clone())
            .with_max_context_tokens(50_000)
            .with_system_prompt("You are a helpful assistant.")
            .with_custom(json!({"key": "value"}));

        assert_eq!(config.session_id, "test-session");
        assert_eq!(config.folder, folder);
        assert_eq!(config.max_context_tokens, 50_000);
        assert_eq!(
            config.system_prompt,
            Some("You are a helpful assistant.".to_string())
        );
        assert_eq!(config.custom["key"], "value");
    }

    #[test]
    fn test_session_event_serialization() {
        let event = SessionEvent::MessageReceived {
            content: "Hello".to_string(),
            participant_id: "user".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("message_received"));
        assert!(json.contains("Hello"));

        let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            SessionEvent::MessageReceived {
                content,
                participant_id,
            } => {
                assert_eq!(content, "Hello");
                assert_eq!(participant_id, "user");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_tool_call() {
        let path = test_path("test.txt");
        let call = ToolCall::new("read_file", json!({"path": path.to_string_lossy()}))
            .with_call_id("call_123");

        assert_eq!(call.name, "read_file");
        assert_eq!(call.args["path"], path.to_string_lossy().as_ref());
        assert_eq!(call.call_id, Some("call_123".to_string()));
    }

    #[test]
    fn test_reactor_error() {
        let err = ReactorError::processing_failed("event handler panicked");
        assert!(err.to_string().contains("Event processing failed"));

        let err = ReactorError::init_failed("missing config");
        assert!(err.to_string().contains("initialization failed"));

        let err = ReactorError::compaction_failed("out of disk space");
        assert!(err.to_string().contains("Compaction failed"));

        let err = ReactorError::storage("connection lost");
        assert!(err.to_string().contains("Storage error"));

        let err = ReactorError::config("invalid value");
        assert!(err.to_string().contains("Configuration error"));
    }

    #[test]
    fn test_reactor_metadata() {
        let meta = ReactorMetadata::new("LinearReactor")
            .with_version("1.0.0")
            .with_description("Simple sequential context flow");

        assert_eq!(meta.name, "LinearReactor");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.description, "Simple sequential context flow");
    }

    #[test]
    fn test_session_event_variants() {
        // Test that all variants serialize correctly
        let events = vec![
            SessionEvent::MessageReceived {
                content: "test".into(),
                participant_id: "user".into(),
            },
            SessionEvent::AgentResponded {
                content: "response".into(),
                tool_calls: vec![],
            },
            SessionEvent::AgentThinking {
                thought: "thinking...".into(),
            },
            SessionEvent::ToolCalled {
                name: "tool".into(),
                args: json!({}),
            },
            SessionEvent::ToolCompleted {
                name: "tool".into(),
                result: "done".into(),
                error: None,
            },
            SessionEvent::SessionStarted {
                config: SessionEventConfig::new("test"),
            },
            SessionEvent::SessionCompacted {
                summary: "summary".into(),
                new_file: test_path("new"),
            },
            SessionEvent::SessionEnded {
                reason: "user closed".into(),
            },
            SessionEvent::SubagentSpawned {
                id: "sub1".into(),
                prompt: "do stuff".into(),
            },
            SessionEvent::SubagentCompleted {
                id: "sub1".into(),
                result: "done".into(),
            },
            SessionEvent::SubagentFailed {
                id: "sub1".into(),
                error: "failed".into(),
            },
            // Streaming events
            SessionEvent::TextDelta {
                delta: "chunk".into(),
                seq: 1,
            },
            // Note events
            SessionEvent::NoteParsed {
                path: PathBuf::from("/notes/test.md"),
                block_count: 5,
                payload: None,
            },
            SessionEvent::NoteCreated {
                path: PathBuf::from("/notes/new.md"),
                title: Some("New Note".into()),
            },
            SessionEvent::NoteModified {
                path: PathBuf::from("/notes/test.md"),
                change_type: NoteChangeType::Content,
            },
            // MCP/Tool discovery events
            SessionEvent::McpAttached {
                server: "crucible".into(),
                tool_count: 10,
            },
            SessionEvent::ToolDiscovered {
                name: "search".into(),
                source: ToolProvider::Mcp {
                    server: "crucible".into(),
                },
                schema: Some(json!({"type": "object"})),
            },
            SessionEvent::Custom {
                name: "custom".into(),
                payload: json!({}),
            },
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let _parsed: SessionEvent = serde_json::from_str(&json).unwrap();
        }
    }

    // Test that Reactor trait can be implemented
    struct TestReactor;

    #[async_trait]
    impl Reactor for TestReactor {
        async fn handle_event(
            &self,
            _ctx: &mut EventContext,
            event: SessionEvent,
        ) -> ReactorResult<SessionEvent> {
            Ok(event)
        }

        fn metadata(&self) -> ReactorMetadata {
            ReactorMetadata::new("TestReactor").with_version("0.1.0")
        }
    }

    #[tokio::test]
    async fn test_reactor_implementation() {
        let reactor = TestReactor;
        let mut ctx = EventContext::new();

        let event = SessionEvent::MessageReceived {
            content: "test".into(),
            participant_id: "user".into(),
        };

        let result = reactor.handle_event(&mut ctx, event).await.unwrap();
        match result {
            SessionEvent::MessageReceived { content, .. } => {
                assert_eq!(content, "test");
            }
            _ => panic!("Wrong variant"),
        }

        // Test default lifecycle methods
        reactor
            .on_session_start(&SessionConfig::default())
            .await
            .unwrap();
        let summary = reactor.on_before_compact(&[]).await.unwrap();
        assert!(summary.is_empty());
        reactor.on_session_end("test").await.unwrap();

        // Test metadata
        let meta = reactor.metadata();
        assert_eq!(meta.name, "TestReactor");
        assert_eq!(meta.version, "0.1.0");
    }

    #[tokio::test]
    async fn test_boxed_reactor() {
        let reactor: BoxedReactor = Box::new(TestReactor);
        let mut ctx = EventContext::new();

        let event = SessionEvent::MessageReceived {
            content: "boxed test".into(),
            participant_id: "user".into(),
        };

        let result = reactor.handle_event(&mut ctx, event).await.unwrap();
        match result {
            SessionEvent::MessageReceived { content, .. } => {
                assert_eq!(content, "boxed test");
            }
            _ => panic!("Wrong variant"),
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // ReactorContext tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_reactor_context_new() {
        let folder = test_path("test");
        let config = Arc::new(SessionConfig::new("test-session", folder.clone()));
        let ctx = ReactorContext::new(config.clone());

        assert_eq!(ctx.session_id(), "test-session");
        assert_eq!(ctx.folder(), &folder);
        assert_eq!(ctx.token_count(), 0);
        assert!(!ctx.compaction_requested());
        assert!(ctx.current_seq().is_none());
        assert!(!ctx.is_subagent());
    }

    #[test]
    fn test_reactor_context_for_subagent() {
        let folder = test_path("sub");
        let config = Arc::new(SessionConfig::new("sub-session", folder));
        let ctx = ReactorContext::for_subagent(config, "parent-session");

        assert_eq!(ctx.session_id(), "sub-session");
        assert!(ctx.is_subagent());
        assert_eq!(ctx.parent_session_id(), Some("parent-session"));
    }

    #[test]
    fn test_reactor_context_config_access() {
        let folder = test_path("config_access");
        let config = Arc::new(
            SessionConfig::new("test", folder)
                .with_max_context_tokens(50_000)
                .with_system_prompt("Test prompt"),
        );
        let ctx = ReactorContext::new(config);

        assert_eq!(ctx.max_context_tokens(), 50_000);
        assert_eq!(ctx.system_prompt(), Some("Test prompt"));
    }

    #[test]
    fn test_reactor_context_emit() {
        let mut ctx = ReactorContext::default();

        assert!(!ctx.has_emitted());
        assert_eq!(ctx.emitted_count(), 0);

        ctx.emit(SessionEvent::AgentThinking {
            thought: "Processing...".into(),
        });

        assert!(ctx.has_emitted());
        assert_eq!(ctx.emitted_count(), 1);

        ctx.emit_custom("custom_event", json!({"key": "value"}));

        assert_eq!(ctx.emitted_count(), 2);

        let emitted = ctx.take_emitted();
        assert_eq!(emitted.len(), 2);
        assert!(!ctx.has_emitted());

        // Verify first event
        match &emitted[0] {
            SessionEvent::AgentThinking { thought } => {
                assert_eq!(thought, "Processing...");
            }
            _ => panic!("Wrong event type"),
        }

        // Verify custom event
        match &emitted[1] {
            SessionEvent::Custom { name, payload } => {
                assert_eq!(name, "custom_event");
                assert_eq!(payload["key"], "value");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_reactor_context_metadata() {
        let mut ctx = ReactorContext::default();

        assert!(!ctx.has_metadata("key1"));

        ctx.set_metadata("key1", json!("value1"));
        ctx.set_metadata("key2", json!(42));

        assert!(ctx.has_metadata("key1"));
        assert_eq!(ctx.get_metadata("key1"), Some(&json!("value1")));
        assert_eq!(ctx.get_metadata("key2"), Some(&json!(42)));
        assert_eq!(ctx.get_metadata("missing"), None);

        let removed = ctx.remove_metadata("key1");
        assert_eq!(removed, Some(json!("value1")));
        assert!(!ctx.has_metadata("key1"));

        // Check metadata() accessor
        let all_metadata = ctx.metadata();
        assert_eq!(all_metadata.len(), 1);
        assert!(all_metadata.contains_key("key2"));

        ctx.clear_metadata();
        assert!(ctx.metadata().is_empty());
    }

    #[test]
    fn test_reactor_context_token_tracking() {
        let folder = test_path("token_tracking");
        let config = Arc::new(SessionConfig::new("test", folder).with_max_context_tokens(1000));
        let mut ctx = ReactorContext::new(config);

        assert_eq!(ctx.token_count(), 0);
        assert!(!ctx.should_compact());

        ctx.add_tokens(500);
        assert_eq!(ctx.token_count(), 500);
        assert!(!ctx.should_compact());

        ctx.add_tokens(600);
        assert_eq!(ctx.token_count(), 1100);
        assert!(ctx.should_compact());
        assert!(ctx.compaction_requested());

        ctx.set_token_count(800);
        assert_eq!(ctx.token_count(), 800);
        assert!(!ctx.should_compact());

        ctx.reset_token_count();
        assert_eq!(ctx.token_count(), 0);
    }

    #[test]
    fn test_reactor_context_compaction_request() {
        let folder = test_path("compaction_request");
        let config = Arc::new(SessionConfig::new("test", folder).with_max_context_tokens(1000));
        let mut ctx = ReactorContext::new(config);

        assert!(!ctx.compaction_requested());

        ctx.request_compaction();
        assert!(ctx.compaction_requested());

        // Reset should clear the request
        ctx.reset_for_event();
        assert!(!ctx.compaction_requested());
    }

    #[test]
    fn test_reactor_context_sequence_tracking() {
        let mut ctx = ReactorContext::default();

        assert!(ctx.current_seq().is_none());

        ctx.set_current_seq(42);
        assert_eq!(ctx.current_seq(), Some(42));

        ctx.clear_current_seq();
        assert!(ctx.current_seq().is_none());
    }

    #[test]
    fn test_reactor_context_reset_for_event() {
        let mut ctx = ReactorContext::default();

        // Set up some state
        ctx.emit(SessionEvent::AgentThinking {
            thought: "test".into(),
        });
        ctx.set_metadata("key", json!("value"));
        ctx.add_tokens(100);
        ctx.request_compaction();
        ctx.set_current_seq(5);

        // Reset for new event
        ctx.reset_for_event();

        // Emitted events, compaction request, and seq should be cleared
        assert!(!ctx.has_emitted());
        assert!(!ctx.compaction_requested());
        assert!(ctx.current_seq().is_none());

        // Metadata and token count should be preserved
        assert!(ctx.has_metadata("key"));
        assert_eq!(ctx.token_count(), 100);
    }

    #[test]
    fn test_reactor_context_full_reset() {
        let mut ctx = ReactorContext::default();

        // Set up state
        ctx.emit(SessionEvent::AgentThinking {
            thought: "test".into(),
        });
        ctx.set_metadata("key", json!("value"));
        ctx.add_tokens(100);
        ctx.request_compaction();
        ctx.set_current_seq(5);

        // Full reset
        ctx.reset();

        // Everything should be cleared
        assert!(!ctx.has_emitted());
        assert!(!ctx.has_metadata("key"));
        assert_eq!(ctx.token_count(), 0);
        assert!(!ctx.compaction_requested());
        assert!(ctx.current_seq().is_none());
    }

    #[test]
    fn test_reactor_context_event_context_access() {
        let mut ctx = ReactorContext::default();

        // Access EventContext
        let event_ctx = ctx.event_context();
        assert!(event_ctx.metadata().is_empty());

        // Mutably access and modify
        ctx.event_context_mut().set("bus_key", json!("bus_value"));

        // Verify change
        assert_eq!(
            ctx.event_context().get("bus_key"),
            Some(&json!("bus_value"))
        );
    }

    #[test]
    fn test_reactor_context_bridge_event_context() {
        use crate::event_bus::Event;

        let mut ctx = ReactorContext::default();

        // Emit events via EventContext
        ctx.event_context_mut()
            .emit(Event::custom("bus_event_1", json!({"from": "bus"})));
        ctx.event_context_mut()
            .emit(Event::custom("bus_event_2", json!({"from": "bus2"})));

        // Bridge to ReactorContext
        ctx.bridge_event_context();

        // Events should now be in ReactorContext
        assert_eq!(ctx.emitted_count(), 2);

        let emitted = ctx.take_emitted();
        match &emitted[0] {
            SessionEvent::Custom { name, payload } => {
                assert_eq!(name, "bus_event_1");
                assert_eq!(payload["from"], "bus");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_reactor_context_default() {
        let ctx = ReactorContext::default();

        // Should have default SessionConfig
        assert!(ctx.session_id().is_empty());
        assert_eq!(ctx.max_context_tokens(), 100_000);
    }

    #[test]
    fn test_reactor_context_token_overflow() {
        let mut ctx = ReactorContext::default();

        // Test saturating add
        ctx.set_token_count(usize::MAX - 10);
        ctx.add_tokens(100);

        assert_eq!(ctx.token_count(), usize::MAX);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // SessionEvent helper tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_session_event_type() {
        assert_eq!(
            SessionEvent::MessageReceived {
                content: "".into(),
                participant_id: "".into()
            }
            .event_type(),
            "message_received"
        );
        assert_eq!(
            SessionEvent::ToolCalled {
                name: "".into(),
                args: json!({})
            }
            .event_type(),
            "tool_called"
        );
        assert_eq!(
            SessionEvent::TextDelta {
                delta: "".into(),
                seq: 0
            }
            .event_type(),
            "text_delta"
        );
        assert_eq!(
            SessionEvent::NoteParsed {
                path: PathBuf::new(),
                block_count: 0,
                payload: None,
            }
            .event_type(),
            "note_parsed"
        );
        assert_eq!(
            SessionEvent::McpAttached {
                server: "".into(),
                tool_count: 0
            }
            .event_type(),
            "mcp_attached"
        );
        assert_eq!(
            SessionEvent::Custom {
                name: "test".into(),
                payload: json!({})
            }
            .event_type(),
            "custom"
        );
    }

    #[test]
    fn test_session_event_category_helpers() {
        // Tool events
        assert!(SessionEvent::ToolCalled {
            name: "".into(),
            args: json!({})
        }
        .is_tool_event());
        assert!(SessionEvent::ToolCompleted {
            name: "".into(),
            result: "".into(),
            error: None
        }
        .is_tool_event());
        assert!(SessionEvent::ToolDiscovered {
            name: "".into(),
            source: ToolProvider::Rune,
            schema: None
        }
        .is_tool_event());
        assert!(!SessionEvent::MessageReceived {
            content: "".into(),
            participant_id: "".into()
        }
        .is_tool_event());

        // Note events
        assert!(SessionEvent::NoteParsed {
            path: PathBuf::new(),
            block_count: 0,
            payload: None,
        }
        .is_note_event());
        assert!(SessionEvent::NoteCreated {
            path: PathBuf::new(),
            title: None
        }
        .is_note_event());
        assert!(SessionEvent::NoteModified {
            path: PathBuf::new(),
            change_type: NoteChangeType::Content
        }
        .is_note_event());
        assert!(!SessionEvent::ToolCalled {
            name: "".into(),
            args: json!({})
        }
        .is_note_event());

        // Lifecycle events
        assert!(SessionEvent::SessionStarted {
            config: SessionEventConfig::new("test")
        }
        .is_lifecycle_event());
        assert!(SessionEvent::SessionEnded { reason: "".into() }.is_lifecycle_event());
        assert!(!SessionEvent::MessageReceived {
            content: "".into(),
            participant_id: "".into()
        }
        .is_lifecycle_event());
    }

    #[test]
    fn test_event_bus_to_session_event_conversion() {
        use crate::event_bus::{Event, EventType};

        // Tool events
        let bus_event = Event::new(EventType::ToolBefore, "search", json!({"query": "test"}));
        let session_event: SessionEvent = event_to_session_event(bus_event);
        match session_event {
            SessionEvent::ToolCalled { name, args } => {
                assert_eq!(name, "search");
                assert_eq!(args["query"], "test");
            }
            _ => panic!("Wrong conversion for ToolBefore"),
        }

        let bus_event = Event::new(EventType::ToolAfter, "search", json!({"result": "found"}));
        let session_event: SessionEvent = event_to_session_event(bus_event);
        match session_event {
            SessionEvent::ToolCompleted { name, error, .. } => {
                assert_eq!(name, "search");
                assert!(error.is_none());
            }
            _ => panic!("Wrong conversion for ToolAfter"),
        }

        let bus_event = Event::new(EventType::ToolError, "search", json!({"error": "failed"}));
        let session_event: SessionEvent = event_to_session_event(bus_event);
        match session_event {
            SessionEvent::ToolCompleted { name, error, .. } => {
                assert_eq!(name, "search");
                assert!(error.is_some());
            }
            _ => panic!("Wrong conversion for ToolError"),
        }

        // Note events
        let bus_event = Event::new(
            EventType::NoteParsed,
            "/notes/test.md",
            json!({"block_count": 5}),
        );
        let session_event: SessionEvent = event_to_session_event(bus_event);
        match session_event {
            SessionEvent::NoteParsed {
                path,
                block_count,
                payload,
            } => {
                assert_eq!(path, PathBuf::from("/notes/test.md"));
                assert_eq!(block_count, 5);
                assert!(payload.is_none()); // Legacy conversion doesn't include payload
            }
            _ => panic!("Wrong conversion for NoteParsed"),
        }

        // MCP events
        let bus_event = Event::new(
            EventType::McpAttached,
            "crucible",
            json!({"tool_count": 10}),
        );
        let session_event: SessionEvent = event_to_session_event(bus_event);
        match session_event {
            SessionEvent::McpAttached { server, tool_count } => {
                assert_eq!(server, "crucible");
                assert_eq!(tool_count, 10);
            }
            _ => panic!("Wrong conversion for McpAttached"),
        }

        // Custom events
        let bus_event = Event::new(EventType::Custom, "my_event", json!({"key": "value"}));
        let session_event: SessionEvent = event_to_session_event(bus_event);
        match session_event {
            SessionEvent::Custom { name, payload } => {
                assert_eq!(name, "my_event");
                assert_eq!(payload["key"], "value");
            }
            _ => panic!("Wrong conversion for Custom"),
        }
    }
}
