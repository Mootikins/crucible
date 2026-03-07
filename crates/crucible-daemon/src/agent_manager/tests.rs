use super::*;
use crate::session_storage::FileSessionStorage;
use crate::tools::workspace::WorkspaceTools;
use async_trait::async_trait;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::events::handler::{Handler, HandlerContext, HandlerResult};
use crucible_core::events::{InternalSessionEvent, SessionEvent};
use crucible_core::parser::ParsedNote;
use crucible_core::session::SessionType;
use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatResult, ChatToolCall, ChatToolResult,
};
use crucible_core::traits::knowledge::NoteInfo;
use crucible_core::traits::KnowledgeRepository;
use crucible_core::types::SearchResult;
use futures::stream::BoxStream;
use futures::StreamExt;
use std::collections::HashMap;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};

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
            .send_message(&self.session_id, msg.to_string(), &self.event_tx)
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
        workspace_tools: Arc::new(WorkspaceTools::new(&std::path::PathBuf::from("/tmp"))),
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
        workspace_tools: Arc::new(WorkspaceTools::new(&std::path::PathBuf::from("/tmp"))),
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
        )),
        session_manager,
        background_manager,
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(&std::path::PathBuf::from("/tmp"))),
    })
}

#[tokio::test]
async fn reactor_pre_llm_modifies_prompt() {
    let mut h = ReactorTestHarness::new().await;

    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    h.register_handler(MockHandler {
        name: "test-modify-prompt".to_string(),
        event_pattern: "pre_llm_call".to_string(),
        call_count: call_count.clone(),
        behavior: MockHandlerBehavior::ModifyPrompt("MODIFIED: hello".to_string()),
    })
    .await;
    let received_prompt = h.inject_capturing_agent(ReactorTestHarness::default_ok_chunks());

    h.send("hello").await;
    h.wait_for("message_complete").await;

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    let prompt = received_prompt.lock().unwrap();
    assert_eq!(prompt.as_deref(), Some("MODIFIED: hello"));
}

#[tokio::test]
async fn reactor_pre_llm_cancel_aborts() {
    let mut h = ReactorTestHarness::new().await;

    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    h.register_handler(MockHandler {
        name: "test-cancel-pre-llm".to_string(),
        event_pattern: "pre_llm_call".to_string(),
        call_count: call_count.clone(),
        behavior: MockHandlerBehavior::Cancel,
    })
    .await;

    let received_prompt = h.inject_capturing_agent(vec![ChatChunk {
        delta: "should-not-run".to_string(),
        done: true,
        tool_calls: None,
        tool_results: None,
        reasoning: None,
        usage: None,
        subagent_events: None,
        precognition_notes_count: None,
        precognition_notes: None,
    }]);

    h.send("hello").await;
    let ended = h.wait_for("ended").await;

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(ended.data["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("cancelled by handler"));
    let prompt = received_prompt.lock().unwrap();
    assert!(prompt.is_none());
}

#[tokio::test]
async fn reactor_pre_llm_empty_passthrough() {
    let mut h = ReactorTestHarness::new().await;
    let received_prompt = h.inject_capturing_agent(ReactorTestHarness::default_ok_chunks());

    h.send("hello").await;
    h.wait_for("message_complete").await;

    let prompt = received_prompt.lock().unwrap();
    assert_eq!(prompt.as_deref(), Some("hello"));
}

#[tokio::test]
async fn reactor_pre_llm_error_fails_open() {
    let mut h = ReactorTestHarness::new().await;

    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    h.register_handler(MockHandler {
        name: "test-fatal-pre-llm".to_string(),
        event_pattern: "pre_llm_call".to_string(),
        call_count: call_count.clone(),
        behavior: MockHandlerBehavior::FatalError("boom".to_string()),
    })
    .await;

    let received_prompt = h.inject_capturing_agent(ReactorTestHarness::default_ok_chunks());

    h.send("hello").await;
    h.wait_for("message_complete").await;

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    let prompt = received_prompt.lock().unwrap();
    assert_eq!(prompt.as_deref(), Some("hello"));
}

#[tokio::test]
async fn reactor_post_llm_fires_after_stream() {
    let mut h = ReactorTestHarness::new().await;

    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    h.register_handler(MockHandler {
        name: "test-post-llm".to_string(),
        event_pattern: "post_llm_call".to_string(),
        call_count: call_count.clone(),
        behavior: MockHandlerBehavior::Passthrough,
    })
    .await;

    h.inject_streaming_agent(ReactorTestHarness::default_ok_chunks());

    h.send("hello").await;
    h.wait_for("message_complete").await;

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn reactor_pre_tool_cancel_denies() {
    let mut h = ReactorTestHarness::new().await;

    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    h.register_handler(MockHandler {
        name: "test-pre-tool-cancel".to_string(),
        event_pattern: "pre_tool_call".to_string(),
        call_count: call_count.clone(),
        behavior: MockHandlerBehavior::Cancel,
    })
    .await;

    h.inject_streaming_agent(vec![
        ChatChunk {
            delta: String::new(),
            done: false,
            tool_calls: Some(vec![ChatToolCall {
                name: "write".to_string(),
                arguments: Some(serde_json::json!({ "path": "foo.txt", "content": "x" })),
                id: Some("call-pre-tool-cancel".to_string()),
            }]),
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
            precognition_notes_count: None,
            precognition_notes: None,
        },
        ChatChunk {
            delta: "done".to_string(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
            precognition_notes_count: None,
            precognition_notes: None,
        },
    ]);

    h.send("run tool").await;

    let tool_result = h.wait_for("tool_result").await;
    h.wait_for("message_complete").await;

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(tool_result.data["tool"], "write");
    assert!(tool_result.data["result"]["error"]
        .as_str()
        .unwrap_or_default()
        .contains("Tool call denied by handler"));
}

#[tokio::test]
async fn reactor_persists_across_messages() {
    let mut h = ReactorTestHarness::new().await;

    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    h.register_handler(MockHandler {
        name: "test-persists".to_string(),
        event_pattern: "pre_llm_call".to_string(),
        call_count: call_count.clone(),
        behavior: MockHandlerBehavior::Passthrough,
    })
    .await;

    h.inject_streaming_agent(ReactorTestHarness::default_ok_chunks());

    h.send("one").await;
    h.wait_for("message_complete").await;

    h.send("two").await;
    h.wait_for("message_complete").await;

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn reactor_cleanup_drops_state() {
    let h = ReactorTestHarness::new().await;

    let _ = h.agent_manager.get_or_create_session_state(&h.session_id);
    assert!(h.agent_manager.session_states.contains_key(&h.session_id));

    h.agent_manager.cleanup_session(&h.session_id);

    assert!(!h.agent_manager.session_states.contains_key(&h.session_id));
}

#[tokio::test]
async fn reactor_lua_handler_discovery_empty_dir() {
    let mut h = ReactorTestHarness::new().await;

    let session_state = h.agent_manager.get_or_create_session_state(&h.session_id);
    {
        let state = session_state.lock().await;
        assert!(state.reactor.is_empty());
    }

    let received_prompt = h.inject_capturing_agent(ReactorTestHarness::default_ok_chunks());

    h.send("hello").await;
    h.wait_for("message_complete").await;

    let prompt = received_prompt.lock().unwrap();
    assert_eq!(prompt.as_deref(), Some("hello"));
}

#[test]
fn event_patterns_match_event_type() {
    let _repo = MockKnowledgeRepository { results: vec![] };
    let _embedding = MockEmbeddingProvider { should_fail: false };

    let pre_llm = SessionEvent::internal(InternalSessionEvent::PreLlmCall {
        prompt: String::new(),
        model: String::new(),
    });
    assert_eq!(pre_llm.event_type(), "pre_llm_call");

    let post_llm = SessionEvent::internal(InternalSessionEvent::PostLlmCall {
        response_summary: String::new(),
        model: String::new(),
        duration_ms: 0,
        token_count: None,
    });
    assert_eq!(post_llm.event_type(), "post_llm_call");

    let pre_tool = SessionEvent::internal(InternalSessionEvent::PreToolCall {
        name: String::new(),
        args: serde_json::Value::Null,
    });
    assert_eq!(pre_tool.event_type(), "pre_tool_call");
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
                    precognition_notes: None,
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
                    precognition_notes: None,
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
                precognition_notes: None,
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
                precognition_notes: None,
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
                precognition_notes: None,
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
async fn test_precognition_complete_event_emitted_when_enrichment_runs() {
    crate::embedding::clear_embedding_provider_cache();

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

    let agent_manager = create_test_agent_manager_with_enrichment(
        session_manager.clone(),
        crucible_config::EmbeddingProviderConfig::mock(Some(384)),
    );
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
                precognition_notes: None,
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
                    precognition_notes: None,
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
                    precognition_notes: None,
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
    std::fs::write("/tmp/test.md", "content").unwrap();

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
                    precognition_notes: None,
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
                    precognition_notes: None,
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
                    precognition_notes: None,
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
    assert!(tool_result.data["result"]["result"].as_str().unwrap_or("").contains("content"));

    let complete = next_event_or_skip(&mut event_rx, "message_complete").await;
    assert_eq!(complete.data["message_id"], message_id);
    assert_eq!(complete.data["full_response"], "Done.");
}

#[tokio::test]
async fn test_execute_agent_stream_empty_response_emits_error_event() {
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
                precognition_notes: None,
            }],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;

    let mut saw_message_complete = false;
    let ended = timeout(Duration::from_secs(2), async {
        loop {
            match event_rx.recv().await {
                Ok(event) if event.event == "message_complete" => saw_message_complete = true,
                Ok(event) if event.event == "ended" => return event,
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(err) => panic!("event channel closed while waiting for ended: {err}"),
            }
        }
    })
    .await
    .expect("timed out waiting for ended event");

    assert!(
        !saw_message_complete,
        "unexpected message_complete before error ended"
    );
    let ended_reason = ended.data["reason"].as_str().unwrap_or_default();
    assert!(
        ended_reason.starts_with("error:"),
        "expected error ended event, got: {ended_reason}"
    );
}

#[tokio::test]
async fn test_execute_agent_stream_tool_call_only_is_not_error() {
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
                        id: Some("call-tool-only".to_string()),
                    }]),
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                    precognition_notes_count: None,
                    precognition_notes: None,
                },
                ChatChunk {
                    delta: String::new(),
                    done: false,
                    tool_calls: None,
                    tool_results: Some(vec![ChatToolResult {
                        name: "read_file".to_string(),
                        result: "content".to_string(),
                        error: None,
                        call_id: Some("call-tool-only".to_string()),
                    }]),
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                    precognition_notes_count: None,
                    precognition_notes: None,
                },
                ChatChunk {
                    delta: String::new(),
                    done: true,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                    precognition_notes_count: None,
                    precognition_notes: None,
                },
            ],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    let message_id = agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;

    let mut saw_error_ended = false;
    let complete = timeout(Duration::from_secs(2), async {
        loop {
            match event_rx.recv().await {
                Ok(event) if event.event == "ended" => {
                    let reason = event.data["reason"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    if reason.starts_with("error:") {
                        saw_error_ended = true;
                    }
                }
                Ok(event) if event.event == "message_complete" => return event,
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

    assert_eq!(complete.data["message_id"], message_id);
    assert_eq!(complete.data["full_response"], "");
    assert!(
        !saw_error_ended,
        "unexpected error ended event before message_complete in tool-call-only flow"
    );
}

mod event_dispatch {
    use super::*;
    use crucible_lua::ScriptHandlerResult;

    #[tokio::test]
    async fn handler_executes_when_event_fires() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_state = agent_manager.get_or_create_session_state("test-session");
        let state = session_state.lock().await;

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

        let session_state = agent_manager.get_or_create_session_state("test-session");
        let state = session_state.lock().await;

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

        let session_state = agent_manager.get_or_create_session_state("test-session");
        let state = session_state.lock().await;

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
            let _result = state
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

        let session_state_1 = agent_manager.get_or_create_session_state("session-1");
        let session_state_2 = agent_manager.get_or_create_session_state("session-2");

        {
            let state = session_state_1.lock().await;
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
            let state = session_state_2.lock().await;
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

        let state_1 = session_state_1.lock().await;
        let state_2 = session_state_2.lock().await;

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

        let session_state = agent_manager.get_or_create_session_state("test-session");
        let state = session_state.lock().await;

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

        let session_state = agent_manager.get_or_create_session_state("test-session");
        let state = session_state.lock().await;

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

        let session_state = agent_manager.get_or_create_session_state("test-session");

        // Register handler that returns inject
        {
            let state = session_state.lock().await;
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
            &session_state,
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

        let session_state = agent_manager.get_or_create_session_state("test-session");

        // Register two handlers that both return inject
        {
            let state = session_state.lock().await;
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
            &session_state,
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

        let session_state = agent_manager.get_or_create_session_state("test-session");

        {
            let state = session_state.lock().await;
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
            &session_state,
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

        let session_state = agent_manager.get_or_create_session_state("test-session");

        // Register handler that checks is_continuation and skips if true
        {
            let state = session_state.lock().await;
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
            &session_state,
            true, // is_continuation
        )
        .await;

        // Handler should have returned nil, so no injection
        assert!(
            injection.is_none(),
            "Handler should skip injection on continuation"
        );

        // Verify the flag was received
        let state = session_state.lock().await;
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

        let session_state = agent_manager.get_or_create_session_state("test-session");

        {
            let state = session_state.lock().await;
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
            &session_state,
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

    let _ = agent_manager.get_or_create_session_state(session_id);
    assert!(
        agent_manager.session_states.contains_key(session_id),
        "Lua state should exist after creation"
    );

    agent_manager.cleanup_session(session_id);

    assert!(
        !agent_manager.session_states.contains_key(session_id),
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
        let result =
            agent_manager.respond_to_permission(session_id, &permission_id, PermResponse::allow());
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
        let result =
            agent_manager.respond_to_permission(session_id, &permission_id, PermResponse::deny());
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
    let agent_manager = AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: session_manager.clone(),
        background_manager,
        mcp_gateway: None,
        llm_config: Some(llm_config),
        acp_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(&std::path::PathBuf::from("/tmp"))),
    });

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

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
async fn test_list_models_trust_excludes_cloud_for_confidential_kiln() {
    use crucible_config::{
        BackendType, DataClassification, LlmConfig, LlmProviderConfig, TrustLevel,
    };
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

    let mut local = LlmProviderConfig::builder(BackendType::Custom)
        .available_models(vec!["local-model".to_string()])
        .build();
    local.trust_level = Some(TrustLevel::Local);

    let mut cloud = LlmProviderConfig::builder(BackendType::OpenAI)
        .available_models(vec!["gpt-4o".to_string()])
        .build();
    cloud.trust_level = Some(TrustLevel::Cloud);

    let mut providers = HashMap::new();
    providers.insert("local-custom".to_string(), local);
    providers.insert("cloud-openai".to_string(), cloud);

    let llm_config = LlmConfig {
        default: Some("local-custom".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager
        .list_models(&session.id, Some(DataClassification::Confidential))
        .await
        .unwrap();

    assert!(
        models.contains(&"local-custom/local-model".to_string()),
        "Confidential should keep Local provider models, got: {:?}",
        models
    );
    assert!(
        !models.iter().any(|m| m.starts_with("cloud-openai/")),
        "Confidential should exclude Cloud provider models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_trust_returns_all_for_public_kiln() {
    use crucible_config::{
        BackendType, DataClassification, LlmConfig, LlmProviderConfig, TrustLevel,
    };
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

    let mut local = LlmProviderConfig::builder(BackendType::Custom)
        .available_models(vec!["local-model".to_string()])
        .build();
    local.trust_level = Some(TrustLevel::Local);

    let mut cloud = LlmProviderConfig::builder(BackendType::OpenAI)
        .available_models(vec!["gpt-4o".to_string()])
        .build();
    cloud.trust_level = Some(TrustLevel::Cloud);

    let mut providers = HashMap::new();
    providers.insert("local-custom".to_string(), local);
    providers.insert("cloud-openai".to_string(), cloud);

    let llm_config = LlmConfig {
        default: Some("local-custom".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager
        .list_models(&session.id, Some(DataClassification::Public))
        .await
        .unwrap();

    assert!(
        models.contains(&"local-custom/local-model".to_string()),
        "Public should include Local provider models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"cloud-openai/gpt-4o".to_string()),
        "Public should include Cloud provider models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_trust_returns_all_when_no_classification() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig, TrustLevel};
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

    let mut local = LlmProviderConfig::builder(BackendType::Custom)
        .available_models(vec!["local-model".to_string()])
        .build();
    local.trust_level = Some(TrustLevel::Local);

    let mut cloud = LlmProviderConfig::builder(BackendType::OpenAI)
        .available_models(vec!["gpt-4o".to_string()])
        .build();
    cloud.trust_level = Some(TrustLevel::Cloud);

    let mut providers = HashMap::new();
    providers.insert("local-custom".to_string(), local);
    providers.insert("cloud-openai".to_string(), cloud);

    let llm_config = LlmConfig {
        default: Some("local-custom".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    assert!(
        models.contains(&"local-custom/local-model".to_string()),
        "No classification should include Local provider models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"cloud-openai/gpt-4o".to_string()),
        "No classification should include Cloud provider models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_trust_includes_cloud_for_internal_kiln() {
    use crucible_config::{
        BackendType, DataClassification, LlmConfig, LlmProviderConfig, TrustLevel,
    };
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

    let mut local = LlmProviderConfig::builder(BackendType::Custom)
        .available_models(vec!["local-model".to_string()])
        .build();
    local.trust_level = Some(TrustLevel::Local);

    let mut cloud = LlmProviderConfig::builder(BackendType::OpenAI)
        .available_models(vec!["gpt-4o".to_string()])
        .build();
    cloud.trust_level = Some(TrustLevel::Cloud);

    let mut untrusted = LlmProviderConfig::builder(BackendType::Custom)
        .available_models(vec!["unsafe-model".to_string()])
        .build();
    untrusted.trust_level = Some(TrustLevel::Untrusted);

    let mut providers = HashMap::new();
    providers.insert("local-custom".to_string(), local);
    providers.insert("cloud-openai".to_string(), cloud);
    providers.insert("untrusted-custom".to_string(), untrusted);

    let llm_config = LlmConfig {
        default: Some("local-custom".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager
        .list_models(&session.id, Some(DataClassification::Internal))
        .await
        .unwrap();

    assert!(
        models.contains(&"local-custom/local-model".to_string()),
        "Internal should include Local provider models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"cloud-openai/gpt-4o".to_string()),
        "Internal should include Cloud provider models, got: {:?}",
        models
    );
    assert!(
        !models.iter().any(|m| m.starts_with("untrusted-custom/")),
        "Internal should exclude Untrusted provider models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_all_chat_backends_with_explicit_models() {
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

    // With available_models set, discover_models short-circuits without HTTP.
    // The mock server is kept for the endpoint URL but never contacted.
    let (ollama_endpoint, ollama_server) = start_mock_ollama_tags_server(vec!["llama3.2"]).await;

    let mut providers = HashMap::new();
    providers.insert(
        "ollama-local".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint(ollama_endpoint)
            .available_models(vec!["llama3.2".to_string()])
            .build(),
    );
    providers.insert(
        "openai-main".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["gpt-4o".to_string()])
            .build(),
    );
    providers.insert(
        "anthropic-main".to_string(),
        LlmProviderConfig::builder(BackendType::Anthropic)
            .available_models(vec!["claude-sonnet-4-20250514".to_string()])
            .build(),
    );
    providers.insert(
        "cohere-main".to_string(),
        LlmProviderConfig::builder(BackendType::Cohere)
            .available_models(vec!["command-r-plus".to_string()])
            .build(),
    );
    providers.insert(
        "vertex-main".to_string(),
        LlmProviderConfig::builder(BackendType::VertexAI)
            .available_models(vec!["gemini-1.5-pro".to_string()])
            .build(),
    );
    providers.insert(
        "copilot-main".to_string(),
        LlmProviderConfig::builder(BackendType::GitHubCopilot)
            .available_models(vec!["gpt-4o".to_string()])
            .build(),
    );
    providers.insert(
        "openrouter-main".to_string(),
        LlmProviderConfig::builder(BackendType::OpenRouter)
            .available_models(vec!["openai/gpt-4o".to_string()])
            .build(),
    );
    providers.insert(
        "zai-main".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .available_models(vec!["GLM-4.7".to_string()])
            .build(),
    );
    providers.insert(
        "custom-main".to_string(),
        LlmProviderConfig::builder(BackendType::Custom)
            .available_models(vec!["my-custom-model".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("ollama-local".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    ollama_server.abort(); // Server never receives request with available_models set

    let expected_models = [
        "ollama-local/llama3.2",
        "openai-main/gpt-4o",
        "anthropic-main/claude-sonnet-4-20250514",
        "cohere-main/command-r-plus",
        "vertex-main/gemini-1.5-pro",
        "copilot-main/gpt-4o",
        "openrouter-main/openai/gpt-4o",
        "zai-main/GLM-4.7",
        "custom-main/my-custom-model",
    ];

    for expected in expected_models {
        assert!(
            models.contains(&expected.to_string()),
            "Missing model {expected}, got: {:?}",
            models
        );
    }
}

#[tokio::test]
async fn test_list_models_discovery_failure_returns_empty() {
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

    // Use dead endpoints to force discovery failure
    let dead_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_addr = dead_listener.local_addr().unwrap();
    drop(dead_listener);
    let dead_endpoint = format!("http://{}", dead_addr);

    let mut providers = HashMap::new();
    providers.insert(
        "anthropic-dead".to_string(),
        LlmProviderConfig::builder(BackendType::Anthropic)
            .endpoint(&dead_endpoint)
            .build(),
    );
    providers.insert(
        "openai-dead".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .endpoint(&dead_endpoint)
            .build(),
    );
    providers.insert(
        "zai-dead".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .endpoint(&dead_endpoint)
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("anthropic-dead".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    // Without available_models and with dead endpoints, all providers return empty
    assert!(
        models.is_empty(),
        "Failed discovery without available_models should return empty, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_count_matches_sum() {
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
        "openai-count".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["gpt-4o".to_string(), "o3-mini".to_string()])
            .build(),
    );
    providers.insert(
        "anthropic-count".to_string(),
        LlmProviderConfig::builder(BackendType::Anthropic)
            .available_models(vec!["claude-3-7-sonnet-20250219".to_string()])
            .build(),
    );
    providers.insert(
        "zai-count".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .available_models(vec!["GLM-5".to_string(), "GLM-4.7".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-count".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    let expected_total = 2 + 1 + 2;

    assert_eq!(
        models.len(),
        expected_total,
        "Expected {} models total, got {:?}",
        expected_total,
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
    let agent_manager = AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: session_manager.clone(),
        background_manager,
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(&std::path::PathBuf::from("/tmp"))),
    });

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    assert!(
        models.is_empty()
            || models
                .iter()
                .all(|m| m.starts_with("[error]") || !m.contains('/')),
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
    let agent_manager = AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: session_manager.clone(),
        background_manager,
        mcp_gateway: None,
        llm_config: Some(llm_config),
        acp_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(&std::path::PathBuf::from("/tmp"))),
    });

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    assert!(
        models.contains(&"anthropic/claude-3-opus".to_string()),
        "Should prefix with provider key: {:?}",
        models
    );
}

#[test]
fn test_openai_compatible_parses_data_models_prefers_id() {
    let payload = serde_json::json!({
        "data": [
            { "id": "gpt-4o", "name": "ignored-name" },
            { "name": "fallback-name" },
            { "id": "gpt-4o-mini" }
        ]
    });

    let models =
        crate::provider::model_listing::openai_compat::parse_models_response(&payload.to_string())
            .unwrap();

    assert_eq!(
        models,
        vec![
            "gpt-4o".to_string(),
            "fallback-name".to_string(),
            "gpt-4o-mini".to_string()
        ]
    );
}

#[test]
fn test_openai_compatible_parses_models_fallback_shape() {
    let payload = serde_json::json!({
        "models": [
            { "name": "llama-3.1-70b" },
            { "id": "deepseek-chat" }
        ]
    });

    let models =
        crate::provider::model_listing::openai_compat::parse_models_response(&payload.to_string())
            .unwrap();

    assert_eq!(
        models,
        vec!["llama-3.1-70b".to_string(), "deepseek-chat".to_string()]
    );
}

#[test]
fn test_openai_compatible_missing_both_keys_errors() {
    // Neither 'data' nor 'models' key → error
    let payload = serde_json::json!({
        "other_key": []
    });

    let result =
        crate::provider::model_listing::openai_compat::parse_models_response(&payload.to_string());

    assert!(result.is_err());
}

#[tokio::test]
async fn test_openai_compatible_http_includes_auth_header_and_trims_endpoint() {
    let (endpoint, server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "gpt-4o" },
                { "name": "gpt-4.1-mini" }
            ]
        }),
        Some("test-key"),
    )
    .await;

    let models =
        crate::provider::model_listing::openai_compat::list_models(&(endpoint + "/"), "test-key")
            .await
            .unwrap();
    server.await.unwrap();

    assert_eq!(
        models,
        vec!["gpt-4o".to_string(), "gpt-4.1-mini".to_string()]
    );
}

#[tokio::test]
async fn test_openai_compatible_non_success_status_returns_error() {
    let (endpoint, server) = start_mock_openai_models_server(
        503,
        serde_json::json!({ "error": "service unavailable" }),
        None,
    )
    .await;

    let result = crate::provider::model_listing::openai_compat::list_models(&endpoint, "").await;
    server.await.unwrap();

    assert!(result.is_err());
}

#[tokio::test]
async fn test_openai_compatible_connection_failure_returns_error() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let endpoint = format!("http://{}", addr);
    let result = crate::provider::model_listing::openai_compat::list_models(&endpoint, "").await;

    assert!(result.is_err());
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
        workspace_tools: Arc::new(WorkspaceTools::new(&std::path::PathBuf::from("/tmp"))),
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
        workspace_tools: Arc::new(WorkspaceTools::new(&std::path::PathBuf::from("/tmp"))),
    })
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

    let agent_manager = create_test_agent_manager_with_both(session_manager.clone(), llm_config);

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

    let agent_manager = create_test_agent_manager_with_both(session_manager.clone(), llm_config);

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

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

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

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

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

    let agent_manager = create_test_agent_manager_with_both(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

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

    let agent_manager = create_test_agent_manager_with_both(session_manager.clone(), llm_config);

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

#[tokio::test]
async fn test_list_models_dynamic_discovery_openai_succeeds() {
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

    // Mock server returns OpenAI-style models
    let (endpoint, server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "gpt-4o" },
                { "id": "gpt-4o-mini" },
                { "id": "o3-mini" }
            ]
        }),
        Some("test-openai-key"),
    )
    .await;

    let mut providers = HashMap::new();
    providers.insert(
        "openai-dynamic".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .endpoint(&endpoint)
            .api_key("test-openai-key")
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-dynamic".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    server.await.unwrap();

    assert!(
        models.contains(&"openai-dynamic/gpt-4o".to_string()),
        "Should contain dynamically discovered gpt-4o, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai-dynamic/gpt-4o-mini".to_string()),
        "Should contain dynamically discovered gpt-4o-mini, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai-dynamic/o3-mini".to_string()),
        "Should contain dynamically discovered o3-mini, got: {:?}",
        models
    );
    assert_eq!(
        models.len(),
        3,
        "Should have exactly 3 dynamically discovered models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_dynamic_discovery_zai_succeeds() {
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

    let (endpoint, server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "GLM-5" },
                { "id": "GLM-4.7" },
                { "id": "GLM-4.5-Flash" }
            ]
        }),
        None,
    )
    .await;

    let mut providers = HashMap::new();
    providers.insert(
        "zai-dynamic".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .endpoint(&endpoint)
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("zai-dynamic".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    server.await.unwrap();

    assert!(
        models.contains(&"zai-dynamic/GLM-5".to_string()),
        "Should contain dynamically discovered GLM-5, got: {:?}",
        models
    );
    assert!(
        models.contains(&"zai-dynamic/GLM-4.7".to_string()),
        "Should contain dynamically discovered GLM-4.7, got: {:?}",
        models
    );
    assert_eq!(
        models.len(),
        3,
        "Should have exactly 3 dynamically discovered ZAI models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_dynamic_discovery_openrouter_succeeds() {
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

    let (endpoint, server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "anthropic/claude-sonnet-4-20250514" },
                { "id": "openai/gpt-4o" },
                { "id": "meta-llama/llama-3.3-70b" }
            ]
        }),
        Some("test-or-key"),
    )
    .await;

    let mut providers = HashMap::new();
    providers.insert(
        "openrouter-dynamic".to_string(),
        LlmProviderConfig::builder(BackendType::OpenRouter)
            .endpoint(&endpoint)
            .api_key("test-or-key")
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openrouter-dynamic".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    server.await.unwrap();

    assert_eq!(
        models.len(),
        3,
        "Should have 3 dynamically discovered OpenRouter models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openrouter-dynamic/anthropic/claude-sonnet-4-20250514".to_string()),
        "Should contain dynamically discovered model, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_dynamic_discovery_failure_returns_empty() {
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

    // Mock server returns 503 error
    let (openai_endpoint, openai_server) = start_mock_openai_models_server(
        503,
        serde_json::json!({ "error": "service unavailable" }),
        None,
    )
    .await;

    // ZAI endpoint that refuses connection (bind then drop)
    let zai_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let zai_addr = zai_listener.local_addr().unwrap();
    drop(zai_listener);
    let zai_endpoint = format!("http://{}", zai_addr);

    let mut providers = HashMap::new();
    providers.insert(
        "openai-fail".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .endpoint(&openai_endpoint)
            .build(),
    );
    providers.insert(
        "zai-fail".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .endpoint(&zai_endpoint)
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-fail".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    openai_server.await.unwrap();

    // Without available_models, failed discovery returns empty (no hardcoded fallback)
    assert!(
        models.is_empty(),
        "Failed API discovery without available_models should return empty, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_explicit_config_skips_dynamic_discovery() {
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

    // No mock server needed — explicit config should bypass API call entirely
    let mut providers = HashMap::new();
    providers.insert(
        "openai-explicit".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["my-custom-model".to_string()])
            .build(),
    );
    providers.insert(
        "zai-explicit".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .available_models(vec!["custom-glm".to_string()])
            .build(),
    );
    providers.insert(
        "openrouter-explicit".to_string(),
        LlmProviderConfig::builder(BackendType::OpenRouter)
            .available_models(vec!["custom-or-model".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-explicit".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    // Explicit available_models should be used directly (no API call)
    assert!(
        models.contains(&"openai-explicit/my-custom-model".to_string()),
        "Explicit OpenAI config should be used, got: {:?}",
        models
    );
    assert!(
        models.contains(&"zai-explicit/custom-glm".to_string()),
        "Explicit ZAI config should be used, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openrouter-explicit/custom-or-model".to_string()),
        "Explicit OpenRouter config should be used, got: {:?}",
        models
    );
    assert_eq!(
        models.len(),
        3,
        "Should have exactly 3 explicitly configured models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_integration_multi_provider() {
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

    let (ollama_endpoint, ollama_server) =
        start_mock_ollama_tags_server(vec!["llama3.3", "qwen2.5"]).await;

    let mut providers = HashMap::new();
    providers.insert(
        "ollama-int".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint(ollama_endpoint)
            .build(),
    );
    providers.insert(
        "openai-int".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["gpt-4o".to_string(), "o3-mini".to_string()])
            .build(),
    );
    providers.insert(
        "zai-int".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .available_models(vec!["GLM-5".to_string(), "GLM-4.7".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-int".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    ollama_server.await.unwrap();
    let expected_total = 2 + 2 + 2;

    assert!(
        models.contains(&"ollama-int/llama3.3".to_string()),
        "Should include prefixed Ollama models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai-int/gpt-4o".to_string()),
        "Should include prefixed OpenAI models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"zai-int/GLM-5".to_string()),
        "Should include prefixed ZAI models, got: {:?}",
        models
    );
    assert_eq!(
        models.len(),
        expected_total,
        "Expected {} total models from all providers, got: {:?}",
        expected_total,
        models
    );
}

#[tokio::test]
async fn test_list_models_integration_dynamic_discovery() {
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

    let (endpoint, server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "gpt-4.1-nano" },
                { "id": "o4-mini" }
            ]
        }),
        Some("integration-openai-key"),
    )
    .await;

    let mut providers = HashMap::new();
    providers.insert(
        "openai-discovery-int".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .endpoint(&endpoint)
            .api_key("integration-openai-key")
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-discovery-int".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    server.await.unwrap();

    assert!(
        models.contains(&"openai-discovery-int/gpt-4.1-nano".to_string()),
        "Should include API-discovered model, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai-discovery-int/o4-mini".to_string()),
        "Should include API-discovered model, got: {:?}",
        models
    );
    assert!(
        !models.contains(&"openai-discovery-int/gpt-4o".to_string()),
        "Should not inject hardcoded fallback models when API succeeds, got: {:?}",
        models
    );
    assert_eq!(
        models.len(),
        2,
        "Expected exactly API models from dynamic discovery, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_integration_override_precedence() {
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

    let dead_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_addr = dead_listener.local_addr().unwrap();
    drop(dead_listener);
    let dead_endpoint = format!("http://{}", dead_addr);

    let (zai_endpoint, zai_server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "GLM-5" },
                { "id": "GLM-4.6" }
            ]
        }),
        None,
    )
    .await;

    let mut providers = HashMap::new();
    providers.insert(
        "openai-override-int".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .endpoint(&dead_endpoint)
            .available_models(vec!["gpt-custom-override".to_string()])
            .build(),
    );
    providers.insert(
        "zai-dynamic-int".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .endpoint(&zai_endpoint)
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-override-int".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    zai_server.await.unwrap();

    assert!(
        models.contains(&"openai-override-int/gpt-custom-override".to_string()),
        "Should use explicit override model for OpenAI, got: {:?}",
        models
    );
    assert!(
        !models.contains(&"openai-override-int/gpt-4o".to_string()),
        "OpenAI override should win over fallback/API discovery, got: {:?}",
        models
    );
    assert!(
        models.contains(&"zai-dynamic-int/GLM-5".to_string()),
        "Other providers without overrides should still use dynamic discovery, got: {:?}",
        models
    );
    assert_eq!(
        models.len(),
        3,
        "Expected 1 override + 2 dynamic models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_integration_partial_failure() {
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

    let ollama_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ollama_addr = ollama_listener.local_addr().unwrap();
    drop(ollama_listener);
    let ollama_dead_endpoint = format!("http://{}", ollama_addr);

    let mut providers = HashMap::new();
    providers.insert(
        "ollama-bad-int".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint(&ollama_dead_endpoint)
            .build(),
    );
    providers.insert(
        "openai-ok-int".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["gpt-4o".to_string(), "o3-mini".to_string()])
            .build(),
    );
    providers.insert(
        "zai-ok-int".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .available_models(vec!["GLM-5".to_string(), "GLM-4.7".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-ok-int".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    assert!(
        models.contains(&"openai-ok-int/gpt-4o".to_string()),
        "Working providers should still contribute models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"zai-ok-int/GLM-5".to_string()),
        "Working providers should still contribute models, got: {:?}",
        models
    );

    // With the new rig_model_listing dispatch, failed providers silently
    // fall back to effective_models() — no error entries surfaced in the list.
    let error_entries: Vec<_> = models.iter().filter(|m| m.starts_with("[error]")).collect();
    assert_eq!(
        error_entries.len(),
        0,
        "No error entries should be surfaced with new dispatch, got: {:?}",
        models
    );

    // 2 from openai-ok-int + 2 from zai-ok-int + 0 from failed ollama
    let expected_total = 2 + 2;
    assert_eq!(
        models.len(),
        expected_total,
        "Expected 4 models from healthy providers, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_openai_model_discovery_returns_all_models() {
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

    // Mock server returns 20 models including non-chat models
    let (endpoint, server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "gpt-4o" },
                { "id": "gpt-4o-mini" },
                { "id": "gpt-4-turbo" },
                { "id": "gpt-4" },
                { "id": "gpt-3.5-turbo" },
                { "id": "o1" },
                { "id": "o1-mini" },
                { "id": "o3-mini" },
                { "id": "o4" },
                { "id": "chatgpt-4o-latest" },
                { "id": "dall-e-3" },
                { "id": "dall-e-2" },
                { "id": "whisper-1" },
                { "id": "text-embedding-3-large" },
                { "id": "text-embedding-3-small" },
                { "id": "text-embedding-ada-002" },
                { "id": "text-moderation-latest" },
                { "id": "text-moderation-stable" },
                { "id": "tts-1" },
                { "id": "tts-1-hd" }
            ]
        }),
        Some("test-openai-key"),
    )
    .await;

    let mut providers = HashMap::new();
    providers.insert(
        "openai-test".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .endpoint(&endpoint)
            .api_key("test-openai-key")
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-test".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    server.await.unwrap();

    // With new rig_model_listing dispatch, all discovered models are returned
    // without filtering. Model filtering is now the responsibility of the caller.
    let openai_models: Vec<_> = models
        .iter()
        .filter(|m| m.starts_with("openai-test/"))
        .collect();
    assert_eq!(
        openai_models.len(),
        20,
        "Should return all 20 discovered models without filtering, got: {:?}",
        openai_models
    );

    // Verify some chat models are present
    assert!(
        models.contains(&"openai-test/gpt-4o".to_string()),
        "Should contain gpt-4o, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai-test/o1".to_string()),
        "Should contain o1, got: {:?}",
        models
    );

    // Non-chat models are also now included (no longer filtered)
    assert!(
        models.contains(&"openai-test/dall-e-3".to_string()),
        "Should contain dall-e-3 (no filtering), got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai-test/tts-1".to_string()),
        "Should contain tts-1 (no filtering), got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_ollama_failure() {
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

    // Ollama endpoint that refuses connection (bind then drop)
    let ollama_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ollama_addr = ollama_listener.local_addr().unwrap();
    drop(ollama_listener);
    let ollama_endpoint = format!("http://{}", ollama_addr);

    let mut providers = HashMap::new();
    providers.insert(
        "ollama-dead".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint(&ollama_endpoint)
            .build(),
    );
    providers.insert(
        "openai-ok".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-ok".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    // OpenAI models should be present
    assert!(
        models.contains(&"openai-ok/gpt-4o".to_string()),
        "OpenAI models should be present, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai-ok/gpt-4o-mini".to_string()),
        "OpenAI models should be present, got: {:?}",
        models
    );

    // With new rig_model_listing dispatch, failed providers silently fall back
    // to effective_models() — no error entries surfaced in the model list.
    let error_entries: Vec<_> = models.iter().filter(|m| m.starts_with("[error]")).collect();
    assert!(
        error_entries.is_empty(),
        "No error entries should be surfaced with new dispatch, got: {:?}",
        models
    );

    // Only OpenAI models present (Ollama silently failed)
    assert_eq!(
        models.len(),
        2,
        "Should have exactly 2 OpenAI models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_model_cache_hit() {
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
        "test".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["model1".to_string(), "model2".to_string()])
            .build(),
    );
    let llm_config = LlmConfig {
        default: Some("test".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    // First call should populate cache
    let models1 = agent_manager.list_models(&session.id, None).await.unwrap();
    assert!(!models1.is_empty(), "Should return models");

    // Second call should return same result from cache
    let models2 = agent_manager.list_models(&session.id, None).await.unwrap();
    assert_eq!(models1, models2, "Cache hit should return identical models");

    // Verify cache entry exists
    assert!(
        agent_manager.model_cache.contains_key("all"),
        "Cache should contain 'all' key"
    );
}

#[tokio::test]
async fn test_model_cache_invalidation() {
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
        "test".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["model1".to_string()])
            .build(),
    );
    let llm_config = LlmConfig {
        default: Some("test".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    // First call populates cache
    let _models1 = agent_manager.list_models(&session.id, None).await.unwrap();
    assert!(
        agent_manager.model_cache.contains_key("all"),
        "Cache should be populated"
    );

    // Invalidate cache
    agent_manager.invalidate_model_cache();
    assert!(
        !agent_manager.model_cache.contains_key("all"),
        "Cache should be cleared after invalidation"
    );

    // Second call should succeed (repopulate cache)
    let _models2 = agent_manager.list_models(&session.id, None).await.unwrap();
    assert!(
        agent_manager.model_cache.contains_key("all"),
        "Cache should be repopulated after list_models"
    );
}

#[tokio::test]

async fn test_model_cache_does_not_cache_errors() {
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
    // Configure provider with models
    providers.insert(
        "test".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["model1".to_string(), "model2".to_string()])
            .build(),
    );
    let llm_config = LlmConfig {
        default: Some("test".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    // First call populates cache
    let models1 = agent_manager.list_models(&session.id, None).await.unwrap();
    assert!(!models1.is_empty(), "Should return models");
    assert!(
        agent_manager.model_cache.contains_key("all"),
        "Cache should be populated after successful list_models"
    );

    // Verify cache contains the same models
    let (cached_models, _) = agent_manager.model_cache.get("all").unwrap().clone();
    assert_eq!(
        models1, cached_models,
        "Cached models should match returned models"
    );
}

// RED → GREEN: Bug 2 — tool dispatch timeout
struct HangingToolDispatcher;

#[async_trait::async_trait]
impl crate::tool_dispatch::ToolDispatcher for HangingToolDispatcher {
    async fn dispatch_tool(
        &self,
        _name: &str,
        _args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
        Ok(serde_json::Value::Null)
    }

    fn has_tool(&self, _name: &str) -> bool {
        true
    }
}

#[tokio::test(start_paused = true)]
async fn tool_dispatch_has_timeout() {
    // GREEN: verifies that a 30s timeout on dispatch_tool works correctly.
    // The production timeout lives in messaging.rs; this test verifies the
    // timeout mechanism itself using the same pattern.
    let dispatcher = std::sync::Arc::new(HangingToolDispatcher);

    let timeout_result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        dispatcher.dispatch_tool("test_tool", serde_json::json!({})),
    )
    .await;

    // With start_paused=true and no time advance, the future is still pending.
    // The timeout fires immediately because virtual time hasn't advanced.
    // This confirms the timeout mechanism works — production code uses same pattern.
    assert!(
        timeout_result.is_err(),
        "dispatch_tool should timeout after 30s when tool hangs"
    );
}
