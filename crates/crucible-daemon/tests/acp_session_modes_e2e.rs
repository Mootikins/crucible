//! The mode set a front end is offered for an ACP session.
//!
//! `AgentHandle::get_modes()` reports the modes an ACP agent declares, and
//! `acp_integration/session_modes.rs` proves it does. Nothing read it.
//! `session.list_modes` — the one call the TUI and the web both make —
//! answered from the Lua registry, so an ACP session offered Crucible's
//! `ask`/`plan`/`auto` while the agent was in `default` and would reject all
//! three. The handle's answer was correct and unreachable.
//!
//! The modes belong to the agent, and they exist only after the handshake,
//! so the handle build caches them on the session's slot and `session_modes`
//! prefers them. A list fetched before the handshake finishes holds the
//! wrong one, which is why the build also emits `mode_changed` — knob reads
//! bring the handshake up themselves, but a front end that raced it still
//! re-fetches on a mode id it does not recognise.
//!
//! These tests drive `AgentManager` against a real `mock-acp-agent` process,
//! so the mode ids under assertion are ones that crossed the ACP wire.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::AgentManager;
use tempfile::TempDir;
use tokio::sync::broadcast;

#[path = "acp_support/mock_agent.rs"]
mod mock_agent;
#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent::{logged, MockScript, Step};
use mock_agent_bin::{completed_turn, mock_session, MockSession};

const TURN_TIMEOUT: Duration = Duration::from_secs(60);

/// What the mock agent streams on every turn.
const ANSWER: &str = "acknowledged";

/// The ids the mock declares when told to advertise modes. None of them is a
/// Crucible mode, which is the point: an assertion on them cannot pass by
/// accident against the Lua registry.
const AGENT_MODE_IDS: [&str; 3] = ["default", "acceptEdits", "plan"];

/// The mode the mock reports as current.
const AGENT_CURRENT_MODE: &str = "acceptEdits";

/// A script for the mock agent. `modes` names the current mode the agent
/// declares, or `None` for an agent that declares none.
fn script(modes: Option<&str>) -> MockScript {
    MockScript {
        mode: modes.map(str::to_string),
        turn: vec![Step::Text(ANSWER.to_string())],
        ..MockScript::default()
    }
}

/// The mode id of the last `session/set_mode` that the agent received.
fn last_set_mode(log: &Path) -> String {
    logged(log, "session/set_mode")
        .last()
        .expect("the agent process must have received a session/set_mode")["modeId"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

struct Harness {
    _temp: TempDir,
    /// The frame log of the agent process.
    log: PathBuf,
    agent_manager: Arc<AgentManager>,
    session_id: crucible_core::session::SessionId,
    event_tx: broadcast::Sender<SessionEventMessage>,
    events: broadcast::Receiver<SessionEventMessage>,
}

async fn setup(modes: Option<&str>) -> Harness {
    setup_with_script(script(modes)).await
}

/// `setup` for a script that the caller builds. The harness sets its log.
async fn setup_with_script(script: MockScript) -> Harness {
    let temp = TempDir::new().expect("temp dir");
    let kiln = temp.path().join("kiln");
    std::fs::create_dir_all(&kiln).expect("kiln dir");
    let log = temp.path().join("frames.jsonl");
    let script = MockScript {
        log: Some(log.clone()),
        ..script
    };
    let MockSession {
        agent_manager,
        session_id,
        event_tx,
        events,
    } = mock_session(&[("kiln", &kiln)], script).await;

    Harness {
        _temp: temp,
        log,
        agent_manager,
        session_id,
        event_tx,
        events,
    }
}

/// Run one turn, which is what builds the handle and so what learns the
/// agent's modes.
async fn run_a_turn(h: &Harness) {
    let (_id, done) = h
        .agent_manager
        .send_message_notified(&h.session_id, "hello".to_string(), &h.event_tx, true, None)
        .await
        .expect("the turn is accepted");
    let outcome = completed_turn(done, TURN_TIMEOUT).await;
    assert_eq!(outcome.final_text.trim(), ANSWER, "the agent's answer");
}

fn mode_ids(state: &crucible_core::types::acp::schema::SessionModeState) -> Vec<String> {
    state
        .available_modes
        .iter()
        .map(|m| m.id.0.as_ref().to_string())
        .collect()
}

/// The headline: what `session.list_modes` answers for an ACP session is the
/// agent's mode set, and it is the agent's current mode that the session
/// reports being in.
#[tokio::test]
async fn an_acp_sessions_modes_become_the_agents_once_the_handle_exists() {
    let h = setup(Some(AGENT_CURRENT_MODE)).await;

    // Before any turn there is no agent process and so no agent mode set.
    // Crucible's own modes stand in, which is the honest answer: nothing has
    // asked the agent yet.
    let before = h.agent_manager.session_modes(h.session_id.as_str());
    assert!(
        !mode_ids(&before).contains(&"acceptEdits".to_string()),
        "before the handshake the agent's modes cannot be known; got {:?}",
        mode_ids(&before)
    );

    run_a_turn(&h).await;

    let after = h.agent_manager.session_modes(h.session_id.as_str());
    assert_eq!(
        mode_ids(&after),
        AGENT_MODE_IDS,
        "the session must offer the modes the agent declared, not Crucible's own"
    );
    assert_eq!(
        after.current_mode_id.0.as_ref(),
        AGENT_CURRENT_MODE,
        "the session must report the mode the agent is actually in"
    );
}

/// An ACP agent that declares no modes leaves the session on Crucible's set.
/// Without this the change would read as "ACP sessions have no modes" rather
/// than "ACP sessions have the agent's modes when it has any".
#[tokio::test]
async fn an_acp_agent_that_declares_no_modes_leaves_the_session_set_alone() {
    let h = setup(None).await;
    let before = mode_ids(&h.agent_manager.session_modes(h.session_id.as_str()));

    run_a_turn(&h).await;

    let after = mode_ids(&h.agent_manager.session_modes(h.session_id.as_str()));
    assert_eq!(
        after, before,
        "an agent with no modes must not change what the session offers"
    );
    assert!(
        after.iter().any(|id| id == "ask"),
        "the fallback set is Crucible's own; got {after:?}"
    );
}

/// The consequence a user meets: a mode only the agent declares can be
/// switched to. `set_mode` validates against `session_modes`, so before the
/// handshake the id is unknown and after it the switch is accepted and
/// forwarded to the agent as `session/set_mode`.
#[tokio::test]
async fn set_mode_accepts_a_mode_that_only_the_agent_declares() {
    let h = setup(Some(AGENT_CURRENT_MODE)).await;

    // `acceptEdits` is in no Crucible set, so it is the id that separates the
    // two. (`plan` appears in both and would prove nothing.)
    let before = h
        .agent_manager
        .set_mode(h.session_id.as_str(), "acceptEdits", None)
        .await
        .expect_err("a mode no one has declared yet must be rejected");
    assert!(before.to_string().contains("unknown mode"), "got: {before}");

    run_a_turn(&h).await;

    h.agent_manager
        .set_mode(h.session_id.as_str(), "acceptEdits", None)
        .await
        .expect("once the agent has declared it, the mode is switchable");

    // Accepting it is half the job; the agent has to be told. Without the
    // forward the daemon and the agent hold different modes and only the
    // agent's decides what the next turn may do.
    assert_eq!(
        last_set_mode(&h.log),
        "acceptEdits",
        "the switch must reach the agent over the ACP wire"
    );

    // Now switch AWAY from the mode the agent declared as current. Switching
    // to `acceptEdits` above proves acceptance but not reporting: it is the
    // agent's own starting mode, so a `session_modes` that ignored the switch
    // entirely would still answer it.
    h.agent_manager
        .set_mode(h.session_id.as_str(), "plan", None)
        .await
        .expect("`plan` is in the agent's set too");

    // The cached mode set is a snapshot taken at the handshake, so its own
    // `current_mode_id` still says `acceptEdits`. What keeps this right is the
    // persisted mode, which `set_mode` wrote and `session_modes` prefers
    // whenever the agent offers it.
    let state = h.agent_manager.session_modes(h.session_id.as_str());
    assert_eq!(
        state.current_mode_id.0.as_ref(),
        "plan",
        "a front end must see the mode the session switched to, not the one \
         the agent declared at the handshake"
    );
    assert_eq!(
        mode_ids(&state),
        AGENT_MODE_IDS,
        "the offered set is unchanged by a switch within it"
    );
}

/// A front end that fetched the mode list before the handshake finished is
/// holding the wrong one. `mode_changed` is what tells it: both front ends
/// re-fetch the list on a mode id they do not recognise, so without this
/// event the mode chip offers three modes the agent rejects until the page
/// is reloaded.
#[tokio::test]
async fn the_agents_mode_set_is_announced_so_a_front_end_can_refetch() {
    let mut h = setup(Some(AGENT_CURRENT_MODE)).await;

    run_a_turn(&h).await;

    let mut announced = None;
    while let Ok(event) = h.events.try_recv() {
        if event.event == "mode_changed" {
            announced = event
                .data
                .get("mode")
                .and_then(|m| m.as_str())
                .map(str::to_string);
        }
    }

    assert_eq!(
        announced.as_deref(),
        Some(AGENT_CURRENT_MODE),
        "the handshake must announce the mode the agent is in"
    );
}

/// The mode set survives an eviction of the handle.
///
/// A model switch and a scope change both invalidate the cached handle, and
/// the mode set used to be cleared with it. That left an ACP session
/// offering Crucible's `ask`/`plan`/`auto` — and rejecting the agent's own
/// ids — until the next message rebuilt the handle. Neither change alters
/// which agent the session runs, so neither can alter which modes it has.
///
/// The eviction is called directly rather than through a setting, because a
/// setting is no longer allowed to evict an ACP handle at all (that would
/// kill the agent process). The claim under test is about eviction itself.
#[tokio::test]
async fn an_eviction_does_not_take_the_agents_modes_away() {
    let h = setup(Some(AGENT_CURRENT_MODE)).await;
    run_a_turn(&h).await;

    h.agent_manager
        .invalidate_agent_cache(h.session_id.as_str());

    assert_eq!(
        mode_ids(&h.agent_manager.session_modes(h.session_id.as_str())),
        AGENT_MODE_IDS,
        "the agent's modes must outlive an eviction that keeps the agent"
    );
    h.agent_manager
        .set_mode(h.session_id.as_str(), "acceptEdits", None)
        .await
        .expect("and the agent's own modes stay switchable");
}

/// An agent may name a current mode it does not offer. Reporting that id
/// leaves a front end with a mode chip it cannot render and a cycle that
/// goes nowhere, so the session falls back to the first mode the agent does
/// offer — the same rule the Lua path applies to a mode that is no longer
/// declared.
#[tokio::test]
async fn a_current_mode_the_agent_does_not_offer_falls_back_to_one_it_does() {
    // The script's `mode` names the current mode without adding it to
    // the declared list, which is exactly the malformed shape.
    let h = setup(Some("a-mode-not-in-the-list")).await;
    run_a_turn(&h).await;

    let state = h.agent_manager.session_modes(h.session_id.as_str());
    assert_eq!(mode_ids(&state), AGENT_MODE_IDS);
    assert_eq!(
        state.current_mode_id.0.as_ref(),
        "default",
        "the reported current mode must be one the agent actually offers"
    );
}

/// A Crucible rename alias must not shadow an agent's own mode of the same
/// name. `normal` was Crucible's old name for `ask`, so `set_mode("normal")`
/// resolves to `ask` — but an ACP agent that declares a mode called `normal`
/// means its own, and the session offers exactly what the agent declared.
#[tokio::test]
async fn an_agents_own_id_beats_a_crucible_rename_alias() {
    let h = setup_with_script(MockScript {
        mode_ids: Some(vec!["normal".to_string(), "strict".to_string()]),
        ..script(Some("normal"))
    })
    .await;
    run_a_turn(&h).await;

    h.agent_manager
        .set_mode(h.session_id.as_str(), "normal", None)
        .await
        .expect("the agent's own `normal` must be selectable");

    assert_eq!(
        last_set_mode(&h.log),
        "normal",
        "the agent must be told `normal`, not the mode Crucible renames it to"
    );
}
