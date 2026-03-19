use super::*;
use crate::session_storage::FileSessionStorage;
use crate::tools::workspace::WorkspaceTools;
use async_trait::async_trait;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::events::handler::{Handler, HandlerContext, HandlerResult};
use crucible_core::events::{InternalSessionEvent, SessionEvent};
use crucible_core::parser::ParsedNote;
use crucible_core::session::SessionType;
use crucible_core::test_support::EnvVarGuard;
use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatResult, ChatToolCall, ChatToolResult,
};
use crucible_core::traits::knowledge::NoteInfo;
use crucible_core::traits::KnowledgeRepository;
use crucible_core::types::SearchResult;
use futures::stream::BoxStream;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex as StdMutex};
use tempfile::TempDir;
use tokio::time::{timeout, Duration};

static ENV_LOCK: LazyLock<StdMutex<()>> = LazyLock::new(|| StdMutex::new(()));

fn clear_provider_env() -> Vec<EnvVarGuard> {
    vec![
        EnvVarGuard::remove("OLLAMA_HOST"),
        EnvVarGuard::remove("OPENAI_API_KEY"),
        EnvVarGuard::remove("ANTHROPIC_API_KEY"),
        EnvVarGuard::remove("COHERE_API_KEY"),
        EnvVarGuard::remove("GOOGLE_API_KEY"),
        EnvVarGuard::remove("OPENROUTER_API_KEY"),
        EnvVarGuard::remove("GLM_AUTH_TOKEN"),
    ]
}
struct MockAgent;

struct StreamingMockAgent {
    chunks: Vec<ChatChunk>,
}

struct MockHandler {
    name: String,
    event_pattern: String,
    call_count: Arc<std::sync::atomic::AtomicUsize>,
    behavior: MockHandlerBehavior,
}

enum MockHandlerBehavior {
    Passthrough,
    ModifyPrompt(String),
    Cancel,
    FatalError(String),
}

#[async_trait::async_trait]
impl Handler for MockHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn event_pattern(&self) -> &str {
        &self.event_pattern
    }

    async fn handle(
        &self,
        _ctx: &mut HandlerContext,
        event: SessionEvent,
    ) -> HandlerResult<SessionEvent> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        match &self.behavior {
            MockHandlerBehavior::Passthrough => HandlerResult::Continue(event),
            MockHandlerBehavior::ModifyPrompt(new_prompt) => {
                if let SessionEvent::Internal(inner) = &event {
                    if let InternalSessionEvent::PreLlmCall { model, .. } = inner.as_ref() {
                        HandlerResult::Continue(SessionEvent::internal(
                            InternalSessionEvent::PreLlmCall {
                                prompt: new_prompt.clone(),
                                model: model.clone(),
                            },
                        ))
                    } else {
                        HandlerResult::Continue(event)
                    }
                } else {
                    HandlerResult::Continue(event)
                }
            }
            MockHandlerBehavior::Cancel => HandlerResult::Cancel,
            MockHandlerBehavior::FatalError(msg) => {
                HandlerResult::FatalError(crucible_core::events::EventError::other(msg.clone()))
            }
        }
    }
}

struct PromptCapturingAgent {
    received_prompt: Arc<std::sync::Mutex<Option<String>>>,
    chunks: Vec<ChatChunk>,
}

#[async_trait::async_trait]
impl AgentHandle for PromptCapturingAgent {
    fn send_message_stream(
        &mut self,
        content: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        *self.received_prompt.lock().unwrap() = Some(content);
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

    async fn search_vectors(&self, _vector: Vec<f32>) -> crucible_core::Result<Vec<SearchResult>> {
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

struct ReactorTestHarness {
    agent_manager: AgentManager,
    session_id: String,
    event_tx: broadcast::Sender<SessionEventMessage>,
    event_rx: broadcast::Receiver<SessionEventMessage>,
    _tmp: TempDir,
}

impl ReactorTestHarness {
    async fn new() -> Self {
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
        let (event_tx, event_rx) = broadcast::channel::<SessionEventMessage>(64);
        Self {
            agent_manager,
            session_id: session.id,
            event_tx,
            event_rx,
            _tmp: tmp,
        }
    }

    async fn register_handler(&self, handler: MockHandler) {
        let session_state = self
            .agent_manager
            .get_or_create_session_state(&self.session_id);
        session_state
            .lock()
            .await
            .reactor
            .register(Box::new(handler))
            .unwrap();
    }

    fn inject_capturing_agent(
        &self,
        chunks: Vec<ChatChunk>,
    ) -> Arc<std::sync::Mutex<Option<String>>> {
        let received_prompt = Arc::new(std::sync::Mutex::new(None::<String>));
        self.agent_manager.agent_cache.insert(
            self.session_id.clone(),
            Arc::new(Mutex::new(Box::new(PromptCapturingAgent {
                received_prompt: received_prompt.clone(),
                chunks,
            }) as BoxedAgentHandle)),
        );
        received_prompt
    }

    fn inject_streaming_agent(&self, chunks: Vec<ChatChunk>) {
        self.agent_manager.agent_cache.insert(
            self.session_id.clone(),
            Arc::new(Mutex::new(
                Box::new(StreamingMockAgent { chunks }) as BoxedAgentHandle
            )),
        );
    }

    fn default_ok_chunks() -> Vec<ChatChunk> {
        vec![ChatChunk {
            delta: "ok".to_string(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
            precognition_notes_count: None,
            precognition_notes: None,
        }]
    }

    async fn send(&mut self, msg: &str) {
        self.agent_manager
            .send_message(&self.session_id, msg.to_string(), &self.event_tx, true)
            .await
            .unwrap();
    }

    async fn wait_for(&mut self, event_name: &str) -> SessionEventMessage {
        next_event_or_skip(&mut self.event_rx, event_name).await
    }

    #[allow(dead_code)]
    async fn assert_no_event_until_complete(&mut self, event_name: &str) {
        assert_no_event_until_message_complete(&mut self.event_rx, event_name).await;
    }
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
    AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager,
        background_manager,
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
    })
}

fn create_test_agent_manager_with_providers(
    session_manager: Arc<SessionManager>,
    llm_config: crucible_config::LlmConfig,
) -> AgentManager {
    let (event_tx, _) = broadcast::channel(16);
    let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
    AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager,
        background_manager,
        mcp_gateway: None,
        llm_config: Some(llm_config),
        acp_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
    })
}

fn create_test_agent_manager_with_enrichment(
    session_manager: Arc<SessionManager>,
    enrichment_config: crucible_config::EmbeddingProviderConfig,
) -> AgentManager {
    let (event_tx, _) = broadcast::channel(16);
    let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
    AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::with_event_tx(
            event_tx,
            Some(enrichment_config),
            crucible_config::default_max_precognition_chars(),
        )),
        session_manager,
        background_manager,
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
    })
}

fn create_test_agent_manager_with_llm_config(
    session_manager: Arc<SessionManager>,
    llm_config: crucible_config::LlmConfig,
) -> AgentManager {
    let (event_tx, _) = broadcast::channel(16);
    let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
    AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager,
        background_manager,
        mcp_gateway: None,
        llm_config: Some(llm_config),
        acp_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
    })
}

async fn start_mock_ollama_tags_server(models: Vec<&str>) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let model_payload = models
        .into_iter()
        .map(|name| serde_json::json!({ "name": name }))
        .collect::<Vec<_>>();
    let body = serde_json::json!({ "models": model_payload }).to_string();

    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0_u8; 1024];
        let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buf)
            .await
            .unwrap();

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
            .await
            .unwrap();
    });

    (format!("http://{}", addr), handle)
}

async fn start_mock_openai_models_server(
    status_code: u16,
    body_json: serde_json::Value,
    expected_api_key: Option<&str>,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let body = body_json.to_string();
    let expected_auth = expected_api_key.map(|key| format!("Authorization: Bearer {}", key));

    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0_u8; 4096];
        let bytes_read = tokio::io::AsyncReadExt::read(&mut socket, &mut buf)
            .await
            .unwrap();
        let request = String::from_utf8_lossy(&buf[..bytes_read]);
        let request_lower = request.to_lowercase();

        assert!(
            request.starts_with("GET /models"),
            "Expected GET /models request, got: {}",
            request
        );

        if let Some(expected_auth) = expected_auth {
            let expected_auth_lower = expected_auth.to_lowercase();
            assert!(
                request_lower.contains(&expected_auth_lower),
                "Expected Authorization header '{}', got request: {}",
                expected_auth,
                request
            );
        }

        let status_text = if status_code == 200 { "OK" } else { "ERROR" };
        let response = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            status_code,
            status_text,
            body.len(),
            body
        );
        tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
            .await
            .unwrap();
    });

    (format!("http://{}", addr), handle)
}

fn create_test_agent_manager_with_both(
    session_manager: Arc<SessionManager>,
    llm_config: crucible_config::LlmConfig,
) -> AgentManager {
    let (event_tx, _) = broadcast::channel(16);
    let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
    AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager,
        background_manager,
        mcp_gateway: None,
        llm_config: Some(llm_config),
        acp_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
    })
}

mod concurrency;
mod dispatch;
mod init_lua;
mod lifecycle;
mod messaging;
mod models;
mod models_discovery;
mod permissions;
mod precognition;
mod reactor;
mod workspace;
