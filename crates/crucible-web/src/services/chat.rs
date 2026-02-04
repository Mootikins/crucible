//! Chat service for managing conversations via the daemon
//!
//! This service provides a channel-based streaming API suitable for SSE
//! delivery to browsers. Currently stubbed — the web chat interface is
//! pending migration to daemon-based sessions (see roadmap).

use crate::{ChatEvent, Result, WebError};
use tokio::sync::broadcast;

/// Configuration for the chat service
#[derive(Debug, Clone)]
pub struct ChatServiceConfig {
    /// Channel buffer size for SSE events
    pub channel_buffer: usize,
}

impl Default for ChatServiceConfig {
    fn default() -> Self {
        Self {
            channel_buffer: 100,
        }
    }
}

/// Chat service that manages agent communication
///
/// Currently stubbed. The web chat interface will be implemented on top of
/// the daemon's session management (RPC over Unix socket), matching how
/// the CLI works. See `crucible-rpc` for the client API.
pub struct ChatService {
    event_tx: broadcast::Sender<ChatEvent>,
    #[allow(dead_code)]
    config: ChatServiceConfig,
}

impl ChatService {
    /// Create a new chat service
    pub fn new(config: ChatServiceConfig) -> Self {
        let (event_tx, _) = broadcast::channel(config.channel_buffer);
        Self { event_tx, config }
    }

    /// Subscribe to chat events
    pub fn subscribe(&self) -> broadcast::Receiver<ChatEvent> {
        self.event_tx.subscribe()
    }

    /// Initialize the chat session
    ///
    /// Currently a no-op. Will connect to the daemon when implemented.
    pub async fn initialize(&self) -> Result<()> {
        tracing::info!("Chat service initialized (stub — daemon integration pending)");
        Ok(())
    }

    /// Send a message and stream the response
    ///
    /// Currently returns an error indicating the web chat is not yet implemented.
    /// Will be wired to daemon RPC (`session.send_message`) when ready.
    pub async fn send_message(&self, _message: String) -> Result<()> {
        Err(WebError::Chat(
            "Web chat is not yet implemented. Use `cru chat` for the TUI interface.".to_string(),
        ))
    }

    /// Handle an interaction response from the browser
    pub async fn submit_interaction_response(
        &self,
        request_id: String,
        response: serde_json::Value,
    ) {
        tracing::info!(%request_id, ?response, "Interaction response received (stub)");
    }
}
