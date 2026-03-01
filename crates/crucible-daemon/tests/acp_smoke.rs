//! ACP E2E smoke tests using mock-acp-agent binary.
//!
//! These tests spawn the mock-acp-agent process to verify the full ACP lifecycle
//! (spawn → handshake → message → delegation → recording) without requiring
//! real LLM API keys.
//!
//! # Prerequisites
//! Build the mock agent first:
//! ```
//! cargo build -p crucible-acp --features test-utils --bin mock-acp-agent
//! ```

use crucible_config::{AcpConfig, AgentProfile, BackendType, DelegationConfig};
use crucible_core::background::{JobResult, JobStatus};
use crucible_core::session::RecordingMode;
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::AgentHandle;
use crucible_daemon::acp_handle::{AcpAgentHandle, AcpAgentHandleParams};
use crucible_daemon::background_manager::{BackgroundJobManager, SubagentContext, SubagentFactory};
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::recording::RecordingWriter;
use futures::StreamExt;
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::{broadcast, oneshot};
use tokio::time::{sleep, Instant};
use tokio::time::{timeout, Duration};

/// Returns the path to the mock-acp-agent binary.
///
/// The binary is built at `target/debug/mock-acp-agent` relative to the workspace root.
/// This function resolves the path from the daemon crate's manifest directory.
pub fn mock_agent_path() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug/mock-acp-agent")
        .canonicalize()
        .expect(
            "mock-acp-agent binary not found. Build it with:\n\
             cargo build -p crucible-acp --features test-utils --bin mock-acp-agent",
        );
    path
}

/// Creates a SessionAgent configured for ACP with the given agent path.
///
/// This helper constructs a minimal SessionAgent with:
/// - agent_type: "acp"
/// - agent_name: the provided agent_path
/// - provider: Mock (for testing)
/// - All other fields set to sensible defaults
pub fn mock_session_agent(agent_path: &str) -> SessionAgent {
    SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some(agent_path.to_string()),
        provider_key: None,
        provider: BackendType::Mock,
        model: "mock-model".to_string(),
        system_prompt: "You are a helpful assistant.".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
    }
}

fn delegation_enabled_agent(agent_path: &str) -> SessionAgent {
    let mut agent = mock_session_agent(agent_path);
    agent.delegation_config = Some(DelegationConfig {
        enabled: true,
        max_depth: 2,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    });
    agent
}

fn make_acp_subagent_factory() -> SubagentFactory {
    Box::new(move |agent_config: &SessionAgent, workspace: &Path| {
        let agent_config = agent_config.clone();
        let workspace = workspace.to_path_buf();
        Box::pin(async move {
            AcpAgentHandle::new(AcpAgentHandleParams {
                agent_config: &agent_config,
                workspace: &workspace,
                kiln_path: None,
                knowledge_repo: None,
                embedding_provider: None,
                background_spawner: None,
                parent_session_id: None,
                delegation_config: None,
                acp_config: None,
                permission_handler: None,
            })
            .await
            .map(|handle| Box::new(handle) as Box<dyn AgentHandle + Send + Sync>)
            .map_err(|e| e.to_string())
        })
            as Pin<
                Box<dyn Future<Output = Result<Box<dyn AgentHandle + Send + Sync>, String>> + Send>,
            >
    })
}

fn register_delegation_context(
    manager: &BackgroundJobManager,
    session_id: &str,
    workspace: &Path,
    agent_path: &str,
) {
    manager.register_subagent_context(
        session_id,
        SubagentContext {
            agent: delegation_enabled_agent(agent_path),
            available_agents: HashMap::new(),
            workspace: workspace.to_path_buf(),
            parent_session_id: Some(session_id.to_string()),
            parent_session_dir: Some(workspace.to_path_buf()),
            delegator_agent_name: Some("parent-agent".to_string()),
            target_agent_name: Some("worker-agent".to_string()),
            delegation_depth: 0,
        },
    );
}

async fn next_event(
    rx: &mut broadcast::Receiver<SessionEventMessage>,
    event_name: &str,
) -> SessionEventMessage {
    timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(event) = rx.recv().await {
                if event.event == event_name {
                    return event;
                }
            }
        }
    })
    .await
    .expect("timed out waiting for event")
}

async fn wait_for_terminal_result(manager: &BackgroundJobManager, job_id: &str) -> JobResult {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(result) = manager.get_job_result(&job_id.to_string()) {
            if result.info.status.is_terminal() {
                return result;
            }
        }

        assert!(
            Instant::now() < deadline,
            "timed out waiting for terminal result"
        );
        sleep(Duration::from_millis(10)).await;
    }
}

#[test]
fn mock_binary_exists_and_runs() {
    let path = mock_agent_path();
    assert!(path.exists(), "mock-acp-agent not found at {:?}", path);

    let output = std::process::Command::new(&path)
        .arg("--help")
        .output()
        .expect("Failed to execute mock-acp-agent");

    assert!(
        output.status.success(),
        "mock-acp-agent --help failed with status: {:?}",
        output.status
    );
}

#[tokio::test]
async fn mock_acp_handshake_succeeds() {
    let workspace = TempDir::new().expect("Failed to create temp workspace");
    let agent_path = mock_agent_path();
    let agent_path = agent_path.to_string_lossy().into_owned();
    let agent_config = mock_session_agent(&agent_path);

    let handle = timeout(
        Duration::from_secs(30),
        AcpAgentHandle::new(AcpAgentHandleParams {
            agent_config: &agent_config,
            workspace: workspace.path(),
            kiln_path: None,
            knowledge_repo: None,
            embedding_provider: None,
            background_spawner: None,
            parent_session_id: None,
            delegation_config: None,
            acp_config: None,
            permission_handler: None,
        }),
    )
    .await
    .expect("ACP handshake timed out")
    .expect("ACP handshake failed");

    assert!(handle.is_connected(), "ACP handle should be connected");
}

#[tokio::test]
async fn mock_acp_agent_returns_message_response() {
    let workspace = TempDir::new().expect("Failed to create temp workspace");
    let agent_path = mock_agent_path();
    let agent_path = agent_path.to_string_lossy().into_owned();
    let agent_config = mock_session_agent(&agent_path);

    let mut handle = timeout(
        Duration::from_secs(30),
        AcpAgentHandle::new(AcpAgentHandleParams {
            agent_config: &agent_config,
            workspace: workspace.path(),
            kiln_path: None,
            knowledge_repo: None,
            embedding_provider: None,
            background_spawner: None,
            parent_session_id: None,
            delegation_config: None,
            acp_config: None,
            permission_handler: None,
        }),
    )
    .await
    .expect("ACP handshake timed out")
    .expect("ACP handshake failed");

    let chunks = timeout(Duration::from_secs(30), async {
        handle
            .send_message_stream("hello from smoke test".to_string())
            .collect::<Vec<_>>()
            .await
    })
    .await
    .expect("Streaming response timed out");

    assert!(!chunks.is_empty(), "Expected at least one response chunk");
    assert!(
        chunks
            .iter()
            .any(|chunk| matches!(chunk, Ok(chunk) if chunk.done)),
        "Expected at least one done=true ChatChunk"
    );
}

#[tokio::test]
async fn missing_binary_returns_connection_error() {
    let workspace = TempDir::new().expect("Failed to create temp workspace");
    let agent_config = mock_session_agent("/nonexistent/path/to/binary");

    let result = timeout(
        Duration::from_secs(10),
        AcpAgentHandle::new(AcpAgentHandleParams {
            agent_config: &agent_config,
            workspace: workspace.path(),
            kiln_path: None,
            knowledge_repo: None,
            embedding_provider: None,
            background_spawner: None,
            parent_session_id: None,
            delegation_config: None,
            acp_config: None,
            permission_handler: None,
        }),
    )
    .await
    .expect("missing binary should fail quickly");

    assert!(
        result.is_err(),
        "AcpAgentHandle::new should return Err for missing binary"
    );
}

#[tokio::test]
async fn inject_errors_causes_handshake_failure() {
    let workspace = TempDir::new().expect("Failed to create temp workspace");
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let agent_config = mock_session_agent(&agent_path);

    let mut acp_config = AcpConfig::default();
    let mut profile = AgentProfile::default();
    profile.args = Some(vec!["--inject-errors".to_string()]);
    acp_config.agents.insert(agent_path, profile);

    let result = timeout(
        Duration::from_secs(30),
        AcpAgentHandle::new(AcpAgentHandleParams {
            agent_config: &agent_config,
            workspace: workspace.path(),
            kiln_path: None,
            knowledge_repo: None,
            embedding_provider: None,
            background_spawner: None,
            parent_session_id: None,
            delegation_config: None,
            acp_config: Some(&acp_config),
            permission_handler: None,
        }),
    )
    .await
    .expect("ACP handshake with injected errors timed out unexpectedly");

    assert!(
        result.is_err(),
        "AcpAgentHandle::new should fail when mock agent injects protocol errors"
    );
}

#[tokio::test]
async fn delegation_depth_limit_enforced() {
    let temp = TempDir::new().expect("temp dir");
    let (event_tx, _) = broadcast::channel(32);
    let manager = Arc::new(
        BackgroundJobManager::new(event_tx).with_subagent_factory(Box::new(
            |_agent, _workspace| {
                Box::pin(async { Err("factory should not be called".to_string()) })
            },
        )),
    );

    let session_id = "session-depth-limit-smoke";
    let mut agent = mock_session_agent("test-agent");
    agent.delegation_config = Some(DelegationConfig {
        enabled: true,
        max_depth: 2,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    });

    manager.register_subagent_context(
        session_id,
        SubagentContext {
            agent,
            available_agents: HashMap::new(),
            workspace: temp.path().to_path_buf(),
            parent_session_id: Some(session_id.to_string()),
            parent_session_dir: Some(temp.path().to_path_buf()),
            delegator_agent_name: Some("parent-agent".to_string()),
            target_agent_name: Some("worker-agent".to_string()),
            delegation_depth: 3,
        },
    );

    let err = manager
        .spawn_subagent(
            session_id,
            "delegate deeper".to_string(),
            Some("depth test".to_string()),
        )
        .await
        .expect_err("depth limit should reject delegation");

    let msg = err.to_string();
    assert!(
        msg.contains("Delegation depth limit exceeded"),
        "error should mention depth limit, got: {msg}"
    );
}

#[tokio::test]
async fn mock_acp_delegation_emits_events() {
    let temp = TempDir::new().expect("temp dir");
    let agent_path = mock_agent_path();
    let agent_path = agent_path.to_string_lossy().into_owned();

    let (event_tx, mut rx) = broadcast::channel(32);
    let manager = Arc::new(
        BackgroundJobManager::new(event_tx).with_subagent_factory(make_acp_subagent_factory()),
    );

    let session_id = "session-acp-delegation-events";
    register_delegation_context(manager.as_ref(), session_id, temp.path(), &agent_path);

    let delegation_id = timeout(
        Duration::from_secs(30),
        manager.spawn_subagent(
            session_id,
            "Delegate this task".to_string(),
            Some("acp smoke test".to_string()),
        ),
    )
    .await
    .expect("timed out while spawning subagent")
    .expect("delegation spawn should succeed");

    let spawned = next_event(&mut rx, "delegation_spawned").await;
    let completed = next_event(&mut rx, "delegation_completed").await;

    assert_eq!(
        spawned.data["delegation_id"].as_str(),
        Some(delegation_id.as_str())
    );
    assert_eq!(spawned.session_id, session_id);
    assert_eq!(spawned.data["parent_session_id"].as_str(), Some(session_id));

    assert_eq!(completed.session_id, session_id);
    assert_eq!(
        completed.data["delegation_id"].as_str(),
        Some(delegation_id.as_str())
    );
    assert_eq!(
        completed.data["parent_session_id"].as_str(),
        Some(session_id)
    );

    let result = wait_for_terminal_result(manager.as_ref(), &delegation_id).await;
    assert_eq!(result.info.status, JobStatus::Completed);
}

#[tokio::test]
async fn mock_acp_delegation_captured_in_recording() {
    let temp = TempDir::new().expect("temp dir");
    let agent_path = mock_agent_path();
    let agent_path = agent_path.to_string_lossy().into_owned();
    let session_id = "session-acp-delegation-recording";
    let recording_path = temp.path().join("recording.jsonl");

    let (writer, recording_tx) = RecordingWriter::new(
        recording_path.clone(),
        session_id.to_string(),
        RecordingMode::Granular,
        None,
    );
    let writer_handle = writer.start();

    let (event_tx, mut assertion_rx) = broadcast::channel(64);
    let manager = Arc::new(
        BackgroundJobManager::new(event_tx.clone())
            .with_subagent_factory(make_acp_subagent_factory()),
    );

    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
    let mut bridge_rx = event_tx.subscribe();
    let bridge_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut stop_rx => break,
                maybe_event = bridge_rx.recv() => {
                    match maybe_event {
                        Ok(event) => {
                            if recording_tx.send(event).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    });

    register_delegation_context(manager.as_ref(), session_id, temp.path(), &agent_path);

    let delegation_id = timeout(
        Duration::from_secs(30),
        manager.spawn_subagent(
            session_id,
            "Delegate and record this task".to_string(),
            Some("acp recording smoke test".to_string()),
        ),
    )
    .await
    .expect("timed out while spawning subagent")
    .expect("delegation spawn should succeed");

    let spawned = next_event(&mut assertion_rx, "delegation_spawned").await;
    let completed = next_event(&mut assertion_rx, "delegation_completed").await;

    assert_eq!(
        spawned.data["delegation_id"].as_str(),
        Some(delegation_id.as_str())
    );
    assert_eq!(
        completed.data["delegation_id"].as_str(),
        Some(delegation_id.as_str())
    );

    let result = wait_for_terminal_result(manager.as_ref(), &delegation_id).await;
    assert_eq!(result.info.status, JobStatus::Completed);

    let _ = stop_tx.send(());
    bridge_handle.await.expect("bridge should join");
    writer_handle
        .await
        .expect("writer task should join")
        .expect("writer should flush recording");

    let recording = tokio::fs::read_to_string(&recording_path)
        .await
        .expect("recording file should be readable");
    assert!(
        recording.contains("delegation_spawned"),
        "recording should contain delegation_spawned"
    );
    assert!(
        recording.contains("delegation_completed"),
        "recording should contain delegation_completed"
    );
}
