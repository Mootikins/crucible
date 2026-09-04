//! An ACP turn driven through the daemon's RPC surface, end to end.
//!
//! The pieces existed and were never joined. `acp_smoke.rs` builds an
//! `AgentManager` in-process; `turn_event_parity.rs` drives an
//! `AcpAgentHandle` directly; `cli_e2e_acp.rs` shells out to `cru` and reads
//! only the exit status. None of them puts a *client* on the daemon's socket
//! and watches an ACP turn come back as session events, which is what every
//! real front end — the TUI, the web UI, another editor — actually does.
//!
//! What this crosses that nothing else does: a `cru daemon serve` process,
//! the JSON-RPC socket, `session.create` with `agent_type: "acp"`,
//! `session.configure_agent` naming a spawned agent binary, the ACP handshake
//! against a second process, the turn stream, and the broadcast events on the
//! way back out. Four processes' worth of wiring, asserted on the values that
//! arrive rather than on an exit code.
//!
//! The agent is `mock-acp-agent` rather than a real one so the turn is
//! deterministic and costs nothing; the real binaries are covered by
//! `acp_real_agents.rs`.

mod common;

use std::collections::HashSet;
use std::time::Duration;

use common::TestDaemon;
use crucible_core::session::SessionAgent;
use crucible_daemon::rpc_client::{DaemonClient, SessionCreateParams};
use crucible_daemon::SessionEvent;

#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent_bin::{mock_agent_path, mock_session_agent};

/// What the mock is told to stream, and therefore what the daemon must
/// deliver to a subscriber verbatim.
const MOCK_ANSWER: &str = "the daemon relayed this";

/// Long enough for a cold daemon to spawn the agent and finish a turn.
const TURN_TIMEOUT: Duration = Duration::from_secs(60);

/// A `SessionAgent` that points the daemon at the mock binary and scripts
/// the turn through the mock's env hooks.
fn acp_agent_streaming(answer: &str) -> SessionAgent {
    let mut agent = mock_session_agent(&mock_agent_path().to_string_lossy());
    agent
        .env_overrides
        .insert("CRU_MOCK_STREAM_CHUNKS".to_string(), answer.to_string());
    agent
}

/// Drain session events until `wanted` all arrive or the deadline passes.
///
/// Returns the events in arrival order so the caller can assert on ordering
/// as well as presence.
async fn collect_until(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<SessionEvent>,
    session_id: &str,
    wanted: &HashSet<&str>,
    timeout: Duration,
) -> Vec<SessionEvent> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut seen = Vec::new();
    let mut still_wanted: HashSet<&str> = wanted.clone();

    while !still_wanted.is_empty() {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(event)) => {
                if event.session_id != session_id {
                    continue;
                }
                still_wanted.remove(event.event.as_str());
                seen.push(event);
            }
            // Channel closed, or the deadline passed mid-wait.
            Ok(None) | Err(_) => break,
        }
    }

    seen
}

#[tokio::test]
#[ignore = "requires: cru binary, mock-acp-agent — spawns a daemon that spawns the agent"]
async fn an_acp_turn_reaches_a_socket_subscriber_as_session_events() {
    let daemon = TestDaemon::start().await.expect("start the test daemon");

    let (client, mut events) = DaemonClient::connect_to_with_events(&daemon.socket_path)
        .await
        .expect("connect to the daemon socket");

    // Subscribe before the session exists: the setup task fires as soon as
    // `session.create` returns.
    client
        .session_subscribe(&["*"])
        .await
        .expect("subscribe to all sessions");

    let created = client
        .session_create(SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crucible_daemon::test_support::kiln_name(TestDaemon::KILN)],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: Some("acp".to_string()),
            isolation: None,
        })
        .await
        .expect("create an ACP session over RPC");

    let session_id = created["session_id"]
        .as_str()
        .expect("session.create returns a session_id")
        .to_string();

    client
        .session_configure_agent(&session_id, &acp_agent_streaming(MOCK_ANSWER))
        .await
        .expect("point the session's ACP agent at the mock binary");

    client
        .session_send_message(&session_id, "say the line", true)
        .await
        .expect("send a message over RPC");

    let wanted: HashSet<&str> = ["text_delta", "message_complete"].into_iter().collect();
    let seen = collect_until(&mut events, &session_id, &wanted, TURN_TIMEOUT).await;

    let names: Vec<&str> = seen.iter().map(|e| e.event.as_str()).collect();
    assert!(
        names.contains(&"text_delta"),
        "the agent's text must reach a socket subscriber; got {names:?}"
    );
    assert!(
        names.contains(&"message_complete"),
        "the turn must be announced as complete; got {names:?}"
    );

    // The point of the whole chain: the bytes the agent process wrote arrive
    // at the subscriber unchanged.
    let relayed: String = seen
        .iter()
        .filter(|e| e.event == "text_delta")
        .filter_map(|e| e.data.get("content").and_then(|c| c.as_str()))
        .collect();
    assert_eq!(
        relayed.trim(),
        MOCK_ANSWER,
        "the daemon must relay the agent's text verbatim; got {relayed:?}"
    );

    // Ordering: no completion before the content it completes.
    let first_complete = names.iter().position(|n| *n == "message_complete");
    let first_text = names.iter().position(|n| *n == "text_delta");
    assert!(
        first_text < first_complete,
        "text must arrive before the turn completes; got {names:?}"
    );

    client
        .session_end(&session_id)
        .await
        .expect("end the session over RPC");
}

#[tokio::test]
#[ignore = "requires: cru binary, mock-acp-agent — spawns a daemon that spawns the agent"]
async fn an_acp_session_survives_a_second_turn_on_the_same_agent() {
    // Every other spawned-agent test runs exactly one turn, so nothing proves
    // the client is returned to its mutex and reusable. A handle that leaked
    // it would fail the second turn with `AgentUnavailable`.
    let daemon = TestDaemon::start().await.expect("start the test daemon");

    let (client, mut events) = DaemonClient::connect_to_with_events(&daemon.socket_path)
        .await
        .expect("connect to the daemon socket");
    client
        .session_subscribe(&["*"])
        .await
        .expect("subscribe to all sessions");

    let created = client
        .session_create(SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crucible_daemon::test_support::kiln_name(TestDaemon::KILN)],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: Some("acp".to_string()),
            isolation: None,
        })
        .await
        .expect("create an ACP session over RPC");
    let session_id = created["session_id"]
        .as_str()
        .expect("session.create returns a session_id")
        .to_string();

    client
        .session_configure_agent(&session_id, &acp_agent_streaming(MOCK_ANSWER))
        .await
        .expect("point the session's ACP agent at the mock binary");

    for turn in 1..=2 {
        client
            .session_send_message(&session_id, "say the line", true)
            .await
            .unwrap_or_else(|e| panic!("turn {turn} failed to send: {e}"));

        let wanted: HashSet<&str> = ["message_complete"].into_iter().collect();
        let seen = collect_until(&mut events, &session_id, &wanted, TURN_TIMEOUT).await;
        let names: Vec<&str> = seen.iter().map(|e| e.event.as_str()).collect();

        assert!(
            names.contains(&"message_complete"),
            "turn {turn} never completed; got {names:?}"
        );
        assert!(
            !names.contains(&"error"),
            "turn {turn} reported an error; got {names:?}"
        );
    }

    client
        .session_end(&session_id)
        .await
        .expect("end the session over RPC");
}
