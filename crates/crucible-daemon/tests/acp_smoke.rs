//! ACP E2E smoke tests using mock-acp-agent binary.
//!
//! These tests spawn the mock-acp-agent process to verify the full ACP lifecycle
//! (spawn → handshake → message → delegation → recording) without requiring
//! real LLM API keys.
//!
//! # Prerequisites
//! Build the mock agent first:
//! ```
//! cargo build -p crucible-daemon --features test-utils --bin mock-acp-agent
//! ```

use crucible_core::background::JobStatus;
use crucible_core::config::{AcpConfig, AgentProfile, BackendType, DelegationConfig};
use crucible_core::session::RecordingMode;
use crucible_core::session::{OutputValidation, SessionAgent, SessionType};
use crucible_core::traits::chat::AgentHandle;
use crucible_core::turn::{Agent, TurnContext, TurnEvent};
use crucible_daemon::acp_handle::{AcpAgentHandle, AcpAgentHandleParams};
use crucible_daemon::agent_manager::AgentFactoryOverride;
use crucible_daemon::background_manager::BackgroundJobManager;
use crucible_daemon::delegation::{DelegationRequest, DelegationService, DelegationSpawner};
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::recording::RecordingWriter;
use crucible_daemon::tools::workspace::WorkspaceTools;
use crucible_daemon::{
    AgentManager, AgentManagerParams, FileSessionStorage, KilnManager, SessionManager,
};
use futures::StreamExt;
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::{broadcast, oneshot};
use tokio::time::{timeout, Duration};

/// Returns the path to the mock-acp-agent binary.
///
/// Prefers `CARGO_BIN_EXE_mock-acp-agent` (set by cargo when the bin target is
/// built, i.e. when the test-utils feature is enabled — honors any custom
/// target-dir). Falls back to the default workspace-root target path.
pub fn mock_agent_path() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_mock-acp-agent") {
        return PathBuf::from(path);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug/mock-acp-agent")
        .canonicalize()
        .expect(
            "mock-acp-agent binary not found. Build it with:\n\
             cargo build -p crucible-daemon --features test-utils --bin mock-acp-agent",
        )
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
        mode: None,
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
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: OutputValidation::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        tool_policy: None,
    }
}

fn delegation_enabled_agent(agent_path: &str) -> SessionAgent {
    let mut agent = mock_session_agent(agent_path);
    // The scheduler rejects empty no-tool turns; have the mock binary stream
    // a deterministic chunk (see CRU_MOCK_STREAM_CHUNKS hook).
    agent.env_overrides.insert(
        "CRU_MOCK_STREAM_CHUNKS".to_string(),
        "mock delegation output".to_string(),
    );
    agent.delegation_config = Some(DelegationConfig {
        enabled: true,
        max_depth: 2,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
        timeout_secs: 300,
    });
    agent
}

/// Agent-factory override that builds the child session's agent as an
/// `AcpAgentHandle` connected to the mock-acp-agent binary. This is the
/// successor to the pre-refactor `SubagentFactory`: the delegation scheduler
/// calls it to construct the CHILD session's agent.
fn make_acp_agent_factory() -> AgentFactoryOverride {
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
                delegation_spawner: None,
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

/// Build the full delegation stack (session manager + agent manager +
/// delegation service) with a scripted child-agent factory installed. The
/// returned `AgentManager` must be kept alive for the duration of the test:
/// the `DelegationService` holds only a `Weak` back-reference to it.
fn build_delegation_stack(
    event_tx: broadcast::Sender<SessionEventMessage>,
    factory: AgentFactoryOverride,
) -> (
    Arc<AgentManager>,
    Arc<SessionManager>,
    Arc<DelegationService>,
) {
    let session_manager = Arc::new(SessionManager::with_storage(Arc::new(
        FileSessionStorage::new(),
    )));
    let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
    let service = DelegationService::new(session_manager.clone(), event_tx.clone());
    let manager = Arc::new(AgentManager::new_with_delegation(
        AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager: session_manager.clone(),
            background_manager,
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            permission_config: None,
            plugin_loader: None,
            workspace_tools: Arc::new(WorkspaceTools::new(PathBuf::from("/tmp"))),
        },
        service.clone(),
    ));
    service.bind_agent_manager(&manager);
    manager.set_agent_factory_override(factory);
    (manager, session_manager, service)
}

/// Create a top-level parent session with `agent` configured as its
/// delegation-capable agent, returning the parent session id.
async fn create_delegation_parent(
    manager: &AgentManager,
    session_manager: &SessionManager,
    workspace: &Path,
    agent: SessionAgent,
) -> String {
    let session = session_manager
        .create_session(
            SessionType::Chat,
            workspace.to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .expect("parent session should be created");
    manager
        .configure_agent(&session.id, agent)
        .await
        .expect("parent agent should be configured");
    session.id
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
            delegation_spawner: None,
            parent_session_id: None,
            delegation_config: None,
            acp_config: None,
            permission_handler: None,
        }),
    )
    .await
    .expect("ACP handshake timed out")
    .expect("ACP handshake failed");

    // If the handshake above returned Ok(handle), the ACP client is connected
    // by construction; no further assertion needed.
    let _ = handle;
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
            delegation_spawner: None,
            parent_session_id: None,
            delegation_config: None,
            acp_config: None,
            permission_handler: None,
        }),
    )
    .await
    .expect("ACP handshake timed out")
    .expect("ACP handshake failed");

    let events = timeout(Duration::from_secs(30), async {
        let stream = handle
            .turn(TurnContext::new("hello from smoke test"))
            .await
            .expect("Agent::turn failed");
        stream.collect::<Vec<_>>().await
    })
    .await
    .expect("Streaming response timed out");

    assert!(!events.is_empty(), "Expected at least one TurnEvent");
    assert!(
        events.iter().any(|e| matches!(e, TurnEvent::Done { .. })),
        "Expected at least one Done event"
    );
}

/// Regression: daemon-injected context (Precognition, Lua `transform_context`)
/// is forwarded into the ACP prompt. The ACP agent owns its history, so
/// `turn()` sends only the new user content — but System-role blocks in
/// `ctx.messages` represent knowledge the external agent has no other way to
/// see and must be forwarded. The mock captures the exact prompt text it
/// received over the wire (gated on `CRU_MOCK_PROMPT_CAPTURE`).
#[tokio::test]
async fn injected_system_context_reaches_acp_prompt() {
    use crucible_core::traits::ContextMessage;

    let workspace = TempDir::new().expect("Failed to create temp workspace");
    let capture_path = workspace.path().join("captured_prompt.txt");
    let agent_path = mock_agent_path().to_string_lossy().into_owned();

    let mut agent_config = mock_session_agent(&agent_path);
    agent_config.env_overrides.insert(
        "CRU_MOCK_PROMPT_CAPTURE".to_string(),
        capture_path.to_string_lossy().into_owned(),
    );

    let mut handle = timeout(
        Duration::from_secs(30),
        AcpAgentHandle::new(AcpAgentHandleParams {
            agent_config: &agent_config,
            workspace: workspace.path(),
            kiln_path: None,
            knowledge_repo: None,
            embedding_provider: None,
            background_spawner: None,
            delegation_spawner: None,
            parent_session_id: None,
            delegation_config: None,
            acp_config: None,
            permission_handler: None,
        }),
    )
    .await
    .expect("ACP handshake timed out")
    .expect("ACP handshake failed");

    // Mirror what the daemon stages on the turn: a System-role Precognition
    // block prepended ahead of the user's message in `ctx.messages`.
    let ctx = TurnContext::new("What is the capital of Testlandia?").with_messages(vec![
        ContextMessage::system("KNOWLEDGE: The capital of Testlandia is Fooville."),
        ContextMessage::user("What is the capital of Testlandia?"),
    ]);

    let _events = timeout(Duration::from_secs(30), async {
        let stream = handle.turn(ctx).await.expect("Agent::turn failed");
        stream.collect::<Vec<_>>().await
    })
    .await
    .expect("Streaming response timed out");

    let captured = std::fs::read_to_string(&capture_path)
        .expect("mock should have captured the prompt it received");

    assert!(
        captured.contains("KNOWLEDGE: The capital of Testlandia is Fooville."),
        "daemon-injected System context must reach the ACP prompt; captured: {captured:?}"
    );
    assert!(
        captured.contains("What is the capital of Testlandia?"),
        "user content must still reach the ACP prompt; captured: {captured:?}"
    );
}

/// ACP model switching round-trips against the live agent: the handle
/// captures the agent-advertised model list at connect, reports
/// `model_switching` capability, exposes the current model, and `switch_model`
/// sends `session/set_model` over the wire (captured by the mock) without
/// restarting the agent process (history-preserving).
#[tokio::test]
async fn acp_model_switching_round_trips() {
    use crucible_core::turn::Agent;

    let workspace = TempDir::new().expect("temp workspace");
    let model_capture = workspace.path().join("set_model.txt");
    let agent_path = mock_agent_path().to_string_lossy().into_owned();

    let mut agent_config = mock_session_agent(&agent_path);
    agent_config
        .env_overrides
        .insert("CRU_MOCK_ADVERTISE_MODELS".to_string(), "1".to_string());
    agent_config.env_overrides.insert(
        "CRU_MOCK_MODEL_CAPTURE".to_string(),
        model_capture.to_string_lossy().into_owned(),
    );

    let mut handle = timeout(
        Duration::from_secs(30),
        AcpAgentHandle::new(AcpAgentHandleParams {
            agent_config: &agent_config,
            workspace: workspace.path(),
            kiln_path: None,
            knowledge_repo: None,
            embedding_provider: None,
            background_spawner: None,
            delegation_spawner: None,
            parent_session_id: None,
            delegation_config: None,
            acp_config: None,
            permission_handler: None,
        }),
    )
    .await
    .expect("ACP handshake timed out")
    .expect("ACP handshake failed");

    // Capability reflects that the agent advertised models.
    assert!(
        Agent::capabilities(&handle).model_switching,
        "model_switching capability should be true when the agent advertises models"
    );

    // Current model = the advertised current.
    assert_eq!(
        AgentHandle::current_model(&handle),
        Some("mock-sonnet"),
        "handle should expose the agent's current model"
    );

    // Available models come from the agent's advertised list.
    let models = handle.fetch_available_models().await;
    assert!(
        models.iter().any(|m| m == "mock-opus"),
        "available models should include the advertised list, got: {models:?}"
    );

    // Switch — sends session/set_model to the live process.
    AgentHandle::switch_model(&mut handle, "mock-opus")
        .await
        .expect("switch_model should succeed for an ACP agent that advertises models");

    assert_eq!(
        AgentHandle::current_model(&handle),
        Some("mock-opus"),
        "current model should update after switch"
    );

    let captured = std::fs::read_to_string(&model_capture)
        .expect("mock should have captured the set_model request");
    assert_eq!(
        captured.trim(),
        "mock-opus",
        "the switched model id must reach the agent over the wire"
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
            delegation_spawner: None,
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
    let profile = AgentProfile {
        args: Some(vec!["--inject-errors".to_string()]),
        ..Default::default()
    };
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
            delegation_spawner: None,
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

    // The factory must never run: the depth check rejects before any child
    // session is created.
    let factory: AgentFactoryOverride = Box::new(|_agent: &SessionAgent, _workspace: &Path| {
        Box::pin(async { Err("factory should not be called".to_string()) })
            as Pin<
                Box<dyn Future<Output = Result<Box<dyn AgentHandle + Send + Sync>, String>> + Send>,
            >
    });
    let (manager, session_manager, service) = build_delegation_stack(event_tx, factory);

    // max_depth = 0 means any child (which sits at depth 1) exceeds the limit.
    let mut agent = mock_session_agent("test-agent");
    agent.delegation_config = Some(DelegationConfig {
        enabled: true,
        max_depth: 0,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
        timeout_secs: 300,
    });

    let parent_id = create_delegation_parent(&manager, &session_manager, temp.path(), agent).await;

    let err = service
        .spawn_delegation(DelegationRequest {
            parent_session_id: parent_id,
            prompt: "delegate deeper".to_string(),
            context: None,
            target_agent: None,
            description: Some("depth test".to_string()),
        })
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
    let agent_path = mock_agent_path().to_string_lossy().into_owned();

    let (event_tx, mut rx) = broadcast::channel(256);
    let (manager, session_manager, service) =
        build_delegation_stack(event_tx, make_acp_agent_factory());

    // Parent is the ACP mock agent with delegation enabled; the delegated
    // child (target None) inherits that config, so it too runs the mock
    // binary through the scheduler.
    let parent_id = create_delegation_parent(
        &manager,
        &session_manager,
        temp.path(),
        delegation_enabled_agent(&agent_path),
    )
    .await;

    let spawned = timeout(
        Duration::from_secs(30),
        service.spawn_delegation(DelegationRequest {
            parent_session_id: parent_id.clone(),
            prompt: "Delegate this task".to_string(),
            context: None,
            target_agent: None,
            description: Some("acp smoke test".to_string()),
        }),
    )
    .await
    .expect("timed out while spawning delegation")
    .expect("delegation spawn should succeed");
    let delegation_id = spawned.delegation_id.clone();

    let spawned_event = next_event(&mut rx, "delegation_spawned").await;
    let completed_event = next_event(&mut rx, "delegation_completed").await;

    assert_eq!(
        spawned_event.data["delegation_id"].as_str(),
        Some(delegation_id.as_str())
    );
    assert_eq!(spawned_event.session_id, parent_id);
    assert_eq!(
        spawned_event.data["parent_session_id"].as_str(),
        Some(parent_id.as_str())
    );

    assert_eq!(completed_event.session_id, parent_id);
    assert_eq!(
        completed_event.data["delegation_id"].as_str(),
        Some(delegation_id.as_str())
    );
    assert_eq!(
        completed_event.data["parent_session_id"].as_str(),
        Some(parent_id.as_str())
    );

    let result = service
        .await_delegation(&delegation_id, Duration::from_secs(30))
        .await
        .expect("await_delegation should return a terminal result");
    assert_eq!(result.info.status, JobStatus::Completed);
}

#[tokio::test]
async fn mock_acp_delegation_captured_in_recording() {
    let temp = TempDir::new().expect("temp dir");
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let recording_path = temp.path().join("recording.jsonl");

    let (event_tx, mut assertion_rx) = broadcast::channel(256);
    let (manager, session_manager, service) =
        build_delegation_stack(event_tx.clone(), make_acp_agent_factory());

    let parent_id = create_delegation_parent(
        &manager,
        &session_manager,
        temp.path(),
        delegation_enabled_agent(&agent_path),
    )
    .await;

    // The recording is filtered by session id, so it must match the parent
    // session the delegation events are emitted on.
    let (writer, recording_tx) = RecordingWriter::new(
        recording_path.clone(),
        parent_id.clone(),
        RecordingMode::Granular,
        None,
    );
    let writer_handle = writer.start();

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

    let spawned = timeout(
        Duration::from_secs(30),
        service.spawn_delegation(DelegationRequest {
            parent_session_id: parent_id.clone(),
            prompt: "Delegate and record this task".to_string(),
            context: None,
            target_agent: None,
            description: Some("acp recording smoke test".to_string()),
        }),
    )
    .await
    .expect("timed out while spawning delegation")
    .expect("delegation spawn should succeed");
    let delegation_id = spawned.delegation_id.clone();

    let spawned_event = next_event(&mut assertion_rx, "delegation_spawned").await;
    let completed_event = next_event(&mut assertion_rx, "delegation_completed").await;

    assert_eq!(
        spawned_event.data["delegation_id"].as_str(),
        Some(delegation_id.as_str())
    );
    assert_eq!(
        completed_event.data["delegation_id"].as_str(),
        Some(delegation_id.as_str())
    );

    let result = service
        .await_delegation(&delegation_id, Duration::from_secs(30))
        .await
        .expect("await_delegation should return a terminal result");
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
