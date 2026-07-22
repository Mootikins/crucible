//! Unix socket server for JSON-RPC

use crate::agent_manager::{AgentError, AgentManager, AgentManagerParams, MODEL_CACHE_TTL};
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
use chrono::Utc;
use crucible_core::config::{DataClassification, LlmConfig, TrustLevel};
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
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

mod core;
pub mod fs;
pub mod kiln;
mod socket_lock;
use socket_lock::acquire_socket_lock;
pub mod lua;
pub mod note_refactor;
pub mod observe;
pub mod platform;
pub mod plugins;
pub mod session;
pub mod storage;

use core::*;
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
    runtimepath: Vec<std::path::PathBuf>,
    plugin_watch: bool,
    auto_archive_hours: Option<u64>,
    llm_config: Option<LlmConfig>,
    schedules: Vec<crucible_core::config::ScheduleEntry>,
    /// Resolved daemon data root (see `BindWithPluginConfigParams::data_home`);
    /// `run()`'s open-kilns/archive-sweep read this instead of `crucible_home()`.
    data_home: std::path::PathBuf,
    #[cfg(feature = "web")]
    #[allow(dead_code)] // web server started externally by crucible-web crate
    web_config: Option<crucible_core::config::WebConfig>,
    mcp_server_manager: Arc<McpServerManager>,
    /// Held open for the daemon's lifetime; the flock on it enforces that only
    /// one daemon binds this socket. Dropped (unlocked) when the Server drops.
    #[allow(dead_code)]
    socket_lock: Option<std::fs::File>,
}

struct NoopSessionRpc;
impl SessionConfigRpc for NoopSessionRpc {}

pub struct LuaSessionState {
    pub(crate) executor: LuaExecutor,
    pub(crate) registry: LuaScriptHandlerRegistry,
    /// Set to `true` after `on_session_end` hooks fire for this session.
    ///
    /// Both `session.end` and `lua.shutdown_session` try to fire
    /// `on_session_end` hooks — the CLI chat REPL invokes both for the
    /// same session lifecycle. Without this guard, non-idempotent hooks
    /// (LLM calls, file writes) would run twice.
    ///
    /// The daemon enforces a single fire per session; plugins do NOT
    /// need to be idempotent.
    pub(crate) end_hooks_fired: bool,
}

/// Parameters for binding the server to a Unix socket with plugin configuration.
pub struct BindWithPluginConfigParams {
    pub path: std::path::PathBuf,
    pub mcp_config: Option<crucible_core::config::McpConfig>,
    pub plugin_config: std::collections::HashMap<String, serde_json::Value>,
    pub runtimepath: Vec<std::path::PathBuf>,
    pub plugin_watch: bool,
    pub auto_archive_hours: Option<u64>,
    pub llm_config: Option<crucible_core::config::LlmConfig>,
    pub enrichment_config: Option<crucible_core::config::EmbeddingProviderConfig>,
    pub max_precognition_chars: usize,
    pub acp_config: Option<crucible_core::config::components::acp::AcpConfig>,
    pub permission_config: Option<crucible_core::config::components::permissions::PermissionConfig>,
    pub web_config: Option<crucible_core::config::WebConfig>,
    pub schedules: Vec<crucible_core::config::ScheduleEntry>,
    /// Full loaded app config as JSON — seeds the Lua `cru.config` store
    /// before init.lua runs (TOML seeds, Lua overrides, RPC merges).
    pub app_config: Option<serde_json::Value>,
    /// Daemon data root — registry (`projects.json`), default session storage,
    /// the home kiln, logs. `None` resolves to `crucible_home()` (the
    /// `$CRUCIBLE_HOME`/`~/.crucible` default). Injected as a TempDir in tests so
    /// the in-process daemon never reads the developer's real `~/.crucible`.
    pub data_home: Option<std::path::PathBuf>,
}

impl Server {
    /// Bind to a Unix socket path
    #[allow(dead_code)] // convenience constructor used in integration tests
    pub async fn bind(
        path: &Path,
        mcp_config: Option<&crucible_core::config::McpConfig>,
    ) -> Result<Self> {
        Self::bind_with_plugin_config(BindWithPluginConfigParams {
            path: path.to_path_buf(),
            mcp_config: mcp_config.cloned(),
            plugin_config: std::collections::HashMap::new(),
            runtimepath: Vec::new(),
            plugin_watch: false,
            auto_archive_hours: None,
            llm_config: None,
            enrichment_config: None,
            max_precognition_chars: crucible_core::config::default_max_precognition_chars(),
            acp_config: None,
            permission_config: None,
            web_config: None,
            schedules: Vec::new(),
            app_config: None,
            data_home: None,
        })
        .await
    }

    /// Test constructor: bind with an isolated data root injected as a value
    /// (no `CRUCIBLE_HOME` env mutation). The daemon reads registry, sessions,
    /// and the home kiln from `data_home` instead of the developer's real
    /// `~/.crucible`.
    ///
    /// CAVEAT: this injects the *value* threaded through `Server`/`RpcContext`,
    /// but it does NOT change the process-global `crucible_home()` that
    /// `is_crucible_home()`/`FileSessionStorage::sessions_base()` still read. So
    /// the injected home is treated as a *regular* kiln: sessions created under
    /// it land at `{data_home}/.crucible/sessions`, whereas production (where
    /// `data_home == crucible_home()`) uses the no-prefix `{home}/sessions`. A
    /// test that seeds a session into the injected home kiln and expects the
    /// production layout must instead pin `CRUCIBLE_HOME` via `EnvVarGuard` (see
    /// the `session_storage` home-detection tests). Untangling that global is a
    /// separate follow-up.
    #[allow(dead_code)] // used by in-process integration-test fixtures
    pub async fn bind_with_data_home(path: &Path, data_home: std::path::PathBuf) -> Result<Self> {
        Self::bind_with_plugin_config(BindWithPluginConfigParams {
            path: path.to_path_buf(),
            mcp_config: None,
            plugin_config: std::collections::HashMap::new(),
            runtimepath: Vec::new(),
            plugin_watch: false,
            auto_archive_hours: None,
            llm_config: None,
            enrichment_config: None,
            max_precognition_chars: crucible_core::config::default_max_precognition_chars(),
            acp_config: None,
            permission_config: None,
            web_config: None,
            schedules: Vec::new(),
            app_config: None,
            data_home: Some(data_home),
        })
        .await
    }

    /// Bind to a Unix socket path with plugin configuration
    pub async fn bind_with_plugin_config(params: BindWithPluginConfigParams) -> Result<Self> {
        // Create the socket's parent dir first (needed for both the lock and
        // the socket itself).
        if let Some(parent) = params.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Exclusive advisory lock: exactly one daemon owns this socket. Acquired
        // BEFORE unlinking the stale socket, so two daemons racing to start can't
        // both unlink+bind (the TOCTOU that orphaned a live daemon and let a
        // second open the same storage). Held for the daemon's lifetime — the
        // File drops with Server, releasing the flock.
        let socket_lock = acquire_socket_lock(&params.path)?;

        // Safe to reclaim the stale socket now that we hold the lock. Single
        // removal path (crucible_core::protocol::remove_socket) — it ignores a
        // missing file.
        crucible_core::protocol::remove_socket(&params.path);

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

        // Seed the Lua app-config store BEFORE init.lua runs so plugin code
        // (and later `cru.config.set` / `config.set` merges) layer on top of
        // the TOML-loaded values.
        if let Some(app_config) = params.app_config.clone() {
            crucible_lua::seed_app_config(app_config);
        }

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
            params.max_precognition_chars,
        ));
        let session_manager = Arc::new(SessionManager::new());
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
        // Resolve the daemon data root ONCE. Every crucible_home() read below and
        // in the runtime handlers (session list, archive sweep) now goes through
        // this value instead of calling the global; `None` keeps the
        // crucible_home() default so production behavior is unchanged, while tests
        // inject a TempDir (no env mutation).
        let data_home = params
            .data_home
            .clone()
            .unwrap_or_else(crucible_core::config::crucible_home);
        let workspace_tools = Arc::new(WorkspaceTools::new(&data_home));
        let delegation_service =
            crate::delegation::DelegationService::new(session_manager.clone(), event_tx.clone());
        let agent_manager = Arc::new(AgentManager::new_with_delegation(
            AgentManagerParams {
                kiln_manager: kiln_manager.clone(),
                session_manager: session_manager.clone(),
                background_manager: background_manager.clone(),
                mcp_gateway,
                llm_config: params.llm_config.clone(),
                acp_config: params.acp_config.clone(),
                permission_config: params.permission_config.clone(),
                plugin_loader: Some(plugin_loader.clone()),
                workspace_tools: Arc::clone(&workspace_tools),
            },
            delegation_service.clone(),
        ));
        delegation_service.bind_agent_manager(&agent_manager);
        let subscription_manager = Arc::new(SubscriptionManager::new());
        let project_manager = Arc::new(ProjectManager::new(data_home.join("projects.json")));
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
            params.mcp_config.clone(),
            data_home.clone(),
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
            runtimepath: params.runtimepath,
            plugin_watch: params.plugin_watch,
            auto_archive_hours: params.auto_archive_hours,
            llm_config: params.llm_config.clone(),
            schedules: params.schedules,
            data_home,
            mcp_server_manager,
            socket_lock,
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

                // Hand the validator registry + plugin Lua handle to the
                // agent manager so the stream loop can dispatch
                // `OutputValidation::Lua { name }` against plugin-registered
                // validators. Bind once; `set_lua_validators` is idempotent.
                self.agent_manager
                    .set_lua_validators(loader.validator_registry(), loader.plugin_lua());

                let tools_api: Arc<dyn crucible_lua::DaemonToolsApi> = Arc::new(
                    crate::tools_bridge::DaemonToolsBridge::new(Arc::clone(&self.workspace_tools)),
                );
                if let Err(e) = loader.upgrade_with_tools(tools_api) {
                    warn!("Failed to upgrade Lua tools module: {}", e);
                }

                // Bootstrap declared plugins from plugins.toml before discovery
                let plugins_toml =
                    dirs::config_dir().map(|d| d.join("crucible").join("plugins.toml"));
                if let Some(ref path) = plugins_toml {
                    if path.exists() {
                        match std::fs::read_to_string(path) {
                            Ok(content) => {
                                match toml::from_str::<crucible_core::config::PluginsConfig>(
                                    &content,
                                ) {
                                    Ok(config) => {
                                        if let Err(e) =
                                            crate::daemon_plugins::bootstrap_plugins(&config.plugin)
                                                .await
                                        {
                                            warn!("Plugin bootstrap error: {}", e);
                                        }
                                    }
                                    Err(e) => warn!("Failed to parse {}: {}", path.display(), e),
                                }
                            }
                            Err(e) => warn!("Failed to read {}: {}", path.display(), e),
                        }
                    }
                }

                let paths = crate::daemon_plugins::daemon_plugin_paths(&self.runtimepath);
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

                // Auto-generate LuaCATS stubs for IDE support
                if let Some(config_dir) = dirs::config_dir() {
                    let stubs_dir = config_dir.join("crucible").join("luals");
                    match loader.generate_stubs(&stubs_dir) {
                        Ok(()) => debug!("Generated LuaCATS stubs at {}", stubs_dir.display()),
                        Err(e) => debug!("LuaCATS stub generation skipped: {}", e),
                    }
                }

                // Register declarative schedules from config
                for schedule in &self.schedules {
                    if !schedule.enabled {
                        continue;
                    }
                    let secs = match crucible_core::config::parse_duration_string(&schedule.every) {
                        Some(d) if d.as_secs() > 0 => d.as_secs(),
                        Some(_) => {
                            warn!(
                                "Schedule '{}': interval must be positive (got '{}')",
                                schedule.name, schedule.every
                            );
                            continue;
                        }
                        None => {
                            warn!(
                                "Schedule '{}': invalid interval '{}'",
                                schedule.name, schedule.every
                            );
                            continue;
                        }
                    };
                    let action = schedule
                        .action
                        .strip_prefix("lua:")
                        .unwrap_or(&schedule.action);
                    let code = format!(
                        "cru.schedule({{ every = {} }}, function() {} end)",
                        secs, action
                    );
                    match loader.eval(&code).await {
                        Ok(_) => {
                            info!(
                                "Registered schedule '{}' (every {})",
                                schedule.name, schedule.every
                            );
                        }
                        Err(e) => {
                            warn!("Failed to register schedule '{}': {}", schedule.name, e);
                        }
                    }
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

        // Warm model cache on startup and refresh periodically
        {
            let am = self.agent_manager.clone();
            tokio::spawn(async move {
                am.warm_model_cache().await;
                let mut interval = tokio::time::interval(MODEL_CACHE_TTL);
                interval.tick().await; // skip immediate tick (just warmed)
                loop {
                    interval.tick().await;
                    am.warm_model_cache().await;
                }
            });
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
            let last_persist_times: DashMap<String, Instant> = DashMap::new();
            let persist_debounce_interval = Duration::from_secs(30);
            loop {
                tokio::select! {
                                    biased;
                                    _ = persist_cancel_clone.cancelled() => {
                                        debug!("Persist task received shutdown signal, draining remaining events");
                                        while let Ok(event) = persist_rx.try_recv() {
                                            forward_to_recording(&sm_clone, &event);
                                            if let Err(e) = sm_clone.update_last_activity(&event.session_id, Utc::now()).await {
                                                if !matches!(e, crate::session_manager::SessionError::NotFound(_)) {
                                                    warn!(session_id = %event.session_id, error = %e, "Failed to update last activity during shutdown drain");
                                                }
                                            }
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

                                                // Determine if this is a terminal event that should always persist
                                                let is_terminal_event = matches!(
                                                    event.event.as_str(),
                                                    "session_end" | "session_error" | "session_start"
                                                );

                                                // Check if we should persist last_activity
                                                let should_persist = if is_terminal_event {
                                                    true
                                                } else {
                                                    // Check if 30 seconds have passed since last persist for this session
                                                    match last_persist_times.get(&event.session_id) {
                                                        Some(last_time) => {
                                                            Instant::now().duration_since(*last_time) >= persist_debounce_interval
                                                        }
                                                        None => true, // First event for this session
                                                    }
                                                };

                                                if should_persist {
                                                    if let Err(e) = sm_clone.update_last_activity(&event.session_id, Utc::now()).await {
                                                        if !matches!(e, crate::session_manager::SessionError::NotFound(_)) {
                                                            warn!(session_id = %event.session_id, error = %e, "Failed to update last activity");
                                                        }
                                                    }
                                                    last_persist_times.insert(event.session_id.clone(), Instant::now());
                                                }

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

        let sweep_session_manager = self.session_manager.clone();
        let sweep_kiln_manager = self.kiln_manager.clone();
        let sweep_subscription_manager = self.subscription_manager.clone();
        let sweep_agent_manager = self.agent_manager.clone();
        let sweep_cancel = CancellationToken::new();
        let sweep_cancel_clone = sweep_cancel.clone();
        let auto_archive_hours = self.auto_archive_hours.unwrap_or(72);
        let sweep_data_home = self.data_home.clone();

        let archive_sweep_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30 * 60));
            loop {
                tokio::select! {
                    biased;
                    _ = sweep_cancel_clone.cancelled() => break,
                    _ = interval.tick() => {
                        match sweep_and_archive_stale_sessions(
                            &sweep_session_manager,
                            &sweep_kiln_manager,
                            &sweep_subscription_manager,
                            &sweep_agent_manager,
                            auto_archive_hours,
                            &sweep_data_home,
                        ).await {
                            Ok(archived) if archived > 0 => {
                                info!(archived, auto_archive_hours, "Auto-archived stale sessions");
                            }
                            Ok(_) => {}
                            Err(e) => {
                                warn!(error = %e, "Auto-archive sweep failed");
                            }
                        }
                    }
                }
            }
        });

        // Auto-title task: when a turn completes in a still-untitled session,
        // generate a topic-based title daemon-side so every client (TUI, web,
        // ACP) gets titled sessions without asking for it.
        let title_sm = self.session_manager.clone();
        let title_am = self.agent_manager.clone();
        let title_event_tx = self.event_tx.clone();
        let mut title_rx = self.event_tx.subscribe();
        let title_cancel = CancellationToken::new();
        let title_cancel_clone = title_cancel.clone();

        let auto_title_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = title_cancel_clone.cancelled() => break,
                    result = title_rx.recv() => {
                        match result {
                            Ok(event) if event.event == "message_complete" => {
                                let untitled = title_sm
                                    .get_session(&event.session_id)
                                    .map(|s| s.title.as_deref().is_none_or(|t| t.trim().is_empty()))
                                    .unwrap_or(false);
                                if untitled {
                                    let am = title_am.clone();
                                    let tx = title_event_tx.clone();
                                    let session_id = event.session_id.clone();
                                    tokio::spawn(async move {
                                        if let Err(e) =
                                            am.generate_session_title(&session_id, &tx).await
                                        {
                                            debug!(
                                                session_id = %session_id,
                                                error = %e,
                                                "Auto-title generation skipped"
                                            );
                                        }
                                    });
                                }
                            }
                            Ok(_) => {}
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!("Auto-title task lagged, dropped {} events", n);
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }
        });

        // Startup: open the registered project kilns (+ crucible home) so a
        // client that never runs `cru chat` — e.g. `cru web` on a fresh
        // daemon — can still resolve notes. Note-open and other file APIs
        // gate on the daemon's OPEN-kiln set (find_enclosing_kiln); with an
        // empty set every note-open 404s ("File not within any open kiln").
        // The same list then feeds the title catch-up sweep (persisted
        // sessions with content but no title). Project kiln entries may point
        // at the `.crucible` data dir — normalize to the kiln root.
        {
            let sm = self.session_manager.clone();
            let km = self.kiln_manager.clone();
            let pm = self.project_manager.clone();
            let tx = self.event_tx.clone();

            // Kilns to OPEN: already-open kilns + registered project kiln roots.
            // Deliberately NOT ~/.crucible — opening the config dir as a kiln
            // leaked it into km.list()/`/api/kilns` forever and spun a watcher
            // over it. Project entries may point at the `.crucible` data dir;
            // normalize to the kiln root.
            let mut open_kilns: Vec<std::path::PathBuf> = km
                .list()
                .await
                .into_iter()
                .map(|(path, _, _)| path)
                .collect();
            for project in pm.list() {
                for kiln in project.kilns {
                    let root = if kiln.path.file_name().is_some_and(|n| n == ".crucible") {
                        kiln.path.parent().map(|p| p.to_path_buf())
                    } else {
                        Some(kiln.path)
                    };
                    if let Some(root) = root {
                        if !open_kilns.contains(&root) {
                            open_kilns.push(root);
                        }
                    }
                }
            }

            // Title catch-up scans the open kilns PLUS ~/.crucible (legacy
            // sessions live at ~/.crucible/sessions) but never OPENS home as a
            // kiln — that is the leak this split fixes.
            let mut sweep_kilns = open_kilns.clone();
            let home = self.data_home.clone();
            if !sweep_kilns.contains(&home) {
                sweep_kilns.push(home);
            }

            tokio::spawn(async move {
                // Open registered project kilns (idempotent) so note-open can
                // resolve them; a failure to open one must not block the others
                // or the title sweep.
                let mut opened = 0;
                for kiln in &open_kilns {
                    match km.open(kiln).await {
                        Ok(()) => opened += 1,
                        Err(e) => {
                            warn!(kiln = %kiln.display(), error = %e, "Startup kiln open failed")
                        }
                    }
                }
                if opened > 0 {
                    info!(opened, "Opened registered kilns on startup");
                }

                let titled = sm.title_untitled_sessions(&sweep_kilns, &tx).await;
                if titled > 0 {
                    info!(titled, "Startup title catch-up completed");
                }
            });
        }

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
        sweep_cancel.cancel();
        title_cancel.cancel();
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
        match tokio::time::timeout(std::time::Duration::from_secs(5), archive_sweep_task).await {
            Ok(Ok(())) => debug!("Auto-archive sweep task completed gracefully"),
            Ok(Err(e)) => warn!("Auto-archive sweep task panicked: {}", e),
            Err(_) => warn!("Auto-archive sweep task did not complete within timeout, aborting"),
        }
        match tokio::time::timeout(std::time::Duration::from_secs(5), auto_title_task).await {
            Ok(Ok(())) => debug!("Auto-title task completed gracefully"),
            Ok(Err(e)) => warn!("Auto-title task panicked: {}", e),
            Err(_) => warn!("Auto-title task did not complete within timeout, aborting"),
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
