//! Context for interaction requests with registry and event push callback.
//!
//! Provides a wrapper struct that holds both the interaction registry and
//! the callback for pushing events to the session event system.

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::events::SessionEvent;
use crate::interaction_registry::InteractionRegistry;

/// Type alias for event push callback.
pub type EventPushCallback = Arc<dyn Fn(SessionEvent) + Send + Sync>;

/// Context for interaction requests.
///
/// Holds the interaction registry for request-response correlation and
/// a callback for pushing events to the session event system.
///
/// # Example
///
/// ```ignore
/// let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
/// let push_event = Arc::new(|event| {
///     // Handle event
/// });
/// let context = InteractionContext::new(registry, push_event);
/// ```
#[derive(Clone)]
pub struct InteractionContext {
    /// Shared interaction registry for request-response correlation.
    pub registry: Arc<Mutex<InteractionRegistry>>,
    /// Callback to push SessionEvent to the event system.
    pub push_event: EventPushCallback,
}

impl InteractionContext {
    /// Create a new context with registry and event push callback.
    ///
    /// # Arguments
    ///
    /// * `registry` - Shared interaction registry for request-response correlation
    /// * `push_event` - Callback to push SessionEvent to the event system
    pub fn new(registry: Arc<Mutex<InteractionRegistry>>, push_event: EventPushCallback) -> Self {
        Self {
            registry,
            push_event,
        }
    }
}
