//! Event system for Crucible.
//!
//! This module provides the core event types and traits for the event-driven
//! architecture following the [Reactor pattern](https://en.wikipedia.org/wiki/Reactor_pattern).
//!
//! # Architecture
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
//! # Key Components
//!
//! - [`Handler`]: Async trait for event handlers (Rust, Rune, Lua)
//! - [`HandlerContext`]: Context passed through handler chain
//! - [`Reactor`]: Central event loop + dispatcher
//! - [`DependencyGraph`]: Handler ordering via topological sort
//! - [`SessionEvent`]: Canonical event type
//! - [`ReactorEventEmitter`]: Adapter for `EventEmitter` trait
//!
//! # Built-in Handlers
//!
//! The [`builtin_handlers`] module provides ready-to-use handlers:
//!
//! - [`LoggingHandler`]: Logs all events (priority 10)
//! - [`MetricsHandler`]: Collects event metrics (priority 5)
//! - [`FilterHandler`]: Conditionally cancels events
//! - [`AsyncCallbackHandler`]: Executes async callbacks
//!
//! # Handler Results
//!
//! The [`handler`] module provides `HandlerResult<E>` for controlling event flow:
//!
//! - `Continue(event)` - Processing succeeded, pass to next handler
//! - `Cancel` - Stop processing, event is discarded
//! - `Cancelled(event)` - Stop processing, event is preserved for inspection
//! - `SoftError { event, error }` - Non-fatal error, continue with event
//! - `FatalError(error)` - Fatal error, stop processing immediately
//!
//! # Example
//!
//! ```rust,ignore
//! use crucible_core::events::{Handler, HandlerContext, HandlerResult, Reactor, SessionEvent};
//! use async_trait::async_trait;
//!
//! struct LoggingHandler;
//!
//! #[async_trait]
//! impl Handler for LoggingHandler {
//!     fn name(&self) -> &str { "logging" }
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
//!
//! // Register with Reactor
//! let mut reactor = Reactor::new();
//! reactor.register(Box::new(LoggingHandler))?;
//!
//! // Emit event
//! let result = reactor.emit(event).await?;
//! ```
//!
//! # Using with EventEmitter Trait
//!
//! If you have code that expects an `EventEmitter`, use `ReactorEventEmitter`:
//!
//! ```rust,ignore
//! use crucible_core::events::{Reactor, ReactorEventEmitter, EventEmitter};
//! use std::sync::Arc;
//! use tokio::sync::RwLock;
//!
//! let reactor = Arc::new(RwLock::new(Reactor::new()));
//! let emitter = ReactorEventEmitter::new(reactor);
//!
//! // Now use via EventEmitter trait
//! let outcome = emitter.emit(event).await?;
//! ```
//!
//! # Script Handlers
//!
//! For Rune scripts, use `crucible_rune::RuneHandler`.
//! For Lua scripts, use `crucible_lua::LuaHandler`.
//!
//! Both implement the `Handler` trait and use `spawn_blocking` for async execution.
//!
//! # Legacy EventBus
//!
//! The `emitter` and `subscriber` modules provide the older EventBus pattern.
//! The `crucible_rune::event_bus` module is transitional and being migrated
//! to use the Reactor pattern. New code should use `Reactor` directly.

pub mod builtin_handlers;
pub mod dependency;
pub mod emitter;
pub mod handler;
pub mod reactor;
pub mod session_event;
pub mod subscriber;

// Re-exports for convenient access

// New unified Handler system
pub use handler::{
    BoxedHandler, Handler, HandlerContext, HandlerResult, HandlerTimer, HandlerTraceEntry,
    SharedHandler, matches_event_pattern,
};

// Dependency graph for handler ordering
pub use dependency::{DependencyError, DependencyGraph, DependencyResult, GraphNode};

// Reactor (central event loop)
pub use reactor::{
    EmitResult as ReactorEmitResult, Reactor, ReactorError, ReactorEventEmitter, ReactorResult,
};

// Built-in handlers
pub use builtin_handlers::{
    AsyncCallbackHandler, FilterHandler, LoggingHandler, MetricsHandler,
};

// Legacy emitter exports
pub use emitter::{
    EmitOutcome, EmitResult, EventEmitter, EventError, HandlerErrorInfo, NoOpEmitter,
    SharedEventBus,
};

// Legacy subscriber exports
pub use subscriber::{
    box_handler, BoxedHandlerFn, EventBus, EventFilter, EventSubscriber, HandlerFuture,
    SubscriptionError, SubscriptionId, SubscriptionIdGenerator, SubscriptionInfo,
    SubscriptionResult,
};

// Session event types
pub use session_event::{
    EntityType, FileChangeKind, NoteChangeType, NotePayload, Priority, SessionEvent,
    SessionEventConfig, ToolCall, ToolSource,
};
