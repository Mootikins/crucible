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
use crate::rpc::{DeferredShutdown, RpcContext, RpcContextParams, RpcDispatcher};
use crate::rpc_helpers::{optional_param, require_param};
use crate::session_manager::{KilnFilter, SessionManager};
use crate::session_storage::{FileSessionStorage, SessionStorage};
use crate::skills::discovery::{default_discovery_paths, FolderDiscovery};
use crate::tools::mcp_gateway::{McpGatewayManager, ReconnectSchedule};
use crate::tools::workspace::WorkspaceTools;
use anyhow::Result;
use chrono::Utc;
use crucible_core::config::{DataClassification, LlmConfig, TrustLevel};
use crucible_core::session::RecordingMode;
use crucible_lua::{
    register_cru_on_api, LuaExecutor, LuaScriptHandlerRegistry, PluginManager,
    Session as LuaSession,
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

mod accept;
mod bind;
mod core;
mod external_announce;
mod file_event_hooks;
pub mod fs;
pub mod grep;
pub mod kiln;
mod plugin_boot;
mod socket_lock;
mod socket_privacy;
pub(crate) mod ui_broadcast;
use accept::{accept_error_is_transient, ACCEPT_ERROR_BACKOFF};
pub use bind::BindWithPluginConfigParams;
use socket_lock::acquire_socket_lock;
use socket_privacy::{bind_private_listener, prepare_socket_dir};
pub mod llm;
pub mod lua;
pub mod lua_plugin_suite;
pub mod note_refactor;
pub mod notifications;
pub mod observe;
pub mod platform;
pub mod plugin_install;
pub mod plugins;
pub mod session;
pub mod storage;

use core::*;
use plugins::*;

/// How many events the broadcast ring retains before a slow receiver's cursor
/// falls off the back of it.
///
/// **A mitigation, not a fix.** No capacity makes lag impossible; it only moves
/// the threshold. What makes the loss survivable is the `stream_gap` marker
/// (`core::stream_gap_event`) — this number just makes the marker rare.
///
/// One ring, shared by every receiver, so the cost is paid once for the daemon
/// rather than per connected client. Sized against the traffic that actually
/// causes lag, which is `text_delta`: ~196 bytes serialized (a 45-char chunk,
/// full envelope with timestamp and seq), so 4096 slots is roughly 800 KiB of
/// retained deltas, and a client has to fall ~4096 deltas — a large fraction of
/// one streamed response — behind before it loses anything.
///
/// The worst case is not bounded by this number and never was: a
/// `message_complete` or a `tool_result` carries an arbitrarily large
/// `serde_json::Value` (a 2 KiB response body already serializes to ~2.3 KiB),
/// so a ring full of those is megabytes at any capacity. Capping event *payload*
/// size is the fix for that and is a different change.
const EVENT_CHANNEL_CAPACITY: usize = 4096;

/// Daemon server that listens on a Unix socket
pub struct Server {
    listener: UnixListener,
    shutdown_tx: broadcast::Sender<()>,
    kiln_manager: Arc<KilnManager>,
    session_manager: Arc<SessionManager>,
    workspace_tools: Arc<WorkspaceTools>,
    agent_manager: Arc<AgentManager>,
    project_manager: Arc<ProjectManager>,
    dispatcher: Arc<RpcDispatcher>,
    /// The same context the dispatcher runs handlers against. Held so plugin
    /// boot can give the Lua session bridge the daemon's real create path
    /// instead of a second, thinner one. The event sender and the
    /// subscription manager live here too; the server reads them from the
    /// context instead of its own clones.
    rpc_context: Arc<RpcContext>,
    plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
    runtimepath: Vec<std::path::PathBuf>,
    plugin_watch: bool,
    auto_archive_hours: Option<u64>,
    schedules: Vec<crucible_core::config::ScheduleEntry>,
    /// Resolved daemon data root (see `BindWithPluginConfigParams::data_home`);
    /// `run()`'s open-kilns/archive-sweep read this instead of `crucible_home()`.
    data_home: std::path::PathBuf,
    /// Held open for the daemon's lifetime; the flock on it enforces that only
    /// one daemon binds this socket. Dropped (unlocked) when the Server drops.
    #[allow(dead_code)]
    socket_lock: Option<std::fs::File>,
    /// The only uid allowed to connect (see `core::peer_accepted`). Always the
    /// daemon's own uid in production; a test overrides it to prove the accept
    /// path really consults it.
    authorized_uid: u32,
    /// The same gateway the agent manager dispatches through. `run()` starts
    /// the reconnect loop on it when an upstream asks for `auto_reconnect`.
    mcp_gateway: Option<Arc<tokio::sync::RwLock<McpGatewayManager>>>,
}

pub struct LuaSessionState {
    pub(crate) executor: LuaExecutor,
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

impl Server {
    /// Bind to a Unix socket path with plugin configuration
    pub async fn bind_with_plugin_config(params: BindWithPluginConfigParams) -> Result<Self> {
        // Verify (or privately create) the socket's parent dir first. This runs
        // BEFORE acquire_socket_lock, not just before the bind: the lock file is
        // opened inside this directory, so a squatted directory would otherwise
        // be reached by the lock before anything had looked at it.
        if let Some(parent) = params.path.parent() {
            prepare_socket_dir(
                parent,
                &crucible_core::protocol::lifecycle::fallback_socket_dir(),
            )?;
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

        let listener = bind_private_listener(&params.path)?;
        let (shutdown_tx, _) = broadcast::channel(1);
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

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

        // Seed the Lua app-config store — but only on the value-injection
        // path (tests, an in-process daemon handed a config value). When the
        // boot evaluation built the loader, the store is already live from
        // that evaluation, and re-seeding would wipe the free-form keys
        // init.lua's `cru.config.set` calls put there.
        if params.loader.is_none() {
            if let Some(app_config) = params.app_config.clone() {
                crucible_lua::seed_app_config(app_config);
            }
        }

        // Resolve the daemon data root ONCE. Every crucible_home() read below and
        // in the runtime handlers (session list, archive sweep) now goes through
        // this value instead of calling the global; `None` keeps the
        // crucible_home() default so production behavior is unchanged, while tests
        // inject a TempDir (no env mutation).
        let data_home = params
            .data_home
            .clone()
            .unwrap_or_else(crucible_core::config::crucible_home);

        // Same treatment for the agent-card roots: the global card directory
        // and the config's `agent_directories`, resolved once here so
        // handlers read a value instead of the environment.
        // The config home the daemon was HANDED. Everything that WRITES under
        // it must read this, not the environment.
        //
        // The environment answers only when nothing was injected at all. A
        // test that injects a data home but no config home must not reach the
        // developer's own `~/.config`: that is how the stub writer edited it
        // on every in-process daemon boot.
        let config_home = params
            .config_home
            .clone()
            .or_else(|| params.data_home.is_none().then(dirs::config_dir).flatten());

        let card_roots = crate::agent_cards::CardRoots::from_app_config(
            config_home.clone(),
            params.app_config.as_ref(),
            dirs::home_dir().as_deref(),
        );

        // The kiln registry, built from the config the daemon was HANDED —
        // never re-read from disk, or the daemon and the client that spawned
        // it would disagree about which kilns exist. A name claimed by two
        // different directories is a startup abort (the `?`), because picking
        // a winner silently re-points already-persisted sessions at a
        // different corpus.
        let kiln_registry = Arc::new(crate::kiln_registry::KilnRegistry::from_app_config(
            crate::kiln_registry::KilnRegistryContext::for_daemon(data_home.clone()),
            params.app_config.as_ref(),
        )?);

        // The state layer, under the config layer. `kilns.json` holds what the
        // daemon was TOLD; the config holds what the user AUTHORED, and the
        // config wins on a name conflict. A shadowed entry stays in the file:
        // the overlay decides which layer answers a name, and it never
        // rewrites the file it read.
        let kiln_state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));
        // A shadowed entry is the one case where a name the user registered
        // resolves somewhere else. Silence here is a support ticket: say it
        // once at startup, naming both paths.
        for shadowed in kiln_registry.overlay_state(kiln_state.registrations()) {
            warn!(
                kiln = shadowed.name,
                config = %shadowed.config_path.display(),
                registered = %shadowed.state_path.display(),
                "The config declares this kiln name, so the registered directory is not used"
            );
        }
        info!(kilns = kiln_registry.len(), "Kiln registry built");

        // The same precedence rule over the provider table. `llm.json` holds
        // the selection `cru init` and the wizard recorded; the config holds
        // what the user authored, and the config wins on a provider key.
        //
        // Overlaid HERE, at the one point the daemon derives its provider
        // table from the config it was handed, and before either consumer sees
        // it. `AgentManager` and `RpcContext` both take a clone of this value,
        // so a second insertion point would be a second answer.
        let llm_state = Arc::new(crate::llm_state::LlmStateStore::new(&data_home));
        let mut llm_config = params.llm_config.clone();
        if let Some(llm) = llm_config.as_mut() {
            for shadowed in llm_state.overlay_onto(llm) {
                warn!(
                    provider = shadowed.name,
                    config_type = shadowed.config_type,
                    recorded_type = shadowed.state_type,
                    "The config declares this provider, so the recorded selection is not used"
                );
            }
        } else {
            // No config layer at all: the state layer IS the provider table.
            // A daemon bound without an app config still has to honour a
            // selection the user made through `cru init`.
            let mut empty = crucible_core::config::LlmConfig::default();
            llm_state.overlay_onto(&mut empty);
            if !empty.providers.is_empty() {
                llm_config = Some(empty);
            }
        }

        let kiln_manager = Arc::new(
            KilnManager::with_event_tx(
                event_tx.clone(),
                params.enrichment_config.clone(),
                params.max_precognition_chars,
            )
            // So a kiln it opened by path can be broadcast by the name the user
            // registered it under. The same registry the session manager and the
            // storage layer resolve against — two would be two answers to "where
            // is kiln X".
            .with_kiln_registry(kiln_registry.clone()),
        );

        // The boot evaluation's loader when one was handed in — init.lua has
        // already evaluated in its VM — otherwise a fresh one (tests, an
        // in-process daemon handed a config value).
        let built_loader = match params.loader {
            Some(loader) => Ok(loader),
            None => DaemonPluginLoader::new(params.plugin_config.clone()),
        };
        let plugin_loader = Arc::new(Mutex::new(
            match built_loader.and_then(|loader| {
                // `kiln://<name>/…` paths in `cru.fs` resolve through the
                // registry inside the daemon; the directory never reaches Lua.
                loader
                    .with_kiln_path_resolver(kiln_registry.clone())?
                    // The named kiln reads and `cru.embed` reach the open
                    // kiln by name, through the same registry.
                    .with_kiln_repository_resolver(kiln_registry.clone(), kiln_manager.clone())?
                    .with_embed_resolver(kiln_registry.clone(), kiln_manager.clone())
            }) {
                Ok(loader) => {
                    info!("Daemon plugin loader initialized");
                    // Persisted settings-pane values live under the same root,
                    // read at boot and replayed after every plugin reload.
                    Some(loader.with_option_store(data_home.clone()))
                }
                Err(e) => {
                    warn!("Failed to initialize daemon plugin loader: {}", e);
                    None
                }
            },
        ));

        // A publication is data a plugin owns and both frontends draw. Push it
        // rather than making them poll: without this a web panel showing a
        // plugin's own state re-fetches on a timer and still shows a stale
        // board between ticks.
        //
        // Addressed to the system pseudo-session, like the file watcher and the
        // kiln manager's classification prompt — these belong to the daemon,
        // not to a conversation.
        if let Ok(guard) = plugin_loader.try_lock() {
            if let Some(loader) = guard.as_ref() {
                let hook_tx = event_tx.clone();
                loader.publications().set_change_hook(std::sync::Arc::new(
                    move |plugin: &str, key: &str| {
                        let event = crucible_core::protocol::SessionEventMessage::new(
                            crate::event_map::SYSTEM_SESSION,
                            crate::event_map::PUBLICATION_CHANGED_EVENT,
                            serde_json::json!({ "plugin": plugin, "key": key }),
                        );
                        crate::event_emitter::emit_event(&hook_tx, event);
                    },
                ));
            }
        }

        // Workspace directories ride in on the serialized app config;
        // `scm.clone` and the startup repo scan read `root_dir` from it, and
        // `session_scratch_dir` (below) seeds the session manager's scratch-workspace
        // base. Absent/unparseable → daemon defaults.
        let workspace_config = params
            .app_config
            .as_ref()
            .and_then(|v| v.get("workspace"))
            .and_then(|w| {
                serde_json::from_value::<crucible_core::config::WorkspaceConfig>(w.clone()).ok()
            });

        // Base directory for per-session scratch workspaces (sessions created
        // without an explicit workspace). Resolved once here so the session
        // manager can materialize `<base>/<session_id>` on create. The default is
        // `<data_home>/workspaces` — `~/.crucible/workspaces` in production, an
        // injected temp dir under test — so tests never touch the real home.
        let session_workspace_dir = crate::scm::resolve_session_scratch_dir(
            workspace_config
                .as_ref()
                .and_then(|c| c.session_scratch_dir.as_deref()),
            dirs::home_dir().as_deref(),
            &data_home,
        );
        let sessions_root = FileSessionStorage::root_for(&data_home);
        // Both halves of the name↔path mapping come from the one registry: the
        // storage layer turns a persisted path back into a name on load, and
        // everything downstream turns a name into a directory. Two registries
        // here would be two answers to "which directory is `notes`".
        let session_manager = Arc::new(
            SessionManager::with_storage(Arc::new(
                FileSessionStorage::new(sessions_root.clone()).with_registry(kiln_registry.clone()),
            ))
            .with_kiln_registry(kiln_registry.clone())
            .with_session_workspace_dir(Some(session_workspace_dir)),
        );
        let workspace_tools = Arc::new(WorkspaceTools::new(&data_home));
        let delegation_service =
            crate::delegation::DelegationService::new(session_manager.clone(), event_tx.clone());
        let agent_manager = Arc::new(
            AgentManager::new_with_delegation(
                AgentManagerParams {
                    kiln_manager: kiln_manager.clone(),
                    session_manager: session_manager.clone(),
                    background_manager: Arc::new(BackgroundJobManager::new(event_tx.clone())),
                    mcp_gateway: mcp_gateway.clone(),
                    llm_config: llm_config.clone(),
                    acp_config: params.acp_config.clone(),
                    context_config: params.context_config.clone(),
                    permission_config: params.permission_config.clone(),
                    plugin_loader: Some(plugin_loader.clone()),
                    card_roots,
                },
                delegation_service.clone(),
            )
            .with_runtimepath(params.runtimepath.clone()),
        );
        delegation_service.bind_agent_manager(&agent_manager);
        let subscription_manager = Arc::new(SubscriptionManager::new());
        let project_manager = Arc::new(ProjectManager::new(data_home.join("projects.json")));

        // Register every checkout that sits directly under the workspace root
        // dir, so a user who keeps all their repositories in one place does not
        // register each by hand before the web root picker shows it.
        //
        // Off unless `[workspace] discover` says otherwise. `register` applies
        // only the daemon floor, while a registered root is a web read/write
        // scope — see `WorkspaceConfig::discover`. Off by default also keeps
        // the scan out of every test that binds a daemon: those pass no
        // `[workspace]` table, and a scan would read the developer's real
        // `~/Projects` rather than an injected root.
        if workspace_config.as_ref().is_some_and(|c| c.discover) {
            let workspace_root_dir = crate::scm::resolve_workspace_root_dir(
                workspace_config.as_ref().map(|c| c.root_dir.as_str()),
                dirs::home_dir().as_deref(),
            );
            let discovered = project_manager.discover_repos_in(&workspace_root_dir);
            info!(
                root = %workspace_root_dir.display(),
                discovered,
                "Scanned the workspace root dir for repositories"
            );
        }

        // Sessions used to be filed inside their owning kiln. Collect any that
        // still are before anything can read or write one — after this point
        // every path resolves against `sessions_root` alone, so a session left
        // behind is a session nobody can find.
        crate::session_migration::migrate_sessions(
            &sessions_root,
            &crate::session_migration::known_kiln_roots(
                params.app_config.as_ref(),
                &project_manager.list(),
                &data_home,
            ),
        )
        .await;
        let lua_sessions = Arc::new(DashMap::new());
        let mcp_server_manager = Arc::new(McpServerManager::new_with_gateway(mcp_gateway.clone()));

        // One hub for both VMs. The drain runs for the daemon's life; a VM
        // only ever holds the channel into it.
        let notifications = Arc::new(crate::notifications::NotificationHub::new(
            &data_home,
            session_manager.clone(),
            project_manager.clone(),
            event_tx.clone(),
        ));
        notifications.spawn_drain();

        let ctx = Arc::new(RpcContext::new(RpcContextParams {
            kiln: kiln_manager.clone(),
            sessions: session_manager.clone(),
            agents: agent_manager.clone(),
            subscriptions: subscription_manager,
            event_tx,
            shutdown_tx: shutdown_tx.clone(),
            project_manager: project_manager.clone(),
            lua_sessions,
            plugin_loader: plugin_loader.clone(),
            mcp_server_manager,
            mcp_config: params.mcp_config.clone(),
            data_home: data_home.clone(),
            workspace_config,
            kiln_registry,
            kiln_state,
            llm_state,
            // What `config.effective` serves: the extracted config this
            // daemon was bound with, and the boot-input hash when it booted
            // through the one-VM evaluation.
            effective_config: params.app_config.clone(),
            boot_hash: params.boot_hash.clone(),
            // `[projects.*]` from the config the daemon was handed. Normalized
            // here so the listing applies the one precedence rule rather than
            // re-deriving it, and so a user can SEE which layer owns a project
            // name — the same problem the kiln shadow row exists to solve.
            config_projects: params
                .app_config
                .as_ref()
                .and_then(|c| c.get("projects"))
                .and_then(|v| v.as_object())
                .map(|projects| {
                    projects
                        .iter()
                        .filter_map(|(name, entry)| {
                            let path = entry.get("path")?.as_str()?;
                            Some(crate::project_manager::ProjectLayerEntry {
                                name: name.clone(),
                                path: std::path::PathBuf::from(path),
                                // The `[projects.*].kilns` list is already kiln
                                // NAMES, which is what the state layer has to
                                // be resolved into to match it.
                                kilns: entry
                                    .get("kilns")
                                    .and_then(|k| k.as_array())
                                    .map(|k| {
                                        k.iter()
                                            .filter_map(|v| v.as_str().map(str::to_string))
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                origin: crucible_core::config::RegistrationOrigin::Config,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            config_path: params.config_path.clone(),
            // The config layer's own answer to "which kiln by default". Read
            // from the config the daemon was HANDED, like every other config
            // value here, never from a file this process re-opens.
            config_default_kiln: params
                .app_config
                .as_ref()
                .and_then(|c| c.get("default_kiln"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            notifications,
        }));
        // Same instance for both paths: delegated children fire plugin start
        // hooks and get their own isolation claim, and the once-only teardown
        // claim is shared, so a child ended by the delegation watcher and a
        // parent ended by `session.end` cannot double-fire a plugin teardown.
        delegation_service.bind_session_lifecycle(ctx.session_lifecycle.clone());

        let dispatcher = Arc::new(RpcDispatcher::new(ctx.clone()));

        info!("Daemon listening on {:?}", params.path);
        Ok(Self {
            listener,
            shutdown_tx,
            kiln_manager,
            session_manager,
            workspace_tools,
            agent_manager,
            project_manager,
            dispatcher,
            rpc_context: ctx,
            mcp_gateway,
            plugin_loader,
            runtimepath: params.runtimepath,
            plugin_watch: params.plugin_watch,
            auto_archive_hours: params.auto_archive_hours,
            schedules: params.schedules,
            data_home,
            socket_lock,
            authorized_uid: daemon_uid(),
        })
    }

    /// Test seam for the peer check: spawning a process under a second uid needs
    /// privileges the suite does not have, so the check is driven end-to-end by
    /// moving the *expected* uid instead of the peer's.
    #[cfg(test)]
    fn set_authorized_uid(&mut self, uid: u32) {
        self.authorized_uid = uid;
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
        self.rpc_context.event_tx.clone()
    }

    /// Run the server until shutdown
    pub async fn run(self) -> Result<()> {
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        self.boot_plugins().await;

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

        // Spawn event persistence task with cancellation support
        // The same registry the session manager resolves against. Without it
        // this task would save sessions whose every kiln name resolves to
        // nothing — silently emptying `kilns` in each `meta.json` it touches.
        let storage = self.session_manager.storage().clone();
        let sm_clone = self.session_manager.clone();
        let mut persist_rx = self.rpc_context.event_tx.subscribe();
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
                                            if let Err(e) = persist_event(&event, &sm_clone, storage.as_ref()).await {
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

                                                if let Err(e) = persist_event(&event, &sm_clone, storage.as_ref()).await {
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
        let mut reprocess_rx = self.rpc_context.event_tx.subscribe();
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
        let sweep_subscription_manager = self.rpc_context.subscriptions.clone();
        let sweep_agent_manager = self.agent_manager.clone();
        let sweep_cancel = CancellationToken::new();
        let sweep_cancel_clone = sweep_cancel.clone();
        let auto_archive_hours = self.auto_archive_hours.unwrap_or(72);

        let archive_sweep_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30 * 60));
            loop {
                tokio::select! {
                    biased;
                    _ = sweep_cancel_clone.cancelled() => break,
                    _ = interval.tick() => {
                        match sweep_and_archive_stale_sessions(
                            &sweep_session_manager,
                            &sweep_subscription_manager,
                            &sweep_agent_manager,
                            auto_archive_hours,
                        ).await {
                            Ok(archived) if archived > 0 => {
                                info!(archived, auto_archive_hours, "Auto-archived stale sessions");
                            }
                            Ok(_) => {}
                            Err(e) => {
                                warn!(error = %e, "Auto-archive sweep failed");
                            }
                        }

                        // Same tick, same sessions root: keep refs whose
                        // session directory has been removed pin git objects
                        // that nothing else will ever release.
                        let dropped = crate::review::sweep_review_refs(
                            sweep_session_manager.sessions_root(),
                        ).await;
                        if dropped > 0 {
                            info!(dropped, "Released review keep refs for deleted sessions");
                        }
                    }
                }
            }
        });

        // Review backstop: watch every session's roots so a write no capture
        // bracket owned is announced instead of being noticed the next time
        // someone happens to reload. Started here rather than in
        // `AgentManager::new` because a watch is an async inotify
        // registration, not a field; a daemon that cannot start one still
        // attributes bracketed writes exactly, it just stops pushing.
        let review_watch_cancel = CancellationToken::new();
        let review_watch_task = match crate::watch::external_changes::ExternalChangeWatch::start(
            Arc::new(crate::watch::external_changes::ExternalChangeTracker::default()),
        )
        .await
        {
            Ok(watch) => {
                let watch = Arc::new(watch);
                self.agent_manager.set_external_watch(Arc::clone(&watch));
                let rx = watch.tracker().subscribe();
                let tx = self.rpc_context.event_tx.clone();
                let cancel = review_watch_cancel.clone();
                Some((
                    Arc::clone(&watch),
                    tokio::spawn(external_announce::announce_external_changes(rx, tx, cancel)),
                ))
            }
            Err(e) => {
                warn!(error = %e, "review external-change watch not started; edits made outside a tool call will not push");
                None
            }
        };

        // Reconnect loop: an upstream with `auto_reconnect = true` comes back
        // without a daemon restart. Skipped when no upstream asks for it, so a
        // gateway-free daemon spawns nothing.
        let reconnect_cancel = CancellationToken::new();
        let reconnect_task = self.mcp_gateway.as_ref().and_then(|gateway| {
            let wanted = gateway
                .try_read()
                .map(|gw| gw.upstream_count_with_auto_reconnect())
                .unwrap_or(0);
            (wanted > 0).then(|| {
                McpGatewayManager::start_reconnect_loop(
                    Arc::clone(gateway),
                    reconnect_cancel.clone(),
                    ReconnectSchedule::default(),
                )
            })
        });

        // Auto-title task: when a turn completes in a still-untitled session,
        // generate a topic-based title daemon-side so every client (TUI, web,
        // ACP) gets titled sessions without asking for it.
        let title_sm = self.session_manager.clone();
        let title_am = self.agent_manager.clone();
        let title_event_tx = self.rpc_context.event_tx.clone();
        let mut title_rx = self.rpc_context.event_tx.subscribe();
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
            let tx = self.rpc_context.event_tx.clone();

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

                let titled = sm.title_untitled_sessions(&tx).await;
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
                            let dispatcher = self.dispatcher.clone();
                            let authorized_uid = self.authorized_uid;
                            let event_rx = self.rpc_context.event_tx.subscribe();
                            tokio::spawn(async move {
                                if let Err(e) =
                                    handle_client(stream, dispatcher, authorized_uid, event_rx).await
                                {
                                    error!("Client error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            // Never retry an accept error at full speed. A
                            // per-connection failure is transient and worth an
                            // immediate retry, but a RESOURCE error — fd
                            // exhaustion above all — fails synchronously without
                            // registering readiness, so retrying at once means
                            // this loop stops returning `Pending`: a pegged core,
                            // a log line per iteration, and a task that cannot
                            // yield to whatever would release the fd it needs.
                            // hyper's `AddrIncoming` backs off for this reason.
                            if accept_error_is_transient(&e) {
                                debug!(error = %e, "Transient accept error; retrying");
                            } else {
                                error!(
                                    error = %e,
                                    backoff_ms = ACCEPT_ERROR_BACKOFF.as_millis() as u64,
                                    "Accept failed; backing off before retrying"
                                );
                                tokio::time::sleep(ACCEPT_ERROR_BACKOFF).await;
                            }
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
        persist_cancel.cancel();
        reprocess_cancel.cancel();
        sweep_cancel.cancel();
        title_cancel.cancel();
        // The notifier parks on `rx.recv()`, and the tracker's sender outlives
        // it (`AgentManager` holds the watch), so `Closed` never arrives on its
        // own — without this the join below always burns its full timeout.
        review_watch_cancel.cancel();
        reconnect_cancel.cancel();
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
        if let Some(task) = reconnect_task {
            match tokio::time::timeout(std::time::Duration::from_secs(5), task).await {
                Ok(Ok(())) => debug!("MCP reconnect task completed gracefully"),
                Ok(Err(e)) => warn!("MCP reconnect task panicked: {}", e),
                Err(_) => warn!("MCP reconnect task did not complete within timeout, aborting"),
            }
        }
        if let Some((watch, task)) = review_watch_task {
            if let Err(e) = watch.shutdown().await {
                warn!(error = %e, "review external-change watch shutdown failed");
            }
            match tokio::time::timeout(std::time::Duration::from_secs(5), task).await {
                Ok(Ok(())) => debug!("Review watch task completed gracefully"),
                Ok(Err(e)) => warn!("Review watch task panicked: {}", e),
                Err(_) => warn!("Review watch task did not complete within timeout, aborting"),
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
