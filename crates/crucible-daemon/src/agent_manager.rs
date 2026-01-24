//! Agent lifecycle management for the daemon.

use crate::agent_factory::{create_agent_from_session_config, AgentFactoryError};
use crate::background_manager::BackgroundTaskManager;
use crate::protocol::SessionEventMessage;
use crate::session_manager::{SessionError, SessionManager};
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::AgentHandle;
use dashmap::DashMap;
use futures::StreamExt;
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

pub struct AgentManager {
    request_state: Arc<DashMap<String, RequestState>>,
    agent_cache: Arc<DashMap<String, Arc<Mutex<BoxedAgentHandle>>>>,
    session_manager: Arc<SessionManager>,
    background_manager: Arc<BackgroundTaskManager>,
}

impl AgentManager {
    pub fn new(
        session_manager: Arc<SessionManager>,
        background_manager: Arc<BackgroundTaskManager>,
    ) -> Self {
        Self {
            request_state: Arc::new(DashMap::new()),
            agent_cache: Arc::new(DashMap::new()),
            session_manager,
            background_manager,
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
        let background_manager = Arc::new(BackgroundTaskManager::new(event_tx));
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
}
