//! Unix socket server for JSON-RPC

use crate::agent_manager::{AgentError, AgentManager, AgentManagerParams};
use crate::background_manager::BackgroundJobManager;
use crate::daemon_plugins::DaemonPluginLoader;
use crate::event_emitter::emit_event;
#[cfg(test)]
use crate::event_emitter::stamp_event;
use crate::kiln_manager::KilnManager;
use crate::mcp_server::McpServerManager;
use crate::project_manager::ProjectManager;
use crate::protocol::{
    Request, Response, SessionEventMessage, INTERNAL_ERROR, INVALID_PARAMS, PARSE_ERROR,
};
use crate::recording::RecordingWriter;
use crate::replay::ReplaySession;
use crate::rpc::{RpcContext, RpcDispatcher};
use crate::rpc_helpers::{optional_param, require_param};
use crate::session_manager::SessionManager;
use crate::session_storage::{FileSessionStorage, SessionStorage};
use crate::skills::discovery::{default_discovery_paths, FolderDiscovery};
use crate::tools::workspace::WorkspaceTools;
use anyhow::Result;
use crucible_config::{DataClassification, LlmConfig, TrustLevel};
use crucible_core::events::SessionEvent;
use crucible_core::session::RecordingMode;
use crucible_lua::stubs::StubGenerator;
use crucible_lua::{
    register_crucible_on_api, LuaExecutor, LuaScriptHandlerRegistry, PluginManager,
    ScriptHandlerResult, Session as LuaSession, SessionConfigRpc,
};
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

mod core;
pub mod kiln;
pub mod lua;
pub mod observe;
pub mod platform;
pub mod plugins;
pub mod session;
pub mod storage;

use core::*;
// use kiln::*;  // kiln handlers are now called via crate::server::kiln::
use plugins::*;

/// Daemon server that listens on a Unix socket
pub struct Server {
    listener: UnixListener,
    shutdown_tx: broadcast::Sender<()>,
    kiln_manager: Arc<KilnManager>,
    session_manager: Arc<SessionManager>,
    workspace_tools: Arc<WorkspaceTools>,
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
    #[allow(dead_code)] // web server started externally by crucible-web crate
    web_config: Option<crucible_config::WebConfig>,
    mcp_server_manager: Arc<McpServerManager>,
}

struct NoopSessionRpc;
impl SessionConfigRpc for NoopSessionRpc {}

pub struct LuaSessionState {
    executor: LuaExecutor,
    registry: LuaScriptHandlerRegistry,
}

/// Parameters for binding the server to a Unix socket with plugin configuration.
pub struct BindWithPluginConfigParams {
    pub path: std::path::PathBuf,
    pub mcp_config: Option<crucible_config::McpConfig>,
    pub plugin_config: std::collections::HashMap<String, serde_json::Value>,
    pub plugin_watch: bool,
    pub llm_config: Option<crucible_config::LlmConfig>,
    pub enrichment_config: Option<crucible_config::EmbeddingProviderConfig>,
    pub acp_config: Option<crucible_config::components::acp::AcpConfig>,
    pub permission_config: Option<crucible_config::components::permissions::PermissionConfig>,
    pub web_config: Option<crucible_config::WebConfig>,
}

impl Server {
    /// Bind to a Unix socket path
    #[allow(dead_code)] // convenience constructor used in integration tests
    pub async fn bind(
        path: &Path,
        mcp_config: Option<&crucible_config::McpConfig>,
    ) -> Result<Self> {
        Self::bind_with_plugin_config(BindWithPluginConfigParams {
            path: path.to_path_buf(),
            mcp_config: mcp_config.cloned(),
            plugin_config: std::collections::HashMap::new(),
            plugin_watch: false,
            llm_config: None,
            enrichment_config: None,
            acp_config: None,
            permission_config: None,
            web_config: None,
        })
        .await
    }

    /// Bind to a Unix socket path with plugin configuration
    pub async fn bind_with_plugin_config(params: BindWithPluginConfigParams) -> Result<Self> {
        // Remove stale socket
        if params.path.exists() {
            std::fs::remove_file(&params.path)?;
        }

        // Create parent directory
        if let Some(parent) = params.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&params.path)?;
        let (shutdown_tx, _) = broadcast::channel(1);
        let (event_tx, _) = broadcast::channel(1024);

        use crate::tools::mcp_gateway::McpGatewayManager;
        use tokio::sync::RwLock;

        let mcp_gateway = if let Some(mcp_cfg) = params.mcp_config.as_ref() {
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

        let plugin_loader = Arc::new(Mutex::new(
            match DaemonPluginLoader::new(params.plugin_config.clone()) {
                Ok(loader) => {
                    info!("Daemon plugin loader initialized");
                    Some(loader)
                }
                Err(e) => {
                    warn!("Failed to initialize daemon plugin loader: {}", e);
                    None
                }
            },
        ));

        let kiln_manager = Arc::new(KilnManager::with_event_tx(
            event_tx.clone(),
            params.enrichment_config.clone(),
        ));
        let session_manager = Arc::new(SessionManager::new());
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
        let workspace_root = crucible_config::crucible_home();
        let workspace_tools = Arc::new(WorkspaceTools::new(&workspace_root));
        let agent_manager = Arc::new(AgentManager::new(AgentManagerParams {
            kiln_manager: kiln_manager.clone(),
            session_manager: session_manager.clone(),
            background_manager: background_manager.clone(),
            mcp_gateway,
            llm_config: params.llm_config.clone(),
            acp_config: params.acp_config.clone(),
            permission_config: params.permission_config.clone(),
            plugin_loader: Some(plugin_loader.clone()),
            workspace_tools: Arc::clone(&workspace_tools),
        }));
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
            project_manager.clone(),
            lua_sessions.clone(),
            plugin_loader.clone(),
            params.llm_config.clone(),
            mcp_server_manager.clone(),
        );
        let dispatcher = Arc::new(RpcDispatcher::new(ctx));

        info!("Daemon listening on {:?}", params.path);
        Ok(Self {
            listener,
            shutdown_tx,
            kiln_manager,
            session_manager,
            workspace_tools,
            agent_manager,
            background_manager,
            subscription_manager,
            project_manager,
            lua_sessions,
            event_tx,
            dispatcher,
            plugin_loader,
            plugin_watch: params.plugin_watch,
            llm_config: params.llm_config.clone(),
            mcp_server_manager,
            #[cfg(feature = "web")]
            web_config: params.web_config.clone(),
        })
    }

    /// Get a shutdown sender for external shutdown triggers
    #[allow(dead_code)] // used in integration tests for graceful shutdown
    pub fn shutdown_handle(&self) -> broadcast::Sender<()> {
        self.shutdown_tx.clone()
    }

    /// Get a clone of the event broadcast sender.
    ///
    /// Used to send session events to all subscribed clients.
    #[allow(dead_code)] // used in integration tests for event verification
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

                let tools_api: Arc<dyn crucible_lua::DaemonToolsApi> = Arc::new(
                    crate::tools_bridge::DaemonToolsBridge::new(Arc::clone(&self.workspace_tools)),
                );
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
            // Web server is started by crucible-web crate, not from daemon.
            // The daemon provides the cancel token; crucible-web calls start_server externally.
            cancel
        };

        #[cfg(not(feature = "web"))]
        let _web_cancel = CancellationToken::new();

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
#[allow(dead_code)] // some fields held for Arc ownership; dispatch accesses them via RpcContext
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

#[cfg(test)]
mod tests;
