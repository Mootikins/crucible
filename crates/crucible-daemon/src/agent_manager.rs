//! Agent lifecycle management for the daemon.

use crate::agent_factory::{create_agent_from_session_config, AgentFactoryError};
use crate::background_manager::BackgroundJobManager;
use crate::protocol::SessionEventMessage;
use crate::session_manager::{SessionError, SessionManager};
use crucible_core::events::SessionEvent;
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::AgentHandle;
use crucible_lua::{register_crucible_on_api, LuaScriptHandlerRegistry};
use dashmap::DashMap;
use futures::StreamExt;
use mlua::Lua;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

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

struct SessionLuaState {
    lua: Lua,
    registry: LuaScriptHandlerRegistry,
}

pub struct AgentManager {
    request_state: Arc<DashMap<String, RequestState>>,
    agent_cache: Arc<DashMap<String, Arc<Mutex<BoxedAgentHandle>>>>,
    session_manager: Arc<SessionManager>,
    background_manager: Arc<BackgroundJobManager>,
    lua_states: Arc<DashMap<String, Arc<Mutex<SessionLuaState>>>>,
}

impl AgentManager {
    pub fn new(
        session_manager: Arc<SessionManager>,
        background_manager: Arc<BackgroundJobManager>,
    ) -> Self {
        Self {
            request_state: Arc::new(DashMap::new()),
            agent_cache: Arc::new(DashMap::new()),
            session_manager,
            background_manager,
            lua_states: Arc::new(DashMap::new()),
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

    fn get_or_create_lua_state(&self, session_id: &str) -> Arc<Mutex<SessionLuaState>> {
        if let Some(state) = self.lua_states.get(session_id) {
            return state.clone();
        }

        let lua = Lua::new();
        let registry = LuaScriptHandlerRegistry::new();

        register_crucible_on_api(&lua, registry.runtime_handlers(), registry.handler_functions())
            .expect("Failed to register crucible.on API");

        let state = Arc::new(Mutex::new(SessionLuaState { lua, registry }));
        self.lua_states.insert(session_id.to_string(), state.clone());
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

        if self.request_state.contains_key(session_id) {
            return Err(AgentError::ConcurrentRequest(session_id.to_string()));
        }

        let agent = self
            .get_or_create_agent(session_id, &agent_config, &session.workspace)
            .await?;

        let (cancel_tx, cancel_rx) = oneshot::channel();

        self.request_state.insert(
            session_id.to_string(),
            RequestState {
                cancel_tx: Some(cancel_tx),
                task_handle: None,
                started_at: Instant::now(),
            },
        );

        let message_id = format!("msg-{}", uuid::Uuid::new_v4());
        let session_id_owned = session_id.to_string();
        let message_id_clone = message_id.clone();
        let event_tx_clone = event_tx.clone();
        let request_state = self.request_state.clone();
        let lua_state = self.get_or_create_lua_state(session_id);

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
    ) -> Result<Arc<Mutex<BoxedAgentHandle>>, AgentError> {
        if let Some(cached) = self.agent_cache.get(session_id) {
            debug!(session_id = %session_id, "Using cached agent");
            return Ok(cached.clone());
        }

        info!(
            session_id = %session_id,
            provider = %agent_config.provider,
            model = %agent_config.model,
            "Creating new agent"
        );

        let agent = create_agent_from_session_config(
            agent_config,
            workspace,
            Some(self.background_manager.clone()),
        )
        .await?;
        let agent = Arc::new(Mutex::new(agent));
        self.agent_cache
            .insert(session_id.to_string(), agent.clone());

        Ok(agent)
    }

    async fn execute_agent_stream(
        agent: Arc<Mutex<BoxedAgentHandle>>,
        content: String,
        session_id: &str,
        message_id: &str,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        accumulated_response: &mut String,
        lua_state: Arc<Mutex<SessionLuaState>>,
        is_continuation: bool,
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
        AgentManager::new(session_manager, background_manager)
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

            let result = state
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

            let order: Vec<String> = state
                .lua
                .load("return execution_order")
                .eval()
                .unwrap();
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
                let result = state
                    .registry
                    .execute_runtime_handler(&state.lua, &handler.name, &event);
                match result {
                    Ok(_) => {}
                    Err(_) => {}
                }
            }

            let order: Vec<String> = state
                .lua
                .load("return execution_order")
                .eval()
                .unwrap();
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

            let session_id: String = state
                .lua
                .load("return received_session_id")
                .eval()
                .unwrap();
            let message_id: String = state
                .lua
                .load("return received_message_id")
                .eval()
                .unwrap();
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
            assert!(injection.is_none(), "Handler should skip injection on continuation");

            // Verify the flag was received
            let state = lua_state.lock().await;
            let received: bool = state
                .lua
                .load("return received_continuation")
                .eval()
                .unwrap();
            assert!(received, "Handler should have received is_continuation=true");
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
}
