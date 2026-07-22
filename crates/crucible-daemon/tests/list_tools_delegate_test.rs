use crucible_core::background::{BackgroundSpawner, JobError, JobId, JobInfo, JobKind, JobResult};
use crucible_core::config::DataClassification;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_daemon::delegation::{DelegationRequest, DelegationSpawned, DelegationSpawner};
use crucible_daemon::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};
use crucible_daemon::tools::{CrucibleMcpServer, DelegationContext};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

struct MockSpawner;

#[async_trait::async_trait]
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

#[async_trait::async_trait]
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
            "test-session".to_string(),
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

fn tool_names(server: &CrucibleMcpServer) -> Vec<String> {
    server
        .list_tools()
        .iter()
        .map(|tool| tool.name.to_string())
        .collect()
}

fn test_dependencies() -> (
    TempDir,
    Arc<dyn KnowledgeRepository>,
    Arc<dyn EmbeddingProvider>,
) {
    let temp = TempDir::new().expect("temp dir");
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
    (temp, knowledge_repo, embedding_provider)
}

fn delegation_context(enabled: bool) -> DelegationContext {
    DelegationContext {
        background_spawner: Arc::new(MockSpawner),
        delegation_spawner: Arc::new(MockDelegationSpawner),
        session_id: "test-session".to_string(),
        targets: vec!["claude".to_string()],
        enabled,
        depth: 0,
        result_max_bytes: 51200,
        timeout_secs: 300,
        data_classification: DataClassification::default(),
    }
}

#[test]
fn test_delegate_session_hidden_when_no_delegation_context() {
    let (temp, knowledge_repo, embedding_provider) = test_dependencies();
    let server = CrucibleMcpServer::new_with_delegation(
        temp.path().to_string_lossy().to_string(),
        knowledge_repo,
        embedding_provider,
        None,
    );

    let names = tool_names(&server);
    assert!(
        !names.contains(&"delegate_session".to_string()),
        "delegate_session should be hidden when delegation_context is None, found tools: {names:?}"
    );
}

#[test]
fn test_delegate_session_hidden_when_delegation_disabled() {
    let (temp, knowledge_repo, embedding_provider) = test_dependencies();
    let server = CrucibleMcpServer::new_with_delegation(
        temp.path().to_string_lossy().to_string(),
        knowledge_repo,
        embedding_provider,
        Some(delegation_context(false)),
    );

    let names = tool_names(&server);
    assert!(
        !names.contains(&"delegate_session".to_string()),
        "delegate_session should be hidden when delegation is disabled, found tools: {names:?}"
    );
}

#[test]
fn test_delegate_session_visible_when_delegation_enabled() {
    let (temp, knowledge_repo, embedding_provider) = test_dependencies();
    let server = CrucibleMcpServer::new_with_delegation(
        temp.path().to_string_lossy().to_string(),
        knowledge_repo,
        embedding_provider,
        Some(delegation_context(true)),
    );

    let names = tool_names(&server);
    assert!(
        names.contains(&"delegate_session".to_string()),
        "delegate_session should be visible when delegation is enabled, found tools: {names:?}"
    );
}
