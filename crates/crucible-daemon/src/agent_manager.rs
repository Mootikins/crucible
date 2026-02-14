//! Agent lifecycle management for the daemon.

use crate::agent_factory::{create_agent_from_session_config, AgentFactoryError};
use crate::background_manager::BackgroundJobManager;
use crate::protocol::SessionEventMessage;
use crate::session_manager::{SessionError, SessionManager};
use crucible_config::PatternStore;
use crucible_core::events::SessionEvent;
use crucible_core::interaction::{InteractionRequest, PermRequest, PermResponse, PermissionScope};
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::AgentHandle;
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

/// Check if a tool is destructive and requires permission before execution.
///
/// Destructive tools can modify files, execute commands, or change state.
/// Read-only tools that only query data do not require permission.
///
/// # Returns
///
/// `true` for tools that modify state:
/// - `write`, `bash`, `delete` - file/command operations
/// - `create_note`, `update_note`, `delete_note` - note mutations
///
/// `false` for read-only tools:
/// - `read_note`, `read_metadata` - reading content
/// - `text_search`, `property_search`, `semantic_search` - search operations
///
/// Default-deny: only explicitly safe tools skip the permission prompt.
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
            | "get_outlinks"
            | "get_inlinks"
    )
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

struct SessionLuaState {
    lua: Lua,
    registry: LuaScriptHandlerRegistry,
    permission_hooks: Arc<StdMutex<Vec<PermissionHook>>>,
    permission_functions: Arc<StdMutex<HashMap<String, RegistryKey>>>,
}

struct PendingPermission {
    request: PermRequest,
    response_tx: oneshot::Sender<PermResponse>,
}

pub struct AgentManager {
    request_state: Arc<DashMap<String, RequestState>>,
    agent_cache: Arc<DashMap<String, Arc<Mutex<BoxedAgentHandle>>>>,
    session_manager: Arc<SessionManager>,
    background_manager: Arc<BackgroundJobManager>,
    lua_states: Arc<DashMap<String, Arc<Mutex<SessionLuaState>>>>,
    pending_permissions: Arc<DashMap<String, HashMap<PermissionId, PendingPermission>>>,
    mcp_gateway: Option<Arc<tokio::sync::RwLock<crucible_tools::mcp_gateway::McpGatewayManager>>>,
    providers_config: crucible_config::ProvidersConfig,
}

impl AgentManager {
    pub fn new(
        session_manager: Arc<SessionManager>,
        background_manager: Arc<BackgroundJobManager>,
        mcp_gateway: Option<
            Arc<tokio::sync::RwLock<crucible_tools::mcp_gateway::McpGatewayManager>>,
        >,
        providers_config: crucible_config::ProvidersConfig,
    ) -> Self {
        Self {
            request_state: Arc::new(DashMap::new()),
            agent_cache: Arc::new(DashMap::new()),
            session_manager,
            background_manager,
            lua_states: Arc::new(DashMap::new()),
            pending_permissions: Arc::new(DashMap::new()),
            mcp_gateway,
            providers_config,
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

    pub fn get_pending_permission(
        &self,
        session_id: &str,
        permission_id: &str,
    ) -> Option<PermRequest> {
        self.pending_permissions
            .get(session_id)
            .and_then(|perms| perms.get(permission_id).map(|p| p.request.clone()))
    }

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

        if event_tx
            .send(SessionEventMessage::user_message(
                session_id,
                &message_id,
                &content,
            ))
            .is_err()
        {
            warn!(session_id = %session_id, "No subscribers for user_message event");
        }

        let session_id_owned = session_id.to_string();
        let message_id_clone = message_id.clone();
        let request_state = self.request_state.clone();
        let lua_state = self.get_or_create_lua_state(session_id);
        let workspace_path = session.workspace.clone();

        let pending_permissions = self.pending_permissions.clone();

        let task = tokio::spawn(async move {
            let mut accumulated_response = String::new();

            tokio::select! {
                _ = cancel_rx => {
                    debug!(session_id = %session_id_owned, "Request cancelled");
                    if event_tx_clone.send(SessionEventMessage::ended(
                        &session_id_owned,
                        "cancelled",
                    )).is_err() {
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

        // Resolve endpoint from providers config if not explicitly set
        let resolved_config = if agent_config.endpoint.is_none() {
            let provider_key = agent_config
                .provider_key
                .as_deref()
                .unwrap_or(&agent_config.provider);
            if let Some(provider) = self.providers_config.get(provider_key) {
                let mut config = agent_config.clone();
                config.endpoint = provider.endpoint();
                debug!(
                    provider_key = %provider_key,
                    endpoint = ?config.endpoint,
                    "Resolved endpoint from providers config"
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

        let agent = create_agent_from_session_config(
            &resolved_config,
            workspace,
            Some(self.background_manager.clone()),
            event_tx,
            self.mcp_gateway.clone(),
        )
        .await?;
        let agent = Arc::new(Mutex::new(agent));
        self.agent_cache
            .insert(session_id.to_string(), agent.clone());

        Ok(agent)
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
    ) {
        let mut agent_guard = agent.lock().await;
        let mut stream = agent_guard.send_message_stream(content);

        while let Some(result) = stream.next().await {
            match result {
                Ok(chunk) => {
                    if !chunk.delta.is_empty() {
                        accumulated_response.push_str(&chunk.delta);
                        debug!(
                            session_id = %session_id,
                            delta_len = chunk.delta.len(),
                            "Sending text_delta event"
                        );
                        let send_result = event_tx
                            .send(SessionEventMessage::text_delta(session_id, &chunk.delta));
                        if send_result.is_err() {
                            warn!(session_id = %session_id, "No subscribers for text_delta event");
                        }
                    }

                    if let Some(reasoning) = &chunk.reasoning {
                        debug!(session_id = %session_id, "Sending thinking event");
                        if event_tx
                            .send(SessionEventMessage::thinking(session_id, reasoning))
                            .is_err()
                        {
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

                                            if event_tx
                                                .send(SessionEventMessage::tool_result(
                                                    session_id,
                                                    &call_id,
                                                    &tc.name,
                                                    serde_json::json!({ "error": error_msg }),
                                                ))
                                                .is_err()
                                            {
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
                                            if event_tx
                                                .send(SessionEventMessage::interaction_requested(
                                                    session_id,
                                                    &permission_id,
                                                    &interaction_request,
                                                ))
                                                .is_err()
                                            {
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
                                                if event_tx
                                                    .send(SessionEventMessage::tool_result(
                                                        session_id,
                                                        &call_id,
                                                        &tc.name,
                                                        serde_json::json!({ "error": error_msg }),
                                                    ))
                                                    .is_err()
                                                {
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

                            if event_tx
                                .send(SessionEventMessage::tool_call(
                                    session_id, &call_id, &tc.name, args,
                                ))
                                .is_err()
                            {
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
                            if event_tx
                                .send(SessionEventMessage::tool_result(
                                    session_id, &call_id, &tr.name, result,
                                ))
                                .is_err()
                            {
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
                        if event_tx
                            .send(SessionEventMessage::message_complete(
                                session_id,
                                message_id,
                                accumulated_response.clone(),
                                chunk.usage.as_ref(),
                            ))
                            .is_err()
                        {
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

                            if event_tx
                                .send(SessionEventMessage::new(
                                    session_id,
                                    "injection_pending",
                                    serde_json::json!({
                                        "content": &injected_content,
                                        "position": &position,
                                        "is_continuation": true,
                                    }),
                                ))
                                .is_err()
                            {
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
                                pending_permissions,
                                workspace_path,
                            ))
                            .await;
                        }

                        break;
                    }
                }
                Err(e) => {
                    error!(session_id = %session_id, error = %e, "Agent stream error");
                    if event_tx
                        .send(SessionEventMessage::ended(
                            session_id,
                            format!("error: {}", e),
                        ))
                        .is_err()
                    {
                        warn!(session_id = %session_id, "No subscribers for error event");
                    }
                    break;
                }
            }
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

        agent_config.model = model_id.to_string();
        session.agent = Some(agent_config.clone());

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;

        self.agent_cache.remove(session_id);

        info!(
            session_id = %session_id,
            model = %model_id,
            provider = %agent_config.provider,
            "Model switched for session (agent cache invalidated)"
        );

        if let Some(tx) = event_tx {
            let _ = tx.send(SessionEventMessage::model_switched(
                session_id,
                model_id,
                &agent_config.provider,
            ));
        }

        Ok(())
    }

    pub async fn list_models(&self, session_id: &str) -> Result<Vec<String>, AgentError> {
        let (_, agent_config) = self.get_session_with_agent(session_id)?;

        let endpoint = agent_config
            .endpoint
            .unwrap_or_else(|| "http://localhost:11434".to_string());

        match agent_config.provider.as_str() {
            "ollama" => self.list_ollama_models(&endpoint).await,
            _ => {
                debug!(
                    provider = %agent_config.provider,
                    "Model listing not supported for provider"
                );
                Ok(Vec::new())
            }
        }
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
            let _ = tx.send(SessionEventMessage::new(
                session_id,
                "thinking_budget_changed",
                serde_json::json!({ "budget": budget }),
            ));
        }

        Ok(())
    }

    pub fn get_thinking_budget(&self, session_id: &str) -> Result<Option<i64>, AgentError> {
        let (_, agent_config) = self.get_session_with_agent(session_id)?;
        Ok(agent_config.thinking_budget)
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
            let _ = tx.send(SessionEventMessage::new(
                session_id,
                "temperature_changed",
                serde_json::json!({ "temperature": temperature }),
            ));
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
            let _ = tx.send(SessionEventMessage::new(
                session_id,
                "notification_added",
                serde_json::json!({ "notification_id": notification.id }),
            ));
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
                let _ = tx.send(SessionEventMessage::new(
                    session_id,
                    "notification_dismissed",
                    serde_json::json!({ "notification_id": notification_id }),
                ));
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
            let _ = tx.send(SessionEventMessage::new(
                session_id,
                "max_tokens_changed",
                serde_json::json!({ "max_tokens": max_tokens }),
            ));
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
    use crate::session_storage::FileSessionStorage;
    use crucible_core::session::SessionType;
    use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatResult};
    use futures::stream::BoxStream;
    use std::collections::HashMap;
    use tempfile::TempDir;

    struct MockAgent;

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

    fn test_agent() -> SessionAgent {
        SessionAgent {
            agent_type: "internal".to_string(),
            agent_name: None,
            provider_key: Some("ollama".to_string()),
            provider: "ollama".to_string(),
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
        }
    }

    fn create_test_agent_manager(session_manager: Arc<SessionManager>) -> AgentManager {
        let (event_tx, _) = broadcast::channel(16);
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
        AgentManager::new(
            session_manager,
            background_manager,
            None,
            crucible_config::ProvidersConfig::default(),
        )
    }

    #[tokio::test]
    async fn test_configure_agent() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
            .await
            .unwrap();

        let agent_manager = create_test_agent_manager(session_manager.clone());

        let mut agent = test_agent();
        agent.temperature = Some(0.9);
        agent.system_prompt = "Custom prompt".to_string();
        agent.provider = "custom-provider".to_string();

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
        assert_eq!(updated_agent.provider, "custom-provider");
    }

    #[tokio::test]
    async fn test_switch_model_emits_event() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));

        let session = session_manager
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
                let result =
                    state
                        .registry
                        .execute_runtime_handler(&state.lua, &handler.name, &event);
                match result {
                    Ok(_) => {}
                    Err(_) => {}
                }
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
            assert!(is_safe("get_outlinks"));
            assert!(is_safe("get_inlinks"));
        }

        #[test]
        fn write_tools_are_not_safe() {
            assert!(!is_safe("write_file"));
            assert!(!is_safe("edit_file"));
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
    }
}
