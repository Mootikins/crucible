//! Registry for pending interaction requests awaiting responses.
//!
//! Provides correlation between `InteractionRequest` events and their responses,
//! allowing callers to await responses to specific requests.

use std::collections::HashMap;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::interaction::InteractionResponse;

/// Registry for pending interaction requests.
///
/// When a script or tool needs to ask the user a question, it:
/// 1. Creates an `AskBatch` with a UUID
/// 2. Registers with the registry, getting a `Receiver`
/// 3. Emits the `InteractionRequested` event
/// 4. Awaits on the receiver
///
/// The UI layer:
/// 1. Receives the `InteractionRequested` event
/// 2. Shows the appropriate dialog
/// 3. Collects user response
/// 4. Calls `complete()` on the registry
///
/// # Example
///
/// ```ignore
/// let mut registry = InteractionRegistry::new();
/// let batch = AskBatch::new().question(...);
/// let rx = registry.register(batch.id);
///
/// emit_event(SessionEvent::InteractionRequested { ... });
///
/// let response = rx.await?;
/// ```
#[derive(Default)]
pub struct InteractionRegistry {
    pending: HashMap<Uuid, oneshot::Sender<InteractionResponse>>,
}

impl InteractionRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a pending interaction, returns receiver for the response.
    ///
    /// The caller should await on the returned receiver after emitting
    /// the corresponding `InteractionRequested` event.
    pub fn register(&mut self, id: Uuid) -> oneshot::Receiver<InteractionResponse> {
        let (tx, rx) = oneshot::channel();
        self.pending.insert(id, tx);
        rx
    }

    /// Complete a pending interaction with a response.
    ///
    /// Returns `true` if the interaction was found and completed,
    /// `false` if no pending interaction with that ID exists.
    pub fn complete(&mut self, id: Uuid, response: InteractionResponse) -> bool {
        if let Some(tx) = self.pending.remove(&id) {
            tx.send(response).is_ok()
        } else {
            false
        }
    }

    /// Check if an interaction is pending.
    pub fn is_pending(&self, id: Uuid) -> bool {
        self.pending.contains_key(&id)
    }

    /// Get the number of pending interactions.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Cancel a pending interaction.
    ///
    /// The receiver will get a `RecvError` when it tries to await.
    pub fn cancel(&mut self, id: Uuid) -> bool {
        self.pending.remove(&id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::{AskBatch, AskBatchResponse, QuestionAnswer};

    #[tokio::test]
    async fn registry_correlates_request_response() {
        let mut registry = InteractionRegistry::new();
        let batch = AskBatch::new();
        let id = batch.id;

        let rx = registry.register(id);
        assert!(registry.is_pending(id));
        assert_eq!(registry.pending_count(), 1);

        let response = AskBatchResponse::new(id).answer(QuestionAnswer::choice(0));
        assert!(registry.complete(id, InteractionResponse::AskBatch(response)));

        assert!(!registry.is_pending(id));

        let received = rx.await.unwrap();
        assert!(matches!(received, InteractionResponse::AskBatch(_)));
    }

    #[test]
    fn complete_unknown_id_returns_false() {
        let mut registry = InteractionRegistry::new();
        let unknown_id = Uuid::new_v4();
        let response = AskBatchResponse::cancelled(unknown_id);

        assert!(!registry.complete(unknown_id, InteractionResponse::AskBatch(response)));
    }

    #[tokio::test]
    async fn cancel_drops_sender() {
        let mut registry = InteractionRegistry::new();
        let id = Uuid::new_v4();

        let rx = registry.register(id);
        assert!(registry.cancel(id));
        assert!(!registry.is_pending(id));

        // Receiver should error since sender was dropped
        assert!(rx.await.is_err());
    }

    #[test]
    fn multiple_pending_interactions() {
        let mut registry = InteractionRegistry::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        let _rx1 = registry.register(id1);
        let _rx2 = registry.register(id2);
        let _rx3 = registry.register(id3);

        assert_eq!(registry.pending_count(), 3);
        assert!(registry.is_pending(id1));
        assert!(registry.is_pending(id2));
        assert!(registry.is_pending(id3));

        // Complete one
        let response = AskBatchResponse::new(id2).answer(QuestionAnswer::choice(0));
        registry.complete(id2, InteractionResponse::AskBatch(response));

        assert_eq!(registry.pending_count(), 2);
        assert!(!registry.is_pending(id2));
    }
}
