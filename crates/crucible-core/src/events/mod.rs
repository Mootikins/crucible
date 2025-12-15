//! Event system for Crucible.
//!
//! This module provides the core event types and traits for the event-driven
//! architecture. Components emit and consume events through a shared EventBus,
//! enabling loose coupling between subsystems.
//!
//! # Event Flow
//!
//! ```text
//! FileChanged -> NoteParsed -> EntityStored -> BlocksUpdated -> EmbeddingRequested -> EmbeddingGenerated
//!      ^            ^              ^               ^                  ^                     ^
//!    Watch       Parser         Storage         Storage           Embedding            Embedding
//! ```
//!
//! # Module Structure
//!
//! - [`emitter`]: The `EventEmitter` trait for emitting events
//! - [`subscriber`]: The `EventSubscriber` trait for subscribing to events
//! - [`handler`]: Handler result types for event processing
//! - [`session_event`]: The `SessionEvent` enum with all event variants (future)
//!
//! # Handler Results
//!
//! The [`handler`] module provides `HandlerResult<E>` for controlling event flow:
//!
//! - `Continue(event)` - Processing succeeded, pass to next handler
//! - `Cancel` - Stop processing, event is cancelled
//! - `SoftError { event, error }` - Non-fatal error, continue with event
//! - `FatalError(error)` - Fatal error, stop processing immediately
//!
//! # Example
//!
//! ```ignore
//! use crucible_core::events::{EventEmitter, EventSubscriber, EmitResult, EmitOutcome, HandlerResult};
//!
//! // Emit a file change event
//! let outcome = emitter.emit(SessionEvent::NoteModified {
//!     path: "/notes/example.md".into(),
//!     change_type: NoteChangeType::Content,
//! }).await?;
//!
//! if outcome.cancelled {
//!     println!("Event was cancelled");
//! }
//!
//! // Subscribe to events with handler result control
//! let id = subscriber.subscribe(
//!     "note_logger",
//!     EventFilter::event_type("note:*"),
//!     box_handler(|event| async move {
//!         tracing::info!("Note event: {:?}", event);
//!         HandlerResult::ok(event)
//!     }),
//! )?;
//! ```

pub mod emitter;
pub mod handler;
pub mod session_event;
pub mod subscriber;

// Re-exports for convenient access
pub use emitter::{
    EmitOutcome, EmitResult, EventEmitter, EventError, HandlerErrorInfo, NoOpEmitter,
    SharedEventBus,
};

pub use handler::HandlerResult;

pub use subscriber::{
    box_handler, BoxedHandlerFn, EventBus, EventFilter, EventSubscriber, HandlerFuture,
    SubscriptionError, SubscriptionId, SubscriptionIdGenerator, SubscriptionInfo,
    SubscriptionResult,
};

// Session event types (Task 1.2.1, 1.2.2, 1.2.3, 1.2.4, 1.3.2, 3.3.3)
pub use session_event::{
    EntityType, FileChangeKind, NoteChangeType, NotePayload, Priority, SessionEvent,
    SessionEventConfig, ToolCall, ToolSource,
};
