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
use crucible_daemon::test_support::{kiln_name, temp_session_manager};

use crucible_core::background::JobStatus;
use crucible_core::config::{AcpConfig, AgentProfile, DelegationConfig};
use crucible_core::session::RecordingMode;
use crucible_core::session::{SessionAgent, SessionType};
use crucible_core::traits::chat::{AgentHandle, ChatError, SessionKnobs};
use crucible_core::turn::{Agent, StopReason, TurnContext, TurnError, TurnEvent};
use crucible_daemon::acp_handle::{AcpAgentHandle, AcpAgentHandleParams, AcpHandleError};
use crucible_daemon::agent_manager::AgentFactoryOverride;
use crucible_daemon::background_manager::BackgroundJobManager;
use crucible_daemon::delegation::{DelegationRequest, DelegationService, DelegationSpawner};
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::recording::RecordingWriter;
use crucible_daemon::{AgentManager, AgentManagerParams, KilnManager, SessionManager};
use futures::StreamExt;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;
use tokio::sync::{broadcast, oneshot};
use tokio::time::{timeout, Duration};

// Shared with the `acp_integration` binary, which drives the same spawned
// mock agent through `AcpAgentHandle`.
#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent_bin::{mock_agent_path, mock_handle_params, mock_session_agent};

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
            AcpAgentHandle::new(mock_handle_params(&agent_config, &workspace))
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
    let session_manager = temp_session_manager();
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
            context_config: None,
            permission_config: None,
            plugin_loader: None,
            card_roots: Default::default(),
            review_snapshot_root: crucible_daemon::test_support::scratch_snapshot_root(),
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
    _workspace: &Path,
    agent: SessionAgent,
) -> String {
    let session = session_manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .expect("parent session should be created");
    manager
        .configure_agent(&session.id, agent)
        .await
        .expect("parent agent should be configured");
    session.id.to_string()
}

async fn next_event(
    rx: &mut broadcast::Receiver<SessionEventMessage>,
    event_name: &str,
) -> SessionEventMessage {
    timeout(Duration::from_secs(30), async {
        loop {
            // `Err(Closed)` must not be discarded. A closed broadcast returns it
            // IMMEDIATELY and forever, so `if let Ok(..)` swallowing it leaves a
            // loop with nothing that can park: it pegs a core and, on the
            // current-thread runtime `#[tokio::test]` builds, starves the timer
            // driver that the `timeout` wrapping this relies on. The intended
            // "timed out waiting for X" panic then never arrives and the test
            // hangs until the harness kills it.
            match rx.recv().await {
                Ok(event) if event.event == event_name => return event,
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    // Self-correcting: the cursor is repositioned and the next
                    // recv parks or delivers. Worth saying, since a dropped event
                    // could be the one being waited for.
                    eprintln!(
                        "event stream lagged, dropped {n} events while waiting for {event_name}"
                    );
                }
                Err(broadcast::error::RecvError::Closed) => {
                    panic!("event stream closed while waiting for {event_name}")
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
        AcpAgentHandle::new(mock_handle_params(&agent_config, workspace.path())),
    )
    .await
    .expect("ACP handshake timed out")
    .expect("ACP handshake failed");

    // The session id is the one the agent minted in its `session/new` reply,
    // so it proves the handshake reached the agent and came back.
    let session_id = handle
        .acp_session_id()
        .expect("a connected handle has an agent session");
    assert!(
        session_id.starts_with("mock-session-"),
        "the handle must adopt the id the agent issued, got {session_id:?}"
    );
}

#[tokio::test]
async fn mock_acp_agent_returns_message_response() {
    const ANSWER: &str = "hello from the mock agent";
    let workspace = TempDir::new().expect("Failed to create temp workspace");
    let agent_path = mock_agent_path();
    let agent_path = agent_path.to_string_lossy().into_owned();
    let mut agent_config = mock_session_agent(&agent_path);
    agent_config
        .env_overrides
        .insert("CRU_MOCK_STREAM_CHUNKS".to_string(), ANSWER.to_string());

    let mut handle = timeout(
        Duration::from_secs(30),
        AcpAgentHandle::new(mock_handle_params(&agent_config, workspace.path())),
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

    let text: String = events
        .iter()
        .filter_map(|event| match event {
            TurnEvent::TextDelta(delta) => Some(delta.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, ANSWER, "the streamed text; events: {events:?}");
    assert!(
        matches!(
            events.last(),
            Some(TurnEvent::Done {
                stop_reason: StopReason::EndTurn
            })
        ),
        "the turn must end with Done(EndTurn); events: {events:?}"
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
        AcpAgentHandle::new(mock_handle_params(&agent_config, workspace.path())),
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

/// Model switching on an ACP agent goes through Session Config Options.
/// The handle reads the `select` option with category `model` from the
/// `session/new` reply, reports `model_switching`, exposes the current
/// model, and `switch_model` sends `session/set_config_option` over the
/// wire (captured by the mock) without a restart of the agent process.
#[tokio::test]
async fn acp_model_switching_sends_set_config_option() {
    let workspace = TempDir::new().expect("temp workspace");
    let model_capture = workspace.path().join("set_config_option.txt");
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
        AcpAgentHandle::new(mock_handle_params(&agent_config, workspace.path())),
    )
    .await
    .expect("ACP handshake timed out")
    .expect("ACP handshake failed");

    assert!(
        Agent::capabilities(&handle).model_switching,
        "model_switching must be on when the agent advertises a model selector"
    );
    assert_eq!(SessionKnobs::current_model(&handle), Some("mock-sonnet"));
    assert_eq!(
        handle.fetch_available_models().await,
        vec!["mock-sonnet".to_string(), "mock-opus".to_string()]
    );

    let err = SessionKnobs::switch_model(&mut handle, "mock-haiku")
        .await
        .expect_err("an id the agent did not list must fail before the wire");
    assert!(
        matches!(err, ChatError::ModeChange(_)),
        "expected ModeChange, got {err:?}"
    );
    assert!(
        !model_capture.exists(),
        "a refused id must not reach the agent"
    );

    SessionKnobs::switch_model(&mut handle, "mock-opus")
        .await
        .expect("switch_model succeeds for a listed id");
    assert_eq!(SessionKnobs::current_model(&handle), Some("mock-opus"));

    let captured = std::fs::read_to_string(&model_capture)
        .expect("mock should have captured the set_config_option request");
    assert_eq!(
        captured.trim(),
        "model=mock-opus",
        "the selector id and the model id must reach the agent over the wire"
    );
}

/// An agent whose `session/new` reply carries no `configOptions` has no
/// model selector: no capability, no current model, no list, and
/// `switch_model` fails with `NotSupported` before the wire.
#[tokio::test]
async fn acp_session_new_without_config_options_reports_no_models() {
    let workspace = TempDir::new().expect("temp workspace");
    let model_capture = workspace.path().join("set_config_option.txt");
    let agent_path = mock_agent_path().to_string_lossy().into_owned();

    let mut agent_config = mock_session_agent(&agent_path);
    agent_config.env_overrides.insert(
        "CRU_MOCK_MODEL_CAPTURE".to_string(),
        model_capture.to_string_lossy().into_owned(),
    );

    let mut handle = timeout(
        Duration::from_secs(30),
        AcpAgentHandle::new(mock_handle_params(&agent_config, workspace.path())),
    )
    .await
    .expect("ACP handshake timed out")
    .expect("ACP handshake failed");

    assert!(!Agent::capabilities(&handle).model_switching);
    assert_eq!(SessionKnobs::current_model(&handle), None);
    assert!(handle.fetch_available_models().await.is_empty());

    let err = SessionKnobs::switch_model(&mut handle, "mock-opus")
        .await
        .expect_err("switch_model must fail without a model selector");
    assert!(
        matches!(err, ChatError::NotSupported(_)),
        "expected NotSupported, got {err:?}"
    );
    assert!(
        !model_capture.exists(),
        "no session/set_config_option frame may reach the agent"
    );
}

#[tokio::test]
async fn missing_binary_returns_connection_error() {
    let workspace = TempDir::new().expect("Failed to create temp workspace");
    let agent_config = mock_session_agent("/nonexistent/path/to/binary");

    let result = timeout(
        Duration::from_secs(10),
        AcpAgentHandle::new(mock_handle_params(&agent_config, workspace.path())),
    )
    .await
    .expect("missing binary should fail quickly");

    let error = result
        .err()
        .expect("AcpAgentHandle::new should return Err for missing binary");
    assert!(
        matches!(&error, AcpHandleError::Connection(message)
            if message.contains("Failed to spawn agent")),
        "a missing binary is a spawn failure, got {error:?}"
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
            acp_config: Some(&acp_config),
            ..mock_handle_params(&agent_config, workspace.path())
        }),
    )
    .await
    .expect("ACP handshake with injected errors timed out unexpectedly");

    let error = result
        .err()
        .expect("AcpAgentHandle::new should fail when mock agent injects protocol errors");
    // The agent's own message ("Simulated initialization error") does not
    // survive: the client reports the error reply as a missing result. So the
    // assertion names the failed step instead.
    assert!(
        matches!(&error, AcpHandleError::Connection(message)
            if message.contains("initialize")),
        "the failure must be the refused initialize, got {error:?}"
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
        // `biased` polls the receiver first, so a stop never wins over an
        // event that is already buffered.
        loop {
            tokio::select! {
                biased;
                maybe_event = bridge_rx.recv() => match maybe_event {
                    Ok(event) => {
                        if recording_tx.send(event).await.is_err() {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        panic!("the recording bridge lagged and dropped {n} events")
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                },
                _ = &mut stop_rx => break,
            }
        }
        // The stop comes after the test saw the events it waits for. Forward
        // whatever is still buffered, so none of them misses the recording.
        loop {
            match bridge_rx.try_recv() {
                Ok(event) => {
                    if recording_tx.send(event).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    panic!("the recording bridge lagged and dropped {n} events")
                }
                Err(
                    broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed,
                ) => return,
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

// ---------------------------------------------------------------------------
// A turn that ends early: dropped by the daemon, or cut by the agent
// ---------------------------------------------------------------------------

/// Wait until the capture file at `path` holds a value, and return it. The
/// mock replaces capture files in one rename, so a read never sees a half
/// written value.
async fn wait_for_capture(path: &Path, what: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(content) = std::fs::read_to_string(path) {
            if !content.is_empty() {
                return content;
            }
        }
        assert!(Instant::now() < deadline, "{what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Run one turn on `handle` to its end, and return its events.
async fn collect_turn(handle: &mut AcpAgentHandle, message: &str) -> Vec<TurnEvent> {
    timeout(Duration::from_secs(30), async {
        let stream = handle
            .turn(TurnContext::new(message))
            .await
            .expect("Agent::turn failed");
        stream.collect::<Vec<_>>().await
    })
    .await
    .expect("the turn timed out")
}

/// Start a held turn, drop its stream, and start the next turn at once.
///
/// Dropping a turn stream is how the daemon cancels an ACP turn. The client
/// must send `session/cancel` for the session the turn ran in. The next
/// turn must wait for the agent to end the dropped turn, and then run. It
/// must not fail because the client is still busy.
///
/// `tick_ms` makes the held turn stream a chunk per tick. Without it the
/// agent is quiet, as in a long tool call, and the client must notice the
/// drop without a chunk.
async fn drop_a_held_turn_then_run_the_next(tick_ms: Option<&str>) {
    const ANSWER: &str = "first words";
    let workspace = TempDir::new().expect("temp workspace");
    let cancel_capture = workspace.path().join("cancel.txt");
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let mut agent_config = mock_session_agent(&agent_path);
    for (key, value) in [
        ("CRU_MOCK_STREAM_CHUNKS", ANSWER.to_string()),
        ("CRU_MOCK_HOLD_UNTIL_CANCEL", "1".to_string()),
        (
            "CRU_MOCK_CANCEL_CAPTURE",
            cancel_capture.to_string_lossy().into_owned(),
        ),
    ] {
        agent_config.env_overrides.insert(key.to_string(), value);
    }
    if let Some(tick_ms) = tick_ms {
        agent_config
            .env_overrides
            .insert("CRU_MOCK_HOLD_TICK_MS".to_string(), tick_ms.to_string());
    }

    let mut handle = timeout(
        Duration::from_secs(30),
        AcpAgentHandle::new(mock_handle_params(&agent_config, workspace.path())),
    )
    .await
    .expect("ACP handshake timed out")
    .expect("ACP handshake failed");
    let session_id = handle
        .acp_session_id()
        .expect("a connected handle has an agent session");

    {
        let mut stream = handle
            .turn(TurnContext::new("start a long turn"))
            .await
            .expect("Agent::turn failed");
        let first = timeout(Duration::from_secs(30), stream.next())
            .await
            .expect("the held turn streamed nothing");
        assert!(
            matches!(&first, Some(TurnEvent::TextDelta(text)) if text == ANSWER),
            "the held turn's first event, got {first:?}"
        );
        // The daemon cancels a turn by dropping its stream.
    }

    let events = collect_turn(&mut handle, "and now a normal turn").await;

    let cancelled = wait_for_capture(
        &cancel_capture,
        "the agent never received session/cancel after the turn was dropped",
    )
    .await;
    assert_eq!(
        cancelled, session_id,
        "session/cancel must name the session the dropped turn ran in"
    );
    let text: String = events
        .iter()
        .filter_map(|event| match event {
            TurnEvent::TextDelta(delta) => Some(delta.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        text, ANSWER,
        "the next turn must run in full; events: {events:?}"
    );
    assert!(
        matches!(
            events.last(),
            Some(TurnEvent::Done {
                stop_reason: StopReason::EndTurn
            })
        ),
        "the next turn must end normally; events: {events:?}"
    );
}

/// The held turn streams a chunk every 20 ms, so a chunk finds the dropped
/// receiver.
#[tokio::test]
async fn dropping_a_turn_sends_session_cancel_and_the_next_turn_runs() {
    drop_a_held_turn_then_run_the_next(Some("20")).await;
}

/// The held turn sends nothing after its first chunk. No chunk finds the
/// dropped receiver, so the client must watch the receiver itself.
#[tokio::test]
async fn dropping_a_turn_of_a_quiet_agent_sends_session_cancel_and_the_next_turn_runs() {
    drop_a_held_turn_then_run_the_next(None).await;
}

/// An agent process that dies mid-turn ends the turn with a connection
/// error. The stream must end, not wait for a response that cannot come.
#[tokio::test]
async fn an_agent_that_exits_mid_turn_ends_the_turn_with_a_connection_error() {
    const PARTIAL: &str = "partial answer";
    let workspace = TempDir::new().expect("temp workspace");
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let mut agent_config = mock_session_agent(&agent_path);
    agent_config
        .env_overrides
        .insert("CRU_MOCK_STREAM_CHUNKS".to_string(), PARTIAL.to_string());
    agent_config
        .env_overrides
        .insert("CRU_MOCK_EXIT_MID_TURN".to_string(), "1".to_string());

    let mut handle = timeout(
        Duration::from_secs(30),
        AcpAgentHandle::new(mock_handle_params(&agent_config, workspace.path())),
    )
    .await
    .expect("ACP handshake timed out")
    .expect("ACP handshake failed");

    let events = timeout(Duration::from_secs(30), async {
        let stream = handle
            .turn(TurnContext::new("start a turn the agent will not finish"))
            .await
            .expect("Agent::turn failed");
        stream.collect::<Vec<_>>().await
    })
    .await
    .expect("a turn whose agent exited must end, not hang");

    assert!(
        matches!(events.first(), Some(TurnEvent::TextDelta(text)) if text == PARTIAL),
        "the chunk sent before the exit still reaches the turn; events: {events:?}"
    );
    assert!(
        matches!(
            events.last(),
            Some(TurnEvent::Error(TurnError::Connection(message)))
                if message.contains("ACP agent connection lost")
        ),
        "an agent exit is a connection error; events: {events:?}"
    );
}
