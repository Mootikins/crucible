//! Tests for [`CrucibleMcpServer`].
//!
//! Split out of `mcp_server.rs` when the parent crossed the 1500-line ceiling
//! that `scripts/check-file-sizes.sh` enforces. Same arrangement as
//! `tools/surface.rs` and `tools/workspace.rs`: the tests are a sibling module,
//! not a whitelist entry.

use super::*;

use crucible_core::background::{JobError, JobInfo, JobKind, JobResult};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

use crate::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};

/// Bash-side test spawner: serves a single canned job `job-test-123` for
/// the job-tool tests.
#[derive(Default)]
struct MockBackgroundSpawner;

/// Delegation-side test spawner: counts spawns and returns canned
/// completed results.
#[derive(Default)]
struct MockDelegationSpawner {
    spawn_calls: AtomicUsize,
}

#[async_trait::async_trait]
impl crate::delegation::DelegationSpawner for MockDelegationSpawner {
    async fn spawn_delegation(
        &self,
        req: crate::delegation::DelegationRequest,
    ) -> Result<crate::delegation::DelegationSpawned, JobError> {
        self.spawn_calls.fetch_add(1, Ordering::SeqCst);
        let _ = req;
        Ok(crate::delegation::DelegationSpawned {
            delegation_id: "agent-child-test".to_string(),
            child_session_id: "agent-child-test".to_string(),
            message_id: "msg-test".to_string(),
        })
    }

    async fn await_delegation(
        &self,
        delegation_id: &str,
        _timeout: std::time::Duration,
    ) -> Result<JobResult, JobError> {
        let mut info = JobInfo::new(
            "chat-parent".to_string(),
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

#[async_trait::async_trait]
impl BackgroundSpawner for MockBackgroundSpawner {
    async fn spawn_bash(
        &self,
        _session_id: &str,
        _command: String,
        _workdir: Option<std::path::PathBuf>,
        _timeout: Option<std::time::Duration>,
    ) -> Result<String, JobError> {
        Err(JobError::SpawnFailed("not implemented in test".to_string()))
    }

    fn list_jobs(&self, session_id: &str) -> Vec<JobInfo> {
        let mut info = JobInfo::new(
            session_id.to_string(),
            JobKind::Subagent {
                prompt: "test task".to_string(),
                context: None,
            },
        );
        info.id = "job-test-123".to_string();
        vec![info]
    }

    fn get_job_result(&self, job_id: &String) -> Option<JobResult> {
        if job_id == "job-test-123" {
            let mut info = JobInfo::new(
                "test-session".to_string(),
                JobKind::Subagent {
                    prompt: "test".to_string(),
                    context: None,
                },
            );
            info.id = "job-test-123".to_string();
            info.mark_completed();
            Some(JobResult::success(info, "completed output".to_string()))
        } else {
            None
        }
    }

    async fn cancel_job(&self, job_id: &String) -> bool {
        job_id == "job-test-123"
    }
}

impl Default for DelegationContext {
    fn default() -> Self {
        Self {
            background_spawner: Arc::new(MockBackgroundSpawner),
            delegation_spawner: Arc::new(MockDelegationSpawner::default()),
            session_id: "chat-parent".to_string(),
            targets: vec![],
            enabled: true,
            depth: 0,
            result_max_bytes: 51200,
            timeout_secs: 300,
            data_classification: DataClassification::Public,
        }
    }
}

#[test]
fn test_server_creation() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let _server = CrucibleMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
    );

    // Server should create successfully
}

#[tokio::test]
async fn skill_view_appends_allowed_tools_advisory() {
    let ws = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();

    let skill = ws.path().join(".crucible").join("skills").join("scoped");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: scoped\ndescription: scoped skill\nallowed-tools: search_notes get_note\n---\n\nSCOPED-BODY.",
    )
    .unwrap();

    let server = CrucibleMcpServer::new_with_workspace_and_delegation(
        kiln.path().to_str().unwrap().to_string(),
        ws.path().to_path_buf(),
        Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
        Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
        None,
        crate::tools::containment::RootSet::Ambient,
    );

    let found = server
        .skill_view(Parameters(SkillViewParams {
            name: "scoped".to_string(),
        }))
        .await
        .unwrap();
    let json = serde_json::to_string(&found).unwrap();
    assert!(
        json.contains("SCOPED-BODY"),
        "body should be present: {json}"
    );
    assert!(
        json.contains("Tool restriction") && json.contains("search_notes, get_note"),
        "advisory listing the allowed tools should be appended, got: {json}"
    );
}

#[tokio::test]
async fn skill_view_finds_workspace_and_kiln_skills() {
    // workspace != kiln: skill_view must discover under both roots, the same
    // way the system-prompt catalog does.
    let ws = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();

    let ws_skill = ws.path().join(".crucible").join("skills").join("ws-skill");
    std::fs::create_dir_all(&ws_skill).unwrap();
    std::fs::write(
        ws_skill.join("SKILL.md"),
        "---\nname: ws-skill\ndescription: workspace skill\n---\n\nWS-BODY-MARKER.",
    )
    .unwrap();

    let kiln_skill = kiln
        .path()
        .join(".crucible")
        .join("skills")
        .join("kiln-skill");
    std::fs::create_dir_all(&kiln_skill).unwrap();
    std::fs::write(
        kiln_skill.join("SKILL.md"),
        "---\nname: kiln-skill\ndescription: kiln skill\n---\n\nKILN-BODY-MARKER.",
    )
    .unwrap();

    let server = CrucibleMcpServer::new_with_workspace_and_delegation(
        kiln.path().to_str().unwrap().to_string(),
        ws.path().to_path_buf(),
        Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
        Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
        None,
        crate::tools::containment::RootSet::Ambient,
    );

    for (name, marker) in [
        ("ws-skill", "WS-BODY-MARKER"),
        ("kiln-skill", "KILN-BODY-MARKER"),
    ] {
        let found = server
            .skill_view(Parameters(SkillViewParams {
                name: name.to_string(),
            }))
            .await
            .unwrap();
        let json = serde_json::to_string(&found).unwrap();
        assert!(
            json.contains(marker),
            "skill_view('{name}') should return the body, got: {json}"
        );
    }

    let missing = server
        .skill_view(Parameters(SkillViewParams {
            name: "nonexistent".to_string(),
        }))
        .await
        .unwrap();
    let missing_json = serde_json::to_string(&missing).unwrap();
    assert!(
        missing_json.contains("No skill named 'nonexistent'"),
        "skill_view should report unknown skills, got: {missing_json}"
    );
}

#[test]
fn test_tool_router_creation() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let _server = CrucibleMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
    );

    // This should compile and not panic - the tool_router macro generates the router
    let _router = CrucibleMcpServer::tool_router();
}

/// Zero kilns is a legitimate session shape — a tools-only agent — and the
/// tools that need a corpus are REMOVED rather than left to fail at call
/// time. Advertising `create_note` with nowhere to write it hands the model
/// a capability it does not have, and the call would reach
/// `validate_path_within_kiln` with `""`, whose `canonicalize()` is an
/// unconditional ENOENT dressed up as a path-traversal refusal.
#[test]
fn a_kiln_less_server_advertises_no_kiln_backed_tools() {
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
    let server = CrucibleMcpServer::new(String::new(), knowledge_repo, embedding_provider);

    let names: Vec<String> = server
        .list_tools()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();

    for tool in KILN_BACKED_TOOLS {
        assert!(
            !names.iter().any(|n| n == tool),
            "{tool} needs a kiln and must not be advertised without one: {names:?}"
        );
    }
    assert!(
        names.iter().any(|n| n == "skill_view"),
        "workspace-backed tools survive a kiln-less session: {names:?}"
    );
}

/// The control: with a kiln attached, nothing is filtered out.
#[test]
fn a_kiln_backed_server_advertises_every_kiln_tool() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
    let server = CrucibleMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
    );

    let names: Vec<String> = server
        .list_tools()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();

    for tool in KILN_BACKED_TOOLS {
        assert!(
            names.iter().any(|n| n == tool),
            "{tool} missing from a kiln-backed server: {names:?}"
        );
    }
}

#[tokio::test]
async fn test_delegate_session_without_context_returns_graceful_error() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
    let server = CrucibleMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
    );

    let result = server
        .delegate_session(Parameters(DelegateSessionParams {
            prompt: "test".to_string(),
            description: Some("desc".to_string()),
            target: None,
            background: Some(true),
        }))
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("no daemon delegation context"));
}

#[tokio::test]
async fn test_delegate_session_spawns_background_subagent() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
    let spawner = Arc::new(MockDelegationSpawner::default());

    let server = CrucibleMcpServer::new_with_delegation(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
        Some(DelegationContext {
            delegation_spawner: spawner.clone(),
            targets: vec!["opencode".to_string()],
            ..Default::default()
        }),
    );

    let result = server
        .delegate_session(Parameters(DelegateSessionParams {
            prompt: "do work".to_string(),
            description: Some("desc".to_string()),
            target: Some("opencode".to_string()),
            background: Some(true),
        }))
        .await;

    assert!(result.is_ok());
    assert_eq!(spawner.spawn_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_delegate_session_description_includes_target_hints() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
    let server = CrucibleMcpServer::new_with_delegation(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
        Some(DelegationContext {
            targets: vec!["my-custom-agent".to_string(), "another-agent".to_string()],
            ..Default::default()
        }),
    );

    let tools = server.list_tools();
    let delegate_tool = tools
        .iter()
        .find(|t| t.name == "delegate_session")
        .expect("delegate_session tool should exist");

    let desc = delegate_tool
        .description
        .as_ref()
        .map(|d| d.as_ref())
        .unwrap_or("");
    assert!(
        desc.contains("my-custom-agent"),
        "Description should contain 'my-custom-agent' target. Got: {}",
        desc
    );
    assert!(
        desc.contains("another-agent"),
        "Description should contain 'another-agent' target. Got: {}",
        desc
    );
}

#[test]
fn test_delegate_session_filtered_when_no_delegation_context() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let server = CrucibleMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
    );

    let tools = server.list_tools();
    assert!(
        !tools.iter().any(|t| t.name == "delegate_session"),
        "delegate_session should be filtered out when no delegation context is set"
    );
}

#[test]
fn test_delegate_session_description_generic_when_empty_targets() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
    let server = CrucibleMcpServer::new_with_delegation(
        temp.path().to_str().unwrap().to_string(),
        knowledge_repo,
        embedding_provider,
        Some(DelegationContext {
            ..Default::default()
        }),
    );

    let tools = server.list_tools();
    let delegate_tool = tools
        .iter()
        .find(|t| t.name == "delegate_session")
        .expect("delegate_session tool should exist");

    let desc = delegate_tool
        .description
        .as_ref()
        .map(|d| d.as_ref())
        .unwrap_or("");
    assert!(
        !desc.contains("Available targets:"),
        "Description should not have 'Available targets:' when targets empty. Got: {}",
        desc
    );
}

fn make_server_without_delegation() -> CrucibleMcpServer {
    let temp = TempDir::new().unwrap();
    CrucibleMcpServer::new(
        temp.path().to_str().unwrap().to_string(),
        Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
        Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
    )
}

fn make_server_with_job_spawner() -> CrucibleMcpServer {
    let temp = TempDir::new().unwrap();
    CrucibleMcpServer::new_with_delegation(
        temp.path().to_str().unwrap().to_string(),
        Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
        Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
        Some(DelegationContext {
            session_id: "test-session".to_string(),
            ..Default::default()
        }),
    )
}

fn make_server_with_delegation_classification(
    data_classification: DataClassification,
) -> (CrucibleMcpServer, Arc<MockDelegationSpawner>) {
    let temp = TempDir::new().unwrap();
    let spawner = Arc::new(MockDelegationSpawner::default());
    let server = CrucibleMcpServer::new_with_delegation(
        temp.path().to_str().unwrap().to_string(),
        Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
        Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
        Some(DelegationContext {
            delegation_spawner: spawner.clone(),
            data_classification,
            ..Default::default()
        }),
    );

    (server, spawner)
}

fn make_server_with_delegation_disabled(
    data_classification: DataClassification,
) -> CrucibleMcpServer {
    let temp = TempDir::new().unwrap();
    CrucibleMcpServer::new_with_delegation(
        temp.path().to_str().unwrap().to_string(),
        Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
        Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
        Some(DelegationContext {
            enabled: false,
            data_classification,
            ..Default::default()
        }),
    )
}

#[tokio::test]
async fn test_delegation_allowed_for_internal_kiln() {
    let (server, spawner) =
        make_server_with_delegation_classification(DataClassification::Internal);

    let result = server
        .delegate_session(Parameters(DelegateSessionParams {
            prompt: "do work".to_string(),
            description: Some("desc".to_string()),
            target: None,
            background: Some(true),
        }))
        .await;

    assert!(result.is_ok());
    assert_eq!(spawner.spawn_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_delegation_trust_is_enforced_service_side_not_in_tool() {
    // Trust used to be checked here with a hardcoded Cloud assumption.
    // It now derives from the CHILD's resolved provider inside
    // DelegationService::spawn_delegation, so the tool defers: the spawn
    // call goes through and the service is the gate (covered by
    // delegation_integration tests).
    let (server, spawner) =
        make_server_with_delegation_classification(DataClassification::Confidential);

    let result = server
        .delegate_session(Parameters(DelegateSessionParams {
            prompt: "do work".to_string(),
            description: Some("desc".to_string()),
            target: None,
            background: Some(true),
        }))
        .await;

    assert!(result.is_ok());
    assert_eq!(spawner.spawn_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_delegation_allowed_for_public_kiln() {
    let (server, spawner) = make_server_with_delegation_classification(DataClassification::Public);

    let result = server
        .delegate_session(Parameters(DelegateSessionParams {
            prompt: "do work".to_string(),
            description: Some("desc".to_string()),
            target: None,
            background: Some(true),
        }))
        .await;

    assert!(result.is_ok());
    assert_eq!(spawner.spawn_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_delegation_disabled_fires_before_trust_check() {
    // enabled=false + Confidential: should get "disabled" error, not trust error
    let server = make_server_with_delegation_disabled(DataClassification::Confidential);
    let result = server
        .delegate_session(Parameters(DelegateSessionParams {
            prompt: "do work".to_string(),
            description: None,
            target: None,
            background: Some(true),
        }))
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("disabled"),
        "Expected 'disabled' error but got: {}",
        err.message
    );
    assert!(
        !err.message.contains("insufficient"),
        "Should not get trust error, got: {}",
        err.message
    );
}

#[tokio::test]
async fn test_delegation_disabled_with_public_kiln() {
    // enabled=false + Public: should still get "disabled" error
    let server = make_server_with_delegation_disabled(DataClassification::Public);
    let result = server
        .delegate_session(Parameters(DelegateSessionParams {
            prompt: "do work".to_string(),
            description: None,
            target: None,
            background: Some(true),
        }))
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("disabled"),
        "Expected 'disabled' error but got: {}",
        err.message
    );
}

#[tokio::test]
async fn test_list_jobs_without_context_returns_error() {
    let server = make_server_without_delegation();
    let result = server.list_jobs(Parameters(ListJobsParams {})).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("no daemon delegation context"));
}

#[tokio::test]
async fn test_list_jobs_returns_jobs_for_session() {
    let server = make_server_with_job_spawner();
    let result = server.list_jobs(Parameters(ListJobsParams {})).await;

    assert!(result.is_ok());
    let call_result = result.unwrap();
    assert!(!call_result.content.is_empty());
}

#[tokio::test]
async fn test_get_job_result_without_context_returns_error() {
    let server = make_server_without_delegation();
    let result = server
        .get_job_result(Parameters(GetJobResultParams {
            job_id: "job-test-123".to_string(),
        }))
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("no daemon delegation context"));
}

#[tokio::test]
async fn test_get_job_result_returns_result_for_known_job() {
    let server = make_server_with_job_spawner();
    let result = server
        .get_job_result(Parameters(GetJobResultParams {
            job_id: "job-test-123".to_string(),
        }))
        .await;

    assert!(result.is_ok());
    let call_result = result.unwrap();
    assert!(!call_result.content.is_empty());
}

#[tokio::test]
async fn test_get_job_result_unknown_job_returns_error() {
    let server = make_server_with_job_spawner();
    let result = server
        .get_job_result(Parameters(GetJobResultParams {
            job_id: "nonexistent-job".to_string(),
        }))
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("Job not found"));
}

#[tokio::test]
async fn test_cancel_job_without_context_returns_error() {
    let server = make_server_without_delegation();
    let result = server
        .cancel_job(Parameters(CancelJobParams {
            job_id: "job-test-123".to_string(),
        }))
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("no daemon delegation context"));
}

#[tokio::test]
async fn test_cancel_job_returns_cancelled_status() {
    let server = make_server_with_job_spawner();
    let result = server
        .cancel_job(Parameters(CancelJobParams {
            job_id: "job-test-123".to_string(),
        }))
        .await;

    assert!(result.is_ok());
    let call_result = result.unwrap();
    assert!(!call_result.content.is_empty());
}

#[test]
fn test_job_tools_appear_in_tool_router() {
    let server = make_server_with_job_spawner();
    let tools = server.list_tools();
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    assert!(
        tool_names.contains(&"list_jobs"),
        "list_jobs should be in tool list: {:?}",
        tool_names
    );
    assert!(
        tool_names.contains(&"get_job_result"),
        "get_job_result should be in tool list: {:?}",
        tool_names
    );
    assert!(
        tool_names.contains(&"cancel_job"),
        "cancel_job should be in tool list: {:?}",
        tool_names
    );
}
