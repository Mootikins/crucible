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
//! use crucible_core::events::{EventEmitter, EmitOutcome, SessionEvent, NoteChangeType};
//!
//! async fn notify_file_change<E: EventEmitter>(emitter: &E, path: &str) -> EmitOutcome<SessionEvent> {
//!     emitter.emit(SessionEvent::NoteModified {
//!         path: path.into(),
//!         change_type: NoteChangeType::Content,
//!     }).await
//! }
//! ```

use async_trait::async_trait;
use std::fmt;
use std::sync::Arc;

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
///     async fn do_work(&self) {
///         // Emit an event
///         let outcome = self.emitter.emit(SessionEvent::Custom {
///             name: "work_started".into(),
///             payload: json!({}),
///         }).await;
///
///         if outcome.cancelled {
///             println!("Work was cancelled by handler");
///         }
///     }
/// }
/// ```
#[async_trait]
pub trait EventEmitter: Send + Sync {
    /// The event type this emitter handles.
    ///
    /// This is typically `SessionEvent` from `crucible-lua`, but the trait
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
    /// Fail-open: a handler failure never stops emission, so this returns
    /// the outcome directly rather than a `Result`.
    async fn emit(&self, event: Self::Event) -> EmitOutcome<Self::Event>;

    /// Check if the event bus is available and ready.
    ///
    /// # Returns
    ///
    /// Returns `true` if events can be emitted, `false` otherwise.
    fn is_available(&self) -> bool;
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

    async fn emit(&self, event: Self::Event) -> EmitOutcome<Self::Event> {
        EmitOutcome::new(event)
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let outcome = emitter.emit("test event".into()).await;
        assert_eq!(outcome.event, "test event");
        assert!(!outcome.cancelled);
        assert!(!outcome.has_errors());
    }
}
