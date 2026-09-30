//! The session history does not depend on the client broadcast.
//!
//! The broadcast ring drops the oldest events for a receiver that falls
//! behind. A client can recover from that, because the stream carries a gap
//! marker and the history is on disk. The history itself cannot recover, so
//! the writer of `session.jsonl` must not read the ring.

use super::*;

/// The `model_id` of each `model_switched` line in `session_id`'s log, in
/// file order.
fn stored_models(sessions_root: &Path, session_id: &str) -> Vec<String> {
    let log = sessions_root.join(session_id).join("session.jsonl");
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|event| event["event"] == "model_switched")
        .filter_map(|event| event["data"]["model_id"].as_str().map(str::to_string))
        .collect()
}

/// A burst that overruns the broadcast ring still reaches the log whole and
/// in order.
///
/// The burst is synchronous on a current-thread runtime, so no receiver can
/// read during it. A receiver of the ring then sees `Lagged` and the oldest
/// events are gone. The log must hold all of them.
///
/// The wait for the persist task is `SessionManager::settle_history`, called
/// directly on the daemon's own handle rather than over the client's RPC
/// socket. That socket also carries this connection's own live event
/// forwarder, and the burst above overruns the broadcast ring for it too —
/// its `Lagged` branch writes a `stream_gap` line straight onto the socket.
/// A naive single-line read of an RPC reply on that same socket can then
/// read the gap marker instead of the intended reply, well before the
/// persist task is done. A fixed poll deadline has the same
/// flaw from the other side: on a loaded machine the persist task can take
/// longer than the deadline to drain a burst this size, and the test would
/// report the burst as lost merely because the write was still in flight.
#[tokio::test]
async fn a_burst_past_the_broadcast_ring_is_stored_whole_and_in_order() {
    const BURST: usize = EVENT_CHANNEL_CAPACITY + 256;
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, TestServer::KILN, 1).await;

    let expected: Vec<String> = (0..BURST).map(|i| format!("model-{i}")).collect();
    for model in &expected {
        server.event_tx.emit(SessionEventMessage::model_switched(
            &session_id,
            model.clone(),
            "mock",
        ));
    }

    server.session_manager.settle_history().await;
    let stored = stored_models(&server.sessions_root(), &session_id);
    server.shutdown().await;

    assert_eq!(
        stored.len(),
        BURST,
        "the log lost {} of {BURST} events that overran the broadcast ring",
        BURST.saturating_sub(stored.len())
    );
    assert!(
        stored == expected,
        "the log must keep the order in which the events were sent"
    );
}

/// A client that reads the history right after the events reads all of them.
///
/// `session.events_after` is the reconnect path of the web chat stream. A
/// client that saw an event live and then asks for the tail must find it on
/// disk, or it loses the event in the reconnect.
#[tokio::test]
async fn a_history_read_after_an_event_sees_the_event() {
    const SENT: usize = 2000;
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, TestServer::KILN, 1).await;

    for i in 0..SENT {
        server.event_tx.emit(SessionEventMessage::model_switched(
            &session_id,
            format!("model-{i}"),
            "mock",
        ));
    }

    let tail = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "session.events_after",
            "params": { "session_id": session_id, "after": 0 }
        }),
    )
    .await;
    server.shutdown().await;

    let switches = tail["result"]
        .as_array()
        .unwrap_or_else(|| panic!("session.events_after failed: {tail}"))
        .iter()
        .filter(|event| event["event"] == "model_switched")
        .count();
    assert_eq!(
        switches, SENT,
        "a history read that follows the events must see every one of them"
    );
}
