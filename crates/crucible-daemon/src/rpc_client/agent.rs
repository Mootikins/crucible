//! Daemon-backed agent handle implementation
//!
//! Implements `AgentHandle` by routing messages through the daemon's agent execution.
//! This allows the TUI to use daemon-managed agents transparently.

use std::sync::Arc;

use async_trait::async_trait;
use crucible_core::interaction::InteractionEvent;
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatError, ChatResult, ChatToolCall, ChatToolResult,
    PrecognitionNoteInfo,
};
use futures::stream::BoxStream;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::ChatResultExt;
use crate::{DaemonClient, SessionEvent};

/// Agent handle that routes messages through the daemon
///
/// This handle implements `AgentHandle` by:
/// 1. Sending messages via `session.send_message` RPC
/// 2. Subscribing to session events for streaming responses
/// 3. Converting `SessionEvent` back to `ChatChunk` for the TUI
/// 4. Routing interaction events to a separate channel for the TUI event loop
pub struct DaemonAgentHandle {
    client: Arc<DaemonClient>,
    session_id: String,
    router_session_id: Arc<tokio::sync::watch::Sender<String>>,
    streaming_rx: Arc<Mutex<mpsc::UnboundedReceiver<SessionEvent>>>,
    interaction_rx: Option<mpsc::UnboundedReceiver<InteractionEvent>>,
    mode_id: String,
    connected: bool,
    cached_model: Option<String>,
    cached_temperature: Option<f64>,
    cached_max_tokens: Option<u32>,
    cached_thinking_budget: Option<i64>,
    cached_system_prompt: Option<String>,
    kiln_path: Option<PathBuf>,
    workspace: Option<PathBuf>,
    cached_agent_config: Option<SessionAgent>,
    event_router_task: Option<JoinHandle<()>>,
}

impl DaemonAgentHandle {
    /// Create a new daemon agent handle with event routing
    ///
    /// Spawns a background task that routes incoming events:
    /// - Streaming events (text_delta, tool_call, etc.) go to the streaming channel
    /// - Interaction events (interaction_requested) go to the interaction channel
    pub fn new(
        client: Arc<DaemonClient>,
        session_id: String,
        event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    ) -> Self {
        let (streaming_tx, streaming_rx) = mpsc::unbounded_channel();
        let (interaction_tx, interaction_rx) = mpsc::unbounded_channel();

        let (session_id_tx, session_id_rx) = tokio::sync::watch::channel(session_id.clone());

        let event_router_task = tokio::spawn(async move {
            event_router(event_rx, streaming_tx, interaction_tx, session_id_rx).await;
        });

        Self {
            client,
            session_id,
            router_session_id: Arc::new(session_id_tx),
            streaming_rx: Arc::new(Mutex::new(streaming_rx)),
            interaction_rx: Some(interaction_rx),
            mode_id: "normal".to_string(),
            connected: true,
            cached_model: None,
            cached_temperature: None,
            cached_max_tokens: None,
            cached_thinking_budget: None,
            cached_system_prompt: None,
            kiln_path: None,
            workspace: None,
            cached_agent_config: None,
            event_router_task: Some(event_router_task),
        }
    }

    /// Create a daemon agent handle and subscribe to its session
    pub async fn new_and_subscribe(
        client: Arc<DaemonClient>,
        session_id: String,
        event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    ) -> ChatResult<Self> {
        tracing::debug!(session_id = %session_id, "Subscribing to daemon session events");

        client
            .session_subscribe(&[&session_id])
            .await
            .map_err(|e| ChatError::Connection(format!("Failed to subscribe: {}", e)))?;

        tracing::info!(session_id = %session_id, "Successfully subscribed to session events");

        let mut handle = Self::new(client.clone(), session_id.clone(), event_rx);

        // Fetch initial cached values from daemon (best-effort, default to None on failure)
        handle.cached_temperature = client
            .session_get_temperature(&session_id)
            .await
            .ok()
            .flatten();
        handle.cached_max_tokens = client
            .session_get_max_tokens(&session_id)
            .await
            .ok()
            .flatten();
        handle.cached_thinking_budget = client
            .session_get_thinking_budget(&session_id)
            .await
            .ok()
            .flatten();
        handle.cached_system_prompt = client
            .session_get_system_prompt(&session_id)
            .await
            .ok()
            .flatten();

        Ok(handle)
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn with_kiln_path(mut self, path: PathBuf) -> Self {
        self.kiln_path = Some(path);
        self
    }

    pub fn with_workspace(mut self, path: PathBuf) -> Self {
        self.workspace = Some(path);
        self
    }

    pub fn with_agent_config(mut self, config: SessionAgent) -> Self {
        self.cached_agent_config = Some(config);
        self
    }
}

/// Background task that routes events from daemon to appropriate channels
///
/// Uses a `watch::Receiver` for the session_id so `clear_history` can atomically
/// switch the router to a new session without restarting this task.
async fn event_router(
    mut event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    streaming_tx: mpsc::UnboundedSender<SessionEvent>,
    interaction_tx: mpsc::UnboundedSender<InteractionEvent>,
    session_id_rx: tokio::sync::watch::Receiver<String>,
) {
    while let Some(event) = event_rx.recv().await {
        let current_session_id = session_id_rx.borrow().clone();
        if event.session_id != current_session_id {
            tracing::trace!(
                event_session = %event.session_id,
                expected_session = %current_session_id,
                "Filtering event from different session in router"
            );
            continue;
        }

        if event.event_type == "interaction_requested" {
            if let (Some(request_id), Some(request_data)) = (
                event.data.get("request_id").and_then(|v| v.as_str()),
                event.data.get("request"),
            ) {
                match serde_json::from_value(request_data.clone()) {
                    Ok(request) => {
                        let interaction_event = InteractionEvent {
                            request_id: request_id.to_string(),
                            request,
                        };
                        if interaction_tx.send(interaction_event).is_err() {
                            tracing::debug!("Interaction channel closed");
                            break;
                        }
                        tracing::debug!(request_id = %request_id, "Routed interaction event");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to deserialize interaction request");
                    }
                }
            } else {
                tracing::warn!("Interaction event missing request_id or request data");
            }
        } else if streaming_tx.send(event).is_err() {
            tracing::debug!("Streaming channel closed");
            break;
        }
    }
    tracing::debug!("Event router task ended");
}

/// Convert a `text_delta` event to a ChatChunk.
fn convert_text_delta(event: &SessionEvent) -> Option<ChatChunk> {
    let content = event.data.get("content")?.as_str()?;
    Some(ChatChunk {
        delta: content.to_string(),
        ..Default::default()
    })
}

/// Convert a `thinking` event to a ChatChunk.
fn convert_thinking(event: &SessionEvent) -> Option<ChatChunk> {
    let content = event.data.get("content")?.as_str()?;
    Some(ChatChunk {
        reasoning: Some(content.to_string()),
        ..Default::default()
    })
}

/// Convert a `tool_call` event to a ChatChunk.
fn convert_tool_call(event: &SessionEvent) -> Option<ChatChunk> {
    let call_id = event.data.get("call_id").and_then(|v| v.as_str());
    let tool = event.data.get("tool")?.as_str()?;
    let args = event.data.get("args").cloned();

    Some(ChatChunk {
        tool_calls: Some(vec![ChatToolCall {
            name: tool.to_string(),
            arguments: args,
            id: call_id.map(String::from),
        }]),
        ..Default::default()
    })
}

/// Convert a `tool_result` event to a ChatChunk.
fn convert_tool_result(event: &SessionEvent) -> Option<ChatChunk> {
    let tool_name = event.data.get("tool").and_then(|v| v.as_str());
    let call_id = event.data.get("call_id").and_then(|v| v.as_str());
    let result = event.data.get("result")?;

    let call_id_str = call_id.map(String::from);
    let name = tool_name
        .map(String::from)
        .or_else(|| call_id_str.clone())
        .unwrap_or_else(|| "tool".to_string());

    let error = result
        .get("error")
        .and_then(|e| e.as_str())
        .map(String::from);

    let result_str = if error.is_some() {
        String::new()
    } else if result.is_string() {
        result.as_str().unwrap_or("").to_string()
    } else if let Some(inner) = result.get("result").and_then(|r| r.as_str()) {
        inner.to_string()
    } else {
        result.to_string()
    };

    Some(ChatChunk {
        tool_results: Some(vec![ChatToolResult {
            name,
            result: result_str,
            error,
            call_id: call_id_str,
        }]),
        ..Default::default()
    })
}

/// Convert a `message_complete` event to a ChatChunk with optional token usage.
fn convert_message_complete(event: &SessionEvent) -> Option<ChatChunk> {
    let usage = event
        .data
        .get("total_tokens")
        .and_then(|v| v.as_u64())
        .map(|total| {
            let prompt = event
                .data
                .get("prompt_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let completion = event
                .data
                .get("completion_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            crucible_core::traits::llm::TokenUsage {
                prompt_tokens: prompt as u32,
                completion_tokens: completion as u32,
                total_tokens: total as u32,
            }
        });
    Some(ChatChunk {
        done: true,
        usage,
        ..Default::default()
    })
}

/// Convert an `ended` event to a terminal ChatChunk.
fn convert_ended() -> ChatChunk {
    ChatChunk {
        delta: String::new(),
        done: true,
        tool_calls: None,
        tool_results: None,
        reasoning: None,
        usage: None,
        subagent_events: None,
        precognition_notes_count: None,
        precognition_notes: None,
    }
}

/// Convert a `precognition_complete` event to a ChatChunk.
fn convert_precognition_complete(event: &SessionEvent) -> Option<ChatChunk> {
    let notes_count = event
        .data
        .get("notes_count")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);
    let precognition_notes = event
        .data
        .get("notes")
        .and_then(|v| serde_json::from_value::<Vec<PrecognitionNoteInfo>>(v.clone()).ok());
    Some(ChatChunk {
        precognition_notes_count: notes_count,
        precognition_notes,
        ..Default::default()
    })
}

/// Convert a SessionEvent to a ChatChunk
///
/// Thin dispatcher that delegates to per-event-family helpers:
/// - `text_delta` / `thinking` → streaming content
/// - `tool_call` / `tool_result` → tool events
/// - `message_complete` / `ended` → completion signals
/// - `precognition_complete` → knowledge graph events
fn session_event_to_chat_chunk(event: &SessionEvent) -> Option<ChatChunk> {
    match event.event_type.as_str() {
        "text_delta" => convert_text_delta(event),
        "thinking" => convert_thinking(event),
        "tool_call" => convert_tool_call(event),
        "tool_result" => convert_tool_result(event),
        "message_complete" => convert_message_complete(event),
        "ended" => Some(convert_ended()),
        "precognition_complete" => convert_precognition_complete(event),
        _ => {
            tracing::debug!("Unknown session event type: {}", event.event_type);
            None
        }
    }
}

#[async_trait]
impl AgentHandle for DaemonAgentHandle {
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
                                            "Unknown command: ", "Command execution failed: ", "Invalid input: ",
                                            "Agent not available: ", "Internal error: ", "Invalid mode: ",
                                            "Invalid command: ", "Operation not supported: ",
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

    async fn clear_history(&mut self) {
        tracing::info!(session_id = %self.session_id, "Clearing session — ending old, creating new");

        let _ = self.client.session_unsubscribe(&[&self.session_id]).await;
        let _ = self.client.session_end(&self.session_id).await;

        let (Some(kiln), Some(ws)) = (&self.kiln_path, &self.workspace) else {
            tracing::warn!("Cannot create new session: missing kiln_path or workspace");
            return;
        };

        let result = match self
            .client
            .session_create(crate::rpc_client::client::SessionCreateParams {
                session_type: "chat".to_string(),
                kiln: kiln.clone(),
                workspace: Some(ws.clone()),
                connect_kilns: vec![],
                recording_mode: None,
                recording_path: None,
            })
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to create new session after clear");
                return;
            }
        };

        let Some(new_id) = result["session_id"].as_str() else {
            tracing::warn!("No session_id in create response");
            return;
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

impl Drop for DaemonAgentHandle {
    fn drop(&mut self) {
        if let Some(task) = self.event_router_task.take() {
            task.abort();
        }

        let client = Arc::clone(&self.client);
        let session_id = self.session_id.clone();
        // try_current() returns None if tokio runtime is gone (shutdown, sync context).
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Err(e) = client.session_end(&session_id).await {
                    tracing::debug!(session_id = %session_id, error = %e, "Failed to end session on drop");
                } else {
                    tracing::info!(session_id = %session_id, "Session ended on agent handle drop");
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_text_delta_conversion() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "text_delta".to_string(),
            data: json!({ "content": "Hello world" }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        assert_eq!(chunk.delta, "Hello world");
        assert!(!chunk.done);
        assert!(chunk.tool_calls.is_none());
        assert!(chunk.reasoning.is_none());
    }

    #[test]
    fn test_thinking_conversion() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "thinking".to_string(),
            data: json!({ "content": "Let me think..." }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        assert_eq!(chunk.delta, "");
        assert_eq!(chunk.reasoning, Some("Let me think...".to_string()));
        assert!(!chunk.done);
    }

    #[test]
    fn test_tool_call_conversion() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "tool_call".to_string(),
            data: json!({
                "call_id": "tc-123",
                "tool": "search",
                "args": { "query": "rust async" }
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        assert_eq!(chunk.delta, "");
        assert!(!chunk.done);

        let tool_calls = chunk.tool_calls.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "search");
        assert_eq!(tool_calls[0].id, Some("tc-123".to_string()));
    }

    #[test]
    fn test_tool_result_conversion() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "tool_result".to_string(),
            data: json!({
                "call_id": "tc-123",
                "result": "Found 5 results"
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        assert_eq!(chunk.delta, "");
        assert!(!chunk.done);

        let results = chunk.tool_results.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result, "Found 5 results");
    }

    #[test]
    fn test_message_complete_conversion() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "message_complete".to_string(),
            data: json!({
                "message_id": "msg-456",
                "full_response": "Complete response text"
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        assert!(chunk.done);
        assert_eq!(chunk.delta, "");
    }

    #[test]
    fn test_ended_conversion() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "ended".to_string(),
            data: json!({ "reason": "user_requested" }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        assert!(chunk.done);
    }

    #[test]
    fn test_ended_with_error_reason_detected() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "ended".to_string(),
            data: json!({ "reason": "error: connection refused" }),
        };

        let chunk = session_event_to_chat_chunk(&event).expect("ended should convert to chunk");
        assert!(chunk.done);
        let reason = event
            .data
            .get("reason")
            .and_then(|value| value.as_str())
            .expect("reason should be present");
        assert!(reason.starts_with("error: "));
    }

    #[test]
    fn test_ended_with_cancelled_reason_yields_done_chunk() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "ended".to_string(),
            data: json!({ "reason": "cancelled" }),
        };

        let chunk = session_event_to_chat_chunk(&event).expect("ended should convert to chunk");
        assert!(chunk.done);
    }

    #[test]
    fn test_ended_with_complete_reason_yields_done_chunk() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "ended".to_string(),
            data: json!({ "reason": "complete" }),
        };

        let chunk = session_event_to_chat_chunk(&event).expect("ended should convert to chunk");
        assert!(chunk.done);
    }
    #[test]
    fn test_error_with_communication_prefix_stripped() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "ended".to_string(),
            data: json!({ "reason": "error: Communication error: LLM timeout" }),
        };

        let chunk = session_event_to_chat_chunk(&event).expect("ended should convert to chunk");
        assert!(chunk.done);
        let reason = event
            .data
            .get("reason")
            .and_then(|value| value.as_str())
            .expect("reason should be present");
        assert!(reason.starts_with("error: "));
    }

    #[test]
    fn test_error_with_connection_prefix_stripped() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "ended".to_string(),
            data: json!({ "reason": "error: Connection error: refused" }),
        };

        let chunk = session_event_to_chat_chunk(&event).expect("ended should convert to chunk");
        assert!(chunk.done);
        let reason = event
            .data
            .get("reason")
            .and_then(|value| value.as_str())
            .expect("reason should be present");
        assert!(reason.starts_with("error: "));
    }

    #[test]
    fn test_error_with_internal_prefix_stripped() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "ended".to_string(),
            data: json!({ "reason": "error: Internal error: panic" }),
        };

        let chunk = session_event_to_chat_chunk(&event).expect("ended should convert to chunk");
        assert!(chunk.done);
        let reason = event
            .data
            .get("reason")
            .and_then(|value| value.as_str())
            .expect("reason should be present");
        assert!(reason.starts_with("error: "));
    }

    #[test]
    fn test_unknown_event_returns_none() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "unknown_event".to_string(),
            data: json!({}),
        };

        assert!(session_event_to_chat_chunk(&event).is_none());
    }

    #[test]
    fn test_malformed_event_returns_none() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "text_delta".to_string(),
            data: json!({}),
        };

        assert!(session_event_to_chat_chunk(&event).is_none());
    }

    #[test]
    fn test_error_format_parity_connection() {
        use crucible_core::traits::chat::ChatError;

        let daemon_err = ChatError::Connection("Event channel closed".to_string());
        let local_err = ChatError::Connection("Connection lost".to_string());

        let daemon_msg = daemon_err.to_string();
        let local_msg = local_err.to_string();

        assert!(
            daemon_msg.contains("Connection") || daemon_msg.contains("connection"),
            "Daemon error should mention connection: {}",
            daemon_msg
        );
        assert!(
            local_msg.contains("Connection") || local_msg.contains("connection"),
            "Local error should mention connection: {}",
            local_msg
        );
    }

    #[test]
    fn test_error_format_parity_communication() {
        use crucible_core::traits::chat::ChatError;

        let daemon_err = ChatError::Communication("Failed to send message: timeout".to_string());
        let local_err = ChatError::Communication("Rig LLM error: connection refused".to_string());

        let daemon_msg = daemon_err.to_string();
        let local_msg = local_err.to_string();

        assert!(!daemon_msg.is_empty(), "Daemon error should have message");
        assert!(!local_msg.is_empty(), "Local error should have message");

        assert!(
            !daemon_msg.contains("ChatError"),
            "Error display should not expose internal type: {}",
            daemon_msg
        );
        assert!(
            !local_msg.contains("ChatError"),
            "Error display should not expose internal type: {}",
            local_msg
        );
    }

    #[test]
    fn test_error_types_are_displayable() {
        use crucible_core::traits::chat::ChatError;

        let errors = vec![
            ChatError::Connection("test".to_string()),
            ChatError::Communication("test".to_string()),
            ChatError::InvalidMode("test".to_string()),
        ];

        for err in errors {
            let msg = err.to_string();
            assert!(
                !msg.is_empty(),
                "All ChatError variants should be displayable"
            );
            assert!(
                msg.len() < 1000,
                "Error messages should be reasonably sized for TUI display"
            );
        }
    }

    #[test]
    fn test_tool_result_with_object_result() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "tool_result".to_string(),
            data: json!({
                "call_id": "tc-123",
                "result": { "files": ["a.rs", "b.rs"], "count": 2 }
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        let results = chunk.tool_results.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].result.contains("files"));
        assert!(results[0].result.contains("count"));
    }

    #[test]
    fn test_tool_call_without_call_id() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "tool_call".to_string(),
            data: json!({
                "tool": "search",
                "args": { "query": "test" }
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        let tool_calls = chunk.tool_calls.unwrap();
        assert_eq!(tool_calls[0].name, "search");
        assert!(tool_calls[0].id.is_none());
    }

    #[test]
    fn test_tool_call_without_args() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "tool_call".to_string(),
            data: json!({
                "tool": "list_files"
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        let tool_calls = chunk.tool_calls.unwrap();
        assert_eq!(tool_calls[0].name, "list_files");
        assert!(tool_calls[0].arguments.is_none());
    }

    #[test]
    fn test_model_switched_event_conversion() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "model_switched".to_string(),
            data: json!({
                "model": "gpt-4",
                "provider": "openai"
            }),
        };

        let chunk = session_event_to_chat_chunk(&event);
        assert!(
            chunk.is_none(),
            "model_switched events should not produce chunks"
        );
    }

    #[test]
    fn test_tool_result_includes_tool_name() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "tool_result".to_string(),
            data: json!({
                "call_id": "tc-123",
                "tool": "read_file",
                "result": "file contents here"
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        let results = chunk.tool_results.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].name, "read_file",
            "tool_result should use tool name, not call_id"
        );
        assert_eq!(results[0].result, "file contents here");
    }

    #[test]
    fn test_tool_result_falls_back_to_call_id_when_no_tool_name() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "tool_result".to_string(),
            data: json!({
                "call_id": "tc-456",
                "result": "some result"
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        let results = chunk.tool_results.unwrap();
        assert_eq!(
            results[0].name, "tc-456",
            "Should fall back to call_id when tool name not provided"
        );
    }

    #[test]
    fn test_tool_result_unwraps_daemon_format() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "tool_result".to_string(),
            data: json!({
                "call_id": "tc-789",
                "tool": "bash",
                "result": { "result": "line1\nline2\nline3" }
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        let results = chunk.tool_results.unwrap();
        assert_eq!(results[0].name, "bash");
        assert_eq!(results[0].result, "line1\nline2\nline3");
        assert!(results[0].result.contains('\n'));
    }

    #[test]
    fn test_tool_result_with_error_extracts_error_field() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "tool_result".to_string(),
            data: json!({
                "call_id": "tc-denied",
                "tool": "bash",
                "result": { "error": "User denied permission to bash echo hello" }
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        let results = chunk.tool_results.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "bash");
        assert!(
            results[0].result.is_empty(),
            "Result should be empty when error is present"
        );
        assert_eq!(
            results[0].error,
            Some("User denied permission to bash echo hello".to_string())
        );
    }

    #[test]
    fn test_tool_result_without_error_has_none_error_field() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "tool_result".to_string(),
            data: json!({
                "call_id": "tc-ok",
                "tool": "read_file",
                "result": "file contents"
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        let results = chunk.tool_results.unwrap();
        assert_eq!(results[0].name, "read_file");
        assert_eq!(results[0].result, "file contents");
        assert!(
            results[0].error.is_none(),
            "Error should be None for successful results"
        );
    }

    #[test]
    fn test_message_complete_with_usage_extraction() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "message_complete".to_string(),
            data: json!({
                "message_id": "msg-1",
                "full_response": "done",
                "prompt_tokens": 200,
                "completion_tokens": 80,
                "total_tokens": 280
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        assert!(chunk.done, "message_complete should set done=true");
        let usage = chunk.usage.expect("Should extract usage from event data");
        assert_eq!(usage.prompt_tokens, 200);
        assert_eq!(usage.completion_tokens, 80);
        assert_eq!(usage.total_tokens, 280);
    }

    #[test]
    fn test_message_complete_without_usage_extraction() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "message_complete".to_string(),
            data: json!({
                "message_id": "msg-2",
                "full_response": "no tokens"
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        assert!(chunk.done, "message_complete should set done=true");
        assert!(
            chunk.usage.is_none(),
            "Should be None when no token fields in event data"
        );
    }

    #[test]
    fn test_message_complete_usage_defaults_missing_fields() {
        // total_tokens present but prompt/completion missing → should still extract
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "message_complete".to_string(),
            data: json!({
                "message_id": "msg-3",
                "full_response": "partial usage",
                "total_tokens": 500
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        let usage = chunk
            .usage
            .expect("Should extract usage when total_tokens present");
        assert_eq!(usage.total_tokens, 500);
        assert_eq!(
            usage.prompt_tokens, 0,
            "Missing prompt_tokens should default to 0"
        );
        assert_eq!(
            usage.completion_tokens, 0,
            "Missing completion_tokens should default to 0"
        );
    }

    #[test]
    fn test_precognition_complete_with_notes() {
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "precognition_complete".to_string(),
            data: json!({
                "notes_count": 2,
                "query_summary": "how to use async",
                "notes": [
                    { "title": "Async Patterns", "kiln_label": null },
                    { "title": "Tokio Guide", "kiln_label": "docs" }
                ]
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        assert_eq!(chunk.precognition_notes_count, Some(2));
        let notes = chunk.precognition_notes.expect("notes should be populated");
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].title, "Async Patterns");
        assert!(notes[0].kiln_label.is_none());
        assert_eq!(notes[1].title, "Tokio Guide");
        assert_eq!(notes[1].kiln_label.as_deref(), Some("docs"));
    }

    #[test]
    fn test_precognition_complete_without_notes_backward_compat() {
        // Old daemon events without "notes" field should still work
        let event = SessionEvent {
            session_id: "test".to_string(),
            event_type: "precognition_complete".to_string(),
            data: json!({
                "notes_count": 3,
                "query_summary": "search query"
            }),
        };

        let chunk = session_event_to_chat_chunk(&event).unwrap();
        assert_eq!(chunk.precognition_notes_count, Some(3));
        assert!(
            chunk.precognition_notes.is_none(),
            "Missing notes field should result in None for backward compatibility"
        );
    }
}
