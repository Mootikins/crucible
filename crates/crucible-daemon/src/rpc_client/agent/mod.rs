//! Daemon-backed agent handle implementation
//!
//! Implements `AgentHandle` by routing messages through the daemon's agent execution.
//! This allows the TUI to use daemon-managed agents transparently.

use std::sync::Arc;

use crucible_core::interaction::{InteractionEvent, InteractionRequest};
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::{ChatError, ChatResult};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::{DaemonClient, SessionEvent};

mod agent_handle;
mod convert;
mod native_agent;

/// Agent handle that routes messages through the daemon
///
/// This handle implements `AgentHandle` by:
/// 1. Sending messages via `session.send_message` RPC
/// 2. Subscribing to session events for streaming responses
/// 3. Translating `SessionEvent`s into `TurnEvent`s for `Agent::turn`
/// 4. Routing interaction events to a separate channel for the TUI event loop
pub struct DaemonAgentHandle {
    pub(super) client: Arc<DaemonClient>,
    pub(super) session_id: String,
    pub(super) router_session_id: Arc<tokio::sync::watch::Sender<String>>,
    pub(super) streaming_rx: Arc<Mutex<mpsc::UnboundedReceiver<SessionEvent>>>,
    pub(super) interaction_rx: Option<mpsc::UnboundedReceiver<InteractionEvent>>,
    /// Raw SessionEvent receiver for callers that want to bypass the
    /// `streaming_rx` → `Agent::turn` path. Used by the live TUI, which
    /// subscribes to SessionEvents directly. Set at construction time
    /// via `new_and_subscribe_with_raw_forwarding`. Only one of
    /// `streaming_rx` / `raw_event_rx` gets populated per handle: when
    /// raw forwarding is enabled, the router skips `streaming_tx`.
    pub(super) raw_event_rx: Option<mpsc::UnboundedReceiver<SessionEvent>>,
    pub(super) mode_id: String,
    pub(super) cached_model: Option<String>,
    pub(super) cached_context_strategy: Option<String>,
    pub(super) cached_precognition: Option<bool>,
    pub(super) cached_plugin_approvals:
        std::collections::BTreeMap<String, crucible_core::session::PluginApproval>,
    pub(super) cached_plugin_turn_limit: u32,
    /// The kiln NAME a `/clear` re-create should attach. Names, not paths:
    /// the daemon resolves them against its `[kilns]` registry.
    pub(super) kiln: Option<crucible_core::config::KilnName>,
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
            mode_id: "ask".to_string(),
            cached_model: None,
            cached_context_strategy: None,
            cached_precognition: None,
            cached_plugin_approvals: Default::default(),
            cached_plugin_turn_limit: 25,
            kiln: None,
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
        Self::new_with_pending(client, session_id, event_rx, Vec::new())
    }

    fn new_with_pending(
        client: Arc<DaemonClient>,
        session_id: String,
        event_rx: mpsc::UnboundedReceiver<SessionEvent>,
        pending: Vec<InteractionEvent>,
    ) -> Self {
        let (streaming_tx, streaming_rx) = mpsc::unbounded_channel();
        let (interaction_tx, interaction_rx) = mpsc::unbounded_channel();
        let (session_id_tx, session_id_rx) = tokio::sync::watch::channel(session_id.clone());

        let event_router_task = tokio::spawn(async move {
            convert::event_router(
                event_rx,
                streaming_tx,
                interaction_tx,
                None,
                session_id_rx,
                pending,
            )
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
        let pending = Self::fetch_pending(&client, &session_id).await;
        let mut handle =
            Self::new_with_pending(client.clone(), session_id.clone(), event_rx, pending);
        handle.fetch_cached_values(&client, &session_id).await;
        Ok(handle)
    }

    /// Subscribe variant that forwards raw SessionEvents to the caller
    /// (via `take_raw_event_receiver`) instead of routing them through
    /// `Agent::turn`'s converter. Used by the live TUI, which owns the
    /// `SessionEvent` stream directly.
    ///
    /// In this mode, `Agent::turn` will block on `streaming_rx` (which
    /// receives nothing) — callers must use
    /// `send_message_fire_and_forget` to dispatch messages.
    pub async fn new_and_subscribe_with_raw_forwarding(
        client: Arc<DaemonClient>,
        session_id: String,
        event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    ) -> ChatResult<Self> {
        Self::subscribe(&client, &session_id).await?;
        let pending = Self::fetch_pending(&client, &session_id).await;

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
                pending,
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

    async fn fetch_pending(client: &DaemonClient, session_id: &str) -> Vec<InteractionEvent> {
        let response = match client.session_pending_interactions().await {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!(session_id = %session_id, error = %error, "Could not recover pending interactions");
                return Vec::new();
            }
        };
        Self::pending_from_response(&response, session_id)
    }

    fn pending_from_response(
        response: &serde_json::Value,
        session_id: &str,
    ) -> Vec<InteractionEvent> {
        response["pending"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| entry["session_id"].as_str() == Some(session_id))
            .filter_map(|entry| {
                Some(InteractionEvent {
                    request_id: entry["request_id"].as_str()?.to_string(),
                    request: serde_json::from_value::<InteractionRequest>(entry["request"].clone())
                        .ok()?,
                })
            })
            .collect()
    }

    /// Fetch initial cached values from daemon (best-effort, default to None on failure).
    async fn fetch_cached_values(&mut self, client: &Arc<DaemonClient>, session_id: &str) {
        self.cached_agent_config = client
            .session_get(session_id)
            .await
            .ok()
            .and_then(|session| serde_json::from_value(session["agent"].clone()).ok());
        self.cached_context_strategy = client
            .session_get_context_strategy(session_id)
            .await
            .ok()
            .flatten();
        self.cached_precognition = client.session_get_precognition(session_id).await.ok();
        self.cached_plugin_approvals = client
            .session_list_plugin_approvals(session_id)
            .await
            .unwrap_or_default();
        self.cached_plugin_turn_limit = client
            .session_get(session_id)
            .await
            .ok()
            .and_then(|session| session["plugin_turn_limit"].as_u64())
            .and_then(|limit| u32::try_from(limit).ok())
            .unwrap_or(25);
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn with_kiln(mut self, kiln: Option<crucible_core::config::KilnName>) -> Self {
        self.kiln = kiln;
        self
    }

    pub fn with_workspace(mut self, path: PathBuf) -> Self {
        self.workspace = Some(path);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::interaction::{AskRequest, InteractionRequest};

    #[tokio::test]
    async fn pending_snapshot_reaches_the_tui_interaction_channel_once() {
        let request = InteractionRequest::Ask(AskRequest::new("Which branch?"));
        let response = serde_json::json!({"pending": [
            {"session_id": "other", "request_id": "other-id", "request": request},
            {"session_id": "wanted", "request_id": "ask-id", "request": request},
        ]});
        let pending = DaemonAgentHandle::pending_from_response(&response, "wanted");
        assert_eq!(pending.len(), 1);

        let (source_tx, event_rx) = mpsc::unbounded_channel();
        let (stream_tx, _stream_rx) = mpsc::unbounded_channel();
        let (interaction_tx, mut interaction_rx) = mpsc::unbounded_channel();
        let (session_id_tx, session_id_rx) = tokio::sync::watch::channel("wanted".to_string());
        let router = tokio::spawn(convert::event_router(
            event_rx,
            stream_tx,
            interaction_tx,
            None,
            session_id_rx,
            pending,
        ));
        let recovered = interaction_rx.recv().await.unwrap();
        assert_eq!(recovered.request_id, "ask-id");
        assert!(matches!(recovered.request, InteractionRequest::Ask(_)));

        source_tx
            .send(SessionEvent::new(
                "wanted",
                "interaction_requested",
                serde_json::json!({"request_id": "ask-id", "request": request}),
            ))
            .unwrap();
        drop(source_tx);
        drop(session_id_tx);
        router.await.unwrap();
        assert!(
            interaction_rx.try_recv().is_err(),
            "the subscribe race must not open a second modal"
        );
    }
}
