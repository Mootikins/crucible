//! `AgentHandle` trait implementation for `DaemonAgentHandle`.
//!
//! All trait method implementations delegate to daemon RPC calls and update
//! locally-cached values. The `send_message_stream` implementation drives the
//! streaming receiver set up by the event router in `convert.rs`.

use std::sync::Arc;

use async_trait::async_trait;
use crucible_core::interaction::InteractionEvent;
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatError, ChatResult};
use futures::stream::BoxStream;
use tokio::sync::mpsc;

use super::convert::session_event_to_chat_chunk;
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

    fn send_message_stream(
        &mut self,
        message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        let client = Arc::clone(&self.client);
        let session_id = self.session_id.clone();
        let streaming_rx = Arc::clone(&self.streaming_rx);

        Box::pin(async_stream::stream! {
            tracing::debug!(session_id = %session_id, "Sending message to daemon");

            let send_result = client
                .session_send_message(&session_id, &message, true)
                .await;
            if let Err(e) = send_result {
                tracing::error!(error = %e, "Failed to send message to daemon");
                yield Err(ChatError::Communication(format!("Failed to send message: {}", e)));
                return;
            }

            tracing::debug!(session_id = %session_id, "Message sent, waiting for streaming events");

            let mut rx = streaming_rx.lock().await;
            loop {
                match rx.recv().await {
                    Some(event) => {
                        tracing::trace!(
                            event_type = %event.event_type,
                            "Received streaming event"
                        );

                        if let Some(chunk) = session_event_to_chat_chunk(&event) {
                            tracing::debug!(
                                delta_len = chunk.delta.len(),
                                done = chunk.done,
                                has_tool_calls = chunk.tool_calls.is_some(),
                                "Converted event to ChatChunk"
                            );
                            if chunk.done {
                                if let Some(reason) = event.data.get("reason").and_then(|value| value.as_str()) {
                                    if let Some(stripped_reason) = reason.strip_prefix("error: ") {
                                        tracing::warn!(reason = %reason, "LLM stream ended with error");
                                        // Strip any ChatError variant Display prefix so TUI shows a single "Communication error: ..." prefix
                                        const CHAT_ERROR_PREFIXES: &[&str] = &[
                                            "Connection error: ", "Communication error: ", "Mode change error: ",
                                            "Command execution failed: ", "Invalid input: ",
                                            "Agent not available: ", "Internal error: ", "Invalid mode: ",
                                            "Operation not supported: ",
                                        ];
                                        let inner = CHAT_ERROR_PREFIXES
                                            .iter()
                                            .find_map(|prefix| stripped_reason.strip_prefix(prefix))
                                            .unwrap_or(stripped_reason);
                                        yield Err(ChatError::Communication(inner.to_string()));
                                        break;
                                    }
                                }
                                yield Ok(chunk);
                                tracing::debug!("Stream complete (done=true)");
                                break;
                            }
                            yield Ok(chunk);
                        } else {
                            tracing::debug!(event_type = %event.event_type, "Event not convertible to chunk");
                        }
                    }
                    None => {
                        tracing::warn!("Streaming channel closed unexpectedly");
                        yield Err(ChatError::Connection("Event channel closed".to_string()));
                        break;
                    }
                }
            }
        })
    }

    fn take_interaction_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<InteractionEvent>> {
        self.interaction_rx.take()
    }

    fn session_id(&self) -> Option<&str> {
        Some(&self.session_id)
    }

    fn as_undoable(&self) -> Option<&dyn crucible_core::traits::Undoable> {
        Some(self)
    }

    fn as_undoable_mut(&mut self) -> Option<&mut dyn crucible_core::traits::Undoable> {
        Some(self)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn get_mode_id(&self) -> &str {
        &self.mode_id
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
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

        let (Some(kiln), Some(ws)) = (&self.kiln_path, &self.workspace) else {
            return Err(ChatError::Internal(
                "Cannot create new session: missing kiln_path or workspace".into(),
            ));
        };

        let result = self
            .client
            .session_create(crate::rpc_client::client::SessionCreateParams {
                session_type: "chat".to_string(),
                kiln: kiln.clone(),
                workspace: Some(ws.clone()),
                connect_kilns: vec![],
                recording_mode: None,
                recording_path: None,
                agent_type: None,
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
            if let Some(temp) = self.cached_temperature {
                config.temperature = Some(temp);
            }
            if let Some(max) = self.cached_max_tokens {
                config.max_tokens = Some(max);
            }
            config.thinking_budget = self.cached_thinking_budget;
            config.max_iterations = self.cached_max_iterations;
            config.execution_timeout_secs = self.cached_execution_timeout;
            if let Some(count) = self.cached_precognition_results {
                config.precognition_results = count;
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

    async fn cancel(&self) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, "Cancelling agent via daemon");
        self.client
            .session_cancel(&self.session_id)
            .await
            .chat_comm()?;
        Ok(())
    }

    async fn set_thinking_budget(&mut self, budget: i64) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, budget = budget, "Setting thinking budget via daemon");
        self.client
            .session_set_thinking_budget(&self.session_id, Some(budget))
            .await
            .map_err(|e| {
                ChatError::Communication(format!("Failed to set thinking budget: {}", e))
            })?;
        self.cached_thinking_budget = Some(budget);
        Ok(())
    }

    fn get_thinking_budget(&self) -> Option<i64> {
        self.cached_thinking_budget
    }

    async fn set_system_prompt(&mut self, prompt: &str) -> ChatResult<()> {
        tracing::debug!(session_id = %self.session_id, "Setting system prompt via daemon");
        self.client
            .session_set_system_prompt(&self.session_id, prompt)
            .await
            .map_err(|e| ChatError::Communication(format!("Failed to set system prompt: {}", e)))?;
        self.cached_system_prompt = Some(prompt.to_string());
        Ok(())
    }

    fn get_system_prompt(&self) -> Option<String> {
        self.cached_system_prompt.clone()
    }

    async fn set_temperature(&mut self, temperature: f64) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, temperature = temperature, "Setting temperature via daemon");
        self.client
            .session_set_temperature(&self.session_id, temperature)
            .await
            .chat_comm()?;
        self.cached_temperature = Some(temperature);
        Ok(())
    }

    fn get_temperature(&self) -> Option<f64> {
        self.cached_temperature
    }

    async fn set_max_tokens(&mut self, max_tokens: Option<u32>) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, max_tokens = ?max_tokens, "Setting max_tokens via daemon");
        self.client
            .session_set_max_tokens(&self.session_id, max_tokens)
            .await
            .chat_comm()?;
        self.cached_max_tokens = max_tokens;
        Ok(())
    }

    fn get_max_tokens(&self) -> Option<u32> {
        self.cached_max_tokens
    }

    async fn set_max_iterations(&mut self, max_iterations: Option<u32>) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, max_iterations = ?max_iterations, "Setting max_iterations via daemon");
        self.client
            .session_set_max_iterations(&self.session_id, max_iterations)
            .await
            .chat_comm()?;
        self.cached_max_iterations = max_iterations;
        Ok(())
    }

    fn get_max_iterations(&self) -> Option<u32> {
        self.cached_max_iterations
    }

    async fn set_execution_timeout(&mut self, timeout_secs: Option<u64>) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, timeout_secs = ?timeout_secs, "Setting execution_timeout via daemon");
        self.client
            .session_set_execution_timeout(&self.session_id, timeout_secs)
            .await
            .chat_comm()?;
        self.cached_execution_timeout = timeout_secs;
        Ok(())
    }

    fn get_execution_timeout(&self) -> Option<u64> {
        self.cached_execution_timeout
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

    async fn set_context_window(&mut self, window: Option<usize>) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, context_window = ?window, "Setting context_window via daemon");
        self.client
            .session_set_context_window(&self.session_id, window)
            .await
            .chat_comm()?;
        self.cached_context_window = window;
        Ok(())
    }

    fn get_context_window(&self) -> Option<usize> {
        self.cached_context_window
    }

    async fn set_output_validation(
        &mut self,
        validation: crucible_core::session::OutputValidation,
    ) -> ChatResult<()> {
        let validation_str = validation.to_string();
        tracing::info!(session_id = %self.session_id, output_validation = %validation_str, "Setting output_validation via daemon");
        self.client
            .session_set_output_validation(&self.session_id, &validation_str)
            .await
            .chat_comm()?;
        self.cached_output_validation = Some(validation_str);
        Ok(())
    }

    fn get_output_validation(&self) -> &crucible_core::session::OutputValidation {
        // We can't return a reference to a parsed value from cached string,
        // so use a static for the default and parse-match for known variants
        static NONE: crucible_core::session::OutputValidation =
            crucible_core::session::OutputValidation::None;
        static JSON: crucible_core::session::OutputValidation =
            crucible_core::session::OutputValidation::Json;
        match self.cached_output_validation.as_deref() {
            Some("json") => &JSON,
            Some("none") | None => &NONE,
            // For regex variants we can't return a reference to a local.
            // Fall back to None; the daemon holds the authoritative value.
            Some(_) => &NONE,
        }
    }

    async fn set_validation_retries(&mut self, retries: u32) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, validation_retries = retries, "Setting validation_retries via daemon");
        self.client
            .session_set_validation_retries(&self.session_id, retries)
            .await
            .chat_comm()?;
        self.cached_validation_retries = Some(retries);
        Ok(())
    }

    fn get_validation_retries(&self) -> u32 {
        self.cached_validation_retries.unwrap_or(3)
    }

    async fn set_precognition_results(&mut self, count: usize) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, precognition_results = count, "Setting precognition_results via daemon");
        self.client
            .session_set_precognition_results(&self.session_id, count)
            .await
            .chat_comm()?;
        self.cached_precognition_results = Some(count);
        Ok(())
    }

    fn get_precognition_results(&self) -> usize {
        self.cached_precognition_results.unwrap_or(5)
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
}

#[async_trait]
impl crucible_core::traits::Undoable for DaemonAgentHandle {
    async fn undo(&mut self, count: usize) -> ChatResult<Vec<crucible_core::types::UndoSummary>> {
        tracing::info!(session_id = %self.session_id, count = count, "Undoing agent turns via daemon");
        self.client
            .session_undo(&self.session_id, count)
            .await
            .map_err(|e| ChatError::Communication(format!("Failed to undo: {}", e)))
    }

    // `can_undo` and `undo_depth` are sync so we can't hit the daemon here.
    // The authoritative values live on the daemon side (session.can_undo /
    // session.undo_depth RPCs). No caller in the TUI consults these — the
    // undo flow goes straight to `undo()` and inspects the returned summary.
    // If these ever become hot paths, cache them from undo events.
    fn can_undo(&self) -> bool {
        false
    }

    fn undo_depth(&self) -> usize {
        0
    }
}
