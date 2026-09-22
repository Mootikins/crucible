//! Resuming an ACP agent's own session across a daemon restart.
//!
//! Three layers already have a piece of this. `agent_handshake_tests.rs`
//! drives `handshake` with a resume id against an in-process mock and
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
//! The agent processes write every frame they receive to one appended file
//! (the script's `log`). Both agent processes append to it, so the file is
//! the complete record of what crossed both handshakes.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crucible_core::session::SessionType;
use crucible_core::traits::chat::AgentHandle;
use crucible_daemon::acp_handle::{AcpAgentHandle, AcpAgentHandleParams};
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::test_support::{kiln_name, temp_session_manager_with_kilns};
use crucible_daemon::{AgentManager, SessionManager};
use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::timeout;

#[path = "acp_support/mock_agent.rs"]
mod mock_agent;
#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent::{read_log, MockScript, Resume, Step};
use mock_agent_bin::{
    acp_manager_params, completed_turn, mock_agent_path, mock_handle_params, mock_profile,
    mock_session_agent, profile_session_agent, MOCK_PROFILE,
};

/// A cold spawn plus a handshake plus a turn, against a second process.
const TURN_TIMEOUT: Duration = Duration::from_secs(60);

const ANSWER: &str = "the resumed agent answered";

/// The script of the agent that both managers resolve `mock-acp` to. Every
/// agent process that runs it appends to `log_path`.
fn resuming_script(log_path: &Path) -> MockScript {
    MockScript {
        turn: vec![Step::Text(ANSWER.to_string())],
        // Answer `session/resume` rather than `-32601`.
        session_resume: Some(Resume::Adopt),
        log: Some(log_path.to_path_buf()),
        ..MockScript::default()
    }
}

/// One `AgentManager` over an existing `SessionManager`. Called twice with
/// the same session manager to model a daemon restart.
fn manager(
    session_manager: Arc<SessionManager>,
    script: MockScript,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Arc<AgentManager> {
    Arc::new(AgentManager::new(acp_manager_params(
        session_manager,
        BTreeMap::from([(
            MOCK_PROFILE.to_string(),
            mock_profile(BTreeMap::from([script.env()])),
        )]),
        event_tx,
    )))
}

/// Every frame that the agent processes recorded, in order, as
/// `<method> <sessionId>`. A frame without a session id shows `-`.
fn method_log(path: &Path) -> Vec<String> {
    read_log(path)
        .iter()
        .map(|frame| {
            let session_id = frame["params"]["sessionId"].as_str().unwrap_or("-");
            format!(
                "{} {session_id}",
                frame["method"].as_str().unwrap_or_default()
            )
        })
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

    let session = session_manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .expect("session");

    // Turn one, on the first manager.
    let first = manager(
        session_manager.clone(),
        resuming_script(&log_path),
        &event_tx,
    );
    first
        .configure_agent(&session.id, profile_session_agent(MOCK_PROFILE))
        .await
        .expect("configure the ACP agent");
    let (_id, done) = first
        .send_message_notified(&session.id, "first".to_string(), &event_tx, true, None)
        .await
        .expect("turn one accepted");
    let outcome = completed_turn(done, TURN_TIMEOUT).await;
    assert_eq!(outcome.final_text.trim(), ANSWER, "turn one's answer");

    let agent_session_id = session_manager
        .get_session(&session.id)
        .and_then(|s| s.acp_session_id)
        .expect("turn one must persist the agent's session id");

    // Turn two, on a second manager: a cold handle cache, like a restart.
    let second = manager(
        session_manager.clone(),
        resuming_script(&log_path),
        &event_tx,
    );
    let (_id, done) = second
        .send_message_notified(&session.id, "second".to_string(), &event_tx, true, None)
        .await
        .expect("turn two accepted");
    let outcome = completed_turn(done, TURN_TIMEOUT).await;
    assert_eq!(outcome.final_text.trim(), ANSWER, "turn two's answer");

    let log = method_log(&log_path);
    let resume_line = format!("session/resume {agent_session_id}");
    assert!(
        log.contains(&resume_line),
        "the rebuilt handle must resume the persisted id; wanted {resume_line:?} in {log:?}"
    );
    // The resumed id is only worth something if the turn runs in it: a
    // prompt addressed to any other session would reach an agent that has
    // no history for it.
    let prompt_line = format!("session/prompt {agent_session_id}");
    let resume_at = log.iter().position(|l| *l == resume_line);
    let prompts_after_resume = resume_at
        .map(|at| {
            log[at..]
                .iter()
                .filter(|l| l.starts_with("session/prompt"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert_eq!(
        prompts_after_resume,
        [&prompt_line],
        "turn two's session/prompt must go to the resumed session; log: {log:?}"
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

/// A stored id that the agent no longer knows is replaced by the id of the
/// session that the fallback opens.
///
/// The agent answers `-32002` for the stored id, and the connect falls back
/// to `session/new`. If `send.rs` keeps the stale id, every later rebuild
/// sends `session/resume` for it again, and every rebuild loses the history
/// of the session that the fallback opened. The second manager proves the
/// replacement: it must resume the NEW id.
#[tokio::test]
async fn a_resume_fallback_replaces_the_stale_stored_id_with_the_new_one() {
    let temp = TempDir::new().expect("temp dir");
    let kiln = temp.path().join("kiln");
    std::fs::create_dir_all(&kiln).expect("kiln dir");
    let log_path = temp.path().join("methods.log");

    let session_manager = temp_session_manager_with_kilns(&[("kiln", &kiln)]);
    let (event_tx, _event_rx) = broadcast::channel(256);

    let session = session_manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .expect("session");

    // The profile of the first manager: the agent forgot every session.
    let forgetting = MockScript {
        session_resume: Some(Resume::Unknown),
        ..resuming_script(&log_path)
    };
    let first = manager(session_manager.clone(), forgetting, &event_tx);
    first
        .configure_agent(&session.id, profile_session_agent(MOCK_PROFILE))
        .await
        .expect("configure the ACP agent");

    // A previous handle stored an id that the agent no longer knows.
    let mut stored = session_manager
        .get_session(&session.id)
        .expect("the session exists");
    stored.acp_session_id = Some("stale".to_string());
    session_manager
        .storage()
        .save(&stored)
        .await
        .expect("store the stale id");
    session_manager.register_transient(stored);

    let (_id, done) = first
        .send_message_notified(&session.id, "first".to_string(), &event_tx, true, None)
        .await
        .expect("turn one accepted");
    completed_turn(done, TURN_TIMEOUT).await;

    let log = method_log(&log_path);
    assert!(
        log.contains(&"session/resume stale".to_string()),
        "the first handle must try to resume the stored id; log: {log:?}"
    );
    let opened: Vec<&str> = log
        .iter()
        .filter_map(|l| l.strip_prefix("session/prompt "))
        .collect();
    let [fresh_id] = opened[..] else {
        panic!("turn one must send exactly one prompt; log: {log:?}");
    };
    assert_ne!(fresh_id, "stale", "the fallback must open a new session");
    let persisted = session_manager
        .get_session(&session.id)
        .and_then(|s| s.acp_session_id);
    assert_eq!(
        persisted.as_deref(),
        Some(fresh_id),
        "the id of the fallback session must replace the stale stored id"
    );
    let fresh_id = fresh_id.to_string();

    // The second manager: the agent resumes what it is asked for.
    let second = manager(
        session_manager.clone(),
        resuming_script(&log_path),
        &event_tx,
    );
    let (_id, done) = second
        .send_message_notified(&session.id, "second".to_string(), &event_tx, true, None)
        .await
        .expect("turn two accepted");
    completed_turn(done, TURN_TIMEOUT).await;

    let log = method_log(&log_path);
    let resumes: Vec<&String> = log
        .iter()
        .filter(|l| l.starts_with("session/resume"))
        .collect();
    assert_eq!(
        resumes,
        [
            &"session/resume stale".to_string(),
            &format!("session/resume {fresh_id}")
        ],
        "the rebuilt handle must resume the NEW id, not the stale one; log: {log:?}"
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
    agent_config.env_overrides.extend([MockScript {
        session_resume: Some(Resume::Adopt),
        mode: Some("plan".into()),
        ..MockScript::default()
    }
    .env()]);

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

/// Build a handle over `workspace` that asks the agent to resume the stored
/// id `stored`. The agent answers `session/resume` as `resume` says.
async fn connect_resuming(
    workspace: &Path,
    resume: Resume,
    stored: &str,
) -> Result<AcpAgentHandle, crucible_daemon::acp_handle::AcpHandleError> {
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let mut agent_config = mock_session_agent(&agent_path);
    agent_config.env_overrides.extend([MockScript {
        session_resume: Some(resume),
        ..MockScript::default()
    }
    .env()]);

    timeout(
        TURN_TIMEOUT,
        AcpAgentHandle::new(AcpAgentHandleParams {
            resume_acp_session_id: Some(stored.into()),
            ..mock_handle_params(&agent_config, workspace)
        }),
    )
    .await
    .expect("the handshake finished inside the timeout")
}

/// `-32601` and a normal error mean different things, and the client must
/// not conflate them. `-32601` says "I do not have this method" and falls
/// back to `session/new` — and so does `-32002` "Resource not found"
/// (claude-agent-acp and codex-acp answer it for a session that never
/// persisted a turn, and reaped sessions answer it too; a stored id that
/// can never resume would otherwise brick the session on every restart).
/// Any OTHER error says "I have the method and I am refusing this call" —
/// an agent-side breakage, most often — and still fails the connect
/// outright. This test pins that third case, deliberately: a silent
/// fallback would hide that the agent lost the conversation, and it must
/// not widen without someone deciding to widen it.
///
/// `resume_fallback_is_announced_in_the_event_stream` in
/// `acp_integration/agent_handshake_tests.rs` pins the `-32601` fallback on
/// the same handle path.
#[tokio::test]
async fn a_resume_refused_with_a_normal_error_fails_the_connect() {
    let workspace = TempDir::new().expect("temp workspace");
    let outcome = connect_resuming(
        workspace.path(),
        Resume::Reject,
        "a-session-the-agent-forgot",
    )
    .await;

    let error = outcome
        .err()
        .expect("a refused resume must not silently become a new session");
    assert!(
        error.to_string().contains("session/resume"),
        "the failure must name the method that refused, got: {error}"
    );
}

/// `-32002` "Resource not found" is what claude-agent-acp and codex-acp
/// answer for a stored session that never persisted a turn — and crucible
/// stores the ACP id at connect, BEFORE the first turn. Without the
/// fallback, a session opened and daemon-restarted before its first chat
/// bricks on every reconnect. The id is unrestoreable, so the connect
/// opens a fresh session instead, the same way `-32601` does.
#[tokio::test]
async fn a_resume_answered_resource_not_found_falls_back_to_a_new_session() {
    let workspace = TempDir::new().expect("temp workspace");
    let handle = connect_resuming(
        workspace.path(),
        Resume::Unknown,
        "a-session-the-agent-forgot",
    )
    .await
    .expect("a -32002 reply to session/resume falls back to session/new");

    let id = handle
        .acp_session_id()
        .expect("the fallback session still has an id");
    assert_ne!(
        id, "a-session-the-agent-forgot",
        "the fallback must open a NEW session, not pretend the old one resumed"
    );
}
