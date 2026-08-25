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
    /// The extracted app config the daemon was bound with, as JSON — what
    /// `config.effective` serves (with the LIVE provider table folded in at
    /// answer time).
    pub effective_config: Option<serde_json::Value>,
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
    pub effective_config: Option<serde_json::Value>,
    pub boot_hash: Option<String>,
    pub config_default_kiln: Option<String>,
    pub llm_state: Arc<crate::llm_state::LlmStateStore>,
    pub config_projects: Vec<crate::project_manager::ProjectLayerEntry>,
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
            effective_config,
            boot_hash,
            config_default_kiln,
            llm_state,
            config_projects,
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
            effective_config,
            boot_hash,
            config_default_kiln,
            llm_state,
            config_projects,
            session_lifecycle,
        }
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
            effective_config: None,
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
        })
    }
}
