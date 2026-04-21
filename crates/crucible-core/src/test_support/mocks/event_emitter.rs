//! Mock event emitter.

use async_trait::async_trait;
use std::sync::{Arc, Mutex};

use crate::events::{EmitOutcome, EmitResult, EventEmitter, EventError, HandlerErrorInfo};

/// Statistics for mock event emitter operations
///
/// Tracks all events emitted through the mock for test assertions.
#[derive(Debug, Clone, Default)]
pub struct MockEventEmitterStats {
    /// Total number of emit calls
    pub emit_count: usize,
    /// Total number of emit_recursive calls
    pub emit_recursive_count: usize,
    /// Number of events that were cancelled
    pub cancelled_count: usize,
    /// Number of emit calls that resulted in errors
    pub error_count: usize,
}

/// Behavior configuration for the mock emitter
#[derive(Debug, Clone, Default)]
pub struct MockEmitterBehavior {
    /// If set, emit() will return this error
    pub error: Option<EventError>,
    /// If true, events will be marked as cancelled
    pub cancel_events: bool,
    /// Handler errors to include in outcomes
    pub handler_errors: Vec<HandlerErrorInfo>,
    /// If true, the emitter reports as unavailable
    pub unavailable: bool,
}

/// Internal state for mock event emitter
#[derive(Debug)]
struct MockEventEmitterState<E> {
    /// All emitted events (for verification)
    emitted_events: Vec<E>,
    /// Operation statistics
    stats: MockEventEmitterStats,
    /// Configured behavior
    behavior: MockEmitterBehavior,
}

impl<E> Default for MockEventEmitterState<E> {
    fn default() -> Self {
        Self {
            emitted_events: Vec::new(),
            stats: MockEventEmitterStats::default(),
            behavior: MockEmitterBehavior::default(),
        }
    }
}

/// Mock event emitter for testing
///
/// This provides a configurable event emitter that records all emitted events
/// for test verification. It supports:
///
/// - **Event Recording**: All events are stored for later inspection
/// - **Error Injection**: Simulate emission failures
/// - **Cancellation Simulation**: Test event cancellation handling
/// - **Handler Errors**: Simulate handler failures with fail-open semantics
/// - **Thread-Safe**: Uses Arc<Mutex<>> for concurrent access
///
/// # Examples
///
/// ## Basic Usage
///
/// ```rust
/// use crate::test_support::mocks::MockEventEmitter;
/// use crate::events::EventEmitter;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let emitter: MockEventEmitter<String> = MockEventEmitter::new();
///
/// // Emit an event
/// let outcome = emitter.emit("test event".to_string()).await?;
/// assert!(!outcome.cancelled);
///
/// // Verify the event was recorded
/// let events = emitter.emitted_events();
/// assert_eq!(events.len(), 1);
/// assert_eq!(events[0], "test event");
///
/// // Check statistics
/// let stats = emitter.stats();
/// assert_eq!(stats.emit_count, 1);
/// # Ok(())
/// # }
/// ```
///
/// ## Error Injection
///
/// ```rust
/// use crate::test_support::mocks::MockEventEmitter;
/// use crate::events::{EventEmitter, EventError};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let emitter: MockEventEmitter<String> = MockEventEmitter::new();
///
/// // Configure to return an error
/// emitter.set_error(Some(EventError::unavailable("test failure")));
///
/// let result = emitter.emit("test".to_string()).await;
/// assert!(result.is_err());
/// # Ok(())
/// # }
/// ```
///
/// ## Cancellation Testing
///
/// ```rust
/// use crate::test_support::mocks::MockEventEmitter;
/// use crate::events::EventEmitter;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let emitter: MockEventEmitter<String> = MockEventEmitter::new();
///
/// // Configure to cancel events
/// emitter.set_cancel_events(true);
///
/// let outcome = emitter.emit("test".to_string()).await?;
/// assert!(outcome.cancelled);
/// # Ok(())
/// # }
/// ```
///
/// ## Handler Error Simulation
///
/// ```rust
/// use crate::test_support::mocks::MockEventEmitter;
/// use crate::events::{EventEmitter, HandlerErrorInfo};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let emitter: MockEventEmitter<String> = MockEventEmitter::new();
///
/// // Configure handler errors (fail-open semantics)
/// emitter.add_handler_error(HandlerErrorInfo::new("test_handler", "handler failed"));
///
/// let outcome = emitter.emit("test".to_string()).await?;
/// assert!(outcome.has_errors());
/// assert_eq!(outcome.error_count(), 1);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct MockEventEmitter<E> {
    state: Arc<Mutex<MockEventEmitterState<E>>>,
}

impl<E> MockEventEmitter<E> {
    /// Create a new mock event emitter
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockEventEmitterState::default())),
        }
    }

    /// Get operation statistics
    pub fn stats(&self) -> MockEventEmitterStats {
        self.state.lock().unwrap().stats.clone()
    }

    /// Get all emitted events
    pub fn emitted_events(&self) -> Vec<E>
    where
        E: Clone,
    {
        self.state.lock().unwrap().emitted_events.clone()
    }

    /// Get the number of emitted events
    pub fn event_count(&self) -> usize {
        self.state.lock().unwrap().emitted_events.len()
    }

    /// Get the last emitted event
    pub fn last_event(&self) -> Option<E>
    where
        E: Clone,
    {
        self.state.lock().unwrap().emitted_events.last().cloned()
    }

    /// Clear all recorded events and reset statistics
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.emitted_events.clear();
        state.stats = MockEventEmitterStats::default();
        state.behavior = MockEmitterBehavior::default();
    }

    /// Configure an error to return on emit
    pub fn set_error(&self, error: Option<EventError>) {
        self.state.lock().unwrap().behavior.error = error;
    }

    /// Configure whether to cancel events
    pub fn set_cancel_events(&self, cancel: bool) {
        self.state.lock().unwrap().behavior.cancel_events = cancel;
    }

    /// Add a handler error to include in outcomes
    pub fn add_handler_error(&self, error: HandlerErrorInfo) {
        self.state
            .lock()
            .unwrap()
            .behavior
            .handler_errors
            .push(error);
    }

    /// Clear all configured handler errors
    pub fn clear_handler_errors(&self) {
        self.state.lock().unwrap().behavior.handler_errors.clear();
    }

    /// Set whether the emitter reports as unavailable
    pub fn set_unavailable(&self, unavailable: bool) {
        self.state.lock().unwrap().behavior.unavailable = unavailable;
    }

    /// Get the current behavior configuration
    pub fn behavior(&self) -> MockEmitterBehavior {
        self.state.lock().unwrap().behavior.clone()
    }
}

impl<E> Default for MockEventEmitter<E> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<E: Send + Sync + Clone + 'static> EventEmitter for MockEventEmitter<E> {
    type Event = E;

    async fn emit(&self, event: Self::Event) -> EmitResult<EmitOutcome<Self::Event>> {
        let mut state = self.state.lock().unwrap();

        state.stats.emit_count += 1;

        // Check for configured error - clone first to avoid borrow conflict
        if let Some(error) = state.behavior.error.clone() {
            state.stats.error_count += 1;
            return Err(error);
        }

        // Record the event
        state.emitted_events.push(event.clone());

        // Build outcome based on configuration
        let cancelled = state.behavior.cancel_events;
        if cancelled {
            state.stats.cancelled_count += 1;
        }

        let handler_errors = state.behavior.handler_errors.clone();

        if cancelled {
            let mut outcome = EmitOutcome::cancelled(event);
            outcome.errors = handler_errors;
            Ok(outcome)
        } else if !handler_errors.is_empty() {
            Ok(EmitOutcome::with_errors(event, handler_errors))
        } else {
            Ok(EmitOutcome::new(event))
        }
    }

    async fn emit_recursive(
        &self,
        event: Self::Event,
    ) -> EmitResult<Vec<EmitOutcome<Self::Event>>> {
        {
            let mut state = self.state.lock().unwrap();
            state.stats.emit_recursive_count += 1;
        }

        // For mock, just delegate to single emit
        let outcome = self.emit(event).await?;
        Ok(vec![outcome])
    }

    fn is_available(&self) -> bool {
        !self.state.lock().unwrap().behavior.unavailable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_event_emitter_basic() {
        let emitter: MockEventEmitter<String> = MockEventEmitter::new();

        // Emit an event
        let outcome = emitter.emit("test event".to_string()).await.unwrap();
        assert!(!outcome.cancelled);
        assert!(!outcome.has_errors());
        assert_eq!(outcome.event, "test event");

        // Check stats
        let stats = emitter.stats();
        assert_eq!(stats.emit_count, 1);
        assert_eq!(stats.cancelled_count, 0);
        assert_eq!(stats.error_count, 0);

        // Check recorded events
        assert_eq!(emitter.event_count(), 1);
        let events = emitter.emitted_events();
        assert_eq!(events[0], "test event");
    }

    #[tokio::test]
    async fn test_mock_event_emitter_multiple_events() {
        let emitter: MockEventEmitter<String> = MockEventEmitter::new();

        emitter.emit("event1".to_string()).await.unwrap();
        emitter.emit("event2".to_string()).await.unwrap();
        emitter.emit("event3".to_string()).await.unwrap();

        assert_eq!(emitter.event_count(), 3);
        assert_eq!(emitter.last_event(), Some("event3".to_string()));

        let stats = emitter.stats();
        assert_eq!(stats.emit_count, 3);
    }

    #[tokio::test]
    async fn test_mock_event_emitter_error_injection() {
        let emitter: MockEventEmitter<String> = MockEventEmitter::new();

        // Configure error
        emitter.set_error(Some(EventError::unavailable("test failure")));

        let result = emitter.emit("test".to_string()).await;
        assert!(result.is_err());

        // Stats should reflect the error
        let stats = emitter.stats();
        assert_eq!(stats.emit_count, 1);
        assert_eq!(stats.error_count, 1);

        // Event should NOT be recorded when error occurs
        assert_eq!(emitter.event_count(), 0);

        // Clear error and emit again
        emitter.set_error(None);
        let outcome = emitter.emit("success".to_string()).await.unwrap();
        assert!(!outcome.cancelled);
        assert_eq!(emitter.event_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_event_emitter_cancellation() {
        let emitter: MockEventEmitter<String> = MockEventEmitter::new();

        emitter.set_cancel_events(true);

        let outcome = emitter.emit("test".to_string()).await.unwrap();
        assert!(outcome.cancelled);

        let stats = emitter.stats();
        assert_eq!(stats.cancelled_count, 1);

        // Disable cancellation
        emitter.set_cancel_events(false);
        let outcome = emitter.emit("not cancelled".to_string()).await.unwrap();
        assert!(!outcome.cancelled);
    }

    #[tokio::test]
    async fn test_mock_event_emitter_handler_errors() {
        let emitter: MockEventEmitter<String> = MockEventEmitter::new();

        // Add handler errors
        emitter.add_handler_error(HandlerErrorInfo::new("handler1", "failed"));
        emitter.add_handler_error(HandlerErrorInfo::new("handler2", "also failed"));

        let outcome = emitter.emit("test".to_string()).await.unwrap();
        assert!(outcome.has_errors());
        assert_eq!(outcome.error_count(), 2);

        // Event should still succeed (fail-open semantics)
        assert!(!outcome.cancelled);
        assert_eq!(outcome.event, "test");

        // Clear errors
        emitter.clear_handler_errors();
        let outcome = emitter.emit("test2".to_string()).await.unwrap();
        assert!(!outcome.has_errors());
    }

    #[tokio::test]
    async fn test_mock_event_emitter_availability() {
        let emitter: MockEventEmitter<String> = MockEventEmitter::new();

        assert!(emitter.is_available());

        emitter.set_unavailable(true);
        assert!(!emitter.is_available());

        emitter.set_unavailable(false);
        assert!(emitter.is_available());
    }

    #[tokio::test]
    async fn test_mock_event_emitter_reset() {
        let emitter: MockEventEmitter<String> = MockEventEmitter::new();

        emitter.emit("event1".to_string()).await.unwrap();
        emitter.emit("event2".to_string()).await.unwrap();
        emitter.set_cancel_events(true);
        emitter.add_handler_error(HandlerErrorInfo::new("handler", "error"));

        // Reset
        emitter.reset();

        // Everything should be cleared
        assert_eq!(emitter.event_count(), 0);
        let stats = emitter.stats();
        assert_eq!(stats.emit_count, 0);

        let behavior = emitter.behavior();
        assert!(!behavior.cancel_events);
        assert!(behavior.handler_errors.is_empty());
    }

    #[tokio::test]
    async fn test_mock_event_emitter_emit_recursive() {
        let emitter: MockEventEmitter<String> = MockEventEmitter::new();

        let outcomes = emitter.emit_recursive("test".to_string()).await.unwrap();
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].event, "test");

        let stats = emitter.stats();
        assert_eq!(stats.emit_count, 1);
        assert_eq!(stats.emit_recursive_count, 1);
    }

    #[tokio::test]
    async fn test_mock_event_emitter_thread_safe() {
        use std::sync::Arc;

        let emitter: Arc<MockEventEmitter<i32>> = Arc::new(MockEventEmitter::new());

        // Spawn multiple tasks that emit concurrently
        let mut handles = vec![];
        for i in 0..10 {
            let emitter_clone = Arc::clone(&emitter);
            handles.push(tokio::spawn(async move {
                emitter_clone.emit(i).await.unwrap();
            }));
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        // All events should be recorded
        assert_eq!(emitter.event_count(), 10);
        let stats = emitter.stats();
        assert_eq!(stats.emit_count, 10);
    }

    #[tokio::test]
    async fn test_mock_event_emitter_with_custom_types() {
        #[derive(Clone, Debug, PartialEq)]
        struct CustomEvent {
            id: u32,
            name: String,
        }

        let emitter: MockEventEmitter<CustomEvent> = MockEventEmitter::new();

        let event = CustomEvent {
            id: 1,
            name: "test".to_string(),
        };

        let outcome = emitter.emit(event.clone()).await.unwrap();
        assert_eq!(outcome.event, event);

        let events = emitter.emitted_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, 1);
        assert_eq!(events[0].name, "test");
    }
}
