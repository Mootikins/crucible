//! The permission gate of an ACP session, driven through `AgentManager`.
//!
//! A mock agent process asks `session/request_permission` during a turn.
//! The daemon loads the Lua defaults, so the Crucible modes `auto` and
//! `plan` exist. These tests assert on the prompt that the daemon emits.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crucible_core::interaction::PermResponse;
use crucible_core::session::{SessionId, SessionType};
use crucible_daemon::daemon_plugins::DaemonPluginLoader;
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::test_support::temp_session_manager;
use crucible_daemon::AgentManager;
use serde_json::json;
use tempfile::TempDir;
use tokio::sync::broadcast;

#[path = "acp_support/mock_agent.rs"]
mod mock_agent;
#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent::{logged, MockScript, Step};
use mock_agent_bin::{
    acp_manager_params, completed_turn, mock_profile, profile_session_agent, MOCK_PROFILE,
};

const TURN_TIMEOUT: Duration = Duration::from_secs(60);

/// A session whose ACP agent asks about one edit in each turn.
struct Gate {
    _temp: TempDir,
    _loader: DaemonPluginLoader,
    log: PathBuf,
    am: Arc<AgentManager>,
    session_id: SessionId,
    event_tx: broadcast::Sender<SessionEventMessage>,
    events: broadcast::Receiver<SessionEventMessage>,
    /// The text that the agent streamed, over every turn.
    text: String,
}

/// The agent reports `mode` as its own current mode.
async fn gate(mode: &str) -> Gate {
    let temp = TempDir::new().expect("temp dir");
    let log = temp.path().join("agent.log");
    let script = MockScript {
        mode: Some(mode.to_string()),
        mode_ids: Some(vec![mode.to_string()]),
        turn: vec![
            Step::Permission(json!({
                "toolCall": {
                    "toolCallId": "call-1",
                    "title": "Edit a.rs",
                    "kind": "edit",
                    "locations": [{ "path": "/w/a.rs" }],
                },
                "options": [
                    { "optionId": "allow_once", "name": "Allow", "kind": "allow_once" },
                    { "optionId": "reject_once", "name": "Reject", "kind": "reject_once" },
                ],
            })),
            Step::IfAllowed(Box::new(Step::Text("edited a.rs".to_string()))),
            Step::Text("done".to_string()),
        ],
        log: Some(log.clone()),
        ..MockScript::default()
    };
    let loader = DaemonPluginLoader::new(HashMap::new()).expect("daemon VM");
    loader
        .executor()
        .lua()
        .load(crucible_lua::BUILTIN_INIT_LUA)
        .exec()
        .expect("the Lua defaults load");
    let session_manager = temp_session_manager();
    let (event_tx, events) = broadcast::channel(256);
    let am = Arc::new(
        AgentManager::new(acp_manager_params(
            session_manager.clone(),
            BTreeMap::from([(
                MOCK_PROFILE.to_string(),
                mock_profile(BTreeMap::from([script.env()])),
            )]),
            &event_tx,
        ))
        .with_modes(Some(loader.mode_registry())),
    );
    am.set_plugin_handlers(loader.plugin_handlers(), loader.plugin_lua());
    let session = session_manager
        .create_session(SessionType::Chat, vec![], None, None)
        .await
        .expect("session");
    am.configure_agent(&session.id, profile_session_agent(MOCK_PROFILE))
        .await
        .expect("configure the agent");
    Gate {
        _temp: temp,
        _loader: loader,
        log,
        am,
        session_id: session.id,
        event_tx,
        events,
        text: String::new(),
    }
}

impl Gate {
    /// Run one turn. Answer each prompt with `answer`. Return true if the
    /// daemon asked the user before the turn finished.
    async fn turn(&mut self, is_interactive: bool, answer: PermResponse) -> bool {
        let (_id, done) = self
            .am
            .send_message_notified(
                &self.session_id,
                "go".to_string(),
                &self.event_tx,
                is_interactive,
                None,
            )
            .await
            .expect("the turn is accepted");
        let mut asked = false;
        loop {
            let msg = tokio::time::timeout(TURN_TIMEOUT, self.events.recv())
                .await
                .expect("the turn must finish")
                .expect("event channel open");
            match msg.event.as_str() {
                "interaction_requested" => {
                    asked = true;
                    let id = msg.data["request_id"].as_str().expect("request_id");
                    self.am
                        .respond_to_permission(&self.session_id, id, answer.clone())
                        .expect("the prompt is registered");
                }
                "text_delta" => self.text += msg.data["content"].as_str().unwrap_or(""),
                "turn_finished" => break,
                _ => {}
            }
        }
        completed_turn(done, TURN_TIMEOUT).await;
        asked
    }

    /// The option that the agent received for each of its questions.
    fn answers(&self) -> Vec<String> {
        logged(&self.log, "permission/answer")
            .iter()
            .map(|a| {
                a["result"]["outcome"]["optionId"]
                    .as_str()
                    .unwrap_or("")
                    .to_string()
            })
            .collect()
    }
}

/// Claude and codex name a mode `auto`. That id is the agent's, so the
/// Crucible `auto` stance (allow all) must not answer the question.
#[tokio::test]
async fn an_agent_mode_named_auto_does_not_take_the_crucible_auto_stance() {
    let mut gate = gate("auto").await;
    assert!(
        gate.turn(true, PermResponse::deny()).await,
        "the user must be asked"
    );
    assert_eq!(gate.answers(), ["reject_once"]);
}

/// Each turn start writes its interactivity for the permission handler of
/// the cached handle. An interactive turn asks the user. A later turn that
/// nobody can answer, on the same handle, refuses with no prompt.
#[tokio::test]
async fn each_turn_start_sets_the_gate_of_the_cached_handle() {
    let mut gate = gate("default").await;
    assert!(
        gate.turn(true, PermResponse::deny()).await,
        "the interactive turn asks the user"
    );
    assert!(
        !gate.turn(false, PermResponse::deny()).await,
        "the non-interactive turn asks nobody"
    );
    assert_eq!(gate.answers(), ["reject_once", "reject_once"]);
    let initializes = logged(&gate.log, "initialize").len();
    assert_eq!(initializes, 1, "both turns ran on one agent process");
}

/// The Crucible plan rule refuses each unsafe call. An agent mode named
/// `plan` is the agent's own rule, so Crucible asks the user.
#[tokio::test]
async fn an_agent_mode_named_plan_does_not_take_the_crucible_plan_rule() {
    let mut gate = gate("plan").await;
    assert!(
        gate.turn(true, PermResponse::deny()).await,
        "the user must be asked"
    );
}

/// The user allows the call once. The agent receives `allow_once` and
/// runs the call.
#[tokio::test]
async fn a_user_who_allows_once_sends_allow_once_to_the_agent() {
    let mut gate = gate("default").await;
    assert!(
        gate.turn(true, PermResponse::allow()).await,
        "the user must be asked"
    );
    assert_eq!(gate.answers(), ["allow_once"]);
    assert!(gate.text.contains("edited a.rs"), "{:?}", gate.text);
}

/// The user rejects the call. The agent receives `reject_once` and does
/// not run the call.
#[tokio::test]
async fn a_user_who_rejects_stops_the_call_of_the_agent() {
    let mut gate = gate("default").await;
    assert!(
        gate.turn(true, PermResponse::deny()).await,
        "the user must be asked"
    );
    assert_eq!(gate.answers(), ["reject_once"]);
    assert!(gate.text.contains("done"), "{:?}", gate.text);
    assert!(!gate.text.contains("edited a.rs"), "{:?}", gate.text);
}
