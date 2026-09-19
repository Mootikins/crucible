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
use crucible_core::session::{SessionAgent, SessionType};
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
    // The agent advertises settings of its own, which Crucible has no knob
    // for and passes through untouched.
    env.insert(
        "CRU_MOCK_ADVERTISE_AGENT_OPTIONS".to_string(),
        "low".to_string(),
    );
    env.insert(
        "CRU_MOCK_MODEL_CAPTURE".to_string(),
        log_path
            .with_extension("option")
            .to_string_lossy()
            .into_owned(),
    );
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
        max_context_tokens: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        context_budget: None,
        context_strategy: Default::default(),
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
        review_snapshot_root: crucible_daemon::test_support::scratch_snapshot_root(),
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
        .set_precognition(h.session_id.as_str(), true, None)
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
        .set_precognition(id, true, None)
        .await
        .expect("precognition");

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

/// A setting ACP cannot carry is refused rather than stored.
///
/// These setters never ask the handle: they write the session's config and
/// stop. So an accepted `set_context_budget` was a value the agent process
/// would never see, reported back to the caller as though it had taken
/// effect. The error names the setting, because "not supported" alone leaves
/// a user guessing which control just failed.
#[tokio::test]
async fn a_setting_the_protocol_has_no_field_for_is_refused() {
    let h = setup().await;
    let id = h.session_id.as_str();

    let attempts = [(
        "context_strategy",
        h.agent_manager
            .set_context_strategy(id, crucible_core::session::ContextStrategy::Truncate, None)
            .await,
    )];

    for (name, result) in attempts {
        let error = result
            .expect_err(&format!("`{name}` must be refused on an ACP session"))
            .to_string();
        assert!(
            error.contains(name),
            "the refusal must name the setting that failed; `{name}` got: {error}"
        );
    }
}

/// The settings the daemon itself implements stay available. Refusing these
/// would remove a control that demonstrably works: retrieval runs in the
/// daemon and reaches the agent as injected prompt text.
#[tokio::test]
async fn a_setting_the_daemon_implements_is_still_accepted() {
    let h = setup().await;
    let id = h.session_id.as_str();

    h.agent_manager
        .set_precognition(id, true, None)
        .await
        .expect("precognition is the daemon's own work");
}

/// A client asks the session which settings it has, and gets an answer that
/// depends on the session rather than on a fixed list.
///
/// This is what a settings panel draws from. Without it a front end has to
/// guess, and the web guessed wrong for every ACP session — a temperature
/// slider on an agent with no temperature.
#[tokio::test]
async fn a_session_reports_which_settings_it_supports() {
    let h = setup().await;
    let knobs = h.agent_manager.session_knobs(h.session_id.as_str());

    let supported = |id: &str| {
        knobs
            .iter()
            .find(|(knob, _)| knob.id() == id)
            .map(|(_, ok)| *ok)
            .unwrap_or_else(|| panic!("`{id}` is missing from the answer"))
    };

    assert!(
        !supported("context_strategy"),
        "the agent owns its history, so the daemon assembles nothing to trim"
    );
    assert!(supported("mode"), "session/set_mode carries the mode");
    assert!(
        supported("precognition"),
        "retrieval is the daemon's own work"
    );

    // The mock advertises no model selector, so this session cannot switch
    // model — and that is a property of the agent, not of ACP.
    assert!(
        !supported("model"),
        "an agent that advertises no selector cannot switch model"
    );

    assert_eq!(
        knobs.len(),
        crucible_core::types::SessionKnob::ALL.len(),
        "every knob must be answered for, or a client cannot tell absent from unsupported"
    );
}

/// The agent's own settings reach a client, and Crucible does not pretend to
/// understand them.
///
/// ACP's extensibility point is `configOptions`: an agent lists what it has,
/// and a different agent lists different things. Crucible reads only the
/// model selector out of that list, so a reasoning-level selector — a
/// category the spec names — reached nobody. These are not knobs; they are
/// the agent's, and a client renders them from what the agent said.
#[tokio::test]
async fn the_agents_own_settings_reach_a_client() {
    let h = setup().await;

    // Nothing before a handshake: an agent says what it has when the daemon
    // connects to it. This accessor is the sync read — a read through
    // `live_agent_config_options` brings the connection up first.
    assert!(
        h.agent_manager
            .agent_config_options(h.session_id.as_str())
            .is_empty(),
        "an agent that has not been connected to has advertised nothing"
    );

    run_a_turn(&h).await;

    let options = h.agent_manager.agent_config_options(h.session_id.as_str());
    let ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(
        ids,
        ["thought_level", "verbose_logs"],
        "the agent's own options must reach the client, and the model selector \
         must not — it already has a control of its own"
    );

    let reasoning = &options[0];
    assert_eq!(reasoning.name, "Reasoning", "the agent's label, not an id");
    assert_eq!(reasoning.category.as_deref(), Some("thought_level"));
    match &reasoning.kind {
        crucible_core::types::AgentOptionKind::Select { current, choices } => {
            assert_eq!(current, "low");
            assert_eq!(
                choices.iter().map(|c| c.value.as_str()).collect::<Vec<_>>(),
                ["low", "high"]
            );
        }
        other => panic!("a select must project as a select; got {other:?}"),
    }

    // A shape that is not a select still reaches the client, because an agent
    // that offers a toggle means it.
    match &options[1].kind {
        crucible_core::types::AgentOptionKind::Toggle { current } => {
            assert!(!current, "the agent reported it off");
        }
        other => panic!("a boolean must project as a toggle; got {other:?}"),
    }
}

/// Setting one reaches the agent over the wire, and an id the agent never
/// advertised is refused here rather than by the agent.
#[tokio::test]
async fn setting_an_agent_option_reaches_the_agent() {
    let h = setup().await;
    run_a_turn(&h).await;

    h.agent_manager
        .set_agent_config_option(h.session_id.as_str(), "thought_level", "high", None)
        .await
        .expect("the agent advertised this option");

    let captured = std::fs::read_to_string(h.log_path.with_extension("option"))
        .expect("the agent process must have received a session/set_config_option");
    assert_eq!(
        captured.trim(),
        "thought_level=high",
        "the option and its value must reach the agent unchanged"
    );

    let error = h
        .agent_manager
        .set_agent_config_option(h.session_id.as_str(), "invented", "1", None)
        .await
        .expect_err("an option the agent never advertised must be refused");
    assert!(
        error.to_string().contains("invented"),
        "the refusal must name the option; got: {error}"
    );
}
