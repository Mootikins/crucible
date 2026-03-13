use crate::{Result, WebError};
use crucible_config::CliAppConfig;
use crucible_daemon::{
    agent_manager::providers::ProviderInfo, DaemonCapabilities, DaemonClient,
    LuaDiscoverPluginsRequest, LuaDiscoverPluginsResponse, LuaPluginHealthRequest,
    LuaPluginHealthResponse, SessionEvent,
};
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

const EVENT_CHANNEL_CAPACITY: usize = 256;

#[derive(Clone)]
pub struct AppState {
    pub daemon: Arc<ReconnectingDaemon>,
    pub events: Arc<EventBroker>,
    pub config: Arc<CliAppConfig>,
    pub http_client: reqwest::Client,
}

pub struct ReconnectingDaemon {
    daemon: Arc<RwLock<DaemonClient>>,
    generation: AtomicU64,
}

impl ReconnectingDaemon {
    pub fn new(daemon: DaemonClient) -> Self {
        Self {
            daemon: Arc::new(RwLock::new(daemon)),
            generation: AtomicU64::new(0),
        }
    }

    async fn call_with_reconnect<T>(
        &self,
        method: &'static str,
        call: impl for<'a> Fn(&'a DaemonClient) -> BoxFuture<'a, anyhow::Result<T>>,
    ) -> anyhow::Result<T> {
        let observed_generation = self.generation.load(Ordering::Acquire);
        let first_attempt = {
            let daemon = self.daemon.read().await;
            call(&daemon).await
        };

        match first_attempt {
            Ok(value) => Ok(value),
            Err(err) if Self::is_connection_error(&err) => {
                tracing::warn!(
                    method,
                    error = %err,
                    "Daemon connection failed, reconnecting and retrying once"
                );
                self.reconnect_if_stale(observed_generation).await?;

                let daemon = self.daemon.read().await;
                call(&daemon).await
            }
            Err(err) => Err(err),
        }
    }

    async fn reconnect_if_stale(&self, observed_generation: u64) -> anyhow::Result<()> {
        if self.generation.load(Ordering::Acquire) != observed_generation {
            return Ok(());
        }

        let mut daemon = self.daemon.write().await;
        if self.generation.load(Ordering::Acquire) != observed_generation {
            return Ok(());
        }

        let new_daemon = DaemonClient::connect_or_start().await?;
        *daemon = new_daemon;
        self.generation.fetch_add(1, Ordering::AcqRel);
        tracing::warn!(
            "Daemon reconnected; event stream may be stale — restart web server to restore SSE"
        );
        Ok(())
    }

    fn is_connection_error(err: &anyhow::Error) -> bool {
        let msg = err.to_string();
        let lower = msg.to_ascii_lowercase();
        let has_connection_text = [
            "broken pipe",
            "connection reset",
            "connection refused",
            "os error 32",
        ]
        .iter()
        .any(|needle| lower.contains(needle));

        if has_connection_text {
            return true;
        }

        for cause in err.chain() {
            if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
                if matches!(
                    io_err.kind(),
                    std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionRefused
                ) {
                    return true;
                }
            }
        }

        false
    }

    pub async fn capabilities(&self) -> anyhow::Result<DaemonCapabilities> {
        self.call_with_reconnect("capabilities", |daemon| Box::pin(daemon.capabilities()))
            .await
    }

    pub async fn kiln_list(&self) -> anyhow::Result<Vec<serde_json::Value>> {
        self.call_with_reconnect("kiln.list", |daemon| Box::pin(daemon.kiln_list()))
            .await
    }

    pub async fn list_notes(
        &self,
        kiln_path: &Path,
        path_filter: Option<&str>,
    ) -> anyhow::Result<Vec<(String, String, Option<String>, Vec<String>, Option<String>)>> {
        let kiln_path = kiln_path.to_path_buf();
        let path_filter = path_filter.map(str::to_string);
        self.call_with_reconnect("list_notes", move |daemon| {
            let kiln_path = kiln_path.clone();
            let path_filter = path_filter.clone();
            Box::pin(async move { daemon.list_notes(&kiln_path, path_filter.as_deref()).await })
        })
        .await
    }

    pub async fn get_note_by_name(
        &self,
        kiln_path: &Path,
        name: &str,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let kiln_path = kiln_path.to_path_buf();
        let name = name.to_string();
        self.call_with_reconnect("get_note_by_name", move |daemon| {
            let kiln_path = kiln_path.clone();
            let name = name.clone();
            Box::pin(async move { daemon.get_note_by_name(&kiln_path, &name).await })
        })
        .await
    }

    pub async fn note_upsert(
        &self,
        kiln_path: &Path,
        note: &crucible_core::storage::NoteRecord,
    ) -> anyhow::Result<()> {
        let kiln_path = kiln_path.to_path_buf();
        let note = note.clone();
        self.call_with_reconnect("note.upsert", move |daemon| {
            let kiln_path = kiln_path.clone();
            let note = note.clone();
            Box::pin(async move { daemon.note_upsert(&kiln_path, &note).await })
        })
        .await
    }

    pub async fn search_vectors(
        &self,
        kiln_path: &Path,
        vector: &[f32],
        limit: usize,
    ) -> anyhow::Result<Vec<(String, f64)>> {
        let kiln_path = kiln_path.to_path_buf();
        let vector = vector.to_vec();
        self.call_with_reconnect("search_vectors", move |daemon| {
            let kiln_path = kiln_path.clone();
            let vector = vector.clone();
            Box::pin(async move { daemon.search_vectors(&kiln_path, &vector, limit).await })
        })
        .await
    }

    pub async fn lua_discover_plugins(
        &self,
        params: LuaDiscoverPluginsRequest,
    ) -> anyhow::Result<LuaDiscoverPluginsResponse> {
        self.call_with_reconnect("lua.discover_plugins", |daemon| {
            Box::pin(daemon.lua_discover_plugins(params.clone()))
        })
        .await
    }

    pub async fn lua_plugin_health(
        &self,
        params: LuaPluginHealthRequest,
    ) -> anyhow::Result<LuaPluginHealthResponse> {
        self.call_with_reconnect("lua.plugin_health", |daemon| {
            Box::pin(daemon.lua_plugin_health(params.clone()))
        })
        .await
    }

    pub async fn mcp_status(&self) -> anyhow::Result<serde_json::Value> {
        self.call_with_reconnect("mcp.status", |daemon| Box::pin(daemon.mcp_status()))
            .await
    }

    pub async fn session_create(
        &self,
        session_type: &str,
        kiln: &Path,
        workspace: Option<&Path>,
        connect_kilns: Vec<&Path>,
        recording_mode: Option<&str>,
        recording_path: Option<&Path>,
    ) -> anyhow::Result<serde_json::Value> {
        let session_type = session_type.to_string();
        let kiln = kiln.to_path_buf();
        let workspace = workspace.map(Path::to_path_buf);
        let connect_kilns: Vec<std::path::PathBuf> =
            connect_kilns.into_iter().map(Path::to_path_buf).collect();
        let recording_mode = recording_mode.map(str::to_string);
        let recording_path = recording_path.map(Path::to_path_buf);

        self.call_with_reconnect("session.create", move |daemon| {
            let session_type = session_type.clone();
            let kiln = kiln.clone();
            let workspace = workspace.clone();
            let connect_kilns = connect_kilns.clone();
            let recording_mode = recording_mode.clone();
            let recording_path = recording_path.clone();
            Box::pin(async move {
                let connect_kilns_ref: Vec<&Path> =
                    connect_kilns.iter().map(|p| p.as_path()).collect();
                daemon
                    .session_create(
                        &session_type,
                        &kiln,
                        workspace.as_deref(),
                        connect_kilns_ref,
                        recording_mode.as_deref(),
                        recording_path.as_deref(),
                    )
                    .await
            })
        })
        .await
    }

    pub async fn session_list(
        &self,
        kiln: Option<&Path>,
        workspace: Option<&Path>,
        session_type: Option<&str>,
        state: Option<&str>,
        include_archived: Option<bool>,
    ) -> anyhow::Result<serde_json::Value> {
        let kiln = kiln.map(Path::to_path_buf);
        let workspace = workspace.map(Path::to_path_buf);
        let session_type = session_type.map(str::to_string);
        let state = state.map(str::to_string);
        self.call_with_reconnect("session.list", move |daemon| {
            let kiln = kiln.clone();
            let workspace = workspace.clone();
            let session_type = session_type.clone();
            let state = state.clone();
            Box::pin(async move {
                daemon
                    .session_list(
                        kiln.as_deref(),
                        workspace.as_deref(),
                        session_type.as_deref(),
                        state.as_deref(),
                        include_archived,
                    )
                    .await
            })
        })
        .await
    }

    pub async fn session_search(
        &self,
        query: &str,
        kiln_path: Option<&Path>,
        limit: Option<usize>,
    ) -> anyhow::Result<serde_json::Value> {
        let query = query.to_string();
        let kiln_path = kiln_path.map(Path::to_path_buf);
        self.call_with_reconnect("session.search", move |daemon| {
            let query = query.clone();
            let kiln_path = kiln_path.clone();
            Box::pin(async move {
                daemon
                    .session_search(&query, kiln_path.as_deref(), limit)
                    .await
            })
        })
        .await
    }

    pub async fn session_get(&self, session_id: &str) -> anyhow::Result<serde_json::Value> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.get", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_get(&session_id).await })
        })
        .await
    }

    pub async fn session_resume_from_storage(
        &self,
        session_id: &str,
        kiln: &Path,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> anyhow::Result<serde_json::Value> {
        let session_id = session_id.to_string();
        let kiln = kiln.to_path_buf();
        self.call_with_reconnect("session.resume_from_storage", move |daemon| {
            let session_id = session_id.clone();
            let kiln = kiln.clone();
            Box::pin(async move {
                daemon
                    .session_resume_from_storage(&session_id, &kiln, limit, offset)
                    .await
            })
        })
        .await
    }

    pub async fn session_pause(&self, session_id: &str) -> anyhow::Result<serde_json::Value> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.pause", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_pause(&session_id).await })
        })
        .await
    }

    pub async fn session_resume(&self, session_id: &str) -> anyhow::Result<serde_json::Value> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.resume", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_resume(&session_id).await })
        })
        .await
    }

    pub async fn session_end(&self, session_id: &str) -> anyhow::Result<serde_json::Value> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.end", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_end(&session_id).await })
        })
        .await
    }

    pub async fn session_delete(
        &self,
        session_id: &str,
        kiln: &Path,
    ) -> anyhow::Result<serde_json::Value> {
        let session_id = session_id.to_string();
        let kiln = kiln.to_path_buf();
        self.call_with_reconnect("session.delete", move |daemon| {
            let session_id = session_id.clone();
            let kiln = kiln.clone();
            Box::pin(async move { daemon.session_delete(&session_id, &kiln).await })
        })
        .await
    }

    pub async fn session_archive(
        &self,
        session_id: &str,
        kiln: &Path,
    ) -> anyhow::Result<serde_json::Value> {
        let session_id = session_id.to_string();
        let kiln = kiln.to_path_buf();
        self.call_with_reconnect("session.archive", move |daemon| {
            let session_id = session_id.clone();
            let kiln = kiln.clone();
            Box::pin(async move { daemon.session_archive(&session_id, &kiln).await })
        })
        .await
    }

    pub async fn session_unarchive(
        &self,
        session_id: &str,
        kiln: &Path,
    ) -> anyhow::Result<serde_json::Value> {
        let session_id = session_id.to_string();
        let kiln = kiln.to_path_buf();
        self.call_with_reconnect("session.unarchive", move |daemon| {
            let session_id = session_id.clone();
            let kiln = kiln.clone();
            Box::pin(async move { daemon.session_unarchive(&session_id, &kiln).await })
        })
        .await
    }

    pub async fn session_cancel(&self, session_id: &str) -> anyhow::Result<bool> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.cancel", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_cancel(&session_id).await })
        })
        .await
    }

    pub async fn session_subscribe(
        &self,
        session_ids: &[&str],
    ) -> anyhow::Result<serde_json::Value> {
        let ids: Vec<String> = session_ids.iter().map(|id| (*id).to_string()).collect();
        self.call_with_reconnect("session.subscribe", move |daemon| {
            let ids = ids.clone();
            Box::pin(async move {
                let borrowed: Vec<&str> = ids.iter().map(String::as_str).collect();
                daemon.session_subscribe(&borrowed).await
            })
        })
        .await
    }

    pub async fn session_configure_agent(
        &self,
        session_id: &str,
        agent: &crucible_core::session::SessionAgent,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        let agent = agent.clone();
        self.call_with_reconnect("session.configure_agent", move |daemon| {
            let session_id = session_id.clone();
            let agent = agent.clone();
            Box::pin(async move { daemon.session_configure_agent(&session_id, &agent).await })
        })
        .await
    }

    pub async fn session_send_message(
        &self,
        session_id: &str,
        content: &str,
    ) -> anyhow::Result<String> {
        let session_id = session_id.to_string();
        let content = content.to_string();
        self.call_with_reconnect("session.send_message", move |daemon| {
            let session_id = session_id.clone();
            let content = content.clone();
            Box::pin(async move { daemon.session_send_message(&session_id, &content).await })
        })
        .await
    }

    pub async fn session_interaction_respond(
        &self,
        session_id: &str,
        request_id: &str,
        response: crucible_core::interaction::InteractionResponse,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        let request_id = request_id.to_string();
        self.call_with_reconnect("session.interaction_respond", move |daemon| {
            let session_id = session_id.clone();
            let request_id = request_id.clone();
            let response = response.clone();
            Box::pin(async move {
                daemon
                    .session_interaction_respond(&session_id, &request_id, response)
                    .await
            })
        })
        .await
    }

    pub async fn session_switch_model(
        &self,
        session_id: &str,
        model_id: &str,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        let model_id = model_id.to_string();
        self.call_with_reconnect("session.switch_model", move |daemon| {
            let session_id = session_id.clone();
            let model_id = model_id.clone();
            Box::pin(async move { daemon.session_switch_model(&session_id, &model_id).await })
        })
        .await
    }

    pub async fn session_set_title(&self, session_id: &str, title: &str) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        let title = title.to_string();
        self.call_with_reconnect("session.set_title", move |daemon| {
            let session_id = session_id.clone();
            let title = title.clone();
            Box::pin(async move { daemon.session_set_title(&session_id, &title).await })
        })
        .await
    }

    pub async fn session_list_models(&self, session_id: &str) -> anyhow::Result<Vec<String>> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.list_models", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_list_models(&session_id).await })
        })
        .await
    }

    pub async fn list_providers(
        &self,
        kiln_path: Option<&std::path::Path>,
    ) -> anyhow::Result<Vec<ProviderInfo>> {
        self.call_with_reconnect("providers.list", move |daemon| {
            let kiln_path = kiln_path.map(|p| p.to_path_buf());
            Box::pin(async move { daemon.list_providers(kiln_path.as_deref()).await })
        })
        .await
    }

    pub async fn session_set_thinking_budget(
        &self,
        session_id: &str,
        budget: Option<i64>,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.set_thinking_budget", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move {
                daemon
                    .session_set_thinking_budget(&session_id, budget)
                    .await
            })
        })
        .await
    }

    pub async fn session_get_thinking_budget(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<i64>> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.get_thinking_budget", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_get_thinking_budget(&session_id).await })
        })
        .await
    }

    pub async fn session_set_temperature(
        &self,
        session_id: &str,
        temperature: f64,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.set_temperature", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move {
                daemon
                    .session_set_temperature(&session_id, temperature)
                    .await
            })
        })
        .await
    }

    pub async fn session_get_temperature(&self, session_id: &str) -> anyhow::Result<Option<f64>> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.get_temperature", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_get_temperature(&session_id).await })
        })
        .await
    }

    pub async fn session_set_max_tokens(
        &self,
        session_id: &str,
        max_tokens: Option<u32>,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.set_max_tokens", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_set_max_tokens(&session_id, max_tokens).await })
        })
        .await
    }

    pub async fn session_get_max_tokens(&self, session_id: &str) -> anyhow::Result<Option<u32>> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.get_max_tokens", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_get_max_tokens(&session_id).await })
        })
        .await
    }

    pub async fn session_set_precognition(
        &self,
        session_id: &str,
        enabled: bool,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.set_precognition", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_set_precognition(&session_id, enabled).await })
        })
        .await
    }

    pub async fn session_get_precognition(&self, session_id: &str) -> anyhow::Result<bool> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.get_precognition", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_get_precognition(&session_id).await })
        })
        .await
    }

    pub async fn project_register(&self, path: &Path) -> anyhow::Result<crucible_core::Project> {
        let path = path.to_path_buf();
        self.call_with_reconnect("project.register", move |daemon| {
            let path = path.clone();
            Box::pin(async move { daemon.project_register(&path).await })
        })
        .await
    }

    pub async fn project_unregister(&self, path: &Path) -> anyhow::Result<()> {
        let path = path.to_path_buf();
        self.call_with_reconnect("project.unregister", move |daemon| {
            let path = path.clone();
            Box::pin(async move { daemon.project_unregister(&path).await })
        })
        .await
    }

    pub async fn project_list(&self) -> anyhow::Result<Vec<crucible_core::Project>> {
        self.call_with_reconnect("project.list", |daemon| Box::pin(daemon.project_list()))
            .await
    }

    pub async fn project_get(&self, path: &Path) -> anyhow::Result<Option<crucible_core::Project>> {
        let path = path.to_path_buf();
        self.call_with_reconnect("project.get", move |daemon| {
            let path = path.clone();
            Box::pin(async move { daemon.project_get(&path).await })
        })
        .await
    }

    pub async fn session_render_markdown(
        &self,
        session_dir: &Path,
        include_timestamps: Option<bool>,
        include_tokens: Option<bool>,
        include_tools: Option<bool>,
        max_content_length: Option<usize>,
    ) -> anyhow::Result<String> {
        let session_dir = session_dir.to_path_buf();
        self.call_with_reconnect("session.render_markdown", move |daemon| {
            let session_dir = session_dir.clone();
            Box::pin(async move {
                daemon
                    .session_render_markdown(
                        &session_dir,
                        include_timestamps,
                        include_tokens,
                        include_tools,
                        max_content_length,
                    )
                    .await
            })
        })
        .await
    }
}

pub struct EventBroker {
    sessions: RwLock<HashMap<String, broadcast::Sender<SessionEvent>>>,
}

impl Default for EventBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBroker {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn subscribe(&self, session_id: &str) -> broadcast::Receiver<SessionEvent> {
        let mut sessions = self.sessions.write().await;
        let tx = sessions
            .entry(session_id.to_string())
            .or_insert_with(|| broadcast::channel(EVENT_CHANNEL_CAPACITY).0);
        tx.subscribe()
    }

    async fn dispatch(&self, event: SessionEvent) {
        let sessions = self.sessions.read().await;
        if let Some(tx) = sessions.get(&event.session_id) {
            let _ = tx.send(event);
        }
    }

    pub async fn remove_session(&self, session_id: &str) {
        self.sessions.write().await.remove(session_id);
    }
}

pub async fn init_daemon(config: CliAppConfig) -> Result<AppState> {
    let (daemon, event_rx) = crucible_daemon::DaemonClient::connect_or_start_with_events()
        .await
        .map_err(|e| WebError::Daemon(format!("Failed to connect to daemon: {e}")))?;

    let daemon = Arc::new(ReconnectingDaemon::new(daemon));
    let broker = Arc::new(EventBroker::new());

    // Auto-register the configured kiln so the frontend has a project on startup
    let kiln_path = config.kiln_path_str().unwrap_or_default();
    if !kiln_path.is_empty() {
        if let Err(e) = daemon
            .project_register(std::path::Path::new(&kiln_path))
            .await
        {
            tracing::warn!("Failed to auto-register kiln {kiln_path}: {e}");
        }
    }

    spawn_event_router(event_rx, broker.clone());

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| WebError::Config(format!("Failed to create HTTP client: {e}")))?;

    Ok(AppState {
        daemon,
        events: broker,
        config: Arc::new(config),
        http_client,
    })
}

fn spawn_event_router(
    mut event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    broker: Arc<EventBroker>,
) {
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            broker.dispatch(event).await;
        }
        tracing::warn!("Daemon event stream ended");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a test SessionEvent
    fn test_event(session_id: &str, event_type: &str) -> SessionEvent {
        SessionEvent {
            session_id: session_id.to_string(),
            event_type: event_type.to_string(),
            data: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn new_creates_empty_broker() {
        let broker = EventBroker::new();
        let sessions = broker.sessions.read().await;
        assert_eq!(sessions.len(), 0, "New broker should have no sessions");
    }

    #[tokio::test]
    async fn subscribe_creates_channel_for_new_session() {
        let broker = EventBroker::new();
        let _rx = broker.subscribe("session-1").await;

        let sessions = broker.sessions.read().await;
        assert_eq!(sessions.len(), 1, "Should have one session after subscribe");
        assert!(
            sessions.contains_key("session-1"),
            "Session key should exist"
        );
    }

    #[tokio::test]
    async fn subscribe_twice_same_session_returns_two_receivers() {
        let broker = EventBroker::new();
        let rx1 = broker.subscribe("session-1").await;
        let rx2 = broker.subscribe("session-1").await;

        // Both receivers should be valid (not panicked)
        drop(rx1);
        drop(rx2);

        let sessions = broker.sessions.read().await;
        assert_eq!(sessions.len(), 1, "Should still have only one session");
    }

    #[tokio::test]
    async fn dispatch_sends_event_to_subscribers() {
        let broker = Arc::new(EventBroker::new());
        let mut rx = broker.subscribe("session-1").await;

        let event = test_event("session-1", "test_event");
        broker.dispatch(event.clone()).await;

        // Receive the event
        let received = rx.recv().await;
        assert!(received.is_ok(), "Should receive event");
        let received_event = received.unwrap();
        assert_eq!(received_event.session_id, "session-1");
        assert_eq!(received_event.event_type, "test_event");
    }

    #[tokio::test]
    async fn dispatch_ignores_unsubscribed_sessions() {
        let broker = Arc::new(EventBroker::new());

        let event = test_event("unknown-session", "test_event");
        // Should not panic
        broker.dispatch(event).await;
    }

    #[tokio::test]
    async fn remove_session_deletes_channel() {
        let broker = EventBroker::new();
        let _rx = broker.subscribe("session-1").await;

        {
            let sessions = broker.sessions.read().await;
            assert_eq!(sessions.len(), 1);
        }

        broker.remove_session("session-1").await;

        let sessions = broker.sessions.read().await;
        assert_eq!(sessions.len(), 0, "Session should be removed");
    }

    #[tokio::test]
    async fn multiple_subscribers_both_receive_event() {
        let broker = Arc::new(EventBroker::new());
        let mut rx1 = broker.subscribe("session-1").await;
        let mut rx2 = broker.subscribe("session-1").await;

        let event = test_event("session-1", "broadcast_test");
        broker.dispatch(event.clone()).await;

        // Both receivers should get the event
        let received1 = rx1.recv().await;
        let received2 = rx2.recv().await;

        assert!(received1.is_ok(), "Subscriber 1 should receive event");
        assert!(received2.is_ok(), "Subscriber 2 should receive event");

        assert_eq!(received1.unwrap().event_type, "broadcast_test");
        assert_eq!(received2.unwrap().event_type, "broadcast_test");
    }

    #[tokio::test]
    async fn multiple_sessions_receive_only_their_events() {
        let broker = Arc::new(EventBroker::new());
        let mut rx1 = broker.subscribe("session-1").await;
        let mut rx2 = broker.subscribe("session-2").await;

        let event1 = test_event("session-1", "event_for_1");
        let event2 = test_event("session-2", "event_for_2");

        broker.dispatch(event1).await;
        broker.dispatch(event2).await;

        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();

        assert_eq!(received1.event_type, "event_for_1");
        assert_eq!(received2.event_type, "event_for_2");
    }
}
