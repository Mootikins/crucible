use crucible_core::events::SessionEvent;
use crucible_core::interaction::{InteractionRequest, InteractionResponse};
use crucible_core::InteractionRegistry;
use std::sync::{Arc, Mutex};

use super::types::{LuaAskBatch, LuaAskBatchResponse};

/// Callback type for pushing session events.
///
/// This abstraction allows the context to work with any event system
/// (EventRing, channels, etc.) without creating circular dependencies.
pub type EventPushCallback = Arc<dyn Fn(SessionEvent) + Send + Sync>;

/// Context for ask_user function execution in Lua.
///
/// Holds references to the interaction registry and an event push callback
/// needed to submit requests and wait for responses.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_lua::ask::LuaAskContext;
/// use crucible_core::InteractionRegistry;
/// use std::sync::{Arc, Mutex};
///
/// let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
/// let push_fn: EventPushCallback = Arc::new(|event| {
///     // Push event to your event system (EventRing, channel, etc.)
///     my_event_ring.push(event);
/// });
///
/// let context = LuaAskContext::new(registry, push_fn);
/// ```
#[derive(Clone)]
pub struct LuaAskContext {
    pub(super) registry: Arc<Mutex<InteractionRegistry>>,
    pub(super) push_event: EventPushCallback,
}

impl LuaAskContext {
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

    /// Submit an ask batch and wait for the response.
    ///
    /// This function:
    /// 1. Registers the batch ID with the registry (gets a receiver)
    /// 2. Pushes an InteractionRequested event via the callback
    /// 3. Blocks waiting for the response via the receiver
    ///
    /// # Note
    ///
    /// This blocks the calling thread until the TUI/UI completes the interaction.
    /// In Lua, this is typically called from a script that runs in a separate
    /// thread from the main event loop.
    pub fn ask_user(&self, batch: LuaAskBatch) -> Result<LuaAskBatchResponse, LuaAskError> {
        let core_batch = batch.inner.clone();
        let id = core_batch.id;

        // Register with the registry to get a receiver
        let rx = {
            let mut guard = self
                .registry
                .lock()
                .map_err(|e| LuaAskError::new(format!("Registry lock failed: {}", e)))?;
            guard.register(id)
        };

        // Push InteractionRequested event via callback
        (self.push_event)(SessionEvent::InteractionRequested {
            request_id: id.to_string(),
            request: InteractionRequest::AskBatch(core_batch),
        });

        // Wait for response (blocking)
        // Note: This blocks the current thread until the TUI completes the interaction
        let response = rx
            .blocking_recv()
            .map_err(|_| LuaAskError::new("Interaction was cancelled or dropped".to_string()))?;

        match response {
            InteractionResponse::AskBatch(batch_response) => Ok(LuaAskBatchResponse {
                inner: batch_response,
            }),
            InteractionResponse::Cancelled => Ok(LuaAskBatchResponse::cancelled(id)),
            _ => Err(LuaAskError::new(format!(
                "Unexpected response type: {:?}",
                response
            ))),
        }
    }
}

/// Error type for ask_user function.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct LuaAskError {
    /// Error message
    pub message: String,
}

impl LuaAskError {
    /// Create a new error with message.
    pub fn new(message: String) -> Self {
        Self { message }
    }
}
