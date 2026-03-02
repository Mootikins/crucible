//! Agent lifecycle management for the daemon.

use crate::agent_factory::{
    create_agent_from_session_config, AgentFactoryError, CreateAgentFromSessionConfigParams,
};
use crate::background_manager::{BackgroundJobManager, SubagentContext};
use crate::daemon_plugins::DaemonPluginLoader;
use crate::event_emitter::emit_event;
use crate::kiln_manager::KilnManager;
use crate::multi_kiln_search::{search_across_kilns, KilnSearchSource};
use crate::permission_bridge::{DaemonPermissionGate, PermissionPromptCallback};
use crate::protocol::SessionEventMessage;
use crate::provider::model_listing;
use crate::session_manager::{SessionError, SessionManager};
use crate::tool_dispatch::{DaemonToolDispatcher, ToolDispatcher};
use crate::trust_resolution::resolve_provider_trust;
use crucible_acp::discovery::default_agent_profiles;
use crucible_config::components::permissions::PermissionConfig;
use crucible_config::{
    AcpConfig, AgentProfile, BackendType, DataClassification, LlmProviderConfig, PatternStore,
};
use crucible_core::discovery::DiscoveryPaths;
use crucible_core::events::{Reactor, ReactorEmitResult as EmitResult, SessionEvent, InternalSessionEvent};
use crucible_core::interaction::{InteractionRequest, PermRequest, PermResponse, PermissionScope};
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::AgentHandle;
use crucible_core::traits::PermissionGate;
use crucible_lua::{
    execute_permission_hooks, register_crucible_on_api, register_permission_hook_api,
    LuaScriptHandlerRegistry, PermissionHook, PermissionHookResult, PermissionRequest,
};
use crucible_tools::workspace::WorkspaceTools;
use dashmap::DashMap;
use futures::StreamExt;
use mlua::Lua;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Unique identifier for a pending permission request.
pub type PermissionId = String;

const MODEL_CACHE_TTL: Duration = Duration::from_secs(300);

/// Check if a tool is safe to execute without requiring explicit permission.
///
/// Safe tools are read-only operations that only query data without modifying state.
/// Unsafe tools can modify files, execute commands, or change state and require permission.
///
/// # Safe Tools (is_safe=true)
///
/// **MCP Tools (10 read-only):**
/// - `semantic_search` — Search notes using semantic similarity
/// - `text_search` — Fast full-text search across notes
/// - `property_search` — Search notes by frontmatter properties (includes tags)
/// - `list_notes` — List notes in a directory
/// - `read_note` — Read note content with optional line range
/// - `read_metadata` — Read note metadata without loading full content
/// - `get_kiln_info` — Get comprehensive kiln information including root path and statistics
/// - `list_jobs` — List all background jobs (running and completed) for the current session
///
/// **Workspace Tools (Rig-native, 3 read-only):**
/// - `read_file` — Read file content
/// - `glob` — Find files matching patterns
/// - `grep` — Search file contents
///
/// # Unsafe Tools (is_safe=false)
///
/// **MCP Tools (6 mutating):**
/// - `create_note` — Create a new note in the kiln
/// - `update_note` — Update an existing note
/// - `delete_note` — Delete a note from the kiln
/// - `delegate_session` — Delegate a task to another AI agent
/// - `get_job_result` — Get the result of a background job
/// - `cancel_job` — Cancel a running background job by ID
///
/// **Workspace Tools (Rig-native, 3 mutating):**
/// - `write` — Write file content
/// - `edit` — Edit file content
/// - `bash` — Execute shell commands
///
/// # Default-Deny Policy
///
/// Only explicitly safe tools skip the permission prompt.
/// Everything unknown (including all external MCP tools) requires permission.
pub fn is_safe(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "read_file"
            | "glob"
            | "grep"
            | "semantic_search"
            | "text_search"
            | "property_search"
            | "list_notes"
            | "read_note"
            | "read_metadata"
            | "get_kiln_info"
            | "list_jobs"
    )
}

fn resolve_agent_profile(
    name: &str,
    configured: &HashMap<String, AgentProfile>,
    available: &HashMap<String, AgentProfile>,
) -> Option<AgentProfile> {
    let profile = configured.get(name)?;
    let base_name = profile.extends.as_deref().unwrap_or(name);

    let mut resolved = available.get(base_name).cloned().unwrap_or_default();
    resolved.extends = profile.extends.clone();

    if let Some(command) = &profile.command {
        resolved.command = Some(command.clone());
    }
    if let Some(args) = &profile.args {
        resolved.args = Some(args.clone());
    }
    if let Some(description) = &profile.description {
        resolved.description = Some(description.clone());
    }
    if let Some(capabilities) = &profile.capabilities {
        resolved.capabilities = Some(capabilities.clone());
    }
    if let Some(delegation) = &profile.delegation {
        resolved.delegation = Some(delegation.clone());
    }

    resolved.env.extend(profile.env.clone());
    Some(resolved)
}

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("No agent configured for session: {0}")]
    NoAgentConfigured(String),

    #[error("Concurrent request in progress for session: {0}")]
    ConcurrentRequest(String),

    #[error("Invalid model ID: {0}")]
    InvalidModelId(String),

    #[error("Permission not found: {0}")]
    PermissionNotFound(String),

    #[error("Session error: {0}")]
    Session(#[from] SessionError),

    #[error("Agent factory error: {0}")]
    Factory(#[from] AgentFactoryError),
}

struct RequestState {
    cancel_tx: Option<oneshot::Sender<()>>,
    task_handle: Option<JoinHandle<()>>,
    #[allow(dead_code)]
    started_at: Instant,
}

type BoxedAgentHandle = Box<dyn AgentHandle + Send + Sync>;

use mlua::RegistryKey;
use std::sync::Mutex as StdMutex;

pub(crate) struct SessionEventState {
    lua: Lua,
    registry: LuaScriptHandlerRegistry,
    permission_hooks: Arc<StdMutex<Vec<PermissionHook>>>,
    permission_functions: Arc<StdMutex<HashMap<String, RegistryKey>>>,
    pub(crate) reactor: Reactor,
}

/// Discover Lua handler files and register them with the Reactor.
/// Logs warnings on discovery/conversion failures, returns silently on empty dirs.
fn discover_and_register_lua_handlers(
    reactor: &mut Reactor,
    kiln_path: &std::path::Path,
    session_id: &str,
) {
    let paths = DiscoveryPaths::new("handlers", Some(kiln_path));
    let existing = paths.existing_paths();
    if existing.is_empty() {
        debug!(session_id = %session_id, "No handler directories found, skipping Lua handlers");
        return;
    }

    let handler_registry = match LuaScriptHandlerRegistry::discover(&existing) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to discover Lua handlers: {}", e);
            return;
        }
    };

    let handlers = match handler_registry.to_core_handlers() {
        Ok(h) => h,
        Err(e) => {
            warn!("Failed to create core handlers from Lua: {}", e);
            return;
        }
    };

    let mut loaded_count = 0;
    for handler in handlers {
        let name = handler.name().to_string();
        if let Err(e) = reactor.register(handler) {
            warn!("Failed to register Lua handler {}: {}", name, e);
        } else {
            loaded_count += 1;
            debug!("Loaded Lua handler: {}", name);
        }
    }
    if loaded_count > 0 {
        info!(session_id = %session_id, "Loaded {} Lua handlers", loaded_count);
    }
}

fn emit_precognition_event(
    event_tx: &broadcast::Sender<SessionEventMessage>,
    session_id: &str,
    query: &str,
    notes_count: usize,
    kilns_searched: usize,
    kilns_failed: usize,
    notes: Option<Vec<crucible_core::traits::chat::PrecognitionNoteInfo>>,
) {
    let query_summary = query.chars().take(100).collect::<String>();
    let event = SessionEvent::internal(InternalSessionEvent::PrecognitionComplete {
        notes_count,
        query_summary: query_summary.clone(),
        kilns_searched,
        kilns_filtered: 0,
        kilns_failed,
    });
    let mut data = serde_json::json!({
        "notes_count": notes_count,
        "query_summary": query_summary,
    });
    if let Some(notes) = notes {
        data["notes"] = serde_json::to_value(notes).unwrap_or_default();
    }
    if !emit_event(
        event_tx,
        SessionEventMessage::new(session_id, event.event_type(), data),
    ) {
        warn!(
            session_id = %session_id,
            "No subscribers for precognition_complete event"
        );
    }
}

struct PendingPermission {
    request: PermRequest,
    response_tx: oneshot::Sender<PermResponse>,
}

#[derive(Clone)]
struct StreamContext {
    session_id: String,
    message_id: String,
    event_tx: broadcast::Sender<SessionEventMessage>,
    session_state: Arc<Mutex<SessionEventState>>,
    pending_permissions: Arc<DashMap<String, HashMap<PermissionId, PendingPermission>>>,
    workspace_path: PathBuf,
    agent_stream_config: AgentStreamConfig,
    tool_dispatcher: Arc<dyn ToolDispatcher>,
}

#[allow(dead_code)]
#[derive(Clone)]
struct AgentStreamConfig {
    model: String,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    thinking_budget: Option<i64>,
    system_prompt: String,
}

impl AgentStreamConfig {
    fn from_session_agent(session_agent: &SessionAgent) -> Self {
        Self {
            model: session_agent.model.clone(),
            temperature: session_agent.temperature,
            max_tokens: session_agent.max_tokens,
            thinking_budget: session_agent.thinking_budget,
            system_prompt: session_agent.system_prompt.clone(),
        }
    }
}

#[derive(Clone)]
struct AgentCache {
    inner: Arc<DashMap<String, Arc<Mutex<BoxedAgentHandle>>>>,
}

impl AgentCache {
    fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    fn get(
        &self,
        key: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, String, Arc<Mutex<BoxedAgentHandle>>>> {
        self.inner.get(key)
    }

    fn insert(&self, key: String, value: Arc<Mutex<BoxedAgentHandle>>) {
        self.inner.insert(key, value);
    }

    fn remove(&self, key: &str) -> Option<(String, Arc<Mutex<BoxedAgentHandle>>)> {
        self.inner.remove(key)
    }

    #[cfg(test)]
    fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }
}

#[derive(Clone)]
struct SessionStateCache {
    inner: Arc<DashMap<String, Arc<tokio::sync::Mutex<SessionEventState>>>>,
}

impl SessionStateCache {
    fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    fn get(
        &self,
        key: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, String, Arc<tokio::sync::Mutex<SessionEventState>>>>
    {
        self.inner.get(key)
    }

    fn insert(&self, key: String, value: Arc<tokio::sync::Mutex<SessionEventState>>) {
        self.inner.insert(key, value);
    }

    fn remove(&self, key: &str) -> Option<(String, Arc<tokio::sync::Mutex<SessionEventState>>)> {
        self.inner.remove(key)
    }

    #[cfg(test)]
    fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedProvider {
    /// Backend/provider type
    pub provider_type: BackendType,
    /// API endpoint, if configured
    pub endpoint: Option<String>,
    /// API key, if configured
    pub api_key: Option<String>,
    /// Which config system this was resolved from, for logging
    pub source: &'static str,
}

pub struct AgentManager {
    request_state: Arc<DashMap<String, RequestState>>,
    // TODO: invalidate agent_cache entries on kiln hot-swap (multi-kiln support)
    agent_cache: AgentCache,
    pub(crate) model_cache: Arc<DashMap<String, (Vec<String>, Instant)>>,
    kiln_manager: Arc<KilnManager>,
    session_manager: Arc<SessionManager>,
    background_manager: Arc<BackgroundJobManager>,
    session_states: SessionStateCache,
    pending_permissions: Arc<DashMap<String, HashMap<PermissionId, PendingPermission>>>,
    mcp_gateway: Option<Arc<tokio::sync::RwLock<crucible_tools::mcp_gateway::McpGatewayManager>>>,
    llm_config: Option<crucible_config::LlmConfig>,
    acp_config: Option<AcpConfig>,
    permission_config: Option<PermissionConfig>,
    plugin_loader: Option<Arc<Mutex<Option<DaemonPluginLoader>>>>,
    tool_dispatcher: Arc<dyn ToolDispatcher>,
}

/// Parameters for creating an AgentManager.
pub struct AgentManagerParams {
    pub kiln_manager: Arc<KilnManager>,
    pub session_manager: Arc<SessionManager>,
    pub background_manager: Arc<BackgroundJobManager>,
    pub mcp_gateway:
        Option<Arc<tokio::sync::RwLock<crucible_tools::mcp_gateway::McpGatewayManager>>>,
    pub llm_config: Option<crucible_config::LlmConfig>,
    pub acp_config: Option<AcpConfig>,
    pub permission_config: Option<PermissionConfig>,
    pub plugin_loader: Option<Arc<Mutex<Option<DaemonPluginLoader>>>>,
    pub workspace_tools: Arc<WorkspaceTools>,
}

impl AgentManager {
    pub fn new(params: AgentManagerParams) -> Self {
        let tool_dispatcher: Arc<dyn ToolDispatcher> =
            Arc::new(DaemonToolDispatcher::new(params.workspace_tools));
        Self {
            request_state: Arc::new(DashMap::new()),
            agent_cache: AgentCache::new(),
            model_cache: Arc::new(DashMap::new()),
            kiln_manager: params.kiln_manager,
            session_manager: params.session_manager,
            background_manager: params.background_manager,
            session_states: SessionStateCache::new(),
            pending_permissions: Arc::new(DashMap::new()),
            mcp_gateway: params.mcp_gateway,
            llm_config: params.llm_config,
            acp_config: params.acp_config,
            permission_config: params.permission_config,
            plugin_loader: params.plugin_loader,
            tool_dispatcher,
        }
    }

    pub fn get_session_with_agent(
        &self,
        session_id: &str,
    ) -> Result<(crucible_core::session::Session, SessionAgent), AgentError> {
        let session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        let agent = session
            .agent
            .clone()
            .ok_or_else(|| AgentError::NoAgentConfigured(session_id.to_string()))?;

        Ok((session, agent))
    }

    #[allow(dead_code)] // Future: concurrency guard for multi-client scenarios
    pub fn get_session_if_idle(
        &self,
        session_id: &str,
    ) -> Result<crucible_core::session::Session, AgentError> {
        if self.request_state.contains_key(session_id) {
            return Err(AgentError::ConcurrentRequest(session_id.to_string()));
        }
        self.session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))
    }

    pub fn invalidate_agent_cache(&self, session_id: &str) {
        self.agent_cache.remove(session_id);
    }

    pub fn invalidate_model_cache(&self) {
        self.model_cache.clear();
    }

    pub fn cleanup_session(&self, session_id: &str) {
        if self.session_states.remove(session_id).is_some() {
            debug!(session_id = %session_id, "Cleaned up Lua state for session");
        }

        self.agent_cache.remove(session_id);

        if let Some((_, mut state)) = self.request_state.remove(session_id) {
            if let Some(cancel_tx) = state.cancel_tx.take() {
                let _ = cancel_tx.send(());
            }
        }

        if self.pending_permissions.remove(session_id).is_some() {
            debug!(session_id = %session_id, "Cleaned up pending permissions for session");
        }
    }

    #[allow(dead_code)]
    pub fn await_permission(
        &self,
        session_id: &str,
        request: PermRequest,
    ) -> (PermissionId, oneshot::Receiver<PermResponse>) {
        let permission_id = format!("perm-{}", uuid::Uuid::new_v4());
        let (response_tx, response_rx) = oneshot::channel();

        let pending = PendingPermission {
            request,
            response_tx,
        };

        self.pending_permissions
            .entry(session_id.to_string())
            .or_default()
            .insert(permission_id.clone(), pending);

        debug!(
            session_id = %session_id,
            permission_id = %permission_id,
            "Created pending permission request"
        );

        (permission_id, response_rx)
    }

    pub fn respond_to_permission(
        &self,
        session_id: &str,
        permission_id: &str,
        response: PermResponse,
    ) -> Result<(), AgentError> {
        let mut session_permissions = self
            .pending_permissions
            .get_mut(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        let pending = session_permissions
            .remove(permission_id)
            .ok_or_else(|| AgentError::PermissionNotFound(permission_id.to_string()))?;

        let _ = pending.response_tx.send(response);

        debug!(
            session_id = %session_id,
            permission_id = %permission_id,
            "Responded to permission request"
        );

        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_pending_permission(
        &self,
        session_id: &str,
        permission_id: &str,
    ) -> Option<PermRequest> {
        self.pending_permissions
            .get(session_id)
            .and_then(|perms| perms.get(permission_id).map(|p| p.request.clone()))
    }

    #[allow(dead_code)]
    pub fn list_pending_permissions(&self, session_id: &str) -> Vec<(PermissionId, PermRequest)> {
        self.pending_permissions
            .get(session_id)
            .map(|perms| {
                perms
                    .iter()
                    .map(|(id, p)| (id.clone(), p.request.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_or_create_session_state(&self, session_id: &str) -> Arc<Mutex<SessionEventState>> {
        if let Some(state) = self.session_states.get(session_id) {
            return state.clone();
        }

        let lua = Lua::new();
        let registry = LuaScriptHandlerRegistry::new();
        let permission_hooks = Arc::new(StdMutex::new(Vec::new()));
        let permission_functions = Arc::new(StdMutex::new(HashMap::new()));

        if let Err(e) = register_crucible_on_api(
            &lua,
            registry.runtime_handlers(),
            registry.handler_functions(),
        ) {
            error!(session_id = %session_id, error = %e, "Failed to register crucible.on API");
        }

        if let Err(e) = register_permission_hook_api(
            &lua,
            permission_hooks.clone(),
            permission_functions.clone(),
        ) {
            error!(session_id = %session_id, error = %e, "Failed to register crucible.permissions API");
        }

        let mut reactor = Reactor::new();
        if let Some(kiln_path) = self
            .session_manager
            .get_session(session_id)
            .map(|s| s.kiln.clone())
        {
            discover_and_register_lua_handlers(&mut reactor, &kiln_path, session_id);
        }

        let state = Arc::new(Mutex::new(SessionEventState {
            lua,
            registry,
            permission_hooks,
            permission_functions,
            reactor,
        }));
        self.session_states
            .insert(session_id.to_string(), state.clone());
        state
    }

    fn get_session(&self, session_id: &str) -> Result<crucible_core::session::Session, AgentError> {
        self.session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))
    }

    pub fn build_available_agents(&self) -> HashMap<String, AgentProfile> {
        let mut available = default_agent_profiles();
        if let Some(config) = &self.acp_config {
            for name in config.agents.keys() {
                if let Some(resolved) = resolve_agent_profile(name, &config.agents, &available) {
                    available.insert(name.clone(), resolved);
                }
            }
        }
        available
    }

    pub async fn configure_agent(
        &self,
        session_id: &str,
        agent: SessionAgent,
    ) -> Result<(), AgentError> {
        let mut session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        session.agent = Some(agent.clone());

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;

        info!(
            session_id = %session_id,
            model = %agent.model,
            provider = %agent.provider,
            "Agent configured for session"
        );

        Ok(())
    }
}

mod messaging;
mod models;
mod precognition;

#[cfg(test)]
mod tests;
