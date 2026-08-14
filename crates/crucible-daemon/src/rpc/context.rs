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
use crucible_core::config::{LlmConfig, McpConfig, ScmConfig};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

pub struct RpcContext {
    pub kiln: Arc<KilnManager>,
    pub sessions: Arc<SessionManager>,
    pub agents: Arc<AgentManager>,
    pub subscriptions: Arc<SubscriptionManager>,
    pub event_tx: broadcast::Sender<SessionEventMessage>,
    pub shutdown_tx: broadcast::Sender<()>,
    pub project_manager: Arc<crate::project_manager::ProjectManager>,
    pub lua_sessions: Arc<DashMap<String, Arc<Mutex<crate::server::LuaSessionState>>>>,
    pub plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
    pub llm_config: Option<LlmConfig>,
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
    /// Root the global agent-card directory (`<config_home>/crucible/agents`)
    /// hangs off — `dirs::config_dir()` in production. Injected as a value for
    /// the same reason `data_home` is: global cards are first in discovery
    /// precedence, so a handler that read the environment would resolve a
    /// developer's personal cards in every test. `None` means "no global
    /// cards".
    pub config_home: Option<std::path::PathBuf>,
    /// Active workflow executions keyed by session id (Phase 3a).
    pub workflows: Arc<WorkflowRegistry>,
    /// SCM (git) config — `scm.clone` reads `projects_dir` from here.
    pub scm_config: Option<ScmConfig>,
    /// Plugin session start/end enforcement, shared with `DelegationService`.
    ///
    /// Built here rather than passed in because every input it needs is
    /// already a field, and `server::bind` hands this same `Arc` to the
    /// delegation service — one instance, so the once-only teardown claim
    /// covers RPC-ended and delegation-ended sessions alike.
    pub session_lifecycle: Arc<SessionLifecycle>,
}

impl RpcContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kiln: Arc<KilnManager>,
        sessions: Arc<SessionManager>,
        agents: Arc<AgentManager>,
        subscriptions: Arc<SubscriptionManager>,
        event_tx: broadcast::Sender<SessionEventMessage>,
        shutdown_tx: broadcast::Sender<()>,
        project_manager: Arc<crate::project_manager::ProjectManager>,
        lua_sessions: Arc<DashMap<String, Arc<Mutex<crate::server::LuaSessionState>>>>,
        plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
        llm_config: Option<LlmConfig>,
        mcp_server_manager: Arc<McpServerManager>,
        mcp_config: Option<McpConfig>,
        data_home: std::path::PathBuf,
        config_home: Option<std::path::PathBuf>,
        scm_config: Option<ScmConfig>,
    ) -> Self {
        let session_lifecycle = SessionLifecycle::new(sessions.clone(), plugin_loader.clone());
        session_lifecycle.bind_agent_manager(&agents);
        Self {
            kiln,
            sessions,
            agents,
            subscriptions,
            event_tx,
            shutdown_tx,
            project_manager,
            lua_sessions,
            plugin_loader,
            llm_config,
            mcp_server_manager,
            mcp_config,
            data_home,
            config_home,
            workflows: Arc::new(WorkflowRegistry::new()),
            scm_config,
            session_lifecycle,
        }
    }

    /// A context for handler unit tests, built from the managers the test
    /// actually cares about; everything else is empty (no plugin loader, no
    /// MCP, no SCM).
    ///
    /// `data_home` is a parameter rather than `crucible_home()` for the usual
    /// reason: a test that reads the developer's real `~/.crucible` passes on
    /// CI and fails locally. `config_home` is `None` for the same reason —
    /// no global agent cards unless a test asks for them.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn for_test(
        kiln: Arc<KilnManager>,
        sessions: Arc<SessionManager>,
        agents: Arc<AgentManager>,
        project_manager: Arc<crate::project_manager::ProjectManager>,
        event_tx: broadcast::Sender<SessionEventMessage>,
        llm_config: Option<LlmConfig>,
        data_home: std::path::PathBuf,
    ) -> Self {
        Self::for_test_with_plugin_loader(
            kiln,
            sessions,
            agents,
            project_manager,
            event_tx,
            llm_config,
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
        llm_config: Option<LlmConfig>,
        data_home: std::path::PathBuf,
        plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self::new(
            kiln,
            sessions,
            agents,
            Arc::new(SubscriptionManager::new()),
            event_tx,
            shutdown_tx,
            project_manager,
            Arc::new(DashMap::new()),
            plugin_loader,
            llm_config,
            Arc::new(McpServerManager::new()),
            None,
            data_home,
            None,
            None,
        )
    }
}
