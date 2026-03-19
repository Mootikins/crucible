use async_trait::async_trait;
use crucible_config::DataClassification;
use crucible_core::background::{BackgroundSpawner, JobError, JobId, JobInfo, JobResult};
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_daemon::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};
use crucible_daemon::tools::{CrucibleMcpServer, DelegationContext};
use crucible_daemon::InProcessMcpHost;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

struct MockSpawner;

#[async_trait]
impl BackgroundSpawner for MockSpawner {
    async fn spawn_bash(
        &self,
        _session_id: &str,
        _command: String,
        _workdir: Option<PathBuf>,
        _timeout: Option<Duration>,
    ) -> Result<JobId, JobError> {
        Ok("mock-bash-job".to_string())
    }

    async fn spawn_subagent(
        &self,
        _session_id: &str,
        _prompt: String,
        _context: Option<String>,
    ) -> Result<JobId, JobError> {
        Ok("mock-subagent-job".to_string())
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

fn delegation_context(enabled: bool) -> DelegationContext {
    DelegationContext {
        background_spawner: Arc::new(MockSpawner),
        session_id: "acp-delegation-e2e-session".to_string(),
        targets: vec!["claude".to_string()],
        enabled,
        depth: 0,
        data_classification: DataClassification::default(),
    }
}

fn parse_jsonrpc_payload(body: &str) -> serde_json::Value {
    let payload = body
        .lines()
        .find(|line| line.starts_with("data: {"))
        .and_then(|line| line.strip_prefix("data: "))
        .unwrap_or(body);

    serde_json::from_str(payload).expect("valid JSON-RPC payload")
}

async fn initialize_mcp_session(client: &reqwest::Client, url: &str, client_name: &str) -> String {
    let init_body = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"2025-03-26\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"{}\",\"version\":\"0.1.0\"}}}}}}",
        client_name
    );

    let init_resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(init_body)
        .send()
        .await
        .expect("initialize should succeed");
    assert!(init_resp.status().is_success());

    let session_id = init_resp
        .headers()
        .get("mcp-session-id")
        .expect("initialize should return mcp-session-id")
        .to_str()
        .expect("session id should be a valid header value")
        .to_string();

    let initialized_resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", &session_id)
        .body(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .send()
        .await
        .expect("initialized notification should succeed");
    assert!(initialized_resp.status().is_success());

    session_id
}

async fn call_tools_list(
    client: &reqwest::Client,
    url: &str,
    session_id: &str,
) -> serde_json::Value {
    let tools_resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", session_id)
        .body(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#)
        .send()
        .await
        .expect("tools/list should succeed");
    assert!(tools_resp.status().is_success());

    let body = tools_resp.text().await.expect("tools/list body");
    parse_jsonrpc_payload(&body)
}

async fn call_semantic_search(
    client: &reqwest::Client,
    url: &str,
    session_id: &str,
    request_id: u64,
) -> serde_json::Value {
    let search_body = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":{},\"method\":\"tools/call\",\"params\":{{\"name\":\"semantic_search\",\"arguments\":{{\"query\":\"acp-delegation-e2e\",\"limit\":5}}}}}}",
        request_id
    );

    let search_resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", session_id)
        .body(search_body)
        .send()
        .await
        .expect("tools/call semantic_search should return response");
    assert!(search_resp.status().is_success());

    let body = search_resp.text().await.expect("semantic_search body");
    parse_jsonrpc_payload(&body)
}

#[tokio::test]
async fn test_acp_delegation_pipeline_all_fixes_work() {
    let temp = TempDir::new().expect("temp dir");
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = match InProcessMcpHost::start(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
        None,
    )
    .await
    {
        Ok(host) => host,
        Err(err) => {
            let err_str = format!("{err:?}");
            if err_str.contains("Operation not permitted") {
                eprintln!("Skipping test (permission denied in environment)");
                return;
            }
            panic!("InProcessMcpHost::start should succeed: {err:?}");
        }
    };

    let client = reqwest::Client::new();
    let url = host.mcp_url();
    let session_id = initialize_mcp_session(&client, &url, "acp-delegation-e2e-providers-only").await;
    let search_payload = call_semantic_search(&client, &url, &session_id, 3).await;

    assert!(
        search_payload.get("result").is_some(),
        "semantic_search should succeed with real providers, got: {search_payload}",
    );
    assert!(
        search_payload.get("error").is_none(),
        "semantic_search should not return provider error, got: {search_payload}",
    );

    host.shutdown().await;

    let (temp_none, knowledge_none, embedding_none) = {
        let temp = TempDir::new().expect("temp dir");
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
        (temp, knowledge_repo, embedding_provider)
    };
    let server_none = CrucibleMcpServer::new_with_delegation(
        temp_none.path().to_string_lossy().to_string(),
        knowledge_none,
        embedding_none,
        None,
    );
    let names_none: Vec<String> = server_none
        .list_tools()
        .iter()
        .map(|tool| tool.name.to_string())
        .collect();
    assert!(
        !names_none.contains(&"delegate_session".to_string()),
        "delegate_session should be hidden when delegation context is None, found: {names_none:?}",
    );

    let (temp_disabled, knowledge_disabled, embedding_disabled) = {
        let temp = TempDir::new().expect("temp dir");
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
        (temp, knowledge_repo, embedding_provider)
    };
    let server_disabled = CrucibleMcpServer::new_with_delegation(
        temp_disabled.path().to_string_lossy().to_string(),
        knowledge_disabled,
        embedding_disabled,
        Some(delegation_context(false)),
    );
    let names_disabled: Vec<String> = server_disabled
        .list_tools()
        .iter()
        .map(|tool| tool.name.to_string())
        .collect();
    assert!(
        !names_disabled.contains(&"delegate_session".to_string()),
        "delegate_session should be hidden when delegation is disabled, found: {names_disabled:?}",
    );

    let (temp_enabled, knowledge_enabled, embedding_enabled) = {
        let temp = TempDir::new().expect("temp dir");
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
        (temp, knowledge_repo, embedding_provider)
    };
    let server_enabled = CrucibleMcpServer::new_with_delegation(
        temp_enabled.path().to_string_lossy().to_string(),
        knowledge_enabled,
        embedding_enabled,
        Some(delegation_context(true)),
    );
    let names_enabled: Vec<String> = server_enabled
        .list_tools()
        .iter()
        .map(|tool| tool.name.to_string())
        .collect();
    assert!(
        names_enabled.contains(&"delegate_session".to_string()),
        "delegate_session should be visible when delegation is enabled, found: {names_enabled:?}",
    );

    let temp = TempDir::new().expect("temp dir");
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = match InProcessMcpHost::start(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
        Some(delegation_context(true)),
    )
    .await
    {
        Ok(host) => host,
        Err(err) => {
            let err_str = format!("{err:?}");
            if err_str.contains("Operation not permitted") {
                eprintln!("Skipping test (permission denied in environment)");
                return;
            }
            panic!("InProcessMcpHost::start with delegation should succeed: {err:?}");
        }
    };

    let client = reqwest::Client::new();
    let url = host.mcp_url();
    let session_id = initialize_mcp_session(&client, &url, "acp-delegation-e2e-integration").await;

    let tools_payload = call_tools_list(&client, &url, &session_id).await;
    let tools = tools_payload["result"]["tools"]
        .as_array()
        .expect("tools/list response should include result.tools");
    let tool_names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(
        tool_names.iter().any(|name| *name == "delegate_session"),
        "tools/list should include delegate_session when delegation is enabled, got: {tool_names:?}",
    );

    let search_payload = call_semantic_search(&client, &url, &session_id, 4).await;
    assert!(
        search_payload.get("result").is_some(),
        "semantic_search should succeed in integration scenario, got: {search_payload}",
    );
    assert!(
        search_payload.get("error").is_none(),
        "semantic_search should not fail in integration scenario, got: {search_payload}",
    );

    host.shutdown().await;
}
