//! Unix socket server for JSON-RPC

use crate::agent_manager::{AgentError, AgentManager};
use crate::background_manager::BackgroundJobManager;
use crate::daemon_plugins::DaemonPluginLoader;
use crate::event_emitter::emit_event;
#[cfg(test)]
use crate::event_emitter::stamp_event;
use crate::kiln_manager::KilnManager;
use crate::mcp_server::McpServerManager;
use crate::project_manager::ProjectManager;
use crate::protocol::{
    Request, Response, SessionEventMessage, INTERNAL_ERROR, INVALID_PARAMS, METHOD_NOT_FOUND,
    PARSE_ERROR,
};
use crate::recording::RecordingWriter;
use crate::replay::ReplaySession;
use crate::rpc::{RpcContext, RpcDispatcher};
use crate::rpc_helpers::{optional_param, require_param};
use crate::session_manager::SessionManager;
use crate::session_storage::{FileSessionStorage, SessionStorage};
use anyhow::Result;
use crucible_config::{DataClassification, LlmConfig, TrustLevel};
use crucible_core::events::SessionEvent;
use crucible_core::session::RecordingMode;
use crucible_lua::stubs::StubGenerator;
use crucible_lua::{
    register_crucible_on_api, LuaExecutor, LuaScriptHandlerRegistry, PluginManager,
    ScriptHandlerResult, Session as LuaSession, SessionConfigRpc,
};
use crucible_skills::discovery::{default_discovery_paths, FolderDiscovery};
use dashmap::DashMap;

use crate::protocol::RequestId;
use crate::subscription::{ClientId, SubscriptionManager};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Log internal error details and return a generic error message.
fn internal_error(req_id: Option<RequestId>, err: impl std::fmt::Display) -> Response {
    error!("Internal error: {}", err);
    Response::error(req_id, INTERNAL_ERROR, "Internal server error")
}

/// Log client error details and return a sanitized error message.
fn invalid_state_error(
    req_id: Option<RequestId>,
    operation: &str,
    err: impl std::fmt::Display,
) -> Response {
    debug!("Invalid state for {}: {}", operation, err);
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("Operation '{}' not allowed in current state", operation),
    )
}

fn session_not_found(req_id: Option<RequestId>, session_id: &str) -> Response {
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("Session not found: {}", session_id),
    )
}

fn agent_not_configured(req_id: Option<RequestId>, session_id: &str) -> Response {
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("No agent configured for session: {}", session_id),
    )
}

fn concurrent_request(req_id: Option<RequestId>, session_id: &str) -> Response {
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("Request already in progress for session: {}", session_id),
    )
}

fn agent_error_to_response(req_id: Option<RequestId>, err: AgentError) -> Response {
    match err {
        AgentError::SessionNotFound(id) => session_not_found(req_id, &id),
        AgentError::NoAgentConfigured(id) => agent_not_configured(req_id, &id),
        AgentError::ConcurrentRequest(id) => concurrent_request(req_id, &id),
        e => internal_error(req_id, e),
    }
}

/// Daemon server that listens on a Unix socket
pub struct Server {
    listener: UnixListener,
    shutdown_tx: broadcast::Sender<()>,
    kiln_manager: Arc<KilnManager>,
    session_manager: Arc<SessionManager>,
    agent_manager: Arc<AgentManager>,
    #[allow(dead_code)] // Stored for Arc lifetime; accessed via AgentManager
    background_manager: Arc<BackgroundJobManager>,
    subscription_manager: Arc<SubscriptionManager>,
    project_manager: Arc<ProjectManager>,
    lua_sessions: Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    dispatcher: Arc<RpcDispatcher>,
    plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
    plugin_watch: bool,
    llm_config: Option<LlmConfig>,
    #[cfg(feature = "web")]
    web_config: Option<crucible_config::WebConfig>,
    mcp_server_manager: Arc<McpServerManager>,
}

struct NoopSessionRpc;

impl SessionConfigRpc for NoopSessionRpc {
    fn get_temperature(&self) -> Option<f64> {
        None
    }

    fn set_temperature(&self, _temp: f64) -> Result<(), String> {
        Ok(())
    }

    fn get_max_tokens(&self) -> Option<u32> {
        None
    }

    fn set_max_tokens(&self, _tokens: Option<u32>) -> Result<(), String> {
        Ok(())
    }

    fn get_thinking_budget(&self) -> Option<i64> {
        None
    }

    fn set_thinking_budget(&self, _budget: i64) -> Result<(), String> {
        Ok(())
    }

    fn get_model(&self) -> Option<String> {
        None
    }

    fn switch_model(&self, _model: &str) -> Result<(), String> {
        Ok(())
    }

    fn list_models(&self) -> Vec<String> {
        Vec::new()
    }

    fn get_mode(&self) -> String {
        "chat".to_string()
    }

    fn set_mode(&self, _mode: &str) -> Result<(), String> {
        Ok(())
    }

    fn notify(&self, _notification: crucible_core::types::Notification) {}

    fn toggle_messages(&self) {}

    fn show_messages(&self) {}

    fn hide_messages(&self) {}

    fn clear_messages(&self) {}
}

struct LuaSessionState {
    executor: LuaExecutor,
    registry: LuaScriptHandlerRegistry,
}

impl Server {
    /// Bind to a Unix socket path
    #[allow(dead_code)]
    pub async fn bind(
        path: &Path,
        mcp_config: Option<&crucible_config::McpConfig>,
    ) -> Result<Self> {
        Self::bind_with_plugin_config(
            path,
            mcp_config,
            std::collections::HashMap::new(),
            false,
            None,
            None,
            None,
            None,
        )
        .await
    }

    /// Bind to a Unix socket path with plugin configuration
    #[allow(clippy::too_many_arguments)]
    pub async fn bind_with_plugin_config(
        path: &Path,
        mcp_config: Option<&crucible_config::McpConfig>,
        plugin_config: std::collections::HashMap<String, serde_json::Value>,
        plugin_watch: bool,
        llm_config: Option<crucible_config::LlmConfig>,
        acp_config: Option<crucible_config::components::acp::AcpConfig>,
        permission_config: Option<crucible_config::components::permissions::PermissionConfig>,
        #[allow(unused_variables)] web_config: Option<crucible_config::WebConfig>,
    ) -> Result<Self> {
        // Remove stale socket
        if path.exists() {
            std::fs::remove_file(path)?;
        }

        // Create parent directory
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(path)?;
        let (shutdown_tx, _) = broadcast::channel(1);
        let (event_tx, _) = broadcast::channel(1024);

        use crucible_tools::mcp_gateway::McpGatewayManager;
        use tokio::sync::RwLock;

        let mcp_gateway = if let Some(mcp_cfg) = mcp_config {
            match McpGatewayManager::from_config(mcp_cfg).await {
                Ok(gw) => {
                    info!(
                        "MCP gateway initialized with {} upstream(s)",
                        gw.upstream_count()
                    );
                    Some(Arc::new(RwLock::new(gw)))
                }
                Err(e) => {
                    warn!("Failed to initialize MCP gateway: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let plugin_loader = Arc::new(Mutex::new(match DaemonPluginLoader::new(plugin_config) {
            Ok(loader) => {
                info!("Daemon plugin loader initialized");
                Some(loader)
            }
            Err(e) => {
                warn!("Failed to initialize daemon plugin loader: {}", e);
                None
            }
        }));

        let kiln_manager = Arc::new(KilnManager::with_event_tx(event_tx.clone()));
        let session_manager = Arc::new(SessionManager::new());
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
        let agent_manager = Arc::new(AgentManager::new(
            kiln_manager.clone(),
            session_manager.clone(),
            background_manager.clone(),
            mcp_gateway,
            llm_config.clone(),
            acp_config,
            permission_config,
            Some(plugin_loader.clone()),
        ));
        let subscription_manager = Arc::new(SubscriptionManager::new());
        let project_manager = Arc::new(ProjectManager::new(
            crucible_config::crucible_home().join("projects.json"),
        ));
        let lua_sessions = Arc::new(DashMap::new());
        let mcp_server_manager = Arc::new(McpServerManager::new());

        let ctx = RpcContext::new(
            kiln_manager.clone(),
            session_manager.clone(),
            agent_manager.clone(),
            subscription_manager.clone(),
            event_tx.clone(),
            shutdown_tx.clone(),
        );
        let dispatcher = Arc::new(RpcDispatcher::new(ctx));

        info!("Daemon listening on {:?}", path);
        Ok(Self {
            listener,
            shutdown_tx,
            kiln_manager,
            session_manager,
            agent_manager,
            background_manager,
            subscription_manager,
            project_manager,
            lua_sessions,
            event_tx,
            dispatcher,
            plugin_loader,
            plugin_watch,
            llm_config,
            mcp_server_manager,
            #[cfg(feature = "web")]
            web_config,
        })
    }

    /// Get a shutdown sender for external shutdown triggers
    #[allow(dead_code)]
    pub fn shutdown_handle(&self) -> broadcast::Sender<()> {
        self.shutdown_tx.clone()
    }

    /// Get a clone of the event broadcast sender.
    ///
    /// Used to send session events to all subscribed clients.
    #[allow(dead_code)]
    pub fn event_sender(&self) -> broadcast::Sender<SessionEventMessage> {
        self.event_tx.clone()
    }

    /// Run the server until shutdown
    pub async fn run(self) -> Result<()> {
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        {
            let mut loader_guard = self.plugin_loader.lock().await;
            if let Some(ref mut loader) = *loader_guard {
                // Upgrade sessions module with real daemon API before loading plugins
                let session_api: Arc<dyn crucible_lua::DaemonSessionApi> =
                    Arc::new(crate::session_bridge::DaemonSessionBridge::new(
                        self.session_manager.clone(),
                        self.agent_manager.clone(),
                        self.event_tx.clone(),
                    ));
                if let Err(e) = loader.upgrade_with_sessions(session_api) {
                    warn!("Failed to upgrade Lua sessions module: {}", e);
                }

                let workspace_root = crucible_config::crucible_home();
                let workspace_tools = Arc::new(crucible_tools::workspace::WorkspaceTools::new(
                    &workspace_root,
                ));
                let tools_api: Arc<dyn crucible_lua::DaemonToolsApi> =
                    Arc::new(crate::tools_bridge::DaemonToolsBridge::new(workspace_tools));
                if let Err(e) = loader.upgrade_with_tools(tools_api) {
                    warn!("Failed to upgrade Lua tools module: {}", e);
                }

                let paths = crate::daemon_plugins::default_daemon_plugin_paths();
                match loader.load_plugins(&paths).await {
                    Ok(specs) => {
                        if !specs.is_empty() {
                            info!("Loaded {} daemon plugin(s)", specs.len());
                        }
                    }
                    Err(e) => {
                        warn!("Failed to load daemon plugins: {}", e);
                    }
                }

                // Extract service functions and spawn them as independent async tasks.
                // Each mlua::Function holds an internal ref to the Lua VM; mlua's
                // reentrant mutex serializes actual Lua execution, giving cooperative
                // multitasking without external coordination.
                let service_fns = loader.take_service_fns();
                if !service_fns.is_empty() {
                    info!("Spawning {} plugin service(s)", service_fns.len());
                }
                for (name, func) in service_fns {
                    info!("Starting service: {}", name);
                    tokio::spawn(async move {
                        match func.call_async::<()>(()).await {
                            Ok(()) => info!("Service '{}' completed", name),
                            Err(e) => warn!("Service '{}' failed: {}", name, e),
                        }
                    });
                }

                if self.plugin_watch {
                    let plugin_dirs = loader.loaded_plugin_dirs();
                    if !plugin_dirs.is_empty() {
                        let plugin_loader_clone = self.plugin_loader.clone();
                        spawn_plugin_watcher(plugin_dirs, plugin_loader_clone);
                    }
                }
            }
        }

        #[cfg(feature = "web")]
        let web_cancel = {
            let cancel = CancellationToken::new();
            if let Some(ref config) = self.web_config {
                if config.enabled {
                    let config = config.clone();
                    let cancel_clone = cancel.clone();
                    info!(
                        "Starting embedded web server on http://{}:{}",
                        config.host, config.port
                    );
                    tokio::spawn(async move {
                        tokio::select! {
                            biased;
                            _ = cancel_clone.cancelled() => {
                                info!("Web server shutting down");
                            }
                            result = crucible_web::start_server(&config) => {
                                match result {
                                    Ok(()) => info!("Web server stopped"),
                                    Err(e) => warn!("Web server error: {}", e),
                                }
                            }
                        }
                    });
                }
            }
            cancel
        };

        // Spawn event persistence task with cancellation support
        let storage = FileSessionStorage::new();
        let sm_clone = self.session_manager.clone();
        let mut persist_rx = self.event_tx.subscribe();
        let persist_cancel = CancellationToken::new();
        let persist_cancel_clone = persist_cancel.clone();

        let persist_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = persist_cancel_clone.cancelled() => {
                        debug!("Persist task received shutdown signal, draining remaining events");
                        while let Ok(event) = persist_rx.try_recv() {
                            forward_to_recording(&sm_clone, &event);
                            if let Err(e) = persist_event(&event, &sm_clone, &storage).await {
                                warn!(session_id = %event.session_id, error = %e, "Failed to persist event during shutdown drain");
                            }
                        }
                        break;
                    }
                    result = persist_rx.recv() => {
                        match result {
                            Ok(event) => {
                                forward_to_recording(&sm_clone, &event);
                                if let Err(e) = persist_event(&event, &sm_clone, &storage).await {
                                    warn!(session_id = %event.session_id, event = %event.event, error = %e, "Failed to persist event");
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!(
                                    "Persist task lagged, dropped {} events", n
                                );
                                continue;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }
        });

        // Spawn file reprocessing task: watches for file_changed events and re-runs pipeline
        let km_reprocess = self.kiln_manager.clone();
        let mut reprocess_rx = self.event_tx.subscribe();
        let reprocess_cancel = CancellationToken::new();
        let reprocess_cancel_clone = reprocess_cancel.clone();

        let reprocess_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = reprocess_cancel_clone.cancelled() => break,
                    result = reprocess_rx.recv() => {
                        match result {
                            Ok(event)
                                if event.session_id == "system"
                                    && event.event == "file_changed" =>
                            {
                                let Some(path_str) =
                                    event.data.get("path").and_then(|v| v.as_str())
                                else {
                                    continue;
                                };
                                let file_path = PathBuf::from(path_str);

                                let Some(kiln_path) =
                                    km_reprocess.find_kiln_for_path(&file_path).await
                                else {
                                    debug!(path = %path_str, "File changed but no matching open kiln");
                                    continue;
                                };

                                match km_reprocess.process_file(&kiln_path, &file_path).await {
                                    Ok(true) => {
                                        info!(path = %path_str, "Reprocessed changed file");
                                    }
                                    Ok(false) => {
                                        debug!(path = %path_str, "File unchanged, skipped");
                                    }
                                    Err(e) => {
                                        warn!(
                                            path = %path_str,
                                            error = %e,
                                            "Failed to reprocess file"
                                        );
                                    }
                                }
                            }
                            Ok(event)
                                if event.session_id == "system"
                                    && event.event == "file_deleted" =>
                            {
                                let Some(path_str) =
                                    event.data.get("path").and_then(|v| v.as_str())
                                else {
                                    continue;
                                };

                                let file_path = PathBuf::from(path_str);
                                let Some(kiln_path) =
                                    km_reprocess.find_kiln_for_path(&file_path).await
                                else {
                                    debug!(path = %path_str, "File deleted but no matching open kiln");
                                    continue;
                                };

                                match km_reprocess
                                    .handle_file_deleted(&kiln_path, &file_path)
                                    .await
                                {
                                    Ok(true) => {
                                        info!(path = %path_str, "Removed deleted file from note store");
                                    }
                                    Ok(false) => {
                                        debug!(path = %path_str, "Deleted file ignored or not found in note store");
                                    }
                                    Err(e) => {
                                        warn!(
                                            path = %path_str,
                                            error = %e,
                                            "Failed to handle deleted file"
                                        );
                                    }
                                }
                            }
                            Ok(_) => {}
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!("Reprocess task lagged, dropped {} events", n);
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }
        });

        loop {
            tokio::select! {
                accept_result = self.listener.accept() => {
                    match accept_result {
                        Ok((stream, _)) => {
                            let ctx = Arc::new(ServerContext {
                                dispatcher: self.dispatcher.clone(),
                                kiln_manager: self.kiln_manager.clone(),
                                session_manager: self.session_manager.clone(),
                                agent_manager: self.agent_manager.clone(),
                                subscription_manager: self.subscription_manager.clone(),
                                project_manager: self.project_manager.clone(),
                                lua_sessions: self.lua_sessions.clone(),
                                event_tx: self.event_tx.clone(),
                                plugin_loader: self.plugin_loader.clone(),
                                llm_config: self.llm_config.clone(),
                                mcp_server_manager: self.mcp_server_manager.clone(),
                            });
                            let event_rx = ctx.event_tx.subscribe();
                            tokio::spawn(async move {
                                if let Err(e) = handle_client(stream, ctx, event_rx).await {
                                    error!("Client error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Accept error: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received");
                    break;
                }
            }
        }

        // Graceful shutdown: signal cancellation, wait with timeout, then abort if needed
        #[cfg(feature = "web")]
        web_cancel.cancel();
        persist_cancel.cancel();
        reprocess_cancel.cancel();
        match tokio::time::timeout(std::time::Duration::from_secs(5), persist_task).await {
            Ok(Ok(())) => debug!("Persist task completed gracefully"),
            Ok(Err(e)) => warn!("Persist task panicked: {}", e),
            Err(_) => warn!("Persist task did not complete within timeout, aborting"),
        }
        match tokio::time::timeout(std::time::Duration::from_secs(5), reprocess_task).await {
            Ok(Ok(())) => debug!("Reprocess task completed gracefully"),
            Ok(Err(e)) => warn!("Reprocess task panicked: {}", e),
            Err(_) => warn!("Reprocess task did not complete within timeout, aborting"),
        }

        Ok(())
    }
}

#[derive(Clone)]
struct ServerContext {
    dispatcher: Arc<RpcDispatcher>,
    kiln_manager: Arc<KilnManager>,
    session_manager: Arc<SessionManager>,
    agent_manager: Arc<AgentManager>,
    subscription_manager: Arc<SubscriptionManager>,
    project_manager: Arc<ProjectManager>,
    lua_sessions: Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
    llm_config: Option<LlmConfig>,
    mcp_server_manager: Arc<McpServerManager>,
}

async fn handle_client(
    stream: UnixStream,
    ctx: Arc<ServerContext>,
    mut event_rx: broadcast::Receiver<SessionEventMessage>,
) -> Result<()> {
    let client_id = ClientId::new();
    let (reader, writer) = stream.into_split();
    let writer: Arc<Mutex<OwnedWriteHalf>> = Arc::new(Mutex::new(writer));
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    let writer_clone = writer.clone();
    let sub_manager = ctx.subscription_manager.clone();
    let event_cancel = CancellationToken::new();
    let event_cancel_clone = event_cancel.clone();
    let event_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = event_cancel_clone.cancelled() => break,
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            if sub_manager.is_subscribed(client_id, &event.session_id) {
                                if let Ok(json) = event.to_json_line() {
                                    let mut w = writer_clone.lock().await;
                                    if w.write_all(json.as_bytes()).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                "Event forwarder lagged, dropped {} events for client {}", n, client_id
                            );
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(req) => handle_request(req, client_id, &ctx).await,
            Err(e) => {
                warn!("Parse error: {}", e);
                Response::error(None, PARSE_ERROR, e.to_string())
            }
        };

        let mut output = serde_json::to_string(&response)?;
        output.push('\n');

        let mut w = writer.lock().await;
        w.write_all(output.as_bytes()).await?;
    }

    // Graceful shutdown of event forwarding
    event_cancel.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_task).await;
    ctx.subscription_manager.remove_client(client_id);

    Ok(())
}

fn forward_to_recording(sm: &SessionManager, event: &SessionEventMessage) {
    if let Some(tx) = sm.get_recording_sender(&event.session_id) {
        if tx.try_send(event.clone()).is_err() {
            warn!(
                session_id = %event.session_id,
                "Recording channel full or closed, dropping event"
            );
        }
    }
}

fn should_persist(event: &SessionEventMessage) -> bool {
    if event.msg_type != "event" {
        return false;
    }

    matches!(
        event.event.as_str(),
        "user_message"
            | "message_complete"
            | "tool_call"
            | "tool_result"
            | "model_switched"
            | "ended"
    )
}

async fn persist_event(
    event: &SessionEventMessage,
    sm: &SessionManager,
    storage: &dyn SessionStorage,
) -> Result<()> {
    if !should_persist(event) {
        return Ok(());
    }
    let session = match sm.get_session(&event.session_id) {
        Some(s) => s,
        None => return Ok(()),
    };

    let json = serde_json::to_string(event)?;
    storage
        .append_event(&session, &json)
        .await
        .map_err(|e| anyhow::anyhow!("append_event failed: {}", e))?;

    match event.event.as_str() {
        "user_message" => {
            if let Some(content) = event.data.get("content").and_then(|v| v.as_str()) {
                storage
                    .append_markdown(&session, "User", content)
                    .await
                    .map_err(|e| anyhow::anyhow!("append_markdown(User) failed: {}", e))?;
            }
        }
        "message_complete" => {
            if let Some(content) = event.data.get("full_response").and_then(|v| v.as_str()) {
                storage
                    .append_markdown(&session, "Assistant", content)
                    .await
                    .map_err(|e| anyhow::anyhow!("append_markdown(Assistant) failed: {}", e))?;
            }
        }
        _ => {}
    }
    Ok(())
}

async fn handle_request(req: Request, client_id: ClientId, ctx: &ServerContext) -> Response {
    let req_clone = req.clone();
    let resp = ctx.dispatcher.dispatch(client_id, req).await;

    if let Some(ref err) = resp.error {
        if err.code == METHOD_NOT_FOUND && err.message.contains("not yet migrated") {
            return handle_legacy_request(
                req_clone,
                &ctx.kiln_manager,
                &ctx.session_manager,
                &ctx.agent_manager,
                &ctx.project_manager,
                &ctx.lua_sessions,
                &ctx.event_tx,
                &ctx.plugin_loader,
                &ctx.llm_config,
                &ctx.mcp_server_manager,
            )
            .await;
        }
    }

    resp
}

#[allow(clippy::too_many_arguments)]
async fn handle_legacy_request(
    req: Request,
    kiln_manager: &Arc<KilnManager>,
    session_manager: &Arc<SessionManager>,
    agent_manager: &Arc<AgentManager>,
    project_manager: &Arc<ProjectManager>,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
    llm_config: &Option<LlmConfig>,
    mcp_server_manager: &Arc<McpServerManager>,
) -> Response {
    tracing::debug!("Legacy handler for method={:?}", req.method);

    match req.method.as_str() {
        "kiln.open" => handle_kiln_open(req, kiln_manager, plugin_loader, event_tx).await,
        "kiln.close" => handle_kiln_close(req, kiln_manager).await,
        "kiln.list" => handle_kiln_list(req, kiln_manager).await,
        "kiln.set_classification" => handle_kiln_set_classification(req, kiln_manager).await,
        "search_vectors" => handle_search_vectors(req, kiln_manager).await,
        "list_notes" => handle_list_notes(req, kiln_manager).await,
        "get_note_by_name" => handle_get_note_by_name(req, kiln_manager).await,
        "note.upsert" => handle_note_upsert(req, kiln_manager).await,
        "note.get" => handle_note_get(req, kiln_manager).await,
        "note.delete" => handle_note_delete(req, kiln_manager).await,
        "note.list" => handle_note_list(req, kiln_manager).await,
        "models.list" => handle_models_list(req, agent_manager).await,
        "process_file" => handle_process_file(req, kiln_manager).await,
        "process_batch" => handle_process_batch(req, kiln_manager, event_tx).await,
        "session.create" => {
            handle_session_create(req, session_manager, project_manager, llm_config).await
        }
        "session.list" => handle_session_list(req, session_manager).await,
        "session.get" => handle_session_get(req, session_manager).await,
        "session.pause" => handle_session_pause(req, session_manager).await,
        "session.resume" => handle_session_resume(req, session_manager).await,
        "session.resume_from_storage" => {
            handle_session_resume_from_storage(req, session_manager).await
        }
        "session.end" => handle_session_end(req, session_manager, agent_manager).await,
        "session.compact" => handle_session_compact(req, session_manager).await,
        "session.configure_agent" => handle_session_configure_agent(req, agent_manager).await,
        "session.send_message" => handle_session_send_message(req, agent_manager, event_tx).await,
        "session.cancel" => handle_session_cancel(req, agent_manager).await,
        "session.interaction_respond" => {
            handle_session_interaction_respond(req, agent_manager, event_tx).await
        }
        "session.switch_model" => handle_session_switch_model(req, agent_manager, event_tx).await,
        "session.list_models" => handle_session_list_models(req, agent_manager).await,
        "session.set_thinking_budget" => {
            handle_session_set_thinking_budget(req, agent_manager, event_tx).await
        }
        "session.get_thinking_budget" => {
            handle_session_get_thinking_budget(req, agent_manager).await
        }
        "session.set_precognition" => {
            handle_session_set_precognition(req, agent_manager, event_tx).await
        }
        "session.get_precognition" => handle_session_get_precognition(req, agent_manager).await,
        "session.add_notification" => {
            handle_session_add_notification(req, agent_manager, event_tx).await
        }
        "session.list_notifications" => handle_session_list_notifications(req, agent_manager).await,
        "session.dismiss_notification" => {
            handle_session_dismiss_notification(req, agent_manager, event_tx).await
        }
        "session.set_temperature" => {
            handle_session_set_temperature(req, agent_manager, event_tx).await
        }
        "session.get_temperature" => handle_session_get_temperature(req, agent_manager).await,
        "session.set_max_tokens" => {
            handle_session_set_max_tokens(req, agent_manager, event_tx).await
        }
        "session.get_max_tokens" => handle_session_get_max_tokens(req, agent_manager).await,
        "session.test_interaction" => handle_session_test_interaction(req, event_tx).await,
        "session.replay" => handle_session_replay(req, session_manager, event_tx).await,
        "plugin.reload" => handle_plugin_reload(req, plugin_loader).await,
        "plugin.list" => handle_plugin_list(req, plugin_loader).await,
        "project.register" => handle_project_register(req, project_manager).await,
        "project.unregister" => handle_project_unregister(req, project_manager).await,
        "project.list" => handle_project_list(req, project_manager).await,
        "project.get" => handle_project_get(req, project_manager).await,
        "storage.verify" => handle_storage_verify(req).await,
        "storage.cleanup" => handle_storage_cleanup(req).await,
        "storage.backup" => handle_storage_backup(req).await,
        "storage.restore" => handle_storage_restore(req).await,
        "session.search" => handle_session_search(req, session_manager).await,
        "session.load_events" => handle_session_load_events(req).await,
        "session.list_persisted" => handle_session_list_persisted(req).await,
        "session.render_markdown" => handle_session_render_markdown(req).await,
        "session.export_to_file" => handle_session_export_to_file(req).await,
        "session.cleanup" => handle_session_cleanup(req).await,
        "session.reindex" => handle_session_reindex(req, kiln_manager).await,
        "lua.init_session" => handle_lua_init_session(req, lua_sessions).await,
        "lua.register_hooks" => handle_lua_register_hooks(req, lua_sessions).await,
        "lua.execute_hook" => handle_lua_execute_hook(req, lua_sessions).await,
        "lua.shutdown_session" => handle_lua_shutdown_session(req, lua_sessions).await,
        "lua.discover_plugins" => handle_lua_discover_plugins(req).await,
        "lua.plugin_health" => handle_lua_plugin_health(req).await,
        "lua.generate_stubs" => handle_lua_generate_stubs(req).await,
        "lua.run_plugin_tests" => handle_lua_run_plugin_tests(req).await,
        "lua.register_commands" => handle_lua_register_commands(req, lua_sessions).await,
        "mcp.start" => handle_mcp_start(req, kiln_manager, mcp_server_manager).await,
        "mcp.stop" => handle_mcp_stop(req, mcp_server_manager).await,
        "mcp.status" => handle_mcp_status(req, mcp_server_manager).await,
        "skills.list" => handle_skills_list(req).await,
        "skills.get" => handle_skills_get(req).await,
        "skills.search" => handle_skills_search(req).await,
        "agents.list_profiles" => handle_agents_list_profiles(req, agent_manager).await,
        "agents.resolve_profile" => handle_agents_resolve_profile(req, agent_manager).await,
        _ => {
            tracing::warn!("Unknown RPC method: {:?}", req.method);
            Response::error(
                req.id,
                METHOD_NOT_FOUND,
                format!("Unknown method: {}", req.method),
            )
        }
    }
}

async fn handle_kiln_open(
    req: Request,
    km: &Arc<KilnManager>,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let path = require_param!(req, "path", as_str);
    let kiln_path = Path::new(path);

    let process = optional_param!(req, "process", as_bool).unwrap_or(false);
    let force = optional_param!(req, "force", as_bool).unwrap_or(false);

    if let Err(e) = km.open(kiln_path).await {
        return internal_error(req.id, e);
    }

    if let Some(handle) = km.get(kiln_path).await {
        let store = handle.as_note_store();
        let loader_guard = plugin_loader.lock().await;
        if let Some(ref loader) = *loader_guard {
            if let Err(e) = loader.upgrade_with_storage(store, kiln_path) {
                warn!("Failed to upgrade Lua modules with storage: {}", e);
            }
        }
    }

    if process {
        match km.open_and_process(kiln_path, force).await {
            Ok((discovered, processed, skipped, errors)) => {
                let _ = event_tx.send(SessionEventMessage::new(
                    "process",
                    "process_complete",
                    serde_json::json!({
                        "kiln": path,
                        "discovered": discovered,
                        "processed": processed,
                        "skipped": skipped,
                        "errors": errors.len()
                    }),
                ));

                Response::success(
                    req.id,
                    serde_json::json!({
                        "status": "ok",
                        "discovered": discovered,
                        "processed": processed,
                        "skipped": skipped,
                        "errors": errors.iter().map(|(p, e)| {
                            serde_json::json!({"path": p.to_string_lossy(), "error": e})
                        }).collect::<Vec<_>>()
                    }),
                )
            }
            Err(e) => {
                warn!("Processing failed for kiln {:?}: {}", kiln_path, e);
                Response::success(
                    req.id,
                    serde_json::json!({
                        "status": "ok",
                        "process_error": e.to_string()
                    }),
                )
            }
        }
    } else {
        Response::success(req.id, serde_json::json!({"status": "ok"}))
    }
}

async fn handle_kiln_close(req: Request, km: &Arc<KilnManager>) -> Response {
    let path = require_param!(req, "path", as_str);

    match km.close(Path::new(path)).await {
        Ok(()) => Response::success(req.id, serde_json::json!({"status": "ok"})),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_kiln_list(req: Request, km: &Arc<KilnManager>) -> Response {
    let kilns = km.list().await;
    let list: Vec<_> = kilns
        .iter()
        .map(|(path, last_access)| {
            serde_json::json!({
                "path": path.to_string_lossy(),
                "last_access_secs_ago": last_access.elapsed().as_secs()
            })
        })
        .collect();
    Response::success(req.id, list)
}

async fn handle_kiln_set_classification(req: Request, _km: &Arc<KilnManager>) -> Response {
    let path_str = require_param!(req, "path", as_str);
    let classification_str = require_param!(req, "classification", as_str);

    let classification = match DataClassification::from_str_insensitive(classification_str) {
        Some(c) => c,
        None => {
            let valid: Vec<&str> = DataClassification::all()
                .iter()
                .map(|c| c.as_str())
                .collect();
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!(
                    "Invalid classification '{}'. Valid values: {}",
                    classification_str,
                    valid.join(", ")
                ),
            );
        }
    };

    let workspace = Path::new(path_str);
    let crucible_dir = workspace.join(".crucible");
    if let Err(e) = std::fs::create_dir_all(&crucible_dir) {
        return internal_error(req.id, e);
    }

    let config_path = crucible_dir.join("workspace.toml");
    let mut config = if config_path.exists() {
        match std::fs::read_to_string(&config_path) {
            Ok(content) => match toml::from_str::<crucible_config::WorkspaceConfig>(&content) {
                Ok(c) => c,
                Err(e) => {
                    return internal_error(
                        req.id,
                        format!("Failed to parse workspace.toml: {}", e),
                    );
                }
            },
            Err(e) => return internal_error(req.id, e),
        }
    } else {
        // Create a minimal workspace config with the kiln path as "."
        crucible_config::WorkspaceConfig {
            workspace: crucible_config::WorkspaceMeta {
                name: workspace
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "workspace".to_string()),
            },
            kilns: vec![crucible_config::KilnAttachment {
                path: ".".into(),
                name: None,
                data_classification: None,
            }],
            security: Default::default(),
        }
    };

    // Update classification on the first kiln entry (or the matching one)
    let mut updated = false;
    if let Some(kiln) = config.kilns.iter_mut().next() {
        kiln.data_classification = Some(classification);
        updated = true;
    }

    if !updated {
        // No kiln entries — add one
        config.kilns.push(crucible_config::KilnAttachment {
            path: ".".into(),
            name: None,
            data_classification: Some(classification),
        });
    }

    let toml_str = match toml::to_string_pretty(&config) {
        Ok(s) => s,
        Err(e) => return internal_error(req.id, e),
    };

    if let Err(e) = std::fs::write(&config_path, toml_str) {
        return internal_error(req.id, e);
    }

    info!(
        "Set data classification to '{}' for workspace at {:?}",
        classification.as_str(),
        workspace
    );

    Response::success(
        req.id,
        serde_json::json!({
            "status": "ok",
            "classification": classification.as_str(),
            "path": path_str,
        }),
    )
}

async fn handle_search_vectors(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let vector_arr = require_param!(req, "vector", as_array);
    let vector: Vec<f32> = vector_arr
        .iter()
        .filter_map(|v: &serde_json::Value| v.as_f64().map(|f| f as f32))
        .collect();
    let limit = optional_param!(req, "limit", as_u64).unwrap_or(20) as usize;

    // Get or open connection to the kiln
    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    // Execute vector search using the backend-agnostic method
    match handle.search_vectors(vector, limit).await {
        Ok(results) => {
            let json_results: Vec<_> = results
                .into_iter()
                .map(|(doc_id, score)| {
                    serde_json::json!({
                        "document_id": doc_id,
                        "score": score
                    })
                })
                .collect();
            Response::success(req.id, json_results)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_list_notes(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let path_filter = optional_param!(req, "path_filter", as_str);

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    match handle.list_notes(path_filter).await {
        Ok(notes) => {
            let json_notes: Vec<_> = notes
                .into_iter()
                .map(|n| {
                    serde_json::json!({
                        "name": n.name,
                        "path": n.path,
                        "title": n.title,
                        "tags": n.tags,
                        "updated_at": n.updated_at.map(|t| t.to_rfc3339())
                    })
                })
                .collect();
            Response::success(req.id, json_notes)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_get_note_by_name(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let name = require_param!(req, "name", as_str);

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    match handle.get_note_by_name(name).await {
        Ok(Some(note)) => Response::success(
            req.id,
            serde_json::json!({
                "path": note.path,
                "title": note.title,
                "tags": note.tags,
                "links_to": note.links_to,
                "content_hash": note.content_hash.to_string()
            }),
        ),
        Ok(None) => Response::success(req.id, serde_json::Value::Null),
        Err(e) => internal_error(req.id, e),
    }
}

// =============================================================================
// NoteStore RPC Handlers
// =============================================================================

async fn handle_note_upsert(req: Request, km: &Arc<KilnManager>) -> Response {
    use crucible_core::storage::NoteRecord;

    let kiln_path = require_param!(req, "kiln", as_str);

    let note_json = match req.params.get("note") {
        Some(n) => n,
        None => return Response::error(req.id, INVALID_PARAMS, "Missing 'note' parameter"),
    };

    let note: NoteRecord = match serde_json::from_value(note_json.clone()) {
        Ok(n) => n,
        Err(e) => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!("Invalid note record: {}", e),
            )
        }
    };

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.upsert(note).await {
        Ok(events) => Response::success(
            req.id,
            serde_json::json!({
                "status": "ok",
                "events_count": events.len()
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_note_get(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let path = require_param!(req, "path", as_str);

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.get(path).await {
        Ok(Some(note)) => match serde_json::to_value(&note) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => internal_error(req.id, e),
        },
        Ok(None) => Response::success(req.id, serde_json::Value::Null),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_note_delete(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let path = require_param!(req, "path", as_str);

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.delete(path).await {
        Ok(_event) => Response::success(req.id, serde_json::json!({"status": "ok"})),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_note_list(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.list().await {
        Ok(notes) => match serde_json::to_value(&notes) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => internal_error(req.id, e),
        },
        Err(e) => internal_error(req.id, e),
    }
}

// =============================================================================
// Pipeline RPC Handlers
// =============================================================================

async fn handle_process_file(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let file_path = require_param!(req, "path", as_str);

    match km
        .process_file(Path::new(kiln_path), Path::new(file_path))
        .await
    {
        Ok(processed) => Response::success(
            req.id,
            serde_json::json!({
                "status": if processed { "processed" } else { "skipped" },
                "path": file_path
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_process_batch(
    req: Request,
    km: &Arc<KilnManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let request_id = req.id.clone();
    let kiln_path = require_param!(req, "kiln", as_str);
    let paths_arr = require_param!(req, "paths", as_array);
    let paths: Vec<std::path::PathBuf> = paths_arr
        .iter()
        .filter_map(|v: &serde_json::Value| v.as_str().map(std::path::PathBuf::from))
        .collect();
    let batch_id = request_id
        .as_ref()
        .map(|id| match id {
            RequestId::Number(n) => format!("batch-{}", n),
            RequestId::String(s) => format!("batch-{}", s),
        })
        .unwrap_or_else(|| "batch-unknown".to_string());

    // Emit start event
    let _ = event_tx.send(SessionEventMessage::new(
        "process",
        "process_start",
        serde_json::json!({
            "type": "process_start",
            "batch_id": &batch_id,
            "total": paths.len(),
            "kiln": kiln_path
        }),
    ));

    let mut processed = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<(PathBuf, String)> = Vec::new();

    for path in &paths {
        match km.process_file(Path::new(kiln_path), path).await {
            Ok(true) => {
                processed += 1;
                let _ = event_tx.send(SessionEventMessage::new(
                    "process",
                    "process_progress",
                    serde_json::json!({
                        "type": "process_progress",
                        "batch_id": &batch_id,
                        "file": path.to_string_lossy(),
                        "result": "processed"
                    }),
                ));
            }
            Ok(false) => {
                skipped += 1;
                let _ = event_tx.send(SessionEventMessage::new(
                    "process",
                    "process_progress",
                    serde_json::json!({
                        "type": "process_progress",
                        "batch_id": &batch_id,
                        "file": path.to_string_lossy(),
                        "result": "skipped"
                    }),
                ));
            }
            Err(e) => {
                let error_msg = e.to_string();
                errors.push((path.clone(), error_msg.clone()));
                let _ = event_tx.send(SessionEventMessage::new(
                    "process",
                    "process_progress",
                    serde_json::json!({
                        "type": "process_progress",
                        "batch_id": &batch_id,
                        "file": path.to_string_lossy(),
                        "result": "error",
                        "error_msg": error_msg
                    }),
                ));
            }
        }
    }

    // Emit completion event
    let _ = event_tx.send(SessionEventMessage::new(
        "process",
        "process_complete",
        serde_json::json!({
            "type": "process_complete",
            "batch_id": &batch_id,
            "processed": processed,
            "skipped": skipped,
            "errors": errors.len()
        }),
    ));

    Response::success(
        request_id,
        serde_json::json!({
            "processed": processed,
            "skipped": skipped,
            "errors": errors
                .iter()
                .map(|(p, err)| {
                    serde_json::json!({
                        "path": p.to_string_lossy(),
                        "error": err
                    })
                })
                .collect::<Vec<_>>()
        }),
    )
}

async fn handle_lua_init_session(
    req: Request,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str).to_string();
    let kiln_root = optional_param!(req, "kiln_path", as_str)
        .or_else(|| optional_param!(req, "kiln", as_str))
        .map(PathBuf::from)
        .unwrap_or_else(crucible_config::crucible_home);

    let mut executor = match LuaExecutor::new() {
        Ok(executor) => executor,
        Err(e) => return internal_error(req.id, e),
    };

    if let Err(e) = executor.load_config(Some(&kiln_root)) {
        warn!(
            session_id = %session_id,
            kiln = %kiln_root.display(),
            error = %e,
            "Failed to load Lua config"
        );
    }

    let session = LuaSession::new("chat".to_string());
    session.bind(Box::new(NoopSessionRpc));
    executor.session_manager().set_current(session.clone());

    if let Err(e) = executor.sync_session_start_hooks() {
        warn!(session_id = %session_id, error = %e, "Failed to sync session_start hooks");
    }
    if let Err(e) = executor.fire_session_start_hooks(&session) {
        warn!(session_id = %session_id, error = %e, "Failed to fire session_start hooks");
    }

    let registry = LuaScriptHandlerRegistry::new();
    if let Err(e) = register_crucible_on_api(
        executor.lua(),
        registry.runtime_handlers(),
        registry.handler_functions(),
    ) {
        warn!(session_id = %session_id, error = %e, "Failed to register crucible.on API");
    }

    lua_sessions.insert(
        session_id.clone(),
        Arc::new(Mutex::new(LuaSessionState { executor, registry })),
    );

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "commands": [],
            "views": [],
        }),
    )
}

async fn handle_lua_register_hooks(
    req: Request,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let hooks = require_param!(req, "hooks", as_array);

    let Some(state) = lua_sessions.get(session_id) else {
        return session_not_found(req.id, session_id);
    };

    let state = state.value().clone();
    let state = state.lock().await;
    let initial_count = state
        .registry
        .runtime_handlers()
        .lock()
        .map(|handlers| handlers.len())
        .unwrap_or(0);

    for hook in hooks {
        let source = if let Some(source) = hook.as_str() {
            Some(source)
        } else if let Some(obj) = hook.as_object() {
            obj.get("source")
                .and_then(|v| v.as_str())
                .or_else(|| obj.get("code").and_then(|v| v.as_str()))
        } else {
            None
        };

        if let Some(source) = source {
            if let Err(e) = state.executor.lua().load(source).exec() {
                warn!(session_id = %session_id, error = %e, "Failed to register Lua hook source");
            }
        }
    }

    let final_count = state
        .registry
        .runtime_handlers()
        .lock()
        .map(|handlers| handlers.len())
        .unwrap_or(initial_count);
    let registered = final_count.saturating_sub(initial_count);
    Response::success(
        req.id,
        serde_json::json!({
            "status": "ok",
            "registered": registered,
        }),
    )
}

async fn handle_lua_execute_hook(
    req: Request,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let hook_name = require_param!(req, "hook_name", as_str);
    let context = req
        .params
        .get("context")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    let Some(state) = lua_sessions.get(session_id) else {
        return session_not_found(req.id, session_id);
    };

    let state = state.value().clone();
    let state = state.lock().await;
    let handlers = state.registry.runtime_handlers_for(hook_name);
    let mut results = Vec::new();

    for handler in handlers {
        let event = SessionEvent::Custom {
            name: hook_name.to_string(),
            payload: context.clone(),
        };

        let result = match state.registry.execute_runtime_handler(
            state.executor.lua(),
            &handler.name,
            &event,
        ) {
            Ok(ScriptHandlerResult::Transform(payload)) => {
                serde_json::json!({"handler": handler.name, "type": "transform", "payload": payload})
            }
            Ok(ScriptHandlerResult::PassThrough) => {
                serde_json::json!({"handler": handler.name, "type": "pass_through"})
            }
            Ok(ScriptHandlerResult::Cancel { reason }) => {
                serde_json::json!({"handler": handler.name, "type": "cancel", "reason": reason})
            }
            Ok(ScriptHandlerResult::Inject { content, position }) => serde_json::json!({
                "handler": handler.name,
                "type": "inject",
                "content": content,
                "position": position,
            }),
            Err(e) => {
                serde_json::json!({"handler": handler.name, "type": "error", "error": e.to_string()})
            }
        };
        results.push(result);
    }

    Response::success(
        req.id,
        serde_json::json!({
            "executed": results.len(),
            "results": results,
        }),
    )
}

async fn handle_lua_shutdown_session(
    req: Request,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let removed = lua_sessions.remove(session_id).is_some();
    Response::success(
        req.id,
        serde_json::json!({
            "shutdown": removed,
        }),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Lua plugin management RPC handlers
// ─────────────────────────────────────────────────────────────────────────────

async fn handle_lua_discover_plugins(req: Request) -> Response {
    let kiln_path = require_param!(req, "kiln_path", as_str).to_string();

    match PluginManager::initialize(Some(Path::new(&kiln_path))) {
        Ok(manager) => {
            let plugins: Vec<serde_json::Value> = manager
                .list()
                .map(|p| {
                    serde_json::json!({
                        "name": p.name(),
                        "version": p.version(),
                        "state": p.state.to_string(),
                        "error": p.last_error,
                    })
                })
                .collect();
            Response::success(req.id, serde_json::json!({ "plugins": plugins }))
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_lua_plugin_health(req: Request) -> Response {
    let plugin_path_str = require_param!(req, "plugin_path", as_str).to_string();
    let plugin_path = PathBuf::from(&plugin_path_str);

    if !plugin_path.exists() {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Plugin path does not exist: {}", plugin_path.display()),
        );
    }

    // Find health.lua in the plugin directory
    let health_path = if plugin_path.file_name().and_then(|n| n.to_str()) == Some("health.lua") {
        Some(plugin_path.clone())
    } else {
        let hp = plugin_path.join("health.lua");
        if hp.exists() {
            Some(hp)
        } else {
            None
        }
    };

    let Some(health_path) = health_path else {
        return Response::success(
            req.id,
            serde_json::json!({
                "name": plugin_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"),
                "healthy": true,
                "checks": [],
                "message": "No health.lua found"
            }),
        );
    };

    let executor = match LuaExecutor::new() {
        Ok(e) => e,
        Err(e) => return internal_error(req.id, e),
    };
    let lua = executor.lua();

    // Setup test mocks (in case health checks use cru.* APIs)
    if let Err(e) = lua
        .load("test_mocks = test_mocks or {}; test_mocks.setup = function() end")
        .exec()
    {
        return internal_error(req.id, e);
    }

    // Load and execute health.lua
    let health_lua = match std::fs::read_to_string(&health_path) {
        Ok(s) => s,
        Err(e) => return internal_error(req.id, e),
    };

    let health_module: mlua::Table = match lua.load(&health_lua).eval() {
        Ok(t) => t,
        Err(e) => return internal_error(req.id, e),
    };

    let check_fn: mlua::Function = match health_module.get("check") {
        Ok(f) => f,
        Err(e) => return internal_error(req.id, e),
    };

    if let Err(e) = check_fn.call::<()>(()) {
        return internal_error(req.id, e);
    }

    // Get results from cru.health.get_results
    let get_results: mlua::Function = match lua.load("return cru.health.get_results").eval() {
        Ok(f) => f,
        Err(e) => return internal_error(req.id, e),
    };

    let results: mlua::Table = match get_results.call(()) {
        Ok(r) => r,
        Err(e) => return internal_error(req.id, e),
    };

    // Extract results
    let name: String = results.get("name").unwrap_or_default();
    let healthy: bool = results.get("healthy").unwrap_or(false);
    let checks_table: Option<mlua::Table> = results.get("checks").ok();

    let mut checks_vec = Vec::new();
    if let Some(table) = checks_table {
        if let Ok(len) = table.len() {
            for i in 1..=len as usize {
                if let Ok(check) = table.get::<mlua::Table>(i) {
                    let level: String = check.get("level").unwrap_or_default();
                    let msg: String = check.get("msg").unwrap_or_default();
                    let advice: Vec<String> = check
                        .get::<mlua::Table>("advice")
                        .ok()
                        .map(|t| {
                            let mut items = Vec::new();
                            if let Ok(alen) = t.len() {
                                for j in 1..=alen as usize {
                                    if let Ok(s) = t.get::<String>(j) {
                                        items.push(s);
                                    }
                                }
                            }
                            items
                        })
                        .unwrap_or_default();
                    let mut obj = serde_json::json!({ "level": level, "msg": msg });
                    if !advice.is_empty() {
                        obj["advice"] = serde_json::json!(advice);
                    }
                    checks_vec.push(obj);
                }
            }
        }
    }

    Response::success(
        req.id,
        serde_json::json!({
            "name": name,
            "healthy": healthy,
            "checks": checks_vec,
        }),
    )
}

async fn handle_lua_generate_stubs(req: Request) -> Response {
    let output_dir = require_param!(req, "output_dir", as_str).to_string();
    let verify = optional_param!(req, "verify", as_bool).unwrap_or(false);

    if verify {
        match StubGenerator::verify(Path::new(&output_dir)) {
            Ok(true) => Response::success(
                req.id,
                serde_json::json!({ "status": "ok", "path": output_dir }),
            ),
            Ok(false) => Response::success(
                req.id,
                serde_json::json!({ "status": "outdated", "path": output_dir }),
            ),
            Err(e) => internal_error(req.id, e),
        }
    } else {
        match StubGenerator::generate(Path::new(&output_dir)) {
            Ok(()) => Response::success(
                req.id,
                serde_json::json!({ "status": "ok", "path": output_dir }),
            ),
            Err(e) => internal_error(req.id, e),
        }
    }
}

async fn handle_lua_run_plugin_tests(req: Request) -> Response {
    let test_path_str = require_param!(req, "test_path", as_str).to_string();
    let filter = optional_param!(req, "filter", as_str).map(|s| s.to_string());
    let test_path = PathBuf::from(&test_path_str);

    if !test_path.exists() {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Test path does not exist: {}", test_path.display()),
        );
    }

    // Discover test files
    let test_files = match discover_plugin_test_files(&test_path) {
        Ok(files) => files,
        Err(e) => return internal_error(req.id, e),
    };

    if test_files.is_empty() {
        return Response::success(
            req.id,
            serde_json::json!({ "passed": 0, "failed": 0, "load_failures": 0, "message": "No test files found" }),
        );
    }

    let executor = match LuaExecutor::new() {
        Ok(e) => e,
        Err(e) => return internal_error(req.id, e),
    };

    // Set package.path to include the plugin root
    let plugin_root = test_path
        .canonicalize()
        .unwrap_or_else(|_| test_path.clone());
    let plugin_root_str = plugin_root.to_string_lossy();
    if let Err(e) = executor
        .lua()
        .load(format!(
            r#"
local plugin_root = {plugin_root_str:?}
local entries = {{
    plugin_root .. "/?.lua",
    plugin_root .. "/?/init.lua",
}}
for _, entry in ipairs(entries) do
    if not package.path:find(entry, 1, true) then
        package.path = entry .. ";" .. package.path
    end
end
"#
        ))
        .set_name("plugin_package_path")
        .exec()
    {
        return internal_error(req.id, e);
    }

    // Setup test mocks
    if let Err(e) = executor
        .lua()
        .load("test_mocks.setup()")
        .set_name("test_mocks_setup")
        .exec()
    {
        return internal_error(req.id, e);
    }

    // Apply test filter if provided
    if let Some(ref filter_str) = filter {
        if let Err(e) = executor
            .lua()
            .globals()
            .set("__cru_plugin_test_filter", filter_str.clone())
        {
            return internal_error(req.id, e);
        }
        if let Err(e) = executor
            .lua()
            .load(
                r#"
                local _orig_it = it
                local _orig_pending = pending
                local filter = _G.__cru_plugin_test_filter

                it = function(name, fn)
                    if string.find(name, filter, 1, true) then
                        return _orig_it(name, fn)
                    end
                end

                pending = function(name, fn)
                    if string.find(name, filter, 1, true) then
                        return _orig_pending(name, fn)
                    end
                end
                "#,
            )
            .set_name("test_filter")
            .exec()
        {
            return internal_error(req.id, e);
        }
    }

    // Load test files
    let mut load_failures: usize = 0;
    for file in &test_files {
        let file_contents = match std::fs::read_to_string(file) {
            Ok(contents) => contents,
            Err(_) => {
                load_failures += 1;
                continue;
            }
        };

        let chunk_name = file.to_string_lossy();
        if executor
            .lua()
            .load(&file_contents)
            .set_name(chunk_name.as_ref())
            .exec()
            .is_err()
        {
            load_failures += 1;
        }
    }

    // Run tests
    let results: mlua::Table = match executor
        .lua()
        .load("return run_tests()")
        .set_name("plugin_test_runner")
        .eval()
    {
        Ok(r) => r,
        Err(e) => return internal_error(req.id, e),
    };

    let passed: usize = results.get("passed").unwrap_or(0);
    let failed: usize = results.get("failed").unwrap_or(0);

    Response::success(
        req.id,
        serde_json::json!({
            "passed": passed,
            "failed": failed,
            "load_failures": load_failures,
        }),
    )
}

async fn handle_lua_register_commands(
    req: Request,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let commands = require_param!(req, "commands", as_array);

    let Some(state) = lua_sessions.get(session_id) else {
        return session_not_found(req.id, session_id);
    };

    let state = state.value().clone();
    let state = state.lock().await;
    let mut registered: usize = 0;

    for cmd in commands {
        if let Some(source) = cmd.get("source").and_then(|v| v.as_str()) {
            if state.executor.lua().load(source).exec().is_ok() {
                registered += 1;
            }
        }
    }

    Response::success(
        req.id,
        serde_json::json!({
            "registered": registered,
        }),
    )
}

/// Discover test files in a plugin directory (files ending with _test.lua or _test.fnl)
fn discover_plugin_test_files(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    let mut files = Vec::new();

    // Check tests/ subdirectory
    let tests_dir = path.join("tests");
    if tests_dir.is_dir() {
        collect_plugin_test_files(&tests_dir, &mut files)?;
    }

    // Check root directory
    collect_plugin_test_files(path, &mut files)?;

    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_plugin_test_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file() {
            let stem = path.file_stem().and_then(|name| name.to_str());
            let ext = path.extension().and_then(|e| e.to_str());
            if matches!((stem, ext), (Some(s), Some("lua" | "fnl")) if s.ends_with("_test")) {
                out.push(path);
            }
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Storage maintenance RPC stubs
// ─────────────────────────────────────────────────────────────────────────────

async fn handle_storage_verify(req: Request) -> Response {
    Response::success(
        req.id,
        serde_json::json!({
            "status": "not_implemented",
            "message": "Storage verification is not yet implemented. Use `cru process --force` to rebuild storage."
        }),
    )
}

async fn handle_storage_cleanup(req: Request) -> Response {
    Response::success(
        req.id,
        serde_json::json!({
            "status": "not_implemented",
            "message": "Storage cleanup is not yet implemented."
        }),
    )
}

async fn handle_storage_backup(req: Request) -> Response {
    Response::success(
        req.id,
        serde_json::json!({
            "status": "not_implemented",
            "message": "Storage backup is not yet implemented. Copy the .crucible directory directly for backup."
        }),
    )
}

async fn handle_storage_restore(req: Request) -> Response {
    Response::success(
        req.id,
        serde_json::json!({
            "status": "not_implemented",
            "message": "Storage restore is not yet implemented."
        }),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Session RPC handlers
// ─────────────────────────────────────────────────────────────────────────────

use crucible_core::session::{SessionState, SessionType};

async fn handle_session_create(
    req: Request,
    sm: &Arc<SessionManager>,
    pm: &Arc<ProjectManager>,
    llm_config: &Option<LlmConfig>,
) -> Response {
    let session_type_str = optional_param!(req, "type", as_str).unwrap_or("chat");
    let session_type = match session_type_str {
        "chat" => SessionType::Chat,
        "agent" => SessionType::Agent,
        "workflow" => SessionType::Workflow,
        _ => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!("Invalid session type: {}", session_type_str),
            );
        }
    };

    let kiln = optional_param!(req, "kiln", as_str)
        .map(PathBuf::from)
        .unwrap_or_else(crucible_config::crucible_home);

    let workspace = optional_param!(req, "workspace", as_str).map(PathBuf::from);

    let provider_trust_level = resolve_provider_trust_level_for_create(&req, llm_config);
    let classification = resolve_kiln_classification_for_create(&kiln, workspace.as_ref());
    if let Some(classification) = classification {
        if let Err(message) = validate_trust_level(provider_trust_level, classification) {
            return Response::error(req.id, INVALID_PARAMS, message);
        }
    }

    let connected_kilns: Vec<PathBuf> = req
        .params
        .get("connect_kilns")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(PathBuf::from))
                .collect()
        })
        .unwrap_or_default();

    let recording_mode = optional_param!(req, "recording_mode", as_str)
        .and_then(|s| s.parse::<RecordingMode>().ok());
    let custom_recording_path = optional_param!(req, "recording_path", as_str).map(PathBuf::from);

    let project_path = workspace.as_ref().unwrap_or(&kiln);
    if let Err(e) = pm.register_if_missing(project_path) {
        tracing::warn!(path = %project_path.display(), error = %e, "Failed to auto-register project");
    }

    match sm
        .create_session(
            session_type,
            kiln,
            workspace,
            connected_kilns,
            recording_mode,
        )
        .await
    {
        Ok(session) => {
            if session.recording_mode == Some(RecordingMode::Granular) {
                let recording_path = match custom_recording_path {
                    Some(ref p) => p.clone(),
                    None => {
                        let session_dir = FileSessionStorage::session_dir_for(&session);
                        session_dir.join("recording.jsonl")
                    }
                };
                let (writer, tx) = RecordingWriter::new(
                    recording_path,
                    session.id.clone(),
                    RecordingMode::Granular,
                    None,
                );
                sm.set_recording_sender(&session.id, tx);
                let _handle = writer.start();
            }

            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session.id,
                    "type": session.session_type.as_prefix(),
                    "kiln": session.kiln,
                    "workspace": session.workspace,
                    "state": format!("{}", session.state),
                }),
            )
        }
        Err(e) => internal_error(req.id, e),
    }
}

fn validate_trust_level(
    provider_trust_level: TrustLevel,
    classification: DataClassification,
) -> Result<(), String> {
    if provider_trust_level.satisfies(classification) {
        return Ok(());
    }

    Err(format!(
        "Provider trust level '{}' is insufficient for kiln data classification '{}'. Requires '{}' trust or higher.",
        provider_trust_level,
        classification,
        classification.required_trust_level()
    ))
}

fn resolve_provider_trust_level_for_create(
    req: &Request,
    llm_config: &Option<LlmConfig>,
) -> TrustLevel {
    if optional_param!(req, "agent_type", as_str) == Some("acp") {
        return TrustLevel::Cloud;
    }

    if let Some(provider_key) = optional_param!(req, "provider_key", as_str) {
        if let Some(config) = llm_config
            .as_ref()
            .and_then(|cfg| cfg.get_provider(provider_key))
        {
            return config.effective_trust_level();
        }
    }

    if let Some(provider_name) = optional_param!(req, "provider", as_str) {
        if let Ok(backend) = provider_name.parse::<crucible_config::BackendType>() {
            return backend.default_trust_level();
        }
    }

    llm_config
        .as_ref()
        .and_then(LlmConfig::default_provider)
        .map(|(_, provider)| provider.effective_trust_level())
        .unwrap_or(TrustLevel::Cloud)
}

fn resolve_kiln_classification_for_create(
    kiln: &Path,
    workspace: Option<&PathBuf>,
) -> Option<DataClassification> {
    let workspace_path = workspace.cloned().unwrap_or_else(|| kiln.to_path_buf());
    crate::trust_resolution::resolve_kiln_classification(&workspace_path, kiln)
}

async fn handle_session_list(req: Request, sm: &Arc<SessionManager>) -> Response {
    // Parse optional filters
    let kiln = optional_param!(req, "kiln", as_str).map(PathBuf::from);
    let workspace = optional_param!(req, "workspace", as_str).map(PathBuf::from);
    let session_type = optional_param!(req, "type", as_str).and_then(|s| match s {
        "chat" => Some(SessionType::Chat),
        "agent" => Some(SessionType::Agent),
        "workflow" => Some(SessionType::Workflow),
        _ => None,
    });
    let state = optional_param!(req, "state", as_str).and_then(|s| match s {
        "active" => Some(SessionState::Active),
        "paused" => Some(SessionState::Paused),
        "compacting" => Some(SessionState::Compacting),
        "ended" => Some(SessionState::Ended),
        _ => None,
    });

    let sessions = sm
        .list_sessions_filtered_async(kiln.as_ref(), workspace.as_ref(), session_type, state)
        .await;

    let sessions_json: Vec<_> = sessions
        .iter()
        .map(|s| {
            serde_json::json!({
                "session_id": s.id,
                "type": s.session_type.as_prefix(),
                "kiln": s.kiln,
                "workspace": s.workspace,
                "state": format!("{}", s.state),
                "started_at": s.started_at.to_rfc3339(),
                "title": s.title,
            })
        })
        .collect();

    Response::success(
        req.id,
        serde_json::json!({
            "sessions": sessions_json,
            "total": sessions_json.len(),
        }),
    )
}

async fn handle_session_search(req: Request, sm: &Arc<SessionManager>) -> Response {
    let query = require_param!(req, "query", as_str);
    let kiln = optional_param!(req, "kiln", as_str).map(PathBuf::from);
    let limit = optional_param!(req, "limit", as_u64).unwrap_or(20) as usize;

    // Determine sessions directory
    let sessions_path = if let Some(kiln_path) = kiln {
        kiln_path.join(".crucible").join("sessions")
    } else {
        return Response::success(
            req.id,
            serde_json::json!({
                "matches": [],
                "total": 0,
                "note": "Specify 'kiln' parameter to search sessions"
            }),
        );
    };

    if !sessions_path.exists() {
        return Response::success(
            req.id,
            serde_json::json!({
                "matches": [],
                "total": 0
            }),
        );
    }

    let query_lower = query.to_lowercase();
    let mut matches = Vec::new();

    let read_dir = match tokio::fs::read_dir(&sessions_path).await {
        Ok(rd) => rd,
        Err(e) => {
            return internal_error(
                req.id,
                anyhow::anyhow!("Failed to read sessions dir: {}", e),
            )
        }
    };

    let mut rd = read_dir;
    while let Ok(Some(entry)) = rd.next_entry().await {
        if matches.len() >= limit {
            break;
        }
        let session_dir = entry.path();
        if !session_dir.is_dir() {
            continue;
        }
        let session_id = session_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let jsonl_path = session_dir.join("session.jsonl");
        if !jsonl_path.exists() {
            continue;
        }
        let content = match tokio::fs::read_to_string(&jsonl_path).await {
            Ok(c) => c,
            Err(_) => continue,
        };
        for (line_num, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&query_lower) {
                let truncated = if line.len() > 100 {
                    // Use floor_char_boundary to avoid panicking on multi-byte UTF-8
                    let end = line.floor_char_boundary(100);
                    format!("{}...", &line[..end])
                } else {
                    line.to_string()
                };
                matches.push(serde_json::json!({
                    "session_id": session_id,
                    "line": line_num + 1,
                    "context": truncated
                }));
                break;
            }
        }
    }

    // Also include active sessions matching by title
    let active_sessions = sm
        .list_sessions_filtered_async(None, None, None, None)
        .await;
    for session in &active_sessions {
        if matches.len() >= limit {
            break;
        }
        if let Some(title) = &session.title {
            if title.to_lowercase().contains(&query_lower)
                && !matches
                    .iter()
                    .any(|m| m["session_id"] == session.id.as_str())
            {
                matches.push(serde_json::json!({
                    "session_id": session.id,
                    "line": 0,
                    "context": format!("[active] {}", title)
                }));
            }
        }
    }

    let total = matches.len();
    Response::success(
        req.id,
        serde_json::json!({
            "matches": matches,
            "total": total
        }),
    )
}

async fn handle_session_get(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match sm.get_session(session_id) {
        Some(session) => {
            let mut response = serde_json::json!({
                "session_id": session.id,
                "type": session.session_type.as_prefix(),
                "kiln": session.kiln,
                "workspace": session.workspace,
                "connected_kilns": session.connected_kilns,
                "state": format!("{}", session.state),
                "started_at": session.started_at.to_rfc3339(),
                "title": session.title,
                "continued_from": session.continued_from,
                "agent": session.agent,
            });

            if let Some(mode) = session.recording_mode {
                response["recording_mode"] = serde_json::json!(format!("{}", mode));
            }

            Response::success(req.id, response)
        }
        None => Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Session not found: {}", session_id),
        ),
    }
}

async fn handle_session_pause(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match sm.pause_session(session_id).await {
        Ok(previous_state) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "previous_state": format!("{}", previous_state),
                "state": "paused",
            }),
        ),
        Err(e) => invalid_state_error(req.id, "pause", e),
    }
}

async fn handle_session_resume(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match sm.resume_session(session_id).await {
        Ok(previous_state) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "previous_state": format!("{}", previous_state),
                "state": "active",
            }),
        ),
        Err(e) => invalid_state_error(req.id, "resume", e),
    }
}

async fn handle_session_resume_from_storage(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let kiln = PathBuf::from(require_param!(req, "kiln", as_str));

    // Optional pagination params
    let limit = optional_param!(req, "limit", as_u64).map(|n| n as usize);
    let offset = optional_param!(req, "offset", as_u64).map(|n| n as usize);

    // Resume session from storage
    let session = match sm.resume_session_from_storage(session_id, &kiln).await {
        Ok(s) => s,
        Err(e) => return invalid_state_error(req.id, "resume_from_storage", e),
    };

    // Load event history with pagination
    let history = match sm
        .load_session_events(session_id, &kiln, limit, offset)
        .await
    {
        Ok(events) => events,
        Err(e) => {
            // Session resumed but history load failed - return session without history
            // Log internally but don't expose error details to client
            warn!("Failed to load session history: {}", e);
            return Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session.id,
                    "type": session.session_type.as_prefix(),
                    "state": format!("{}", session.state),
                    "kiln": session.kiln,
                    "history": [],
                    "total_events": 0,
                }),
            );
        }
    };

    // Get total event count for pagination
    let total = sm
        .count_session_events(session_id, &kiln)
        .await
        .unwrap_or(0);

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session.id,
            "type": session.session_type.as_prefix(),
            "state": format!("{}", session.state),
            "kiln": session.kiln,
            "history": history,
            "total_events": total,
        }),
    )
}

async fn handle_session_end(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match sm.end_session(session_id).await {
        Ok(session) => {
            am.cleanup_session(session_id);
            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session.id,
                    "state": "ended",
                    "kiln": session.kiln,
                }),
            )
        }
        Err(e) => invalid_state_error(req.id, "end", e),
    }
}

async fn handle_session_replay(
    req: Request,
    sm: &Arc<SessionManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let recording_path = require_param!(req, "recording_path", as_str);
    let speed = req
        .params
        .get("speed")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);

    let recording_path = PathBuf::from(recording_path);
    let replay_session_id = format!("replay-{}", uuid::Uuid::new_v4());

    match ReplaySession::new(
        recording_path,
        speed,
        event_tx.clone(),
        replay_session_id.clone(),
    ) {
        Ok(replay) => {
            sm.register_transient(replay.session().clone());
            let _handle = replay.start();

            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": replay_session_id,
                    "status": "replaying",
                    "speed": speed,
                }),
            )
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_compact(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match sm.request_compaction(session_id).await {
        Ok(session) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session.id,
                "state": format!("{}", session.state),
                "compaction_requested": true,
            }),
        ),
        Err(e) => invalid_state_error(req.id, "compact", e),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
async fn handle_session_configure_agent(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    let agent_json = match req.params.get("agent") {
        Some(v) => v.clone(),
        None => {
            return Response::error(req.id, INVALID_PARAMS, "Missing 'agent' parameter");
        }
    };

    let agent: crucible_core::session::SessionAgent = match serde_json::from_value(agent_json) {
        Ok(a) => a,
        Err(e) => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!("Invalid agent config: {}", e),
            );
        }
    };

    match am.configure_agent(session_id, agent).await {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "configured": true,
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_send_message(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let content = require_param!(req, "content", as_str);

    match am
        .send_message(session_id, content.to_string(), event_tx)
        .await
    {
        Ok(message_id) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "message_id": message_id,
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_cancel(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    let cancelled = am.cancel(session_id).await;
    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "cancelled": cancelled,
        }),
    )
}

async fn handle_session_interaction_respond(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let request_id = require_param!(req, "request_id", as_str);
    let response_obj = require_param!(req, "response", as_object);

    let response: crucible_core::interaction::InteractionResponse =
        match serde_json::from_value(serde_json::Value::Object(response_obj.clone())) {
            Ok(r) => r,
            Err(e) => {
                return Response::error(
                    req.id,
                    INVALID_PARAMS,
                    format!("Invalid interaction response: {}", e),
                )
            }
        };

    if let crucible_core::interaction::InteractionResponse::Permission(perm_response) = &response {
        if let Err(e) = am.respond_to_permission(session_id, request_id, perm_response.clone()) {
            tracing::warn!(
                session_id = %session_id,
                request_id = %request_id,
                error = %e,
                "Failed to send permission response to channel (may have timed out)"
            );
        }
    }

    let _ = emit_event(
        event_tx,
        SessionEventMessage::new(
            session_id,
            "interaction_completed",
            serde_json::json!({
                "request_id": request_id,
                "response": response,
            }),
        ),
    );

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "request_id": request_id,
        }),
    )
}

async fn handle_session_test_interaction(
    req: Request,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    let get_str = |key: &str| -> Option<&str> { req.params.get(key)?.as_str() };

    let interaction_type = get_str("type").unwrap_or("ask");
    let request_id = format!("test-{}", uuid::Uuid::new_v4());

    let request = match interaction_type {
        "ask" => {
            let question =
                get_str("question").unwrap_or("Test question: Which option do you prefer?");

            // InteractionRequest uses #[serde(tag = "kind")] internally-tagged format
            serde_json::json!({
                "kind": "ask",
                "question": question,
                "choices": ["Option A", "Option B", "Option C"],
                "allow_other": true,
                "multi_select": false
            })
        }
        "permission" => {
            let action = get_str("action").unwrap_or("rm -rf /tmp/test");

            // PermRequest uses externally-tagged format for its inner Bash/Read/Write/Tool
            serde_json::json!({
                "kind": "permission",
                "Bash": {
                    "command": action
                }
            })
        }
        _ => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!(
                    "Unknown interaction type: {}. Use 'ask' or 'permission'",
                    interaction_type
                ),
            )
        }
    };

    let _ = emit_event(
        event_tx,
        SessionEventMessage::new(
            session_id.to_string(),
            "interaction_requested",
            serde_json::json!({
                "request_id": request_id,
                "request": request,
            }),
        ),
    );

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "request_id": request_id,
            "type": interaction_type,
        }),
    )
}

async fn handle_session_switch_model(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let model_id = require_param!(req, "model_id", as_str);

    match am.switch_model(session_id, model_id, Some(event_tx)).await {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "model_id": model_id,
                "switched": true,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            Response::error(req.id, INVALID_PARAMS, format!("Session not found: {}", id))
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => Response::error(
            req.id,
            INVALID_PARAMS,
            format!("No agent configured for session: {}", id),
        ),
        Err(crate::agent_manager::AgentError::ConcurrentRequest(id)) => Response::error(
            req.id,
            INVALID_PARAMS,
            format!(
                "Cannot switch model while request is in progress for session: {}",
                id
            ),
        ),
        Err(crate::agent_manager::AgentError::InvalidModelId(msg)) => {
            Response::error(req.id, INVALID_PARAMS, msg)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_list_models(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    let classification = match am.get_session_with_agent(session_id) {
        Ok((session, _)) => {
            crate::trust_resolution::resolve_kiln_classification(&session.workspace, &session.kiln)
        }
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            return session_not_found(req.id, &id);
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(_)) => {
            return Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session_id,
                    "models": Vec::<String>::new(),
                }),
            );
        }
        Err(e) => return internal_error(req.id, e),
    };

    match am.list_models(session_id, classification).await {
        Ok(models) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "models": models,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(_)) => {
            // Return empty models list if no agent is configured
            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session_id,
                    "models": Vec::<String>::new(),
                }),
            )
        }
        Err(e) => internal_error(req.id, e),
    }
}

/// List all available models without requiring an active session.
///
/// Accepts an optional `kiln_path` parameter. When provided, the handler
async fn handle_models_list(req: Request, am: &Arc<AgentManager>) -> Response {
    let kiln_path = req
        .params
        .get("kiln_path")
        .and_then(|v| v.as_str())
        .map(PathBuf::from);

    let classification = kiln_path
        .as_ref()
        .and_then(|kiln| crate::trust_resolution::find_workspace_and_resolve_classification(kiln));

    match am.list_models("", classification).await {
        Ok(models) => Response::success(req.id, serde_json::json!({ "models": models })),
        Err(crate::agent_manager::AgentError::SessionNotFound(_)) => {
            // No session fallback path hit — return empty list
            Response::success(req.id, serde_json::json!({ "models": [] }))
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_set_thinking_budget(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let budget = optional_param!(req, "thinking_budget", as_i64);

    // When budget is None, clear the thinking budget override
    let effective_budget = budget.unwrap_or(0);

    match am
        .set_thinking_budget(session_id, effective_budget, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "thinking_budget": budget,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

async fn handle_session_get_thinking_budget(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_thinking_budget(session_id) {
        Ok(budget) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "thinking_budget": budget,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_set_precognition(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let enabled = optional_param!(req, "enabled", as_bool).unwrap_or(true);

    match am
        .set_precognition(session_id, enabled, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "precognition_enabled": enabled,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

async fn handle_session_get_precognition(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_precognition(session_id) {
        Ok(enabled) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "precognition_enabled": enabled,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_add_notification(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let notification_obj = require_param!(req, "notification", as_object);

    let notification = match serde_json::from_value::<crucible_core::types::Notification>(
        serde_json::Value::Object(notification_obj.clone()),
    ) {
        Ok(n) => n,
        Err(e) => return Response::error(req.id, -32602, format!("Invalid notification: {}", e)),
    };

    match am
        .add_notification(session_id, notification, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "success": true,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_list_notifications(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.list_notifications(session_id).await {
        Ok(notifications) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "notifications": notifications,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_dismiss_notification(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let notification_id = require_param!(req, "notification_id", as_str);

    match am
        .dismiss_notification(session_id, notification_id, Some(event_tx))
        .await
    {
        Ok(success) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "notification_id": notification_id,
                "success": success,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_set_temperature(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let temperature = require_param!(req, "temperature", as_f64);

    match am
        .set_temperature(session_id, temperature, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "temperature": temperature,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

async fn handle_session_get_temperature(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_temperature(session_id) {
        Ok(temperature) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "temperature": temperature,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_set_max_tokens(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    // max_tokens can be null to clear the limit, so we use optional
    let max_tokens = optional_param!(req, "max_tokens", as_u64).map(|v| v as u32);

    match am
        .set_max_tokens(session_id, max_tokens, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "max_tokens": max_tokens,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

async fn handle_session_get_max_tokens(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_max_tokens(session_id) {
        Ok(max_tokens) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "max_tokens": max_tokens,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_plugin_reload(
    req: Request,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    let name = require_param!(req, "name", as_str);

    let mut loader_guard = plugin_loader.lock().await;
    let loader = match loader_guard.as_mut() {
        Some(l) => l,
        None => return internal_error(req.id, "Plugin loader not initialized"),
    };

    match loader.reload_plugin(name).await {
        Ok(spec) => {
            let service_fns = loader.take_service_fns();
            for (svc_name, func) in service_fns {
                info!("Re-spawning service after reload: {}", svc_name);
                tokio::spawn(async move {
                    match func.call_async::<()>(()).await {
                        Ok(()) => info!("Service '{}' completed", svc_name),
                        Err(e) => warn!("Service '{}' failed: {}", svc_name, e),
                    }
                });
            }

            Response::success(
                req.id,
                serde_json::json!({
                    "name": name,
                    "reloaded": true,
                    "tools": spec.tools.len(),
                    "commands": spec.commands.len(),
                    "handlers": spec.handlers.len(),
                    "services": spec.services.len(),
                }),
            )
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_plugin_list(
    req: Request,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    let loader_guard = plugin_loader.lock().await;
    let names = match loader_guard.as_ref() {
        Some(l) => l.loaded_plugin_names(),
        None => Vec::new(),
    };

    Response::success(
        req.id,
        serde_json::json!({
            "plugins": names,
        }),
    )
}

// --- Project handlers ---

async fn handle_project_register(req: Request, pm: &Arc<ProjectManager>) -> Response {
    let path = require_param!(req, "path", as_str);

    match pm.register(Path::new(path)) {
        Ok(project) => match serde_json::to_value(project) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
        },
        Err(e) => Response::error(req.id, INVALID_PARAMS, e.to_string()),
    }
}

async fn handle_project_unregister(req: Request, pm: &Arc<ProjectManager>) -> Response {
    let path = require_param!(req, "path", as_str);

    match pm.unregister(Path::new(path)) {
        Ok(()) => Response::success(req.id, serde_json::json!({"status": "ok"})),
        Err(e) => Response::error(req.id, INVALID_PARAMS, e.to_string()),
    }
}

async fn handle_project_list(req: Request, pm: &Arc<ProjectManager>) -> Response {
    let projects = pm.list();
    match serde_json::to_value(projects) {
        Ok(v) => Response::success(req.id, v),
        Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
    }
}

async fn handle_project_get(req: Request, pm: &Arc<ProjectManager>) -> Response {
    let path = require_param!(req, "path", as_str);

    match pm.get(Path::new(path)) {
        Some(project) => match serde_json::to_value(project) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
        },
        None => Response::success(req.id, serde_json::Value::Null),
    }
}

fn spawn_plugin_watcher(
    plugin_dirs: Vec<(String, PathBuf)>,
    plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
) {
    use notify::{RecursiveMode, Watcher};

    let dir_to_plugin: std::collections::HashMap<PathBuf, String> = plugin_dirs
        .iter()
        .map(|(name, dir)| (dir.clone(), name.clone()))
        .collect();

    let watch_dirs: Vec<PathBuf> = plugin_dirs.into_iter().map(|(_, dir)| dir).collect();

    let (sync_tx, sync_rx) = std::sync::mpsc::channel::<PathBuf>();

    let mut watcher = match notify::recommended_watcher(
        move |res: std::result::Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                if !event.kind.is_modify() && !event.kind.is_create() {
                    return;
                }
                for path in &event.paths {
                    let ext = path.extension().and_then(|e| e.to_str());
                    if matches!(ext, Some("lua") | Some("fnl")) {
                        let _ = sync_tx.send(path.clone());
                    }
                }
            }
        },
    ) {
        Ok(w) => w,
        Err(e) => {
            warn!("Failed to create plugin file watcher: {}", e);
            return;
        }
    };

    for dir in &watch_dirs {
        if let Err(e) = watcher.watch(dir, RecursiveMode::Recursive) {
            warn!("Failed to watch plugin dir {}: {}", dir.display(), e);
        }
    }

    info!(
        "Plugin file watcher active for {} director(ies)",
        watch_dirs.len()
    );

    tokio::spawn(async move {
        let _watcher_guard = watcher;
        let debounce = tokio::time::Duration::from_millis(500);
        let mut pending: std::collections::HashMap<String, tokio::time::Instant> =
            std::collections::HashMap::new();

        loop {
            let next_fire = pending.values().copied().min();

            let timeout = match next_fire {
                Some(t) => t.saturating_duration_since(tokio::time::Instant::now()),
                None => tokio::time::Duration::from_millis(100),
            };

            tokio::time::sleep(timeout).await;

            while let Ok(changed_path) = sync_rx.try_recv() {
                if let Some(plugin_name) = find_owning_plugin(&changed_path, &dir_to_plugin) {
                    pending.insert(plugin_name, tokio::time::Instant::now() + debounce);
                }
            }

            let now = tokio::time::Instant::now();
            let ready: Vec<String> = pending
                .iter()
                .filter(|(_, &t)| t <= now)
                .map(|(name, _)| name.clone())
                .collect();

            for name in ready {
                pending.remove(&name);
                let mut guard = plugin_loader.lock().await;
                if let Some(ref mut loader) = *guard {
                    match loader.reload_plugin(&name).await {
                        Ok(_spec) => {
                            info!("Plugin '{}' auto-reloaded due to file change", name);
                            let service_fns = loader.take_service_fns();
                            drop(guard);
                            for (svc_name, func) in service_fns {
                                info!("Re-spawning service after auto-reload: {}", svc_name);
                                tokio::spawn(async move {
                                    match func.call_async::<()>(()).await {
                                        Ok(()) => info!("Service '{}' completed", svc_name),
                                        Err(e) => warn!("Service '{}' failed: {}", svc_name, e),
                                    }
                                });
                            }
                        }
                        Err(e) => {
                            warn!("Auto-reload failed for plugin '{}': {}", name, e);
                        }
                    }
                }
            }
        }
    });
}

fn find_owning_plugin(
    path: &Path,
    dir_to_plugin: &std::collections::HashMap<PathBuf, String>,
) -> Option<String> {
    for (dir, name) in dir_to_plugin {
        if path.starts_with(dir) {
            return Some(name.clone());
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Session observe RPC handlers (load_events, list_persisted, render_markdown,
//                                export_to_file, cleanup, reindex)
// ─────────────────────────────────────────────────────────────────────────────

/// Load events from a persisted session's JSONL log.
///
/// Params:
///   - `session_dir` (string, required): Path to the session directory
///     Returns: Array of LogEvent objects
async fn handle_session_load_events(req: Request) -> Response {
    let session_dir = require_param!(req, "session_dir", as_str);

    match crucible_observe::load_events(session_dir).await {
        Ok(events) => match serde_json::to_value(&events) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => internal_error(req.id, e),
        },
        Err(e) => internal_error(req.id, e),
    }
}

/// List persisted sessions from a kiln's session directory.
///
/// Params:
///   - `kiln` (string, required): Path to the kiln
///   - `session_type` (string, optional): Filter by type ("chat", "agent", etc.)
///   - `limit` (u64, optional): Max sessions to return (default 50, newest first)
///     Returns: { sessions: [...], total: N }
async fn handle_session_list_persisted(req: Request) -> Response {
    let kiln = require_param!(req, "kiln", as_str);
    let session_type_filter = optional_param!(req, "session_type", as_str);
    let limit = optional_param!(req, "limit", as_u64).unwrap_or(50) as usize;

    let sessions_path = FileSessionStorage::sessions_base(Path::new(kiln));

    if !sessions_path.exists() {
        return Response::success(req.id, serde_json::json!({ "sessions": [], "total": 0 }));
    }

    let mut ids = match crucible_observe::list_sessions(&sessions_path).await {
        Ok(ids) => ids,
        Err(e) => return internal_error(req.id, e),
    };

    // Filter by session type if specified
    if let Some(type_filter) = session_type_filter {
        if let Ok(filter_type) = type_filter.parse::<crucible_observe::SessionType>() {
            ids.retain(|id| id.session_type() == filter_type);
        }
    }

    // Newest first, then limit
    ids.reverse();
    ids.truncate(limit);

    let mut session_entries = Vec::new();
    for id in &ids {
        let session_dir = sessions_path.join(id.as_str());
        let events = crucible_observe::load_events(&session_dir)
            .await
            .unwrap_or_default();
        let msg_count = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    crucible_observe::LogEvent::User { .. }
                        | crucible_observe::LogEvent::Assistant { .. }
                )
            })
            .count();

        let title = events
            .iter()
            .find_map(|e| match e {
                crucible_observe::LogEvent::User { content, .. } => {
                    let preview: String = content.chars().take(50).collect();
                    if content.len() > 50 {
                        Some(format!("{}...", preview))
                    } else {
                        Some(preview)
                    }
                }
                _ => None,
            })
            .unwrap_or_else(|| "(empty)".to_string());

        session_entries.push(serde_json::json!({
            "id": id.as_str(),
            "session_type": format!("{}", id.session_type()),
            "message_count": msg_count,
            "title": title,
        }));
    }

    let total = session_entries.len();
    Response::success(
        req.id,
        serde_json::json!({
            "sessions": session_entries,
            "total": total,
        }),
    )
}

/// Render a persisted session's events to markdown.
///
/// Params:
///   - `session_dir` (string, required): Path to the session directory
///   - `include_timestamps` (bool, optional): Include timestamps (default false)
///   - `include_tokens` (bool, optional): Include token stats (default true)
///   - `include_tools` (bool, optional): Include tool details (default true)
///   - `max_content_length` (u64, optional): Truncation limit (default 0 = no limit)
///     Returns: { markdown: "..." }
async fn handle_session_render_markdown(req: Request) -> Response {
    let session_dir = require_param!(req, "session_dir", as_str);
    let include_timestamps = optional_param!(req, "include_timestamps", as_bool).unwrap_or(false);
    let include_tokens = optional_param!(req, "include_tokens", as_bool).unwrap_or(true);
    let include_tools = optional_param!(req, "include_tools", as_bool).unwrap_or(true);
    let max_content_length =
        optional_param!(req, "max_content_length", as_u64).unwrap_or(0) as usize;

    let events = match crucible_observe::load_events(session_dir).await {
        Ok(e) => e,
        Err(e) => return internal_error(req.id, e),
    };

    let options = crucible_observe::RenderOptions {
        include_timestamps,
        include_tokens,
        include_tools,
        max_content_length,
    };

    let md = crucible_observe::render_to_markdown(&events, &options);

    Response::success(req.id, serde_json::json!({ "markdown": md }))
}

/// Export a session to a markdown file.
///
/// Params:
///   - `session_dir` (string, required): Path to the session directory
///   - `output_path` (string, optional): Output file path (default: session_dir/session.md)
///   - `include_timestamps` (bool, optional): Include timestamps (default false)
///     Returns: { status: "ok", output_path: "..." }
async fn handle_session_export_to_file(req: Request) -> Response {
    let session_dir_str = require_param!(req, "session_dir", as_str);
    let output_path = optional_param!(req, "output_path", as_str);
    let timestamps = optional_param!(req, "include_timestamps", as_bool).unwrap_or(false);

    let session_dir = Path::new(session_dir_str);

    let events = match crucible_observe::load_events(session_dir).await {
        Ok(e) => e,
        Err(e) => return internal_error(req.id, e),
    };

    let options = crucible_observe::RenderOptions {
        include_timestamps: timestamps,
        ..Default::default()
    };

    let md = crucible_observe::render_to_markdown(&events, &options);

    let out_path = match output_path {
        Some(p) => PathBuf::from(p),
        None => session_dir.join("session.md"),
    };

    if let Err(e) = tokio::fs::write(&out_path, &md).await {
        return internal_error(req.id, e);
    }

    Response::success(
        req.id,
        serde_json::json!({
            "status": "ok",
            "output_path": out_path.to_string_lossy(),
        }),
    )
}

/// Clean up old persisted sessions.
///
/// Params:
///   - `kiln` (string, required): Path to the kiln
///   - `older_than_days` (u64, required): Delete sessions older than N days
///   - `dry_run` (bool, optional): If true, just report what would be deleted (default false)
///     Returns: { deleted: [...], total: N, dry_run: bool }
async fn handle_session_cleanup(req: Request) -> Response {
    let kiln = require_param!(req, "kiln", as_str);
    let older_than_days = require_param!(req, "older_than_days", as_u64);
    let dry_run = optional_param!(req, "dry_run", as_bool).unwrap_or(false);

    let sessions_path = FileSessionStorage::sessions_base(Path::new(kiln));

    if !sessions_path.exists() {
        return Response::success(
            req.id,
            serde_json::json!({ "deleted": [], "total": 0, "dry_run": dry_run }),
        );
    }

    let ids = match crucible_observe::list_sessions(&sessions_path).await {
        Ok(ids) => ids,
        Err(e) => return internal_error(req.id, e),
    };

    let cutoff = chrono::Utc::now() - chrono::Duration::days(older_than_days as i64);

    let mut to_delete = Vec::new();

    for id in ids {
        let session_dir = sessions_path.join(id.as_str());
        let events = crucible_observe::load_events(&session_dir)
            .await
            .unwrap_or_default();

        let latest = events.iter().map(|e| e.timestamp()).max();
        if let Some(ts) = latest {
            if ts < cutoff {
                to_delete.push((id, session_dir));
            }
        }
    }

    let mut deleted_ids = Vec::new();
    if !dry_run {
        for (id, dir) in &to_delete {
            if let Err(e) = tokio::fs::remove_dir_all(dir).await {
                warn!(
                    session_id = %id,
                    error = %e,
                    "Failed to delete session directory"
                );
            } else {
                deleted_ids.push(id.as_str().to_string());
            }
        }
    } else {
        deleted_ids = to_delete
            .iter()
            .map(|(id, _)| id.as_str().to_string())
            .collect();
    }

    let total = deleted_ids.len();
    Response::success(
        req.id,
        serde_json::json!({
            "deleted": deleted_ids,
            "total": total,
            "dry_run": dry_run,
        }),
    )
}

/// Reindex persisted sessions into the kiln's NoteStore.
///
/// Params:
///   - `kiln` (string, required): Path to the kiln
///   - `force` (bool, optional): Re-index even if already present (default false)
///     Returns: { indexed: N, skipped: N, errors: N }
async fn handle_session_reindex(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_str = require_param!(req, "kiln", as_str);
    let force = optional_param!(req, "force", as_bool).unwrap_or(false);

    let kiln_path = Path::new(kiln_str);
    let sessions_path = FileSessionStorage::sessions_base(kiln_path);

    if !sessions_path.exists() {
        return Response::success(
            req.id,
            serde_json::json!({ "indexed": 0, "skipped": 0, "errors": 0 }),
        );
    }

    let ids = match crucible_observe::list_sessions(&sessions_path).await {
        Ok(ids) => ids,
        Err(e) => return internal_error(req.id, e),
    };

    let handle = match km.get_or_open(kiln_path).await {
        Ok(h) => h,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();

    let mut indexed = 0u32;
    let mut skipped = 0u32;
    let mut errors = 0u32;

    for id in &ids {
        let session_dir = sessions_path.join(id.as_str());
        let path = format!("sessions/{}", id.as_str());

        if !force {
            match note_store.get(&path).await {
                Ok(Some(_)) => {
                    skipped += 1;
                    continue;
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }

        let events = match crucible_observe::load_events(&session_dir).await {
            Ok(e) => e,
            Err(_) => {
                errors += 1;
                continue;
            }
        };

        let content = match crucible_observe::extract_session_content(id.as_str(), &events) {
            Some(c) => c,
            None => {
                skipped += 1;
                continue;
            }
        };

        let record = content.to_note_record(None);
        if note_store.upsert(record).await.is_err() {
            errors += 1;
            continue;
        }

        indexed += 1;
    }

    Response::success(
        req.id,
        serde_json::json!({
            "indexed": indexed,
            "skipped": skipped,
            "errors": errors,
        }),
    )
}

// =============================================================================
// MCP Server RPC Handlers
// =============================================================================

async fn handle_mcp_start(
    req: Request,
    km: &Arc<KilnManager>,
    mcp_mgr: &Arc<McpServerManager>,
) -> Response {
    let kiln_path = require_param!(req, "kiln_path", as_str);
    let transport = optional_param!(req, "transport", as_str).unwrap_or("sse");
    let port = optional_param!(req, "port", as_u64).unwrap_or(3847) as u16;
    let no_just = optional_param!(req, "no_just", as_bool).unwrap_or(false);
    let just_dir = optional_param!(req, "just_dir", as_str);

    match mcp_mgr
        .start(km, transport, port, kiln_path, no_just, just_dir)
        .await
    {
        Ok(result) => Response::success(req.id, result),
        Err(e) => Response::error(req.id, INVALID_PARAMS, e),
    }
}

async fn handle_mcp_stop(req: Request, mcp_mgr: &Arc<McpServerManager>) -> Response {
    match mcp_mgr.stop().await {
        Ok(result) => Response::success(req.id, result),
        Err(e) => Response::error(req.id, INVALID_PARAMS, e),
    }
}

async fn handle_mcp_status(req: Request, mcp_mgr: &Arc<McpServerManager>) -> Response {
    let status = mcp_mgr.status().await;
    Response::success(req.id, status)
}

async fn handle_skills_list(req: Request) -> Response {
    let kiln_path = require_param!(req, "kiln_path", as_str).to_string();
    let scope_filter = optional_param!(req, "scope_filter", as_str).map(|s| s.to_string());

    let result = tokio::task::spawn_blocking(move || {
        let cwd = std::env::current_dir().unwrap_or_default();
        let kiln = PathBuf::from(&kiln_path);
        let paths = default_discovery_paths(Some(&cwd), Some(&kiln));
        let discovery = FolderDiscovery::new(paths);
        discovery.discover()
    })
    .await;

    match result {
        Ok(Ok(skills)) => {
            let mut entries: Vec<serde_json::Value> = skills
                .iter()
                .filter(|(_, resolved)| {
                    if let Some(ref filter) = scope_filter {
                        resolved.skill.source.scope.to_string() == *filter
                    } else {
                        true
                    }
                })
                .map(|(name, resolved)| {
                    serde_json::json!({
                        "name": name,
                        "scope": resolved.skill.source.scope.to_string(),
                        "description": resolved.skill.description,
                        "shadowed_count": resolved.shadowed.len(),
                    })
                })
                .collect();
            entries.sort_by(|a, b| {
                a["name"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(b["name"].as_str().unwrap_or(""))
            });
            Response::success(req.id, serde_json::json!({ "skills": entries }))
        }
        Ok(Err(e)) => internal_error(req.id, e),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_skills_get(req: Request) -> Response {
    let name = require_param!(req, "name", as_str).to_string();
    let kiln_path = require_param!(req, "kiln_path", as_str).to_string();

    let result = tokio::task::spawn_blocking(move || {
        let cwd = std::env::current_dir().unwrap_or_default();
        let kiln = PathBuf::from(&kiln_path);
        let paths = default_discovery_paths(Some(&cwd), Some(&kiln));
        let discovery = FolderDiscovery::new(paths);
        discovery.discover()
    })
    .await;

    match result {
        Ok(Ok(skills)) => match skills.get(&name) {
            Some(resolved) => {
                let skill = &resolved.skill;
                Response::success(
                    req.id,
                    serde_json::json!({
                        "name": skill.name,
                        "scope": skill.source.scope.to_string(),
                        "description": skill.description,
                        "source_path": skill.source.path.to_string_lossy(),
                        "agent": skill.source.agent,
                        "license": skill.license,
                        "body": skill.body,
                    }),
                )
            }
            None => Response::error(req.id, INVALID_PARAMS, format!("Skill not found: {}", name)),
        },
        Ok(Err(e)) => internal_error(req.id, e),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_skills_search(req: Request) -> Response {
    let query = require_param!(req, "query", as_str).to_string();
    let kiln_path = require_param!(req, "kiln_path", as_str).to_string();
    let limit = optional_param!(req, "limit", as_u64).unwrap_or(20) as usize;

    let result = tokio::task::spawn_blocking(move || {
        let cwd = std::env::current_dir().unwrap_or_default();
        let kiln = PathBuf::from(&kiln_path);
        let paths = default_discovery_paths(Some(&cwd), Some(&kiln));
        let discovery = FolderDiscovery::new(paths);
        discovery.discover()
    })
    .await;

    match result {
        Ok(Ok(skills)) => {
            let query_lower = query.to_lowercase();
            let matches: Vec<serde_json::Value> = skills
                .iter()
                .filter(|(name, resolved)| {
                    name.to_lowercase().contains(&query_lower)
                        || resolved
                            .skill
                            .description
                            .to_lowercase()
                            .contains(&query_lower)
                })
                .take(limit)
                .map(|(name, resolved)| {
                    serde_json::json!({
                        "name": name,
                        "scope": resolved.skill.source.scope.to_string(),
                        "description": resolved.skill.description,
                        "shadowed_count": resolved.shadowed.len(),
                    })
                })
                .collect();
            Response::success(req.id, serde_json::json!({ "skills": matches }))
        }
        Ok(Err(e)) => internal_error(req.id, e),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_agents_list_profiles(req: Request, agent_manager: &Arc<AgentManager>) -> Response {
    let profiles = agent_manager.build_available_agents();
    let builtins = crucible_acp::discovery::default_agent_profiles();

    let mut entries: Vec<serde_json::Value> = profiles
        .iter()
        .map(|(name, profile)| {
            serde_json::json!({
                "name": name,
                "description": profile.description.clone().unwrap_or_default(),
                "command": profile.command.clone().unwrap_or_default(),
                "is_builtin": builtins.contains_key(name),
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });
    Response::success(req.id, serde_json::json!({ "profiles": entries }))
}

async fn handle_agents_resolve_profile(
    req: Request,
    agent_manager: &Arc<AgentManager>,
) -> Response {
    let name = require_param!(req, "name", as_str).to_string();
    let profiles = agent_manager.build_available_agents();
    let builtins = crucible_acp::discovery::default_agent_profiles();

    match profiles.get(&name) {
        Some(profile) => Response::success(
            req.id,
            serde_json::json!({
                "name": name,
                "description": profile.description.clone().unwrap_or_default(),
                "command": profile.command.clone().unwrap_or_default(),
                "is_builtin": builtins.contains_key(&name),
                "args": profile.args.clone().unwrap_or_default(),
                "env": profile.env,
            }),
        ),
        None => Response::success(req.id, serde_json::Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_storage::FileSessionStorage;
    use serde_json::json;
    use serde_json::Value;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    fn build_llm_config(
        default_key: &str,
        provider_type: crucible_config::BackendType,
    ) -> LlmConfig {
        build_llm_config_with_trust(default_key, provider_type, None)
    }

    fn build_llm_config_with_trust(
        default_key: &str,
        provider_type: crucible_config::BackendType,
        trust_level: Option<crucible_config::TrustLevel>,
    ) -> LlmConfig {
        let mut providers = HashMap::new();
        providers.insert(
            default_key.to_string(),
            crucible_config::LlmProviderConfig {
                provider_type,
                endpoint: None,
                default_model: None,
                temperature: None,
                max_tokens: None,
                timeout_secs: None,
                api_key: None,
                available_models: None,
                trust_level,
            },
        );
        LlmConfig {
            default: Some(default_key.to_string()),
            providers,
        }
    }

    fn create_session_request(kiln: &Path, workspace: &Path, provider_key: &str) -> Request {
        serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.create",
            "params": {
                "type": "chat",
                "kiln": kiln,
                "workspace": workspace,
                "provider_key": provider_key
            }
        }))
        .unwrap()
    }

    fn write_workspace_config(
        workspace: &Path,
        kiln_relative_path: &str,
        classification: Option<&str>,
    ) {
        let crucible_dir = workspace.join(".crucible");
        std::fs::create_dir_all(&crucible_dir).unwrap();
        let mut config = format!(
            "[workspace]\nname = \"test\"\n\n[[kilns]]\npath = \"{}\"\n",
            kiln_relative_path
        );
        if let Some(classification) = classification {
            config.push_str(&format!("data_classification = \"{}\"\n", classification));
        }
        std::fs::write(crucible_dir.join("workspace.toml"), config).unwrap();
    }

    async fn rpc_call(client: &mut UnixStream, request: Value) -> Value {
        let request = serde_json::to_string(&request).unwrap();
        client
            .write_all(format!("{}\n", request).as_bytes())
            .await
            .unwrap();

        let mut buf = vec![0u8; 8192];
        let n = client.read(&mut buf).await.unwrap();
        serde_json::from_slice(&buf[..n]).unwrap()
    }

    fn extract_session_id(response: &Value) -> String {
        response["result"]["session_id"]
            .as_str()
            .expect("session.create should return session_id")
            .to_string()
    }

    async fn create_chat_session(client: &mut UnixStream, kiln: &Path, id: u64) -> String {
        let response = rpc_call(
            client,
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "session.create",
                "params": {
                    "type": "chat",
                    "kiln": kiln,
                }
            }),
        )
        .await;

        assert!(
            response["error"].is_null(),
            "session.create failed: {response:?}"
        );
        extract_session_id(&response)
    }

    async fn configure_internal_mock_agent(
        client: &mut UnixStream,
        session_id: &str,
        id: u64,
        model: &str,
    ) -> Value {
        rpc_call(
            client,
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "session.configure_agent",
                "params": {
                    "session_id": session_id,
                    "agent": {
                        "agent_type": "internal",
                        "provider": "mock",
                        "model": model,
                        "system_prompt": "test",
                        "provider_key": "mock"
                    }
                }
            }),
        )
        .await
    }

    #[tokio::test]
    async fn cloud_provider_confidential_kiln_returns_insufficient_error() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("workspace");
        let kiln = workspace.join("notes");
        std::fs::create_dir_all(&kiln).unwrap();
        write_workspace_config(&workspace, "./notes", Some("confidential"));

        let llm_config = Some(build_llm_config(
            "cloud",
            crucible_config::BackendType::OpenAI,
        ));
        let request = create_session_request(&kiln, &workspace, "cloud");

        let storage = Arc::new(FileSessionStorage::new());
        let sm = Arc::new(SessionManager::with_storage(storage));
        let pm = Arc::new(ProjectManager::new(tmp.path().join("projects.json")));

        let response = handle_session_create(request, &sm, &pm, &llm_config).await;
        let error = response.error.expect("expected trust-level rejection");

        assert_eq!(error.code, INVALID_PARAMS);
        assert!(error.message.contains("insufficient"));
        assert!(error.message.contains("cloud"));
        assert!(error.message.contains("confidential"));
        assert_eq!(sm.list_sessions().len(), 0);
    }

    #[tokio::test]
    async fn local_provider_confidential_kiln_allows_session_creation() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("workspace");
        let kiln = workspace.join("notes");
        std::fs::create_dir_all(&kiln).unwrap();
        write_workspace_config(&workspace, "./notes", Some("confidential"));

        let llm_config = Some(build_llm_config(
            "local",
            crucible_config::BackendType::Mock,
        ));
        let request = create_session_request(&kiln, &workspace, "local");

        let storage = Arc::new(FileSessionStorage::new());
        let sm = Arc::new(SessionManager::with_storage(storage));
        let pm = Arc::new(ProjectManager::new(tmp.path().join("projects.json")));

        let response = handle_session_create(request, &sm, &pm, &llm_config).await;

        assert!(response.error.is_none());
        assert!(response.result.is_some());
        assert_eq!(sm.list_sessions().len(), 1);
    }

    #[tokio::test]
    async fn cloud_provider_public_or_missing_classification_allows_session_creation() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("workspace");
        let kiln = workspace.join("notes");
        std::fs::create_dir_all(&kiln).unwrap();
        write_workspace_config(&workspace, "./notes", None);

        let llm_config = Some(build_llm_config(
            "cloud",
            crucible_config::BackendType::OpenAI,
        ));
        let request = create_session_request(&kiln, &workspace, "cloud");

        let storage = Arc::new(FileSessionStorage::new());
        let sm = Arc::new(SessionManager::with_storage(storage));
        let pm = Arc::new(ProjectManager::new(tmp.path().join("projects.json")));

        let response = handle_session_create(request, &sm, &pm, &llm_config).await;

        assert!(response.error.is_none());
        assert!(response.result.is_some());
        assert_eq!(sm.list_sessions().len(), 1);
    }

    #[tokio::test]
    async fn untrusted_provider_internal_kiln_returns_error() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("workspace");
        let kiln = workspace.join("notes");
        std::fs::create_dir_all(&kiln).unwrap();
        write_workspace_config(&workspace, "./notes", Some("internal"));

        let llm_config = Some(build_llm_config_with_trust(
            "untrusted",
            crucible_config::BackendType::Custom,
            Some(crucible_config::TrustLevel::Untrusted),
        ));
        let request = create_session_request(&kiln, &workspace, "untrusted");

        let storage = Arc::new(FileSessionStorage::new());
        let sm = Arc::new(SessionManager::with_storage(storage));
        let pm = Arc::new(ProjectManager::new(tmp.path().join("projects.json")));

        let response = handle_session_create(request, &sm, &pm, &llm_config).await;
        let error = response.error.expect("expected trust-level rejection");

        assert_eq!(error.code, INVALID_PARAMS);
        assert!(error.message.contains("insufficient"));
        assert!(error.message.contains("untrusted"));
        assert!(error.message.contains("internal"));
        assert_eq!(sm.list_sessions().len(), 0);
    }

    #[tokio::test]
    async fn test_server_ping() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();

        // Spawn server
        let server_task = tokio::spawn(async move { server.run().await });

        // Give server time to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Connect and send ping
        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"result\":\"pong\""));
        assert!(response.contains("\"id\":1"));

        // Shutdown
        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_kiln_open_missing_path_param() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        // Missing "path" parameter
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"kiln.open\",\"params\":{}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32602")); // INVALID_PARAMS

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_kiln_close_missing_path_param() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        // Missing "path" parameter
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"kiln.close\",\"params\":{}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32602")); // INVALID_PARAMS

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_kiln_list_returns_array() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"kiln.list\",\"params\":{}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"result\":[]")); // Empty array initially

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_search_vectors_rpc_success_and_missing_vector_error() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();

        let open_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 10,
                "method": "kiln.open",
                "params": { "path": kiln_path }
            }),
        )
        .await;
        assert!(
            open_response["error"].is_null(),
            "kiln.open failed: {open_response:?}"
        );

        let ok_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 11,
                "method": "search_vectors",
                "params": {
                    "kiln": kiln_path,
                    "vector": [0.1, 0.2, 0.3],
                    "limit": 5
                }
            }),
        )
        .await;
        assert!(
            ok_response["error"].is_null(),
            "search_vectors failed: {ok_response:?}"
        );
        assert!(ok_response["result"].is_array());

        let err_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 12,
                "method": "search_vectors",
                "params": {
                    "kiln": kiln_path
                }
            }),
        )
        .await;
        assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_list_rpc_returns_shape_and_accepts_invalid_filters() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        let _session_id = create_chat_session(&mut client, &kiln_path, 20).await;

        let ok_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 21,
                "method": "session.list",
                "params": {}
            }),
        )
        .await;
        assert!(
            ok_response["error"].is_null(),
            "session.list failed: {ok_response:?}"
        );
        assert!(ok_response["result"]["sessions"].is_array());
        assert!(ok_response["result"]["total"].as_u64().unwrap_or(0) >= 1);

        let invalid_filters_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 22,
                "method": "session.list",
                "params": {
                    "type": 123,
                    "state": ["bad"],
                    "kiln": false
                }
            }),
        )
        .await;
        assert!(
            invalid_filters_response["error"].is_null(),
            "session.list should ignore invalid optional filters: {invalid_filters_response:?}"
        );
        assert!(invalid_filters_response["result"]["sessions"].is_array());

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_pause_rpc_success_and_missing_param_error() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        let session_id = create_chat_session(&mut client, &kiln_path, 30).await;

        let ok_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 31,
                "method": "session.pause",
                "params": { "session_id": session_id }
            }),
        )
        .await;
        assert!(
            ok_response["error"].is_null(),
            "session.pause failed: {ok_response:?}"
        );
        assert_eq!(ok_response["result"]["state"], "paused");

        let err_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 32,
                "method": "session.pause",
                "params": {}
            }),
        )
        .await;
        assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_resume_rpc_success_and_missing_param_error() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        let session_id = create_chat_session(&mut client, &kiln_path, 40).await;

        let _pause_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 41,
                "method": "session.pause",
                "params": { "session_id": session_id }
            }),
        )
        .await;

        let ok_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 42,
                "method": "session.resume",
                "params": { "session_id": session_id }
            }),
        )
        .await;
        assert!(
            ok_response["error"].is_null(),
            "session.resume failed: {ok_response:?}"
        );
        assert_eq!(ok_response["result"]["state"], "active");

        let err_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 43,
                "method": "session.resume",
                "params": {}
            }),
        )
        .await;
        assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_configure_agent_rpc_success_and_missing_agent_error() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        let session_id = create_chat_session(&mut client, &kiln_path, 50).await;

        let ok_response =
            configure_internal_mock_agent(&mut client, &session_id, 51, "mock-initial").await;
        assert!(
            ok_response["error"].is_null(),
            "session.configure_agent failed: {ok_response:?}"
        );
        assert_eq!(ok_response["result"]["configured"], true);

        let err_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 52,
                "method": "session.configure_agent",
                "params": {
                    "session_id": session_id
                }
            }),
        )
        .await;
        assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_send_message_rpc_no_agent_configured_error_and_missing_content_error() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        let session_id = create_chat_session(&mut client, &kiln_path, 60).await;

        let no_agent_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 61,
                "method": "session.send_message",
                "params": {
                    "session_id": session_id,
                    "content": "hello"
                }
            }),
        )
        .await;
        assert!(no_agent_response["error"].is_object());
        assert_eq!(no_agent_response["error"]["code"], INTERNAL_ERROR);

        let missing_content_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 62,
                "method": "session.send_message",
                "params": {
                    "session_id": session_id
                }
            }),
        )
        .await;
        assert_eq!(missing_content_response["error"]["code"], INVALID_PARAMS);

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_cancel_rpc_success_and_missing_param_error() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        let session_id = create_chat_session(&mut client, &kiln_path, 70).await;

        let ok_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 71,
                "method": "session.cancel",
                "params": { "session_id": session_id }
            }),
        )
        .await;
        assert!(
            ok_response["error"].is_null(),
            "session.cancel failed: {ok_response:?}"
        );
        assert!(ok_response["result"]["cancelled"].is_boolean());

        let err_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 72,
                "method": "session.cancel",
                "params": {}
            }),
        )
        .await;
        assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_switch_model_rpc_success_and_empty_model_error() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        let session_id = create_chat_session(&mut client, &kiln_path, 80).await;
        let configure_response =
            configure_internal_mock_agent(&mut client, &session_id, 81, "mock-initial").await;
        assert!(
            configure_response["error"].is_null(),
            "configure failed: {configure_response:?}"
        );

        let ok_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 82,
                "method": "session.switch_model",
                "params": {
                    "session_id": session_id,
                    "model_id": "mock-switched"
                }
            }),
        )
        .await;
        assert!(
            ok_response["error"].is_null(),
            "session.switch_model failed: {ok_response:?}"
        );
        assert_eq!(ok_response["result"]["switched"], true);
        assert_eq!(ok_response["result"]["model_id"], "mock-switched");

        let err_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 83,
                "method": "session.switch_model",
                "params": {
                    "session_id": session_id,
                    "model_id": "   "
                }
            }),
        )
        .await;
        assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_list_models_rpc_success_and_missing_param_error() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        let session_id = create_chat_session(&mut client, &kiln_path, 90).await;

        let ok_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 91,
                "method": "session.list_models",
                "params": {
                    "session_id": session_id
                }
            }),
        )
        .await;
        assert!(
            ok_response["error"].is_null(),
            "session.list_models failed: {ok_response:?}"
        );
        assert!(ok_response["result"]["models"].is_array());

        let err_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 92,
                "method": "session.list_models",
                "params": {}
            }),
        )
        .await;
        assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_models_list_rpc_no_session() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();

        // Call models.list with no params — should succeed without a session
        let response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "models.list",
                "params": {}
            }),
        )
        .await;
        assert!(
            response["error"].is_null(),
            "models.list failed: {response:?}"
        );
        assert!(
            response["result"]["models"].is_array(),
            "models.list should return a models array: {response:?}"
        );

        // Call models.list with a kiln_path — should also succeed
        let response_with_kiln = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "models.list",
                "params": {
                    "kiln_path": tmp.path().to_string_lossy()
                }
            }),
        )
        .await;
        assert!(
            response_with_kiln["error"].is_null(),
            "models.list with kiln_path failed: {response_with_kiln:?}"
        );
        assert!(response_with_kiln["result"]["models"].is_array());

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_set_thinking_budget_rpc_success_and_missing_session_id_error() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        let session_id = create_chat_session(&mut client, &kiln_path, 100).await;
        let configure_response =
            configure_internal_mock_agent(&mut client, &session_id, 101, "mock-budget").await;
        assert!(
            configure_response["error"].is_null(),
            "configure failed: {configure_response:?}"
        );

        let ok_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 102,
                "method": "session.set_thinking_budget",
                "params": {
                    "session_id": session_id,
                    "thinking_budget": 256
                }
            }),
        )
        .await;
        assert!(
            ok_response["error"].is_null(),
            "session.set_thinking_budget failed: {ok_response:?}"
        );
        assert_eq!(ok_response["result"]["thinking_budget"], 256);

        let err_response = rpc_call(
            &mut client,
            json!({
                "jsonrpc": "2.0",
                "id": 103,
                "method": "session.set_thinking_budget",
                "params": {
                    "thinking_budget": 1
                }
            }),
        )
        .await;
        assert_eq!(err_response["error"]["code"], INVALID_PARAMS);

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_method_not_found() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(
                b"{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"unknown.method\",\"params\":{}}\n",
            )
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32601")); // METHOD_NOT_FOUND

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_parse_error() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        // Invalid JSON
        client.write_all(b"{invalid json}\n").await.unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32700")); // PARSE_ERROR

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_shutdown_method() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"shutdown\",\"params\":{}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"result\":\"shutting down\""));

        // Server should shut down gracefully
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), server_task).await;

        assert!(result.is_ok(), "Server should shutdown within timeout");
    }

    #[tokio::test]
    async fn test_kiln_open_nonexistent_path_fails() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        // Valid request format, but path doesn't exist
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"kiln.open\",\"params\":{\"path\":\"/nonexistent/path/to/kiln\"}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32603")); // INTERNAL_ERROR (can't open DB)

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_client_disconnect_closes_connection() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Connect and immediately disconnect
        {
            let _client = UnixStream::connect(&sock_path).await.unwrap();
            // Client drops here, closing connection
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Server should still be running and accept new connections
        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"result\":\"pong\""));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_server_has_event_broadcast() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let event_tx = server.event_sender();

        // Subscribe a receiver so send() succeeds
        let mut rx = event_tx.subscribe();

        // Should be able to send events
        let event = SessionEventMessage::text_delta("test-session", "hello");
        assert!(event_tx.send(event).is_ok());

        // Verify the event was received
        let received = rx.recv().await.unwrap();
        assert_eq!(received.session_id, "test-session");
        assert_eq!(received.event, "text_delta");
    }

    #[tokio::test]
    async fn test_session_subscribe_rpc() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"chat-test\"]}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"subscribed\""));
        assert!(response.contains("chat-test"));
        assert!(response.contains("\"client_id\""));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_subscribe_multiple_sessions() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"session-1\",\"session-2\",\"session-3\"]}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"subscribed\""));
        assert!(response.contains("session-1"));
        assert!(response.contains("session-2"));
        assert!(response.contains("session-3"));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_subscribe_wildcard() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"*\"]}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"subscribed\""));
        assert!(response.contains("\"*\""));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_subscribe_missing_session_ids() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(
                b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{}}\n",
            )
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32602")); // INVALID_PARAMS
        assert!(response.contains("session_ids"));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_subscribe_invalid_session_ids_type() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        // session_ids is a string, not an array
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":\"not-an-array\"}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32602")); // INVALID_PARAMS
        assert!(
            response.contains("session_ids") || response.contains("invalid type"),
            "Expected error message to mention 'session_ids' or 'invalid type', got: {}",
            response
        );

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_unsubscribe_rpc() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();

        // First subscribe
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"chat-test\"]}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let _ = client.read(&mut buf).await.unwrap();

        // Then unsubscribe
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"session.unsubscribe\",\"params\":{\"session_ids\":[\"chat-test\"]}}\n")
            .await
            .unwrap();

        buf.fill(0);
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"unsubscribed\""));
        assert!(response.contains("chat-test"));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_unsubscribe_missing_session_ids() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.unsubscribe\",\"params\":{}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32602")); // INVALID_PARAMS
        assert!(response.contains("session_ids"));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_event_broadcast_to_subscriber() {
        use std::time::Duration;

        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let event_tx = server.event_sender();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();

        // Subscribe to a session
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"chat-test\"]}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 4096];
        let _ = client.read(&mut buf).await.unwrap(); // consume subscription response

        // Send event through broadcast channel
        let event = SessionEventMessage::text_delta("chat-test", "hello world");
        event_tx.send(event).unwrap();

        // Client should receive the event
        tokio::time::sleep(Duration::from_millis(100)).await;

        buf.fill(0);
        let n = tokio::time::timeout(Duration::from_millis(500), client.read(&mut buf))
            .await
            .expect("timeout waiting for event")
            .unwrap();

        let received = String::from_utf8_lossy(&buf[..n]);
        assert!(
            received.contains("\"type\":\"event\""),
            "Response: {}",
            received
        );
        assert!(
            received.contains("\"session_id\":\"chat-test\""),
            "Response: {}",
            received
        );
        assert!(received.contains("hello world"), "Response: {}", received);

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_event_not_sent_to_non_subscriber() {
        use std::time::Duration;

        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let event_tx = server.event_sender();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();

        // Subscribe to session "other-session"
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"other-session\"]}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 4096];
        let _ = client.read(&mut buf).await.unwrap(); // consume subscription response

        // Send event for "chat-test" (different session)
        let event = SessionEventMessage::text_delta("chat-test", "should not receive");
        event_tx.send(event).unwrap();

        // Client should NOT receive the event (timeout expected)
        tokio::time::sleep(Duration::from_millis(50)).await;

        buf.fill(0);
        let result = tokio::time::timeout(Duration::from_millis(100), client.read(&mut buf)).await;
        assert!(
            result.is_err(),
            "Should timeout - client shouldn't receive unsubscribed events"
        );

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_process_batch_emits_per_file_progress_events() {
        use std::time::Duration;

        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        let good_file = kiln_path.join("ok.md");
        std::fs::write(&good_file, "# ok\n").unwrap();
        let missing_file = kiln_path.join("missing.md");

        let km = Arc::new(KilnManager::new());
        let (event_tx, _) = broadcast::channel(64);
        let mut event_rx = event_tx.subscribe();

        let req = Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(42)),
            method: "process_batch".to_string(),
            params: serde_json::json!({
                "kiln": kiln_path.to_string_lossy(),
                "paths": [
                    good_file.to_string_lossy(),
                    missing_file.to_string_lossy()
                ]
            }),
        };

        let response = handle_process_batch(req, &km, &event_tx).await;
        assert!(response.error.is_none());

        let mut events = Vec::new();
        for _ in 0..4 {
            let event = tokio::time::timeout(Duration::from_secs(2), event_rx.recv())
                .await
                .expect("timed out waiting for process event")
                .expect("event channel closed unexpectedly");
            events.push(event);
        }

        let progress_events: Vec<&SessionEventMessage> = events
            .iter()
            .filter(|e| e.event == "process_progress")
            .collect();
        assert_eq!(
            progress_events.len(),
            2,
            "expected 2 process_progress events"
        );

        let processed_event = progress_events
            .iter()
            .find(|e| {
                e.data.get("file").and_then(|v| v.as_str())
                    == Some(good_file.to_string_lossy().as_ref())
            })
            .expect("missing progress event for processed file");
        assert_eq!(
            processed_event.data.get("type").and_then(|v| v.as_str()),
            Some("process_progress")
        );
        assert_eq!(
            processed_event.data.get("result").and_then(|v| v.as_str()),
            Some("processed")
        );

        let error_event = progress_events
            .iter()
            .find(|e| {
                e.data.get("file").and_then(|v| v.as_str())
                    == Some(missing_file.to_string_lossy().as_ref())
            })
            .expect("missing progress event for failed file");
        assert_eq!(
            error_event.data.get("result").and_then(|v| v.as_str()),
            Some("error")
        );
        assert!(error_event
            .data
            .get("error_msg")
            .and_then(|v| v.as_str())
            .is_some());
    }

    #[tokio::test]
    async fn test_file_deleted_event_removes_note_from_store() {
        use crucible_core::parser::BlockHash;
        use crucible_core::storage::{NoteRecord, NoteStore};
        use std::time::Duration;

        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(kiln_path.join("notes")).unwrap();

        let server = Server::bind(&sock_path, None).await.unwrap();
        let km = server.kiln_manager.clone();
        let event_tx = server.event_sender();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(50)).await;

        let handle = km.get_or_open(&kiln_path).await.unwrap();
        let note_store = handle.as_note_store();

        let deleted_note_path = "notes/deleted.md";
        let keep_note_path = "notes/keep.md";

        note_store
            .upsert(
                NoteRecord::new(deleted_note_path, BlockHash::zero())
                    .with_title("Deleted")
                    .with_links(vec!["notes/target.md".to_string()]),
            )
            .await
            .unwrap();
        note_store
            .upsert(NoteRecord::new(keep_note_path, BlockHash::zero()).with_title("Keep"))
            .await
            .unwrap();

        assert!(note_store.get(deleted_note_path).await.unwrap().is_some());
        assert!(note_store.get(keep_note_path).await.unwrap().is_some());

        event_tx
            .send(SessionEventMessage::new(
                "system",
                "file_deleted",
                json!({ "path": kiln_path.join(deleted_note_path).to_string_lossy() }),
            ))
            .unwrap();

        let removed = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if note_store.get(deleted_note_path).await.unwrap().is_none() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        assert!(
            removed.is_ok(),
            "deleted note should be removed after event"
        );

        event_tx
            .send(SessionEventMessage::new(
                "system",
                "file_deleted",
                json!({ "path": kiln_path.join("notes/ignore.txt").to_string_lossy() }),
            ))
            .unwrap();
        event_tx
            .send(SessionEventMessage::new(
                "system",
                "file_deleted",
                json!({ "path": kiln_path.join("notes/missing.md").to_string_lossy() }),
            ))
            .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(note_store.get(keep_note_path).await.unwrap().is_some());

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_events_auto_persisted() {
        use std::time::Duration;

        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        let server = Server::bind(&sock_path, None).await.unwrap();
        let event_tx = server.event_sender();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();

        // Create a session
        let create_req = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"session.create","params":{{"type":"chat","kiln":"{}"}}}}"#,
            kiln_path.display()
        );
        client.write_all(create_req.as_bytes()).await.unwrap();
        client.write_all(b"\n").await.unwrap();

        let mut buf = vec![0u8; 4096];
        let n = client.read(&mut buf).await.unwrap();
        let response: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();
        let session_id = response["result"]["session_id"]
            .as_str()
            .unwrap()
            .to_string();

        // Send event through broadcast channel
        // Use user_message since text_delta is filtered out to reduce storage
        let event = SessionEventMessage::user_message(&session_id, "msg-1", "hello world");
        event_tx.send(event).unwrap();

        // Wait for persistence
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Check that event was persisted
        let session_dir = kiln_path
            .join(".crucible")
            .join("sessions")
            .join(&session_id);
        let jsonl_path = session_dir.join("session.jsonl");

        let content = tokio::fs::read_to_string(&jsonl_path).await.unwrap();
        assert!(content.contains("hello world"));
        assert!(content.contains("user_message"));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[test]
    fn test_emitted_event_has_timestamp() {
        let seq_counter = std::sync::atomic::AtomicU64::new(0);
        let event = SessionEventMessage::text_delta("test-session", "hello");

        let stamped = stamp_event(event, &seq_counter);

        assert!(stamped.timestamp.is_some());
    }

    #[test]
    fn test_emitted_events_have_increasing_seq() {
        let seq_counter = std::sync::atomic::AtomicU64::new(0);

        let events: Vec<SessionEventMessage> = (0..5)
            .map(|_| {
                stamp_event(
                    SessionEventMessage::text_delta("test-session", "x"),
                    &seq_counter,
                )
            })
            .collect();

        let seqs: Vec<u64> = events.into_iter().map(|event| event.seq.unwrap()).collect();
        assert_eq!(seqs, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_timestamp_not_in_constructor() {
        let event = SessionEventMessage::text_delta("test-session", "hello");
        assert!(event.timestamp.is_none());
    }

    #[test]
    fn test_internal_error_returns_correct_code_and_message() {
        let req_id = Some(RequestId::Number(42));
        let err_msg = "database connection failed";
        let response = internal_error(req_id.clone(), err_msg);

        assert_eq!(response.id, req_id);
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, INTERNAL_ERROR);
        assert_eq!(error.message, "Internal server error");
        assert!(response.result.is_none());
    }

    #[test]
    fn test_invalid_state_error_returns_correct_code_and_message() {
        let req_id = Some(RequestId::String("test-id".to_string()));
        let operation = "pause_session";
        let err_msg = "session already paused";
        let response = invalid_state_error(req_id.clone(), operation, err_msg);

        assert_eq!(response.id, req_id);
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, INVALID_PARAMS);
        assert!(error.message.contains(operation));
        assert!(error.message.contains("not allowed"));
        assert!(response.result.is_none());
    }

    #[test]
    fn test_session_not_found_includes_session_id() {
        let req_id = Some(RequestId::Number(1));
        let session_id = "sess-123-abc";
        let response = session_not_found(req_id.clone(), session_id);

        assert_eq!(response.id, req_id);
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, INVALID_PARAMS);
        assert!(error.message.contains(session_id));
        assert!(error.message.contains("not found"));
        assert!(response.result.is_none());
    }

    #[test]
    fn test_agent_not_configured_includes_session_id() {
        let req_id = None;
        let session_id = "sess-xyz-789";
        let response = agent_not_configured(req_id, session_id);

        assert_eq!(response.id, None);
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, INVALID_PARAMS);
        assert!(error.message.contains(session_id));
        assert!(error.message.contains("No agent"));
        assert!(response.result.is_none());
    }

    #[test]
    fn test_concurrent_request_includes_session_id() {
        let req_id = Some(RequestId::Number(99));
        let session_id = "sess-concurrent-test";
        let response = concurrent_request(req_id.clone(), session_id);

        assert_eq!(response.id, req_id);
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, INVALID_PARAMS);
        assert!(error.message.contains(session_id));
        assert!(error.message.contains("already in progress"));
        assert!(response.result.is_none());
    }

    #[test]
    fn test_agent_error_to_response_dispatches_correctly() {
        // Test SessionNotFound variant
        let req_id = Some(RequestId::Number(1));
        let err = AgentError::SessionNotFound("sess-1".to_string());
        let response = agent_error_to_response(req_id.clone(), err);

        assert_eq!(response.id, req_id);
        let error = response.error.unwrap();
        assert_eq!(error.code, INVALID_PARAMS);
        assert!(error.message.contains("sess-1"));

        // Test NoAgentConfigured variant
        let err = AgentError::NoAgentConfigured("sess-2".to_string());
        let response = agent_error_to_response(req_id.clone(), err);
        let error = response.error.unwrap();
        assert_eq!(error.code, INVALID_PARAMS);
        assert!(error.message.contains("sess-2"));

        // Test ConcurrentRequest variant
        let err = AgentError::ConcurrentRequest("sess-3".to_string());
        let response = agent_error_to_response(req_id.clone(), err);
        let error = response.error.unwrap();
        assert_eq!(error.code, INVALID_PARAMS);
        assert!(error.message.contains("sess-3"));
    }

    mod persist_event_tests {
        use super::*;
        use crate::session_manager::SessionError;
        use crate::session_storage::SessionStorage;
        use async_trait::async_trait;
        use crucible_core::session::{SessionSummary, SessionType};

        struct FailingStorage;

        #[async_trait]
        impl SessionStorage for FailingStorage {
            async fn save(&self, _s: &crucible_core::session::Session) -> Result<(), SessionError> {
                Ok(())
            }
            async fn load(
                &self,
                _id: &str,
                _k: &Path,
            ) -> Result<crucible_core::session::Session, SessionError> {
                Err(SessionError::NotFound("mock".to_string()))
            }
            async fn list(&self, _k: &Path) -> Result<Vec<SessionSummary>, SessionError> {
                Ok(vec![])
            }
            async fn append_event(
                &self,
                _s: &crucible_core::session::Session,
                _e: &str,
            ) -> Result<(), SessionError> {
                Err(SessionError::IoError("simulated disk failure".to_string()))
            }
            async fn append_markdown(
                &self,
                _s: &crucible_core::session::Session,
                _r: &str,
                _c: &str,
            ) -> Result<(), SessionError> {
                Err(SessionError::IoError("simulated disk failure".to_string()))
            }
            async fn load_events(
                &self,
                _id: &str,
                _k: &Path,
                _limit: Option<usize>,
                _offset: Option<usize>,
            ) -> Result<Vec<serde_json::Value>, SessionError> {
                Ok(vec![])
            }
            async fn count_events(&self, _id: &str, _k: &Path) -> Result<usize, SessionError> {
                Ok(0)
            }
        }

        #[tokio::test]
        async fn test_persist_event_returns_error_on_storage_failure() {
            let tmp = TempDir::new().unwrap();
            let sm = Arc::new(SessionManager::new());
            let session = sm
                .create_session(
                    SessionType::Chat,
                    tmp.path().to_path_buf(),
                    None,
                    vec![],
                    None,
                )
                .await
                .unwrap();

            let event = SessionEventMessage::new(
                session.id.clone(),
                "user_message",
                serde_json::json!({"content": "hello"}),
            );

            let storage = FailingStorage;
            let result = persist_event(&event, &sm, &storage).await;
            assert!(
                result.is_err(),
                "persist_event must propagate storage errors, not swallow them"
            );
        }

        #[tokio::test]
        async fn test_persist_event_skips_non_persistent_events() {
            let tmp = TempDir::new().unwrap();
            let sm = Arc::new(SessionManager::new());
            let session = sm
                .create_session(
                    SessionType::Chat,
                    tmp.path().to_path_buf(),
                    None,
                    vec![],
                    None,
                )
                .await
                .unwrap();

            let event = SessionEventMessage::new(
                session.id.clone(),
                "stream_chunk",
                serde_json::json!({"chunk": "partial"}),
            );

            let storage = FailingStorage;
            let result = persist_event(&event, &sm, &storage).await;
            assert!(
                result.is_ok(),
                "Non-persistent events should be skipped without error"
            );
        }

        #[tokio::test]
        async fn test_should_persist_filters_correctly() {
            let persistent = [
                "user_message",
                "message_complete",
                "tool_call",
                "tool_result",
                "model_switched",
                "ended",
            ];
            for event_name in &persistent {
                let event = SessionEventMessage::new("test", *event_name, serde_json::json!({}));
                assert!(should_persist(&event), "{} should be persisted", event_name);
            }

            let non_persistent = ["stream_chunk", "thinking", "status_update", "unknown"];
            for event_name in &non_persistent {
                let event = SessionEventMessage::new("test", *event_name, serde_json::json!({}));
                assert!(
                    !should_persist(&event),
                    "{} should NOT be persisted",
                    event_name
                );
            }

            let mut replay_event =
                SessionEventMessage::new("test", "user_message", serde_json::json!({}));
            replay_event.msg_type = "replay_event".to_string();
            assert!(
                !should_persist(&replay_event),
                "replay events should not be persisted"
            );
        }

        #[tokio::test]
        async fn test_session_create_with_granular_recording_mode() {
            let tmp = TempDir::new().unwrap();
            let sock_path = tmp.path().join("test.sock");

            let server = Server::bind(&sock_path, None).await.unwrap();
            let shutdown_handle = server.shutdown_handle();
            let server_task = tokio::spawn(server.run());

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            let mut client = UnixStream::connect(&sock_path).await.unwrap();
            client
                .write_all(
                    b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.create\",\"params\":{\"recording_mode\":\"granular\"}}\n",
                )
                .await
                .unwrap();

            let mut buf = vec![0u8; 2048];
            let n = client.read(&mut buf).await.unwrap();
            let response = String::from_utf8_lossy(&buf[..n]);

            assert!(
                response.contains("\"result\""),
                "Should have successful result"
            );
            assert!(
                response.contains("\"session_id\""),
                "Should have session_id in response"
            );

            let _ = shutdown_handle.send(());
            let _ = server_task.await;
        }

        #[tokio::test]
        async fn test_session_create_default_no_recording_mode() {
            let tmp = TempDir::new().unwrap();
            let sock_path = tmp.path().join("test.sock");

            let server = Server::bind(&sock_path, None).await.unwrap();
            let shutdown_handle = server.shutdown_handle();
            let server_task = tokio::spawn(server.run());

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            let mut client = UnixStream::connect(&sock_path).await.unwrap();
            // Create session without recording_mode parameter
            client
                .write_all(
                    b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.create\",\"params\":{}}\n",
                )
                .await
                .unwrap();

            let mut buf = vec![0u8; 2048];
            let n = client.read(&mut buf).await.unwrap();
            let response = String::from_utf8_lossy(&buf[..n]);

            assert!(
                response.contains("\"result\""),
                "Should have successful result"
            );
            assert!(
                response.contains("\"session_id\""),
                "Should have session_id in response"
            );

            let _ = shutdown_handle.send(());
            let _ = server_task.await;
        }

        #[tokio::test]
        async fn test_session_get_includes_recording_mode() {
            let tmp = TempDir::new().unwrap();
            let sock_path = tmp.path().join("test.sock");

            let server = Server::bind(&sock_path, None).await.unwrap();
            let shutdown_handle = server.shutdown_handle();
            let server_task = tokio::spawn(server.run());

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            let mut client = UnixStream::connect(&sock_path).await.unwrap();

            // First, create a session with granular recording mode
            client
                .write_all(
                    b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.create\",\"params\":{\"recording_mode\":\"granular\"}}\n",
                )
                .await
                .unwrap();

            let mut buf = vec![0u8; 2048];
            let n = client.read(&mut buf).await.unwrap();
            let response_str = String::from_utf8_lossy(&buf[..n]);

            // Extract session_id from response
            let response: serde_json::Value =
                serde_json::from_str(&response_str).expect("Failed to parse create response");
            let session_id = response["result"]["session_id"]
                .as_str()
                .expect("No session_id in response");

            // Now get the session and verify recording_mode is in response
            let get_request = format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"session.get\",\"params\":{{\"session_id\":\"{}\"}}}}\n",
                session_id
            );
            client.write_all(get_request.as_bytes()).await.unwrap();

            let mut buf = vec![0u8; 2048];
            let n = client.read(&mut buf).await.unwrap();
            let get_response = String::from_utf8_lossy(&buf[..n]);

            assert!(
                get_response.contains("recording_mode"),
                "session.get response should include recording_mode field"
            );
            assert!(
                get_response.contains("granular"),
                "recording_mode should be 'granular'"
            );

            let _ = shutdown_handle.send(());
            let _ = server_task.await;
        }

        #[tokio::test]
        async fn test_granular_session_creates_recording_file() {
            use std::time::Duration;

            let tmp = TempDir::new().unwrap();
            let sock_path = tmp.path().join("test.sock");
            let kiln_path = tmp.path().join("kiln");
            std::fs::create_dir_all(&kiln_path).unwrap();

            let server = Server::bind(&sock_path, None).await.unwrap();
            let event_tx = server.event_sender();
            let shutdown_handle = server.shutdown_handle();
            let server_task = tokio::spawn(server.run());

            tokio::time::sleep(Duration::from_millis(50)).await;

            let mut client = UnixStream::connect(&sock_path).await.unwrap();

            let create_req = format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"session.create","params":{{"type":"chat","kiln":"{}","recording_mode":"granular"}}}}"#,
                kiln_path.display()
            );
            client.write_all(create_req.as_bytes()).await.unwrap();
            client.write_all(b"\n").await.unwrap();

            let mut buf = vec![0u8; 4096];
            let n = client.read(&mut buf).await.unwrap();
            let response: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();
            let session_id = response["result"]["session_id"]
                .as_str()
                .unwrap()
                .to_string();

            let event = SessionEventMessage::text_delta(&session_id, "hello world");
            event_tx.send(event).unwrap();

            // Wait for recording writer flush (500ms interval + margin)
            tokio::time::sleep(Duration::from_millis(700)).await;

            let session_dir = kiln_path
                .join(".crucible")
                .join("sessions")
                .join(&session_id);
            let recording_path = session_dir.join("recording.jsonl");

            assert!(
                recording_path.exists(),
                "recording.jsonl should exist for granular session"
            );

            let content = tokio::fs::read_to_string(&recording_path).await.unwrap();
            let lines: Vec<&str> = content.lines().collect();
            assert!(
                lines.len() >= 2,
                "Should have header + at least 1 event, got {} lines",
                lines.len()
            );

            let _ = shutdown_handle.send(());
            let _ = server_task.await;
        }

        #[tokio::test]
        async fn test_non_granular_session_has_no_recording_file() {
            use std::time::Duration;

            let tmp = TempDir::new().unwrap();
            let sock_path = tmp.path().join("test.sock");
            let kiln_path = tmp.path().join("kiln");
            std::fs::create_dir_all(&kiln_path).unwrap();

            let server = Server::bind(&sock_path, None).await.unwrap();
            let event_tx = server.event_sender();
            let shutdown_handle = server.shutdown_handle();
            let server_task = tokio::spawn(server.run());

            tokio::time::sleep(Duration::from_millis(50)).await;

            let mut client = UnixStream::connect(&sock_path).await.unwrap();

            let create_req = format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"session.create","params":{{"type":"chat","kiln":"{}"}}}}"#,
                kiln_path.display()
            );
            client.write_all(create_req.as_bytes()).await.unwrap();
            client.write_all(b"\n").await.unwrap();

            let mut buf = vec![0u8; 4096];
            let n = client.read(&mut buf).await.unwrap();
            let response: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();
            let session_id = response["result"]["session_id"]
                .as_str()
                .unwrap()
                .to_string();

            let event = SessionEventMessage::user_message(&session_id, "msg-1", "hello");
            event_tx.send(event).unwrap();

            tokio::time::sleep(Duration::from_millis(300)).await;

            let session_dir = kiln_path
                .join(".crucible")
                .join("sessions")
                .join(&session_id);
            let recording_path = session_dir.join("recording.jsonl");

            assert!(
                !recording_path.exists(),
                "recording.jsonl should NOT exist for non-granular session"
            );

            let _ = shutdown_handle.send(());
            let _ = server_task.await;
        }

        #[tokio::test]
        async fn test_granular_recording_stops_on_session_end() {
            use std::time::Duration;

            let tmp = TempDir::new().unwrap();
            let sock_path = tmp.path().join("test.sock");
            let kiln_path = tmp.path().join("kiln");
            std::fs::create_dir_all(&kiln_path).unwrap();

            let server = Server::bind(&sock_path, None).await.unwrap();
            let event_tx = server.event_sender();
            let shutdown_handle = server.shutdown_handle();
            let server_task = tokio::spawn(server.run());

            tokio::time::sleep(Duration::from_millis(50)).await;

            let mut client = UnixStream::connect(&sock_path).await.unwrap();

            let create_req = format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"session.create","params":{{"type":"chat","kiln":"{}","recording_mode":"granular"}}}}"#,
                kiln_path.display()
            );
            client.write_all(create_req.as_bytes()).await.unwrap();
            client.write_all(b"\n").await.unwrap();

            let mut buf = vec![0u8; 4096];
            let n = client.read(&mut buf).await.unwrap();
            let response: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();
            let session_id = response["result"]["session_id"]
                .as_str()
                .unwrap()
                .to_string();

            let event = SessionEventMessage::text_delta(&session_id, "before end");
            event_tx.send(event).unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;

            // End the session
            let end_req = format!(
                r#"{{"jsonrpc":"2.0","id":2,"method":"session.end","params":{{"session_id":"{}"}}}}"#,
                session_id
            );
            client.write_all(end_req.as_bytes()).await.unwrap();
            client.write_all(b"\n").await.unwrap();

            buf.fill(0);
            let n = client.read(&mut buf).await.unwrap();
            let end_response = String::from_utf8_lossy(&buf[..n]);
            assert!(
                end_response.contains("\"state\":\"ended\""),
                "Session should be ended: {}",
                end_response
            );

            // Wait for writer to flush footer
            tokio::time::sleep(Duration::from_millis(300)).await;

            let session_dir = kiln_path
                .join(".crucible")
                .join("sessions")
                .join(&session_id);
            let recording_path = session_dir.join("recording.jsonl");
            let content = tokio::fs::read_to_string(&recording_path).await.unwrap();
            let lines: Vec<&str> = content.lines().collect();

            // Last line should be footer with total_events
            let last_line = lines.last().unwrap();
            let footer: serde_json::Value = serde_json::from_str(last_line).unwrap();
            assert!(
                footer.get("total_events").is_some(),
                "Footer should have total_events field"
            );

            let _ = shutdown_handle.send(());
            let _ = server_task.await;
        }
    }

    // Tests for resolve_provider_trust_level_for_create
    #[test]
    fn provider_trust_acp_agent_always_cloud() {
        let req: Request = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.create",
            "params": {
                "agent_type": "acp",
                "kiln": "/tmp/kiln"
            }
        }))
        .unwrap();
        // Even with a Local-trust provider in config, ACP always returns Cloud
        let llm_config = Some(build_llm_config_with_trust(
            "local-provider",
            crucible_config::BackendType::Mock,
            Some(crucible_config::TrustLevel::Local),
        ));
        let result = resolve_provider_trust_level_for_create(&req, &llm_config);
        assert_eq!(result, crucible_config::TrustLevel::Cloud);
    }

    #[test]
    fn provider_trust_bare_backend_name_cloud() {
        let req: Request = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.create",
            "params": {
                "provider": "ollama",
                "kiln": "/tmp/kiln"
            }
        }))
        .unwrap();
        let result = resolve_provider_trust_level_for_create(&req, &None);
        assert_eq!(result, crucible_config::TrustLevel::Cloud);
    }

    #[test]
    fn provider_trust_bare_backend_name_local() {
        let req: Request = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.create",
            "params": {
                "provider": "fastembed",
                "kiln": "/tmp/kiln"
            }
        }))
        .unwrap();
        let result = resolve_provider_trust_level_for_create(&req, &None);
        assert_eq!(result, crucible_config::TrustLevel::Local);
    }

    #[test]
    fn provider_trust_default_provider_fallback() {
        // No agent_type, no provider_key, no provider → falls back to default provider in llm_config
        let req: Request = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.create",
            "params": {
                "kiln": "/tmp/kiln"
            }
        }))
        .unwrap();
        // Build config where default provider is Local trust
        let llm_config = Some(build_llm_config_with_trust(
            "my-local",
            crucible_config::BackendType::Mock,
            Some(crucible_config::TrustLevel::Local),
        ));
        let result = resolve_provider_trust_level_for_create(&req, &llm_config);
        assert_eq!(result, crucible_config::TrustLevel::Local);
    }

    // Tests for resolve_kiln_classification_for_create wrapper
    #[test]
    fn kiln_classification_workspace_none_returns_none() {
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln).unwrap();
        // No workspace.toml at kiln dir → returns None (no silent default)
        let result = resolve_kiln_classification_for_create(&kiln, None);
        assert_eq!(result, None);
    }

    #[test]
    fn kiln_classification_relative_path_matches() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("workspace");
        let kiln = workspace.join("notes");
        std::fs::create_dir_all(&kiln).unwrap();
        write_workspace_config(&workspace, "./notes", Some("internal"));
        let result = resolve_kiln_classification_for_create(&kiln, Some(&workspace));
        assert_eq!(result, Some(crucible_config::DataClassification::Internal));
    }

    // --- Golden tests for UTF-8–safe truncation logic ---
    //
    // These capture the current behavior of the truncation pattern used in
    // `handle_grep_request` (the `floor_char_boundary(100)` call). The helper
    // below mirrors that inline logic so we can test it in isolation.

    /// Mirror of the inline truncation logic in `handle_grep_request`.
    fn truncate_utf8_safe(line: &str, max_bytes: usize) -> String {
        if line.len() > max_bytes {
            let end = line.floor_char_boundary(max_bytes);
            format!("{}...", &line[..end])
        } else {
            line.to_string()
        }
    }

    #[test]
    fn truncation_ascii_under_limit() {
        let line = "a".repeat(50);
        let result = truncate_utf8_safe(&line, 100);
        assert_eq!(
            result, line,
            "under-limit ASCII should be returned verbatim"
        );
    }

    #[test]
    fn truncation_ascii_exactly_at_limit() {
        let line = "a".repeat(100);
        let result = truncate_utf8_safe(&line, 100);
        assert_eq!(
            result, line,
            "exactly-at-limit ASCII should be returned verbatim (no trailing ...)"
        );
    }

    #[test]
    fn truncation_ascii_over_limit() {
        let line = "a".repeat(120);
        let result = truncate_utf8_safe(&line, 100);
        assert_eq!(result.len(), 103, "100 chars + 3 for '...'");
        assert!(result.ends_with("..."));
        assert_eq!(&result[..100], &"a".repeat(100));
    }

    #[test]
    fn truncation_multibyte_2byte_boundary() {
        // 'é' is U+00E9 → 2 bytes in UTF-8. Placing it at byte 99-100
        // means the char straddles the boundary. floor_char_boundary(100)
        // should round down to 99 (start of the char).
        let mut line = "a".repeat(99);
        line.push('é'); // bytes 99-100 (total 101)
        let result = truncate_utf8_safe(&line, 100);
        // GOLDEN: captures current behavior — floor rounds to 99
        assert_eq!(&result[..99], &"a".repeat(99));
        assert!(result.ends_with("..."));
        assert_eq!(result.len(), 99 + 3);
    }

    #[test]
    fn truncation_cjk_3byte_boundary() {
        // Each CJK char ('中') is 3 bytes. 33 chars = 99 bytes. 34 chars = 102 bytes.
        let line: String = std::iter::repeat('中').take(34).collect();
        assert_eq!(line.len(), 102);
        let result = truncate_utf8_safe(&line, 100);
        // GOLDEN: captures current behavior — floor rounds 100 down to 99
        // (byte 99 is mid-char), keeping 33 CJK chars (99 bytes).
        let expected_prefix: String = std::iter::repeat('中').take(33).collect();
        assert!(result.starts_with(&expected_prefix));
        assert!(result.ends_with("..."));
        assert_eq!(result.len(), 99 + 3);
    }

    #[test]
    fn truncation_emoji_4byte_boundary() {
        // 🚀 is U+1F680 → 4 bytes in UTF-8.
        // 97 ASCII bytes + 4-byte emoji = 101 bytes total → over limit.
        // floor_char_boundary(100) rounds down to 97 (start of the emoji).
        let mut line = "a".repeat(97);
        line.push('🚀'); // bytes 97-100 (total 101)
        assert_eq!(line.len(), 101);
        let result = truncate_utf8_safe(&line, 100);
        // GOLDEN: captures current behavior — floor rounds to 97
        assert_eq!(&result[..97], &"a".repeat(97));
        assert!(result.ends_with("..."));
        assert_eq!(result.len(), 97 + 3);
    }

    #[test]
    fn truncation_empty_line() {
        let result = truncate_utf8_safe("", 100);
        assert_eq!(result, "", "empty string should be returned verbatim");
    }

    // ── Session Observe Handler Tests ──────────────────────────────────

    /// Create a test session directory with a JSONL file containing sample events.
    fn create_test_session_dir(tmp: &TempDir) -> PathBuf {
        let session_dir = tmp.path().join("chat-20260101-1200-abcd");
        std::fs::create_dir_all(&session_dir).unwrap();
        let jsonl = session_dir.join("session.jsonl");
        let events = vec![
            "{\"type\":\"init\",\"ts\":\"2026-01-01T12:00:00Z\",\"session_id\":\"chat-20260101-1200-abcd\"}",
            "{\"type\":\"user\",\"ts\":\"2026-01-01T12:00:01Z\",\"content\":\"Hello world\"}",
            "{\"type\":\"assistant\",\"ts\":\"2026-01-01T12:00:02Z\",\"content\":\"Hi there!\"}",
        ];
        std::fs::write(&jsonl, events.join("\n") + "\n").unwrap();
        session_dir
    }

    fn make_request(method: &str, params: Value) -> Request {
        serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn session_load_events_returns_events_from_jsonl() {
        let tmp = TempDir::new().unwrap();
        let session_dir = create_test_session_dir(&tmp);

        let req = make_request(
            "session.load_events",
            json!({ "session_dir": session_dir.to_string_lossy().to_string() }),
        );
        let resp = handle_session_load_events(req).await;

        assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
        let result = resp.result.unwrap();
        let events = result.as_array().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["type"], "init");
        assert_eq!(events[1]["type"], "user");
        assert_eq!(events[2]["type"], "assistant");
    }

    #[tokio::test]
    async fn session_load_events_missing_dir_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("nonexistent");

        let req = make_request(
            "session.load_events",
            json!({ "session_dir": missing.to_string_lossy().to_string() }),
        );
        let resp = handle_session_load_events(req).await;

        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let events = result.as_array().unwrap();
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn session_render_markdown_produces_output() {
        let tmp = TempDir::new().unwrap();
        let session_dir = create_test_session_dir(&tmp);

        let req = make_request(
            "session.render_markdown",
            json!({ "session_dir": session_dir.to_string_lossy().to_string() }),
        );
        let resp = handle_session_render_markdown(req).await;

        assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
        let result = resp.result.unwrap();
        let md = result["markdown"].as_str().unwrap();
        assert!(md.contains("Hello world"), "should contain user message");
        assert!(md.contains("Hi there!"), "should contain assistant message");
    }

    #[tokio::test]
    async fn session_export_to_file_writes_markdown() {
        let tmp = TempDir::new().unwrap();
        let session_dir = create_test_session_dir(&tmp);
        let output = tmp.path().join("exported.md");

        let req = make_request(
            "session.export_to_file",
            json!({
                "session_dir": session_dir.to_string_lossy().to_string(),
                "output_path": output.to_string_lossy().to_string(),
            }),
        );
        let resp = handle_session_export_to_file(req).await;

        assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
        let result = resp.result.unwrap();
        assert_eq!(result["status"], "ok");
        assert!(output.exists(), "exported file should exist");
        let content = std::fs::read_to_string(&output).unwrap();
        assert!(content.contains("Hello world"));
    }

    #[tokio::test]
    async fn session_list_persisted_returns_sessions() {
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path().join("kiln");
        let sessions_dir = kiln.join(".crucible").join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let sid = "chat-20260101-1200-abcd";
        let session_dir = sessions_dir.join(sid);
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(
            session_dir.join("session.jsonl"),
            "{\"type\":\"user\",\"ts\":\"2026-01-01T12:00:01Z\",\"content\":\"Test message\"}",
        )
        .unwrap();

        let req = make_request(
            "session.list_persisted",
            json!({ "kiln": kiln.to_string_lossy().to_string() }),
        );
        let resp = handle_session_list_persisted(req).await;

        assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
        let result = resp.result.unwrap();
        assert_eq!(result["total"], 1);
        let sessions = result["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0]["id"], sid);
        assert_eq!(sessions[0]["message_count"], 1);
    }

    #[tokio::test]
    async fn session_list_persisted_empty_kiln_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path().join("empty-kiln");
        std::fs::create_dir_all(&kiln).unwrap();

        let req = make_request(
            "session.list_persisted",
            json!({ "kiln": kiln.to_string_lossy().to_string() }),
        );
        let resp = handle_session_list_persisted(req).await;

        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["total"], 0);
        assert_eq!(result["sessions"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn session_cleanup_dry_run_does_not_delete() {
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path().join("kiln");
        let sessions_dir = kiln.join(".crucible").join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let sid = "chat-20200101-1200-a0b1";
        let session_dir = sessions_dir.join(sid);
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(
            session_dir.join("session.jsonl"),
            "{\"type\":\"user\",\"ts\":\"2020-01-01T12:00:00Z\",\"content\":\"Old message\"}",
        )
        .unwrap();

        let req = make_request(
            "session.cleanup",
            json!({
                "kiln": kiln.to_string_lossy().to_string(),
                "older_than_days": 1,
                "dry_run": true,
            }),
        );
        let resp = handle_session_cleanup(req).await;

        assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
        let result = resp.result.unwrap();
        assert_eq!(result["dry_run"], true);
        assert_eq!(result["total"], 1);
        assert!(session_dir.exists(), "dry run should not delete");
    }

    #[tokio::test]
    async fn session_cleanup_deletes_old_sessions() {
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path().join("kiln");
        let sessions_dir = kiln.join(".crucible").join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let sid = "chat-20200101-1200-a0b2";
        let session_dir = sessions_dir.join(sid);
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(
            session_dir.join("session.jsonl"),
            "{\"type\":\"user\",\"ts\":\"2020-01-01T12:00:00Z\",\"content\":\"Old message\"}",
        )
        .unwrap();

        let req = make_request(
            "session.cleanup",
            json!({
                "kiln": kiln.to_string_lossy().to_string(),
                "older_than_days": 1,
                "dry_run": false,
            }),
        );
        let resp = handle_session_cleanup(req).await;

        assert!(resp.error.is_none(), "unexpected error: {:?}", resp.error);
        let result = resp.result.unwrap();
        assert_eq!(result["dry_run"], false);
        assert_eq!(result["total"], 1);
        assert!(!session_dir.exists(), "old session should be deleted");
    }
}
