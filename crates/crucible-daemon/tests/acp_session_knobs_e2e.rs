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

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crucible_core::config::AgentProfile;
use crucible_core::session::SessionType;
use crucible_core::types::SessionKnob;
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::test_support::{kiln_name, temp_session_manager_with_kilns};
use crucible_daemon::AgentManager;
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::broadcast;

#[path = "acp_support/mock_agent.rs"]
mod mock_agent;
#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent::{logged, read_log, MockScript, Resume, Step};
use mock_agent_bin::{
    acp_manager_params, completed_turn, mock_profile, mock_session, profile_session_agent,
    MockSession, MOCK_PROFILE,
};

const TURN_TIMEOUT: Duration = Duration::from_secs(60);

/// What the mock agent streams on every turn.
const ANSWER: &str = "acknowledged";

/// The script of the mock agent. Resume is OFF: an agent that cannot resume
/// is the one a killed process costs the most, so the test fails loudly
/// rather than recovering. A caller adds fields for an agent that
/// advertises more.
fn script() -> MockScript {
    MockScript {
        // The agent advertises settings of its own, which Crucible has no
        // knob for and passes through untouched.
        agent_options: Some("low".to_string()),
        turn: vec![Step::Text(ANSWER.to_string())],
        ..MockScript::default()
    }
}

/// A profile that runs the mock agent with `script`. The agent appends
/// every frame it receives to `log_path`.
fn logging_profile(log_path: &Path, script: MockScript) -> AgentProfile {
    let script = MockScript {
        log: Some(log_path.to_path_buf()),
        ..script
    };
    mock_profile(BTreeMap::from([script.env()]))
}

/// Each `session/set_config_option` in `frames`, as `configId=value`.
fn option_sets(frames: &[Value]) -> Vec<String> {
    frames
        .iter()
        .filter(|frame| frame["method"] == "session/set_config_option")
        .map(|frame| {
            let params = &frame["params"];
            format!(
                "{}={}",
                params["configId"].as_str().unwrap_or_default(),
                params["value"].as_str().unwrap_or_default()
            )
        })
        .collect()
}

struct Harness {
    _temp: TempDir,
    log_path: std::path::PathBuf,
    agent_manager: Arc<AgentManager>,
    session_id: crucible_core::session::SessionId,
    event_tx: broadcast::Sender<SessionEventMessage>,
}

async fn setup() -> Harness {
    setup_with(script()).await
}

/// `setup` with a script that the caller builds.
async fn setup_with(script: MockScript) -> Harness {
    let temp = TempDir::new().expect("temp dir");
    let kiln = temp.path().join("kiln");
    std::fs::create_dir_all(&kiln).expect("kiln dir");
    let log_path = temp.path().join("methods.log");
    let script = MockScript {
        log: Some(log_path.clone()),
        ..script
    };
    let MockSession {
        agent_manager,
        session_id,
        event_tx,
        ..
    } = mock_session(&[("kiln", &kiln)], None, script).await;

    Harness {
        _temp: temp,
        log_path,
        agent_manager,
        session_id,
        event_tx,
    }
}

async fn run_a_turn(h: &Harness) {
    let (_id, done) = h
        .agent_manager
        .send_message_notified(&h.session_id, "hello".to_string(), &h.event_tx, true, None)
        .await
        .expect("the turn is accepted");
    let outcome = completed_turn(done, TURN_TIMEOUT).await;
    assert_eq!(outcome.final_text.trim(), ANSWER, "the agent's answer");
}

/// One `initialize` per agent process, so counting them counts processes.
fn handshakes(h: &Harness) -> usize {
    logged(&h.log_path, "initialize").len()
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

/// The same for every knob. One of them escaping the rule is as bad as all
/// of them, and they are added often enough that naming them here is worth
/// the repetition: the match below is exhaustive, so a new knob does not
/// compile until this test says what it does.
///
/// The agent advertises a model selector and a mode set, so the `Model` and
/// `Mode` knobs reach the wire instead of stopping at "not supported".
/// `ContextStrategy` has no ACP field and is refused (see
/// `a_setting_the_protocol_has_no_field_for_is_refused`); a refusal must not
/// cost the agent either.
#[tokio::test]
async fn no_knob_restarts_the_agent_process() {
    let h = setup_with(MockScript {
        models: true,
        mode: Some("default".to_string()),
        ..script()
    })
    .await;
    run_a_turn(&h).await;

    let id = h.session_id.as_str();
    for knob in SessionKnob::ALL {
        match knob {
            SessionKnob::ContextStrategy => {
                h.agent_manager
                    .set_context_strategy(
                        id,
                        crucible_core::session::ContextStrategy::Truncate,
                        None,
                    )
                    .await
                    .expect_err("ACP has no context strategy; the setter refuses it");
            }
            SessionKnob::Precognition => h
                .agent_manager
                .set_precognition(id, true, None)
                .await
                .expect("precognition"),
            SessionKnob::Model => {
                h.agent_manager
                    .switch_model(id, "mock-opus", None)
                    .await
                    .expect("the agent advertised mock-opus");
                assert_eq!(
                    option_sets(&read_log(&h.log_path))
                        .last()
                        .map(String::as_str),
                    Some("model=mock-opus"),
                    "the switch reached the agent"
                );
            }
            SessionKnob::Mode => h
                .agent_manager
                .set_mode(id, "plan", None)
                .await
                .expect("the agent declared `plan`"),
            SessionKnob::PluginTurnLimit => h
                .agent_manager
                .set_plugin_turn_limit(id, 7, None)
                .await
                .expect("the daemon owns this limit"),
        }
        assert_eq!(
            handshakes(&h),
            1,
            "setting `{}` cost the session its agent process",
            knob.id()
        );
    }

    run_a_turn(&h).await;

    assert_eq!(
        handshakes(&h),
        1,
        "no setting may cost the session its agent process"
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

    let captured = option_sets(&read_log(&h.log_path));
    assert_eq!(
        captured.last().map(String::as_str),
        Some("thought_level=high"),
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

/// The frames of the log that the agent process of the LAST handshake
/// received. Each `initialize` starts a new agent process.
fn last_process_frames(log_path: &Path) -> Vec<Value> {
    let log = read_log(log_path);
    let start = log
        .iter()
        .rposition(|frame| frame["method"] == "initialize")
        .expect("at least one agent process started");
    log[start..].to_vec()
}

/// Set the mode and the model on one manager, then rebuild the handle on a
/// second manager over the same sessions: a daemon restart.
///
/// The session stores both values, and both front ends show them. The
/// rebuilt handle must send both to the agent again. If it does not, the
/// agent runs its default while the session shows the old value.
///
/// With `resume_unknown`, the agent answers `-32002` to `session/resume`
/// and the connect opens a new session. A new session starts with the
/// defaults of the agent, so the daemon must send each value again.
async fn knobs_survive_a_handle_rebuild(resume_unknown: bool) {
    let temp = TempDir::new().expect("temp dir");
    let kiln = temp.path().join("kiln");
    std::fs::create_dir_all(&kiln).expect("kiln dir");
    let log_path = temp.path().join("methods.log");

    let session_manager = temp_session_manager_with_kilns(&[("kiln", &kiln)]);
    let (event_tx, _events) = broadcast::channel(256);
    let manager = |resume: Resume| {
        let script = MockScript {
            models: true,
            mode: Some("default".to_string()),
            session_resume: Some(resume),
            ..script()
        };
        Arc::new(AgentManager::new(acp_manager_params(
            session_manager.clone(),
            BTreeMap::from([(MOCK_PROFILE.to_string(), logging_profile(&log_path, script))]),
            &event_tx,
        )))
    };
    let turn = |agent_manager: Arc<AgentManager>, session_id: String| {
        let event_tx = event_tx.clone();
        async move {
            let (_id, done) = agent_manager
                .send_message_notified(&session_id, "hello".to_string(), &event_tx, true, None)
                .await
                .expect("the turn is accepted");
            completed_turn(done, TURN_TIMEOUT).await;
        }
    };

    let session = session_manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .expect("session");
    let id = session.id.as_str().to_string();

    // Manager one: open the agent session, then set both values.
    let first = manager(Resume::Adopt);
    first
        .configure_agent(&session.id, profile_session_agent(MOCK_PROFILE))
        .await
        .expect("configure the agent");
    turn(first.clone(), id.clone()).await;
    // The configured model is the profile name, and the agent does not offer
    // it. The session must then show the model that the agent runs.
    assert_eq!(
        session_manager
            .get_session(&id)
            .and_then(|s| s.agent)
            .map(|a| a.model)
            .as_deref(),
        Some("mock-sonnet"),
        "the session must show the model of the agent, not a model it does not offer"
    );
    first
        .set_mode(&id, "plan", None)
        .await
        .expect("the agent declared `plan`");
    first
        .switch_model(&id, "mock-opus", None)
        .await
        .expect("the agent advertised mock-opus");
    drop(first);
    let first_frames = read_log(&log_path);
    assert!(
        first_frames
            .iter()
            .any(|frame| frame["method"] == "session/set_mode"),
        "manager one sent the mode"
    );
    assert!(
        !option_sets(&first_frames).is_empty(),
        "manager one sent the model"
    );

    // Manager two: a cold handle cache, like a restarted daemon.
    let second = manager(if resume_unknown {
        Resume::Unknown
    } else {
        Resume::Adopt
    });
    turn(second.clone(), id.clone()).await;

    // Only the frames of the rebuilt handle's process count below.
    let methods = last_process_frames(&log_path);
    let stored = session_manager
        .get_session(&id)
        .and_then(|s| s.agent)
        .expect("the session keeps its agent");

    // The mode.
    assert_eq!(stored.mode.as_deref(), Some("plan"), "the stored mode");
    assert_eq!(
        second.session_modes(&id).current_mode_id.0.as_ref(),
        "plan",
        "the rebuilt handle must report the stored mode"
    );
    let modes: Vec<&Value> = methods
        .iter()
        .filter(|frame| frame["method"] == "session/set_mode")
        .map(|frame| &frame["params"]["modeId"])
        .collect();
    assert!(
        !modes.is_empty(),
        "the rebuilt handle must send the stored mode to the agent; methods: {methods:?}"
    );
    assert_eq!(
        modes.last().and_then(|mode| mode.as_str()),
        Some("plan"),
        "the agent must receive the stored mode"
    );

    // The model. A real resume keeps the model in the session of the agent.
    // The mock does not send `configOptions` in its `session/resume` reply,
    // so after a plain resume the handle has no model selector, and the
    // daemon has nothing to send the model to. Only the fallback to
    // `session/new` gives the handle a selector.
    if resume_unknown {
        assert_eq!(stored.model, "mock-opus", "the stored model");
        let options = option_sets(&methods);
        assert!(
            !options.is_empty(),
            "the rebuilt handle must send the stored model to the new agent \
             session; methods: {methods:?}"
        );
        assert_eq!(
            options.last().map(String::as_str),
            Some("model=mock-opus"),
            "the agent must run the model that the session shows"
        );
    }
}

#[tokio::test]
async fn the_mode_and_model_survive_a_handle_rebuild_that_resumes() {
    knobs_survive_a_handle_rebuild(false).await;
}

#[tokio::test]
async fn the_mode_and_model_survive_a_handle_rebuild_that_falls_back_to_a_new_session() {
    knobs_survive_a_handle_rebuild(true).await;
}
