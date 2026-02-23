//! Agent lifecycle management for the daemon.

use crate::agent_factory::{create_agent_from_session_config, AgentFactoryError};
use crate::background_manager::{BackgroundJobManager, SubagentContext};
use crate::event_emitter::emit_event;
use crate::kiln_manager::KilnManager;
use crate::llm_hooks::{LlmHook as _, LlmHookChain};
use crate::multi_kiln_search::{search_across_kilns, KilnSearchSource};
use crate::permission_bridge::{DaemonPermissionGate, PermissionPromptCallback};
use crate::protocol::SessionEventMessage;
use crate::session_manager::{SessionError, SessionManager};
use crate::trust_resolution::resolve_provider_trust;
use crucible_acp::discovery::default_agent_profiles;
use crucible_config::components::permissions::PermissionConfig;
use crucible_config::{AcpConfig, AgentProfile, BackendType, PatternStore};
use crucible_core::events::llm_hook_context::{PostLlmContext, PreLlmContext, PreLlmResult};
use crucible_core::events::SessionEvent;
use crucible_core::interaction::{InteractionRequest, PermRequest, PermResponse, PermissionScope};
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::AgentHandle;
use crucible_core::traits::PermissionGate;
use crucible_lua::{
    execute_permission_hooks, register_crucible_on_api, register_permission_hook_api,
    LuaScriptHandlerRegistry, PermissionHook, PermissionHookResult, PermissionRequest,
};
use dashmap::DashMap;
use futures::StreamExt;
use mlua::Lua;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Unique identifier for a pending permission request.
pub type PermissionId = String;

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
/// - `get_kiln_roots` — Get kiln root directories
/// - `get_kiln_stats` — Get kiln statistics
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
            | "get_kiln_roots"
            | "get_kiln_stats"
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

pub(crate) struct SessionLuaState {
    pub(crate) lua: Lua,
    registry: LuaScriptHandlerRegistry,
    permission_hooks: Arc<StdMutex<Vec<PermissionHook>>>,
    permission_functions: Arc<StdMutex<HashMap<String, RegistryKey>>>,
}

struct PendingPermission {
    #[allow(dead_code)]
    request: PermRequest,
    response_tx: oneshot::Sender<PermResponse>,
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
    agent_cache: Arc<DashMap<String, Arc<Mutex<BoxedAgentHandle>>>>,
    kiln_manager: Arc<KilnManager>,
    session_manager: Arc<SessionManager>,
    background_manager: Arc<BackgroundJobManager>,
    lua_states: Arc<DashMap<String, Arc<Mutex<SessionLuaState>>>>,
    pending_permissions: Arc<DashMap<String, HashMap<PermissionId, PendingPermission>>>,
    hook_chain: Arc<LlmHookChain>,
    mcp_gateway: Option<Arc<tokio::sync::RwLock<crucible_tools::mcp_gateway::McpGatewayManager>>>,
    llm_config: Option<crucible_config::LlmConfig>,
    acp_config: Option<AcpConfig>,
    permission_config: Option<PermissionConfig>,
}

impl AgentManager {
    pub fn new(
        kiln_manager: Arc<KilnManager>,
        session_manager: Arc<SessionManager>,
        background_manager: Arc<BackgroundJobManager>,
        mcp_gateway: Option<
            Arc<tokio::sync::RwLock<crucible_tools::mcp_gateway::McpGatewayManager>>,
        >,
        llm_config: Option<crucible_config::LlmConfig>,
        acp_config: Option<AcpConfig>,
        permission_config: Option<PermissionConfig>,
    ) -> Self {
        Self {
            request_state: Arc::new(DashMap::new()),
            agent_cache: Arc::new(DashMap::new()),
            kiln_manager,
            session_manager,
            background_manager,
            lua_states: Arc::new(DashMap::new()),
            pending_permissions: Arc::new(DashMap::new()),
            hook_chain: Arc::new(LlmHookChain::new()),
            mcp_gateway,
            llm_config,
            acp_config,
            permission_config,
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

    pub fn cleanup_session(&self, session_id: &str) {
        if self.lua_states.remove(session_id).is_some() {
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

    fn get_or_create_lua_state(&self, session_id: &str) -> Arc<Mutex<SessionLuaState>> {
        if let Some(state) = self.lua_states.get(session_id) {
            return state.clone();
        }

        let lua = Lua::new();
        let registry = LuaScriptHandlerRegistry::new();
        let permission_hooks = Arc::new(StdMutex::new(Vec::new()));
        let permission_functions = Arc::new(StdMutex::new(HashMap::new()));

        register_crucible_on_api(
            &lua,
            registry.runtime_handlers(),
            registry.handler_functions(),
        )
        .expect("Failed to register crucible.on API");

        register_permission_hook_api(&lua, permission_hooks.clone(), permission_functions.clone())
            .expect("Failed to register crucible.permissions API");

        let state = Arc::new(Mutex::new(SessionLuaState {
            lua,
            registry,
            permission_hooks,
            permission_functions,
        }));
        self.lua_states
            .insert(session_id.to_string(), state.clone());
        state
    }

    fn get_session(&self, session_id: &str) -> Result<crucible_core::session::Session, AgentError> {
        self.session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))
    }

    fn build_available_agents(&self) -> HashMap<String, AgentProfile> {
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

    pub async fn send_message(
        &self,
        session_id: &str,
        content: String,
        event_tx: &broadcast::Sender<SessionEventMessage>,
    ) -> Result<String, AgentError> {
        let session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        let agent_config = session
            .agent
            .clone()
            .ok_or_else(|| AgentError::NoAgentConfigured(session_id.to_string()))?;

        use dashmap::mapref::entry::Entry;
        let (cancel_tx, cancel_rx) = oneshot::channel();

        match self.request_state.entry(session_id.to_string()) {
            Entry::Occupied(_) => {
                return Err(AgentError::ConcurrentRequest(session_id.to_string()));
            }
            Entry::Vacant(e) => {
                e.insert(RequestState {
                    cancel_tx: Some(cancel_tx),
                    task_handle: None,
                    started_at: Instant::now(),
                });
            }
        }

        let event_tx_clone = event_tx.clone();
        let agent = match self
            .get_or_create_agent(
                session_id,
                &agent_config,
                &session.workspace,
                &event_tx_clone,
            )
            .await
        {
            Ok(agent) => agent,
            Err(e) => {
                self.request_state.remove(session_id);
                return Err(e);
            }
        };

        let message_id = format!("msg-{}", uuid::Uuid::new_v4());
        let original_content = content;

        if !emit_event(
            event_tx,
            SessionEventMessage::user_message(session_id, &message_id, &original_content),
        ) {
            warn!(session_id = %session_id, "No subscribers for user_message event");
        }

        let has_kiln = !session.kiln.as_os_str().is_empty();
        let content = if agent_config.precognition_enabled
            && !original_content.starts_with("/search")
            && has_kiln
        {
            let kiln_path = session.kiln.as_path();
            match self.kiln_manager.get_or_open(kiln_path).await {
                Ok(handle) => {
                    match self.kiln_manager.get_enrichment_config(kiln_path).await {
                        Some(primary_config) => {
                            match crate::embedding::get_or_create_embedding_provider(&primary_config)
                                .await
                            {
                                Ok(embedding_provider) => {
                                    match embedding_provider.embed(&original_content).await {
                                        Ok(query_embedding) => {
                                            let mut sources = vec![KilnSearchSource {
                                                kiln_path: session.kiln.clone(),
                                                knowledge_repo: handle.as_knowledge_repository(),
                                                is_primary: true,
                                            }];

                                            for connected_kiln in &session.connected_kilns {
                                                let connected_handle =
                                                    match self.kiln_manager.get_or_open(connected_kiln).await
                                                    {
                                                        Ok(handle) => handle,
                                                        Err(error) => {
                                                            warn!(
                                                                session_id = %session_id,
                                                                kiln = %connected_kiln.display(),
                                                                error = %error,
                                                                "Failed to open connected kiln for precognition"
                                                            );
                                                            continue;
                                                        }
                                                    };

                                                let Some(connected_config) = self
                                                    .kiln_manager
                                                    .get_enrichment_config(connected_kiln)
                                                    .await
                                                else {
                                                    debug!(
                                                        session_id = %session_id,
                                                        kiln = %connected_kiln.display(),
                                                        "Skipping connected kiln without enrichment config"
                                                    );
                                                    continue;
                                                };

                                                if connected_config.model_name()
                                                    != primary_config.model_name()
                                                {
                                                    warn!(
                                                        session_id = %session_id,
                                                        kiln = %connected_kiln.display(),
                                                        primary_model = primary_config.model_name(),
                                                        connected_model = connected_config.model_name(),
                                                        "Skipping connected kiln with mismatched embedding model"
                                                    );
                                                    continue;
                                                }

                                                sources.push(KilnSearchSource {
                                                    kiln_path: connected_kiln.clone(),
                                                    knowledge_repo: connected_handle
                                                        .as_knowledge_repository(),
                                                    is_primary: false,
                                                });
                                            }

                                            let provider_trust =
                                                resolve_provider_trust(&agent_config, self.llm_config.as_ref());

                                            match search_across_kilns(
                                                &sources,
                                                query_embedding,
                                                5,
                                                Some(provider_trust),
                                                &session.workspace,
                                            )
                                            .await
                                            {
                                                Ok(results) => {
                                                    let mut enriched_prompt = original_content.clone();
                                                    if !results.is_empty() {
                                                        let context = results
                                                            .iter()
                                                            .enumerate()
                                                            .map(|(i, result)| {
                                                                let title = result
                                                                    .document_id
                                                                    .0
                                                                    .split('/')
                                                                    .next_back()
                                                                    .unwrap_or(&result.document_id.0)
                                                                    .trim_end_matches(".md");

                                                                let kiln_label = result
                                                                    .kiln_path
                                                                    .as_ref()
                                                                    .filter(|path| path != &&session.kiln)
                                                                    .and_then(|path| path.file_name())
                                                                    .and_then(|name| name.to_str())
                                                                    .map(|name| format!(" [from: {name}]"))
                                                                    .unwrap_or_default();

                                                                format!(
                                                                    "## Context #{}: {}{} (similarity: {:.2})\n\n{}\n",
                                                                    i + 1,
                                                                    title,
                                                                    kiln_label,
                                                                    result.score,
                                                                    result.snippet.clone().unwrap_or_default()
                                                                )
                                                            })
                                                            .collect::<Vec<_>>()
                                                            .join("\n");

                                                        enriched_prompt = format!(
                                                            "# Context from Knowledge Base\n\n{}\n\n---\n\n# User Query\n\n{}",
                                                            context, original_content
                                                        );
                                                    }

                                                    let query_summary = original_content
                                                        .chars()
                                                        .take(100)
                                                        .collect::<String>();
                                                    let event = SessionEvent::PrecognitionComplete {
                                                        notes_count: results.len(),
                                                        query_summary: query_summary.clone(),
                                                        kilns_searched: sources.len(),
                                                        kilns_filtered: 0,
                                                        kilns_failed: 0,
                                                    };
                                                    if !emit_event(
                                                        event_tx,
                                                        SessionEventMessage::new(
                                                            session_id,
                                                            event.event_type(),
                                                            serde_json::json!({
                                                                "notes_count": results.len(),
                                                                "query_summary": query_summary,
                                                            }),
                                                        ),
                                                    ) {
                                                        warn!(
                                                            session_id = %session_id,
                                                            "No subscribers for precognition_complete event"
                                                        );
                                                    }
                                                    enriched_prompt
                                                }
                                                Err(error) => {
                                                    warn!(
                                                        session_id = %session_id,
                                                        error = %error,
                                                        "Precognition search across kilns failed"
                                                    );
                                                    let query_summary = original_content
                                                        .chars()
                                                        .take(100)
                                                        .collect::<String>();
                                                    let event = SessionEvent::PrecognitionComplete {
                                                        notes_count: 0,
                                                        query_summary: query_summary.clone(),
                                                        kilns_searched: 1,
                                                        kilns_filtered: 0,
                                                        kilns_failed: 1,
                                                    };
                                                    if !emit_event(
                                                        event_tx,
                                                        SessionEventMessage::new(
                                                            session_id,
                                                            event.event_type(),
                                                            serde_json::json!({
                                                                "notes_count": 0,
                                                                "query_summary": query_summary,
                                                            }),
                                                        ),
                                                    ) {
                                                        warn!(
                                                            session_id = %session_id,
                                                            "No subscribers for precognition_complete event"
                                                        );
                                                    }
                                                    original_content.clone()
                                                }
                                            }
                                        }
                                        Err(error) => {
                                            warn!(
                                                session_id = %session_id,
                                                error = %error,
                                                "Precognition embedding failed"
                                            );
                                            let query_summary = original_content
                                                .chars()
                                                .take(100)
                                                .collect::<String>();
                                            let event = SessionEvent::PrecognitionComplete {
                                                notes_count: 0,
                                                query_summary: query_summary.clone(),
                                                kilns_searched: 1,
                                                kilns_filtered: 0,
                                                kilns_failed: 1,
                                            };
                                            if !emit_event(
                                                event_tx,
                                                SessionEventMessage::new(
                                                    session_id,
                                                    event.event_type(),
                                                    serde_json::json!({
                                                        "notes_count": 0,
                                                        "query_summary": query_summary,
                                                    }),
                                                ),
                                            ) {
                                                warn!(
                                                    session_id = %session_id,
                                                    "No subscribers for precognition_complete event"
                                                );
                                            }
                                            original_content.clone()
                                        }
                                    }
                                }
                                Err(error) => {
                                    warn!(
                                        session_id = %session_id,
                                        error = %error,
                                        "Failed to create embedding provider for precognition"
                                    );
                                    original_content.clone()
                                }
                            }
                        }
                        None => original_content.clone(),
                    }
                }
                Err(error) => {
                    warn!(
                        session_id = %session_id,
                        error = %error,
                        "Failed to open kiln for precognition"
                    );
                    original_content.clone()
                }
            }
        } else {
            original_content.clone()
        };

        let session_id_owned = session_id.to_string();
        let message_id_clone = message_id.clone();
        let request_state = self.request_state.clone();
        let lua_state = self.get_or_create_lua_state(session_id);
        let workspace_path = session.workspace.clone();

        let pending_permissions = self.pending_permissions.clone();
        let hook_chain = self.hook_chain.clone();
        let model = agent_config.model.clone();

        let task = tokio::spawn(async move {
            let mut accumulated_response = String::new();

            tokio::select! {
                _ = cancel_rx => {
                    debug!(session_id = %session_id_owned, "Request cancelled");
                    if !emit_event(
                        &event_tx_clone,
                        SessionEventMessage::ended(&session_id_owned, "cancelled"),
                    ) {
                        warn!(session_id = %session_id_owned, "No subscribers for cancelled event");
                    }
                }
                _ = Self::execute_agent_stream(
                    agent,
                    content,
                    &session_id_owned,
                    &message_id_clone,
                    &event_tx_clone,
                    &mut accumulated_response,
                    lua_state,
                    false,
                    pending_permissions,
                    workspace_path,
                    hook_chain,
                    model,
                ) => {}
            }

            request_state.remove(&session_id_owned);
        });

        if let Some(mut state) = self.request_state.get_mut(session_id) {
            state.task_handle = Some(task);
        }

        Ok(message_id)
    }

    async fn get_or_create_agent(
        &self,
        session_id: &str,
        agent_config: &SessionAgent,
        workspace: &std::path::Path,
        event_tx: &broadcast::Sender<SessionEventMessage>,
    ) -> Result<Arc<Mutex<BoxedAgentHandle>>, AgentError> {
        if let Some(cached) = self.agent_cache.get(session_id) {
            debug!(session_id = %session_id, "Using cached agent");
            return Ok(cached.clone());
        }

        let resolved_config = if agent_config.endpoint.is_none() {
            let provider_key = agent_config
                .provider_key
                .as_deref()
                .unwrap_or_else(|| agent_config.provider.as_str());
            if let Some(provider) = self.resolve_provider_config(provider_key) {
                let mut config = agent_config.clone();
                config.endpoint = provider.endpoint;
                debug!(
                    provider_key = %provider_key,
                    endpoint = ?config.endpoint,
                    "Resolved endpoint from llm config"
                );
                config
            } else {
                agent_config.clone()
            }
        } else {
            agent_config.clone()
        };

        info!(
            session_id = %session_id,
            provider = %resolved_config.provider,
            model = %resolved_config.model,
            endpoint = ?resolved_config.endpoint,
            "Creating new agent"
        );

        let acp_permission_handler = if resolved_config.agent_type == "acp" {
            Some(self.build_acp_permission_handler(session_id, event_tx))
        } else {
            None
        };

        let session_for_factory = self.session_manager.get_session(session_id);
        let kiln_path = session_for_factory.as_ref().map(|s| s.kiln.as_path());
        let mut knowledge_repo = None;
        let mut embedding_provider = None;

        if let Some(kiln_path) = kiln_path {
            let storage = self
                .kiln_manager
                .get_or_open(kiln_path)
                .await
                .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?;
            knowledge_repo = Some(storage.as_knowledge_repository());

            if let Some(config) = self.kiln_manager.get_enrichment_config(kiln_path).await {
                embedding_provider = Some(
                    crate::embedding::get_or_create_embedding_provider(&config)
                        .await
                        .map_err(|e| AgentFactoryError::AgentBuild(e.to_string()))?,
                );
            }
        }

        let agent = create_agent_from_session_config(
            &resolved_config,
            workspace,
            kiln_path,
            Some(session_id),
            Some(self.background_manager.clone()),
            event_tx,
            self.mcp_gateway.clone(),
            acp_permission_handler,
            knowledge_repo,
            embedding_provider,
        )
        .await?;

        if resolved_config.delegation_config.is_some() {
            if let Some(session) = session_for_factory {
                let parent_session_id = session
                    .parent_session_id
                    .clone()
                    .or_else(|| Some(session.id.clone()));
                let available_agents = self.build_available_agents();
                self.background_manager.register_subagent_context(
                    session_id,
                    SubagentContext {
                        agent: resolved_config.clone(),
                        available_agents,
                        workspace: session.kiln.clone(),
                        parent_session_id,
                        parent_session_dir: Some(session.storage_path()),
                        delegator_agent_name: resolved_config.agent_name.clone(),
                        target_agent_name: None,
                        delegation_depth: 0,
                    },
                );
            }
        }

        let agent = Arc::new(Mutex::new(agent));
        self.agent_cache
            .insert(session_id.to_string(), agent.clone());

        Ok(agent)
    }

    fn build_acp_permission_handler(
        &self,
        session_id: &str,
        event_tx: &broadcast::Sender<SessionEventMessage>,
    ) -> crucible_acp::client::PermissionRequestHandler {
        let pending_permissions = self.pending_permissions.clone();
        let session_id_owned = session_id.to_string();
        let event_tx_owned = event_tx.clone();

        let ask_callback: PermissionPromptCallback = Arc::new(move |perm_request: PermRequest| {
            let pending_permissions = pending_permissions.clone();
            let session_id_owned = session_id_owned.clone();
            let event_tx_owned = event_tx_owned.clone();

            Box::pin(async move {
                let permission_id = format!("perm-{}", uuid::Uuid::new_v4());
                let (response_tx, response_rx) = oneshot::channel();

                let pending = PendingPermission {
                    request: perm_request.clone(),
                    response_tx,
                };

                pending_permissions
                    .entry(session_id_owned.clone())
                    .or_default()
                    .insert(permission_id.clone(), pending);

                let interaction_request = InteractionRequest::Permission(perm_request);
                let _ = emit_event(
                    &event_tx_owned,
                    SessionEventMessage::interaction_requested(
                        &session_id_owned,
                        &permission_id,
                        &interaction_request,
                    ),
                );

                let result =
                    tokio::time::timeout(std::time::Duration::from_secs(300), response_rx).await;

                match result {
                    Ok(Ok(response)) => response,
                    Ok(Err(_)) => {
                        if let Some(mut session_map) =
                            pending_permissions.get_mut(&session_id_owned)
                        {
                            session_map.remove(&permission_id);
                        }
                        PermResponse::deny_with_reason(
                            "Permission request channel closed before response",
                        )
                    }
                    Err(_) => {
                        if let Some(mut session_map) =
                            pending_permissions.get_mut(&session_id_owned)
                        {
                            session_map.remove(&permission_id);
                        }
                        PermResponse::deny_with_reason("Permission request timed out")
                    }
                }
            })
        });

        let gate: Arc<dyn PermissionGate> = Arc::new(
            DaemonPermissionGate::new(self.permission_config.clone(), true)
                .with_prompt_callback(ask_callback),
        );

        Arc::new(
            move |request: agent_client_protocol::RequestPermissionRequest| {
                let gate = gate.clone();

                Box::pin(async move {
                    use agent_client_protocol::{
                        PermissionOptionKind, RequestPermissionOutcome, SelectedPermissionOutcome,
                    };

                    let tool_name = request
                        .tool_call
                        .fields
                        .title
                        .as_deref()
                        .unwrap_or("acp_tool")
                        .to_string();
                    let args = request
                        .tool_call
                        .fields
                        .raw_input
                        .clone()
                        .unwrap_or(serde_json::Value::Null);

                    let permission = PermRequest::tool(tool_name, args);
                    let response = gate.request_permission(permission).await;

                    let desired_kind = if response.allowed {
                        if response.scope == PermissionScope::Project
                            || response.scope == PermissionScope::User
                            || response.scope == PermissionScope::Session
                            || response.pattern.is_some()
                        {
                            PermissionOptionKind::AllowAlways
                        } else {
                            PermissionOptionKind::AllowOnce
                        }
                    } else {
                        PermissionOptionKind::RejectOnce
                    };

                    request
                        .options
                        .iter()
                        .find(|opt| opt.kind == desired_kind)
                        .map(|opt| {
                            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                                opt.option_id.clone(),
                            ))
                        })
                        .unwrap_or(RequestPermissionOutcome::Cancelled)
                })
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_agent_stream(
        agent: Arc<Mutex<BoxedAgentHandle>>,
        content: String,
        session_id: &str,
        message_id: &str,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        accumulated_response: &mut String,
        lua_state: Arc<Mutex<SessionLuaState>>,
        is_continuation: bool,
        pending_permissions: Arc<DashMap<String, HashMap<PermissionId, PendingPermission>>>,
        workspace_path: PathBuf,
        hook_chain: Arc<LlmHookChain>,
        model: String,
    ) {
        let pre_ctx = PreLlmContext {
            prompt: content,
            model: if model.is_empty() {
                "unknown".to_string()
            } else {
                model.clone()
            },
            system_prompt: None,
            context_messages: vec![],
            session_id: session_id.to_string(),
        };

        let prompt = match hook_chain.run_pre_llm(pre_ctx).await {
            PreLlmResult::Continue(modified_ctx) => modified_ctx.prompt,
            PreLlmResult::Cancel(reason) => {
                if !emit_event(
                    event_tx,
                    SessionEventMessage::ended(
                        session_id,
                        format!("error: pre-llm hook cancelled request: {reason}"),
                    ),
                ) {
                    warn!(session_id = %session_id, "No subscribers for pre_llm cancellation event");
                }
                return;
            }
        };

        let mut agent_guard = agent.lock().await;
        let mut stream = agent_guard.send_message_stream(prompt);

        while let Some(result) = stream.next().await {
            match result {
                Ok(chunk) => {
                    if !chunk.delta.is_empty() {
                        // Guard: some LLM backends (both internal and ACP) re-send the
                        // full accumulated response as a final streaming delta. Detect
                        // this by checking if the incoming delta exactly matches what
                        // we've already accumulated, and skip it to prevent duplication.
                        if !accumulated_response.is_empty() && chunk.delta == *accumulated_response
                        {
                            debug!(
                                session_id = %session_id,
                                delta_len = chunk.delta.len(),
                                "Skipping duplicate full-text delta (matches accumulated response)"
                            );
                        } else {
                            accumulated_response.push_str(&chunk.delta);
                            debug!(
                                session_id = %session_id,
                                delta_len = chunk.delta.len(),
                                "Sending text_delta event"
                            );
                            let send_result = emit_event(
                                event_tx,
                                SessionEventMessage::text_delta(session_id, &chunk.delta),
                            );
                            if !send_result {
                                warn!(session_id = %session_id, "No subscribers for text_delta event");
                            }
                        }
                    }

                    if let Some(reasoning) = &chunk.reasoning {
                        debug!(session_id = %session_id, "Sending thinking event");
                        if !emit_event(
                            event_tx,
                            SessionEventMessage::thinking(session_id, reasoning),
                        ) {
                            warn!(session_id = %session_id, "No subscribers for thinking event");
                        }
                    }

                    if let Some(tool_calls) = &chunk.tool_calls {
                        for tc in tool_calls {
                            let call_id = tc
                                .id
                                .clone()
                                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                            let args = tc.arguments.clone().unwrap_or(serde_json::Value::Null);

                            if !is_safe(&tc.name) {
                                // Check if tool call matches a whitelisted pattern
                                let project_path = workspace_path.to_string_lossy();
                                let pattern_store =
                                    PatternStore::load_sync(&project_path).unwrap_or_default();

                                let pattern_matched =
                                    Self::check_pattern_match(&tc.name, &args, &pattern_store);

                                if pattern_matched {
                                    debug!(
                                        session_id = %session_id,
                                        tool = %tc.name,
                                        "Tool call matches whitelisted pattern, skipping permission prompt"
                                    );
                                } else {
                                    // Check Lua permission hooks (with 1-second timeout)
                                    let hook_result = Self::execute_permission_hooks_with_timeout(
                                        &lua_state, &tc.name, &args, session_id,
                                    )
                                    .await;

                                    match hook_result {
                                        PermissionHookResult::Allow => {
                                            debug!(
                                                session_id = %session_id,
                                                tool = %tc.name,
                                                "Lua hook allowed tool, skipping permission prompt"
                                            );
                                        }
                                        PermissionHookResult::Deny => {
                                            debug!(
                                                session_id = %session_id,
                                                tool = %tc.name,
                                                "Lua hook denied tool"
                                            );
                                            let resource_desc =
                                                Self::brief_resource_description(&args);
                                            let error_msg = format!(
                                                "Lua hook denied permission to {} {}",
                                                tc.name, resource_desc
                                            );

                                            if !emit_event(
                                                event_tx,
                                                SessionEventMessage::tool_result(
                                                    session_id,
                                                    &call_id,
                                                    &tc.name,
                                                    serde_json::json!({ "error": error_msg }),
                                                ),
                                            ) {
                                                warn!(
                                                    session_id = %session_id,
                                                    tool = %tc.name,
                                                    "No subscribers for hook denied tool_result event"
                                                );
                                            }
                                            continue;
                                        }
                                        PermissionHookResult::Prompt => {
                                            // No pattern match, no hook decision - emit permission request
                                            let perm_request =
                                                PermRequest::tool(&tc.name, args.clone());
                                            let interaction_request =
                                                InteractionRequest::Permission(
                                                    perm_request.clone(),
                                                );

                                            // Register pending permission and get receiver
                                            let permission_id =
                                                format!("perm-{}", uuid::Uuid::new_v4());
                                            let (response_tx, response_rx) = oneshot::channel();

                                            let pending = PendingPermission {
                                                request: perm_request,
                                                response_tx,
                                            };

                                            pending_permissions
                                                .entry(session_id.to_string())
                                                .or_default()
                                                .insert(permission_id.clone(), pending);

                                            debug!(
                                                session_id = %session_id,
                                                tool = %tc.name,
                                                permission_id = %permission_id,
                                                "Emitting permission request for destructive tool"
                                            );

                                            // Emit the interaction request event
                                            if !emit_event(
                                                event_tx,
                                                SessionEventMessage::interaction_requested(
                                                    session_id,
                                                    &permission_id,
                                                    &interaction_request,
                                                ),
                                            ) {
                                                warn!(
                                                    session_id = %session_id,
                                                    tool = %tc.name,
                                                    "No subscribers for permission request event"
                                                );
                                            }

                                            // Block until user responds to permission request
                                            debug!(
                                                session_id = %session_id,
                                                tool = %tc.name,
                                                permission_id = %permission_id,
                                                "Waiting for permission response"
                                            );

                                            let (permission_granted, deny_reason) =
                                                match response_rx.await {
                                                    Ok(response) => {
                                                        debug!(
                                                            session_id = %session_id,
                                                            tool = %tc.name,
                                                            permission_id = %permission_id,
                                                            allowed = response.allowed,
                                                            pattern = ?response.pattern,
                                                            "Permission response received"
                                                        );

                                                        if response.allowed {
                                                            if let Some(ref pattern) =
                                                                response.pattern
                                                            {
                                                                if response.scope
                                                                    == PermissionScope::Project
                                                                {
                                                                    if let Err(e) =
                                                                        Self::store_pattern(
                                                                            &tc.name,
                                                                            pattern,
                                                                            &project_path,
                                                                        )
                                                                    {
                                                                        warn!(
                                                                            session_id = %session_id,
                                                                            tool = %tc.name,
                                                                            pattern = %pattern,
                                                                            error = %e,
                                                                            "Failed to store pattern"
                                                                        );
                                                                    } else {
                                                                        info!(
                                                                            session_id = %session_id,
                                                                            tool = %tc.name,
                                                                            pattern = %pattern,
                                                                            "Pattern stored for future use"
                                                                        );
                                                                    }
                                                                }
                                                            }
                                                            (true, None)
                                                        } else {
                                                            (false, response.reason)
                                                        }
                                                    }
                                                    Err(_) => {
                                                        warn!(
                                                            session_id = %session_id,
                                                            tool = %tc.name,
                                                            permission_id = %permission_id,
                                                            "Permission channel dropped, treating as deny"
                                                        );
                                                        (false, None)
                                                    }
                                                };

                                            if !permission_granted {
                                                let resource_desc =
                                                    Self::brief_resource_description(&args);
                                                let error_msg = if let Some(reason) = &deny_reason {
                                                    format!(
                                                        "User denied permission to {} {}. Feedback: {}",
                                                        tc.name, resource_desc, reason
                                                    )
                                                } else {
                                                    format!(
                                                        "User denied permission to {} {}",
                                                        tc.name, resource_desc
                                                    )
                                                };

                                                debug!(
                                                    session_id = %session_id,
                                                    tool = %tc.name,
                                                    error = %error_msg,
                                                    "Permission denied, emitting error result"
                                                );

                                                // Emit tool_result with error so LLM sees the denial
                                                if !emit_event(
                                                    event_tx,
                                                    SessionEventMessage::tool_result(
                                                        session_id,
                                                        &call_id,
                                                        &tc.name,
                                                        serde_json::json!({ "error": error_msg }),
                                                    ),
                                                ) {
                                                    warn!(
                                                        session_id = %session_id,
                                                        tool = %tc.name,
                                                        "No subscribers for permission denied tool_result event"
                                                    );
                                                }

                                                continue;
                                            }
                                        }
                                    }
                                }
                            }

                            if !emit_event(
                                event_tx,
                                SessionEventMessage::tool_call(
                                    session_id, &call_id, &tc.name, args,
                                ),
                            ) {
                                warn!(session_id = %session_id, tool = %tc.name, "No subscribers for tool_call event");
                            }
                        }
                    }

                    if let Some(tool_results) = &chunk.tool_results {
                        for tr in tool_results {
                            let call_id = uuid::Uuid::new_v4().to_string();
                            let result = if let Some(err) = &tr.error {
                                serde_json::json!({ "error": err })
                            } else {
                                serde_json::json!({ "result": tr.result })
                            };
                            if !emit_event(
                                event_tx,
                                SessionEventMessage::tool_result(
                                    session_id, &call_id, &tr.name, result,
                                ),
                            ) {
                                warn!(session_id = %session_id, tool = %tr.name, "No subscribers for tool_result event");
                            }
                        }
                    }

                    if chunk.done {
                        debug!(
                            session_id = %session_id,
                            message_id = %message_id,
                            response_len = accumulated_response.len(),
                            "Sending message_complete event"
                        );
                        if !emit_event(
                            event_tx,
                            SessionEventMessage::message_complete(
                                session_id,
                                message_id,
                                accumulated_response.clone(),
                                chunk.usage.as_ref(),
                            ),
                        ) {
                            warn!(session_id = %session_id, "No subscribers for message_complete event");
                        }

                        let injection = Self::dispatch_turn_complete_handlers(
                            session_id,
                            message_id,
                            accumulated_response,
                            &lua_state,
                            is_continuation,
                        )
                        .await;

                        if let Some((injected_content, position)) = injection {
                            info!(
                                session_id = %session_id,
                                content_len = injected_content.len(),
                                position = %position,
                                "Processing handler injection"
                            );

                            if !emit_event(
                                event_tx,
                                SessionEventMessage::new(
                                    session_id,
                                    "injection_pending",
                                    serde_json::json!({
                                        "content": &injected_content,
                                        "position": &position,
                                        "is_continuation": true,
                                    }),
                                ),
                            ) {
                                warn!(session_id = %session_id, "No subscribers for injection_pending event");
                            }

                            drop(stream);
                            drop(agent_guard);

                            accumulated_response.clear();
                            let injection_message_id = format!("msg-{}", uuid::Uuid::new_v4());

                            Box::pin(Self::execute_agent_stream(
                                agent,
                                injected_content,
                                session_id,
                                &injection_message_id,
                                event_tx,
                                accumulated_response,
                                lua_state,
                                true,
                                pending_permissions.clone(),
                                workspace_path.clone(),
                                hook_chain.clone(),
                                model.clone(),
                            ))
                            .await;
                        }

                        break;
                    }
                }
                Err(e) => {
                    error!(session_id = %session_id, error = %e, "Agent stream error");
                    if !emit_event(
                        event_tx,
                        SessionEventMessage::ended(session_id, format!("error: {}", e)),
                    ) {
                        warn!(session_id = %session_id, "No subscribers for error event");
                    }
                    break;
                }
            }
        }

        let post_ctx = PostLlmContext {
            response: accumulated_response.clone(),
            model: if model.is_empty() {
                "unknown".to_string()
            } else {
                model.clone()
            },
            session_id: session_id.to_string(),
            duration_ms: 0,
            token_count: None,
        };

        hook_chain.run_post_llm(&post_ctx).await;

        let response_summary: String = accumulated_response.chars().take(200).collect();
        let post_model = post_ctx.model.clone();
        let post_event = SessionEvent::PostLlmCall {
            response_summary: response_summary.clone(),
            model: post_model.clone(),
            duration_ms: 0,
            token_count: None,
        };
        if !emit_event(
            event_tx,
            SessionEventMessage::new(
                session_id,
                post_event.event_type(),
                serde_json::json!({
                    "response_summary": response_summary,
                    "model": post_model,
                    "duration_ms": 0,
                    "token_count": Option::<u64>::None,
                }),
            ),
        ) {
            warn!(session_id = %session_id, "No subscribers for post_llm_call event");
        }
    }

    async fn dispatch_turn_complete_handlers(
        session_id: &str,
        message_id: &str,
        response: &str,
        lua_state: &Arc<Mutex<SessionLuaState>>,
        is_continuation: bool,
    ) -> Option<(String, String)> {
        use crucible_lua::ScriptHandlerResult;

        let state = lua_state.lock().await;
        let handlers = state.registry.runtime_handlers_for("turn:complete");

        if handlers.is_empty() {
            return None;
        }

        debug!(
            session_id = %session_id,
            handler_count = handlers.len(),
            is_continuation = is_continuation,
            "Dispatching turn:complete handlers"
        );

        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({
                "session_id": session_id,
                "message_id": message_id,
                "response_length": response.len(),
                "is_continuation": is_continuation,
            }),
        };

        let mut pending_injection: Option<(String, String)> = None;

        for handler in handlers {
            match state
                .registry
                .execute_runtime_handler(&state.lua, &handler.name, &event)
            {
                Ok(result) => {
                    debug!(
                        session_id = %session_id,
                        handler = %handler.name,
                        result = ?result,
                        "Handler executed"
                    );

                    if let ScriptHandlerResult::Inject { content, position } = result {
                        debug!(
                            session_id = %session_id,
                            handler = %handler.name,
                            content_len = content.len(),
                            position = %position,
                            "Handler returned inject"
                        );
                        pending_injection = Some((content, position));
                    }
                }
                Err(e) => {
                    error!(
                        session_id = %session_id,
                        handler = %handler.name,
                        error = %e,
                        "Handler failed"
                    );
                }
            }
        }

        pending_injection
    }

    fn brief_resource_description(args: &serde_json::Value) -> String {
        if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
            return path.to_string();
        }
        if let Some(file) = args.get("file").and_then(|v| v.as_str()) {
            return file.to_string();
        }
        if let Some(command) = args.get("command").and_then(|v| v.as_str()) {
            let truncated: String = command.chars().take(50).collect();
            if command.len() > 50 {
                return format!("{}...", truncated);
            }
            return truncated;
        }
        if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
            return name.to_string();
        }
        String::new()
    }

    fn check_pattern_match(
        tool_name: &str,
        args: &serde_json::Value,
        pattern_store: &PatternStore,
    ) -> bool {
        match tool_name {
            "bash" => {
                if let Some(command) = args.get("command").and_then(|v| v.as_str()) {
                    pattern_store.matches_bash(command)
                } else {
                    false
                }
            }
            "write_file" | "edit_file" | "create_note" | "update_note" | "delete_note" => {
                let path = args
                    .get("path")
                    .or_else(|| args.get("file"))
                    .or_else(|| args.get("name"))
                    .and_then(|v| v.as_str());
                if let Some(path) = path {
                    pattern_store.matches_file(path)
                } else {
                    false
                }
            }
            _ => pattern_store.matches_tool(tool_name),
        }
    }

    fn store_pattern(
        tool_name: &str,
        pattern: &str,
        project_path: &str,
    ) -> Result<(), crucible_config::PatternError> {
        let mut store = PatternStore::load_sync(project_path).unwrap_or_default();

        match tool_name {
            "bash" => store.add_bash_pattern(pattern)?,
            "write_file" | "edit_file" | "create_note" | "update_note" | "delete_note" => {
                store.add_file_pattern(pattern)?
            }
            _ => store.add_tool_pattern(pattern)?,
        }

        store.save_sync(project_path)?;
        Ok(())
    }

    async fn execute_permission_hooks_with_timeout(
        lua_state: &Arc<Mutex<SessionLuaState>>,
        tool_name: &str,
        args: &serde_json::Value,
        session_id: &str,
    ) -> PermissionHookResult {
        let file_path = args
            .get("path")
            .or_else(|| args.get("file"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let request = PermissionRequest {
            tool_name: tool_name.to_string(),
            args: args.clone(),
            file_path,
        };

        let state = lua_state.lock().await;
        let hooks_guard = state.permission_hooks.lock().unwrap();
        let functions_guard = state.permission_functions.lock().unwrap();

        if hooks_guard.is_empty() {
            return PermissionHookResult::Prompt;
        }

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(1);

        let result = execute_permission_hooks(&state.lua, &hooks_guard, &functions_guard, &request);

        if start.elapsed() > timeout {
            warn!(
                session_id = %session_id,
                tool = %tool_name,
                elapsed_ms = start.elapsed().as_millis(),
                "Permission hook exceeded 1 second timeout"
            );
            return PermissionHookResult::Prompt;
        }

        match result {
            Ok(hook_result) => hook_result,
            Err(e) => {
                warn!(
                    session_id = %session_id,
                    tool = %tool_name,
                    error = %e,
                    "Permission hook execution failed"
                );
                PermissionHookResult::Prompt
            }
        }
    }

    pub async fn cancel(&self, session_id: &str) -> bool {
        if let Some((_, mut state)) = self.request_state.remove(session_id) {
            if let Some(cancel_tx) = state.cancel_tx.take() {
                let _ = cancel_tx.send(());
            }

            if let Some(handle) = state.task_handle.take() {
                // Give task 500ms to respond to cancellation signal before force-aborting
                match tokio::time::timeout(std::time::Duration::from_millis(500), handle).await {
                    Ok(Ok(())) => debug!(session_id = %session_id, "Task completed gracefully"),
                    Ok(Err(e)) => warn!(session_id = %session_id, error = %e, "Task panicked"),
                    Err(_) => {
                        debug!(session_id = %session_id, "Task did not respond to cancellation, was aborted");
                    }
                }
            }

            info!(session_id = %session_id, "Request cancelled");
            true
        } else {
            warn!(session_id = %session_id, "No active request to cancel");
            false
        }
    }

    /// Resolve provider configuration from either config system.
    ///
    /// Checks `LlmConfig` for configured providers.
    /// Returns `None` if the provider key is not found in either system.
    fn resolve_provider_config(&self, provider_key: &str) -> Option<ResolvedProvider> {
        if let Some(llm_provider) = self
            .llm_config
            .as_ref()
            .and_then(|c| c.providers.get(provider_key))
        {
            debug!(
                provider_key = %provider_key,
                source = "llm_config",
                provider_type = %llm_provider.provider_type.as_str(),
                "Resolved provider from llm_config"
            );
            return Some(ResolvedProvider {
                provider_type: llm_provider.provider_type,
                endpoint: Some(llm_provider.endpoint()),
                api_key: llm_provider.api_key.clone(),
                source: "llm_config",
            });
        }

        debug!(
            provider_key = %provider_key,
            "Provider not found in any config"
        );
        None
    }

    /// Parse a model ID into optional provider key and model name.
    ///
    /// Splits on the first `/` and checks if the prefix matches a configured provider key.
    /// Returns `(Some(provider_key), model_name)` if the prefix is a valid provider,
    /// otherwise `(None, model_id)` to treat the entire string as a model name.
    ///
    /// # Examples
    ///
    /// - `"zai/claude-sonnet-4"` → `(Some("zai"), "claude-sonnet-4")` if "zai" is configured
    /// - `"llama3.2"` → `(None, "llama3.2")` (no `/` separator)
    /// - `"unknown/model"` → `(None, "unknown/model")` if "unknown" is not configured
    /// - `"library/llama3:latest"` → `(Some("library"), "llama3:latest")` if "library" is configured
    fn parse_provider_model(&self, model_id: &str) -> (Option<String>, String) {
        if let Some((prefix, model_name)) = model_id.split_once('/') {
            if let Some(ref llm_config) = self.llm_config {
                if llm_config.providers.contains_key(prefix) {
                    return (Some(prefix.to_string()), model_name.to_string());
                }
            }
        }
        (None, model_id.to_string())
    }

    pub async fn switch_model(
        &self,
        session_id: &str,
        model_id: &str,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<(), AgentError> {
        let model_id = model_id.trim();
        if model_id.is_empty() {
            return Err(AgentError::InvalidModelId(
                "Model ID cannot be empty".to_string(),
            ));
        }

        if self.request_state.contains_key(session_id) {
            return Err(AgentError::ConcurrentRequest(session_id.to_string()));
        }

        let mut session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        let mut agent_config = session
            .agent
            .clone()
            .ok_or_else(|| AgentError::NoAgentConfigured(session_id.to_string()))?;

        let (provider_key_opt, model_name) = self.parse_provider_model(model_id);

        if let Some(provider_key) = provider_key_opt {
            if let Some(resolved) = self.resolve_provider_config(&provider_key) {
                info!(
                    session_id = %session_id,
                    provider_key = %provider_key,
                    model = %model_name,
                    source = %resolved.source,
                    "Resolved provider '{}' via {}",
                    provider_key,
                    resolved.source,
                );

                agent_config.provider = resolved.provider_type;
                agent_config.provider_key = Some(provider_key);
                agent_config.endpoint = resolved.endpoint;
                agent_config.model = model_name;
            } else {
                info!(
                    session_id = %session_id,
                    model = %model_id,
                    "No provider config found for prefix, treating as model-only switch"
                );
                agent_config.model = model_id.to_string();
            }
        } else {
            info!(
                session_id = %session_id,
                model = %model_name,
                "Model-only switch (no provider prefix)"
            );
            agent_config.model = model_name;
        }

        session.agent = Some(agent_config.clone());

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;

        self.agent_cache.remove(session_id);

        info!(
            session_id = %session_id,
            model = %agent_config.model,
            provider = %agent_config.provider,
            "Model switched for session (agent cache invalidated)"
        );

        if let Some(tx) = event_tx {
            let _ = emit_event(
                tx,
                SessionEventMessage::model_switched(
                    session_id,
                    &agent_config.model,
                    agent_config.provider.as_str(),
                ),
            );
        }

        Ok(())
    }

    pub async fn list_models(&self, session_id: &str) -> Result<Vec<String>, AgentError> {
        use crucible_config::BackendType;

        let mut all_models = Vec::new();

        if let Some(ref llm_config) = self.llm_config {
            for (provider_key, provider_config) in &llm_config.providers {
                let models = match &provider_config.provider_type {
                    BackendType::Ollama => {
                        let endpoint = provider_config
                            .endpoint
                            .as_deref()
                            .unwrap_or(crucible_config::DEFAULT_OLLAMA_ENDPOINT);
                        match self.list_ollama_models(endpoint).await {
                            Ok(models) => models,
                            Err(e) => {
                                debug!(
                                    provider_key = %provider_key,
                                    error = %e,
                                    "Failed to list Ollama models, skipping"
                                );
                                continue;
                            }
                        }
                    }
                    _ => provider_config.effective_models(),
                };

                for model in models {
                    all_models.push(format!("{}/{}", provider_key, model));
                }
            }
        }

        if all_models.is_empty() {
            let (_, agent_config) = self.get_session_with_agent(session_id)?;

            let endpoint = agent_config
                .endpoint
                .unwrap_or_else(|| crucible_config::DEFAULT_OLLAMA_ENDPOINT.to_string());

            match agent_config.provider.as_str() {
                "ollama" => return self.list_ollama_models(&endpoint).await,
                _ => {
                    debug!(
                        provider = %agent_config.provider,
                        "Model listing not supported for provider"
                    );
                }
            }
        }

        Ok(all_models)
    }

    pub async fn set_thinking_budget(
        &self,
        session_id: &str,
        budget: i64,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<(), AgentError> {
        if self.request_state.contains_key(session_id) {
            return Err(AgentError::ConcurrentRequest(session_id.to_string()));
        }

        let (mut session, mut agent_config) = self.get_session_with_agent(session_id)?;

        agent_config.thinking_budget = Some(budget);
        session.agent = Some(agent_config.clone());

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;

        self.invalidate_agent_cache(session_id);

        info!(
            session_id = %session_id,
            budget = budget,
            "Thinking budget updated (agent cache invalidated)"
        );

        if let Some(tx) = event_tx {
            let _ = emit_event(
                tx,
                SessionEventMessage::new(
                    session_id,
                    "thinking_budget_changed",
                    serde_json::json!({ "budget": budget }),
                ),
            );
        }

        Ok(())
    }

    pub fn get_thinking_budget(&self, session_id: &str) -> Result<Option<i64>, AgentError> {
        let (_, agent_config) = self.get_session_with_agent(session_id)?;
        Ok(agent_config.thinking_budget)
    }

    pub async fn set_precognition(
        &self,
        session_id: &str,
        enabled: bool,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<(), AgentError> {
        if self.request_state.contains_key(session_id) {
            return Err(AgentError::ConcurrentRequest(session_id.to_string()));
        }

        let (mut session, mut agent_config) = self.get_session_with_agent(session_id)?;

        agent_config.precognition_enabled = enabled;
        session.agent = Some(agent_config.clone());

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;

        self.invalidate_agent_cache(session_id);

        info!(
            session_id = %session_id,
            enabled = enabled,
            "Precognition toggle updated (agent cache invalidated)"
        );

        if let Some(tx) = event_tx {
            let _ = emit_event(
                tx,
                SessionEventMessage::new(
                    session_id,
                    "precognition_toggled",
                    serde_json::json!({ "enabled": enabled }),
                ),
            );
        }

        Ok(())
    }

    pub fn get_precognition(&self, session_id: &str) -> Result<bool, AgentError> {
        let (_, agent_config) = self.get_session_with_agent(session_id)?;
        Ok(agent_config.precognition_enabled)
    }

    pub async fn set_temperature(
        &self,
        session_id: &str,
        temperature: f64,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<(), AgentError> {
        if self.request_state.contains_key(session_id) {
            return Err(AgentError::ConcurrentRequest(session_id.to_string()));
        }

        let (mut session, mut agent_config) = self.get_session_with_agent(session_id)?;

        agent_config.temperature = Some(temperature);
        session.agent = Some(agent_config.clone());

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;

        self.invalidate_agent_cache(session_id);

        info!(
            session_id = %session_id,
            temperature = temperature,
            "Temperature updated (agent cache invalidated)"
        );

        if let Some(tx) = event_tx {
            let _ = emit_event(
                tx,
                SessionEventMessage::new(
                    session_id,
                    "temperature_changed",
                    serde_json::json!({ "temperature": temperature }),
                ),
            );
        }

        Ok(())
    }

    pub fn get_temperature(&self, session_id: &str) -> Result<Option<f64>, AgentError> {
        let (_, agent_config) = self.get_session_with_agent(session_id)?;
        Ok(agent_config.temperature)
    }

    pub async fn add_notification(
        &self,
        session_id: &str,
        notification: crucible_core::types::Notification,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<(), AgentError> {
        let mut session = self.get_session(session_id)?;

        session.notifications.add(notification.clone());

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;

        info!(
            session_id = %session_id,
            notification_id = %notification.id,
            "Notification added"
        );

        if let Some(tx) = event_tx {
            let _ = emit_event(
                tx,
                SessionEventMessage::new(
                    session_id,
                    "notification_added",
                    serde_json::json!({ "notification_id": notification.id }),
                ),
            );
        }

        Ok(())
    }

    pub async fn list_notifications(
        &self,
        session_id: &str,
    ) -> Result<Vec<crucible_core::types::Notification>, AgentError> {
        let session = self.get_session(session_id)?;
        Ok(session.notifications.list())
    }

    pub async fn dismiss_notification(
        &self,
        session_id: &str,
        notification_id: &str,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<bool, AgentError> {
        let mut session = self.get_session(session_id)?;

        let success = session.notifications.dismiss(notification_id);

        if success {
            self.session_manager
                .update_session(&session)
                .await
                .map_err(AgentError::Session)?;

            info!(
                session_id = %session_id,
                notification_id = %notification_id,
                "Notification dismissed"
            );

            if let Some(tx) = event_tx {
                let _ = emit_event(
                    tx,
                    SessionEventMessage::new(
                        session_id,
                        "notification_dismissed",
                        serde_json::json!({ "notification_id": notification_id }),
                    ),
                );
            }
        }

        Ok(success)
    }

    pub async fn set_max_tokens(
        &self,
        session_id: &str,
        max_tokens: Option<u32>,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<(), AgentError> {
        if self.request_state.contains_key(session_id) {
            return Err(AgentError::ConcurrentRequest(session_id.to_string()));
        }

        let (mut session, mut agent_config) = self.get_session_with_agent(session_id)?;

        agent_config.max_tokens = max_tokens;
        session.agent = Some(agent_config.clone());

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;

        self.invalidate_agent_cache(session_id);

        info!(
            session_id = %session_id,
            max_tokens = ?max_tokens,
            "Max tokens updated (agent cache invalidated)"
        );

        if let Some(tx) = event_tx {
            let _ = emit_event(
                tx,
                SessionEventMessage::new(
                    session_id,
                    "max_tokens_changed",
                    serde_json::json!({ "max_tokens": max_tokens }),
                ),
            );
        }

        Ok(())
    }

    pub fn get_max_tokens(&self, session_id: &str) -> Result<Option<u32>, AgentError> {
        let (_, agent_config) = self.get_session_with_agent(session_id)?;
        Ok(agent_config.max_tokens)
    }

    async fn list_ollama_models(&self, endpoint: &str) -> Result<Vec<String>, AgentError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| AgentError::InvalidModelId(format!("HTTP client error: {}", e)))?;

        let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));

        #[derive(serde::Deserialize)]
        struct TagsResponse {
            models: Vec<ModelInfo>,
        }
        #[derive(serde::Deserialize)]
        struct ModelInfo {
            name: String,
        }

        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let tags: TagsResponse = resp.json().await.map_err(|e| {
                    AgentError::InvalidModelId(format!("Failed to parse models: {}", e))
                })?;
                Ok(tags.models.into_iter().map(|m| m.name).collect())
            }
            Ok(resp) => {
                debug!(status = %resp.status(), "Ollama returned non-success status");
                Ok(Vec::new())
            }
            Err(e) => {
                debug!(error = %e, "Failed to connect to Ollama");
                Ok(Vec::new())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_hooks::LlmHook;
    use crate::precognition::DaemonPrecognition;
    use crate::session_storage::FileSessionStorage;
    use async_trait::async_trait;
    use crucible_core::events::llm_hook_context::{PostLlmContext, PreLlmContext, PreLlmResult};
    use crucible_core::enrichment::EmbeddingProvider;
    use crucible_core::parser::ParsedNote;
    use crucible_core::session::SessionType;
    use crucible_core::traits::chat::{
        AgentHandle, ChatChunk, ChatResult, ChatToolCall, ChatToolResult,
    };
    use crucible_core::traits::knowledge::NoteInfo;
    use crucible_core::traits::KnowledgeRepository;
    use crucible_core::types::{DocumentId, SearchResult};
    use futures::stream::BoxStream;
    use futures::StreamExt;
    use std::collections::HashMap;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;
    use tempfile::TempDir;
    use tokio::time::{timeout, Duration};

    struct MockAgent;

    struct StreamingMockAgent {
        chunks: Vec<ChatChunk>,
    }

    struct CountingStreamAgent {
        send_calls: Arc<AtomicUsize>,
        last_prompt: Arc<StdMutex<Option<String>>>,
        response: String,
    }

    struct ModifyPromptHook {
        suffix: String,
        pre_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl LlmHook for ModifyPromptHook {
        async fn on_pre_llm(&self, mut ctx: PreLlmContext) -> anyhow::Result<PreLlmResult> {
            self.pre_calls.fetch_add(1, Ordering::SeqCst);
            ctx.prompt.push_str(&self.suffix);
            Ok(PreLlmResult::Continue(ctx))
        }

        async fn on_post_llm(&self, _ctx: &PostLlmContext) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct CancelHook {
        reason: String,
    }

    #[async_trait]
    impl LlmHook for CancelHook {
        async fn on_pre_llm(&self, _ctx: PreLlmContext) -> anyhow::Result<PreLlmResult> {
            Ok(PreLlmResult::Cancel(self.reason.clone()))
        }

        async fn on_post_llm(&self, _ctx: &PostLlmContext) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct ErrorHook;

    #[async_trait]
    impl LlmHook for ErrorHook {
        async fn on_pre_llm(&self, _ctx: PreLlmContext) -> anyhow::Result<PreLlmResult> {
            anyhow::bail!("test pre-llm hook failure")
        }

        async fn on_post_llm(&self, _ctx: &PostLlmContext) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct MockKnowledgeRepository {
        results: Vec<SearchResult>,
    }

    #[async_trait]
    impl KnowledgeRepository for MockKnowledgeRepository {
        async fn get_note_by_name(&self, _name: &str) -> crucible_core::Result<Option<ParsedNote>> {
            Ok(None)
        }

        async fn list_notes(&self, _path: Option<&str>) -> crucible_core::Result<Vec<NoteInfo>> {
            Ok(vec![])
        }

        async fn search_vectors(
            &self,
            _vector: Vec<f32>,
        ) -> crucible_core::Result<Vec<SearchResult>> {
            Ok(self.results.clone())
        }
    }

    struct MockEmbeddingProvider {
        should_fail: bool,
    }

    #[async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            if self.should_fail {
                return Err(anyhow::anyhow!("embedding failed"));
            }
            Ok(vec![0.1, 0.2, 0.3])
        }

        async fn embed_batch(&self, _texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
            if self.should_fail {
                return Err(anyhow::anyhow!("batch embedding failed"));
            }
            Ok(vec![vec![0.1, 0.2, 0.3]])
        }

        fn model_name(&self) -> &str {
            "mock-model"
        }

        fn dimensions(&self) -> usize {
            3
        }

        fn provider_name(&self) -> &str {
            "mock"
        }

        async fn list_models(&self) -> anyhow::Result<Vec<String>> {
            Ok(vec!["mock-model".to_string()])
        }
    }

    #[async_trait::async_trait]
    impl AgentHandle for MockAgent {
        fn send_message_stream(&mut self, _: String) -> BoxStream<'static, ChatResult<ChatChunk>> {
            Box::pin(futures::stream::empty())
        }

        fn is_connected(&self) -> bool {
            true
        }

        async fn set_mode_str(&mut self, _: &str) -> ChatResult<()> {
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl AgentHandle for StreamingMockAgent {
        fn send_message_stream(&mut self, _: String) -> BoxStream<'static, ChatResult<ChatChunk>> {
            let chunks = self.chunks.clone();
            futures::stream::iter(chunks.into_iter().map(Ok)).boxed()
        }

        fn is_connected(&self) -> bool {
            true
        }

        async fn set_mode_str(&mut self, _: &str) -> ChatResult<()> {
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl AgentHandle for CountingStreamAgent {
        fn send_message_stream(&mut self, prompt: String) -> BoxStream<'static, ChatResult<ChatChunk>> {
            self.send_calls.fetch_add(1, Ordering::SeqCst);
            *self.last_prompt.lock().expect("last_prompt lock poisoned") = Some(prompt);

            let chunk = ChatChunk {
                delta: self.response.clone(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
            };

            futures::stream::iter(vec![Ok(chunk)]).boxed()
        }

        fn is_connected(&self) -> bool {
            true
        }

        async fn set_mode_str(&mut self, _: &str) -> ChatResult<()> {
            Ok(())
        }
    }

    async fn next_event_or_skip(
        event_rx: &mut broadcast::Receiver<SessionEventMessage>,
        event_name: &str,
    ) -> SessionEventMessage {
        timeout(Duration::from_secs(2), async {
            loop {
                match event_rx.recv().await {
                    Ok(event) if event.event == event_name => return event,
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(err) => {
                        panic!("event channel closed while waiting for {event_name}: {err}")
                    }
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {event_name}"))
    }

    async fn assert_no_event_until_message_complete(
        event_rx: &mut broadcast::Receiver<SessionEventMessage>,
        event_name: &str,
    ) {
        timeout(Duration::from_secs(2), async {
            loop {
                match event_rx.recv().await {
                    Ok(event) if event.event == event_name => {
                        panic!("unexpected {event_name} event: {event:?}")
                    }
                    Ok(event) if event.event == "message_complete" => return,
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(err) => {
                        panic!("event channel closed while waiting for message_complete: {err}")
                    }
                }
            }
        })
        .await
        .expect("timed out waiting for message_complete");
    }

    fn test_agent() -> SessionAgent {
        SessionAgent {
            agent_type: "internal".to_string(),
            agent_name: None,
            provider_key: Some("ollama".to_string()),
            provider: BackendType::Ollama,
            model: "llama3.2".to_string(),
            system_prompt: "You are helpful.".to_string(),
            temperature: Some(0.7),
            max_tokens: None,
            max_context_tokens: None,
            thinking_budget: None,
            endpoint: None,
            env_overrides: HashMap::new(),
            mcp_servers: Vec::new(),
            agent_card_name: None,
            capabilities: None,
            agent_description: None,
            delegation_config: None,
            precognition_enabled: false,
        }
    }

    fn create_test_agent_manager(session_manager: Arc<SessionManager>) -> AgentManager {
        let (event_tx, _) = broadcast::channel(16);
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
        AgentManager::new(
            Arc::new(KilnManager::new()),
            session_manager,
            background_manager,
            None,
            None,
            None,
            None,
        )
    }

    fn create_test_agent_manager_with_providers(
        session_manager: Arc<SessionManager>,
        llm_config: crucible_config::LlmConfig,
    ) -> AgentManager {
        let (event_tx, _) = broadcast::channel(16);
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
        AgentManager::new(
            Arc::new(KilnManager::new()),
            session_manager,
            background_manager,
            None,
            Some(llm_config),
            None,
            None,
        )
    }

    #[tokio::test]
    async fn test_configure_agent() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let updated = session_manager.get_session(&session.id).unwrap();
        assert!(updated.agent.is_some());
        assert_eq!(updated.agent.as_ref().unwrap().model, "llama3.2");
    }

    #[tokio::test]
    async fn test_configure_agent_not_found() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let result = agent_manager
            .configure_agent("nonexistent", test_agent())
            .await;

        assert!(matches!(result, Err(AgentError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_send_message_no_agent() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager);
        let (event_tx, _) = broadcast::channel(16);

        let result = agent_manager
            .send_message(&session.id, "hello".to_string(), &event_tx)
            .await;

        assert!(matches!(result, Err(AgentError::NoAgentConfigured(_))));
    }

    #[tokio::test]
    async fn test_cancel_nonexistent() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let cancelled = agent_manager.cancel("nonexistent").await;
        assert!(!cancelled);
    }

    #[tokio::test]
    async fn test_switch_model() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let updated = session_manager.get_session(&session.id).unwrap();
        assert_eq!(updated.agent.as_ref().unwrap().model, "llama3.2");

        agent_manager
            .switch_model(&session.id, "gpt-4", None)
            .await
            .unwrap();

        let updated = session_manager.get_session(&session.id).unwrap();
        assert_eq!(updated.agent.as_ref().unwrap().model, "gpt-4");
    }

    #[tokio::test]
    async fn test_switch_model_no_agent() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager);

        let result = agent_manager.switch_model(&session.id, "gpt-4", None).await;

        assert!(matches!(result, Err(AgentError::NoAgentConfigured(_))));
    }

    #[tokio::test]
    async fn test_switch_model_session_not_found() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let result = agent_manager
            .switch_model("nonexistent", "gpt-4", None)
            .await;

        assert!(matches!(result, Err(AgentError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_switch_model_rejects_empty_model_id() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let result = agent_manager.switch_model("any-session", "", None).await;
        assert!(matches!(result, Err(AgentError::InvalidModelId(_))));

        let result = agent_manager.switch_model("any-session", "   ", None).await;
        assert!(matches!(result, Err(AgentError::InvalidModelId(_))));
    }

    #[tokio::test]
    async fn test_switch_model_rejected_during_active_request() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        agent_manager.request_state.insert(
            session.id.clone(),
            super::RequestState {
                cancel_tx: None,
                task_handle: None,
                started_at: std::time::Instant::now(),
            },
        );

        let result = agent_manager.switch_model(&session.id, "gpt-4", None).await;

        assert!(matches!(result, Err(AgentError::ConcurrentRequest(_))));

        let updated = session_manager.get_session(&session.id).unwrap();
        assert_eq!(
            updated.agent.as_ref().unwrap().model,
            "llama3.2",
            "Model should not change during active request"
        );
    }

    #[tokio::test]
    async fn test_switch_model_invalidates_cache() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(MockAgent))),
        );

        assert!(agent_manager.agent_cache.contains_key(&session.id));

        agent_manager
            .switch_model(&session.id, "gpt-4", None)
            .await
            .unwrap();

        assert!(
            !agent_manager.agent_cache.contains_key(&session.id),
            "Cache should be invalidated after model switch"
        );
    }

    #[tokio::test]
    async fn test_broadcast_send_with_no_receivers_returns_error() {
        let (tx, _rx) = broadcast::channel::<SessionEventMessage>(16);

        drop(_rx);

        let result = tx.send(SessionEventMessage::ended("test-session", "cancelled"));

        assert!(
            result.is_err(),
            "Broadcast send should return error when no receivers"
        );
    }

    #[tokio::test]
    async fn test_broadcast_send_with_receiver_succeeds() {
        let (tx, mut rx) = broadcast::channel::<SessionEventMessage>(16);

        let result = tx.send(SessionEventMessage::text_delta("test-session", "hello"));

        assert!(
            result.is_ok(),
            "Broadcast send should succeed with receiver"
        );

        let received = rx.recv().await.unwrap();
        assert_eq!(received.session_id, "test-session");
        assert_eq!(received.event, "text_delta");
    }

    #[tokio::test]
    async fn test_switch_model_multiple_times_updates_each_time() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let models = ["model-a", "model-b", "model-c", "model-d"];
        for model in models {
            agent_manager
                .switch_model(&session.id, model, None)
                .await
                .unwrap();

            let updated = session_manager.get_session(&session.id).unwrap();
            assert_eq!(
                updated.agent.as_ref().unwrap().model,
                model,
                "Model should be updated to {}",
                model
            );
            assert!(
                !agent_manager.agent_cache.contains_key(&session.id),
                "Cache should be invalidated after each switch"
            );
        }
    }

    #[tokio::test]
    async fn test_switch_model_preserves_other_agent_config() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());

        let mut agent = test_agent();
        agent.temperature = Some(0.9);
        agent.system_prompt = "Custom prompt".to_string();
        agent.provider = BackendType::Custom;

        agent_manager
            .configure_agent(&session.id, agent)
            .await
            .unwrap();

        agent_manager
            .switch_model(&session.id, "new-model", None)
            .await
            .unwrap();

        let updated = session_manager.get_session(&session.id).unwrap();
        let updated_agent = updated.agent.as_ref().unwrap();

        assert_eq!(updated_agent.model, "new-model");
        assert_eq!(updated_agent.temperature, Some(0.9));
        assert_eq!(updated_agent.system_prompt, "Custom prompt");
        assert_eq!(updated_agent.provider, BackendType::Custom);
    }

    #[tokio::test]
    async fn test_switch_model_emits_event() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let (tx, mut rx) = broadcast::channel::<SessionEventMessage>(16);

        agent_manager
            .switch_model(&session.id, "gpt-4", Some(&tx))
            .await
            .unwrap();

        let event = rx.recv().await.unwrap();
        assert_eq!(event.session_id, session.id);
        assert_eq!(event.event, "model_switched");
        assert_eq!(event.data["model_id"], "gpt-4");
        assert_eq!(event.data["provider"], "ollama");
    }

    #[tokio::test]
    async fn send_message_emits_text_delta_events_in_order() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(StreamingMockAgent {
                chunks: vec![
                    ChatChunk {
                        delta: "hello".to_string(),
                        done: false,
                        tool_calls: None,
                        tool_results: None,
                        reasoning: None,
                        usage: None,
                        subagent_events: None,
                        precognition_notes_count: None,
                    },
                    ChatChunk {
                        delta: " world".to_string(),
                        done: true,
                        tool_calls: None,
                        tool_results: None,
                        reasoning: None,
                        usage: None,
                        subagent_events: None,
                        precognition_notes_count: None,
                    },
                ],
            }))),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        let message_id = agent_manager
            .send_message(&session.id, "test".to_string(), &event_tx)
            .await
            .unwrap();

        let user_message = next_event_or_skip(&mut event_rx, "user_message").await;
        assert_eq!(user_message.data["content"], "test");
        assert_eq!(user_message.data["message_id"], message_id);

        let first_delta = next_event_or_skip(&mut event_rx, "text_delta").await;
        assert_eq!(first_delta.data["content"], "hello");

        let second_delta = next_event_or_skip(&mut event_rx, "text_delta").await;
        assert_eq!(second_delta.data["content"], " world");

        let complete = next_event_or_skip(&mut event_rx, "message_complete").await;
        assert_eq!(complete.data["message_id"], message_id);
        assert_eq!(complete.data["full_response"], "hello world");
    }

    #[tokio::test]
    async fn hook_chain_pre_llm_modifies_prompt() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let mut agent_manager = create_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let pre_calls = Arc::new(AtomicUsize::new(0));
        Arc::get_mut(&mut agent_manager.hook_chain)
            .expect("hook_chain should be uniquely owned in test")
            .add_hook(Box::new(ModifyPromptHook {
                suffix: " [hooked]".to_string(),
                pre_calls: pre_calls.clone(),
            }));

        let send_calls = Arc::new(AtomicUsize::new(0));
        let last_prompt = Arc::new(StdMutex::new(None));
        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(CountingStreamAgent {
                send_calls: send_calls.clone(),
                last_prompt: last_prompt.clone(),
                response: "ok".to_string(),
            }) as BoxedAgentHandle)),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        agent_manager
            .send_message(&session.id, "hello".to_string(), &event_tx)
            .await
            .unwrap();

        let _ = next_event_or_skip(&mut event_rx, "user_message").await;
        let complete = next_event_or_skip(&mut event_rx, "message_complete").await;
        assert_eq!(complete.data["full_response"], "ok");

        assert_eq!(pre_calls.load(Ordering::SeqCst), 1);
        assert_eq!(send_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            last_prompt.lock().unwrap().clone(),
            Some("hello [hooked]".to_string())
        );
    }

    #[tokio::test]
    async fn hook_chain_post_llm_event_emitted() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let send_calls = Arc::new(AtomicUsize::new(0));
        let last_prompt = Arc::new(StdMutex::new(None));
        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(CountingStreamAgent {
                send_calls: send_calls.clone(),
                last_prompt,
                response: "post event response".to_string(),
            }) as BoxedAgentHandle)),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        agent_manager
            .send_message(&session.id, "hello".to_string(), &event_tx)
            .await
            .unwrap();

        let _ = next_event_or_skip(&mut event_rx, "user_message").await;
        let _ = next_event_or_skip(&mut event_rx, "message_complete").await;
        let post_event = next_event_or_skip(&mut event_rx, "post_llm_call").await;

        assert_eq!(post_event.data["response_summary"], "post event response");
        assert_eq!(post_event.data["model"], "llama3.2");
        assert_eq!(send_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn hook_chain_cancel_prevents_llm_call() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let mut agent_manager = create_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        Arc::get_mut(&mut agent_manager.hook_chain)
            .expect("hook_chain should be uniquely owned in test")
            .add_hook(Box::new(CancelHook {
                reason: "blocked by test hook".to_string(),
            }));

        let send_calls = Arc::new(AtomicUsize::new(0));
        let last_prompt = Arc::new(StdMutex::new(None));
        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(CountingStreamAgent {
                send_calls: send_calls.clone(),
                last_prompt,
                response: "should-not-run".to_string(),
            }) as BoxedAgentHandle)),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        agent_manager
            .send_message(&session.id, "cancel me".to_string(), &event_tx)
            .await
            .unwrap();

        let _ = next_event_or_skip(&mut event_rx, "user_message").await;
        let ended = next_event_or_skip(&mut event_rx, "ended").await;
        assert!(ended.data["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("pre-llm hook cancelled request: blocked by test hook"));

        assert_eq!(send_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn hook_chain_empty_chain_passes_through() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let send_calls = Arc::new(AtomicUsize::new(0));
        let last_prompt = Arc::new(StdMutex::new(None));
        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(CountingStreamAgent {
                send_calls: send_calls.clone(),
                last_prompt: last_prompt.clone(),
                response: "plain".to_string(),
            }) as BoxedAgentHandle)),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        agent_manager
            .send_message(&session.id, "no hooks".to_string(), &event_tx)
            .await
            .unwrap();

        let _ = next_event_or_skip(&mut event_rx, "user_message").await;
        let complete = next_event_or_skip(&mut event_rx, "message_complete").await;
        assert_eq!(complete.data["full_response"], "plain");

        assert_eq!(send_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            last_prompt.lock().unwrap().clone(),
            Some("no hooks".to_string())
        );
    }

    #[tokio::test]
    async fn hook_chain_error_fails_open() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let mut agent_manager = create_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        Arc::get_mut(&mut agent_manager.hook_chain)
            .expect("hook_chain should be uniquely owned in test")
            .add_hook(Box::new(ErrorHook));

        let send_calls = Arc::new(AtomicUsize::new(0));
        let last_prompt = Arc::new(StdMutex::new(None));
        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(CountingStreamAgent {
                send_calls: send_calls.clone(),
                last_prompt: last_prompt.clone(),
                response: "fail-open".to_string(),
            }) as BoxedAgentHandle)),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        agent_manager
            .send_message(&session.id, "keep going".to_string(), &event_tx)
            .await
            .unwrap();

        let _ = next_event_or_skip(&mut event_rx, "user_message").await;
        let complete = next_event_or_skip(&mut event_rx, "message_complete").await;
        assert_eq!(complete.data["full_response"], "fail-open");

        assert_eq!(send_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            last_prompt.lock().unwrap().clone(),
            Some("keep going".to_string())
        );
    }

    #[tokio::test]
    async fn test_precognition_skipped_when_disabled() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                Some(tmp.path().to_path_buf()),
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());
        let mut agent = test_agent();
        agent.precognition_enabled = false;
        agent_manager
            .configure_agent(&session.id, agent)
            .await
            .unwrap();

        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(StreamingMockAgent {
                chunks: vec![ChatChunk {
                    delta: "ok".to_string(),
                    done: true,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                    precognition_notes_count: None,
                }],
            }))),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        agent_manager
            .send_message(&session.id, "hello".to_string(), &event_tx)
            .await
            .unwrap();

        let _ = next_event_or_skip(&mut event_rx, "user_message").await;
        assert_no_event_until_message_complete(&mut event_rx, "precognition_complete").await;
    }

    #[tokio::test]
    async fn test_precognition_skipped_for_search_command() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                Some(tmp.path().to_path_buf()),
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());
        let mut agent = test_agent();
        agent.precognition_enabled = true;
        agent_manager
            .configure_agent(&session.id, agent)
            .await
            .unwrap();

        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(StreamingMockAgent {
                chunks: vec![ChatChunk {
                    delta: "ok".to_string(),
                    done: true,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                    precognition_notes_count: None,
                }],
            }))),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        agent_manager
            .send_message(&session.id, "/search rust".to_string(), &event_tx)
            .await
            .unwrap();

        let _ = next_event_or_skip(&mut event_rx, "user_message").await;
        assert_no_event_until_message_complete(&mut event_rx, "precognition_complete").await;
    }

    #[tokio::test]
    async fn test_precognition_skipped_when_no_kiln() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                std::path::PathBuf::new(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());
        let mut agent = test_agent();
        agent.precognition_enabled = true;
        agent_manager
            .configure_agent(&session.id, agent)
            .await
            .unwrap();

        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(StreamingMockAgent {
                chunks: vec![ChatChunk {
                    delta: "ok".to_string(),
                    done: true,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                    precognition_notes_count: None,
                }],
            }))),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        agent_manager
            .send_message(&session.id, "hello".to_string(), &event_tx)
            .await
            .unwrap();

        let _ = next_event_or_skip(&mut event_rx, "user_message").await;
        assert_no_event_until_message_complete(&mut event_rx, "precognition_complete").await;
    }

    #[tokio::test]
    async fn test_daemon_precognition_format_matches_expected() {
        let knowledge_repo = Arc::new(MockKnowledgeRepository {
            results: vec![
                SearchResult {
                    document_id: DocumentId("docs/alpha.md".to_string()),
                    score: 0.93,
                    highlights: None,
                    snippet: Some("Alpha snippet".to_string()),
                    kiln_path: None,
                },
                SearchResult {
                    document_id: DocumentId("docs/beta.md".to_string()),
                    score: 0.79,
                    highlights: None,
                    snippet: Some("Beta snippet".to_string()),
                    kiln_path: None,
                },
            ],
        });
        let embedding_provider = Arc::new(MockEmbeddingProvider { should_fail: false });
        let precognition = DaemonPrecognition::new(knowledge_repo, embedding_provider);

        let result = precognition.enrich("test query", 5).await.unwrap();

        let expected = "# Context from Knowledge Base\n\n## Context #1: alpha (similarity: 0.93)\n\nAlpha snippet\n\n## Context #2: beta (similarity: 0.79)\n\nBeta snippet\n\n\n---\n\n# User Query\n\ntest query";
        assert_eq!(result.enriched_prompt, expected);
        assert_eq!(result.notes_count, 2);
    }

    #[tokio::test]
    async fn test_precognition_event_has_correct_notes_count() {
        let knowledge_repo = Arc::new(MockKnowledgeRepository {
            results: vec![
                SearchResult {
                    document_id: DocumentId("docs/alpha.md".to_string()),
                    score: 0.93,
                    highlights: None,
                    snippet: Some("Alpha snippet".to_string()),
                    kiln_path: None,
                },
                SearchResult {
                    document_id: DocumentId("docs/beta.md".to_string()),
                    score: 0.79,
                    highlights: None,
                    snippet: Some("Beta snippet".to_string()),
                    kiln_path: None,
                },
                SearchResult {
                    document_id: DocumentId("docs/gamma.md".to_string()),
                    score: 0.55,
                    highlights: None,
                    snippet: Some("Gamma snippet".to_string()),
                    kiln_path: None,
                },
            ],
        });
        let embedding_provider = Arc::new(MockEmbeddingProvider { should_fail: false });
        let precognition = DaemonPrecognition::new(knowledge_repo, embedding_provider);

        let result = precognition.enrich("count notes", 2).await.unwrap();

        assert_eq!(result.notes_count, 2);
        assert!(result
            .enriched_prompt
            .contains("## Context #1: alpha (similarity: 0.93)"));
        assert!(result
            .enriched_prompt
            .contains("## Context #2: beta (similarity: 0.79)"));
        assert!(!result
            .enriched_prompt
            .contains("## Context #3: gamma (similarity: 0.55)"));
    }

    #[tokio::test]
    async fn test_precognition_enrichment_error_fallback() {
        let knowledge_repo = Arc::new(MockKnowledgeRepository {
            results: vec![SearchResult {
                document_id: DocumentId("docs/alpha.md".to_string()),
                score: 0.93,
                highlights: None,
                snippet: Some("Alpha snippet".to_string()),
                kiln_path: None,
            }],
        });
        let embedding_provider = Arc::new(MockEmbeddingProvider { should_fail: true });
        let precognition = DaemonPrecognition::new(knowledge_repo, embedding_provider);

        let result = precognition.enrich("fallback query", 5).await.unwrap();

        assert_eq!(result.notes_count, 0);
        assert_eq!(result.enriched_prompt, "fallback query");
    }

    #[tokio::test]
    async fn test_precognition_empty_results_returns_original() {
        let knowledge_repo = Arc::new(MockKnowledgeRepository { results: vec![] });
        let embedding_provider = Arc::new(MockEmbeddingProvider { should_fail: false });
        let precognition = DaemonPrecognition::new(knowledge_repo, embedding_provider);

        let result = precognition.enrich("empty query", 5).await.unwrap();

        assert_eq!(result.notes_count, 0);
        assert_eq!(result.enriched_prompt, "empty query");
    }

    #[tokio::test]
    async fn test_precognition_complete_event_emitted_when_enrichment_runs() {
        crate::embedding::clear_embedding_provider_cache();

        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("crucible.toml"),
            "[enrichment]\n[enrichment.provider]\ntype = \"ollama\"\nmodel = \"nomic-embed-text\"\nbase_url = \"http://127.0.0.1:9\"\n\n[enrichment.pipeline]\n",
        )
        .unwrap();

        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                Some(tmp.path().to_path_buf()),
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());
        let mut agent = test_agent();
        agent.precognition_enabled = true;
        agent_manager
            .configure_agent(&session.id, agent)
            .await
            .unwrap();

        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(StreamingMockAgent {
                chunks: vec![ChatChunk {
                    delta: "ok".to_string(),
                    done: true,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                    precognition_notes_count: None,
                }],
            }))),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        agent_manager
            .send_message(&session.id, "hello precognition".to_string(), &event_tx)
            .await
            .unwrap();

        let _ = next_event_or_skip(&mut event_rx, "user_message").await;
        let event = next_event_or_skip(&mut event_rx, "precognition_complete").await;

        assert_eq!(event.data["notes_count"], 0);
        assert_eq!(event.data["query_summary"], "hello precognition");

        crate::embedding::clear_embedding_provider_cache();
    }

    #[tokio::test]
    async fn send_message_emits_thinking_before_text_delta() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(StreamingMockAgent {
                chunks: vec![
                    ChatChunk {
                        delta: String::new(),
                        done: false,
                        tool_calls: None,
                        tool_results: None,
                        reasoning: Some("thinking...".to_string()),
                        usage: None,
                        subagent_events: None,
                        precognition_notes_count: None,
                    },
                    ChatChunk {
                        delta: "response".to_string(),
                        done: true,
                        tool_calls: None,
                        tool_results: None,
                        reasoning: None,
                        usage: None,
                        subagent_events: None,
                        precognition_notes_count: None,
                    },
                ],
            }))),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        agent_manager
            .send_message(&session.id, "test".to_string(), &event_tx)
            .await
            .unwrap();

        let user_message = next_event_or_skip(&mut event_rx, "user_message").await;
        assert_eq!(user_message.data["content"], "test");

        let first_after_user = timeout(Duration::from_secs(2), event_rx.recv())
            .await
            .expect("timed out waiting for first post-user event")
            .expect("event channel closed");
        assert_eq!(first_after_user.event, "thinking");
        assert_eq!(first_after_user.data["content"], "thinking...");

        let second_after_user = timeout(Duration::from_secs(2), event_rx.recv())
            .await
            .expect("timed out waiting for second post-user event")
            .expect("event channel closed");
        assert_eq!(second_after_user.event, "text_delta");
        assert_eq!(second_after_user.data["content"], "response");

        let complete = next_event_or_skip(&mut event_rx, "message_complete").await;
        assert_eq!(complete.data["full_response"], "response");
    }

    #[tokio::test]
    async fn send_message_emits_tool_call_and_tool_result_events() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(StreamingMockAgent {
                chunks: vec![
                    ChatChunk {
                        delta: String::new(),
                        done: false,
                        tool_calls: Some(vec![ChatToolCall {
                            name: "read_file".to_string(),
                            arguments: Some(serde_json::json!({ "path": "test.md" })),
                            id: Some("call1".to_string()),
                        }]),
                        tool_results: None,
                        reasoning: None,
                        usage: None,
                        subagent_events: None,
                        precognition_notes_count: None,
                    },
                    ChatChunk {
                        delta: String::new(),
                        done: false,
                        tool_calls: None,
                        tool_results: Some(vec![ChatToolResult {
                            name: "read_file".to_string(),
                            result: "content".to_string(),
                            error: None,
                            call_id: Some("call1".to_string()),
                        }]),
                        reasoning: None,
                        usage: None,
                        subagent_events: None,
                        precognition_notes_count: None,
                    },
                    ChatChunk {
                        delta: "Done.".to_string(),
                        done: true,
                        tool_calls: None,
                        tool_results: None,
                        reasoning: None,
                        usage: None,
                        subagent_events: None,
                        precognition_notes_count: None,
                    },
                ],
            }))),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        let message_id = agent_manager
            .send_message(&session.id, "test".to_string(), &event_tx)
            .await
            .unwrap();

        let user_message = next_event_or_skip(&mut event_rx, "user_message").await;
        assert_eq!(user_message.data["content"], "test");

        let tool_call = next_event_or_skip(&mut event_rx, "tool_call").await;
        assert_eq!(tool_call.data["tool"], "read_file");
        assert_eq!(tool_call.data["args"]["path"], "test.md");

        let tool_result = next_event_or_skip(&mut event_rx, "tool_result").await;
        assert_eq!(tool_result.data["tool"], "read_file");
        assert_eq!(tool_result.data["result"]["result"], "content");

        let complete = next_event_or_skip(&mut event_rx, "message_complete").await;
        assert_eq!(complete.data["message_id"], message_id);
        assert_eq!(complete.data["full_response"], "Done.");
    }

    #[tokio::test]
    async fn send_message_emits_message_complete_for_empty_done_chunk() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(StreamingMockAgent {
                chunks: vec![ChatChunk {
                    delta: String::new(),
                    done: true,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                    precognition_notes_count: None,
                }],
            }))),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        let message_id = agent_manager
            .send_message(&session.id, "test".to_string(), &event_tx)
            .await
            .unwrap();

        let user_message = next_event_or_skip(&mut event_rx, "user_message").await;
        assert_eq!(user_message.data["content"], "test");

        let complete = next_event_or_skip(&mut event_rx, "message_complete").await;
        assert_eq!(complete.data["message_id"], message_id);
        assert_eq!(complete.data["full_response"], "");
    }

    mod event_dispatch {
        use super::*;
        use crucible_lua::ScriptHandlerResult;

        #[tokio::test]
        async fn handler_executes_when_event_fires() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let lua_state = agent_manager.get_or_create_lua_state("test-session");
            let state = lua_state.lock().await;

            state
                .lua
                .load(
                    r#"
                crucible.on("turn:complete", function(ctx, event)
                    return nil
                end)
            "#,
                )
                .exec()
                .unwrap();

            let handlers = state.registry.runtime_handlers_for("turn:complete");
            assert_eq!(handlers.len(), 1);

            let event = SessionEvent::Custom {
                name: "turn:complete".to_string(),
                payload: serde_json::json!({}),
            };

            let result =
                state
                    .registry
                    .execute_runtime_handler(&state.lua, &handlers[0].name, &event);
            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn multiple_handlers_run_in_priority_order() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let lua_state = agent_manager.get_or_create_lua_state("test-session");
            let state = lua_state.lock().await;

            state
                .lua
                .load(
                    r#"
                execution_order = {}
                crucible.on("turn:complete", function(ctx, event)
                    table.insert(execution_order, "first")
                    return nil
                end)
                crucible.on("turn:complete", function(ctx, event)
                    table.insert(execution_order, "second")
                    return nil
                end)
            "#,
                )
                .exec()
                .unwrap();

            let handlers = state.registry.runtime_handlers_for("turn:complete");
            assert_eq!(handlers.len(), 2);

            let event = SessionEvent::Custom {
                name: "turn:complete".to_string(),
                payload: serde_json::json!({}),
            };

            for handler in &handlers {
                let _ = state
                    .registry
                    .execute_runtime_handler(&state.lua, &handler.name, &event);
            }

            let order: Vec<String> = state.lua.load("return execution_order").eval().unwrap();
            assert_eq!(order, vec!["first", "second"]);
        }

        #[tokio::test]
        async fn handler_errors_dont_break_chain() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let lua_state = agent_manager.get_or_create_lua_state("test-session");
            let state = lua_state.lock().await;

            state
                .lua
                .load(
                    r#"
                execution_order = {}
                crucible.on("turn:complete", function(ctx, event)
                    table.insert(execution_order, "first")
                    error("intentional error")
                end)
                crucible.on("turn:complete", function(ctx, event)
                    table.insert(execution_order, "second")
                    return nil
                end)
            "#,
                )
                .exec()
                .unwrap();

            let handlers = state.registry.runtime_handlers_for("turn:complete");
            let event = SessionEvent::Custom {
                name: "turn:complete".to_string(),
                payload: serde_json::json!({}),
            };

            for handler in &handlers {
                let _result =
                    state
                        .registry
                        .execute_runtime_handler(&state.lua, &handler.name, &event);
            }

            let order: Vec<String> = state.lua.load("return execution_order").eval().unwrap();
            assert_eq!(order, vec!["first", "second"]);
        }

        #[tokio::test]
        async fn handlers_are_session_scoped() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let lua_state_1 = agent_manager.get_or_create_lua_state("session-1");
            let lua_state_2 = agent_manager.get_or_create_lua_state("session-2");

            {
                let state = lua_state_1.lock().await;
                state
                    .lua
                    .load(
                        r#"
                    crucible.on("turn:complete", function(ctx, event)
                        return nil
                    end)
                "#,
                    )
                    .exec()
                    .unwrap();
            }

            {
                let state = lua_state_2.lock().await;
                state
                    .lua
                    .load(
                        r#"
                    crucible.on("turn:complete", function(ctx, event)
                        return nil
                    end)
                    crucible.on("turn:complete", function(ctx, event)
                        return nil
                    end)
                "#,
                    )
                    .exec()
                    .unwrap();
            }

            let state_1 = lua_state_1.lock().await;
            let state_2 = lua_state_2.lock().await;

            let handlers_1 = state_1.registry.runtime_handlers_for("turn:complete");
            let handlers_2 = state_2.registry.runtime_handlers_for("turn:complete");

            assert_eq!(handlers_1.len(), 1, "Session 1 should have 1 handler");
            assert_eq!(handlers_2.len(), 2, "Session 2 should have 2 handlers");
        }

        #[tokio::test]
        async fn handler_receives_event_payload() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let lua_state = agent_manager.get_or_create_lua_state("test-session");
            let state = lua_state.lock().await;

            state
                .lua
                .load(
                    r#"
                received_session_id = nil
                received_message_id = nil
                crucible.on("turn:complete", function(ctx, event)
                    received_session_id = event.payload.session_id
                    received_message_id = event.payload.message_id
                    return nil
                end)
            "#,
                )
                .exec()
                .unwrap();

            let handlers = state.registry.runtime_handlers_for("turn:complete");
            let event = SessionEvent::Custom {
                name: "turn:complete".to_string(),
                payload: serde_json::json!({
                    "session_id": "test-123",
                    "message_id": "msg-456",
                }),
            };

            let _ = state
                .registry
                .execute_runtime_handler(&state.lua, &handlers[0].name, &event);

            let session_id: String = state.lua.load("return received_session_id").eval().unwrap();
            let message_id: String = state.lua.load("return received_message_id").eval().unwrap();
            assert_eq!(session_id, "test-123");
            assert_eq!(message_id, "msg-456");
        }

        #[tokio::test]
        async fn handler_can_return_cancel() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let lua_state = agent_manager.get_or_create_lua_state("test-session");
            let state = lua_state.lock().await;

            state
                .lua
                .load(
                    r#"
                crucible.on("turn:complete", function(ctx, event)
                    return { cancel = true, reason = "test cancel" }
                end)
            "#,
                )
                .exec()
                .unwrap();

            let handlers = state.registry.runtime_handlers_for("turn:complete");
            let event = SessionEvent::Custom {
                name: "turn:complete".to_string(),
                payload: serde_json::json!({}),
            };

            let result = state
                .registry
                .execute_runtime_handler(&state.lua, &handlers[0].name, &event)
                .unwrap();

            match result {
                ScriptHandlerResult::Cancel { reason } => {
                    assert_eq!(reason, "test cancel");
                }
                _ => panic!("Expected Cancel result"),
            }
        }

        #[tokio::test]
        async fn handler_returns_inject_collected_by_dispatch() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let lua_state = agent_manager.get_or_create_lua_state("test-session");

            // Register handler that returns inject
            {
                let state = lua_state.lock().await;
                state
                    .lua
                    .load(
                        r#"
                    crucible.on("turn:complete", function(ctx, event)
                        return { inject = { content = "Continue working" } }
                    end)
                "#,
                    )
                    .exec()
                    .unwrap();
            }

            // Dispatch handlers and check for injection
            let injection = AgentManager::dispatch_turn_complete_handlers(
                "test-session",
                "msg-123",
                "Some response",
                &lua_state,
                false, // is_continuation
            )
            .await;

            assert!(injection.is_some(), "Expected injection to be returned");
            let (content, _position) = injection.unwrap();
            assert_eq!(content, "Continue working");
        }

        #[tokio::test]
        async fn second_inject_replaces_first() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let lua_state = agent_manager.get_or_create_lua_state("test-session");

            // Register two handlers that both return inject
            {
                let state = lua_state.lock().await;
                state
                    .lua
                    .load(
                        r#"
                    crucible.on("turn:complete", function(ctx, event)
                        return { inject = { content = "First injection" } }
                    end)
                    crucible.on("turn:complete", function(ctx, event)
                        return { inject = { content = "Second injection" } }
                    end)
                "#,
                    )
                    .exec()
                    .unwrap();
            }

            // Dispatch handlers - last one should win
            let injection = AgentManager::dispatch_turn_complete_handlers(
                "test-session",
                "msg-123",
                "Some response",
                &lua_state,
                false,
            )
            .await;

            assert!(injection.is_some(), "Expected injection to be returned");
            let (content, _position) = injection.unwrap();
            assert_eq!(content, "Second injection", "Last inject should win");
        }

        #[tokio::test]
        async fn inject_includes_position() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let lua_state = agent_manager.get_or_create_lua_state("test-session");

            {
                let state = lua_state.lock().await;
                state
                    .lua
                    .load(
                        r#"
                    crucible.on("turn:complete", function(ctx, event)
                        return { inject = { content = "Suffix content", position = "user_suffix" } }
                    end)
                "#,
                    )
                    .exec()
                    .unwrap();
            }

            let injection = AgentManager::dispatch_turn_complete_handlers(
                "test-session",
                "msg-123",
                "Some response",
                &lua_state,
                false,
            )
            .await;

            assert!(injection.is_some());
            let (content, position) = injection.unwrap();
            assert_eq!(content, "Suffix content");
            assert_eq!(position, "user_suffix");
        }

        #[tokio::test]
        async fn continuation_flag_passed_to_handlers() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let lua_state = agent_manager.get_or_create_lua_state("test-session");

            // Register handler that checks is_continuation and skips if true
            {
                let state = lua_state.lock().await;
                state
                    .lua
                    .load(
                        r#"
                    received_continuation = nil
                    crucible.on("turn:complete", function(ctx, event)
                        received_continuation = event.payload.is_continuation
                        if event.payload.is_continuation then
                            return nil  -- Skip injection on continuation
                        end
                        return { inject = { content = "Should not inject" } }
                    end)
                "#,
                    )
                    .exec()
                    .unwrap();
            }

            // Dispatch with is_continuation = true
            let injection = AgentManager::dispatch_turn_complete_handlers(
                "test-session",
                "msg-123",
                "Some response",
                &lua_state,
                true, // is_continuation
            )
            .await;

            // Handler should have returned nil, so no injection
            assert!(
                injection.is_none(),
                "Handler should skip injection on continuation"
            );

            // Verify the flag was received
            let state = lua_state.lock().await;
            let received: bool = state
                .lua
                .load("return received_continuation")
                .eval()
                .unwrap();
            assert!(
                received,
                "Handler should have received is_continuation=true"
            );
        }

        #[tokio::test]
        async fn no_inject_when_handler_returns_nil() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let lua_state = agent_manager.get_or_create_lua_state("test-session");

            {
                let state = lua_state.lock().await;
                state
                    .lua
                    .load(
                        r#"
                    crucible.on("turn:complete", function(ctx, event)
                        return nil
                    end)
                "#,
                    )
                    .exec()
                    .unwrap();
            }

            let injection = AgentManager::dispatch_turn_complete_handlers(
                "test-session",
                "msg-123",
                "Some response",
                &lua_state,
                false,
            )
            .await;

            assert!(injection.is_none(), "No injection when handler returns nil");
        }
    }

    #[tokio::test]
    async fn cleanup_session_removes_lua_state() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";

        let _ = agent_manager.get_or_create_lua_state(session_id);
        assert!(
            agent_manager.lua_states.contains_key(session_id),
            "Lua state should exist after creation"
        );

        agent_manager.cleanup_session(session_id);

        assert!(
            !agent_manager.lua_states.contains_key(session_id),
            "Lua state should be removed after cleanup"
        );
    }

    #[tokio::test]
    async fn cleanup_session_removes_agent_cache() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";

        agent_manager.agent_cache.insert(
            session_id.to_string(),
            Arc::new(Mutex::new(Box::new(MockAgent))),
        );
        assert!(
            agent_manager.agent_cache.contains_key(session_id),
            "Agent cache should exist after insertion"
        );

        agent_manager.cleanup_session(session_id);

        assert!(
            !agent_manager.agent_cache.contains_key(session_id),
            "Agent cache should be removed after cleanup"
        );
    }

    #[tokio::test]
    async fn cleanup_session_cancels_pending_requests() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";
        let (cancel_tx, mut cancel_rx) = oneshot::channel();

        agent_manager.request_state.insert(
            session_id.to_string(),
            RequestState {
                cancel_tx: Some(cancel_tx),
                task_handle: None,
                started_at: Instant::now(),
            },
        );

        assert!(
            agent_manager.request_state.contains_key(session_id),
            "Request state should exist after insertion"
        );

        agent_manager.cleanup_session(session_id);

        assert!(
            !agent_manager.request_state.contains_key(session_id),
            "Request state should be removed after cleanup"
        );

        let result = cancel_rx.try_recv();
        assert!(
            result.is_ok(),
            "Cancel signal should have been sent during cleanup"
        );
    }

    mod is_safe_tests {
        use super::*;

        #[test]
        fn read_only_tools_are_safe() {
            assert!(is_safe("read_file"));
            assert!(is_safe("glob"));
            assert!(is_safe("grep"));
            assert!(is_safe("read_note"));
            assert!(is_safe("read_metadata"));
            assert!(is_safe("text_search"));
            assert!(is_safe("property_search"));
            assert!(is_safe("semantic_search"));
            assert!(is_safe("get_kiln_info"));
            assert!(is_safe("list_notes"));
            assert!(is_safe("get_kiln_roots"));
            assert!(is_safe("get_kiln_stats"));
        }

        #[test]
        fn list_jobs_is_safe() {
            assert!(is_safe("list_jobs"), "list_jobs should be safe");
        }

        #[test]
        fn write_tools_are_not_safe() {
            assert!(!is_safe("write"));
            assert!(!is_safe("edit"));
            assert!(!is_safe("bash"));
            assert!(!is_safe("create_note"));
            assert!(!is_safe("update_note"));
            assert!(!is_safe("delete_note"));
        }

        #[test]
        fn unknown_tools_are_not_safe() {
            assert!(!is_safe("unknown_tool"));
            assert!(!is_safe(""));
            assert!(!is_safe("some_custom_tool"));
            assert!(!is_safe("fs_write_file")); // MCP prefixed tools
            assert!(!is_safe("gh_create_issue"));
        }

        #[test]
        fn delegate_session_is_not_safe() {
            assert!(!is_safe("delegate_session"));
        }

        #[test]
        fn cancel_job_is_not_safe() {
            assert!(!is_safe("cancel_job"));
        }
    }

    mod brief_resource_description_tests {
        use super::*;

        #[test]
        fn extracts_path_field() {
            let args = serde_json::json!({"path": "/home/user/file.txt"});
            assert_eq!(
                AgentManager::brief_resource_description(&args),
                "/home/user/file.txt"
            );
        }

        #[test]
        fn extracts_file_field() {
            let args = serde_json::json!({"file": "config.toml"});
            assert_eq!(
                AgentManager::brief_resource_description(&args),
                "config.toml"
            );
        }

        #[test]
        fn extracts_command_field() {
            let args = serde_json::json!({"command": "echo hello"});
            assert_eq!(
                AgentManager::brief_resource_description(&args),
                "echo hello"
            );
        }

        #[test]
        fn truncates_long_commands() {
            let long_cmd = "a".repeat(100);
            let args = serde_json::json!({"command": long_cmd});
            let result = AgentManager::brief_resource_description(&args);
            assert!(result.ends_with("..."));
            assert!(result.len() <= 53); // 50 chars + "..."
        }

        #[test]
        fn extracts_name_field() {
            let args = serde_json::json!({"name": "my-note"});
            assert_eq!(AgentManager::brief_resource_description(&args), "my-note");
        }

        #[test]
        fn returns_empty_for_no_matching_fields() {
            let args = serde_json::json!({"other": "value"});
            assert_eq!(AgentManager::brief_resource_description(&args), "");
        }

        #[test]
        fn path_takes_precedence_over_other_fields() {
            let args = serde_json::json!({
                "path": "/path/to/file",
                "command": "some command",
                "name": "some name"
            });
            assert_eq!(
                AgentManager::brief_resource_description(&args),
                "/path/to/file"
            );
        }
    }

    mod pattern_matching_tests {
        use super::*;

        #[test]
        fn bash_command_matches_prefix() {
            let mut store = PatternStore::new();
            store.add_bash_pattern("npm install").unwrap();

            let args = serde_json::json!({"command": "npm install lodash"});
            assert!(AgentManager::check_pattern_match("bash", &args, &store));
        }

        #[test]
        fn bash_command_no_match() {
            let mut store = PatternStore::new();
            store.add_bash_pattern("npm install").unwrap();

            let args = serde_json::json!({"command": "rm -rf /"});
            assert!(!AgentManager::check_pattern_match("bash", &args, &store));
        }

        #[test]
        fn bash_command_missing_command_arg() {
            let store = PatternStore::new();
            let args = serde_json::json!({"other": "value"});
            assert!(!AgentManager::check_pattern_match("bash", &args, &store));
        }

        #[test]
        fn file_path_matches_prefix() {
            let mut store = PatternStore::new();
            store.add_file_pattern("src/").unwrap();

            let args = serde_json::json!({"path": "src/lib.rs"});
            assert!(AgentManager::check_pattern_match(
                "write_file",
                &args,
                &store
            ));
        }

        #[test]
        fn file_path_no_match() {
            let mut store = PatternStore::new();
            store.add_file_pattern("src/").unwrap();

            let args = serde_json::json!({"path": "tests/test.rs"});
            assert!(!AgentManager::check_pattern_match(
                "write_file",
                &args,
                &store
            ));
        }

        #[test]
        fn file_operations_check_file_patterns() {
            let mut store = PatternStore::new();
            store.add_file_pattern("notes/").unwrap();

            let args = serde_json::json!({"name": "notes/my-note.md"});

            assert!(AgentManager::check_pattern_match(
                "create_note",
                &args,
                &store
            ));
            assert!(AgentManager::check_pattern_match(
                "update_note",
                &args,
                &store
            ));
            assert!(AgentManager::check_pattern_match(
                "delete_note",
                &args,
                &store
            ));
        }

        #[test]
        fn tool_matches_always_allow() {
            let mut store = PatternStore::new();
            store.add_tool_pattern("custom_tool").unwrap();

            let args = serde_json::json!({});
            assert!(AgentManager::check_pattern_match(
                "custom_tool",
                &args,
                &store
            ));
        }

        #[test]
        fn tool_no_match() {
            let store = PatternStore::new();
            let args = serde_json::json!({});
            assert!(!AgentManager::check_pattern_match(
                "unknown_tool",
                &args,
                &store
            ));
        }

        #[test]
        fn empty_store_matches_nothing() {
            let store = PatternStore::new();

            let bash_args = serde_json::json!({"command": "npm install"});
            assert!(!AgentManager::check_pattern_match(
                "bash", &bash_args, &store
            ));

            let file_args = serde_json::json!({"path": "src/lib.rs"});
            assert!(!AgentManager::check_pattern_match(
                "write", &file_args, &store
            ));

            let tool_args = serde_json::json!({});
            assert!(!AgentManager::check_pattern_match(
                "custom_tool",
                &tool_args,
                &store
            ));
        }

        #[test]
        fn store_pattern_adds_bash_pattern() {
            let tmp = TempDir::new().unwrap();
            let project_path = tmp.path().to_string_lossy().to_string();

            AgentManager::store_pattern("bash", "cargo build", &project_path).unwrap();

            let store = PatternStore::load_sync(&project_path).unwrap();
            assert!(store.matches_bash("cargo build --release"));
        }

        #[test]
        fn store_pattern_adds_file_pattern() {
            let tmp = TempDir::new().unwrap();
            let project_path = tmp.path().to_string_lossy().to_string();

            AgentManager::store_pattern("write_file", "src/", &project_path).unwrap();

            let store = PatternStore::load_sync(&project_path).unwrap();
            assert!(store.matches_file("src/main.rs"));
        }

        #[test]
        fn store_pattern_adds_tool_pattern() {
            let tmp = TempDir::new().unwrap();
            let project_path = tmp.path().to_string_lossy().to_string();

            AgentManager::store_pattern("custom_tool", "custom_tool", &project_path).unwrap();

            let store = PatternStore::load_sync(&project_path).unwrap();
            assert!(store.matches_tool("custom_tool"));
        }

        #[test]
        fn store_pattern_rejects_star_pattern() {
            let tmp = TempDir::new().unwrap();
            let project_path = tmp.path().to_string_lossy().to_string();

            let result = AgentManager::store_pattern("bash", "*", &project_path);
            assert!(result.is_err());
        }
    }

    mod permission_channel_tests {
        use super::*;
        use crucible_core::interaction::{PermRequest, PermResponse};

        #[tokio::test]
        async fn await_permission_creates_pending_request() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let session_id = "test-session";
            let request = PermRequest::bash(["npm", "install"]);

            let (permission_id, _rx) = agent_manager.await_permission(session_id, request.clone());

            assert!(
                permission_id.starts_with("perm-"),
                "Permission ID should have perm- prefix"
            );

            let pending = agent_manager.get_pending_permission(session_id, &permission_id);
            assert!(pending.is_some(), "Pending permission should exist");
        }

        #[tokio::test]
        async fn respond_to_permission_allow_sends_response() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let session_id = "test-session";
            let request = PermRequest::bash(["npm", "install"]);

            let (permission_id, rx) = agent_manager.await_permission(session_id, request);

            // Respond with allow
            let result = agent_manager.respond_to_permission(
                session_id,
                &permission_id,
                PermResponse::allow(),
            );
            assert!(result.is_ok(), "respond_to_permission should succeed");

            // Verify response received
            let response = rx.await.expect("Should receive response");
            assert!(response.allowed, "Response should be allowed");
        }

        #[tokio::test]
        async fn respond_to_permission_deny_sends_response() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let session_id = "test-session";
            let request = PermRequest::bash(["rm", "-rf", "/"]);

            let (permission_id, rx) = agent_manager.await_permission(session_id, request);

            // Respond with deny
            let result = agent_manager.respond_to_permission(
                session_id,
                &permission_id,
                PermResponse::deny(),
            );
            assert!(result.is_ok(), "respond_to_permission should succeed");

            // Verify response received
            let response = rx.await.expect("Should receive response");
            assert!(!response.allowed, "Response should be denied");
        }

        #[tokio::test]
        async fn channel_drop_results_in_recv_error() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let session_id = "test-session";
            let request = PermRequest::bash(["npm", "install"]);

            let (permission_id, rx) = agent_manager.await_permission(session_id, request);

            // Remove the pending permission without responding (simulates cleanup/drop)
            agent_manager.pending_permissions.remove(session_id);

            // Verify the permission was removed
            let pending = agent_manager.get_pending_permission(session_id, &permission_id);
            assert!(pending.is_none(), "Pending permission should be removed");

            // The receiver should get an error when sender is dropped
            let result = rx.await;
            assert!(
                result.is_err(),
                "Receiver should error when sender is dropped"
            );
        }

        #[tokio::test]
        async fn respond_to_nonexistent_permission_returns_error() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let result = agent_manager.respond_to_permission(
                "nonexistent-session",
                "nonexistent-perm",
                PermResponse::allow(),
            );

            assert!(
                matches!(result, Err(AgentError::SessionNotFound(_))),
                "Should return SessionNotFound error"
            );
        }

        #[tokio::test]
        async fn respond_to_wrong_permission_id_returns_error() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let session_id = "test-session";
            let request = PermRequest::bash(["npm", "install"]);

            // Create a pending permission
            let (_permission_id, _rx) = agent_manager.await_permission(session_id, request);

            // Try to respond with wrong permission ID
            let result = agent_manager.respond_to_permission(
                session_id,
                "wrong-permission-id",
                PermResponse::allow(),
            );

            assert!(
                matches!(result, Err(AgentError::PermissionNotFound(_))),
                "Should return PermissionNotFound error"
            );
        }

        #[tokio::test]
        async fn list_pending_permissions_returns_all() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let session_id = "test-session";

            // Create multiple pending permissions
            let request1 = PermRequest::bash(["npm", "install"]);
            let request2 = PermRequest::write(["src", "main.rs"]);
            let request3 = PermRequest::tool("delete", serde_json::json!({"path": "/tmp/file"}));

            let (id1, _rx1) = agent_manager.await_permission(session_id, request1);
            let (id2, _rx2) = agent_manager.await_permission(session_id, request2);
            let (id3, _rx3) = agent_manager.await_permission(session_id, request3);

            let pending = agent_manager.list_pending_permissions(session_id);
            assert_eq!(pending.len(), 3, "Should have 3 pending permissions");

            let ids: Vec<_> = pending.iter().map(|(id, _)| id.clone()).collect();
            assert!(ids.contains(&id1), "Should contain first permission");
            assert!(ids.contains(&id2), "Should contain second permission");
            assert!(ids.contains(&id3), "Should contain third permission");
        }

        #[tokio::test]
        async fn list_pending_permissions_empty_for_unknown_session() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let pending = agent_manager.list_pending_permissions("unknown-session");
            assert!(
                pending.is_empty(),
                "Should return empty list for unknown session"
            );
        }

        #[tokio::test]
        async fn cleanup_session_removes_pending_permissions() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let session_id = "test-session";
            let request = PermRequest::bash(["npm", "install"]);

            let (permission_id, _rx) = agent_manager.await_permission(session_id, request);

            // Verify permission exists
            assert!(
                agent_manager
                    .get_pending_permission(session_id, &permission_id)
                    .is_some(),
                "Permission should exist before cleanup"
            );

            // Cleanup session
            agent_manager.cleanup_session(session_id);

            // Verify permission is removed
            assert!(
                agent_manager
                    .get_pending_permission(session_id, &permission_id)
                    .is_none(),
                "Permission should be removed after cleanup"
            );
        }

        #[tokio::test]
        async fn multiple_sessions_have_isolated_permissions() {
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));
            let agent_manager = create_test_agent_manager(session_manager);

            let session1 = "session-1";
            let session2 = "session-2";

            let request1 = PermRequest::bash(["npm", "install"]);
            let request2 = PermRequest::bash(["cargo", "build"]);

            let (id1, _rx1) = agent_manager.await_permission(session1, request1);
            let (id2, _rx2) = agent_manager.await_permission(session2, request2);

            // Each session should only see its own permissions
            let pending1 = agent_manager.list_pending_permissions(session1);
            let pending2 = agent_manager.list_pending_permissions(session2);

            assert_eq!(pending1.len(), 1, "Session 1 should have 1 permission");
            assert_eq!(pending2.len(), 1, "Session 2 should have 1 permission");

            assert_eq!(
                pending1[0].0, id1,
                "Session 1 should have its own permission"
            );
            assert_eq!(
                pending2[0].0, id2,
                "Session 2 should have its own permission"
            );

            // Cleanup session 1 should not affect session 2
            agent_manager.cleanup_session(session1);

            let pending1_after = agent_manager.list_pending_permissions(session1);
            let pending2_after = agent_manager.list_pending_permissions(session2);

            assert!(
                pending1_after.is_empty(),
                "Session 1 should have no permissions after cleanup"
            );
            assert_eq!(
                pending2_after.len(),
                1,
                "Session 2 should still have its permission"
            );
        }

        #[tokio::test]
        async fn test_switch_model_cross_provider() {
            use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

            let tmp = TempDir::new().unwrap();
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));

            let session = session_manager
                .create_session(
                    SessionType::Chat,
                    tmp.path().to_path_buf(),
                    None,
                    vec![],
                    None,
                )
                .await
                .unwrap();

            // Create providers config with multiple providers
            let mut providers = std::collections::HashMap::new();
            providers.insert(
                "ollama".to_string(),
                LlmProviderConfig::builder(BackendType::Ollama)
                    .endpoint("http://localhost:11434")
                    .build(),
            );
            providers.insert(
                "zai".to_string(),
                LlmProviderConfig::builder(BackendType::Anthropic)
                    .endpoint("https://api.zaiforge.com/v1")
                    .build(),
            );
            let llm_config = LlmConfig {
                default: Some("ollama".to_string()),
                providers,
            };

            let agent_manager =
                create_test_agent_manager_with_providers(session_manager.clone(), llm_config);

            // Configure with ollama provider
            agent_manager
                .configure_agent(&session.id, test_agent())
                .await
                .unwrap();

            // Switch to zai/claude-sonnet-4
            agent_manager
                .switch_model(&session.id, "zai/claude-sonnet-4", None)
                .await
                .unwrap();

            let updated = session_manager.get_session(&session.id).unwrap();
            let agent = updated.agent.as_ref().unwrap();

            assert_eq!(agent.model, "claude-sonnet-4", "Model should be updated");
            assert_eq!(
                agent.provider_key.as_deref(),
                Some("zai"),
                "Provider key should be updated"
            );
            assert_eq!(
                agent.endpoint.as_deref(),
                Some("https://api.zaiforge.com/v1"),
                "Endpoint should be updated"
            );
            assert_eq!(
                agent.provider,
                BackendType::Anthropic,
                "Provider should be updated"
            );
        }

        #[tokio::test]
        async fn test_switch_model_unprefixed_same_provider() {
            use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

            let tmp = TempDir::new().unwrap();
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));

            let session = session_manager
                .create_session(
                    SessionType::Chat,
                    tmp.path().to_path_buf(),
                    None,
                    vec![],
                    None,
                )
                .await
                .unwrap();

            let mut providers = std::collections::HashMap::new();
            providers.insert(
                "ollama".to_string(),
                LlmProviderConfig::builder(BackendType::Ollama)
                    .endpoint("http://localhost:11434")
                    .build(),
            );
            let llm_config = LlmConfig {
                default: Some("ollama".to_string()),
                providers,
            };

            let agent_manager =
                create_test_agent_manager_with_providers(session_manager.clone(), llm_config);

            agent_manager
                .configure_agent(&session.id, test_agent())
                .await
                .unwrap();

            let before = session_manager.get_session(&session.id).unwrap();
            let before_provider = before.agent.as_ref().unwrap().provider;
            let before_endpoint = before.agent.as_ref().unwrap().endpoint.clone();

            // Switch to unprefixed model (should only change model, not provider)
            agent_manager
                .switch_model(&session.id, "llama3.3", None)
                .await
                .unwrap();

            let updated = session_manager.get_session(&session.id).unwrap();
            let agent = updated.agent.as_ref().unwrap();

            assert_eq!(agent.model, "llama3.3", "Model should be updated");
            assert_eq!(
                agent.provider, before_provider,
                "Provider should remain unchanged"
            );
            assert_eq!(
                agent.endpoint, before_endpoint,
                "Endpoint should remain unchanged"
            );
        }

        #[tokio::test]
        async fn test_switch_model_unknown_provider_prefix() {
            use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

            let tmp = TempDir::new().unwrap();
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));

            let session = session_manager
                .create_session(
                    SessionType::Chat,
                    tmp.path().to_path_buf(),
                    None,
                    vec![],
                    None,
                )
                .await
                .unwrap();

            let mut providers = std::collections::HashMap::new();
            providers.insert(
                "ollama".to_string(),
                LlmProviderConfig::builder(BackendType::Ollama)
                    .endpoint("http://localhost:11434")
                    .build(),
            );
            let llm_config = LlmConfig {
                default: Some("ollama".to_string()),
                providers,
            };

            let agent_manager =
                create_test_agent_manager_with_providers(session_manager.clone(), llm_config);

            agent_manager
                .configure_agent(&session.id, test_agent())
                .await
                .unwrap();

            let before = session_manager.get_session(&session.id).unwrap();
            let before_provider = before.agent.as_ref().unwrap().provider;

            agent_manager
                .switch_model(&session.id, "unknown/model", None)
                .await
                .unwrap();

            let updated = session_manager.get_session(&session.id).unwrap();
            let agent = updated.agent.as_ref().unwrap();

            assert_eq!(
                agent.model, "unknown/model",
                "Model should be set to full string"
            );
            assert_eq!(
                agent.provider, before_provider,
                "Provider should remain unchanged"
            );
        }

        #[tokio::test]
        async fn test_switch_model_cross_provider_invalidates_cache() {
            use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

            let tmp = TempDir::new().unwrap();
            let storage = Arc::new(FileSessionStorage::new());
            let session_manager = Arc::new(SessionManager::with_storage(storage));

            let session = session_manager
                .create_session(
                    SessionType::Chat,
                    tmp.path().to_path_buf(),
                    None,
                    vec![],
                    None,
                )
                .await
                .unwrap();

            let mut providers = std::collections::HashMap::new();
            providers.insert(
                "ollama".to_string(),
                LlmProviderConfig::builder(BackendType::Ollama)
                    .endpoint("http://localhost:11434")
                    .build(),
            );
            providers.insert(
                "zai".to_string(),
                LlmProviderConfig::builder(BackendType::Anthropic)
                    .endpoint("https://api.zaiforge.com/v1")
                    .build(),
            );
            let llm_config = LlmConfig {
                default: Some("ollama".to_string()),
                providers,
            };

            let agent_manager =
                create_test_agent_manager_with_providers(session_manager.clone(), llm_config);

            agent_manager
                .configure_agent(&session.id, test_agent())
                .await
                .unwrap();

            agent_manager
                .switch_model(&session.id, "zai/claude-sonnet-4", None)
                .await
                .unwrap();

            assert!(
                !agent_manager.agent_cache.contains_key(&session.id),
                "Cache should be invalidated after cross-provider switch"
            );
        }
    }

    #[tokio::test]
    async fn test_list_models_returns_all_providers() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
        use std::collections::HashMap;

        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let mut providers = HashMap::new();
        providers.insert(
            "ollama".to_string(),
            LlmProviderConfig::builder(BackendType::Ollama)
                .endpoint("http://localhost:11434")
                .available_models(vec!["llama3.2".to_string(), "qwen2.5".to_string()])
                .build(),
        );
        providers.insert(
            "openai".to_string(),
            LlmProviderConfig::builder(BackendType::OpenAI)
                .available_models(vec!["gpt-4".to_string(), "gpt-3.5-turbo".to_string()])
                .build(),
        );

        let llm_config = LlmConfig {
            default: Some("ollama".to_string()),
            providers,
        };

        let (event_tx, _) = broadcast::channel(16);
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
        let agent_manager = AgentManager::new(
            Arc::new(KilnManager::new()),
            session_manager.clone(),
            background_manager,
            None,
            Some(llm_config),
            None,
            None,
        );

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let models = agent_manager.list_models(&session.id).await.unwrap();

        assert!(
            models.iter().any(|m| m.starts_with("openai/")),
            "Should have openai/ prefixed models, got: {:?}",
            models
        );
        assert!(
            models.contains(&"openai/gpt-4".to_string()),
            "Should contain openai/gpt-4, got: {:?}",
            models
        );
        assert!(
            models.contains(&"openai/gpt-3.5-turbo".to_string()),
            "Should contain openai/gpt-3.5-turbo, got: {:?}",
            models
        );
    }

    #[tokio::test]
    async fn test_list_models_no_llm_config() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let (event_tx, _) = broadcast::channel(16);
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
        let agent_manager = AgentManager::new(
            Arc::new(KilnManager::new()),
            session_manager.clone(),
            background_manager,
            None,
            None,
            None,
            None,
        );

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let models = agent_manager.list_models(&session.id).await.unwrap();

        assert!(
            models.is_empty() || !models[0].contains('/'),
            "Should not prefix models when llm_config is None"
        );
    }

    #[tokio::test]
    async fn test_list_models_prefixes_with_provider_key() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
        use std::collections::HashMap;

        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let mut providers = HashMap::new();
        providers.insert(
            "anthropic".to_string(),
            LlmProviderConfig::builder(BackendType::Anthropic)
                .available_models(vec!["claude-3-opus".to_string()])
                .build(),
        );

        let llm_config = LlmConfig {
            default: Some("zai-coding".to_string()),
            providers,
        };

        let (event_tx, _) = broadcast::channel(16);
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
        let agent_manager = AgentManager::new(
            Arc::new(KilnManager::new()),
            session_manager.clone(),
            background_manager,
            None,
            Some(llm_config),
            None,
            None,
        );

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let models = agent_manager.list_models(&session.id).await.unwrap();

        assert!(
            models.contains(&"anthropic/claude-3-opus".to_string()),
            "Should prefix with provider key: {:?}",
            models
        );
    }

    fn create_test_agent_manager_with_llm_config(
        session_manager: Arc<SessionManager>,
        llm_config: crucible_config::LlmConfig,
    ) -> AgentManager {
        let (event_tx, _) = broadcast::channel(16);
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
        AgentManager::new(
            Arc::new(KilnManager::new()),
            session_manager,
            background_manager,
            None,
            Some(llm_config),
            None,
            None,
        )
    }

    fn create_test_agent_manager_with_both(
        session_manager: Arc<SessionManager>,
        llm_config: crucible_config::LlmConfig,
    ) -> AgentManager {
        let (event_tx, _) = broadcast::channel(16);
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
        AgentManager::new(
            Arc::new(KilnManager::new()),
            session_manager,
            background_manager,
            None,
            Some(llm_config),
            None,
            None,
        )
    }

    #[tokio::test]
    async fn test_parse_provider_model_llm_config_found() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "zai-coding".to_string(),
            LlmProviderConfig::builder(BackendType::ZAI)
                .endpoint("https://api.z.ai/api/coding/paas/v4")
                .available_models(vec!["GLM-4.7".to_string()])
                .build(),
        );

        let llm_config = LlmConfig {
            default: Some("zai-coding".to_string()),
            providers,
        };

        let agent_manager =
            create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

        let (provider_key, model_name) = agent_manager.parse_provider_model("zai-coding/GLM-4.7");
        assert_eq!(
            provider_key.as_deref(),
            Some("zai-coding"),
            "Should find provider key in llm_config"
        );
        assert_eq!(model_name, "GLM-4.7", "Model name should be extracted");
    }

    #[tokio::test]
    async fn test_parse_provider_model_llm_config_not_found() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "zai-coding".to_string(),
            LlmProviderConfig::builder(BackendType::ZAI).build(),
        );

        let llm_config = LlmConfig {
            default: None,
            providers,
        };

        let agent_manager =
            create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

        let (provider_key, model_name) = agent_manager.parse_provider_model("unknown/model");
        assert_eq!(
            provider_key, None,
            "Should return None when prefix not in either config"
        );
        assert_eq!(
            model_name, "unknown/model",
            "Should return full string as model"
        );
    }

    #[tokio::test]
    async fn test_parse_provider_model_legacy_takes_precedence() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let mut llm_providers = std::collections::HashMap::new();
        llm_providers.insert(
            "local".to_string(),
            LlmProviderConfig::builder(BackendType::Ollama)
                .endpoint("http://different:11434")
                .build(),
        );
        let llm_config = LlmConfig {
            default: None,
            providers: llm_providers,
        };

        let agent_manager =
            create_test_agent_manager_with_both(session_manager.clone(), llm_config);

        let (provider_key, model_name) = agent_manager.parse_provider_model("local/llama3.2");
        assert_eq!(
            provider_key.as_deref(),
            Some("local"),
            "Configured provider key should be detected"
        );
        assert_eq!(model_name, "llama3.2");
    }

    #[tokio::test]
    async fn test_parse_provider_model_empty_string() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let (provider_key, model_name) = agent_manager.parse_provider_model("");
        assert_eq!(
            provider_key, None,
            "Empty string should return None provider"
        );
        assert_eq!(
            model_name, "",
            "Empty string should return empty model name"
        );
    }

    #[tokio::test]
    async fn test_parse_provider_model_trailing_slash() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "provider".to_string(),
            LlmProviderConfig::builder(BackendType::Ollama)
                .endpoint("http://localhost:11434")
                .build(),
        );
        let llm_config = LlmConfig {
            default: Some("provider".to_string()),
            providers,
        };

        let agent_manager =
            create_test_agent_manager_with_providers(session_manager.clone(), llm_config);

        let (provider_key, model_name) = agent_manager.parse_provider_model("provider/");
        assert_eq!(
            provider_key.as_deref(),
            Some("provider"),
            "Trailing slash should still parse provider"
        );
        assert_eq!(
            model_name, "",
            "Trailing slash should result in empty model name"
        );
    }

    #[tokio::test]
    async fn test_parse_provider_model_whitespace() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "provider".to_string(),
            LlmProviderConfig::builder(BackendType::Ollama)
                .endpoint("http://localhost:11434")
                .build(),
        );
        let llm_config = LlmConfig {
            default: Some("provider".to_string()),
            providers,
        };

        let agent_manager =
            create_test_agent_manager_with_providers(session_manager.clone(), llm_config);

        let (provider_key, model_name) = agent_manager.parse_provider_model("  provider/model  ");
        assert_eq!(
            provider_key, None,
            "Whitespace prefix prevents provider match (no trimming in parse)"
        );
        assert_eq!(
            model_name, "  provider/model  ",
            "Full string with whitespace returned as model"
        );
    }

    #[tokio::test]
    async fn test_parse_provider_model_case_sensitivity() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "ollama".to_string(),
            LlmProviderConfig::builder(BackendType::Ollama)
                .endpoint("http://localhost:11434")
                .build(),
        );
        let llm_config = LlmConfig {
            default: Some("ollama".to_string()),
            providers,
        };

        let agent_manager =
            create_test_agent_manager_with_providers(session_manager.clone(), llm_config);

        let (provider_key, model_name) = agent_manager.parse_provider_model("ollama/model");
        assert_eq!(
            provider_key.as_deref(),
            Some("ollama"),
            "Lowercase should match"
        );
        assert_eq!(model_name, "model");

        let (provider_key, model_name) = agent_manager.parse_provider_model("OLLAMA/model");
        assert_eq!(
            provider_key, None,
            "Uppercase should not match (case-sensitive)"
        );
        assert_eq!(model_name, "OLLAMA/model", "Full string returned as model");
    }

    #[tokio::test]
    async fn test_switch_model_zai_llm_config() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "zai-coding".to_string(),
            LlmProviderConfig::builder(BackendType::ZAI)
                .endpoint("https://api.z.ai/api/coding/paas/v4")
                .available_models(vec!["GLM-4.7".to_string()])
                .build(),
        );

        let llm_config = LlmConfig {
            default: Some("zai-coding".to_string()),
            providers,
        };

        let agent_manager =
            create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        // Switch to zai-coding/GLM-4.7
        agent_manager
            .switch_model(&session.id, "zai-coding/GLM-4.7", None)
            .await
            .unwrap();

        let updated = session_manager.get_session(&session.id).unwrap();
        let agent = updated.agent.as_ref().unwrap();

        assert_eq!(agent.model, "GLM-4.7", "Model should be updated");
        assert_eq!(
            agent.provider,
            BackendType::ZAI,
            "Provider should be set to zai via as_str()"
        );
        assert_eq!(
            agent.provider_key.as_deref(),
            Some("zai-coding"),
            "Provider key should be set"
        );
        assert_eq!(
            agent.endpoint.as_deref(),
            Some("https://api.z.ai/api/coding/paas/v4"),
            "Endpoint should be updated from llm_config"
        );
    }

    #[tokio::test]
    async fn test_switch_model_legacy_still_works() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let mut llm_providers = std::collections::HashMap::new();
        llm_providers.insert(
            "local".to_string(),
            LlmProviderConfig::builder(BackendType::Ollama)
                .endpoint("http://localhost:11434")
                .build(),
        );
        llm_providers.insert(
            "zai-coding".to_string(),
            LlmProviderConfig::builder(BackendType::ZAI)
                .endpoint("https://api.z.ai/api/coding/paas/v4")
                .build(),
        );
        let llm_config = LlmConfig {
            default: None,
            providers: llm_providers,
        };

        let agent_manager =
            create_test_agent_manager_with_both(session_manager.clone(), llm_config);

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        // Switch using legacy config key
        agent_manager
            .switch_model(&session.id, "local/llama3.3", None)
            .await
            .unwrap();

        let updated = session_manager.get_session(&session.id).unwrap();
        let agent = updated.agent.as_ref().unwrap();

        assert_eq!(agent.model, "llama3.3", "Model should be updated");
        assert_eq!(
            agent.provider,
            BackendType::Ollama,
            "Provider should be set from llm config"
        );
        assert_eq!(
            agent.provider_key.as_deref(),
            Some("local"),
            "Provider key should be set"
        );
        assert_eq!(
            agent.endpoint.as_deref(),
            Some("http://localhost:11434"),
            "Endpoint should come from llm config"
        );
    }

    #[tokio::test]
    async fn test_switch_model_llm_config_invalidates_cache() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "zai-coding".to_string(),
            LlmProviderConfig::builder(BackendType::ZAI)
                .endpoint("https://api.z.ai/api/coding/paas/v4")
                .build(),
        );

        let llm_config = LlmConfig {
            default: None,
            providers,
        };

        let agent_manager =
            create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        agent_manager
            .switch_model(&session.id, "zai-coding/GLM-4.7", None)
            .await
            .unwrap();

        assert!(
            !agent_manager.agent_cache.contains_key(&session.id),
            "Cache should be invalidated after llm_config cross-provider switch"
        );
    }

    #[tokio::test]
    async fn test_switch_model_unknown_provider_prefix() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        agent_manager
            .switch_model(&session.id, "unknown-provider/model", None)
            .await
            .unwrap();

        let updated = session_manager.get_session(&session.id).unwrap();
        let agent = updated.agent.as_ref().unwrap();

        assert_eq!(
            agent.model, "unknown-provider/model",
            "Unknown provider should be treated as model name"
        );
        assert_eq!(
            agent.provider,
            BackendType::Ollama,
            "Provider should remain unchanged (default)"
        );
        assert_eq!(
            agent.provider_key.as_deref(),
            Some("ollama"),
            "Provider key should remain unchanged"
        );
    }

    #[tokio::test]
    async fn test_switch_model_org_slash_model_format() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        agent_manager
            .switch_model(&session.id, "meta-llama/llama-3.2-1b", None)
            .await
            .unwrap();

        let updated = session_manager.get_session(&session.id).unwrap();
        let agent = updated.agent.as_ref().unwrap();

        assert_eq!(
            agent.model, "meta-llama/llama-3.2-1b",
            "Org/model format should be treated as full model name"
        );
        assert_eq!(
            agent.provider,
            BackendType::Ollama,
            "Provider should remain unchanged (default)"
        );
    }

    #[tokio::test]
    async fn test_list_models_multi_provider_with_zai() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
        use std::collections::HashMap;

        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let mut providers = HashMap::new();
        providers.insert(
            "zai-coding".to_string(),
            LlmProviderConfig::builder(BackendType::ZAI)
                .endpoint("https://api.z.ai/api/coding/paas/v4")
                .available_models(vec![
                    "GLM-5".to_string(),
                    "GLM-4.7".to_string(),
                    "GLM-4.5-Air".to_string(),
                ])
                .build(),
        );
        providers.insert(
            "openai".to_string(),
            LlmProviderConfig::builder(BackendType::OpenAI)
                .available_models(vec!["gpt-4".to_string(), "gpt-3.5-turbo".to_string()])
                .build(),
        );

        let llm_config = LlmConfig {
            default: Some("ollama".to_string()),
            providers,
        };

        let agent_manager =
            create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let models = agent_manager.list_models(&session.id).await.unwrap();

        // Verify ZAI models are present with correct prefix
        assert!(
            models.iter().any(|m| m.starts_with("zai-coding/")),
            "Should have zai-coding/ prefixed models, got: {:?}",
            models
        );
        assert!(
            models.contains(&"zai-coding/GLM-5".to_string()),
            "Should contain zai-coding/GLM-5, got: {:?}",
            models
        );
        assert!(
            models.contains(&"zai-coding/GLM-4.7".to_string()),
            "Should contain zai-coding/GLM-4.7, got: {:?}",
            models
        );
        assert!(
            models.contains(&"zai-coding/GLM-4.5-Air".to_string()),
            "Should contain zai-coding/GLM-4.5-Air, got: {:?}",
            models
        );

        // Verify OpenAI models are also present
        assert!(
            models.contains(&"openai/gpt-4".to_string()),
            "Should contain openai/gpt-4, got: {:?}",
            models
        );
    }

    #[tokio::test]
    async fn test_list_models_legacy_providers_config() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "openai".to_string(),
            LlmProviderConfig::builder(BackendType::OpenAI)
                .available_models(vec![
                    "gpt-4".to_string(),
                    "text-embedding-3-small".to_string(),
                ])
                .build(),
        );
        providers.insert(
            "anthropic".to_string(),
            LlmProviderConfig::builder(BackendType::Anthropic)
                .available_models(vec!["claude-3-opus".to_string()])
                .build(),
        );
        let llm_config = LlmConfig {
            default: Some("openai".to_string()),
            providers,
        };
        let agent_manager =
            create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let models = agent_manager.list_models(&session.id).await.unwrap();

        assert!(
            models.contains(&"openai/gpt-4".to_string()),
            "Should contain openai/gpt-4 from legacy config, got: {:?}",
            models
        );
        assert!(
            models.contains(&"openai/text-embedding-3-small".to_string()),
            "Should contain openai/text-embedding-3-small from legacy config, got: {:?}",
            models
        );
        assert!(
            models.contains(&"anthropic/claude-3-opus".to_string()),
            "Should contain anthropic/claude-3-opus from legacy config, got: {:?}",
            models
        );
    }

    #[tokio::test]
    async fn test_list_models_both_configs() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
        use std::collections::HashMap;

        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let mut llm_providers = HashMap::new();
        llm_providers.insert(
            "legacy-openai".to_string(),
            LlmProviderConfig::builder(BackendType::OpenAI)
                .available_models(vec!["gpt-3.5-turbo".to_string()])
                .build(),
        );
        llm_providers.insert(
            "new-anthropic".to_string(),
            LlmProviderConfig::builder(BackendType::Anthropic)
                .available_models(vec!["claude-sonnet-4".to_string()])
                .build(),
        );

        let llm_config = LlmConfig {
            default: Some("new-anthropic".to_string()),
            providers: llm_providers,
        };

        let agent_manager =
            create_test_agent_manager_with_both(session_manager.clone(), llm_config);

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let models = agent_manager.list_models(&session.id).await.unwrap();

        assert!(
            models.contains(&"new-anthropic/claude-sonnet-4".to_string()),
            "Should contain new-anthropic/claude-sonnet-4 from LlmConfig, got: {:?}",
            models
        );
        assert!(
            models.contains(&"legacy-openai/gpt-3.5-turbo".to_string()),
            "Should contain legacy-openai/gpt-3.5-turbo, got: {:?}",
            models
        );
    }

    #[tokio::test]
    async fn test_switch_model_to_zai_provider() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
        use std::collections::HashMap;

        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let mut providers = HashMap::new();
        providers.insert(
            "openai".to_string(),
            LlmProviderConfig::builder(BackendType::OpenAI)
                .model("gpt-4")
                .build(),
        );
        providers.insert(
            "zai-coding".to_string(),
            LlmProviderConfig::builder(BackendType::ZAI)
                .endpoint("https://api.z.ai/api/coding/paas/v4")
                .available_models(vec![
                    "GLM-5".to_string(),
                    "GLM-4.7".to_string(),
                    "GLM-4.5-Air".to_string(),
                ])
                .build(),
        );

        let llm_config = LlmConfig {
            default: Some("openai".to_string()),
            providers,
        };

        let agent_manager =
            create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

        // Configure with OpenAI provider
        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        // Switch to ZAI provider with GLM-4.7 model
        agent_manager
            .switch_model(&session.id, "zai-coding/GLM-4.7", None)
            .await
            .unwrap();

        // Verify the agent was updated
        let updated = session_manager.get_session(&session.id).unwrap();
        let agent = updated.agent.as_ref().unwrap();

        assert_eq!(
            agent.provider_key.as_deref(),
            Some("zai-coding"),
            "Provider key should be updated to zai-coding"
        );
        assert_eq!(agent.model, "GLM-4.7", "Model should be updated to GLM-4.7");
        assert_eq!(
            agent.endpoint.as_deref(),
            Some("https://api.z.ai/api/coding/paas/v4"),
            "Endpoint should be updated to ZAI Coding Plan endpoint"
        );

        // Verify cache was invalidated
        assert!(
            !agent_manager.agent_cache.contains_key(&session.id),
            "Cache should be invalidated after cross-provider switch to ZAI"
        );
    }

    #[tokio::test]
    async fn test_resolve_provider_config_from_llm_config() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "zai-coding".to_string(),
            LlmProviderConfig::builder(BackendType::ZAI)
                .endpoint("https://api.z.ai/api/coding/paas/v4")
                .api_key("test-key-123")
                .build(),
        );

        let llm_config = LlmConfig {
            default: Some("zai-coding".to_string()),
            providers,
        };

        let agent_manager =
            create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

        let resolved = agent_manager.resolve_provider_config("zai-coding");
        assert!(resolved.is_some(), "Should resolve from llm_config");
        let resolved = resolved.unwrap();
        assert_eq!(resolved.provider_type, BackendType::ZAI);
        assert_eq!(
            resolved.endpoint.as_deref(),
            Some("https://api.z.ai/api/coding/paas/v4")
        );
        assert_eq!(resolved.api_key.as_deref(), Some("test-key-123"));
        assert_eq!(resolved.source, "llm_config");
    }

    #[tokio::test]
    async fn test_resolve_provider_config_from_providers_config() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "local".to_string(),
            LlmProviderConfig::builder(BackendType::Ollama)
                .endpoint("http://localhost:11434")
                .api_key("ollama-key")
                .build(),
        );
        let llm_config = LlmConfig {
            default: Some("local".to_string()),
            providers,
        };

        let agent_manager =
            create_test_agent_manager_with_providers(session_manager.clone(), llm_config);

        let resolved = agent_manager.resolve_provider_config("local");
        assert!(resolved.is_some(), "Should resolve from llm_config");
        let resolved = resolved.unwrap();
        assert_eq!(resolved.provider_type, BackendType::Ollama);
        assert_eq!(resolved.endpoint.as_deref(), Some("http://localhost:11434"));
        assert_eq!(resolved.api_key.as_deref(), Some("ollama-key"));
        assert_eq!(resolved.source, "llm_config");
    }

    #[tokio::test]
    async fn test_resolve_provider_config_does_not_use_legacy_providers_config() {
        use crucible_config::LlmConfig;

        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let llm_config = LlmConfig::default();
        let agent_manager = create_test_agent_manager_with_providers(session_manager, llm_config);

        let resolved = agent_manager.resolve_provider_config("legacy");
        assert!(
            resolved.is_none(),
            "legacy providers config should not be used for resolution"
        );
    }

    #[tokio::test]
    async fn test_resolve_provider_config_not_found() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let resolved = agent_manager.resolve_provider_config("nonexistent");
        assert!(
            resolved.is_none(),
            "Should return None when provider not in either config"
        );
    }

    #[tokio::test]
    async fn test_resolve_provider_config_llm_config_wins_over_providers_config() {
        use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let mut llm_providers = std::collections::HashMap::new();
        llm_providers.insert(
            "shared".to_string(),
            LlmProviderConfig::builder(BackendType::OpenAI)
                .endpoint("https://api.openai.com/v1")
                .api_key("openai-key")
                .build(),
        );
        let llm_config = LlmConfig {
            default: None,
            providers: llm_providers,
        };

        let agent_manager =
            create_test_agent_manager_with_both(session_manager.clone(), llm_config);

        let resolved = agent_manager.resolve_provider_config("shared");
        assert!(resolved.is_some(), "Should resolve when in both configs");
        let resolved = resolved.unwrap();
        assert_eq!(
            resolved.source, "llm_config",
            "LlmConfig should take priority"
        );
        assert_eq!(resolved.provider_type, BackendType::OpenAI);
        assert_eq!(
            resolved.endpoint.as_deref(),
            Some("https://api.openai.com/v1")
        );
        assert_eq!(resolved.api_key.as_deref(), Some("openai-key"));
    }

    /// A mock agent whose stream never yields — blocks forever until cancelled.
    struct PendingMockAgent;

    #[async_trait::async_trait]
    impl AgentHandle for PendingMockAgent {
        fn send_message_stream(&mut self, _: String) -> BoxStream<'static, ChatResult<ChatChunk>> {
            Box::pin(futures::stream::pending())
        }

        fn is_connected(&self) -> bool {
            true
        }

        async fn set_mode_str(&mut self, _: &str) -> ChatResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn concurrent_send_to_same_session_returns_error() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        agent_manager.request_state.insert(
            session.id.clone(),
            super::RequestState {
                cancel_tx: None,
                task_handle: None,
                started_at: std::time::Instant::now(),
            },
        );

        let (event_tx, _event_rx) = broadcast::channel::<SessionEventMessage>(64);
        let result = agent_manager
            .send_message(&session.id, "hello".to_string(), &event_tx)
            .await;

        assert!(
            matches!(result, Err(AgentError::ConcurrentRequest(_))),
            "Second send_message should return ConcurrentRequest, got: {:?}",
            result,
        );
    }

    #[tokio::test]
    async fn cancel_during_streaming_emits_ended_event() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(PendingMockAgent) as BoxedAgentHandle)),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        let _message_id = agent_manager
            .send_message(&session.id, "test".to_string(), &event_tx)
            .await
            .unwrap();

        let user_msg = next_event_or_skip(&mut event_rx, "user_message").await;
        assert_eq!(user_msg.data["content"], "test");

        tokio::time::sleep(Duration::from_millis(50)).await;

        let cancelled = agent_manager.cancel(&session.id).await;
        assert!(cancelled, "cancel() should return true for active request");

        let ended = next_event_or_skip(&mut event_rx, "ended").await;
        assert_eq!(ended.session_id, session.id);
        assert_eq!(ended.data["reason"], "cancelled");
    }

    #[tokio::test]
    async fn empty_stream_without_done_cleans_up_request_state() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        agent_manager.agent_cache.insert(
            session.id.clone(),
            Arc::new(Mutex::new(Box::new(MockAgent) as BoxedAgentHandle)),
        );

        let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
        let _message_id = agent_manager
            .send_message(&session.id, "test".to_string(), &event_tx)
            .await
            .unwrap();

        let user_msg = next_event_or_skip(&mut event_rx, "user_message").await;
        assert_eq!(user_msg.data["content"], "test");

        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(
            !agent_manager.request_state.contains_key(&session.id),
            "request_state should be cleaned up after empty stream completes"
        );
    }
}
