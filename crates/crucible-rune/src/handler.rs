//! Handler trait for the event ring buffer system.
//!
//! This module defines the `RingHandler<E>` trait that enables topo-sorted event
//! processing in the Disruptor-style ring buffer architecture.
//!
//! ## Design
//!
//! Ring handlers:
//! - Receive `Arc<E>` references (cheap clone, no deep copy)
//! - Process events asynchronously
//! - Can emit new events without copying the original
//! - Declare dependencies for topological ordering
//!
//! ## Naming
//!
//! Named `RingHandler` to distinguish from `event_bus::Handler` which is a
//! synchronous closure-based handler for the pub/sub EventBus.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::handler::{RingHandler, RingHandlerContext};
//! use async_trait::async_trait;
//! use std::sync::Arc;
//!
//! struct LoggingHandler;
//!
//! #[async_trait]
//! impl RingHandler<MyEvent> for LoggingHandler {
//!     fn name(&self) -> &str { "logging" }
//!
//!     async fn handle(
//!         &self,
//!         ctx: &mut RingHandlerContext<MyEvent>,
//!         event: Arc<MyEvent>,
//!         seq: u64,
//!     ) -> RingHandlerResult<()> {
//!         tracing::info!("Event {}: {:?}", seq, event);
//!         Ok(())
//!     }
//! }
//! ```

use async_trait::async_trait;
use std::sync::Arc;

/// Context passed to ring handlers during event processing.
///
/// Provides:
/// - Event emission (handlers can produce new events)
/// - Cross-handler metadata passing
/// - Cancellation signaling
#[derive(Debug)]
pub struct RingHandlerContext<E> {
    /// Events emitted by this handler
    emitted: Vec<E>,
    /// Handler-local metadata (JSON values)
    metadata: serde_json::Map<String, serde_json::Value>,
    /// Whether processing should stop after this handler
    cancelled: bool,
}

impl<E> Default for RingHandlerContext<E> {
    fn default() -> Self {
        Self {
            emitted: Vec::new(),
            metadata: serde_json::Map::new(),
            cancelled: false,
        }
    }
}

impl<E> RingHandlerContext<E> {
    /// Create a new empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Emit a new event from this handler.
    ///
    /// Emitted events are collected and pushed to the ring buffer after
    /// all handlers have processed the current event.
    pub fn emit(&mut self, event: E) {
        self.emitted.push(event);
    }

    /// Take all emitted events, leaving the context empty.
    pub fn take_emitted(&mut self) -> Vec<E> {
        std::mem::take(&mut self.emitted)
    }

    /// Get the number of emitted events.
    pub fn emitted_count(&self) -> usize {
        self.emitted.len()
    }

    /// Set metadata value.
    pub fn set_metadata(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.metadata.insert(key.into(), value);
    }

    /// Get metadata value.
    pub fn get_metadata(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }

    /// Remove and return metadata value.
    pub fn remove_metadata(&mut self, key: &str) -> Option<serde_json::Value> {
        self.metadata.remove(key)
    }

    /// Cancel further processing.
    ///
    /// When called, handlers after this one in the topo-sort order
    /// will not be invoked for the current event.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Check if processing has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    /// Reset the context for reuse.
    pub fn reset(&mut self) {
        self.emitted.clear();
        self.metadata.clear();
        self.cancelled = false;
    }
}

/// Result type for ring handler operations.
pub type RingHandlerResult<T> = Result<T, RingHandlerError>;

/// Errors that can occur during ring handler execution.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RingHandlerError {
    /// Handler failed but processing should continue (fail-open).
    #[error("Handler '{handler}' failed: {message}")]
    NonFatal { handler: String, message: String },

    /// Handler failed and processing should stop.
    #[error("Handler '{handler}' fatal error: {message}")]
    Fatal { handler: String, message: String },
}

impl RingHandlerError {
    /// Create a non-fatal error (fail-open semantics).
    pub fn non_fatal(handler: impl Into<String>, message: impl Into<String>) -> Self {
        Self::NonFatal {
            handler: handler.into(),
            message: message.into(),
        }
    }

    /// Create a fatal error that stops the pipeline.
    pub fn fatal(handler: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Fatal {
            handler: handler.into(),
            message: message.into(),
        }
    }

    /// Check if this error is fatal.
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::Fatal { .. })
    }

    /// Get the handler name.
    pub fn handler_name(&self) -> &str {
        match self {
            Self::NonFatal { handler, .. } | Self::Fatal { handler, .. } => handler,
        }
    }
}

/// A handler in the ring buffer event processing pipeline.
///
/// Ring handlers are invoked in topological order based on their declared
/// dependencies. Each handler receives an `Arc<E>` reference to the event,
/// avoiding deep copies for performance.
///
/// ## Dependency Model
///
/// Handlers declare dependencies via `depends_on()`. The reactor ensures:
/// - A handler runs only after all its dependencies have processed the event
/// - Circular dependencies are detected at registration time
///
/// ## Thread Safety
///
/// Handlers must be `Send + Sync` to support concurrent invocation across
/// multiple events. State should be externalized or protected with appropriate
/// synchronization primitives.
#[async_trait]
pub trait RingHandler<E>: Send + Sync {
    /// Unique name for this handler.
    ///
    /// Used for:
    /// - Dependency declarations
    /// - Logging and diagnostics
    /// - Handler lookup
    fn name(&self) -> &str;

    /// Handler dependencies (names of other handlers that must run first).
    ///
    /// Returns an empty slice by default (no dependencies).
    fn depends_on(&self) -> &[&str] {
        &[]
    }

    /// Process an event.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Context for emitting events and passing metadata
    /// * `event` - Arc reference to the event (cheap clone)
    /// * `seq` - Sequence number in the ring buffer
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Handler completed successfully
    /// * `Err(RingHandlerError::NonFatal)` - Handler failed, continue with next
    /// * `Err(RingHandlerError::Fatal)` - Handler failed, stop processing
    async fn handle(
        &self,
        ctx: &mut RingHandlerContext<E>,
        event: Arc<E>,
        seq: u64,
    ) -> RingHandlerResult<()>;

    /// Called when the handler is first registered.
    ///
    /// Use for one-time initialization. Default implementation does nothing.
    async fn on_register(&self) -> RingHandlerResult<()> {
        Ok(())
    }

    /// Called when the handler is being unregistered.
    ///
    /// Use for cleanup. Default implementation does nothing.
    async fn on_unregister(&self) -> RingHandlerResult<()> {
        Ok(())
    }
}

/// Boxed ring handler for type erasure.
///
/// Useful when storing heterogeneous handlers in collections.
pub type BoxedRingHandler<E> = Box<dyn RingHandler<E>>;

/// Ring handler metadata for introspection.
#[derive(Debug, Clone)]
pub struct RingHandlerInfo {
    /// Handler name
    pub name: String,
    /// Dependencies
    pub depends_on: Vec<String>,
}

impl RingHandlerInfo {
    /// Create handler info from a ring handler.
    pub fn from_handler<E>(handler: &dyn RingHandler<E>) -> Self {
        Self {
            name: handler.name().to_string(),
            depends_on: handler.depends_on().iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHandler {
        name: &'static str,
        deps: &'static [&'static str],
    }

    #[async_trait]
    impl RingHandler<String> for TestHandler {
        fn name(&self) -> &str {
            self.name
        }

        fn depends_on(&self) -> &[&str] {
            self.deps
        }

        async fn handle(
            &self,
            ctx: &mut RingHandlerContext<String>,
            event: Arc<String>,
            seq: u64,
        ) -> RingHandlerResult<()> {
            ctx.emit(format!("{}:{}:{}", self.name, seq, event));
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_ring_handler_context_emit() {
        let mut ctx = RingHandlerContext::<String>::new();

        ctx.emit("event1".to_string());
        ctx.emit("event2".to_string());

        assert_eq!(ctx.emitted_count(), 2);

        let emitted = ctx.take_emitted();
        assert_eq!(emitted, vec!["event1", "event2"]);
        assert_eq!(ctx.emitted_count(), 0);
    }

    #[tokio::test]
    async fn test_ring_handler_context_metadata() {
        let mut ctx = RingHandlerContext::<String>::new();

        ctx.set_metadata("key1", serde_json::json!("value1"));
        ctx.set_metadata("key2", serde_json::json!(42));

        assert_eq!(ctx.get_metadata("key1"), Some(&serde_json::json!("value1")));
        assert_eq!(ctx.get_metadata("key2"), Some(&serde_json::json!(42)));
        assert_eq!(ctx.get_metadata("missing"), None);

        let removed = ctx.remove_metadata("key1");
        assert_eq!(removed, Some(serde_json::json!("value1")));
        assert_eq!(ctx.get_metadata("key1"), None);
    }

    #[tokio::test]
    async fn test_ring_handler_context_cancel() {
        let mut ctx = RingHandlerContext::<String>::new();

        assert!(!ctx.is_cancelled());
        ctx.cancel();
        assert!(ctx.is_cancelled());
    }

    #[tokio::test]
    async fn test_ring_handler_context_reset() {
        let mut ctx = RingHandlerContext::<String>::new();

        ctx.emit("event".to_string());
        ctx.set_metadata("key", serde_json::json!("value"));
        ctx.cancel();

        ctx.reset();

        assert_eq!(ctx.emitted_count(), 0);
        assert_eq!(ctx.get_metadata("key"), None);
        assert!(!ctx.is_cancelled());
    }

    #[tokio::test]
    async fn test_ring_handler_trait() {
        let handler = TestHandler {
            name: "test",
            deps: &["dep1", "dep2"],
        };

        assert_eq!(handler.name(), "test");
        assert_eq!(handler.depends_on(), &["dep1", "dep2"]);

        let mut ctx = RingHandlerContext::new();
        let event = Arc::new("hello".to_string());

        handler.handle(&mut ctx, event, 42).await.unwrap();

        let emitted = ctx.take_emitted();
        assert_eq!(emitted, vec!["test:42:hello"]);
    }

    #[tokio::test]
    async fn test_ring_handler_no_deps() {
        struct NoDepsHandler;

        #[async_trait]
        impl RingHandler<()> for NoDepsHandler {
            fn name(&self) -> &str {
                "no_deps"
            }

            async fn handle(
                &self,
                _ctx: &mut RingHandlerContext<()>,
                _event: Arc<()>,
                _seq: u64,
            ) -> RingHandlerResult<()> {
                Ok(())
            }
        }

        let handler = NoDepsHandler;
        assert!(handler.depends_on().is_empty());
    }

    #[test]
    fn test_ring_handler_error() {
        let non_fatal = RingHandlerError::non_fatal("handler1", "something went wrong");
        assert!(!non_fatal.is_fatal());
        assert_eq!(non_fatal.handler_name(), "handler1");

        let fatal = RingHandlerError::fatal("handler2", "critical failure");
        assert!(fatal.is_fatal());
        assert_eq!(fatal.handler_name(), "handler2");
    }

    #[test]
    fn test_ring_handler_info() {
        let handler = TestHandler {
            name: "info_test",
            deps: &["a", "b"],
        };

        let info = RingHandlerInfo::from_handler(&handler);
        assert_eq!(info.name, "info_test");
        assert_eq!(info.depends_on, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn test_boxed_ring_handler() {
        let handler: BoxedRingHandler<String> = Box::new(TestHandler {
            name: "boxed",
            deps: &[],
        });

        assert_eq!(handler.name(), "boxed");

        let mut ctx = RingHandlerContext::new();
        let event = Arc::new("test".to_string());

        handler.handle(&mut ctx, event, 0).await.unwrap();
        assert_eq!(ctx.take_emitted(), vec!["boxed:0:test"]);
    }
}
