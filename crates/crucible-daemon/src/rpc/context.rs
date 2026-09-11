//! RPC context holding shared state for handlers

use crate::agent_manager::AgentManager;
use crate::daemon_plugins::DaemonPluginLoader;
use crate::kiln_manager::KilnManager;
use crate::mcp_server::McpServerManager;
use crate::protocol::SessionEventMessage;
use crate::session_lifecycle::SessionLifecycle;
use crate::session_manager::SessionManager;
use crate::subscription::SubscriptionManager;
use crate::workflow_registry::WorkflowRegistry;
use crucible_core::config::{McpConfig, WorkspaceConfig};
use dashmap::DashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

/// The daemon's shutdown signal, with the latch that keeps an RPC-initiated
/// shutdown from outrunning its own reply.
///
/// `Server::run` breaks its accept loop as soon as the signal lands and the
/// process exits behind it, while the reply to `shutdown` is written by a
/// separate connection task. Signalling from inside the handler therefore races
/// the confirmation the caller is blocked reading: on a loaded machine the
/// daemon is gone first and the caller sees EOF. So `shutdown` *arms* the
/// signal and the connection *fires* it once the confirmation is on the wire.
///
/// Signals and tests hold the sender directly (`Server::shutdown_handle`); they
/// have no reply to order against.
pub struct DeferredShutdown {
    tx: broadcast::Sender<()>,
    armed: AtomicBool,
}

impl DeferredShutdown {
    pub fn new(tx: broadcast::Sender<()>) -> Self {
        Self {
            tx,
            armed: AtomicBool::new(false),
        }
    }

    /// Accept a shutdown request without acting on it yet.
    pub fn arm(&self) {
        self.armed.store(true, Ordering::Release);
    }

    /// Signal an armed shutdown; a no-op for every reply that did not arm one.
    /// Called once per written reply, so the swap is what makes it fire once.
    pub fn fire_if_armed(&self) {
        if self.armed.swap(false, Ordering::AcqRel) {
            let _ = self.tx.send(());
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.tx.subscribe()
    }
}

pub struct RpcContext {
    pub kiln: Arc<KilnManager>,
    pub sessions: Arc<SessionManager>,
    pub agents: Arc<AgentManager>,
    pub subscriptions: Arc<SubscriptionManager>,
    pub event_tx: broadcast::Sender<SessionEventMessage>,
    pub shutdown: Arc<DeferredShutdown>,
    pub project_manager: Arc<crate::project_manager::ProjectManager>,
    pub lua_sessions: Arc<DashMap<String, Arc<Mutex<crate::server::LuaSessionState>>>>,
    pub plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
    /// `<data_home>/llm.json`: the provider selection the daemon recorded.
    pub llm_state: Arc<crate::llm_state::LlmStateStore>,
    /// The projects the CONFIG layer declares.
    ///
    /// Normalized at bind rather than carried as a config document, so
    /// `overlay_layers` does the same work over it that it does over kilns and
    /// providers — one precedence rule, three registries, three shapes.
    pub config_projects: Vec<crate::project_manager::ProjectLayerEntry>,
    /// The provider table, shared with `AgentManager` rather than cloned.
    ///
    /// One table: it can gain a provider while the daemon runs, and two copies
    /// would be two answers. See [`LiveLlmConfig`] for why only ADDITIONS are
    /// allowed to land live.
    ///
    /// [`LiveLlmConfig`]: crate::llm_state::LiveLlmConfig
    pub llm_config: crate::llm_state::LiveLlmConfig,
    pub mcp_server_manager: Arc<McpServerManager>,
    /// Daemon-global MCP config, threaded through because it is authoritative
    /// for WHICH servers exist: `session.create`'s setup task lists a configured
    /// server even when the gateway never connected to it, so the UI shows it
    /// disconnected rather than omitting it.
    ///
    /// It is no longer here to avoid the gateway — the setup task now also reads
    /// live tool names via `AgentManager::mcp_tools_by_upstream`, because
    /// emitting the config alone left `tools: []` / `connected: false` and the
    /// TUI forked its own MCP connections to recover them.
    pub mcp_config: Option<McpConfig>,
    /// Resolved daemon data root (see `BindWithPluginConfigParams::data_home`).
    /// Runtime handlers (session list) read this instead of calling
    /// `crucible_home()`, so they honor the injected data_home in tests.
    pub data_home: std::path::PathBuf,
    /// Active workflow executions keyed by session id (Phase 3a).
    pub workflows: Arc<WorkflowRegistry>,
    /// Workspace directories — `scm.clone` reads `root_dir` from here.
    pub workspace_config: Option<WorkspaceConfig>,
    /// Name → kiln, and the only door a filesystem path may become one
    /// through. Built once at bind from the config the daemon was handed;
    /// handlers resolve names against it rather than accepting paths.
    pub kiln_registry: Arc<crate::kiln_registry::KilnRegistry>,
    /// `<data_home>/kilns.json` — the registrations the daemon was told about,
    /// and the only writer of them. The registry is the in-memory authority;
    /// this is what makes a registration outlive the process.
    pub kiln_state: Arc<crate::kiln_state::KilnStateStore>,
    /// The config FILE this daemon's config came from, when the spawning
    /// client knew it. A refusal that names the config layer names this file,
    /// so the user knows which one to edit.
    pub config_path: Option<std::path::PathBuf>,
    /// The extracted app config the daemon was BOUND with, as JSON.
    ///
    /// Not what a handler wants: it is frozen at bind, so a `config.set`
    /// never reached it and the setting changed nothing at all — not after a
    /// restart, but immediately. Call [`RpcContext::effective_config`], which
    /// reads the live store. This field survives only to supply the location
    /// keys, which the store drops when the boot phase ends.
    pub bound_config: Option<serde_json::Value>,
    /// The boot-input hash (`daemon_plugins::boot_input_hash`) recorded at
    /// boot; `None` for a daemon handed a config value directly.
    pub boot_hash: Option<String>,
    /// `default_kiln` as the CONFIG layer states it, when it states one.
    ///
    /// The state store carries its own default — the chat preflight sets it —
    /// and the config out-ranks it, the same rule that decides a name conflict.
    /// Held here so the listing applies that rule instead of re-deriving it.
    pub config_default_kiln: Option<String>,
    /// Plugin session start/end enforcement, shared with `DelegationService`.
    ///
    /// Built here rather than passed in because every input it needs is
    /// already a field, and `server::bind` hands this same `Arc` to the
    /// delegation service — one instance, so the once-only teardown claim
    /// covers RPC-ended and delegation-ended sessions alike.
    pub session_lifecycle: Arc<SessionLifecycle>,
    /// The daemon's notification ring and its fan-out. Every VM's
    /// `cru.log.notify` lands here; `notification.list` reads it.
    pub notifications: Arc<crate::notifications::NotificationHub>,
}

/// Everything `RpcContext::new` needs from its caller. A struct, not a
/// parameter list, so each value has a name at the call site and the
/// argument count no longer needs a lint exception.
pub struct RpcContextParams {
    pub kiln: Arc<KilnManager>,
    pub sessions: Arc<SessionManager>,
    pub agents: Arc<AgentManager>,
    pub subscriptions: Arc<SubscriptionManager>,
    pub event_tx: broadcast::Sender<SessionEventMessage>,
    pub shutdown_tx: broadcast::Sender<()>,
    pub project_manager: Arc<crate::project_manager::ProjectManager>,
    pub lua_sessions: Arc<DashMap<String, Arc<Mutex<crate::server::LuaSessionState>>>>,
    pub plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
    pub mcp_server_manager: Arc<McpServerManager>,
    pub mcp_config: Option<McpConfig>,
    pub data_home: std::path::PathBuf,
    pub workspace_config: Option<WorkspaceConfig>,
    pub kiln_registry: Arc<crate::kiln_registry::KilnRegistry>,
    pub kiln_state: Arc<crate::kiln_state::KilnStateStore>,
    pub config_path: Option<std::path::PathBuf>,
    pub bound_config: Option<serde_json::Value>,
    pub boot_hash: Option<String>,
    pub config_default_kiln: Option<String>,
    pub llm_state: Arc<crate::llm_state::LlmStateStore>,
    pub config_projects: Vec<crate::project_manager::ProjectLayerEntry>,
    pub notifications: Arc<crate::notifications::NotificationHub>,
}

/// [`RpcContext::effective_config`]'s pure half: the live store value, with
/// the bind snapshot's location keys folded back in.
///
/// Split out because the impure half reads a process global, and a resolver
/// that reads a global cannot be tested for the case that matters — a store
/// that has moved on since bind.
///
/// `live` alone is wrong (it has no locations after `end_boot_phase`), and
/// `bound` alone is the bug (it is frozen at bind).
fn fold_locations(
    live: Option<serde_json::Value>,
    bound: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    // No bind snapshot means the daemon was bound without an app config.
    // `config.effective` says so rather than inventing one from the store,
    // which is what it did before this function existed.
    let bound_value = bound?;
    let (Some(bound_map), Some(serde_json::Value::Object(mut merged))) =
        (bound_value.as_object(), live)
    else {
        return Some(bound_value.clone());
    };
    for key in crucible_core::config::LOCATION_CONFIG_KEYS {
        if let Some(value) = bound_map.get(key) {
            merged.insert(key.to_string(), value.clone());
        }
    }
    Some(serde_json::Value::Object(merged))
}

impl RpcContext {
    pub fn new(params: RpcContextParams) -> Self {
        let RpcContextParams {
            kiln,
            sessions,
            agents,
            subscriptions,
            event_tx,
            shutdown_tx,
            project_manager,
            lua_sessions,
            plugin_loader,
            mcp_server_manager,
            mcp_config,
            data_home,
            workspace_config,
            kiln_registry,
            kiln_state,
            config_path,
            bound_config,
            boot_hash,
            config_default_kiln,
            llm_state,
            config_projects,
            notifications,
        } = params;
        // Taken from the agent manager, never built here: one provider table,
        // shared, so a provider added at runtime is visible to both.
        let llm_config = agents.llm_handle();
        let session_lifecycle = SessionLifecycle::new(sessions.clone(), plugin_loader.clone());
        session_lifecycle.bind_agent_manager(&agents);
        Self {
            kiln,
            sessions,
            agents,
            subscriptions,
            event_tx,
            shutdown: Arc::new(DeferredShutdown::new(shutdown_tx)),
            project_manager,
            lua_sessions,
            plugin_loader,
            llm_config,
            mcp_server_manager,
            mcp_config,
            data_home,
            workflows: Arc::new(WorkflowRegistry::new()),
            workspace_config,
            kiln_registry,
            kiln_state,
            config_path,
            bound_config,
            boot_hash,
            config_default_kiln,
            llm_state,
            config_projects,
            session_lifecycle,
            notifications,
        }
    }

    /// The daemon's effective config: the LIVE config store, with the
    /// location keys the daemon was bound with folded back in.
    ///
    /// Two halves, because the two change on different clocks. Everything a
    /// user can set at runtime comes from the store, so a `config.set` is
    /// visible to the next reader in the same process. The seven
    /// `LOCATION_CONFIG_KEYS` come from the bind snapshot: they name where
    /// the daemon executes code, `ConfigStore::end_boot_phase` drops them
    /// from the plugin-visible value on purpose, and they cannot change
    /// without a restart anyway.
    ///
    /// Reading `bound_config` directly is the bug this replaces — the field
    /// is bound once, so every handler served a value frozen at boot.
    pub fn effective_config(&self) -> Option<serde_json::Value> {
        fold_locations(crucible_lua::get_app_config(), self.bound_config.as_ref())
    }

    /// A context for handler unit tests, built from the managers the test
    /// actually cares about; everything else is empty (no plugin loader, no
    /// MCP, no SCM).
    ///
    /// `data_home` is a parameter rather than `crucible_home()` for the usual
    /// reason: a test that reads the developer's real `~/.crucible` passes on
    /// CI and fails locally. The agent manager's card roots are empty for
    /// the same reason — no global agent cards unless a test asks for them.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn for_test(
        kiln: Arc<KilnManager>,
        sessions: Arc<SessionManager>,
        agents: Arc<AgentManager>,
        project_manager: Arc<crate::project_manager::ProjectManager>,
        event_tx: broadcast::Sender<SessionEventMessage>,
        data_home: std::path::PathBuf,
    ) -> Self {
        Self::for_test_with_plugin_loader(
            kiln,
            sessions,
            agents,
            project_manager,
            event_tx,
            data_home,
            Arc::new(Mutex::new(None)),
        )
    }

    /// As [`Self::for_test`], with a live plugin loader.
    ///
    /// Separate because the loader handle is what `SessionLifecycle` locks
    /// across plugin hook execution: a test that exercises a hook re-entering
    /// the daemon needs the *same* handle the lifecycle holds, and every other
    /// handler test is better off with no plugin runtime at all.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn for_test_with_plugin_loader(
        kiln: Arc<KilnManager>,
        sessions: Arc<SessionManager>,
        agents: Arc<AgentManager>,
        project_manager: Arc<crate::project_manager::ProjectManager>,
        event_tx: broadcast::Sender<SessionEventMessage>,
        data_home: std::path::PathBuf,
        plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let registry = sessions.kiln_registry().clone();
        let notifications = Arc::new(crate::notifications::NotificationHub::new(
            &data_home,
            sessions.clone(),
            project_manager.clone(),
            event_tx.clone(),
        ));
        Self::new(RpcContextParams {
            kiln,
            sessions,
            agents,
            subscriptions: Arc::new(SubscriptionManager::new()),
            event_tx,
            shutdown_tx,
            project_manager,
            lua_sessions: Arc::new(DashMap::new()),
            plugin_loader,
            mcp_server_manager: Arc::new(McpServerManager::new()),
            mcp_config: None,
            kiln_state: Arc::new(crate::kiln_state::KilnStateStore::new(&data_home)),
            llm_state: Arc::new(crate::llm_state::LlmStateStore::new(&data_home)),
            config_projects: Vec::new(),
            config_path: None,
            bound_config: None,
            boot_hash: None,
            config_default_kiln: None,
            data_home,
            workspace_config: None,
            // The session manager's own registry, not a second empty one: the
            // handlers resolve caller-supplied names through `ctx`, the storage
            // layer resolves persisted paths through `sessions`, and two
            // registries would be two answers to "which directory is `notes`".
            // A test whose fixture disagreed with itself that way would pass or
            // fail for reasons unrelated to the code under test.
            kiln_registry: registry,
            notifications,
        })
    }
}

#[cfg(test)]
mod effective_config_tests {
    use super::fold_locations;
    use serde_json::json;

    /// A value the store gained after bind reaches the reader.
    ///
    /// This is the whole point. `config.set` merges into the store and
    /// nothing else; serving the bind snapshot made every such write a no-op
    /// in the same process, not only across a restart.
    #[test]
    fn a_later_store_write_reaches_the_reader() {
        let bound = json!({ "chat": { "context_budget": 1024 } });
        let live = json!({ "chat": { "context_budget": 4096 } });

        let effective = fold_locations(Some(live), Some(&bound)).expect("a bound config");

        assert_eq!(
            effective.pointer("/chat/context_budget"),
            Some(&json!(4096)),
            "the store is the live truth; the bind snapshot is not"
        );
    }

    /// Location keys come from the bind snapshot, because the store drops
    /// them when the boot phase ends.
    #[test]
    fn location_keys_survive_from_the_bind_snapshot() {
        let bound = json!({ "kiln_path": "/k", "data_home": "/d", "chat": { "x": 1 } });
        let live = json!({ "chat": { "x": 2 } });

        let effective = fold_locations(Some(live), Some(&bound)).expect("a bound config");

        assert_eq!(effective.get("kiln_path"), Some(&json!("/k")));
        assert_eq!(effective.get("data_home"), Some(&json!("/d")));
        assert_eq!(effective.pointer("/chat/x"), Some(&json!(2)));
    }

    /// Every location key is folded, not the two a test happened to name.
    ///
    /// Derived from the constant rather than a literal list, so a new
    /// location key cannot be forgotten here.
    #[test]
    fn every_location_key_is_folded() {
        let bound = serde_json::Value::Object(
            crucible_core::config::LOCATION_CONFIG_KEYS
                .iter()
                .map(|k| ((*k).to_string(), json!(format!("bound-{k}"))))
                .collect(),
        );
        let live = json!({});

        let effective = fold_locations(Some(live), Some(&bound)).expect("a bound config");

        for key in crucible_core::config::LOCATION_CONFIG_KEYS {
            assert_eq!(
                effective.get(key),
                Some(&json!(format!("bound-{key}"))),
                "{key} must come from the bind snapshot"
            );
        }
    }

    /// A store that never dropped a location key does not shadow the
    /// snapshot's. The daemon reads locations to decide where it executes
    /// code, so the boot-time answer is the only safe one.
    #[test]
    fn a_live_location_key_never_wins() {
        let bound = json!({ "kiln_path": "/bound" });
        let live = json!({ "kiln_path": "/live" });

        let effective = fold_locations(Some(live), Some(&bound)).expect("a bound config");

        assert_eq!(effective.get("kiln_path"), Some(&json!("/bound")));
    }

    /// No bind snapshot stays "this daemon was bound without an app config".
    #[test]
    fn no_bound_config_stays_absent() {
        assert!(fold_locations(Some(json!({ "chat": {} })), None).is_none());
    }

    /// No store yet serves the snapshot unchanged.
    #[test]
    fn no_live_store_serves_the_snapshot() {
        let bound = json!({ "chat": { "context_budget": 1024 } });
        assert_eq!(fold_locations(None, Some(&bound)), Some(bound));
    }
}
