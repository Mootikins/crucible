//! Daemon-backed agent handle implementation
//!
//! Implements `AgentHandle` by routing messages through the daemon's agent execution.
//! This allows the TUI to use daemon-managed agents transparently.

use std::sync::Arc;

use async_trait::async_trait;
use crucible_core::interaction::InteractionEvent;
use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatError, ChatResult, ChatToolCall, ChatToolResult, CommandDescriptor,
};
use crucible_core::types::acp::schema::{AvailableCommand, SessionModeState};
use futures::stream::BoxStream;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

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
    streaming_rx: Arc<Mutex<mpsc::UnboundedReceiver<SessionEvent>>>,
    interaction_rx: Option<mpsc::UnboundedReceiver<InteractionEvent>>,
    mode_id: String,
    connected: bool,
    cached_model: Option<String>,
    cached_temperature: Option<f64>,
    cached_max_tokens: Option<u32>,
    cached_thinking_budget: Option<i64>,
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

        let session_id_clone = session_id.clone();
        tokio::spawn(async move {
            event_router(event_rx, streaming_tx, interaction_tx, session_id_clone).await;
        });

        Self {
            client,
            session_id,
            streaming_rx: Arc::new(Mutex::new(streaming_rx)),
            interaction_rx: Some(interaction_rx),
            mode_id: "normal".to_string(),
            connected: true,
            cached_model: None,
            cached_temperature: None,
            cached_max_tokens: None,
            cached_thinking_budget: None,
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

        Ok(handle)
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

/// Background task that routes events from daemon to appropriate channels
async fn event_router(
    mut event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    streaming_tx: mpsc::UnboundedSender<SessionEvent>,
    interaction_tx: mpsc::UnboundedSender<InteractionEvent>,
    session_id: String,
) {
    while let Some(event) = event_rx.recv().await {
        if event.session_id != session_id {
            tracing::trace!(
                event_session = %event.session_id,
                expected_session = %session_id,
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

/// Convert a SessionEvent to a ChatChunk
///
/// Maps daemon event types to ChatChunk fields:
/// - `text_delta` → delta
/// - `thinking` → reasoning
/// - `tool_call` → tool_calls
/// - `tool_result` → tool_results
/// - `message_complete` → done: true
fn session_event_to_chat_chunk(event: &SessionEvent) -> Option<ChatChunk> {
    match event.event_type.as_str() {
        "text_delta" => {
            let content = event.data.get("content")?.as_str()?;
            Some(ChatChunk {
                delta: content.to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
            })
        }
        "thinking" => {
            let content = event.data.get("content")?.as_str()?;
            Some(ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: Some(content.to_string()),
                usage: None,
                subagent_events: None,
            })
        }
        "tool_call" => {
            let call_id = event.data.get("call_id").and_then(|v| v.as_str());
            let tool = event.data.get("tool")?.as_str()?;
            let args = event.data.get("args").cloned();

            Some(ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: Some(vec![ChatToolCall {
                    name: tool.to_string(),
                    arguments: args,
                    id: call_id.map(String::from),
                }]),
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
            })
        }
        "tool_result" => {
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
                delta: String::new(),
                done: false,
                tool_calls: None,
                tool_results: Some(vec![ChatToolResult {
                    name,
                    result: result_str,
                    error,
                    call_id: call_id_str,
                }]),
                reasoning: None,
                usage: None,
                subagent_events: None,
            })
        }
        "message_complete" => {
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
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage,
                subagent_events: None,
            })
        }
        "ended" => Some(ChatChunk {
            delta: String::new(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
        }),
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

            let send_result = client.session_send_message(&session_id, &message).await;
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
                            let is_done = chunk.done;
                            yield Ok(chunk);
                            if is_done {
                                tracing::debug!("Stream complete (done=true)");
                                break;
                            }
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

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn on_commands_update(&mut self, _commands: Vec<CommandDescriptor>) -> ChatResult<()> {
        Ok(())
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        None
    }

    fn get_mode_id(&self) -> &str {
        &self.mode_id
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        self.mode_id = mode_id.to_string();
        Ok(())
    }

    fn get_commands(&self) -> &[AvailableCommand] {
        &[]
    }

    fn clear_history(&mut self) {
        tracing::info!(session_id = %self.session_id, "Clear history requested (daemon handles internally)");
    }

    async fn switch_model(&mut self, model_id: &str) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, model = %model_id, "Switching model via daemon");
        self.client
            .session_switch_model(&self.session_id, model_id)
            .await
            .map_err(|e| ChatError::Communication(format!("Failed to switch model: {}", e)))?;
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
            .map_err(|e| ChatError::Communication(format!("Failed to cancel agent: {}", e)))?;
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

    async fn set_temperature(&mut self, temperature: f64) -> ChatResult<()> {
        tracing::info!(session_id = %self.session_id, temperature = temperature, "Setting temperature via daemon");
        self.client
            .session_set_temperature(&self.session_id, temperature)
            .await
            .map_err(|e| ChatError::Communication(format!("Failed to set temperature: {}", e)))?;
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
            .map_err(|e| ChatError::Communication(format!("Failed to set max_tokens: {}", e)))?;
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
}
