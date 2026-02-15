//! Unix socket server for JSON-RPC

use crate::agent_manager::AgentManager;
use crate::background_manager::BackgroundJobManager;
use crate::daemon_plugins::DaemonPluginLoader;
use crate::kiln_manager::KilnManager;
use crate::project_manager::ProjectManager;
use crate::protocol::{
    Request, Response, SessionEventMessage, INTERNAL_ERROR, INVALID_PARAMS, METHOD_NOT_FOUND,
    PARSE_ERROR,
};
use crate::rpc::{RpcContext, RpcDispatcher};
use crate::rpc_helpers::{
    optional_i64_param, optional_str_param, optional_u64_param, require_array_param,
    require_f64_param, require_obj_param, require_str_param,
};
use crate::session_manager::SessionManager;
use crate::session_storage::{FileSessionStorage, SessionStorage};
use anyhow::Result;

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
    event_tx: broadcast::Sender<SessionEventMessage>,
    dispatcher: Arc<RpcDispatcher>,
    plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
    #[cfg(feature = "web")]
    web_config: Option<crucible_config::WebConfig>,
}

impl Server {
    /// Bind to a Unix socket path
    pub async fn bind(
        path: &Path,
        mcp_config: Option<&crucible_config::McpConfig>,
    ) -> Result<Self> {
        Self::bind_with_plugin_config(
            path,
            mcp_config,
            std::collections::HashMap::new(),
            crucible_config::ProvidersConfig::default(),
            None,
            None,
        )
        .await
    }

    /// Bind to a Unix socket path with plugin configuration
    pub async fn bind_with_plugin_config(
        path: &Path,
        mcp_config: Option<&crucible_config::McpConfig>,
        plugin_config: std::collections::HashMap<String, serde_json::Value>,
        providers_config: crucible_config::ProvidersConfig,
        llm_config: Option<crucible_config::LlmConfig>,
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

        let kiln_manager = Arc::new(KilnManager::with_event_tx(event_tx.clone()));
        let session_manager = Arc::new(SessionManager::new());
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
        let agent_manager = Arc::new(AgentManager::new(
            session_manager.clone(),
            background_manager.clone(),
            mcp_gateway,
            providers_config,
            llm_config,
        ));
        let subscription_manager = Arc::new(SubscriptionManager::new());
        let project_manager = Arc::new(ProjectManager::new(
            crucible_config::crucible_home().join("projects.json"),
        ));

        let ctx = RpcContext::new(
            kiln_manager.clone(),
            session_manager.clone(),
            agent_manager.clone(),
            subscription_manager.clone(),
            event_tx.clone(),
            shutdown_tx.clone(),
        );
        let dispatcher = Arc::new(RpcDispatcher::new(ctx));

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
            event_tx,
            dispatcher,
            plugin_loader,
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
            fn should_persist(event: &SessionEventMessage) -> bool {
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
                storage: &FileSessionStorage,
            ) {
                if !should_persist(event) {
                    return;
                }
                if let Some(session) = sm.get_session(&event.session_id) {
                    // Persist JSONL event
                    if let Ok(json) = serde_json::to_string(event) {
                        let _ = storage.append_event(&session, &json).await;
                    }

                    // Persist markdown for user/assistant messages
                    match event.event.as_str() {
                        "user_message" => {
                            if let Some(content) =
                                event.data.get("content").and_then(|v| v.as_str())
                            {
                                let _ = storage.append_markdown(&session, "User", content).await;
                            }
                        }
                        "message_complete" => {
                            if let Some(content) =
                                event.data.get("full_response").and_then(|v| v.as_str())
                            {
                                let _ = storage
                                    .append_markdown(&session, "Assistant", content)
                                    .await;
                            }
                        }
                        _ => {}
                    }
                }
            }

            loop {
                tokio::select! {
                    biased;
                    _ = persist_cancel_clone.cancelled() => {
                        debug!("Persist task received shutdown signal, draining remaining events");
                        while let Ok(event) = persist_rx.try_recv() {
                            persist_event(&event, &sm_clone, &storage).await;
                        }
                        break;
                    }
                    result = persist_rx.recv() => {
                        match result {
                            Ok(event) => {
                                persist_event(&event, &sm_clone, &storage).await;
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
                            let dispatcher = self.dispatcher.clone();
                            let km = self.kiln_manager.clone();
                            let sm = self.session_manager.clone();
                            let am = self.agent_manager.clone();
                            let sub_m = self.subscription_manager.clone();
                            let pm = self.project_manager.clone();
                            let event_tx = self.event_tx.clone();
                            let event_rx = self.event_tx.subscribe();
                            let pl = self.plugin_loader.clone();
                            tokio::spawn(async move {
                                if let Err(e) = handle_client(stream, dispatcher, km, sm, am, sub_m, pm, event_tx, event_rx, pl).await {
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

#[allow(clippy::too_many_arguments)]
async fn handle_client(
    stream: UnixStream,
    dispatcher: Arc<RpcDispatcher>,
    kiln_manager: Arc<KilnManager>,
    session_manager: Arc<SessionManager>,
    agent_manager: Arc<AgentManager>,
    subscription_manager: Arc<SubscriptionManager>,
    project_manager: Arc<ProjectManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    mut event_rx: broadcast::Receiver<SessionEventMessage>,
    plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Result<()> {
    let client_id = ClientId::new();
    let (reader, writer) = stream.into_split();
    let writer: Arc<Mutex<OwnedWriteHalf>> = Arc::new(Mutex::new(writer));
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    let writer_clone = writer.clone();
    let sub_manager = subscription_manager.clone();
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
            Ok(req) => {
                handle_request(
                    req,
                    client_id,
                    &dispatcher,
                    &kiln_manager,
                    &session_manager,
                    &agent_manager,
                    &subscription_manager,
                    &project_manager,
                    &event_tx,
                    &plugin_loader,
                )
                .await
            }
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
    subscription_manager.remove_client(client_id);

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_request(
    req: Request,
    client_id: ClientId,
    dispatcher: &Arc<RpcDispatcher>,
    kiln_manager: &Arc<KilnManager>,
    session_manager: &Arc<SessionManager>,
    agent_manager: &Arc<AgentManager>,
    _subscription_manager: &Arc<SubscriptionManager>,
    project_manager: &Arc<ProjectManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    let req_clone = req.clone();
    let resp = dispatcher.dispatch(client_id, req).await;

    if let Some(ref err) = resp.error {
        if err.code == METHOD_NOT_FOUND && err.message.contains("not yet migrated") {
            return handle_legacy_request(
                req_clone,
                kiln_manager,
                session_manager,
                agent_manager,
                project_manager,
                event_tx,
                plugin_loader,
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
    event_tx: &broadcast::Sender<SessionEventMessage>,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    tracing::debug!("Legacy handler for method={:?}", req.method);

    match req.method.as_str() {
        "kiln.open" => handle_kiln_open(req, kiln_manager, plugin_loader).await,
        "kiln.close" => handle_kiln_close(req, kiln_manager).await,
        "kiln.list" => handle_kiln_list(req, kiln_manager).await,
        "search_vectors" => handle_search_vectors(req, kiln_manager).await,
        "list_notes" => handle_list_notes(req, kiln_manager).await,
        "get_note_by_name" => handle_get_note_by_name(req, kiln_manager).await,
        "note.upsert" => handle_note_upsert(req, kiln_manager).await,
        "note.get" => handle_note_get(req, kiln_manager).await,
        "note.delete" => handle_note_delete(req, kiln_manager).await,
        "note.list" => handle_note_list(req, kiln_manager).await,
        "process_file" => handle_process_file(req, kiln_manager).await,
        "process_batch" => handle_process_batch(req, kiln_manager).await,
        "session.create" => handle_session_create(req, session_manager, project_manager).await,
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
        "plugin.reload" => handle_plugin_reload(req, plugin_loader).await,
        "plugin.list" => handle_plugin_list(req, plugin_loader).await,
        "project.register" => handle_project_register(req, project_manager).await,
        "project.unregister" => handle_project_unregister(req, project_manager).await,
        "project.list" => handle_project_list(req, project_manager).await,
        "project.get" => handle_project_get(req, project_manager).await,
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
) -> Response {
    let path = require_str_param!(req, "path");
    let kiln_path = Path::new(path);

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

    Response::success(req.id, serde_json::json!({"status": "ok"}))
}

async fn handle_kiln_close(req: Request, km: &Arc<KilnManager>) -> Response {
    let path = require_str_param!(req, "path");

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

async fn handle_search_vectors(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_str_param!(req, "kiln");
    let vector_arr = require_array_param!(req, "vector");
    let vector: Vec<f32> = vector_arr
        .iter()
        .filter_map(|v: &serde_json::Value| v.as_f64().map(|f| f as f32))
        .collect();
    let limit = optional_u64_param!(req, "limit").unwrap_or(20) as usize;

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
    let kiln_path = require_str_param!(req, "kiln");
    let path_filter = optional_str_param!(req, "path_filter");

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
    let kiln_path = require_str_param!(req, "kiln");
    let name = require_str_param!(req, "name");

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
    use crucible_core::storage::{NoteRecord, NoteStore};

    let kiln_path = require_str_param!(req, "kiln");

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
    use crucible_core::storage::NoteStore;

    let kiln_path = require_str_param!(req, "kiln");
    let path = require_str_param!(req, "path");

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
    use crucible_core::storage::NoteStore;

    let kiln_path = require_str_param!(req, "kiln");
    let path = require_str_param!(req, "path");

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
    use crucible_core::storage::NoteStore;

    let kiln_path = require_str_param!(req, "kiln");

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
    let kiln_path = require_str_param!(req, "kiln");
    let file_path = require_str_param!(req, "path");

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

async fn handle_process_batch(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_str_param!(req, "kiln");
    let paths_arr = require_array_param!(req, "paths");
    let paths: Vec<std::path::PathBuf> = paths_arr
        .iter()
        .filter_map(|v: &serde_json::Value| v.as_str().map(std::path::PathBuf::from))
        .collect();

    match km.process_batch(Path::new(kiln_path), &paths).await {
        Ok((processed, skipped, errors)) => Response::success(
            req.id,
            serde_json::json!({
                "processed": processed,
                "skipped": skipped,
                "errors": errors.iter().map(|(p, _)| {
                    serde_json::json!({
                        "path": p.to_string_lossy(),
                        "error": "processing failed"
                    })
                }).collect::<Vec<_>>()
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Session RPC handlers
// ─────────────────────────────────────────────────────────────────────────────

use crucible_core::session::{SessionState, SessionType};

async fn handle_session_create(
    req: Request,
    sm: &Arc<SessionManager>,
    pm: &Arc<ProjectManager>,
) -> Response {
    let session_type_str = optional_str_param!(req, "type").unwrap_or("chat");
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

    let kiln = optional_str_param!(req, "kiln")
        .map(PathBuf::from)
        .unwrap_or_else(crucible_config::crucible_home);

    let workspace = optional_str_param!(req, "workspace").map(PathBuf::from);

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

    let project_path = workspace.as_ref().unwrap_or(&kiln);
    if let Err(e) = pm.register_if_missing(project_path) {
        tracing::warn!(path = %project_path.display(), error = %e, "Failed to auto-register project");
    }

    match sm
        .create_session(session_type, kiln, workspace, connected_kilns)
        .await
    {
        Ok(session) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session.id,
                "type": session.session_type.as_prefix(),
                "kiln": session.kiln,
                "workspace": session.workspace,
                "state": format!("{}", session.state),
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_list(req: Request, sm: &Arc<SessionManager>) -> Response {
    // Parse optional filters
    let kiln = optional_str_param!(req, "kiln").map(PathBuf::from);
    let workspace = optional_str_param!(req, "workspace").map(PathBuf::from);
    let session_type = optional_str_param!(req, "type").and_then(|s| match s {
        "chat" => Some(SessionType::Chat),
        "agent" => Some(SessionType::Agent),
        "workflow" => Some(SessionType::Workflow),
        _ => None,
    });
    let state = optional_str_param!(req, "state").and_then(|s| match s {
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

async fn handle_session_get(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

    match sm.get_session(session_id) {
        Some(session) => Response::success(
            req.id,
            serde_json::json!({
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
            }),
        ),
        None => Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Session not found: {}", session_id),
        ),
    }
}

async fn handle_session_pause(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

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
    let session_id = require_str_param!(req, "session_id");

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
    let session_id = require_str_param!(req, "session_id");
    let kiln = PathBuf::from(require_str_param!(req, "kiln"));

    // Optional pagination params
    let limit = optional_u64_param!(req, "limit").map(|n| n as usize);
    let offset = optional_u64_param!(req, "offset").map(|n| n as usize);

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
    let session_id = require_str_param!(req, "session_id");

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

async fn handle_session_compact(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

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
    let session_id = require_str_param!(req, "session_id");

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
    let session_id = require_str_param!(req, "session_id");
    let content = require_str_param!(req, "content");

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
    let session_id = require_str_param!(req, "session_id");

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
    let session_id = require_str_param!(req, "session_id");
    let request_id = require_str_param!(req, "request_id");
    let response_obj = require_obj_param!(req, "response");

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

    let _ = event_tx.send(SessionEventMessage::new(
        session_id,
        "interaction_completed",
        serde_json::json!({
            "request_id": request_id,
            "response": response,
        }),
    ));

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
    let session_id = require_str_param!(req, "session_id");

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

    let _ = event_tx.send(SessionEventMessage::new(
        session_id.to_string(),
        "interaction_requested",
        serde_json::json!({
            "request_id": request_id,
            "request": request,
        }),
    ));

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
    let session_id = require_str_param!(req, "session_id");
    let model_id = require_str_param!(req, "model_id");

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
    let session_id = require_str_param!(req, "session_id");

    match am.list_models(session_id).await {
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

async fn handle_session_set_thinking_budget(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_str_param!(req, "session_id");
    let budget = optional_i64_param!(req, "thinking_budget");

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
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::ConcurrentRequest(id)) => {
            concurrent_request(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_get_thinking_budget(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

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

async fn handle_session_add_notification(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_str_param!(req, "session_id");
    let notification_obj = require_obj_param!(req, "notification");

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
    let session_id = require_str_param!(req, "session_id");

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
    let session_id = require_str_param!(req, "session_id");
    let notification_id = require_str_param!(req, "notification_id");

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
    let session_id = require_str_param!(req, "session_id");
    let temperature = require_f64_param!(req, "temperature");

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
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::ConcurrentRequest(id)) => {
            concurrent_request(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_get_temperature(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

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
    let session_id = require_str_param!(req, "session_id");
    // max_tokens can be null to clear the limit, so we use optional
    let max_tokens = optional_u64_param!(req, "max_tokens").map(|v| v as u32);

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
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::ConcurrentRequest(id)) => {
            concurrent_request(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_get_max_tokens(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

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
    let name = require_str_param!(req, "name");

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
    let path = require_str_param!(req, "path");

    match pm.register(Path::new(path)) {
        Ok(project) => match serde_json::to_value(project) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
        },
        Err(e) => Response::error(req.id, INVALID_PARAMS, e.to_string()),
    }
}

async fn handle_project_unregister(req: Request, pm: &Arc<ProjectManager>) -> Response {
    let path = require_str_param!(req, "path");

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
    let path = require_str_param!(req, "path");

    match pm.get(Path::new(path)) {
        Some(project) => match serde_json::to_value(project) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
        },
        None => Response::success(req.id, serde_json::Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

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
}
