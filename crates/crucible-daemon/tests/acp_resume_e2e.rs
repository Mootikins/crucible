//! Resuming an ACP agent's own session across a daemon restart.
//!
//! Three layers already have a piece of this. `agent_handshake_tests.rs`
//! drives `connect_with_best_mcp_resuming` against an in-process mock and
//! proves the two dispositions. `messaging.rs` proves the id an agent
//! reports is persisted and survives storage. Neither joins them: nothing
//! shows that a *second* handle, built by the production factory from what
//! the first turn persisted, actually sends `session/resume` for that id.
//!
//! That join is the whole feature. A break anywhere along it — the factory
//! not reading the stored id, `send.rs` not writing it, the id changing
//! shape in storage — leaves every existing test green while the agent
//! silently forgets the conversation on every daemon restart.
//!
//! Two `AgentManager`s over one `SessionManager` is a daemon restart as far
//! as the agent is concerned: the second has an empty handle cache, so it
//! must rebuild the handle from persisted state, which is exactly what a
//! restarted daemon does.
//!
//! The agent processes write every method they receive to one appended file
//! (`CRU_MOCK_METHOD_LOG`). Both agent processes append to it, so the file is
//! the complete record of what crossed both handshakes.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crucible_core::config::{AcpConfig, AgentProfile, BackendType};
use crucible_core::session::{OutputValidation, SessionAgent, SessionType};
use crucible_core::traits::chat::AgentHandle;
use crucible_daemon::acp_handle::{AcpAgentHandle, AcpAgentHandleParams};
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::test_support::{kiln_name, temp_session_manager_with_kilns};
use crucible_daemon::{
    AgentManager, AgentManagerParams, BackgroundJobManager, KilnManager, SessionManager,
};
use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::timeout;

#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent_bin::{mock_agent_path, mock_handle_params, mock_session_agent};

/// A cold spawn plus a handshake plus a turn, against a second process.
const TURN_TIMEOUT: Duration = Duration::from_secs(60);

const ANSWER: &str = "the resumed agent answered";

/// The profile both managers resolve `mock-acp` to. `log_path` is appended
/// by every agent process the profile starts.
fn resuming_profile(log_path: &Path) -> AgentProfile {
    let mut env = BTreeMap::new();
    env.insert("CRU_MOCK_STREAM_CHUNKS".to_string(), ANSWER.to_string());
    // Answer `session/resume` rather than `-32601`.
    env.insert("CRU_MOCK_SESSION_RESUME".to_string(), "1".to_string());
    env.insert(
        "CRU_MOCK_METHOD_LOG".to_string(),
        log_path.to_string_lossy().into_owned(),
    );
    AgentProfile {
        extends: None,
        command: Some(mock_agent_path().to_string_lossy().into_owned()),
        args: Some(Vec::new()),
        env,
        description: Some("mock ACP agent for resume tests".to_string()),
        delegation: None,
        permissions: None,
    }
}

/// A session agent that names the `mock-acp` profile.
fn acp_agent() -> SessionAgent {
    SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some("mock-acp".to_string()),
        provider_key: None,
        provider: BackendType::Custom,
        model: "mock-acp".to_string(),
        system_prompt: String::new(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        output_validation: OutputValidation::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        tool_policy: None,
        mode: None,
    }
}

/// One `AgentManager` over an existing `SessionManager`. Called twice with
/// the same session manager to model a daemon restart.
fn manager(
    session_manager: Arc<SessionManager>,
    acp_config: AcpConfig,
    event_tx: broadcast::Sender<SessionEventMessage>,
) -> Arc<AgentManager> {
    Arc::new(AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager,
        background_manager: Arc::new(BackgroundJobManager::new(event_tx)),
        mcp_gateway: None,
        llm_config: None,
        acp_config: Some(acp_config),
        context_config: None,
        permission_config: None,
        plugin_loader: None,
        card_roots: Default::default(),
    }))
}

/// Every method line the agent processes recorded, in order.
fn method_log(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// The full loop: turn one opens an agent session, the daemon persists its
/// id, and a rebuilt handle resumes that id instead of opening a second one.
#[tokio::test]
async fn a_rebuilt_handle_resumes_the_agent_session_the_first_turn_opened() {
    let temp = TempDir::new().expect("temp dir");
    let kiln = temp.path().join("kiln");
    std::fs::create_dir_all(&kiln).expect("kiln dir");
    let log_path = temp.path().join("methods.log");

    let session_manager = temp_session_manager_with_kilns(&[("kiln", &kiln)]);
    let (event_tx, _event_rx) = broadcast::channel(256);
    let acp_config = AcpConfig {
        default_agent: None,
        streaming_timeout_minutes: 1,
        agents: BTreeMap::from([("mock-acp".to_string(), resuming_profile(&log_path))]),
    };

    let session = session_manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .expect("session");

    // Turn one, on the first manager.
    let first = manager(
        session_manager.clone(),
        acp_config.clone(),
        event_tx.clone(),
    );
    first
        .configure_agent(&session.id, acp_agent())
        .await
        .expect("configure the ACP agent");
    let (_id, done) = first
        .send_message_notified(&session.id, "first".to_string(), &event_tx, true, None)
        .await
        .expect("turn one accepted");
    let _ = timeout(TURN_TIMEOUT, done)
        .await
        .expect("turn one finished");

    let agent_session_id = session_manager
        .get_session(&session.id)
        .and_then(|s| s.acp_session_id)
        .expect("turn one must persist the agent's session id");

    // Turn two, on a second manager: a cold handle cache, like a restart.
    let second = manager(session_manager.clone(), acp_config, event_tx.clone());
    let (_id, done) = second
        .send_message_notified(&session.id, "second".to_string(), &event_tx, true, None)
        .await
        .expect("turn two accepted");
    let _ = timeout(TURN_TIMEOUT, done)
        .await
        .expect("turn two finished");

    let log = method_log(&log_path);
    let resume_line = format!("session/resume {agent_session_id}");
    assert!(
        log.contains(&resume_line),
        "the rebuilt handle must resume the persisted id; wanted {resume_line:?} in {log:?}"
    );
    assert_eq!(
        log.iter().filter(|l| l.starts_with("session/new")).count(),
        1,
        "only the FIRST handshake may open a session; a second means the \
         agent lost its history. log: {log:?}"
    );
    assert_eq!(
        session_manager
            .get_session(&session.id)
            .and_then(|s| s.acp_session_id)
            .as_deref(),
        Some(agent_session_id.as_str()),
        "the resumed id must still be the persisted one after turn two"
    );
}

/// An agent that reports its mode set again on `session/resume` hands it to
/// the resumed session. The `session/new` path is covered by
/// `acp_integration/session_modes.rs`; the resume path builds its
/// `AcpSession` from a different response type and had no test at all, so a
/// resumed session could offer Crucible's internal modes while the agent
/// was in one of its own.
#[tokio::test]
async fn a_resumed_session_adopts_the_modes_the_agent_reports_on_resume() {
    let workspace = TempDir::new().expect("temp workspace");
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let mut agent_config = mock_session_agent(&agent_path);
    agent_config
        .env_overrides
        .insert("CRU_MOCK_SESSION_RESUME".into(), "1".into());
    agent_config
        .env_overrides
        .insert("CRU_MOCK_ADVERTISE_MODES".into(), "plan".into());

    let handle = timeout(
        TURN_TIMEOUT,
        AcpAgentHandle::new(AcpAgentHandleParams {
            resume_acp_session_id: Some("mock-session-carried".into()),
            ..mock_handle_params(&agent_config, workspace.path())
        }),
    )
    .await
    .expect("the handshake finished inside the timeout")
    .expect("the resume handshake succeeds");

    assert_eq!(
        handle.acp_session_id().as_deref(),
        Some("mock-session-carried"),
        "the resumed session keeps the id it asked for"
    );

    let modes = handle.get_modes().expect("a resumed session reports modes");
    let ids: Vec<&str> = modes
        .available_modes
        .iter()
        .map(|mode| mode.id.0.as_ref())
        .collect();
    assert_eq!(
        ids,
        ["default", "acceptEdits", "plan"],
        "a resumed session must offer the agent's modes, not Crucible's own"
    );
    assert_eq!(
        handle.get_mode_id(),
        "plan",
        "the resumed session starts in the mode the agent reports as current"
    );
}

/// `-32601` and a normal error mean different things, and the client must
/// not conflate them. `-32601` says "I do not have this method" and falls
/// back to `session/new`. Any other error says "I have the method and I am
/// refusing this call" — a stale id, most often, because the agent itself
/// restarted and no longer knows the session.
///
/// Today that second case fails the connect outright. This test pins that,
/// deliberately: the behaviour is defensible (a silent fallback would hide
/// that the agent lost the conversation) but it is a real user-facing
/// outcome — a session whose stored id has gone stale cannot start until
/// the id is cleared — and it must not change without someone deciding to
/// change it.
#[tokio::test]
async fn a_resume_refused_with_a_normal_error_fails_the_connect() {
    let workspace = TempDir::new().expect("temp workspace");
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let mut agent_config = mock_session_agent(&agent_path);
    agent_config
        .env_overrides
        .insert("CRU_MOCK_SESSION_RESUME".into(), "1".into());
    agent_config
        .env_overrides
        .insert("CRU_MOCK_RESUME_REJECT".into(), "1".into());

    let outcome = timeout(
        TURN_TIMEOUT,
        AcpAgentHandle::new(AcpAgentHandleParams {
            resume_acp_session_id: Some("a-session-the-agent-forgot".into()),
            ..mock_handle_params(&agent_config, workspace.path())
        }),
    )
    .await
    .expect("the handshake finished inside the timeout");

    let error = outcome
        .err()
        .expect("a refused resume must not silently become a new session");
    assert!(
        error.to_string().contains("session/resume"),
        "the failure must name the method that refused, got: {error}"
    );
}

/// The other half of the pair: `-32601` still falls back, so the two error
/// replies are demonstrably handled differently by one running client. On
/// its own the test above could be satisfied by a client that failed on
/// every resume error, including the one it must tolerate.
#[tokio::test]
async fn a_resume_refused_with_method_not_found_still_falls_back_to_a_new_session() {
    let workspace = TempDir::new().expect("temp workspace");
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    // No CRU_MOCK_SESSION_RESUME: the mock answers -32601.
    let agent_config = mock_session_agent(&agent_path);

    let handle = timeout(
        TURN_TIMEOUT,
        AcpAgentHandle::new(AcpAgentHandleParams {
            resume_acp_session_id: Some("a-session-this-agent-never-had".into()),
            ..mock_handle_params(&agent_config, workspace.path())
        }),
    )
    .await
    .expect("the handshake finished inside the timeout")
    .expect("a -32601 reply to session/resume falls back to session/new");

    let id = handle
        .acp_session_id()
        .expect("the fallback session still has an id");
    assert_ne!(
        id, "a-session-this-agent-never-had",
        "the fallback must open a NEW session, not pretend the old one resumed"
    );
}
