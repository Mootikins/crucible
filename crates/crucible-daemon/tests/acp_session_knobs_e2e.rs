//! What an ACP session's adjustable settings do, and to what.
//!
//! Crucible's knob set is modelled on the internal agent. ACP carries almost
//! none of it: the wire has a model selector, a mode, and whatever the agent
//! advertises in `configOptions`. There is no temperature and no token cap.
//!
//! Two things followed from applying that set to ACP anyway, and these tests
//! pin both.
//!
//! A knob change evicted the session's cached handle so the next turn would
//! rebuild it. Dropping the last `Arc` to an `AcpAgentHandle` sends
//! `session/close` and then SIGKILLs the agent process, so setting the
//! temperature killed the agent. Nothing in an ACP handle is built from these
//! fields, so the rebuild was pure cost — and only `session/resume` got the
//! conversation back.
//!
//! The agent process writes every method it receives to one appended file, so
//! a second `initialize` in that file is a second agent process. That is the
//! assertion: one process across the whole session.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crucible_core::config::{AcpConfig, AgentProfile, BackendType};
use crucible_core::session::{OutputValidation, SessionAgent, SessionType};
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::test_support::{kiln_name, temp_session_manager_with_kilns};
use crucible_daemon::{AgentManager, AgentManagerParams, BackgroundJobManager, KilnManager};
use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::timeout;

#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent_bin::mock_agent_path;

const TURN_TIMEOUT: Duration = Duration::from_secs(60);

/// A profile that runs the mock agent and appends every method it receives to
/// `log_path`. Resume is OFF: an agent that cannot resume is the one a killed
/// process costs the most, so the test fails loudly rather than recovering.
fn logging_profile(log_path: &Path) -> AgentProfile {
    let mut env = BTreeMap::new();
    env.insert(
        "CRU_MOCK_STREAM_CHUNKS".to_string(),
        "acknowledged".to_string(),
    );
    env.insert(
        "CRU_MOCK_METHOD_LOG".to_string(),
        log_path.to_string_lossy().into_owned(),
    );
    AgentProfile {
        extends: None,
        command: Some(mock_agent_path().to_string_lossy().into_owned()),
        args: Some(Vec::new()),
        env,
        description: Some("mock ACP agent for knob tests".to_string()),
        delegation: None,
        permissions: None,
    }
}

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
        context_window: None,
        output_validation: OutputValidation::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        tool_policy: None,
        mode: None,
    }
}

struct Harness {
    _temp: TempDir,
    log_path: std::path::PathBuf,
    agent_manager: Arc<AgentManager>,
    session_id: crucible_core::session::SessionId,
    event_tx: broadcast::Sender<SessionEventMessage>,
}

async fn setup() -> Harness {
    let temp = TempDir::new().expect("temp dir");
    let kiln = temp.path().join("kiln");
    std::fs::create_dir_all(&kiln).expect("kiln dir");
    let log_path = temp.path().join("methods.log");

    let session_manager = temp_session_manager_with_kilns(&[("kiln", &kiln)]);
    let (event_tx, _events) = broadcast::channel(256);

    let agent_manager = Arc::new(AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: session_manager.clone(),
        background_manager: Arc::new(BackgroundJobManager::new(event_tx.clone())),
        mcp_gateway: None,
        llm_config: None,
        acp_config: Some(AcpConfig {
            default_agent: None,
            streaming_timeout_minutes: 1,
            agents: BTreeMap::from([("mock-acp".to_string(), logging_profile(&log_path))]),
        }),
        context_config: None,
        permission_config: None,
        plugin_loader: None,
        card_roots: Default::default(),
    }));

    let session = session_manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .expect("session");
    agent_manager
        .configure_agent(&session.id, acp_agent())
        .await
        .expect("configure the agent");

    Harness {
        _temp: temp,
        log_path,
        agent_manager,
        session_id: session.id,
        event_tx,
    }
}

async fn run_a_turn(h: &Harness) {
    let (_id, done) = h
        .agent_manager
        .send_message_notified(&h.session_id, "hello".to_string(), &h.event_tx, true, None)
        .await
        .expect("the turn is accepted");
    let _ = timeout(TURN_TIMEOUT, done)
        .await
        .expect("the turn finished");
}

/// One `initialize` per agent process, so counting them counts processes.
fn handshakes(h: &Harness) -> usize {
    std::fs::read_to_string(&h.log_path)
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("initialize"))
        .count()
}

/// The headline: a knob change must not kill the agent.
#[tokio::test]
async fn changing_a_knob_does_not_restart_the_agent_process() {
    let h = setup().await;
    run_a_turn(&h).await;
    assert_eq!(handshakes(&h), 1, "the first turn starts one agent");

    h.agent_manager
        .set_temperature(h.session_id.as_str(), 0.2, None)
        .await
        .expect("the setting is accepted");
    run_a_turn(&h).await;

    assert_eq!(
        handshakes(&h),
        1,
        "the session must still be talking to the agent it started; a second \
         handshake means the first process was killed and its conversation lost"
    );
}

/// The same for every other knob that routes through the shared config
/// mutator. One of them escaping the rule is as bad as all of them, and they
/// are added often enough that naming them here is worth the repetition.
#[tokio::test]
async fn no_knob_restarts_the_agent_process() {
    let h = setup().await;
    run_a_turn(&h).await;

    let id = h.session_id.as_str();
    h.agent_manager
        .set_thinking_budget(id, 4096, None)
        .await
        .expect("thinking budget");
    h.agent_manager
        .set_max_tokens(id, Some(2048), None)
        .await
        .expect("max tokens");
    h.agent_manager
        .set_precognition(id, true, None)
        .await
        .expect("precognition");
    h.agent_manager
        .set_precognition_results(id, 3, None)
        .await
        .expect("precognition results");

    run_a_turn(&h).await;

    assert_eq!(
        handshakes(&h),
        1,
        "no setting may cost the session its agent process"
    );
}

/// Fails fast and explains, rather than letting the tests above die on a
/// spawn error that names nothing.
#[test]
fn the_mock_agent_binary_is_available() {
    let path = mock_agent_path();
    assert!(
        path.exists(),
        "mock-acp-agent is missing at {}; build it with \
         `cargo build -p crucible-daemon --features test-utils --bin mock-acp-agent`",
        path.display()
    );
}
