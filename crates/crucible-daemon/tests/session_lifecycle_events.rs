//! `session:created` and `session:ended` reach the daemon's broadcast bus.
//!
//! These two events are daemon-wide: they are addressed to the system session,
//! not to the session they report on, because the audience is a client or
//! plugin watching *every* session. A session list surface is the motivating
//! case, and it is by definition not attached to the session that just started.
//!
//! `event_map`'s own tests prove the other half of the chain — that a message
//! built by `event_map::session_created` decodes to the `session:created` hook
//! and presents that name to a Lua handler. This file proves the emission: that
//! `session.create` and `session.end` actually put one on the bus. Neither test
//! is redundant, and the gap between them is where `webhook:received` lived for
//! its whole life, broadcast to nobody.

mod common;

use crucible_core::protocol::requests::SessionCreateRequest;
use crucible_core::protocol::RpcMethod;
use crucible_daemon::{DaemonClient, SessionEvent};
use std::time::Duration;
use tokio::sync::mpsc;

/// The session id every daemon-wide event is addressed to.
const SYSTEM_SESSION: &str = "system";

async fn start_server() -> common::InProcessDaemon {
    common::InProcessDaemonBuilder::new()
        .expect("a test daemon builder")
        .with_ready_timeout(std::time::Duration::from_secs(60))
        .with_kiln("kiln")
        .start()
        .await
        .expect("Failed to start server")
}

/// Wait for one named event, returning it, or `None` once the deadline passes.
///
/// Polls for a condition rather than draining a fixed count: the bus carries
/// every setup event too, and their order against this one is not a contract.
async fn wait_for_event(
    event_rx: &mut mpsc::UnboundedReceiver<SessionEvent>,
    name: &str,
    timeout: Duration,
) -> Option<SessionEvent> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return None;
        }
        match tokio::time::timeout(deadline - now, event_rx.recv()).await {
            Ok(Some(ev)) if ev.event == name => return Some(ev),
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => return None,
        }
    }
}

async fn create_session(client: &DaemonClient) -> String {
    let resp = client
        .session_create(SessionCreateRequest {
            session_type: "chat".to_string(),
            kilns: SessionCreateRequest::kiln_set(vec![crucible_daemon::test_support::kiln_name(
                "kiln",
            )]),
            agent_type: Some("internal".to_string()),
            ..Default::default()
        })
        .await
        .expect("session_create failed");
    resp.id.to_string()
}

#[tokio::test]
async fn session_create_emits_session_created_daemon_wide() {
    let server = start_server().await;
    let (client, mut event_rx) = DaemonClient::connect_to_with_events(server.socket_path())
        .await
        .expect("connect with events");
    client
        .session_subscribe(&["*"])
        .await
        .expect("pre-subscribe");

    let session_id = create_session(&client).await;

    let ev = wait_for_event(&mut event_rx, "session:created", Duration::from_secs(10))
        .await
        .expect("`session:created` reaches the bus");

    assert_eq!(
        ev.session_id, SYSTEM_SESSION,
        "a daemon-wide event is addressed to the system session, not to the new one"
    );
    assert_eq!(
        ev.data.get("session_id").and_then(|v| v.as_str()),
        Some(session_id.as_str()),
        "the payload names the session that was created"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn session_end_emits_session_ended_daemon_wide() {
    let server = start_server().await;
    let (client, mut event_rx) = DaemonClient::connect_to_with_events(server.socket_path())
        .await
        .expect("connect with events");
    client
        .session_subscribe(&["*"])
        .await
        .expect("pre-subscribe");

    let session_id = create_session(&client).await;
    client
        .session_end(&session_id)
        .await
        .expect("session_end failed");

    let ev = wait_for_event(&mut event_rx, "session:ended", Duration::from_secs(10))
        .await
        .expect("`session:ended` reaches the bus");

    assert_eq!(ev.session_id, SYSTEM_SESSION);
    assert_eq!(
        ev.data.get("session_id").and_then(|v| v.as_str()),
        Some(session_id.as_str())
    );

    server.shutdown().await;
}

/// A fork is a session, so it owes the same event.
///
/// Its own test because the fork path reads a different key: `session_id` in a
/// fork request names the PARENT, and the fork reports its own id as `id`. A
/// fork emitting the parent's id would look correct on the bus and put a
/// duplicate row in every session list.
#[tokio::test]
async fn session_fork_emits_session_created_for_the_fork() {
    let server = start_server().await;
    let (client, mut event_rx) = DaemonClient::connect_to_with_events(server.socket_path())
        .await
        .expect("connect with events");
    client
        .session_subscribe(&["*"])
        .await
        .expect("pre-subscribe");

    let parent_id = create_session(&client).await;
    let _ = wait_for_event(&mut event_rx, "session:created", Duration::from_secs(10))
        .await
        .expect("the parent's own event");

    let forked: serde_json::Value = client
        .call(
            RpcMethod::SessionFork,
            serde_json::json!({ "session_id": parent_id }),
        )
        .await
        .expect("session.fork failed");
    let fork_id = forked["id"]
        .as_str()
        .expect("a fork reports its id as `id`")
        .to_string();

    let ev = wait_for_event(&mut event_rx, "session:created", Duration::from_secs(10))
        .await
        .expect("`session:created` reaches the bus for a fork");

    assert_eq!(ev.session_id, SYSTEM_SESSION);
    assert_eq!(
        ev.data.get("session_id").and_then(|v| v.as_str()),
        Some(fork_id.as_str()),
        "the fork reports its own id, not the parent's"
    );
    assert_ne!(fork_id, parent_id, "a fork is a different session");

    server.shutdown().await;
}
