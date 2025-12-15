//! Handler result types for event processing.
//!
//! This module defines the `HandlerResult` enum that handlers return to indicate
//! how event processing should continue. The result determines:
//!
//! - Whether the event continues to the next handler
//! - Whether the event is cancelled
//! - Whether an error occurred (soft or fatal)
//!
//! # Result Variants
//!
//! - [`HandlerResult::Continue`]: Processing succeeded, pass event to next handler
//! - [`HandlerResult::Cancel`]: Stop processing, event is cancelled
//! - [`HandlerResult::SoftError`]: Non-fatal error, continue with event
//! - [`HandlerResult::FatalError`]: Fatal error, stop processing immediately
//!
//! # Example
//!
//! ```ignore
//! use crucible_core::events::{HandlerResult, SessionEvent};
//!
//! fn my_handler(event: SessionEvent) -> HandlerResult {
//!     match &event {
//!         SessionEvent::ToolCalled { name, .. } if name == "dangerous" => {
//!             // Cancel dangerous tool calls
//!             HandlerResult::Cancel
//!         }
//!         SessionEvent::FileChanged { path, .. } => {
//!             // Continue processing file changes
//!             HandlerResult::Continue(event)
//!         }
//!         _ => HandlerResult::Continue(event),
//!     }
//! }
//! ```

use super::emitter::EventError;

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
        matches!(self, Self::Cancel | Self::Cancelled(_) | Self::FatalError(_))
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

        let result: HandlerResult<String> =
            HandlerResult::soft_error("test".into(), "error");
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

        let result: HandlerResult<String> =
            HandlerResult::soft_error("test".into(), "oops");
        assert_eq!(format!("{}", result), "SoftError: oops");
    }

    #[test]
    fn test_handler_result_default() {
        let result: HandlerResult<String> = HandlerResult::default();
        assert!(result.is_continue());
        assert_eq!(result.event(), Some(String::default()));
    }
}
