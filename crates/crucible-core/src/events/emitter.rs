//! EventEmitter trait for emitting events to the event bus.
//!
//! This module defines the core `EventEmitter` trait that components use to emit
//! events. The trait is designed for:
//!
//! - **Async operation**: Events may be dispatched asynchronously
//! - **Fail-open semantics**: Handler errors don't block emission
//! - **Type-safe events**: Events are strongly typed via `SessionEvent`
//!
//! # Example
//!
//! ```ignore
//! use crucible_core::events::{EventEmitter, EmitResult, SessionEvent, NoteChangeType};
//!
//! async fn notify_file_change<E: EventEmitter>(emitter: &E, path: &str) -> EmitResult<()> {
//!     emitter.emit(SessionEvent::NoteModified {
//!         path: path.into(),
//!         change_type: NoteChangeType::Content,
//!     }).await
//! }
//! ```

use async_trait::async_trait;
use std::fmt;
use std::sync::Arc;

/// Result type for event emission operations.
pub type EmitResult<T> = Result<T, EventError>;

/// Errors that can occur during event emission.
#[derive(Debug, Clone)]
pub enum EventError {
    /// One or more handlers failed during event processing.
    ///
    /// Contains the list of handler names and their error messages.
    /// Events still complete with fail-open semantics.
    HandlersFailed {
        /// Handler names that failed
        handlers: Vec<String>,
        /// Error messages from each handler
        messages: Vec<String>,
    },

    /// Event was cancelled by a handler.
    ///
    /// This is not necessarily an error - some events (like `tool:before`)
    /// can be cancelled to prevent execution.
    Cancelled {
        /// Handler that cancelled the event
        cancelled_by: String,
    },

    /// Event bus is not available.
    ///
    /// The event bus may be disconnected or not yet initialized.
    Unavailable {
        /// Reason for unavailability
        reason: String,
    },

    /// Serialization error when converting event payload.
    SerializationError {
        /// Error message
        message: String,
    },

    /// Generic emission error.
    Other {
        /// Error message
        message: String,
    },
}

impl EventError {
    /// Create a handlers failed error.
    pub fn handlers_failed(handlers: Vec<String>, messages: Vec<String>) -> Self {
        Self::HandlersFailed { handlers, messages }
    }

    /// Create a cancelled error.
    pub fn cancelled(by: impl Into<String>) -> Self {
        Self::Cancelled {
            cancelled_by: by.into(),
        }
    }

    /// Create an unavailable error.
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }

    /// Create a serialization error.
    pub fn serialization(message: impl Into<String>) -> Self {
        Self::SerializationError {
            message: message.into(),
        }
    }

    /// Create a generic error.
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other {
            message: message.into(),
        }
    }

    /// Check if this error represents a cancellation (not a failure).
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled { .. })
    }

    /// Check if this error is fatal (should stop processing).
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::Unavailable { .. })
    }
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HandlersFailed { handlers, messages } => {
                write!(f, "Event handlers failed: ")?;
                for (h, m) in handlers.iter().zip(messages.iter()) {
                    write!(f, "[{}: {}] ", h, m)?;
                }
                Ok(())
            }
            Self::Cancelled { cancelled_by } => {
                write!(f, "Event cancelled by handler '{}'", cancelled_by)
            }
            Self::Unavailable { reason } => {
                write!(f, "Event bus unavailable: {}", reason)
            }
            Self::SerializationError { message } => {
                write!(f, "Event serialization error: {}", message)
            }
            Self::Other { message } => {
                write!(f, "Event error: {}", message)
            }
        }
    }
}

impl std::error::Error for EventError {}

/// Outcome of emitting an event.
///
/// This captures both the (possibly modified) event and any non-fatal errors
/// that occurred during handler processing.
#[derive(Debug, Clone)]
pub struct EmitOutcome<E> {
    /// The event after handler processing (may be modified).
    pub event: E,

    /// Non-fatal errors from handlers (fail-open semantics).
    pub errors: Vec<HandlerErrorInfo>,

    /// Whether the event was cancelled.
    pub cancelled: bool,
}

impl<E> EmitOutcome<E> {
    /// Create a new emit outcome.
    pub fn new(event: E) -> Self {
        Self {
            event,
            errors: Vec::new(),
            cancelled: false,
        }
    }

    /// Create an outcome with errors.
    pub fn with_errors(event: E, errors: Vec<HandlerErrorInfo>) -> Self {
        Self {
            event,
            errors,
            cancelled: false,
        }
    }

    /// Create a cancelled outcome.
    pub fn cancelled(event: E) -> Self {
        Self {
            event,
            errors: Vec::new(),
            cancelled: true,
        }
    }

    /// Check if any handlers reported errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Get the number of handler errors.
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }
}

/// Information about a handler error.
#[derive(Debug, Clone)]
pub struct HandlerErrorInfo {
    /// Name of the handler that failed.
    pub handler_name: String,

    /// Error message.
    pub message: String,

    /// Whether this error was marked as fatal.
    pub fatal: bool,
}

impl HandlerErrorInfo {
    /// Create a new handler error info.
    pub fn new(handler_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            handler_name: handler_name.into(),
            message: message.into(),
            fatal: false,
        }
    }

    /// Create a fatal handler error.
    pub fn fatal(handler_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            handler_name: handler_name.into(),
            message: message.into(),
            fatal: true,
        }
    }
}

impl fmt::Display for HandlerErrorInfo {
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

/// Trait for emitting events to the event bus.
///
/// This is the primary interface for components to emit events. Implementations
/// handle dispatching events to registered handlers and collecting results.
///
/// # Fail-Open Semantics
///
/// By default, handler failures don't prevent event emission. Non-fatal errors
/// are collected and returned in the `EmitOutcome`, while the event continues
/// through the handler chain.
///
/// # Cancellation
///
/// Some events (like `tool:before`) can be cancelled by handlers. When cancelled,
/// the event stops propagating and `EmitOutcome::cancelled` is set to `true`.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` to enable use across async boundaries.
/// The trait uses `async_trait` to support async event dispatch.
///
/// # Example
///
/// ```ignore
/// use crucible_core::events::{EventEmitter, EmitOutcome};
///
/// struct MyComponent<E: EventEmitter> {
///     emitter: E,
/// }
///
/// impl<E: EventEmitter> MyComponent<E> {
///     async fn do_work(&self) -> Result<(), Box<dyn std::error::Error>> {
///         // Emit an event
///         let outcome = self.emitter.emit(SessionEvent::Custom {
///             name: "work_started".into(),
///             payload: json!({}),
///         }).await?;
///
///         if outcome.cancelled {
///             println!("Work was cancelled by handler");
///         }
///
///         Ok(())
///     }
/// }
/// ```
#[async_trait]
pub trait EventEmitter: Send + Sync {
    /// The event type this emitter handles.
    ///
    /// This is typically `SessionEvent` from `crucible-rune`, but the trait
    /// is generic to allow for different event systems.
    type Event: Send + Clone;

    /// Emit an event through the handler pipeline.
    ///
    /// The event is dispatched to all matching handlers in priority order.
    /// Handlers may modify the event, and the final (possibly modified)
    /// event is returned in the outcome.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to emit
    ///
    /// # Returns
    ///
    /// Returns an `EmitOutcome` containing:
    /// - The (possibly modified) event
    /// - Any non-fatal handler errors
    /// - Whether the event was cancelled
    ///
    /// Returns an `EventError` if emission fails catastrophically.
    async fn emit(&self, event: Self::Event) -> EmitResult<EmitOutcome<Self::Event>>;

    /// Emit an event and recursively process any events emitted by handlers.
    ///
    /// Some handlers may emit new events during processing. This method
    /// continues processing until no new events are generated.
    ///
    /// # Arguments
    ///
    /// * `event` - The initial event to emit
    ///
    /// # Returns
    ///
    /// Returns a vector of outcomes, one for each event processed (including
    /// the original and any events emitted by handlers).
    async fn emit_recursive(
        &self,
        event: Self::Event,
    ) -> EmitResult<Vec<EmitOutcome<Self::Event>>> {
        // Default implementation: just emit once
        let outcome = self.emit(event).await?;
        Ok(vec![outcome])
    }

    /// Check if the event bus is available and ready.
    ///
    /// # Returns
    ///
    /// Returns `true` if events can be emitted, `false` otherwise.
    fn is_available(&self) -> bool {
        true
    }
}

/// A shared event bus reference.
///
/// This type wraps an `Arc<dyn EventEmitter>` for convenient sharing across
/// components. It implements `Clone` for easy distribution.
pub type SharedEventBus<E> = Arc<dyn EventEmitter<Event = E>>;

/// No-op event emitter for testing or disabled event systems.
///
/// This emitter accepts all events but does nothing with them.
/// Useful for:
/// - Unit testing components in isolation
/// - Running without an event bus
/// - Benchmarking without event overhead
#[derive(Debug, Clone, Default)]
pub struct NoOpEmitter<E> {
    _phantom: std::marker::PhantomData<E>,
}

impl<E> NoOpEmitter<E> {
    /// Create a new no-op emitter.
    pub fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<E: Send + Sync + Clone + 'static> EventEmitter for NoOpEmitter<E> {
    type Event = E;

    async fn emit(&self, event: Self::Event) -> EmitResult<EmitOutcome<Self::Event>> {
        Ok(EmitOutcome::new(event))
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_error_display() {
        let err = EventError::handlers_failed(
            vec!["handler1".into(), "handler2".into()],
            vec!["failed".into(), "timeout".into()],
        );
        let s = format!("{}", err);
        assert!(s.contains("handler1"));
        assert!(s.contains("handler2"));

        let err = EventError::cancelled("my_handler");
        assert!(format!("{}", err).contains("my_handler"));

        let err = EventError::unavailable("disconnected");
        assert!(format!("{}", err).contains("disconnected"));

        let err = EventError::serialization("invalid json");
        assert!(format!("{}", err).contains("invalid json"));

        let err = EventError::other("unknown");
        assert!(format!("{}", err).contains("unknown"));
    }

    #[test]
    fn test_event_error_predicates() {
        assert!(EventError::cancelled("test").is_cancelled());
        assert!(!EventError::other("test").is_cancelled());

        assert!(EventError::unavailable("test").is_fatal());
        assert!(!EventError::cancelled("test").is_fatal());
        assert!(!EventError::other("test").is_fatal());
    }

    #[test]
    fn test_emit_outcome() {
        let outcome: EmitOutcome<String> = EmitOutcome::new("test".into());
        assert!(!outcome.has_errors());
        assert!(!outcome.cancelled);
        assert_eq!(outcome.error_count(), 0);

        let errors = vec![HandlerErrorInfo::new("h1", "failed")];
        let outcome: EmitOutcome<String> = EmitOutcome::with_errors("test".to_string(), errors);
        assert!(outcome.has_errors());
        assert_eq!(outcome.error_count(), 1);

        let outcome: EmitOutcome<String> = EmitOutcome::cancelled("test".into());
        assert!(outcome.cancelled);
    }

    #[test]
    fn test_handler_error_info() {
        let info = HandlerErrorInfo::new("handler", "message");
        assert!(!info.fatal);
        assert!(format!("{}", info).contains("handler"));
        assert!(format!("{}", info).contains("message"));

        let info = HandlerErrorInfo::fatal("handler", "critical");
        assert!(info.fatal);
        assert!(format!("{}", info).contains("(fatal)"));
    }

    #[tokio::test]
    async fn test_noop_emitter() {
        let emitter: NoOpEmitter<String> = NoOpEmitter::new();

        assert!(emitter.is_available());

        let outcome = emitter.emit("test event".into()).await.unwrap();
        assert_eq!(outcome.event, "test event");
        assert!(!outcome.cancelled);
        assert!(!outcome.has_errors());
    }

    #[tokio::test]
    async fn test_emit_recursive_default() {
        let emitter: NoOpEmitter<String> = NoOpEmitter::new();

        let outcomes = emitter.emit_recursive("test".into()).await.unwrap();
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].event, "test");
    }
}
