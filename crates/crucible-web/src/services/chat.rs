//! Chat service for managing conversations with ACP agents
//!
//! This service wraps the ACP client and provides a channel-based streaming API
//! suitable for SSE delivery to browsers.

use crate::{ChatEvent, Result, WebError};
use crucible_acp::{ChatSession, ChatSessionConfig};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

/// Configuration for the chat service
#[derive(Debug, Clone)]
pub struct ChatServiceConfig {
    /// Agent binary path (e.g., "claude")
    pub agent_path: String,
    /// Channel buffer size for SSE events
    pub channel_buffer: usize,
}

impl Default for ChatServiceConfig {
    fn default() -> Self {
        Self {
            agent_path: "claude".to_string(),
            channel_buffer: 100,
        }
    }
}

/// Chat service that manages ACP agent communication
///
/// For MVP, this wraps the existing ChatSession and provides events
/// after the response completes. True streaming will require changes
/// to crucible-acp.
pub struct ChatService {
    session: Arc<Mutex<Option<ChatSession>>>,
    event_tx: broadcast::Sender<ChatEvent>,
    #[allow(dead_code)]
    config: ChatServiceConfig,
}

impl ChatService {
    /// Create a new chat service
    pub fn new(config: ChatServiceConfig) -> Self {
        let (event_tx, _) = broadcast::channel(config.channel_buffer);
        Self {
            session: Arc::new(Mutex::new(None)),
            event_tx,
            config,
        }
    }

    /// Subscribe to chat events
    pub fn subscribe(&self) -> broadcast::Receiver<ChatEvent> {
        self.event_tx.subscribe()
    }

    /// Initialize the chat session (connects to agent)
    pub async fn initialize(&self) -> Result<()> {
        let mut session_guard = self.session.lock().await;

        if session_guard.is_some() {
            return Ok(()); // Already initialized
        }

        // Create chat session with mock mode for now
        // TODO: Connect to actual ACP agent
        let chat_config = ChatSessionConfig::default();
        let session = ChatSession::new(chat_config);

        *session_guard = Some(session);
        tracing::info!("Chat service initialized");
        Ok(())
    }

    /// Send a message and stream the response
    ///
    /// For MVP, this sends the message, waits for the complete response,
    /// then emits events. True token-by-token streaming requires changes
    /// to crucible-acp's streaming API.
    pub async fn send_message(&self, message: String) -> Result<()> {
        let mut session_guard = self.session.lock().await;

        let session = session_guard
            .as_mut()
            .ok_or_else(|| WebError::Chat("Chat session not initialized".to_string()))?;

        // Send message and get response
        // TODO: Replace with streaming API when available in crucible-acp
        let (response, tool_calls) = session
            .send_message(&message)
            .await
            .map_err(WebError::Acp)?;

        // Emit tool call events
        for tool in &tool_calls {
            let _ = self.event_tx.send(ChatEvent::ToolCall {
                id: tool.id.clone().unwrap_or_default(),
                title: tool.title.clone(),
                arguments: tool.arguments.clone(),
            });
        }

        // Emit the response as a single "token" for now
        // With true streaming, this would be multiple Token events
        if !response.is_empty() {
            let _ = self.event_tx.send(ChatEvent::Token {
                content: response.clone(),
            });
        }

        // Emit completion event
        let _ = self.event_tx.send(ChatEvent::MessageComplete {
            id: uuid::Uuid::new_v4().to_string(),
            content: response,
            tool_calls: tool_calls
                .into_iter()
                .map(|t| crate::events::ToolCallSummary {
                    id: t.id.unwrap_or_default(),
                    title: t.title,
                })
                .collect(),
        });

        Ok(())
    }

    pub async fn submit_interaction_response(
        &self,
        request_id: String,
        response: serde_json::Value,
    ) {
        tracing::info!(%request_id, ?response, "Interaction response received");
        // TODO: Route to ACP agent when interaction support is added
    }
}
