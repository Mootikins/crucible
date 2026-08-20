//! The pending-permission registry, and routing a client's answer to it.
//!
//! Its own module beside `AgentManager` for the reason `interaction.rs` and
//! `residue.rs` are — it names private fields, and a child module sees its
//! parent's privates.
//!
//! The pairing with `interaction.rs` is the point.
//! [`AgentManager::deliver_client_reply`] is the one entry both registries
//! share: they carry the same key type and answer the same wire method
//! (`session.interaction_respond`), so the reply alone cannot say which one it
//! belongs to. Keeping the two halves in sibling files, with the router named
//! here beside the registry it disambiguates, is what makes "route by
//! ownership, never by the reply's shape" readable rather than a comment.

use crucible_core::interaction::{PermRequest, PermResponse};
use tokio::sync::oneshot;
use tracing::debug;

use super::{AgentError, AgentManager, PendingPermission, PermissionId};

impl AgentManager {
    #[allow(dead_code)] // permission system API, exercised by tests
    pub fn await_permission(
        &self,
        session_id: &str,
        request: PermRequest,
    ) -> (PermissionId, oneshot::Receiver<PermResponse>) {
        let permission_id = format!("perm-{}", uuid::Uuid::new_v4());
        let (response_tx, response_rx) = oneshot::channel();

        let pending = PendingPermission {
            request,
            response_tx,
        };

        self.slot(session_id)
            .insert_permission(permission_id.clone(), pending);

        debug!(
            session_id = %session_id,
            permission_id = %permission_id,
            "Created pending permission request"
        );

        (permission_id, response_rx)
    }

    /// Deliver a client's answer to whichever registry is holding its id.
    ///
    /// Routing is by OWNERSHIP, never by the reply's own shape. The two
    /// registries share a key type and one wire method
    /// (`session.interaction_respond`), so the reply alone cannot say which one
    /// it belongs to — and matching on it got this wrong in both directions: a
    /// `Cancelled` answering a permission prompt went to the interactions map
    /// and stalled the prompt for its full timeout, while a differently-shaped
    /// reply to a question could reach the wrong waiter.
    ///
    /// A permission that is cancelled resolves to *deny*, stated here once,
    /// because that is the answer the gate acts on and the asker never sees.
    pub fn deliver_client_reply(
        &self,
        session_id: &str,
        request_id: &str,
        response: crucible_core::interaction::InteractionResponse,
    ) -> Result<(), AgentError> {
        use crucible_core::interaction::InteractionResponse;

        let slot = self
            .existing_slot(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        if slot.holds_permission(request_id) {
            let perm = match response {
                InteractionResponse::Permission(perm) => perm,
                // Any other shape answering a permission prompt is a refusal.
                // A client that dismissed the dialog did not approve anything.
                _ => PermResponse::deny_with_reason("permission prompt was dismissed"),
            };
            return self.respond_to_permission(session_id, request_id, perm);
        }

        self.respond_to_interaction(session_id, request_id, response)
    }

    pub fn respond_to_permission(
        &self,
        session_id: &str,
        permission_id: &str,
        response: PermResponse,
    ) -> Result<(), AgentError> {
        let pending = self
            .existing_slot(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?
            .take_permission(permission_id)
            .ok_or_else(|| AgentError::PermissionNotFound(permission_id.to_string()))?;

        let _ = pending.response_tx.send(response);

        debug!(
            session_id = %session_id,
            permission_id = %permission_id,
            "Responded to permission request"
        );

        Ok(())
    }

    #[allow(dead_code)] // permission system API, exercised by tests
    pub fn get_pending_permission(
        &self,
        session_id: &str,
        permission_id: &str,
    ) -> Option<PermRequest> {
        self.existing_slot(session_id)
            .and_then(|slot| slot.permission_request(permission_id))
    }

    #[allow(dead_code)] // permission system API, exercised by tests
    pub fn list_pending_permissions(&self, session_id: &str) -> Vec<(PermissionId, PermRequest)> {
        self.existing_slot(session_id)
            .map(|slot| slot.list_permissions())
            .unwrap_or_default()
    }

    /// All pending permission prompts across every session. The web Inbox
    /// needs the aggregate view: a session waiting on a permission must
    /// surface even when no browser tab is subscribed to its event stream.
    pub fn list_all_pending_permissions(&self) -> Vec<(String, PermissionId, PermRequest)> {
        self.slots
            .iter()
            .flat_map(|entry| {
                let session_id = entry.key().clone();
                entry
                    .value()
                    .list_permissions()
                    .into_iter()
                    .map(move |(id, request)| (session_id.clone(), id, request))
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}
