#[path = "acp_support/mcp_http.rs"]
mod mcp_http;

use async_trait::async_trait;
use crucible_core::background::{BackgroundSpawner, JobError, JobId, JobInfo, JobKind, JobResult};
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_daemon::delegation::{DelegationRequest, DelegationSpawned, DelegationSpawner};
use crucible_daemon::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};
use crucible_daemon::tools::{CrucibleMcpServer, DelegationContext};
use mcp_http::{mcp_http_open_session, mcp_http_request, start_host};
use serde_json::{json, Value};
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

/// Delegation-side test spawner: returns canned completed results. These
/// tests only exercise tool visibility, so the spawner is never actually
/// driven — it just satisfies the `DelegationContext` field.
struct MockDelegationSpawner;

#[async_trait]
impl DelegationSpawner for MockDelegationSpawner {
    async fn spawn_delegation(
        &self,
        _req: DelegationRequest,
    ) -> Result<DelegationSpawned, JobError> {
        Ok(DelegationSpawned {
            delegation_id: "agent-child-test".to_string(),
            child_session_id: "agent-child-test".to_string(),
            message_id: "msg-test".to_string(),
        })
    }

    async fn await_delegation(
        &self,
        delegation_id: &str,
        _timeout: Duration,
    ) -> Result<JobResult, JobError> {
        let mut info = JobInfo::new(
            "acp-delegation-e2e-session".to_string(),
            JobKind::Subagent {
                prompt: "test".to_string(),
                context: None,
            },
        );
        info.id = delegation_id.to_string();
        info.mark_completed();
        Ok(JobResult::success(info, "done".to_string()))
    }

    fn list_delegations(&self, _parent_session_id: &str) -> Vec<JobInfo> {
        Vec::new()
    }

    fn get_delegation_result(&self, _delegation_id: &str) -> Option<JobResult> {
        None
    }

    async fn cancel_delegation(&self, _delegation_id: &str) -> bool {
        false
    }
}

fn delegation_context(enabled: bool) -> DelegationContext {
    DelegationContext {
        background_spawner: Arc::new(MockSpawner),
        delegation_spawner: Arc::new(MockDelegationSpawner),
        session_id: "acp-delegation-e2e-session".to_string(),
        targets: vec!["claude".to_string()],
        enabled,
        result_max_bytes: 51200,
        timeout_secs: 300,
        source_roots: Default::default(),
    }
}

/// Call `semantic_search` on an open MCP session.
async fn semantic_search(client: &reqwest::Client, url: &str, session_id: &str, id: u64) -> Value {
    let args = json!({"name": "semantic_search", "arguments": {"query": "acp-delegation-e2e", "limit": 5}});
    mcp_http_request(client, url, session_id, id, "tools/call", args).await
}

/// The MCP server an ACP agent reaches answers `semantic_search` through
/// its providers, and lists `delegate_session` only when the session's
/// delegation context is present and enabled. No ACP agent runs here: this
/// is the tool surface such an agent would see.
#[tokio::test]
async fn mcp_server_serves_search_and_gates_delegate_session_on_delegation() {
    let temp = TempDir::new().expect("temp dir");
    let host = start_host(temp.path(), None).await;

    let client = reqwest::Client::new();
    let url = host.mcp_url();
    let session_id = mcp_http_open_session(&client, &url).await;
    let search_payload = semantic_search(&client, &url, &session_id, 3).await;

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
        let knowledge_repo =
            Arc::new(MockKnowledgeRepository::new()) as Arc<dyn KnowledgeRepository>;
        let embedding_provider =
            Arc::new(MockEmbeddingProvider::new()) as Arc<dyn EmbeddingProvider>;
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
        let knowledge_repo =
            Arc::new(MockKnowledgeRepository::new()) as Arc<dyn KnowledgeRepository>;
        let embedding_provider =
            Arc::new(MockEmbeddingProvider::new()) as Arc<dyn EmbeddingProvider>;
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
        let knowledge_repo =
            Arc::new(MockKnowledgeRepository::new()) as Arc<dyn KnowledgeRepository>;
        let embedding_provider =
            Arc::new(MockEmbeddingProvider::new()) as Arc<dyn EmbeddingProvider>;
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
    let host = start_host(temp.path(), Some(delegation_context(true))).await;

    let client = reqwest::Client::new();
    let url = host.mcp_url();
    let session_id = mcp_http_open_session(&client, &url).await;

    let tools_payload =
        mcp_http_request(&client, &url, &session_id, 2, "tools/list", json!({})).await;
    let tools = tools_payload["result"]["tools"]
        .as_array()
        .expect("tools/list response should include result.tools");
    let tool_names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(
        tool_names.contains(&"delegate_session"),
        "tools/list should include delegate_session when delegation is enabled, got: {tool_names:?}",
    );

    let search_payload = semantic_search(&client, &url, &session_id, 4).await;
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
