//! The Reactor - Single Event Loop for Handler Dispatch
//!
//! This module provides the `Reactor` which is the central event dispatcher
//! following the [Reactor pattern](https://en.wikipedia.org/wiki/Reactor_pattern).
//!
//! ## Design
//!
//! The Reactor:
//! - Owns all handlers (Rust, Rune, Lua)
//! - Maintains dependency graph for ordering
//! - Dispatches events through matching handlers
//! - Handles async execution with spawn_blocking for sync scripts
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_core::events::{Reactor, Handler, SessionEvent};
//!
//! let mut reactor = Reactor::new();
//!
//! // Register handlers (any language)
//! reactor.register(Box::new(LoggingHandler))?;
//! reactor.register(Box::new(PersistHandler))?;
//! reactor.register(rune_handler)?;
//! reactor.register(lua_handler)?;
//!
//! // Emit an event - handlers run in dependency + priority order
//! let result = reactor.emit(event).await?;
//! ```

use super::dependency::{DependencyError, DependencyGraph};
use super::handler::{
    matches_event_pattern, BoxedHandler, HandlerContext, HandlerResult, HandlerTimer,
    SharedHandler,
};
#[cfg(test)]
use super::handler::Handler;
use super::session_event::SessionEvent;
use std::collections::HashMap;
use std::sync::Arc;

/// Result of emitting an event through the Reactor.
#[derive(Debug)]
pub enum EmitResult {
    /// Event was processed by all handlers successfully.
    Completed {
        /// The final event after all handler processing.
        event: SessionEvent,
        /// Number of handlers that processed the event.
        handler_count: usize,
        /// Handlers that ran (in order).
        handlers_run: Vec<String>,
    },
    /// Event was cancelled by a handler.
    Cancelled {
        /// Handler that cancelled the event.
        by_handler: String,
        /// Handlers that ran before cancellation.
        handlers_run: Vec<String>,
    },
    /// A handler returned a fatal error.
    Failed {
        /// Handler that failed.
        handler: String,
        /// Error message.
        error: String,
        /// Handlers that ran before failure.
        handlers_run: Vec<String>,
    },
}

impl EmitResult {
    /// Check if the event completed successfully.
    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }

    /// Check if the event was cancelled.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled { .. })
    }

    /// Check if processing failed.
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    /// Get the final event if completed.
    pub fn event(self) -> Option<SessionEvent> {
        match self {
            Self::Completed { event, .. } => Some(event),
            _ => None,
        }
    }

    /// Get the handlers that ran.
    pub fn handlers_run(&self) -> &[String] {
        match self {
            Self::Completed { handlers_run, .. } => handlers_run,
            Self::Cancelled { handlers_run, .. } => handlers_run,
            Self::Failed { handlers_run, .. } => handlers_run,
        }
    }
}

/// Error type for Reactor operations.
#[derive(Debug, thiserror::Error)]
pub enum ReactorError {
    /// Handler registration failed.
    #[error("Failed to register handler: {0}")]
    Registration(#[from] DependencyError),

    /// Event emission failed.
    #[error("Event emission failed: {0}")]
    Emission(String),
}

/// Result type for Reactor operations.
pub type ReactorResult<T> = Result<T, ReactorError>;

/// The Reactor - central event dispatcher.
///
/// Owns all handlers and dispatches events through them in dependency + priority
/// order. This is the single event loop that all handlers (Rust, Rune, Lua)
/// register with.
///
/// ## Handler Ordering
///
/// Handlers are executed in topological order based on dependencies, with
/// priority as a tiebreaker for handlers at the same dependency level.
///
/// ## Async Execution
///
/// The `emit` method is async. Script handlers (Rune, Lua) should use
/// `tokio::task::spawn_blocking` internally to avoid blocking the reactor.
pub struct Reactor {
    /// Handlers stored by name.
    handlers: HashMap<String, SharedHandler>,
    /// Dependency graph for ordering.
    graph: DependencyGraph,
    /// Fail-open mode: continue on handler errors (default: true).
    fail_open: bool,
}

impl Default for Reactor {
    fn default() -> Self {
        Self::new()
    }
}

impl Reactor {
    /// Create a new empty Reactor.
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            graph: DependencyGraph::new(),
            fail_open: true, // Default to fail-open
        }
    }

    /// Create a Reactor with fail-closed mode.
    ///
    /// In fail-closed mode, any handler error stops processing immediately.
    pub fn fail_closed() -> Self {
        Self {
            handlers: HashMap::new(),
            graph: DependencyGraph::new(),
            fail_open: false,
        }
    }

    /// Set fail-open mode.
    pub fn set_fail_open(&mut self, fail_open: bool) {
        self.fail_open = fail_open;
    }

    /// Register a handler with the Reactor.
    ///
    /// The handler's name, dependencies, and priority are automatically
    /// extracted and added to the dependency graph.
    ///
    /// # Errors
    ///
    /// Returns an error if a handler with the same name already exists.
    pub fn register(&mut self, handler: BoxedHandler) -> ReactorResult<()> {
        let name = handler.name().to_string();
        let deps: Vec<String> = handler
            .dependencies()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let priority = handler.priority();

        // Add to dependency graph first
        self.graph.add_with_priority(&name, deps, priority)?;

        // Then store the handler
        self.handlers.insert(name, Arc::from(handler));

        Ok(())
    }

    /// Register a shared handler with the Reactor.
    pub fn register_shared(&mut self, handler: SharedHandler) -> ReactorResult<()> {
        let name = handler.name().to_string();
        let deps: Vec<String> = handler
            .dependencies()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let priority = handler.priority();

        // Add to dependency graph first
        self.graph.add_with_priority(&name, deps, priority)?;

        // Then store the handler
        self.handlers.insert(name, handler);

        Ok(())
    }

    /// Unregister a handler by name.
    ///
    /// # Errors
    ///
    /// Returns an error if the handler doesn't exist.
    pub fn unregister(&mut self, name: &str) -> ReactorResult<SharedHandler> {
        self.graph.remove(name)?;
        self.handlers
            .remove(name)
            .ok_or_else(|| ReactorError::Registration(DependencyError::HandlerNotFound(name.to_string())))
    }

    /// Check if a handler is registered.
    pub fn has_handler(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    /// Get a handler by name.
    pub fn get_handler(&self, name: &str) -> Option<&SharedHandler> {
        self.handlers.get(name)
    }

    /// Get all registered handler names.
    pub fn handler_names(&self) -> impl Iterator<Item = &str> {
        self.handlers.keys().map(|s| s.as_str())
    }

    /// Get the number of registered handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// Check if the Reactor has any handlers.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// Validate the handler dependency graph.
    ///
    /// # Errors
    ///
    /// Returns an error if there are cycles or missing dependencies.
    pub fn validate(&self) -> ReactorResult<()> {
        self.graph.validate_dependencies()?;
        Ok(())
    }

    /// Get the execution order of handlers.
    ///
    /// Returns handler names in the order they will be invoked.
    pub fn execution_order(&mut self) -> ReactorResult<Vec<String>> {
        Ok(self.graph.execution_order()?)
    }

    /// Emit an event through all matching handlers.
    ///
    /// Handlers are invoked in dependency + priority order. Only handlers
    /// whose event pattern matches the event type are invoked.
    ///
    /// ## Handler Results
    ///
    /// - `Continue(event)`: Pass event to next handler
    /// - `Cancel`: Stop processing, return `EmitResult::Cancelled`
    /// - `SoftError`: Log error, continue processing (in fail-open mode)
    /// - `FatalError`: Stop processing, return `EmitResult::Failed`
    pub async fn emit(&mut self, event: SessionEvent) -> ReactorResult<EmitResult> {
        let mut ctx = HandlerContext::new();
        let mut current_event = event;
        let event_type = current_event.event_type();

        // Get handlers in topological order
        let order = self.graph.execution_order()?;
        let mut handlers_run = Vec::new();

        for handler_name in order {
            let handler = match self.handlers.get(&handler_name) {
                Some(h) => h.clone(),
                None => continue,
            };

            // Check if handler matches this event type
            if !matches_event_pattern(handler.event_pattern(), event_type) {
                continue;
            }

            let timer = HandlerTimer::start(&handler_name);

            let result = handler.handle(&mut ctx, current_event.clone()).await;

            match result {
                HandlerResult::Continue(modified) => {
                    timer.stop(&mut ctx);
                    ctx.mark_completed(&handler_name);
                    handlers_run.push(handler_name.clone());
                    current_event = modified;
                }
                HandlerResult::Cancel => {
                    timer.stop_with_outcome(&mut ctx, "cancelled");
                    handlers_run.push(handler_name.clone());
                    return Ok(EmitResult::Cancelled {
                        by_handler: handler_name,
                        handlers_run,
                    });
                }
                HandlerResult::Cancelled(modified) => {
                    timer.stop_with_outcome(&mut ctx, "cancelled");
                    handlers_run.push(handler_name.clone());
                    // Cancelled with event preserved - we still return Cancelled
                    // but the event is available in the context if needed
                    let _ = modified; // Event discarded but was preserved
                    return Ok(EmitResult::Cancelled {
                        by_handler: handler_name,
                        handlers_run,
                    });
                }
                HandlerResult::SoftError { event, error } => {
                    timer.stop_with_outcome(&mut ctx, "soft_error");
                    tracing::warn!("Handler {} soft error: {}", handler_name, error);
                    ctx.mark_completed(&handler_name);
                    handlers_run.push(handler_name.clone());
                    current_event = event;
                    // Continue processing
                }
                HandlerResult::FatalError(error) => {
                    timer.stop_with_outcome(&mut ctx, "fatal_error");
                    handlers_run.push(handler_name.clone());

                    if self.fail_open {
                        tracing::error!("Handler {} fatal error (fail-open): {}", handler_name, error);
                        // Continue with the current event
                    } else {
                        return Ok(EmitResult::Failed {
                            handler: handler_name,
                            error: error.to_string(),
                            handlers_run,
                        });
                    }
                }
            }
        }

        // Process any emitted events
        let emitted_events = ctx.take_emitted();
        for emitted in emitted_events {
            // Recursively emit follow-up events
            // Note: In production, you might want to queue these instead
            Box::pin(self.emit(emitted)).await?;
        }

        Ok(EmitResult::Completed {
            event: current_event,
            handler_count: handlers_run.len(),
            handlers_run,
        })
    }

    /// Clear all handlers from the Reactor.
    pub fn clear(&mut self) {
        self.handlers.clear();
        self.graph.clear();
    }
}

impl std::fmt::Debug for Reactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reactor")
            .field("handler_count", &self.handlers.len())
            .field("handlers", &self.handlers.keys().collect::<Vec<_>>())
            .field("fail_open", &self.fail_open)
            .finish()
    }
}

// ============================================================================
// ReactorEventEmitter - Adapter for EventEmitter trait
// ============================================================================

use super::emitter::{EmitOutcome, EmitResult as EmitterResult, EventEmitter, HandlerErrorInfo};
use async_trait::async_trait;
use tokio::sync::RwLock;

/// Adapter that implements `EventEmitter` by wrapping a `Reactor`.
///
/// This allows components using the `EventEmitter` trait to seamlessly
/// work with the new `Reactor`-based event system.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_core::events::{Reactor, ReactorEventEmitter, EventEmitter, SessionEvent};
/// use std::sync::Arc;
/// use tokio::sync::RwLock;
///
/// // Create reactor and wrap it
/// let reactor = Arc::new(RwLock::new(Reactor::new()));
/// let emitter = ReactorEventEmitter::new(reactor);
///
/// // Use through EventEmitter trait
/// let outcome = emitter.emit(SessionEvent::Custom {
///     name: "test".into(),
///     payload: serde_json::json!({}),
/// }).await?;
/// ```
pub struct ReactorEventEmitter {
    reactor: Arc<RwLock<Reactor>>,
}

impl ReactorEventEmitter {
    /// Create a new emitter wrapping a shared Reactor.
    pub fn new(reactor: Arc<RwLock<Reactor>>) -> Self {
        Self { reactor }
    }

    /// Get a reference to the underlying reactor.
    pub fn reactor(&self) -> &Arc<RwLock<Reactor>> {
        &self.reactor
    }
}

impl Clone for ReactorEventEmitter {
    fn clone(&self) -> Self {
        Self {
            reactor: Arc::clone(&self.reactor),
        }
    }
}

impl std::fmt::Debug for ReactorEventEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReactorEventEmitter")
            .field("reactor", &"<Arc<RwLock<Reactor>>>")
            .finish()
    }
}

#[async_trait]
impl EventEmitter for ReactorEventEmitter {
    type Event = SessionEvent;

    async fn emit(&self, event: Self::Event) -> EmitterResult<EmitOutcome<Self::Event>> {
        let mut reactor = self.reactor.write().await;
        let result = reactor.emit(event.clone()).await;

        match result {
            Ok(emit_result) => match emit_result {
                EmitResult::Completed { event, .. } => Ok(EmitOutcome::new(event)),
                EmitResult::Cancelled { by_handler, .. } => {
                    // Return the original event as cancelled
                    let mut outcome = EmitOutcome::cancelled(event);
                    outcome.errors.push(HandlerErrorInfo::new(
                        by_handler,
                        "Event cancelled by handler",
                    ));
                    Ok(outcome)
                }
                EmitResult::Failed {
                    handler, error, ..
                } => {
                    // Return the original event with fatal error
                    Ok(EmitOutcome::with_errors(
                        event,
                        vec![HandlerErrorInfo::fatal(handler, error)],
                    ))
                }
            },
            Err(e) => {
                // Reactor error - return original event with error
                Ok(EmitOutcome::with_errors(
                    event,
                    vec![HandlerErrorInfo::fatal("reactor", e.to_string())],
                ))
            }
        }
    }

    async fn emit_recursive(
        &self,
        event: Self::Event,
    ) -> EmitterResult<Vec<EmitOutcome<Self::Event>>> {
        // The Reactor already handles recursive emission via HandlerContext::emit()
        let outcome = self.emit(event).await?;
        Ok(vec![outcome])
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    /// Simple test handler.
    struct TestHandler {
        name: &'static str,
        deps: Vec<&'static str>,
        priority: i32,
        pattern: &'static str,
    }

    impl TestHandler {
        fn new(name: &'static str) -> Self {
            Self {
                name,
                deps: vec![],
                priority: 50,
                pattern: "*",
            }
        }

        fn with_deps(mut self, deps: Vec<&'static str>) -> Self {
            self.deps = deps;
            self
        }

        fn with_priority(mut self, priority: i32) -> Self {
            self.priority = priority;
            self
        }

        fn with_pattern(mut self, pattern: &'static str) -> Self {
            self.pattern = pattern;
            self
        }
    }

    #[async_trait]
    impl Handler for TestHandler {
        fn name(&self) -> &str {
            self.name
        }

        fn dependencies(&self) -> &[&str] {
            &self.deps
        }

        fn priority(&self) -> i32 {
            self.priority
        }

        fn event_pattern(&self) -> &str {
            self.pattern
        }

        async fn handle(
            &self,
            _ctx: &mut HandlerContext,
            event: SessionEvent,
        ) -> HandlerResult<SessionEvent> {
            HandlerResult::ok(event)
        }
    }

    /// Handler that cancels events.
    struct CancellingHandler;

    #[async_trait]
    impl Handler for CancellingHandler {
        fn name(&self) -> &str {
            "canceller"
        }

        async fn handle(
            &self,
            _ctx: &mut HandlerContext,
            _event: SessionEvent,
        ) -> HandlerResult<SessionEvent> {
            HandlerResult::cancel()
        }
    }

    #[tokio::test]
    async fn test_empty_reactor() {
        let mut reactor = Reactor::new();
        assert!(reactor.is_empty());

        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        };

        let result = reactor.emit(event).await.unwrap();
        assert!(result.is_completed());
        assert_eq!(result.handlers_run().len(), 0);
    }

    #[tokio::test]
    async fn test_single_handler() {
        let mut reactor = Reactor::new();
        reactor.register(Box::new(TestHandler::new("test"))).unwrap();

        assert!(!reactor.is_empty());
        assert_eq!(reactor.handler_count(), 1);
        assert!(reactor.has_handler("test"));

        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        };

        let result = reactor.emit(event).await.unwrap();
        assert!(result.is_completed());
        assert_eq!(result.handlers_run(), &["test"]);
    }

    #[tokio::test]
    async fn test_handler_ordering() {
        let mut reactor = Reactor::new();

        // Register in reverse order
        reactor
            .register(Box::new(TestHandler::new("C").with_deps(vec!["B"])))
            .unwrap();
        reactor
            .register(Box::new(TestHandler::new("B").with_deps(vec!["A"])))
            .unwrap();
        reactor.register(Box::new(TestHandler::new("A"))).unwrap();

        let order = reactor.execution_order().unwrap();
        assert_eq!(order, vec!["A", "B", "C"]);
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        let mut reactor = Reactor::new();

        // Register independent handlers with different priorities
        reactor
            .register(Box::new(TestHandler::new("A").with_priority(50)))
            .unwrap();
        reactor
            .register(Box::new(TestHandler::new("B").with_priority(10))) // Lowest = first
            .unwrap();
        reactor
            .register(Box::new(TestHandler::new("C").with_priority(30)))
            .unwrap();

        let order = reactor.execution_order().unwrap();
        assert_eq!(order, vec!["B", "C", "A"]);
    }

    #[tokio::test]
    async fn test_event_pattern_filtering() {
        let mut reactor = Reactor::new();

        reactor
            .register(Box::new(TestHandler::new("tool_handler").with_pattern("tool:*")))
            .unwrap();
        reactor
            .register(Box::new(TestHandler::new("note_handler").with_pattern("note:*")))
            .unwrap();

        // Emit a tool event
        let event = SessionEvent::ToolCalled {
            name: "search".into(),
            args: serde_json::json!({}),
        };

        let result = reactor.emit(event).await.unwrap();
        assert!(result.is_completed());
        // Only tool_handler should run
        assert_eq!(result.handlers_run(), &["tool_handler"]);
    }

    #[tokio::test]
    async fn test_cancellation() {
        let mut reactor = Reactor::new();

        reactor
            .register(Box::new(TestHandler::new("first").with_priority(10)))
            .unwrap();
        reactor.register(Box::new(CancellingHandler)).unwrap();
        reactor
            .register(Box::new(TestHandler::new("last").with_priority(90)))
            .unwrap();

        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        };

        let result = reactor.emit(event).await.unwrap();
        assert!(result.is_cancelled());
        if let EmitResult::Cancelled { by_handler, handlers_run } = result {
            assert_eq!(by_handler, "canceller");
            // "first" ran before cancellation
            assert!(handlers_run.contains(&"first".to_string()));
            // "last" should NOT have run
            assert!(!handlers_run.contains(&"last".to_string()));
        }
    }

    #[tokio::test]
    async fn test_unregister() {
        let mut reactor = Reactor::new();

        reactor.register(Box::new(TestHandler::new("test"))).unwrap();
        assert!(reactor.has_handler("test"));

        reactor.unregister("test").unwrap();
        assert!(!reactor.has_handler("test"));
    }

    #[tokio::test]
    async fn test_duplicate_handler_error() {
        let mut reactor = Reactor::new();

        reactor.register(Box::new(TestHandler::new("test"))).unwrap();
        let result = reactor.register(Box::new(TestHandler::new("test")));

        assert!(result.is_err());
    }

    #[test]
    fn test_reactor_debug() {
        let mut reactor = Reactor::new();
        reactor.register(Box::new(TestHandler::new("test"))).unwrap();

        let debug = format!("{:?}", reactor);
        assert!(debug.contains("Reactor"));
        assert!(debug.contains("handler_count: 1"));
    }

    // ========================================================================
    // ReactorEventEmitter tests
    // ========================================================================

    #[tokio::test]
    async fn test_emitter_emit_completed() {
        let mut reactor = Reactor::new();
        reactor.register(Box::new(TestHandler::new("test"))).unwrap();

        let reactor = Arc::new(RwLock::new(reactor));
        let emitter = ReactorEventEmitter::new(reactor);

        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        };

        let outcome = emitter.emit(event).await.unwrap();
        assert!(!outcome.cancelled);
        assert!(!outcome.has_errors());
    }

    #[tokio::test]
    async fn test_emitter_emit_cancelled() {
        let mut reactor = Reactor::new();
        reactor.register(Box::new(CancellingHandler)).unwrap();

        let reactor = Arc::new(RwLock::new(reactor));
        let emitter = ReactorEventEmitter::new(reactor);

        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        };

        let outcome = emitter.emit(event).await.unwrap();
        assert!(outcome.cancelled);
        assert!(outcome.has_errors());
        assert_eq!(outcome.errors[0].handler_name, "canceller");
    }

    #[tokio::test]
    async fn test_emitter_clone() {
        let reactor = Arc::new(RwLock::new(Reactor::new()));
        let emitter1 = ReactorEventEmitter::new(reactor);
        let emitter2 = emitter1.clone();

        // Both should point to same reactor
        assert!(emitter1.is_available());
        assert!(emitter2.is_available());
    }

    #[test]
    fn test_emitter_debug() {
        let reactor = Arc::new(RwLock::new(Reactor::new()));
        let emitter = ReactorEventEmitter::new(reactor);

        let debug = format!("{:?}", emitter);
        assert!(debug.contains("ReactorEventEmitter"));
    }

    #[tokio::test]
    async fn test_emitter_emit_recursive() {
        let reactor = Arc::new(RwLock::new(Reactor::new()));
        let emitter = ReactorEventEmitter::new(reactor);

        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        };

        let outcomes = emitter.emit_recursive(event).await.unwrap();
        assert_eq!(outcomes.len(), 1);
        assert!(!outcomes[0].cancelled);
    }
}
