//! RPC context holding shared state for handlers

use crate::agent_manager::AgentManager;
use crate::daemon_plugins::DaemonPluginLoader;
use crate::kiln_manager::KilnManager;
use crate::mcp_server::McpServerManager;
use crate::protocol::SessionEventMessage;
use crate::session_manager::SessionManager;
use crate::subscription::SubscriptionManager;
use crucible_config::LlmConfig;
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
    ) -> Self {
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
        }
    }
}
