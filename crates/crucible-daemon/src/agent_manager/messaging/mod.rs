use super::*;
use crucible_core::types::ToolSource;

pub(crate) mod gate_decision;
mod isolation_gate;
pub(in crate::agent_manager) mod permission;
pub(crate) mod review_capture;
pub(in crate::agent_manager) mod send;
pub(in crate::agent_manager) mod stream;
mod tool_call;
pub(in crate::agent_manager) mod tool_hooks;

impl AgentManager {
    fn format_tool_source(source: &ToolSource) -> String {
        match source {
            ToolSource::Core => "Core".to_string(),
            ToolSource::Crucible => "Crucible".to_string(),
            ToolSource::Mcp { server } => format!("Mcp:{server}"),
            ToolSource::Plugin { name } => format!("Plugin:{name}"),
            ToolSource::Acp { agent } => format!("Acp:{agent}"),
        }
    }

    pub async fn cancel(&self, session_id: &str) -> bool {
        // Cascade to delegated children first: a cancelled parent turn must
        // not leave its children running (fire-and-forget children of a
        // cancelled turn are orphans by definition). Spawned so a child's
        // own cancel (which routes back through this method with the child's
        // id) never recurses on the stack; children have no grandchildren
        // (delegation_config is cleared on spawn), so the cascade is finite.
        {
            let service = self.delegation_service.clone();
            let session_id = session_id.to_string();
            tokio::spawn(async move {
                service.cancel_children_of(&session_id).await;
            });
        }

        // Drop any pending permission `oneshot::Sender`s for this session so
        // their receivers Err out immediately and the caller parked in
        // `prompt_user` releases the per-session prompt lock. Without
        // this, partial cancel (user hits Esc) leaves prompts dangling with
        // no limit, blocking subsequent prompts behind them.
        let dropped_pending = self
            .existing_slot(session_id)
            .map(|slot| slot.drop_permissions())
            .unwrap_or(0);
        if dropped_pending > 0 {
            debug!(
                session_id = %session_id,
                count = dropped_pending,
                "Dropped pending permission senders on cancel"
            );
        }

        // Same reasoning for non-permission interactions: a plugin parked on
        // `request_interaction` when the user cancels must be released now,
        // not at its timeout.
        let dropped_interactions = self
            .existing_slot(session_id)
            .map(|slot| slot.drop_interactions())
            .unwrap_or(0);
        if dropped_interactions > 0 {
            debug!(
                session_id = %session_id,
                count = dropped_interactions,
                "Dropped pending interaction senders on cancel"
            );
        }

        // Signal the turn and take its handle WITHOUT vacating the slot. The
        // slot is the session's one-turn claim, and the turn task releases it
        // itself on every exit path — after its last event was broadcast.
        // Removing the entry here used to free the claim while the task was
        // still winding down, so a send arriving in that window was admitted
        // beside a live stream: two concurrent turns, and the new turn's
        // user_message recorded while the old turn's tail was still emitting.
        let task_handle = match self.request_state.get_mut(session_id) {
            Some(mut state) => {
                if let Some(cancel_tx) = state.cancel_tx.take() {
                    let _ = cancel_tx.send(());
                }
                state.task_handle.take()
            }
            None => {
                if dropped_pending > 0 {
                    // No active request, but we did clear stale prompts.
                    info!(session_id = %session_id, "Request cancelled");
                    return true;
                }
                warn!(session_id = %session_id, "No active request to cancel");
                return false;
            }
        };

        if let Some(handle) = task_handle {
            // Give task 500ms to respond to cancellation signal before force-aborting
            match tokio::time::timeout(std::time::Duration::from_millis(500), handle).await {
                Ok(Ok(())) => debug!(session_id = %session_id, "Task completed gracefully"),
                Ok(Err(e)) => warn!(session_id = %session_id, error = %e, "Task panicked"),
                Err(_) => {
                    debug!(session_id = %session_id, "Task did not respond to cancellation, was aborted");
                }
            }
        }

        // Release the slot only if the task has not already (its own tail
        // removes it) and no newer send has claimed it (a fresh entry carries
        // its own task_handle). This covers a task that panicked before its
        // tail or outlived the grace — without it, one wedged turn would hold
        // the session's slot forever.
        let stale = self
            .request_state
            .get(session_id)
            .is_some_and(|state| state.task_handle.is_none());
        if stale {
            self.request_state.remove(session_id);
        }

        info!(session_id = %session_id, "Request cancelled");
        true
    }
}
