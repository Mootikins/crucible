//! `AgentHandle` trait implementation for `DaemonAgentHandle`.
//!
//! All trait method implementations delegate to daemon RPC calls and update
//! locally-cached values. The `Agent::turn` stream (see `native_agent.rs`)
//! consumes the streaming receiver set up by the event router in `convert.rs`.

use async_trait::async_trait;
use crucible_core::interaction::InteractionEvent;
use crucible_core::traits::chat::{AgentHandle, ChatError, ChatResult, SessionKnobs};
use tokio::sync::mpsc;

use super::DaemonAgentHandle;
use crate::ChatResultExt;

#[async_trait]
impl AgentHandle for DaemonAgentHandle {
    async fn send_message_fire_and_forget(&mut self, message: String) -> ChatResult<()> {
        tracing::debug!(session_id = %self.session_id, "Sending message to daemon (fire-and-forget)");
        self.client
            .session_send_message(&self.session_id, &message, true)
            .await
            .map_err(|e| ChatError::Communication(format!("Failed to send message: {}", e)))?;
        Ok(())
    }

    fn take_interaction_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<InteractionEvent>> {
        self.interaction_rx.take()
    }

    fn session_id(&self) -> Option<&str> {
        Some(&self.session_id)
    }

    fn get_mode_id(&self) -> &str {
        &self.mode_id
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        // Propagate to the daemon-side agent: modes shape behavior there
        // (plan mode filters write tools). Setting only the local field made
        // TUI mode switches cosmetic for daemon-backed sessions.
        self.client
            .session_set_mode(&self.session_id, mode_id)
            .await
            .map_err(|e| ChatError::ModeChange(format!("session.set_mode failed: {}", e)))?;
        self.mode_id = mode_id.to_string();
        Ok(())
    }

    async fn apply_mode(&mut self, mode_id: &str) -> ChatResult<()> {
        // Daemon-internal mirror sync only. `set_mode_str` would RPC back
        // into the daemon (this handle's DaemonClient targets the same
        // daemon); when this handle type is cached inside the daemon's own
        // agent_cache (test setups, possible future in-process callers),
        // that round-trip re-enters `AgentManager::set_mode` while the
        // cached handle's mutex is still held. Just update the local mirror
        // — the authoritative caller already persisted and emitted.
        self.mode_id = mode_id.to_string();
        Ok(())
    }

    async fn clear_history(&mut self) -> ChatResult<()> {
        // ACP sessions own their conversation state inside the spawned
        // agent process. The session_end+session_create dance below would
        // hijack the ACP session into an internal one (agent_type: None),
        // so refuse and let the TUI surface the error.
        if self
            .cached_agent_config
            .as_ref()
            .is_some_and(|a| a.agent_type == "acp")
        {
            return Err(ChatError::NotSupported(
                "ACP agents manage their own history; clearing would require restarting the agent"
                    .into(),
            ));
        }

        tracing::info!(session_id = %self.session_id, "Clearing session — ending old, creating new");

        let _ = self.client.session_unsubscribe(&[&self.session_id]).await;
        let _ = self.client.session_end(&self.session_id).await;

        let (Some(kiln), Some(ws)) = (&self.kiln, &self.workspace) else {
            return Err(ChatError::Internal(
                "Cannot create new session: missing kiln or workspace".into(),
            ));
        };

        let result = self
            .client
            .session_create(crate::rpc_client::client::SessionCreateParams {
                session_type: "chat".to_string(),
                kilns: vec![kiln.clone()],
                workspace: Some(ws.clone()),
                recording_mode: None,
                recording_path: None,
                agent_type: None,
                isolation: None,
            })
            .await
            .chat_comm()?;

        let Some(new_id) = result["session_id"].as_str() else {
            return Err(ChatError::Internal(
                "No session_id in session_create response".into(),
            ));
        };
        let new_id = new_id.to_string();

        if let Some(agent_config) = &self.cached_agent_config {
            let mut config = agent_config.clone();
            if let Some(model) = &self.cached_model {
                config.model = model.clone();
            }
            if let Some(enabled) = self.cached_precognition {
                config.precognition_enabled = enabled;
            }
            if let Err(e) = self.client.session_configure_agent(&new_id, &config).await {
                tracing::warn!(error = %e, "Failed to configure agent on new session");
            }
        }

        if let Err(e) = self.client.session_subscribe(&[&new_id]).await {
            tracing::warn!(error = %e, "Failed to subscribe to new session");
        }

        tracing::info!(old = %self.session_id, new = %new_id, "Session switched");
        self.session_id = new_id.clone();
        let _ = self.router_session_id.send(new_id);
        Ok(())
    }

    async fn cancel(&self) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, "Cancelling agent via daemon");
        self.client
            .session_cancel(&self.session_id)
            .await
            .chat_comm()?;
        Ok(())
    }

    async fn interaction_respond(
        &mut self,
        request_id: String,
        response: crucible_core::interaction::InteractionResponse,
    ) -> ChatResult<()> {
        tracing::info!(
            session_id = %self.session_id,
            request_id = %request_id,
            "Sending interaction response via daemon"
        );
        self.client
            .session_interaction_respond(&self.session_id, &request_id, response)
            .await
            .map_err(|e| {
                ChatError::Communication(format!("Failed to send interaction response: {}", e))
            })
    }

    async fn undo(&mut self, count: usize) -> ChatResult<Vec<crucible_core::types::UndoSummary>> {
        tracing::info!(session_id = %self.session_id, count = count, "Undoing agent turns via daemon");
        self.client
            .session_undo(&self.session_id, count)
            .await
            .map_err(|e| ChatError::Communication(format!("Failed to undo: {}", e)))
    }
}

#[async_trait]
impl SessionKnobs for DaemonAgentHandle {
    /// A proxy handle was not built with a prompt; the daemon's own handle
    /// holds it. There is no RPC to read it back, and nothing needs one.
    fn get_system_prompt(&self) -> Option<String> {
        None
    }

    async fn switch_model(&mut self, model_id: &str) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, model = %model_id, "Switching model via daemon");
        self.client
            .session_switch_model(&self.session_id, model_id)
            .await
            .chat_comm()?;
        self.cached_model = Some(model_id.to_string());
        Ok(())
    }

    fn current_model(&self) -> Option<&str> {
        self.cached_model.as_deref()
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        match self.client.session_list_models(&self.session_id).await {
            Ok(models) => models,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch models from daemon");
                Vec::new()
            }
        }
    }

    async fn fetch_available_modes(&mut self) -> Vec<String> {
        match self.client.session_list_modes(&self.session_id).await {
            Ok(state) => state.modes.into_iter().map(|m| m.id).collect(),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch modes from daemon");
                Vec::new()
            }
        }
    }

    async fn set_context_budget(&mut self, budget: Option<usize>) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, context_budget = ?budget, "Setting context_budget via daemon");
        self.client
            .session_set_context_budget(&self.session_id, budget)
            .await
            .chat_comm()?;
        self.cached_context_budget = budget;
        Ok(())
    }

    fn get_context_budget(&self) -> Option<usize> {
        self.cached_context_budget
    }

    async fn set_context_strategy(
        &mut self,
        strategy: crucible_core::session::ContextStrategy,
    ) -> ChatResult<()> {
        let strategy_str = strategy.to_string();
        tracing::info!(session_id = %self.session_id, context_strategy = %strategy_str, "Setting context_strategy via daemon");
        self.client
            .session_set_context_strategy(&self.session_id, &strategy_str)
            .await
            .chat_comm()?;
        self.cached_context_strategy = Some(strategy_str);
        Ok(())
    }

    fn get_context_strategy(&self) -> crucible_core::session::ContextStrategy {
        self.cached_context_strategy
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or_default()
    }

    async fn set_precognition(&mut self, enabled: bool) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, precognition = enabled, "Setting precognition via daemon");
        self.client
            .session_set_precognition(&self.session_id, enabled)
            .await
            .chat_comm()?;
        self.cached_precognition = Some(enabled);
        Ok(())
    }

    fn get_precognition(&self) -> bool {
        self.cached_precognition.unwrap_or(true)
    }
}
