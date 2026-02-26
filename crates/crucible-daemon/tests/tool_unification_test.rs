use std::collections::HashSet;
use std::sync::Arc;

use crucible_acp::InProcessMcpHost;
use crucible_config::DataClassification;
use crucible_core::background::{BackgroundSpawner, JobError, JobId, JobInfo, JobResult};
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_tools::in_process_adapter::{InProcessMcpAdapter, PLAN_TOOL_NAMES};
use crucible_tools::mcp_server::{CrucibleMcpServer, DelegationContext};
use tempfile::TempDir;

const EXPECTED_TOOL_NAMES: &[&str] = &[
    "semantic_search",
    "text_search",
    "property_search",
    "list_notes",
    "read_note",
    "read_metadata",
    "get_kiln_info",
    "list_jobs",
    "create_note",
    "update_note",
    "delete_note",
    "delegate_session",
    "get_job_result",
    "cancel_job",
];

struct MockKnowledgeRepository;
struct MockEmbeddingProvider;
struct MockBackgroundSpawner;

#[async_trait::async_trait]
impl KnowledgeRepository for MockKnowledgeRepository {
    async fn get_note_by_name(
        &self,
        _name: &str,
    ) -> crucible_core::Result<Option<crucible_core::parser::ParsedNote>> {
        Ok(None)
    }

    async fn list_notes(
        &self,
        _path: Option<&str>,
    ) -> crucible_core::Result<Vec<crucible_core::traits::knowledge::NoteInfo>> {
        Ok(vec![])
    }

    async fn search_vectors(
        &self,
        _vector: Vec<f32>,
    ) -> crucible_core::Result<Vec<crucible_core::types::SearchResult>> {
        Ok(vec![])
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(vec![0.1; 384])
    }

    async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(vec![vec![0.1; 384]; texts.len()])
    }

    fn model_name(&self) -> &str {
        "mock-model"
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec!["mock-model".to_string()])
    }
}

#[async_trait::async_trait]
impl BackgroundSpawner for MockBackgroundSpawner {
    async fn spawn_bash(
        &self,
        _session_id: &str,
        _command: String,
        _workdir: Option<std::path::PathBuf>,
        _timeout: Option<std::time::Duration>,
    ) -> Result<JobId, JobError> {
        Err(JobError::SpawnFailed("not used in test".to_string()))
    }

    async fn spawn_subagent(
        &self,
        _session_id: &str,
        _prompt: String,
        _context: Option<String>,
    ) -> Result<JobId, JobError> {
        Ok("job-id".to_string())
    }

    async fn spawn_subagent_blocking(
        &self,
        _session_id: &str,
        _prompt: String,
        _context: Option<String>,
        _config: crucible_core::background::SubagentBlockingConfig,
        _cancel_rx: Option<tokio::sync::oneshot::Receiver<()>>,
    ) -> Result<JobResult, JobError> {
        Err(JobError::SpawnFailed("not used in test".to_string()))
    }

    fn list_jobs(&self, _session_id: &str) -> Vec<JobInfo> {
        vec![]
    }

    fn get_job_result(&self, _job_id: &JobId) -> Option<JobResult> {
        None
    }

    async fn cancel_job(&self, _job_id: &JobId) -> bool {
        false
    }
}

fn to_set(names: &[&str]) -> HashSet<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

fn create_adapter(temp: &TempDir) -> InProcessMcpAdapter {
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
    let server = CrucibleMcpServer::new(
        temp.path().to_string_lossy().to_string(),
        knowledge_repo,
        embedding_provider,
    );
    InProcessMcpAdapter::new(Arc::new(server))
}

fn create_disabled_delegation_adapter(temp: &TempDir) -> InProcessMcpAdapter {
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let server = CrucibleMcpServer::new_with_delegation(
        temp.path().to_string_lossy().to_string(),
        knowledge_repo,
        embedding_provider,
        Some(DelegationContext {
            background_spawner: Arc::new(MockBackgroundSpawner),
            session_id: "session-tool-unification".to_string(),
            targets: vec![],
            enabled: false,
            depth: 0,
            data_classification: DataClassification::Public,
        }),
    );

    InProcessMcpAdapter::new(Arc::new(server))
}

fn is_permission_denied(err: &crucible_acp::ClientError) -> bool {
    matches!(
        err,
        crucible_acp::ClientError::Connection(message) if message.contains("Operation not permitted")
    )
}

async fn start_mcp_host(
    kiln_path: std::path::PathBuf,
    knowledge_repo: Arc<dyn KnowledgeRepository>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
) -> InProcessMcpHost {
    match InProcessMcpHost::start(kiln_path, knowledge_repo, embedding_provider, None).await {
        Ok(host) => host,
        Err(err) => {
            if is_permission_denied(&err) {
                panic!(
                    "In-process MCP HTTP server requires localhost bind (sandbox denied): {}",
                    err
                );
            }
            panic!("failed to start in-process MCP host: {err:?}");
        }
    }
}

async fn list_tool_names_over_http(host: &InProcessMcpHost) -> Vec<String> {
    let client = reqwest::Client::new();
    let url = host.mcp_url();

    let init_resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"tool-unification-test","version":"0.1.0"}}}"#)
        .send()
        .await
        .expect("initialize should succeed");
    assert!(init_resp.status().is_success());

    let session_id = init_resp
        .headers()
        .get("mcp-session-id")
        .expect("initialize should return mcp-session-id")
        .to_str()
        .expect("session id should be valid header value")
        .to_string();

    let _ = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", &session_id)
        .body(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .send()
        .await
        .expect("initialized notification should succeed");

    let tools_resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", &session_id)
        .body(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#)
        .send()
        .await
        .expect("tools/list should succeed");
    assert!(tools_resp.status().is_success());

    let body = tools_resp.text().await.expect("tools/list body");
    let payload = body
        .lines()
        .find(|line| line.starts_with("data: {"))
        .and_then(|line| line.strip_prefix("data: "))
        .unwrap_or(body.as_str());

    let parsed: serde_json::Value = serde_json::from_str(payload).expect("valid JSON response");
    let tools = parsed["result"]["tools"]
        .as_array()
        .expect("tools/list response should include result.tools");

    tools
        .iter()
        .map(|tool| {
            tool["name"]
                .as_str()
                .expect("each tool should have a name")
                .to_string()
        })
        .collect()
}

#[test]
fn test_internal_agent_tool_names() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = create_adapter(&temp);
    let tool_names: HashSet<String> = adapter.list_tool_names().into_iter().collect();

    assert_eq!(tool_names, to_set(EXPECTED_TOOL_NAMES));
}

#[tokio::test]
async fn test_acp_mcp_server_tool_names() {
    let temp = TempDir::new().expect("temp dir");
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let tool_names: HashSet<String> = list_tool_names_over_http(&host).await.into_iter().collect();
    host.shutdown().await;

    assert_eq!(tool_names, to_set(EXPECTED_TOOL_NAMES));
}

#[tokio::test]
async fn test_plan_mode_tool_filtering() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = create_adapter(&temp);

    let full_names: HashSet<String> = adapter.list_tool_names().into_iter().collect();
    let plan_names: HashSet<String> = adapter
        .create_rig_tools("plan")
        .iter()
        .map(|tool| tool.name())
        .collect();

    assert_eq!(full_names.len(), 14);
    assert_eq!(plan_names, to_set(PLAN_TOOL_NAMES));
    assert!(!plan_names.contains("create_note"));
    assert!(!plan_names.contains("delegate_session"));
    assert!(!plan_names.contains("cancel_job"));
}

#[tokio::test]
async fn test_internal_agent_tool_call_e2e() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = create_adapter(&temp);

    let tools = adapter.create_rig_tools("auto");
    let tool = tools
        .iter()
        .find(|tool| tool.name() == "get_kiln_info")
        .expect("get_kiln_info should be available");

    let response = tool
        .call("{}".to_string())
        .await
        .expect("tool call should succeed");

    assert!(!response.is_empty());
    assert!(response.contains("name"));
}

#[tokio::test]
async fn test_delegation_disabled_behavior() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = create_disabled_delegation_adapter(&temp);

    let tools = adapter.create_rig_tools("auto");
    let delegate_tool = tools
        .iter()
        .find(|tool| tool.name() == "delegate_session")
        .expect("delegate_session should be present");

    let err = delegate_tool
        .call(r#"{"prompt":"do work","background":true}"#.to_string())
        .await
        .expect_err("delegate_session should fail when delegation is disabled");

    assert!(err.to_string().contains("disabled"));
}
