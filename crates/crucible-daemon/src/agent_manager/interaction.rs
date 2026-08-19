//! Client-facing interactions the daemon parks on: `cru.ui`'s daemon half.
//!
//! Lives beside `AgentManager` rather than inside `mod.rs` for the reason
//! `residue.rs` does — it names private fields, and a child module sees its
//! parent's privates.
//!
//! The permission twin is `messaging/permission.rs`. The two are deliberately
//! separate: an unanswered permission must resolve to *deny*, a decision the
//! gate acts on, while an unanswered question resolves to *cancelled*, which
//! the asker interprets. One code path carrying both would need a type that
//! means "deny" and "cancelled" at once, and there isn't one.

use super::{AgentError, AgentManager, PermissionId};
use crate::protocol::SessionEventMessage;
use crucible_core::interaction::{InteractionRequest, InteractionResponse};
use tokio::sync::{broadcast, oneshot};
use tracing::debug;

/// A non-permission interaction a caller is parked on.
///
/// The permission twin (`super::PendingPermission`) resolves to `PermResponse`, which has a
/// meaningful default (deny). This one resolves to `InteractionResponse`,
/// whose no-answer case is [`InteractionResponse::Cancelled`] — a distinct
/// value the asker interprets, not a decision the daemon makes for it.
pub(crate) struct PendingInteraction {
    /// Visible to `slot.rs`, which lists what a session still owes an answer
    /// to without consuming the waiter.
    pub(super) request: InteractionRequest,
    response_tx: oneshot::Sender<InteractionResponse>,
}

impl AgentManager {
    /// Ask whichever client is attached, and park until it answers.
    ///
    /// The permission path's shape (`messaging/permission.rs:137`) generalized
    /// to the other six [`InteractionRequest`] variants: mint an id, register
    /// the waiter, emit `interaction_requested`, await the oneshot.
    ///
    /// Deliberately **not** run through `PermissionSerializer`. That exists so
    /// permission modals open one at a time in arrival order, which is right
    /// for a gate the agent is blocked on and wrong here — two plugins asking
    /// unrelated questions must not queue behind each other, and a plugin that
    /// asks from inside a permission handler would deadlock against it.
    ///
    /// Returns [`InteractionResponse::Cancelled`] on timeout or on a dropped
    /// sender. Both mean "nobody answered", and collapsing them is deliberate:
    /// a caller that must tell them apart is asking the wrong question of a
    /// UI that may have no user in front of it at all.
    pub async fn request_interaction(
        &self,
        session_id: &str,
        request: InteractionRequest,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        timeout: std::time::Duration,
    ) -> Result<InteractionResponse, AgentError> {
        // Existence is checked before the id is minted so a bad session id is
        // an error rather than a request nothing will ever answer.
        self.get_session(session_id)?;

        let request_id = format!("ix-{}", uuid::Uuid::new_v4());
        let (response_tx, response_rx) = oneshot::channel();
        let slot = self.slot(session_id);
        slot.insert_interaction(
            request_id.clone(),
            PendingInteraction {
                request: request.clone(),
                response_tx,
            },
        );

        if !crate::event_emitter::emit_event(
            event_tx,
            SessionEventMessage::interaction_requested(session_id, &request_id, &request),
        ) {
            // No client is listening, so nothing will ever answer. Reap the
            // registration now instead of parking for the full timeout on a
            // question nobody was shown.
            slot.take_interaction(&request_id);
            debug!(
                session_id = %session_id,
                request_id = %request_id,
                "no subscribers for interaction_requested; cancelling immediately"
            );
            return Ok(InteractionResponse::Cancelled);
        }

        match tokio::time::timeout(timeout, response_rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                slot.take_interaction(&request_id);
                debug!(
                    session_id = %session_id,
                    request_id = %request_id,
                    "interaction channel closed before response"
                );
                Ok(InteractionResponse::Cancelled)
            }
            Err(_) => {
                slot.take_interaction(&request_id);
                debug!(
                    session_id = %session_id,
                    request_id = %request_id,
                    "interaction request timed out"
                );
                Ok(InteractionResponse::Cancelled)
            }
        }
    }

    /// Deliver a client's answer to a parked [`Self::request_interaction`].
    pub fn respond_to_interaction(
        &self,
        session_id: &str,
        request_id: &str,
        response: InteractionResponse,
    ) -> Result<(), AgentError> {
        let pending = self
            .existing_slot(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?
            .take_interaction(request_id)
            .ok_or_else(|| AgentError::PermissionNotFound(request_id.to_string()))?;

        let _ = pending.response_tx.send(response);

        debug!(
            session_id = %session_id,
            request_id = %request_id,
            "Responded to interaction request"
        );

        Ok(())
    }

    /// Every non-permission interaction any session is waiting on.
    pub fn list_all_pending_interactions(&self) -> Vec<(String, PermissionId, InteractionRequest)> {
        self.slots
            .iter()
            .flat_map(|entry| {
                let session_id = entry.key().clone();
                entry
                    .value()
                    .list_interactions()
                    .into_iter()
                    .map(move |(id, request)| (session_id.clone(), id, request))
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}
