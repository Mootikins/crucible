//! EventSubscriber trait for subscribing to events from the event bus.
//!
//! This module defines the `EventSubscriber` trait that components use to subscribe
//! to events. The trait is designed for:
//!
//! - **Async operation**: Handlers may process events asynchronously
//! - **Pattern matching**: Subscribers can filter events by type and pattern
//! - **Fail-open semantics**: Handler errors don't block other handlers
//!
//! # Example
//!
//! ```ignore
//! use crucible_core::events::{EventSubscriber, SubscriptionId, HandlerFn};
//!
//! // Create a subscriber
//! let mut subscriber = MySubscriber::new();
//!
//! // Subscribe to note events
//! let id = subscriber.subscribe(
//!     "note_logger",
//!     EventFilter::event_type("note:*"),
//!     |event| async move {
//!         println!("Note event: {:?}", event);
//!         Ok(event)
//!     },
//! )?;
//!
//! // Later, unsubscribe
//! subscriber.unsubscribe(id)?;
//! ```

use async_trait::async_trait;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// Import HandlerResult from handler module (consolidated type)
pub use super::handler::HandlerResult;

// ─────────────────────────────────────────────────────────────────────────────
// Subscription ID
// ─────────────────────────────────────────────────────────────────────────────

/// Unique identifier for a subscription.
///
/// Used to manage (update, unsubscribe) individual handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(u64);

impl SubscriptionId {
    /// Create a new subscription ID.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the inner ID value.
    pub fn inner(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for SubscriptionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sub_{}", self.0)
    }
}

/// Generator for unique subscription IDs.
#[derive(Debug, Default)]
pub struct SubscriptionIdGenerator {
    next_id: AtomicU64,
}

impl SubscriptionIdGenerator {
    /// Create a new ID generator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate the next unique subscription ID.
    pub fn next(&self) -> SubscriptionId {
        SubscriptionId(self.next_id.fetch_add(1, Ordering::Relaxed))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Event Filter
// ─────────────────────────────────────────────────────────────────────────────

/// Filter for matching events.
///
/// Supports filtering by event type and/or identifier pattern.
#[derive(Debug, Clone)]
pub struct EventFilter {
    /// Event type pattern (e.g., "note:*", "tool:before").
    ///
    /// Uses glob-style matching with `*` and `?` wildcards.
    pub event_type: Option<String>,

    /// Identifier pattern (e.g., "read_*", "notes/*.md").
    ///
    /// Uses glob-style matching with `*` and `?` wildcards.
    pub identifier: Option<String>,
}

impl EventFilter {
    /// Create a filter that matches all events.
    pub fn all() -> Self {
        Self {
            event_type: None,
            identifier: None,
        }
    }

    /// Create a filter for a specific event type pattern.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Match all note events
    /// let filter = EventFilter::event_type("note:*");
    ///
    /// // Match only tool:before events
    /// let filter = EventFilter::event_type("tool:before");
    /// ```
    pub fn event_type(pattern: impl Into<String>) -> Self {
        Self {
            event_type: Some(pattern.into()),
            identifier: None,
        }
    }

    /// Create a filter for a specific identifier pattern.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Match all events for tools starting with "read_"
    /// let filter = EventFilter::identifier("read_*");
    /// ```
    pub fn identifier(pattern: impl Into<String>) -> Self {
        Self {
            event_type: None,
            identifier: Some(pattern.into()),
        }
    }

    /// Add an event type pattern to this filter.
    pub fn with_event_type(mut self, pattern: impl Into<String>) -> Self {
        self.event_type = Some(pattern.into());
        self
    }

    /// Add an identifier pattern to this filter.
    pub fn with_identifier(mut self, pattern: impl Into<String>) -> Self {
        self.identifier = Some(pattern.into());
        self
    }

    /// Check if this filter matches the given event type and identifier.
    ///
    /// Both patterns must match (if specified) for the filter to match.
    pub fn matches(&self, event_type: &str, identifier: &str) -> bool {
        let type_matches = self
            .event_type
            .as_ref()
            .map(|p| match_glob(p, event_type))
            .unwrap_or(true);

        let id_matches = self
            .identifier
            .as_ref()
            .map(|p| match_glob(p, identifier))
            .unwrap_or(true);

        type_matches && id_matches
    }
}

impl Default for EventFilter {
    fn default() -> Self {
        Self::all()
    }
}

/// Simple glob pattern matching.
///
/// Supports `*` (matches any sequence) and `?` (matches single char).
fn match_glob(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let mut pattern_idx = 0;
    let mut text_idx = 0;
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    let mut star_idx: Option<usize> = None;
    let mut match_idx: Option<usize> = None;

    while text_idx < text_chars.len() {
        if pattern_idx < pattern_chars.len() && pattern_chars[pattern_idx] == '*' {
            star_idx = Some(pattern_idx);
            match_idx = Some(text_idx);
            pattern_idx += 1;
        } else if pattern_idx < pattern_chars.len()
            && (pattern_chars[pattern_idx] == text_chars[text_idx]
                || pattern_chars[pattern_idx] == '?')
        {
            pattern_idx += 1;
            text_idx += 1;
        } else if let Some(star) = star_idx {
            pattern_idx = star + 1;
            match_idx = Some(match_idx.unwrap() + 1);
            text_idx = match_idx.unwrap();
        } else {
            return false;
        }
    }

    // Check for remaining stars in pattern
    while pattern_idx < pattern_chars.len() && pattern_chars[pattern_idx] == '*' {
        pattern_idx += 1;
    }

    pattern_idx == pattern_chars.len()
}

// ─────────────────────────────────────────────────────────────────────────────
// Handler Function Types
// ─────────────────────────────────────────────────────────────────────────────

/// Boxed async handler future.
pub type HandlerFuture<'a, E> = Pin<Box<dyn Future<Output = HandlerResult<E>> + Send + 'a>>;

/// Type-erased async handler function.
///
/// Takes an event and returns a future that produces a `HandlerResult`.
pub type BoxedHandlerFn<E> =
    Arc<dyn Fn(E) -> Pin<Box<dyn Future<Output = HandlerResult<E>> + Send>> + Send + Sync>;

/// Create a boxed handler from an async function.
///
/// # Example
///
/// ```ignore
/// let handler = box_handler(|event: SessionEvent| async move {
///     println!("Received: {:?}", event);
///     HandlerResult::ok(event)
/// });
/// ```
pub fn box_handler<E, F, Fut>(f: F) -> BoxedHandlerFn<E>
where
    E: Send + 'static,
    F: Fn(E) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = HandlerResult<E>> + Send + 'static,
{
    Arc::new(move |event| Box::pin(f(event)))
}

// ─────────────────────────────────────────────────────────────────────────────
// Subscription Info
// ─────────────────────────────────────────────────────────────────────────────

/// Information about a registered subscription.
#[derive(Debug, Clone)]
pub struct SubscriptionInfo {
    /// Unique subscription ID.
    pub id: SubscriptionId,

    /// Human-readable name for this subscription.
    pub name: String,

    /// Filter for matching events.
    pub filter: EventFilter,

    /// Priority (lower = earlier execution).
    pub priority: i64,

    /// Whether this subscription is enabled.
    pub enabled: bool,
}

impl SubscriptionInfo {
    /// Create new subscription info.
    pub fn new(id: SubscriptionId, name: impl Into<String>, filter: EventFilter) -> Self {
        Self {
            id,
            name: name.into(),
            filter,
            priority: 100,
            enabled: true,
        }
    }

    /// Set the priority.
    pub fn with_priority(mut self, priority: i64) -> Self {
        self.priority = priority;
        self
    }

    /// Set the enabled state.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Subscription Error
// ─────────────────────────────────────────────────────────────────────────────

/// Errors that can occur during subscription operations.
#[derive(Debug, Clone)]
pub enum SubscriptionError {
    /// Subscription with this name already exists.
    DuplicateName {
        /// The duplicate name
        name: String,
    },

    /// Subscription ID not found.
    NotFound {
        /// The ID that was not found
        id: SubscriptionId,
    },

    /// Invalid filter pattern.
    InvalidFilter {
        /// Error message
        message: String,
    },

    /// Subscriber is not available (e.g., disconnected).
    Unavailable {
        /// Reason for unavailability
        reason: String,
    },
}

impl SubscriptionError {
    /// Create a duplicate name error.
    pub fn duplicate_name(name: impl Into<String>) -> Self {
        Self::DuplicateName { name: name.into() }
    }

    /// Create a not found error.
    pub fn not_found(id: SubscriptionId) -> Self {
        Self::NotFound { id }
    }

    /// Create an invalid filter error.
    pub fn invalid_filter(message: impl Into<String>) -> Self {
        Self::InvalidFilter {
            message: message.into(),
        }
    }

    /// Create an unavailable error.
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }
}

impl fmt::Display for SubscriptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateName { name } => {
                write!(f, "Subscription with name '{}' already exists", name)
            }
            Self::NotFound { id } => {
                write!(f, "Subscription {} not found", id)
            }
            Self::InvalidFilter { message } => {
                write!(f, "Invalid event filter: {}", message)
            }
            Self::Unavailable { reason } => {
                write!(f, "Subscriber unavailable: {}", reason)
            }
        }
    }
}

impl std::error::Error for SubscriptionError {}

/// Result type for subscription operations.
pub type SubscriptionResult<T> = Result<T, SubscriptionError>;

// ─────────────────────────────────────────────────────────────────────────────
// EventSubscriber Trait
// ─────────────────────────────────────────────────────────────────────────────

/// Trait for subscribing to events from the event bus.
///
/// This is the interface for registering event handlers. Implementations
/// manage subscriptions and dispatch events to matching handlers.
///
/// # Pattern Matching
///
/// Subscriptions use glob patterns to filter events:
/// - `*` matches any sequence of characters
/// - `?` matches a single character
///
/// # Priority
///
/// Handlers are executed in priority order (lower priority = earlier execution).
/// Handlers with the same priority execute in registration order.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` to enable use across async boundaries.
///
/// # Example
///
/// ```ignore
/// use crucible_core::events::{EventSubscriber, EventFilter, box_handler, HandlerResult};
///
/// // Subscribe to all tool events
/// let id = subscriber.subscribe(
///     "tool_logger",
///     EventFilter::event_type("tool:*"),
///     box_handler(|event| async move {
///         tracing::info!("Tool event: {:?}", event);
///         HandlerResult::ok(event)
///     }),
/// )?;
///
/// // Change handler priority
/// subscriber.set_priority(id, 50)?;
///
/// // Temporarily disable
/// subscriber.set_enabled(id, false)?;
///
/// // List all subscriptions
/// for info in subscriber.list_subscriptions() {
///     println!("{}: {:?}", info.name, info.filter);
/// }
///
/// // Unsubscribe when done
/// subscriber.unsubscribe(id)?;
/// ```
#[async_trait]
pub trait EventSubscriber: Send + Sync {
    /// The event type this subscriber handles.
    type Event: Send + Clone;

    /// Subscribe to events matching the given filter.
    ///
    /// # Arguments
    ///
    /// * `name` - Human-readable name for this subscription (must be unique)
    /// * `filter` - Filter for matching events
    /// * `handler` - Async function to handle matching events
    ///
    /// # Returns
    ///
    /// Returns a `SubscriptionId` that can be used to manage the subscription.
    fn subscribe(
        &mut self,
        name: impl Into<String> + Send,
        filter: EventFilter,
        handler: BoxedHandlerFn<Self::Event>,
    ) -> SubscriptionResult<SubscriptionId>;

    /// Subscribe with a specific priority.
    ///
    /// Lower priority values execute earlier.
    fn subscribe_with_priority(
        &mut self,
        name: impl Into<String> + Send,
        filter: EventFilter,
        priority: i64,
        handler: BoxedHandlerFn<Self::Event>,
    ) -> SubscriptionResult<SubscriptionId>;

    /// Unsubscribe a handler by ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The subscription ID returned from `subscribe`
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if unsubscribed, or `Err` if the ID was not found.
    fn unsubscribe(&mut self, id: SubscriptionId) -> SubscriptionResult<()>;

    /// Unsubscribe a handler by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name provided when subscribing
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if unsubscribed, or `Err` if the name was not found.
    fn unsubscribe_by_name(&mut self, name: &str) -> SubscriptionResult<()>;

    /// Get subscription info by ID.
    fn get_subscription(&self, id: SubscriptionId) -> Option<SubscriptionInfo>;

    /// Get subscription info by name.
    fn get_subscription_by_name(&self, name: &str) -> Option<SubscriptionInfo>;

    /// List all subscriptions.
    fn list_subscriptions(&self) -> Vec<SubscriptionInfo>;

    /// Count subscriptions matching a filter.
    fn count_subscriptions(&self, filter: &EventFilter) -> usize;

    /// Set the priority of a subscription.
    ///
    /// # Arguments
    ///
    /// * `id` - The subscription ID
    /// * `priority` - New priority value (lower = earlier)
    fn set_priority(&mut self, id: SubscriptionId, priority: i64) -> SubscriptionResult<()>;

    /// Enable or disable a subscription.
    ///
    /// Disabled subscriptions don't receive events but remain registered.
    ///
    /// # Arguments
    ///
    /// * `id` - The subscription ID
    /// * `enabled` - Whether the subscription should be enabled
    fn set_enabled(&mut self, id: SubscriptionId, enabled: bool) -> SubscriptionResult<()>;

    /// Check if the subscriber is available and ready.
    fn is_available(&self) -> bool {
        true
    }

    /// Clear all subscriptions.
    fn clear(&mut self);
}

/// A combined emitter and subscriber for bidirectional event handling.
///
/// This trait combines `EventEmitter` and `EventSubscriber` for components
/// that both emit and receive events.
#[async_trait]
pub trait EventBus: super::EventEmitter + EventSubscriber {
    /// Get the number of registered handlers.
    fn handler_count(&self) -> usize;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_id() {
        let id = SubscriptionId::new(42);
        assert_eq!(id.inner(), 42);
        assert_eq!(format!("{}", id), "sub_42");
    }

    #[test]
    fn test_subscription_id_generator() {
        let gen = SubscriptionIdGenerator::new();
        let id1 = gen.next();
        let id2 = gen.next();
        let id3 = gen.next();

        assert_eq!(id1.inner(), 0);
        assert_eq!(id2.inner(), 1);
        assert_eq!(id3.inner(), 2);
    }

    #[test]
    fn test_event_filter_all() {
        let filter = EventFilter::all();
        assert!(filter.matches("any_type", "any_identifier"));
        assert!(filter.matches("tool:before", "read_file"));
    }

    #[test]
    fn test_event_filter_event_type() {
        let filter = EventFilter::event_type("tool:*");
        assert!(filter.matches("tool:before", "anything"));
        assert!(filter.matches("tool:after", "anything"));
        assert!(!filter.matches("note:parsed", "anything"));
    }

    #[test]
    fn test_event_filter_identifier() {
        let filter = EventFilter::identifier("read_*");
        assert!(filter.matches("any_type", "read_file"));
        assert!(filter.matches("any_type", "read_note"));
        assert!(!filter.matches("any_type", "write_file"));
    }

    #[test]
    fn test_event_filter_combined() {
        let filter = EventFilter::event_type("tool:*").with_identifier("read_*");
        assert!(filter.matches("tool:before", "read_file"));
        assert!(filter.matches("tool:after", "read_note"));
        assert!(!filter.matches("tool:before", "write_file"));
        assert!(!filter.matches("note:parsed", "read_file"));
    }

    #[test]
    fn test_glob_pattern_star() {
        assert!(match_glob("*", "anything"));
        assert!(match_glob("tool:*", "tool:before"));
        assert!(match_glob("tool:*", "tool:after"));
        assert!(match_glob("*:before", "tool:before"));
        assert!(match_glob("*_test_*", "unit_test_foo"));
        assert!(!match_glob("tool:*", "note:parsed"));
    }

    #[test]
    fn test_glob_pattern_question() {
        assert!(match_glob("test?", "tests"));
        assert!(match_glob("t?st", "test"));
        assert!(!match_glob("test?", "test"));
        assert!(!match_glob("test?", "testing"));
    }

    #[test]
    fn test_glob_pattern_exact() {
        assert!(match_glob("exact", "exact"));
        assert!(!match_glob("exact", "exacty"));
        assert!(!match_glob("exact", "exac"));
    }

    // HandlerResult tests are now in handler.rs module (consolidated).
    // These tests verify the re-export works correctly.

    #[test]
    fn test_handler_result_reexport_ok() {
        // Verify HandlerResult is usable through the subscriber module
        let result: HandlerResult<String> = HandlerResult::ok("event".into());
        assert!(result.is_continue());
        assert!(!result.is_cancel());
        assert!(!result.is_cancelled());
        assert_eq!(result.event(), Some("event".into()));
    }

    #[test]
    fn test_handler_result_reexport_cancelled() {
        // Verify Cancelled variant works (preserves event)
        let result: HandlerResult<String> = HandlerResult::cancelled("event".into());
        assert!(!result.is_continue());
        assert!(result.is_cancel()); // is_cancel() matches both Cancel and Cancelled
        assert!(result.is_cancelled()); // is_cancelled() only matches Cancelled
        assert_eq!(result.event(), Some("event".into()));
    }

    #[test]
    fn test_handler_result_reexport_soft_error() {
        // Verify SoftError variant works (continues with event)
        let result: HandlerResult<String> =
            HandlerResult::soft_error("event".into(), "handler failed");
        assert!(!result.is_continue());
        assert!(result.is_soft_error());
        assert!(result.should_continue()); // SoftError allows continuation
        assert_eq!(result.soft_error_msg(), Some("handler failed"));
        assert_eq!(result.event(), Some("event".into()));
    }

    #[test]
    fn test_subscription_info() {
        let info = SubscriptionInfo::new(
            SubscriptionId::new(1),
            "test_handler",
            EventFilter::event_type("tool:*"),
        )
        .with_priority(50)
        .with_enabled(false);

        assert_eq!(info.id.inner(), 1);
        assert_eq!(info.name, "test_handler");
        assert_eq!(info.priority, 50);
        assert!(!info.enabled);
    }

    #[test]
    fn test_subscription_error_display() {
        let err = SubscriptionError::duplicate_name("handler");
        assert!(format!("{}", err).contains("handler"));
        assert!(format!("{}", err).contains("already exists"));

        let err = SubscriptionError::not_found(SubscriptionId::new(42));
        assert!(format!("{}", err).contains("sub_42"));
        assert!(format!("{}", err).contains("not found"));

        let err = SubscriptionError::invalid_filter("bad pattern");
        assert!(format!("{}", err).contains("bad pattern"));

        let err = SubscriptionError::unavailable("disconnected");
        assert!(format!("{}", err).contains("disconnected"));
    }
}
