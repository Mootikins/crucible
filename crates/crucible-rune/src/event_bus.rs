//! Unified Event Bus for Crucible
//!
//! This module provides the core event system that powers hooks, interceptors,
//! and all event-driven functionality in Crucible.
//!
//! ## Event Types
//!
//! - `tool:before` - Before tool execution (can modify args or cancel)
//! - `tool:after` - After tool execution (can transform result)
//! - `tool:error` - Tool execution failed
//! - `tool:discovered` - New tool discovered (for filtering/enrichment)
//! - `note:parsed` - Note parsing complete (includes AST)
//! - `note:created` - New note created
//! - `note:modified` - Note content changed
//! - `mcp:attached` - Upstream MCP server connected
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::event_bus::{EventBus, Event, EventType, Handler};
//!
//! let mut bus = EventBus::new();
//!
//! // Register a handler
//! bus.register(Handler::new("log_tools", EventType::ToolAfter, "*", |ctx, event| {
//!     println!("Tool executed: {}", event.identifier);
//!     Ok(event)
//! }));
//!
//! // Emit an event
//! let event = Event::tool_after("just_test", json!({"output": "..."}));
//! let result = bus.emit(event).await?;
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Event types supported by the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// Before tool execution - can modify args or cancel
    #[serde(rename = "tool:before")]
    ToolBefore,

    /// After successful tool execution - can transform result
    #[serde(rename = "tool:after")]
    ToolAfter,

    /// Tool execution failed
    #[serde(rename = "tool:error")]
    ToolError,

    /// Tool was discovered (for filtering/enrichment)
    #[serde(rename = "tool:discovered")]
    ToolDiscovered,

    /// Note was parsed (includes AST blocks)
    #[serde(rename = "note:parsed")]
    NoteParsed,

    /// New note was created
    #[serde(rename = "note:created")]
    NoteCreated,

    /// Note content was modified
    #[serde(rename = "note:modified")]
    NoteModified,

    /// Upstream MCP server connected
    #[serde(rename = "mcp:attached")]
    McpAttached,

    /// Custom event type for user-defined events
    #[serde(rename = "custom")]
    Custom,
}

impl EventType {
    /// Parse event type from string (e.g., "tool:after")
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "tool:before" => Some(Self::ToolBefore),
            "tool:after" => Some(Self::ToolAfter),
            "tool:error" => Some(Self::ToolError),
            "tool:discovered" => Some(Self::ToolDiscovered),
            "note:parsed" => Some(Self::NoteParsed),
            "note:created" => Some(Self::NoteCreated),
            "note:modified" => Some(Self::NoteModified),
            "mcp:attached" => Some(Self::McpAttached),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ToolBefore => "tool:before",
            Self::ToolAfter => "tool:after",
            Self::ToolError => "tool:error",
            Self::ToolDiscovered => "tool:discovered",
            Self::NoteParsed => "note:parsed",
            Self::NoteCreated => "note:created",
            Self::NoteModified => "note:modified",
            Self::McpAttached => "mcp:attached",
            Self::Custom => "custom",
        }
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// An event in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Event type (tool:before, tool:after, etc.)
    pub event_type: EventType,

    /// Identifier for pattern matching (e.g., tool name "just_test")
    pub identifier: String,

    /// Event payload (tool args, result, note content, etc.)
    pub payload: JsonValue,

    /// Timestamp in milliseconds since UNIX epoch
    pub timestamp_ms: u64,

    /// Whether this event has been cancelled (for tool:before)
    #[serde(default)]
    pub cancelled: bool,

    /// Source of the event (kiln, just, rune, upstream:server_name)
    #[serde(default)]
    pub source: Option<String>,
}

impl Event {
    /// Create a new event
    pub fn new(event_type: EventType, identifier: impl Into<String>, payload: JsonValue) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self {
            event_type,
            identifier: identifier.into(),
            payload,
            timestamp_ms,
            cancelled: false,
            source: None,
        }
    }

    /// Create a tool:before event
    pub fn tool_before(tool_name: impl Into<String>, args: JsonValue) -> Self {
        Self::new(EventType::ToolBefore, tool_name, args)
    }

    /// Create a tool:after event
    pub fn tool_after(tool_name: impl Into<String>, result: JsonValue) -> Self {
        Self::new(EventType::ToolAfter, tool_name, result)
    }

    /// Create a tool:error event
    pub fn tool_error(tool_name: impl Into<String>, error: JsonValue) -> Self {
        Self::new(EventType::ToolError, tool_name, error)
    }

    /// Create a tool:discovered event
    pub fn tool_discovered(tool_name: impl Into<String>, metadata: JsonValue) -> Self {
        Self::new(EventType::ToolDiscovered, tool_name, metadata)
    }

    /// Create a note:parsed event
    pub fn note_parsed(note_path: impl Into<String>, parsed_data: JsonValue) -> Self {
        Self::new(EventType::NoteParsed, note_path, parsed_data)
    }

    /// Create a note:created event
    pub fn note_created(note_path: impl Into<String>, metadata: JsonValue) -> Self {
        Self::new(EventType::NoteCreated, note_path, metadata)
    }

    /// Create a note:modified event
    pub fn note_modified(note_path: impl Into<String>, changes: JsonValue) -> Self {
        Self::new(EventType::NoteModified, note_path, changes)
    }

    /// Create an mcp:attached event
    pub fn mcp_attached(server_name: impl Into<String>, info: JsonValue) -> Self {
        Self::new(EventType::McpAttached, server_name, info)
    }

    /// Create a custom event
    pub fn custom(name: impl Into<String>, payload: JsonValue) -> Self {
        Self::new(EventType::Custom, name, payload)
    }

    /// Set the source of this event
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Cancel this event (only meaningful for tool:before)
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Check if this event is cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

/// Context passed to event handlers
///
/// Provides access to metadata storage, emission capability, and cross-handler state.
#[derive(Debug, Clone, Default)]
pub struct EventContext {
    /// Arbitrary metadata storage for passing data between handlers
    metadata: HashMap<String, JsonValue>,

    /// Events emitted by handlers during processing
    emitted_events: Vec<Event>,
}

impl EventContext {
    /// Create a new empty context
    pub fn new() -> Self {
        Self::default()
    }

    /// Store metadata value
    pub fn set(&mut self, key: impl Into<String>, value: JsonValue) {
        self.metadata.insert(key.into(), value);
    }

    /// Get metadata value
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        self.metadata.get(key)
    }

    /// Remove and return metadata value
    pub fn remove(&mut self, key: &str) -> Option<JsonValue> {
        self.metadata.remove(key)
    }

    /// Check if metadata key exists
    pub fn contains(&self, key: &str) -> bool {
        self.metadata.contains_key(key)
    }

    /// Emit a new event from within a handler
    ///
    /// Emitted events are collected and dispatched after the current event completes.
    pub fn emit(&mut self, event: Event) {
        self.emitted_events.push(event);
    }

    /// Emit a custom event by name
    pub fn emit_custom(&mut self, name: impl Into<String>, payload: JsonValue) {
        self.emit(Event::custom(name, payload));
    }

    /// Take all emitted events (used by EventBus after processing)
    pub fn take_emitted(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.emitted_events)
    }

    /// Get reference to all metadata
    pub fn metadata(&self) -> &HashMap<String, JsonValue> {
        &self.metadata
    }
}

/// Result type for handler execution
pub type HandlerResult = Result<Event, HandlerError>;

/// Errors that can occur during handler execution
#[derive(Debug, Clone)]
pub struct HandlerError {
    /// Name of the handler that failed
    pub handler_name: String,
    /// Error message
    pub message: String,
    /// Whether this error should stop the pipeline (default: false for fail-open)
    pub fatal: bool,
}

impl HandlerError {
    /// Create a non-fatal error (fail-open semantics)
    pub fn non_fatal(handler_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            handler_name: handler_name.into(),
            message: message.into(),
            fatal: false,
        }
    }

    /// Create a fatal error that stops the pipeline
    pub fn fatal(handler_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            handler_name: handler_name.into(),
            message: message.into(),
            fatal: true,
        }
    }
}

impl fmt::Display for HandlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Handler '{}' error: {}{}",
            self.handler_name,
            self.message,
            if self.fatal { " (fatal)" } else { "" }
        )
    }
}

impl std::error::Error for HandlerError {}

/// Handler function type
///
/// Takes mutable context and event, returns modified event or error.
pub type HandlerFn = Arc<dyn Fn(&mut EventContext, Event) -> HandlerResult + Send + Sync>;

/// A registered event handler
pub struct Handler {
    /// Unique name for this handler
    pub name: String,

    /// Event type to handle
    pub event_type: EventType,

    /// Glob pattern for matching event identifiers
    pub pattern: String,

    /// Priority (lower = earlier execution)
    pub priority: i64,

    /// Whether this handler is enabled
    pub enabled: bool,

    /// The handler function
    handler_fn: HandlerFn,
}

impl Handler {
    /// Create a new handler
    pub fn new<F>(
        name: impl Into<String>,
        event_type: EventType,
        pattern: impl Into<String>,
        handler_fn: F,
    ) -> Self
    where
        F: Fn(&mut EventContext, Event) -> HandlerResult + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            event_type,
            pattern: pattern.into(),
            priority: 100,
            enabled: true,
            handler_fn: Arc::new(handler_fn),
        }
    }

    /// Set priority (lower = earlier)
    pub fn with_priority(mut self, priority: i64) -> Self {
        self.priority = priority;
        self
    }

    /// Set enabled state
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Check if this handler matches an event
    pub fn matches(&self, event: &Event) -> bool {
        if !self.enabled {
            return false;
        }
        if self.event_type != event.event_type {
            return false;
        }
        match_glob(&self.pattern, &event.identifier)
    }

    /// Execute this handler
    pub fn handle(&self, ctx: &mut EventContext, event: Event) -> HandlerResult {
        (self.handler_fn)(ctx, event)
    }
}

impl fmt::Debug for Handler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Handler")
            .field("name", &self.name)
            .field("event_type", &self.event_type)
            .field("pattern", &self.pattern)
            .field("priority", &self.priority)
            .field("enabled", &self.enabled)
            .finish()
    }
}

/// The event bus - central dispatcher for all events
#[derive(Default)]
pub struct EventBus {
    /// Registered handlers
    handlers: Vec<Handler>,
}

impl EventBus {
    /// Create a new empty event bus
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handler
    pub fn register(&mut self, handler: Handler) {
        self.handlers.push(handler);
        // Keep sorted by priority (stable sort preserves registration order for same priority)
        self.handlers.sort_by_key(|h| h.priority);
    }

    /// Unregister a handler by name
    pub fn unregister(&mut self, name: &str) -> bool {
        let before_len = self.handlers.len();
        self.handlers.retain(|h| h.name != name);
        self.handlers.len() < before_len
    }

    /// Get handler by name
    pub fn get_handler(&self, name: &str) -> Option<&Handler> {
        self.handlers.iter().find(|h| h.name == name)
    }

    /// List all registered handlers
    pub fn list_handlers(&self) -> impl Iterator<Item = &Handler> {
        self.handlers.iter()
    }

    /// Count handlers for a specific event type
    pub fn count_handlers(&self, event_type: EventType) -> usize {
        self.handlers
            .iter()
            .filter(|h| h.event_type == event_type && h.enabled)
            .count()
    }

    /// Emit an event through the handler pipeline
    ///
    /// Returns the (possibly modified) event after all handlers have processed it.
    /// Uses fail-open semantics: handler errors are logged but don't stop processing.
    pub fn emit(&self, event: Event) -> (Event, EventContext, Vec<HandlerError>) {
        let mut ctx = EventContext::new();
        let mut current_event = event;
        let mut errors = Vec::new();

        // Find matching handlers (already sorted by priority)
        let matching: Vec<_> = self
            .handlers
            .iter()
            .filter(|h| h.matches(&current_event))
            .collect();

        for handler in matching {
            match handler.handle(&mut ctx, current_event.clone()) {
                Ok(modified_event) => {
                    current_event = modified_event;

                    // Check if event was cancelled (for tool:before)
                    if current_event.is_cancelled() {
                        break;
                    }
                }
                Err(e) => {
                    // Log error but continue (fail-open)
                    tracing::warn!(
                        "Handler '{}' failed for event '{}:{}': {}",
                        e.handler_name,
                        current_event.event_type,
                        current_event.identifier,
                        e.message
                    );
                    errors.push(e.clone());

                    // Only stop if error is fatal
                    if e.fatal {
                        break;
                    }
                }
            }
        }

        (current_event, ctx, errors)
    }

    /// Emit event and process any events emitted by handlers
    ///
    /// This recursively processes events emitted via `ctx.emit()` during handling.
    pub fn emit_recursive(&self, event: Event) -> Vec<(Event, Vec<HandlerError>)> {
        let mut results = Vec::new();
        let mut pending = vec![event];

        while let Some(evt) = pending.pop() {
            let (processed, mut ctx, errors) = self.emit(evt);
            results.push((processed, errors));

            // Add any events emitted by handlers
            pending.extend(ctx.take_emitted());
        }

        results
    }
}

impl fmt::Debug for EventBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventBus")
            .field("handler_count", &self.handlers.len())
            .field("handlers", &self.handlers)
            .finish()
    }
}

/// Simple glob pattern matching (reused from hook_types)
///
/// Supports `*` (matches any sequence) and `?` (matches single char)
fn match_glob(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let mut pattern_idx = 0;
    let mut text_idx = 0;
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    let mut star_idx: Option<usize> = None;
    let mut match_idx: Option<usize> = None;

    while text_idx < text_chars.len() {
        if pattern_idx < pattern_chars.len() && pattern_chars[pattern_idx] == '*' {
            star_idx = Some(pattern_idx);
            match_idx = Some(text_idx);
            pattern_idx += 1;
        } else if pattern_idx < pattern_chars.len()
            && (pattern_chars[pattern_idx] == text_chars[text_idx]
                || pattern_chars[pattern_idx] == '?')
        {
            pattern_idx += 1;
            text_idx += 1;
        } else if let Some(star) = star_idx {
            pattern_idx = star + 1;
            match_idx = Some(match_idx.unwrap() + 1);
            text_idx = match_idx.unwrap();
        } else {
            return false;
        }
    }

    // Check for remaining stars in pattern
    while pattern_idx < pattern_chars.len() && pattern_chars[pattern_idx] == '*' {
        pattern_idx += 1;
    }

    pattern_idx == pattern_chars.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_event_type_parse() {
        assert_eq!(EventType::parse("tool:before"), Some(EventType::ToolBefore));
        assert_eq!(EventType::parse("tool:after"), Some(EventType::ToolAfter));
        assert_eq!(EventType::parse("note:parsed"), Some(EventType::NoteParsed));
        assert_eq!(EventType::parse("invalid"), None);
    }

    #[test]
    fn test_event_type_as_str() {
        assert_eq!(EventType::ToolBefore.as_str(), "tool:before");
        assert_eq!(EventType::ToolAfter.as_str(), "tool:after");
        assert_eq!(EventType::NoteParsed.as_str(), "note:parsed");
    }

    #[test]
    fn test_event_creation() {
        let event = Event::tool_after("just_test", json!({"output": "success"}));
        assert_eq!(event.event_type, EventType::ToolAfter);
        assert_eq!(event.identifier, "just_test");
        assert!(!event.cancelled);
        assert!(event.timestamp_ms > 0);
    }

    #[test]
    fn test_event_with_source() {
        let event = Event::tool_after("gh_search", json!({})).with_source("upstream:github");
        assert_eq!(event.source, Some("upstream:github".to_string()));
    }

    #[test]
    fn test_event_cancel() {
        let mut event = Event::tool_before("test", json!({}));
        assert!(!event.is_cancelled());
        event.cancel();
        assert!(event.is_cancelled());
    }

    #[test]
    fn test_event_context_metadata() {
        let mut ctx = EventContext::new();

        ctx.set("key1", json!("value1"));
        ctx.set("key2", json!(42));

        assert_eq!(ctx.get("key1"), Some(&json!("value1")));
        assert_eq!(ctx.get("key2"), Some(&json!(42)));
        assert_eq!(ctx.get("missing"), None);

        assert!(ctx.contains("key1"));
        assert!(!ctx.contains("missing"));

        let removed = ctx.remove("key1");
        assert_eq!(removed, Some(json!("value1")));
        assert!(!ctx.contains("key1"));
    }

    #[test]
    fn test_event_context_emit() {
        let mut ctx = EventContext::new();

        ctx.emit(Event::custom("event1", json!({})));
        ctx.emit_custom("event2", json!({"data": true}));

        let emitted = ctx.take_emitted();
        assert_eq!(emitted.len(), 2);
        assert_eq!(emitted[0].identifier, "event1");
        assert_eq!(emitted[1].identifier, "event2");

        // take_emitted clears the list
        assert!(ctx.take_emitted().is_empty());
    }

    #[test]
    fn test_handler_matches() {
        let handler = Handler::new(
            "test_handler",
            EventType::ToolAfter,
            "just_*",
            |_ctx, event| Ok(event),
        );

        let matching_event = Event::tool_after("just_test", json!({}));
        let non_matching_event = Event::tool_after("rune_test", json!({}));
        let wrong_type_event = Event::tool_before("just_test", json!({}));

        assert!(handler.matches(&matching_event));
        assert!(!handler.matches(&non_matching_event));
        assert!(!handler.matches(&wrong_type_event));
    }

    #[test]
    fn test_handler_disabled() {
        let handler = Handler::new("test_handler", EventType::ToolAfter, "*", |_ctx, event| {
            Ok(event)
        })
        .with_enabled(false);

        let event = Event::tool_after("anything", json!({}));
        assert!(!handler.matches(&event));
    }

    #[test]
    fn test_handler_priority() {
        let handler = Handler::new("test", EventType::ToolAfter, "*", |_ctx, event| Ok(event))
            .with_priority(50);

        assert_eq!(handler.priority, 50);
    }

    #[test]
    fn test_event_bus_register_and_emit() {
        let mut bus = EventBus::new();

        // Register a handler that modifies the payload
        bus.register(Handler::new(
            "modifier",
            EventType::ToolAfter,
            "*",
            |_ctx, mut event| {
                if let Some(obj) = event.payload.as_object_mut() {
                    obj.insert("modified".to_string(), json!(true));
                }
                Ok(event)
            },
        ));

        let event = Event::tool_after("test", json!({"original": true}));
        let (result, _ctx, errors) = bus.emit(event);

        assert!(errors.is_empty());
        assert_eq!(result.payload["original"], json!(true));
        assert_eq!(result.payload["modified"], json!(true));
    }

    #[test]
    fn test_event_bus_priority_ordering() {
        let mut bus = EventBus::new();

        // Register handlers in reverse priority order
        bus.register(
            Handler::new("third", EventType::ToolAfter, "*", |_ctx, mut event| {
                let val = event.payload.as_str().unwrap_or("");
                event.payload = json!(format!("{}3", val));
                Ok(event)
            })
            .with_priority(300),
        );

        bus.register(
            Handler::new("first", EventType::ToolAfter, "*", |_ctx, mut event| {
                let val = event.payload.as_str().unwrap_or("");
                event.payload = json!(format!("{}1", val));
                Ok(event)
            })
            .with_priority(100),
        );

        bus.register(
            Handler::new("second", EventType::ToolAfter, "*", |_ctx, mut event| {
                let val = event.payload.as_str().unwrap_or("");
                event.payload = json!(format!("{}2", val));
                Ok(event)
            })
            .with_priority(200),
        );

        let event = Event::tool_after("test", json!(""));
        let (result, _ctx, _errors) = bus.emit(event);

        // Should execute in priority order: first(100), second(200), third(300)
        assert_eq!(result.payload, json!("123"));
    }

    #[test]
    fn test_event_bus_fail_open() {
        let mut bus = EventBus::new();

        // First handler fails
        bus.register(
            Handler::new("failing", EventType::ToolAfter, "*", |_ctx, _event| {
                Err(HandlerError::non_fatal("failing", "intentional failure"))
            })
            .with_priority(100),
        );

        // Second handler should still run
        bus.register(
            Handler::new(
                "succeeding",
                EventType::ToolAfter,
                "*",
                |_ctx, mut event| {
                    event.payload = json!("success");
                    Ok(event)
                },
            )
            .with_priority(200),
        );

        let event = Event::tool_after("test", json!("original"));
        let (result, _ctx, errors) = bus.emit(event);

        // Second handler ran despite first failing
        assert_eq!(result.payload, json!("success"));
        // Error was recorded
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].handler_name, "failing");
    }

    #[test]
    fn test_event_bus_fatal_error_stops_pipeline() {
        let mut bus = EventBus::new();

        bus.register(
            Handler::new("fatal", EventType::ToolAfter, "*", |_ctx, _event| {
                Err(HandlerError::fatal("fatal", "stop everything"))
            })
            .with_priority(100),
        );

        bus.register(
            Handler::new(
                "never_runs",
                EventType::ToolAfter,
                "*",
                |_ctx, mut event| {
                    event.payload = json!("should not see this");
                    Ok(event)
                },
            )
            .with_priority(200),
        );

        let event = Event::tool_after("test", json!("original"));
        let (result, _ctx, errors) = bus.emit(event);

        // Pipeline stopped, payload unchanged
        assert_eq!(result.payload, json!("original"));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].fatal);
    }

    #[test]
    fn test_event_bus_cancel_event() {
        let mut bus = EventBus::new();

        bus.register(
            Handler::new(
                "canceller",
                EventType::ToolBefore,
                "*",
                |_ctx, mut event| {
                    event.cancel();
                    Ok(event)
                },
            )
            .with_priority(100),
        );

        bus.register(
            Handler::new(
                "never_runs",
                EventType::ToolBefore,
                "*",
                |_ctx, mut event| {
                    event.payload = json!("should not see this");
                    Ok(event)
                },
            )
            .with_priority(200),
        );

        let event = Event::tool_before("test", json!("original"));
        let (result, _ctx, _errors) = bus.emit(event);

        assert!(result.is_cancelled());
        assert_eq!(result.payload, json!("original")); // Second handler didn't run
    }

    #[test]
    fn test_event_bus_unregister() {
        let mut bus = EventBus::new();

        bus.register(Handler::new(
            "test",
            EventType::ToolAfter,
            "*",
            |_ctx, e| Ok(e),
        ));

        assert_eq!(bus.count_handlers(EventType::ToolAfter), 1);

        let removed = bus.unregister("test");
        assert!(removed);
        assert_eq!(bus.count_handlers(EventType::ToolAfter), 0);

        // Removing non-existent handler returns false
        assert!(!bus.unregister("nonexistent"));
    }

    #[test]
    fn test_event_bus_pattern_matching() {
        let mut bus = EventBus::new();

        bus.register(Handler::new(
            "just_only",
            EventType::ToolAfter,
            "just_*",
            |_ctx, mut event| {
                event.payload = json!("just");
                Ok(event)
            },
        ));

        // Matching event
        let just_event = Event::tool_after("just_test", json!(""));
        let (result, _ctx, _errors) = bus.emit(just_event);
        assert_eq!(result.payload, json!("just"));

        // Non-matching event
        let rune_event = Event::tool_after("rune_tool", json!("original"));
        let (result, _ctx, _errors) = bus.emit(rune_event);
        assert_eq!(result.payload, json!("original")); // Unchanged
    }

    #[test]
    fn test_event_bus_emit_recursive() {
        let mut bus = EventBus::new();

        // Handler that emits a secondary event
        bus.register(Handler::new(
            "emitter",
            EventType::ToolAfter,
            "primary",
            |ctx, event| {
                ctx.emit(Event::custom("secondary", json!({"from": "emitter"})));
                Ok(event)
            },
        ));

        // Handler for secondary event
        bus.register(Handler::new(
            "secondary_handler",
            EventType::Custom,
            "secondary",
            |_ctx, mut event| {
                event.payload = json!({"handled": true});
                Ok(event)
            },
        ));

        let event = Event::tool_after("primary", json!({}));
        let results = bus.emit_recursive(event);

        assert_eq!(results.len(), 2);
        // First result is primary event
        assert_eq!(results[0].0.identifier, "primary");
        // Second result is secondary event
        assert_eq!(results[1].0.identifier, "secondary");
        assert_eq!(results[1].0.payload["handled"], json!(true));
    }

    #[test]
    fn test_glob_pattern_star() {
        assert!(match_glob("*", "anything"));
        assert!(match_glob("just_*", "just_test"));
        assert!(match_glob("just_*", "just_build"));
        assert!(match_glob("*_test", "unit_test"));
        assert!(match_glob("*_test_*", "unit_test_foo"));
        assert!(!match_glob("just_*", "other_test"));
    }

    #[test]
    fn test_glob_pattern_question() {
        assert!(match_glob("test?", "tests"));
        assert!(match_glob("t?st", "test"));
        assert!(!match_glob("test?", "test"));
        assert!(!match_glob("test?", "testing"));
    }

    #[test]
    fn test_glob_pattern_exact() {
        assert!(match_glob("exact", "exact"));
        assert!(!match_glob("exact", "exacty"));
        assert!(!match_glob("exact", "exac"));
    }

    #[test]
    fn test_event_serialization() {
        let event = Event::tool_after("test", json!({"key": "value"})).with_source("kiln");

        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.event_type, EventType::ToolAfter);
        assert_eq!(parsed.identifier, "test");
        assert_eq!(parsed.payload["key"], "value");
        assert_eq!(parsed.source, Some("kiln".to_string()));
    }

    #[test]
    fn test_handler_error_display() {
        let err = HandlerError::non_fatal("my_handler", "something went wrong");
        assert_eq!(
            format!("{}", err),
            "Handler 'my_handler' error: something went wrong"
        );

        let fatal_err = HandlerError::fatal("my_handler", "critical failure");
        assert_eq!(
            format!("{}", fatal_err),
            "Handler 'my_handler' error: critical failure (fatal)"
        );
    }
}
