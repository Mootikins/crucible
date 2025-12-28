//! Handler trait and result types for event processing.
//!
//! This module provides:
//!
//! - [`Handler`]: Async trait for event handlers (Rust, Rune, Lua)
//! - [`HandlerResult`]: Result enum controlling event flow
//! - [`HandlerContext`]: Context passed through handler chain
//!
//! # Architecture
//!
//! The handler system follows the Reactor pattern:
//!
//! ```text
//! Reactor (core owns)
//!    │
//!    ├── Rust handlers (built-in)
//!    ├── Rune handlers (script)
//!    └── Lua handlers (script)
//!
//! All handlers implement the same Handler trait and interleave
//! in dependency + priority order.
//! ```
//!
//! # Handler Trait
//!
//! Handlers implement the async [`Handler`] trait:
//!
//! ```ignore
//! use crucible_core::events::{Handler, HandlerContext, HandlerResult, SessionEvent};
//! use async_trait::async_trait;
//!
//! struct LoggingHandler;
//!
//! #[async_trait]
//! impl Handler for LoggingHandler {
//!     fn name(&self) -> &str { "logging" }
//!
//!     fn priority(&self) -> i32 { 10 } // Run early
//!
//!     async fn handle(
//!         &self,
//!         ctx: &mut HandlerContext,
//!         event: SessionEvent,
//!     ) -> HandlerResult<SessionEvent> {
//!         tracing::info!("Event: {:?}", event.event_type());
//!         HandlerResult::ok(event)
//!     }
//! }
//! ```
//!
//! # Handler Dependencies
//!
//! Handlers can declare dependencies on other handlers:
//!
//! ```ignore
//! impl Handler for PersistHandler {
//!     fn name(&self) -> &str { "persist" }
//!
//!     fn dependencies(&self) -> &[&str] {
//!         &["validate", "transform"]  // Must run after these
//!     }
//!
//!     // ...
//! }
//! ```
//!
//! # Result Variants
//!
//! - [`HandlerResult::Continue`]: Processing succeeded, pass event to next handler
//! - [`HandlerResult::Cancel`]: Stop processing, event is cancelled
//! - [`HandlerResult::SoftError`]: Non-fatal error, continue with event
//! - [`HandlerResult::FatalError`]: Fatal error, stop processing immediately

use super::emitter::EventError;
use super::session_event::SessionEvent;
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// Result returned by event handlers to control event flow.
///
/// Handlers return this enum to indicate how event processing should continue.
/// The event system uses this to determine whether to:
/// - Pass the event to the next handler
/// - Stop processing (cancellation or fatal error)
/// - Log errors but continue (soft error)
///
/// # Variants Summary
///
/// | Variant | Continues? | Preserves Event? | Use Case |
/// |---------|------------|------------------|----------|
/// | `Continue` | Yes | Yes | Normal processing |
/// | `Cancel` | No | No | Stop without event access |
/// | `Cancelled` | No | Yes | Stop but preserve event |
/// | `SoftError` | Yes | Yes | Non-fatal error |
/// | `FatalError` | No | No | Fatal error |
#[derive(Debug, Clone)]
pub enum HandlerResult<E> {
    /// Handler processed successfully, continue with the (possibly modified) event.
    ///
    /// The event may be modified by the handler before being passed to the next
    /// handler in the chain.
    Continue(E),

    /// Handler cancelled the event, stop processing (event discarded).
    ///
    /// Use this for events like `ToolCalled` where a handler wants to prevent
    /// the tool from executing. The event will not be passed to subsequent handlers.
    /// Use `Cancelled(E)` if you need to preserve the event for inspection.
    Cancel,

    /// Handler cancelled the event, stop processing (event preserved).
    ///
    /// Similar to `Cancel` but preserves the event for inspection by the caller.
    /// Useful when the cancellation reason depends on event content that the
    /// caller may want to log or inspect.
    Cancelled(E),

    /// Handler encountered a recoverable error.
    ///
    /// The event continues to the next handler, but the error is logged.
    /// Use this for non-critical failures that shouldn't stop the pipeline.
    SoftError {
        /// The event to continue processing
        event: E,
        /// Error message describing what went wrong
        error: String,
    },

    /// Handler encountered a fatal error, stop processing immediately.
    ///
    /// The event pipeline stops and the error is propagated to the caller.
    /// Use sparingly - most errors should be soft errors to maintain fail-open semantics.
    FatalError(EventError),
}

impl<E> HandlerResult<E> {
    /// Create a continue result with the given event.
    pub fn ok(event: E) -> Self {
        Self::Continue(event)
    }

    /// Create a cancel result (event discarded).
    pub fn cancel() -> Self {
        Self::Cancel
    }

    /// Create a cancelled result (event preserved).
    ///
    /// Use this when you need to cancel processing but preserve the event
    /// for inspection by the caller.
    pub fn cancelled(event: E) -> Self {
        Self::Cancelled(event)
    }

    /// Create a soft error result.
    pub fn soft_error(event: E, error: impl Into<String>) -> Self {
        Self::SoftError {
            event,
            error: error.into(),
        }
    }

    /// Create a fatal error result.
    pub fn fatal(error: EventError) -> Self {
        Self::FatalError(error)
    }

    /// Create a fatal error with a message.
    pub fn fatal_msg(message: impl Into<String>) -> Self {
        Self::FatalError(EventError::other(message))
    }

    /// Check if this result indicates successful continuation.
    pub fn is_continue(&self) -> bool {
        matches!(self, Self::Continue(_))
    }

    /// Check if this result is a cancellation (Cancel or Cancelled).
    pub fn is_cancel(&self) -> bool {
        matches!(self, Self::Cancel | Self::Cancelled(_))
    }

    /// Check if this result is a cancellation with preserved event.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled(_))
    }

    /// Check if this result is a soft error.
    pub fn is_soft_error(&self) -> bool {
        matches!(self, Self::SoftError { .. })
    }

    /// Check if this result is a fatal error.
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::FatalError(_))
    }

    /// Check if processing should continue (Continue or SoftError).
    pub fn should_continue(&self) -> bool {
        matches!(self, Self::Continue(_) | Self::SoftError { .. })
    }

    /// Check if processing should stop (Cancel, Cancelled, or FatalError).
    pub fn should_stop(&self) -> bool {
        matches!(
            self,
            Self::Cancel | Self::Cancelled(_) | Self::FatalError(_)
        )
    }

    /// Get the event if available (Continue, Cancelled, or SoftError).
    pub fn event(self) -> Option<E> {
        match self {
            Self::Continue(e) | Self::Cancelled(e) | Self::SoftError { event: e, .. } => Some(e),
            Self::Cancel | Self::FatalError(_) => None,
        }
    }

    /// Get the event reference if available.
    pub fn event_ref(&self) -> Option<&E> {
        match self {
            Self::Continue(e) | Self::Cancelled(e) | Self::SoftError { event: e, .. } => Some(e),
            Self::Cancel | Self::FatalError(_) => None,
        }
    }

    /// Get the error message if this is a soft error.
    pub fn soft_error_msg(&self) -> Option<&str> {
        match self {
            Self::SoftError { error, .. } => Some(error),
            _ => None,
        }
    }

    /// Get the fatal error if this is a fatal error.
    pub fn fatal_error(self) -> Option<EventError> {
        match self {
            Self::FatalError(e) => Some(e),
            _ => None,
        }
    }

    /// Map the event type using a function.
    pub fn map<F, U>(self, f: F) -> HandlerResult<U>
    where
        F: FnOnce(E) -> U,
    {
        match self {
            Self::Continue(e) => HandlerResult::Continue(f(e)),
            Self::Cancel => HandlerResult::Cancel,
            Self::Cancelled(e) => HandlerResult::Cancelled(f(e)),
            Self::SoftError { event, error } => HandlerResult::SoftError {
                event: f(event),
                error,
            },
            Self::FatalError(e) => HandlerResult::FatalError(e),
        }
    }
}

impl<E: Default> Default for HandlerResult<E> {
    fn default() -> Self {
        Self::Continue(E::default())
    }
}

impl<E: std::fmt::Debug> std::fmt::Display for HandlerResult<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Continue(_) => write!(f, "Continue"),
            Self::Cancel => write!(f, "Cancel"),
            Self::Cancelled(_) => write!(f, "Cancelled"),
            Self::SoftError { error, .. } => write!(f, "SoftError: {}", error),
            Self::FatalError(e) => write!(f, "FatalError: {}", e),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Handler Trait
// ─────────────────────────────────────────────────────────────────────────────

/// Async event handler trait.
///
/// Handlers are registered with the Reactor and process events in dependency
/// + priority order. They can be implemented in Rust, Rune, or Lua.
///
/// # Identification
///
/// Each handler has a unique name used for:
/// - Dependency resolution (other handlers can depend on this one)
/// - Logging and debugging
/// - Registration deduplication
///
/// Naming convention: `"language:path:function"` (e.g., `"rune:auth.rn:check_perms"`)
///
/// # Dependencies
///
/// Handlers declare dependencies via [`Handler::dependencies`]. The Reactor
/// ensures dependent handlers complete before this handler runs.
///
/// # Priority
///
/// Priority is a tiebreaker when handlers have no dependency relationship.
/// Lower values run earlier (default: 50).
///
/// # Event Patterns
///
/// Handlers can filter events by pattern (e.g., `"tool:*"`, `"note:modified"`).
/// The Reactor only invokes handlers whose pattern matches the event type.
///
/// # Async Execution
///
/// The `handle` method is async. For sync scripts (Rune, Lua), implementations
/// should use `tokio::task::spawn_blocking` to avoid blocking the reactor.
#[async_trait]
pub trait Handler: Send + Sync {
    /// Unique identifier for this handler.
    ///
    /// Used for dependency resolution and logging.
    /// Convention: `"language:path:function"` (e.g., `"rust:builtin:persist"`)
    fn name(&self) -> &str;

    /// Handlers that must complete before this one runs.
    ///
    /// Returns handler names (as returned by [`Handler::name`]).
    /// Empty by default.
    fn dependencies(&self) -> &[&str] {
        &[]
    }

    /// Priority for ordering when no dependency relationship exists.
    ///
    /// Lower values run earlier. Default is 50.
    /// Range: 0 (highest priority) to 100 (lowest priority).
    fn priority(&self) -> i32 {
        50
    }

    /// Event type pattern to match.
    ///
    /// Uses glob patterns:
    /// - `"*"` matches all events (default)
    /// - `"tool:*"` matches all tool events
    /// - `"note:modified"` matches only note_modified events
    fn event_pattern(&self) -> &str {
        "*"
    }

    /// Process an event.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Handler context for metadata passing and event emission
    /// * `event` - The event to process
    ///
    /// # Returns
    ///
    /// A `HandlerResult` controlling event flow:
    /// - `Continue(event)` - Pass (modified) event to next handler
    /// - `Cancel` - Stop processing, discard event
    /// - `SoftError` - Log error, continue processing
    /// - `FatalError` - Stop processing with error
    async fn handle(
        &self,
        ctx: &mut HandlerContext,
        event: SessionEvent,
    ) -> HandlerResult<SessionEvent>;
}

/// Boxed handler for type erasure.
pub type BoxedHandler = Box<dyn Handler>;

/// Arc-wrapped handler for shared ownership.
pub type SharedHandler = Arc<dyn Handler>;

// ─────────────────────────────────────────────────────────────────────────────
// Handler Context
// ─────────────────────────────────────────────────────────────────────────────

/// Context passed through the handler chain.
///
/// `HandlerContext` provides:
///
/// - **Metadata storage**: Cross-handler state via key-value pairs
/// - **Execution trace**: Record of handlers that have run
/// - **Event emission**: Ability to emit follow-up events
///
/// # Metadata
///
/// Handlers can store arbitrary metadata that subsequent handlers can read:
///
/// ```ignore
/// ctx.set("validated", json!(true));
/// // Later handler:
/// if ctx.get("validated") == Some(&json!(true)) { ... }
/// ```
///
/// # Event Emission
///
/// Handlers can emit additional events to be processed after the current chain:
///
/// ```ignore
/// ctx.emit(SessionEvent::Custom {
///     name: "follow_up".into(),
///     payload: json!({}),
/// });
/// ```
#[derive(Debug, Clone, Default)]
pub struct HandlerContext {
    /// Arbitrary metadata (shared between handlers).
    metadata: HashMap<String, JsonValue>,

    /// Handler execution trace (for debugging).
    trace: Vec<HandlerTraceEntry>,

    /// Handlers that have completed processing.
    completed: Vec<String>,

    /// Events emitted during processing (queued for later dispatch).
    emitted: Vec<SessionEvent>,
}

impl HandlerContext {
    /// Create a new empty context.
    pub fn new() -> Self {
        Self::default()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Metadata storage
    // ─────────────────────────────────────────────────────────────────────────

    /// Store a metadata value.
    pub fn set(&mut self, key: impl Into<String>, value: JsonValue) {
        self.metadata.insert(key.into(), value);
    }

    /// Get a metadata value by key.
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        self.metadata.get(key)
    }

    /// Remove and return a metadata value.
    pub fn remove(&mut self, key: &str) -> Option<JsonValue> {
        self.metadata.remove(key)
    }

    /// Check if a metadata key exists.
    pub fn has(&self, key: &str) -> bool {
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
    // Execution trace
    // ─────────────────────────────────────────────────────────────────────────

    /// Record a handler execution.
    pub fn record_handler(&mut self, name: impl Into<String>, duration: std::time::Duration) {
        self.trace.push(HandlerTraceEntry {
            handler: name.into(),
            duration_ms: duration.as_millis() as u64,
            outcome: "ok".into(),
        });
    }

    /// Record a handler execution with outcome.
    pub fn record_handler_with_outcome(
        &mut self,
        name: impl Into<String>,
        duration: std::time::Duration,
        outcome: impl Into<String>,
    ) {
        self.trace.push(HandlerTraceEntry {
            handler: name.into(),
            duration_ms: duration.as_millis() as u64,
            outcome: outcome.into(),
        });
    }

    /// Get the execution trace.
    pub fn trace(&self) -> &[HandlerTraceEntry] {
        &self.trace
    }

    /// Clear the execution trace.
    pub fn clear_trace(&mut self) {
        self.trace.clear();
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Completed handlers
    // ─────────────────────────────────────────────────────────────────────────

    /// Mark a handler as completed.
    pub fn mark_completed(&mut self, name: impl Into<String>) {
        self.completed.push(name.into());
    }

    /// Check if a handler has completed.
    pub fn is_completed(&self, name: &str) -> bool {
        self.completed.iter().any(|s| s == name)
    }

    /// Get all completed handler names.
    pub fn completed(&self) -> &[String] {
        &self.completed
    }

    /// Clear completed handlers.
    pub fn clear_completed(&mut self) {
        self.completed.clear();
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Event emission
    // ─────────────────────────────────────────────────────────────────────────

    /// Emit an event to be processed after the current chain completes.
    pub fn emit(&mut self, event: SessionEvent) {
        self.emitted.push(event);
    }

    /// Take all emitted events.
    pub fn take_emitted(&mut self) -> Vec<SessionEvent> {
        std::mem::take(&mut self.emitted)
    }

    /// Check if any events have been emitted.
    pub fn has_emitted(&self) -> bool {
        !self.emitted.is_empty()
    }

    /// Get the number of emitted events.
    pub fn emitted_count(&self) -> usize {
        self.emitted.len()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Reset
    // ─────────────────────────────────────────────────────────────────────────

    /// Reset the context for processing a new event.
    ///
    /// Clears completed handlers and emitted events, but preserves metadata.
    pub fn reset_for_event(&mut self) {
        self.completed.clear();
        self.emitted.clear();
        // Metadata is preserved across events
    }

    /// Fully reset the context.
    pub fn reset(&mut self) {
        self.metadata.clear();
        self.trace.clear();
        self.completed.clear();
        self.emitted.clear();
    }
}

/// Entry in the handler execution trace.
#[derive(Debug, Clone)]
pub struct HandlerTraceEntry {
    /// Handler name.
    pub handler: String,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
    /// Outcome string (e.g., "ok", "cancelled", "error").
    pub outcome: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Pattern Matching Helper
// ─────────────────────────────────────────────────────────────────────────────

/// Check if an event type matches a pattern.
///
/// Patterns:
/// - `"*"` matches everything
/// - `"tool:*"` matches `"tool_called"`, `"tool_completed"`, etc.
/// - `"note:modified"` matches exactly `"note_modified"`
pub fn matches_event_pattern(pattern: &str, event_type: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    // Handle "prefix:*" patterns
    if let Some(prefix) = pattern.strip_suffix(":*") {
        // e.g., "tool:*" should match "tool_called", "tool_completed"
        // Convert pattern prefix to event type prefix
        return event_type.starts_with(prefix);
    }

    // Handle "prefix:suffix" patterns
    if let Some((prefix, suffix)) = pattern.split_once(':') {
        // e.g., "note:modified" should match "note_modified"
        let expected = format!("{}_{}", prefix, suffix);
        return event_type == expected;
    }

    // Exact match
    pattern == event_type
}

// ─────────────────────────────────────────────────────────────────────────────
// Handler Timing Helper
// ─────────────────────────────────────────────────────────────────────────────

/// Helper for timing handler execution.
pub struct HandlerTimer {
    start: Instant,
    handler_name: String,
}

impl HandlerTimer {
    /// Start timing a handler.
    pub fn start(handler_name: impl Into<String>) -> Self {
        Self {
            start: Instant::now(),
            handler_name: handler_name.into(),
        }
    }

    /// Stop timing and record in context.
    pub fn stop(self, ctx: &mut HandlerContext) {
        ctx.record_handler(self.handler_name, self.start.elapsed());
    }

    /// Stop timing and record with custom outcome.
    pub fn stop_with_outcome(self, ctx: &mut HandlerContext, outcome: impl Into<String>) {
        ctx.record_handler_with_outcome(self.handler_name, self.start.elapsed(), outcome);
    }

    /// Get elapsed duration.
    pub fn elapsed(&self) -> std::time::Duration {
        self.start.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_result_continue() {
        let result: HandlerResult<String> = HandlerResult::ok("event".into());
        assert!(result.is_continue());
        assert!(!result.is_cancel());
        assert!(!result.is_soft_error());
        assert!(!result.is_fatal());
        assert!(result.should_continue());
        assert!(!result.should_stop());
    }

    #[test]
    fn test_handler_result_cancel() {
        let result: HandlerResult<String> = HandlerResult::cancel();
        assert!(!result.is_continue());
        assert!(result.is_cancel());
        assert!(!result.is_cancelled()); // Cancel without event
        assert!(!result.is_soft_error());
        assert!(!result.is_fatal());
        assert!(!result.should_continue());
        assert!(result.should_stop());
        assert!(result.event().is_none());
    }

    #[test]
    fn test_handler_result_cancelled_with_event() {
        let result: HandlerResult<String> = HandlerResult::cancelled("preserved".into());
        assert!(!result.is_continue());
        assert!(result.is_cancel()); // Both Cancel and Cancelled are "cancelled"
        assert!(result.is_cancelled()); // But only Cancelled preserves event
        assert!(!result.is_soft_error());
        assert!(!result.is_fatal());
        assert!(!result.should_continue());
        assert!(result.should_stop());
        assert_eq!(result.event(), Some("preserved".into())); // Event preserved!
    }

    #[test]
    fn test_handler_result_soft_error() {
        let result: HandlerResult<String> =
            HandlerResult::soft_error("event".into(), "something went wrong");
        assert!(!result.is_continue());
        assert!(!result.is_cancel());
        assert!(result.is_soft_error());
        assert!(!result.is_fatal());
        assert!(result.should_continue());
        assert!(!result.should_stop());
        assert_eq!(result.soft_error_msg(), Some("something went wrong"));
    }

    #[test]
    fn test_handler_result_fatal() {
        let result: HandlerResult<String> =
            HandlerResult::fatal(EventError::other("critical failure"));
        assert!(!result.is_continue());
        assert!(!result.is_cancel());
        assert!(!result.is_soft_error());
        assert!(result.is_fatal());
        assert!(!result.should_continue());
        assert!(result.should_stop());
        assert!(result.event().is_none());
    }

    #[test]
    fn test_handler_result_fatal_msg() {
        let result: HandlerResult<String> = HandlerResult::fatal_msg("critical failure");
        assert!(result.is_fatal());
    }

    #[test]
    fn test_handler_result_event() {
        let result: HandlerResult<String> = HandlerResult::ok("test".into());
        assert_eq!(result.event(), Some("test".into()));

        let result: HandlerResult<String> = HandlerResult::cancel();
        assert_eq!(result.event(), None);

        let result: HandlerResult<String> = HandlerResult::cancelled("preserved".into());
        assert_eq!(result.event(), Some("preserved".into()));

        let result: HandlerResult<String> = HandlerResult::soft_error("test".into(), "error");
        assert_eq!(result.event(), Some("test".into()));
    }

    #[test]
    fn test_handler_result_event_ref() {
        let result: HandlerResult<String> = HandlerResult::ok("test".into());
        assert_eq!(result.event_ref(), Some(&"test".into()));

        let result: HandlerResult<String> = HandlerResult::cancel();
        assert_eq!(result.event_ref(), None);
    }

    #[test]
    fn test_handler_result_map() {
        let result: HandlerResult<i32> = HandlerResult::ok(42);
        let mapped = result.map(|n| n.to_string());
        assert_eq!(mapped.event(), Some("42".to_string()));

        let result: HandlerResult<i32> = HandlerResult::cancel();
        let mapped = result.map(|n| n.to_string());
        assert!(mapped.is_cancel());
        assert!(!mapped.is_cancelled());

        let result: HandlerResult<i32> = HandlerResult::cancelled(42);
        let mapped = result.map(|n| n.to_string());
        assert!(mapped.is_cancelled());
        assert_eq!(mapped.event(), Some("42".to_string()));

        let result: HandlerResult<i32> = HandlerResult::soft_error(42, "oops");
        let mapped = result.map(|n| n.to_string());
        assert!(mapped.is_soft_error());
        assert_eq!(mapped.event(), Some("42".to_string()));
    }

    #[test]
    fn test_handler_result_display() {
        let result: HandlerResult<String> = HandlerResult::ok("test".into());
        assert_eq!(format!("{}", result), "Continue");

        let result: HandlerResult<String> = HandlerResult::cancel();
        assert_eq!(format!("{}", result), "Cancel");

        let result: HandlerResult<String> = HandlerResult::cancelled("test".into());
        assert_eq!(format!("{}", result), "Cancelled");

        let result: HandlerResult<String> = HandlerResult::soft_error("test".into(), "oops");
        assert_eq!(format!("{}", result), "SoftError: oops");
    }

    #[test]
    fn test_handler_result_default() {
        let result: HandlerResult<String> = HandlerResult::default();
        assert!(result.is_continue());
        assert_eq!(result.event(), Some(String::default()));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // HandlerContext tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_handler_context_new() {
        let ctx = HandlerContext::new();
        assert!(ctx.metadata().is_empty());
        assert!(ctx.trace().is_empty());
        assert!(ctx.completed().is_empty());
        assert!(!ctx.has_emitted());
    }

    #[test]
    fn test_handler_context_metadata() {
        let mut ctx = HandlerContext::new();

        // Set and get
        ctx.set("key1", serde_json::json!("value1"));
        assert!(ctx.has("key1"));
        assert_eq!(ctx.get("key1"), Some(&serde_json::json!("value1")));
        assert!(!ctx.has("missing"));
        assert_eq!(ctx.get("missing"), None);

        // Remove
        let removed = ctx.remove("key1");
        assert_eq!(removed, Some(serde_json::json!("value1")));
        assert!(!ctx.has("key1"));

        // Clear
        ctx.set("key2", serde_json::json!(42));
        ctx.clear_metadata();
        assert!(ctx.metadata().is_empty());
    }

    #[test]
    fn test_handler_context_trace() {
        let mut ctx = HandlerContext::new();

        ctx.record_handler("handler1", std::time::Duration::from_millis(10));
        ctx.record_handler_with_outcome(
            "handler2",
            std::time::Duration::from_millis(20),
            "cancelled",
        );

        let trace = ctx.trace();
        assert_eq!(trace.len(), 2);
        assert_eq!(trace[0].handler, "handler1");
        assert_eq!(trace[0].duration_ms, 10);
        assert_eq!(trace[0].outcome, "ok");
        assert_eq!(trace[1].handler, "handler2");
        assert_eq!(trace[1].outcome, "cancelled");

        ctx.clear_trace();
        assert!(ctx.trace().is_empty());
    }

    #[test]
    fn test_handler_context_completed() {
        let mut ctx = HandlerContext::new();

        ctx.mark_completed("handler1");
        ctx.mark_completed("handler2");

        assert!(ctx.is_completed("handler1"));
        assert!(ctx.is_completed("handler2"));
        assert!(!ctx.is_completed("handler3"));

        let completed = ctx.completed();
        assert_eq!(completed.len(), 2);

        ctx.clear_completed();
        assert!(ctx.completed().is_empty());
    }

    #[test]
    fn test_handler_context_emit() {
        let mut ctx = HandlerContext::new();

        assert!(!ctx.has_emitted());
        assert_eq!(ctx.emitted_count(), 0);

        ctx.emit(SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        });

        assert!(ctx.has_emitted());
        assert_eq!(ctx.emitted_count(), 1);

        let emitted = ctx.take_emitted();
        assert_eq!(emitted.len(), 1);
        assert!(!ctx.has_emitted());
    }

    #[test]
    fn test_handler_context_reset() {
        let mut ctx = HandlerContext::new();

        ctx.set("key", serde_json::json!("value"));
        ctx.mark_completed("handler");
        ctx.emit(SessionEvent::default());

        // Reset for event preserves metadata
        ctx.reset_for_event();
        assert!(ctx.has("key"));
        assert!(ctx.completed().is_empty());
        assert!(!ctx.has_emitted());

        // Full reset clears everything
        ctx.mark_completed("handler");
        ctx.reset();
        assert!(!ctx.has("key"));
        assert!(ctx.completed().is_empty());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Pattern matching tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_matches_event_pattern_wildcard() {
        assert!(matches_event_pattern("*", "tool_called"));
        assert!(matches_event_pattern("*", "note_modified"));
        assert!(matches_event_pattern("*", "anything"));
    }

    #[test]
    fn test_matches_event_pattern_prefix() {
        assert!(matches_event_pattern("tool:*", "tool_called"));
        assert!(matches_event_pattern("tool:*", "tool_completed"));
        assert!(!matches_event_pattern("tool:*", "note_modified"));

        assert!(matches_event_pattern("note:*", "note_parsed"));
        assert!(matches_event_pattern("note:*", "note_modified"));
        assert!(!matches_event_pattern("note:*", "tool_called"));
    }

    #[test]
    fn test_matches_event_pattern_specific() {
        assert!(matches_event_pattern("note:modified", "note_modified"));
        assert!(!matches_event_pattern("note:modified", "note_parsed"));
        assert!(!matches_event_pattern("note:modified", "tool_called"));

        assert!(matches_event_pattern("tool:called", "tool_called"));
        assert!(!matches_event_pattern("tool:called", "tool_completed"));
    }

    #[test]
    fn test_matches_event_pattern_exact() {
        assert!(matches_event_pattern("custom", "custom"));
        assert!(!matches_event_pattern("custom", "other"));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // HandlerTimer tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_handler_timer() {
        let mut ctx = HandlerContext::new();
        let timer = HandlerTimer::start("test_handler");

        std::thread::sleep(std::time::Duration::from_millis(1));

        timer.stop(&mut ctx);

        let trace = ctx.trace();
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].handler, "test_handler");
        assert!(trace[0].duration_ms >= 1);
        assert_eq!(trace[0].outcome, "ok");
    }

    #[test]
    fn test_handler_timer_with_outcome() {
        let mut ctx = HandlerContext::new();
        let timer = HandlerTimer::start("test_handler");

        timer.stop_with_outcome(&mut ctx, "cancelled");

        let trace = ctx.trace();
        assert_eq!(trace[0].outcome, "cancelled");
    }

    #[test]
    fn test_handler_timer_elapsed() {
        let timer = HandlerTimer::start("test");
        std::thread::sleep(std::time::Duration::from_millis(1));
        assert!(timer.elapsed() >= std::time::Duration::from_millis(1));
    }
}
