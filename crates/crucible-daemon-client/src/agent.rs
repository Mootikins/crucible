//! Daemon-backed agent handle implementation
//!
//! Implements `AgentHandle` by routing messages through the daemon's agent execution.
//! This allows the TUI to use daemon-managed agents transparently.

use std::sync::Arc;

use async_trait::async_trait;
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
pub struct DaemonAgentHandle {
    client: Arc<DaemonClient>,
    session_id: String,
    event_rx: Arc<Mutex<mpsc::UnboundedReceiver<SessionEvent>>>,
    mode_id: String,
    connected: bool,
}

impl DaemonAgentHandle {
    /// Create a new daemon agent handle
    ///
    /// The client should have been created with `connect_with_events()` so that
    /// the event receiver is properly set up.
    pub fn new(
        client: Arc<DaemonClient>,
        session_id: String,
        event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    ) -> Self {
        Self {
            client,
            session_id,
            event_rx: Arc::new(Mutex::new(event_rx)),
            mode_id: "plan".to_string(),
            connected: true,
        }
    }

    /// Create a daemon agent handle and subscribe to its session
    ///
    /// This is a convenience constructor that:
    /// 1. Takes an existing client with event channel
    /// 2. Subscribes to the session for events
    /// 3. Returns the configured handle
    pub async fn new_and_subscribe(
        client: Arc<DaemonClient>,
        session_id: String,
        event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    ) -> ChatResult<Self> {
        client
            .session_subscribe(&[&session_id])
            .await
            .map_err(|e| ChatError::Connection(format!("Failed to subscribe: {}", e)))?;

        Ok(Self::new(client, session_id, event_rx))
    }

    /// Get the session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
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
            })
        }
        "tool_result" => {
            let call_id = event.data.get("call_id").and_then(|v| v.as_str());
            let result = event.data.get("result")?;

            let name = call_id.unwrap_or("tool").to_string();
            let result_str = if result.is_string() {
                result.as_str().unwrap_or("").to_string()
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
                    error: None,
                }]),
                reasoning: None,
            })
        }
        "message_complete" => Some(ChatChunk {
            delta: String::new(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
        }),
        "ended" => Some(ChatChunk {
            delta: String::new(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
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
        let event_rx = Arc::clone(&self.event_rx);

        Box::pin(async_stream::stream! {
            let send_result = client.session_send_message(&session_id, &message).await;
            if let Err(e) = send_result {
                yield Err(ChatError::Communication(format!("Failed to send message: {}", e)));
                return;
            }

            let mut rx = event_rx.lock().await;
            loop {
                match rx.recv().await {
                    Some(event) => {
                        if event.session_id != session_id {
                            continue;
                        }
                        if let Some(chunk) = session_event_to_chat_chunk(&event) {
                            let is_done = chunk.done;
                            yield Ok(chunk);
                            if is_done {
                                break;
                            }
                        }
                    }
                    None => {
                        yield Err(ChatError::Connection("Event channel closed".to_string()));
                        break;
                    }
                }
            }
        })
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
}
