//! Daemon-backed agent handle implementation
//!
//! Implements `AgentHandle` by routing messages through the daemon's agent execution.
//! This allows the TUI to use daemon-managed agents transparently.

use std::sync::Arc;

use crucible_core::interaction::InteractionEvent;
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::{ChatError, ChatResult};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::{DaemonClient, SessionEvent};

mod agent_handle;
mod convert;

#[cfg(test)]
mod tests;

/// Agent handle that routes messages through the daemon
///
/// This handle implements `AgentHandle` by:
/// 1. Sending messages via `session.send_message` RPC
/// 2. Subscribing to session events for streaming responses
/// 3. Converting `SessionEvent` back to `ChatChunk` for the TUI
/// 4. Routing interaction events to a separate channel for the TUI event loop
pub struct DaemonAgentHandle {
    pub(super) client: Arc<DaemonClient>,
    pub(super) session_id: String,
    pub(super) router_session_id: Arc<tokio::sync::watch::Sender<String>>,
    pub(super) streaming_rx: Arc<Mutex<mpsc::UnboundedReceiver<SessionEvent>>>,
    pub(super) interaction_rx: Option<mpsc::UnboundedReceiver<InteractionEvent>>,
    /// Raw SessionEvent receiver for callers that want to bypass the
    /// streaming_rx → ChatChunk conversion. Used by the live TUI, which
    /// subscribes to SessionEvents directly instead of consuming ChatChunks.
    /// Set at construction time via `new_and_subscribe_with_raw_forwarding`.
    /// Only one of `streaming_rx` / `raw_event_rx` gets populated per
    /// handle: when raw forwarding is enabled, the router skips
    /// `streaming_tx`.
    pub(super) raw_event_rx: Option<mpsc::UnboundedReceiver<SessionEvent>>,
    pub(super) mode_id: String,
    pub(super) cached_model: Option<String>,
    pub(super) cached_temperature: Option<f64>,
    pub(super) cached_max_tokens: Option<u32>,
    pub(super) cached_thinking_budget: Option<i64>,
    pub(super) cached_max_iterations: Option<u32>,
    pub(super) cached_execution_timeout: Option<u64>,
    pub(super) cached_system_prompt: Option<String>,
    pub(super) cached_context_budget: Option<usize>,
    pub(super) cached_context_strategy: Option<String>,
    pub(super) cached_context_window: Option<usize>,
    pub(super) cached_output_validation: Option<String>,
    pub(super) cached_validation_retries: Option<u32>,
    pub(super) cached_precognition_results: Option<usize>,
    pub(super) kiln_path: Option<PathBuf>,
    pub(super) workspace: Option<PathBuf>,
    pub(super) cached_agent_config: Option<SessionAgent>,
    pub(super) event_router_task: Option<JoinHandle<()>>,
}

impl DaemonAgentHandle {
    /// Build a handle with all default cached fields. Both public constructors
    /// call this, then patch their specific overrides, avoiding field-list
    /// duplication.
    fn new_base(
        client: Arc<DaemonClient>,
        session_id: String,
        session_id_tx: tokio::sync::watch::Sender<String>,
        streaming_rx: mpsc::UnboundedReceiver<SessionEvent>,
        interaction_rx: mpsc::UnboundedReceiver<InteractionEvent>,
        event_router_task: JoinHandle<()>,
    ) -> Self {
        Self {
            client,
            session_id,
            router_session_id: Arc::new(session_id_tx),
            streaming_rx: Arc::new(Mutex::new(streaming_rx)),
            interaction_rx: Some(interaction_rx),
            raw_event_rx: None,
            mode_id: "normal".to_string(),
            cached_model: None,
            cached_temperature: None,
            cached_max_tokens: None,
            cached_thinking_budget: None,
            cached_max_iterations: None,
            cached_execution_timeout: None,
            cached_system_prompt: None,
            cached_context_budget: None,
            cached_context_strategy: None,
            cached_context_window: None,
            cached_output_validation: None,
            cached_validation_retries: None,
            cached_precognition_results: None,
            kiln_path: None,
            workspace: None,
            cached_agent_config: None,
            event_router_task: Some(event_router_task),
        }
    }

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
            convert::event_router(event_rx, streaming_tx, interaction_tx, None, session_id_rx)
                .await;
        });

        Self::new_base(
            client,
            session_id,
            session_id_tx,
            streaming_rx,
            interaction_rx,
            event_router_task,
        )
    }

    /// Take the raw SessionEvent receiver, consumable once.
    ///
    /// Only populated when the handle was constructed with
    /// `new_and_subscribe_with_raw_forwarding`. Used by the live TUI to
    /// feed events through the unified `SessionEventStream` converter.
    pub fn take_raw_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<SessionEvent>> {
        self.raw_event_rx.take()
    }

    /// Create a daemon agent handle and subscribe to its session
    pub async fn new_and_subscribe(
        client: Arc<DaemonClient>,
        session_id: String,
        event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    ) -> ChatResult<Self> {
        Self::subscribe(&client, &session_id).await?;
        let mut handle = Self::new(client.clone(), session_id.clone(), event_rx);
        handle.fetch_cached_values(&client, &session_id).await;
        Ok(handle)
    }

    /// Subscribe variant that forwards raw SessionEvents to the caller
    /// (via `take_raw_event_receiver`) instead of converting them into
    /// ChatChunks internally. Used by the live TUI.
    ///
    /// In this mode, `send_message_stream` will block indefinitely — callers
    /// must use `send_message_fire_and_forget` instead.
    pub async fn new_and_subscribe_with_raw_forwarding(
        client: Arc<DaemonClient>,
        session_id: String,
        event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    ) -> ChatResult<Self> {
        Self::subscribe(&client, &session_id).await?;

        let (streaming_tx, streaming_rx) = mpsc::unbounded_channel();
        let (interaction_tx, interaction_rx) = mpsc::unbounded_channel();
        let (session_id_tx, session_id_rx) = tokio::sync::watch::channel(session_id.clone());
        let (raw_event_tx, raw_event_rx) = mpsc::unbounded_channel();

        let event_router_task = tokio::spawn(async move {
            convert::event_router(
                event_rx,
                streaming_tx,
                interaction_tx,
                Some(raw_event_tx),
                session_id_rx,
            )
            .await;
        });

        let mut handle = Self::new_base(
            client.clone(),
            session_id.clone(),
            session_id_tx,
            streaming_rx,
            interaction_rx,
            event_router_task,
        );
        handle.raw_event_rx = Some(raw_event_rx);
        handle.fetch_cached_values(&client, &session_id).await;
        Ok(handle)
    }

    async fn subscribe(client: &Arc<DaemonClient>, session_id: &str) -> ChatResult<()> {
        tracing::debug!(session_id = %session_id, "Subscribing to daemon session events");
        client
            .session_subscribe(&[session_id])
            .await
            .map_err(|e| ChatError::Connection(format!("Failed to subscribe: {}", e)))?;
        tracing::info!(session_id = %session_id, "Successfully subscribed to session events");
        Ok(())
    }

    /// Fetch initial cached values from daemon (best-effort, default to None on failure).
    async fn fetch_cached_values(&mut self, client: &Arc<DaemonClient>, session_id: &str) {
        self.cached_temperature = client
            .session_get_temperature(session_id)
            .await
            .ok()
            .flatten();
        self.cached_max_tokens = client
            .session_get_max_tokens(session_id)
            .await
            .ok()
            .flatten();
        self.cached_thinking_budget = client
            .session_get_thinking_budget(session_id)
            .await
            .ok()
            .flatten();
        self.cached_max_iterations = client
            .session_get_max_iterations(session_id)
            .await
            .ok()
            .flatten();
        self.cached_execution_timeout = client
            .session_get_execution_timeout(session_id)
            .await
            .ok()
            .flatten();
        self.cached_system_prompt = client
            .session_get_system_prompt(session_id)
            .await
            .ok()
            .flatten();
        self.cached_context_budget = client
            .session_get_context_budget(session_id)
            .await
            .ok()
            .flatten();
        self.cached_context_strategy = client
            .session_get_context_strategy(session_id)
            .await
            .ok()
            .flatten();
        self.cached_context_window = client
            .session_get_context_window(session_id)
            .await
            .ok()
            .flatten();
        self.cached_output_validation = client
            .session_get_output_validation(session_id)
            .await
            .ok()
            .flatten();
        self.cached_validation_retries = client
            .session_get_validation_retries(session_id)
            .await
            .ok()
            .flatten();
        self.cached_precognition_results = client
            .session_get_precognition_results(session_id)
            .await
            .ok()
            .flatten();
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
