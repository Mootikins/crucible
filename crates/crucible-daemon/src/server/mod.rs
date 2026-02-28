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

mod core;
mod kiln;
mod lua;
mod observe;
mod platform;
mod plugins;
mod session;
mod storage;

use core::*;
use kiln::*;
use lua::*;
use observe::*;
use platform::*;
use plugins::*;
use session::*;
use storage::*;

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
impl SessionConfigRpc for NoopSessionRpc {}

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
        enrichment_config: Option<crucible_config::EmbeddingProviderConfig>,
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

        let kiln_manager = Arc::new(KilnManager::with_event_tx(
            event_tx.clone(),
            enrichment_config.clone(),
        ));
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
        use crucible_core::storage::NoteRecord;
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
