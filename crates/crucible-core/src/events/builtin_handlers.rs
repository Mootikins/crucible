//! Built-in handlers for common event processing patterns.
//!
//! This module provides reusable handlers that can be registered with the Reactor.
//! These serve as examples and building blocks for custom handlers.
//!
//! # Available Handlers
//!
//! - [`LoggingHandler`]: Logs all events (debug level)
//! - [`MetricsHandler`]: Collects event processing metrics
//! - [`FilterHandler`]: Conditionally cancels events
//!
//! # Example
//!
//! ```rust,ignore
//! use crucible_core::events::{Reactor, LoggingHandler, MetricsHandler};
//!
//! let mut reactor = Reactor::new();
//!
//! // Add logging first (low priority)
//! reactor.register(Box::new(LoggingHandler::new()))?;
//!
//! // Add metrics collection
//! reactor.register(Box::new(MetricsHandler::new()))?;
//! ```

use super::handler::{Handler, HandlerContext, HandlerResult};
use super::session_event::SessionEvent;
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ============================================================================
// LoggingHandler - Logs all events
// ============================================================================

/// Handler that logs all events passing through the reactor.
///
/// Useful for debugging and audit trails. Runs at low priority (10)
/// to capture events before other handlers modify them.
///
/// # Example
///
/// ```rust,ignore
/// let handler = LoggingHandler::new()
///     .with_level(tracing::Level::INFO);
/// reactor.register(Box::new(handler))?;
/// ```
pub struct LoggingHandler {
    name: String,
    priority: i32,
    pattern: String,
}

impl Default for LoggingHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl LoggingHandler {
    /// Create a new logging handler with default settings.
    pub fn new() -> Self {
        Self {
            name: "builtin:logging".to_string(),
            priority: 10, // Run early
            pattern: "*".to_string(),
        }
    }

    /// Set a custom name for this handler.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the priority for this handler.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set the event pattern to match.
    pub fn with_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.pattern = pattern.into();
        self
    }
}

#[async_trait]
impl Handler for LoggingHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    fn event_pattern(&self) -> &str {
        &self.pattern
    }

    async fn handle(
        &self,
        _ctx: &mut HandlerContext,
        event: SessionEvent,
    ) -> HandlerResult<SessionEvent> {
        tracing::debug!(
            event_type = %event.event_type(),
            handler = %self.name,
            "Event received"
        );
        HandlerResult::ok(event)
    }
}

// ============================================================================
// MetricsHandler - Collects event metrics
// ============================================================================

/// Handler that collects metrics about event processing.
///
/// Tracks:
/// - Total events processed
/// - Events by type
///
/// Runs at low priority (5) to count events before processing.
pub struct MetricsHandler {
    name: String,
    priority: i32,
    pattern: String,
    total_events: Arc<AtomicU64>,
}

impl Default for MetricsHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsHandler {
    /// Create a new metrics handler.
    pub fn new() -> Self {
        Self {
            name: "builtin:metrics".to_string(),
            priority: 5, // Run very early
            pattern: "*".to_string(),
            total_events: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Set a custom name for this handler.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Get the total number of events processed.
    pub fn total_events(&self) -> u64 {
        self.total_events.load(Ordering::Relaxed)
    }

    /// Get a clone of the counter for external monitoring.
    pub fn counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.total_events)
    }
}

#[async_trait]
impl Handler for MetricsHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    fn event_pattern(&self) -> &str {
        &self.pattern
    }

    async fn handle(
        &self,
        _ctx: &mut HandlerContext,
        event: SessionEvent,
    ) -> HandlerResult<SessionEvent> {
        self.total_events.fetch_add(1, Ordering::Relaxed);
        HandlerResult::ok(event)
    }
}

// ============================================================================
// FilterHandler - Conditionally filters events
// ============================================================================

/// Handler that filters (cancels) events based on a predicate.
///
/// # Example
///
/// ```rust,ignore
/// // Cancel all tool events for "dangerous" tools
/// let filter = FilterHandler::new("security:block_dangerous", |event| {
///     if let SessionEvent::ToolCalled { name, .. } = event {
///         name == "dangerous_tool"
///     } else {
///         false
///     }
/// });
/// reactor.register(Box::new(filter))?;
/// ```
pub struct FilterHandler<F>
where
    F: Fn(&SessionEvent) -> bool + Send + Sync,
{
    name: String,
    priority: i32,
    pattern: String,
    should_cancel: F,
}

impl<F> FilterHandler<F>
where
    F: Fn(&SessionEvent) -> bool + Send + Sync,
{
    /// Create a new filter handler.
    ///
    /// The predicate returns `true` if the event should be cancelled.
    pub fn new(name: impl Into<String>, should_cancel: F) -> Self {
        Self {
            name: name.into(),
            priority: 20, // Run early but after logging/metrics
            pattern: "*".to_string(),
            should_cancel,
        }
    }

    /// Set the priority for this handler.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set the event pattern to match.
    pub fn with_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.pattern = pattern.into();
        self
    }
}

#[async_trait]
impl<F> Handler for FilterHandler<F>
where
    F: Fn(&SessionEvent) -> bool + Send + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    fn event_pattern(&self) -> &str {
        &self.pattern
    }

    async fn handle(
        &self,
        _ctx: &mut HandlerContext,
        event: SessionEvent,
    ) -> HandlerResult<SessionEvent> {
        if (self.should_cancel)(&event) {
            tracing::info!(
                handler = %self.name,
                event_type = %event.event_type(),
                "Event cancelled by filter"
            );
            HandlerResult::cancel()
        } else {
            HandlerResult::ok(event)
        }
    }
}

// ============================================================================
// AsyncCallbackHandler - Executes async callback on events
// ============================================================================

use std::future::Future;
use std::pin::Pin;

/// Type alias for async callback functions.
pub type AsyncCallback = Box<
    dyn Fn(SessionEvent) -> Pin<Box<dyn Future<Output = HandlerResult<SessionEvent>> + Send>>
        + Send
        + Sync,
>;

/// Handler that executes an async callback for each matching event.
///
/// This is the most flexible built-in handler, allowing arbitrary async
/// processing while integrating with the reactor's dependency system.
///
/// # Example
///
/// ```rust,ignore
/// let handler = AsyncCallbackHandler::new("storage:persist", |event| {
///     Box::pin(async move {
///         // Do async work...
///         save_to_database(&event).await?;
///         HandlerResult::ok(event)
///     })
/// });
/// reactor.register(Box::new(handler))?;
/// ```
pub struct AsyncCallbackHandler {
    name: String,
    deps: Vec<&'static str>,
    priority: i32,
    pattern: String,
    callback: AsyncCallback,
}

impl AsyncCallbackHandler {
    /// Create a new async callback handler.
    pub fn new<F, Fut>(name: impl Into<String>, callback: F) -> Self
    where
        F: Fn(SessionEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HandlerResult<SessionEvent>> + Send + 'static,
    {
        Self {
            name: name.into(),
            deps: vec![],
            priority: 50,
            pattern: "*".to_string(),
            callback: Box::new(move |event| Box::pin(callback(event))),
        }
    }

    /// Set dependencies for this handler.
    pub fn with_dependencies(mut self, deps: Vec<&'static str>) -> Self {
        self.deps = deps;
        self
    }

    /// Set the priority for this handler.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set the event pattern to match.
    pub fn with_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.pattern = pattern.into();
        self
    }
}

#[async_trait]
impl Handler for AsyncCallbackHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn dependencies(&self) -> &[&str] {
        &self.deps
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    fn event_pattern(&self) -> &str {
        &self.pattern
    }

    async fn handle(
        &self,
        _ctx: &mut HandlerContext,
        event: SessionEvent,
    ) -> HandlerResult<SessionEvent> {
        (self.callback)(event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Reactor;

    #[tokio::test]
    async fn test_logging_handler() {
        let mut reactor = Reactor::new();
        reactor
            .register(Box::new(LoggingHandler::new()))
            .unwrap();

        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        };

        let result = reactor.emit(event).await.unwrap();
        assert!(result.is_completed());
    }

    #[tokio::test]
    async fn test_metrics_handler() {
        let metrics = MetricsHandler::new();
        let counter = metrics.counter();

        let mut reactor = Reactor::new();
        reactor.register(Box::new(metrics)).unwrap();

        assert_eq!(counter.load(Ordering::Relaxed), 0);

        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        };
        reactor.emit(event).await.unwrap();

        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_filter_handler_pass() {
        let filter = FilterHandler::new("test_filter", |_| false);

        let mut reactor = Reactor::new();
        reactor.register(Box::new(filter)).unwrap();

        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        };

        let result = reactor.emit(event).await.unwrap();
        assert!(result.is_completed());
    }

    #[tokio::test]
    async fn test_filter_handler_cancel() {
        let filter = FilterHandler::new("test_filter", |event| {
            matches!(event, SessionEvent::Custom { name, .. } if name == "blocked")
        });

        let mut reactor = Reactor::new();
        reactor.register(Box::new(filter)).unwrap();

        // This should pass
        let event = SessionEvent::Custom {
            name: "allowed".into(),
            payload: serde_json::json!({}),
        };
        let result = reactor.emit(event).await.unwrap();
        assert!(result.is_completed());

        // This should be cancelled
        let event = SessionEvent::Custom {
            name: "blocked".into(),
            payload: serde_json::json!({}),
        };
        let result = reactor.emit(event).await.unwrap();
        assert!(result.is_cancelled());
    }

    #[tokio::test]
    async fn test_async_callback_handler() {
        let handler = AsyncCallbackHandler::new("test_callback", |event| async move {
            // Simulate async work
            tokio::task::yield_now().await;
            HandlerResult::ok(event)
        });

        let mut reactor = Reactor::new();
        reactor.register(Box::new(handler)).unwrap();

        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        };

        let result = reactor.emit(event).await.unwrap();
        assert!(result.is_completed());
    }

    #[tokio::test]
    async fn test_handler_chaining() {
        let metrics = MetricsHandler::new();
        let counter = metrics.counter();

        let mut reactor = Reactor::new();
        reactor.register(Box::new(metrics)).unwrap();
        reactor.register(Box::new(LoggingHandler::new())).unwrap();
        reactor
            .register(Box::new(FilterHandler::new("pass_all", |_| false)))
            .unwrap();

        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        };

        let result = reactor.emit(event).await.unwrap();
        assert!(result.is_completed());
        assert_eq!(result.handlers_run().len(), 3);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_logging_handler_builder() {
        let handler = LoggingHandler::new()
            .with_name("custom:logger")
            .with_priority(100)
            .with_pattern("tool:*");

        assert_eq!(handler.name(), "custom:logger");
        assert_eq!(handler.priority(), 100);
        assert_eq!(handler.event_pattern(), "tool:*");
    }
}
