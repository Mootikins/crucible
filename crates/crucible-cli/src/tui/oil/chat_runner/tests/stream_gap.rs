//! The client half of the daemon's broadcast gap marker.
//!
//! The daemon's event forwarder turns a `broadcast` `Lagged(n)` into a
//! `stream_gap` event written straight to that client's socket
//! (`daemon/src/server/core.rs`). A marker nothing renders is not a marker, so
//! these pin the two things that have to be true for the TUI to show it: the
//! wildcard-addressed event has to survive the session filter, and it has to
//! translate into something the user sees.

use super::super::*;
use tokio::sync::mpsc;

/// Await one message with a deadline. The deadline is not a timing assumption —
/// the consumer forwards synchronously — it just turns "the marker was dropped"
/// into a fast failure instead of a hung test.
async fn recv_within(rx: &mut mpsc::UnboundedReceiver<ChatAppMsg>) -> ChatAppMsg {
    tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .expect("the consumer must forward the marker, not drop it")
        .expect("channel closed without a message")
}

/// The gap has to reach the user. It is the only signal that the transcript
/// on screen is missing events, and it is not recoverable later.
#[test]
fn a_stream_gap_becomes_a_visible_warning_naming_the_count() {
    let msgs = session_event_to_chat_msgs("stream_gap", &serde_json::json!({ "dropped": 12 }));

    assert_eq!(msgs.len(), 1, "expected exactly one message: {msgs:?}");
    match &msgs[0] {
        ChatAppMsg::Error(text) => {
            assert!(
                text.contains("12"),
                "the count is the actionable part — how much is missing: {text:?}"
            );
        }
        other => panic!("expected a surfaced warning, got {other:?}"),
    }
}

/// A malformed marker must still warn. `dropped` is the daemon's own field, so a
/// missing one means a version skew, and skew is not a reason to hide data loss.
#[test]
fn a_stream_gap_without_a_count_still_warns() {
    let msgs = session_event_to_chat_msgs("stream_gap", &serde_json::json!({}));
    assert!(
        matches!(msgs.as_slice(), [ChatAppMsg::Error(_)]),
        "expected a warning even with no count: {msgs:?}"
    );
}

/// `Lagged(n)` names no session, so the marker is addressed to the wildcard —
/// and the consumer's filter used to drop anything whose `session_id` was not an
/// exact match, which would have deleted the marker on arrival.
///
/// The same filter gap silently discards every other wildcard-addressed event,
/// `ui_style_changed` included; the daemon's forwarder has treated the wildcard
/// as symmetric since it was written (`server/core.rs`) and only the client half
/// was missing.
#[tokio::test]
async fn a_wildcard_addressed_event_is_not_filtered_out_of_a_session() {
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    let consumer = tokio::spawn(session_event_consumer(
        "my-session".to_string(),
        event_rx,
        msg_tx,
        None,
    ));

    event_tx
        .send(crucible_daemon::SessionEvent {
            session_id: "*".to_string(),
            event_type: "stream_gap".to_string(),
            data: serde_json::json!({ "dropped": 7 }),
        })
        .unwrap();

    // Awaited, not slept on: the consumer sends as soon as it translates. The
    // deadline only bounds a regression — a dropped marker would otherwise hang.
    let msg = recv_within(&mut msg_rx).await;
    match msg {
        ChatAppMsg::Error(text) => assert!(text.contains('7'), "{text:?}"),
        other => panic!("expected a warning, got {other:?}"),
    }

    drop(event_tx);
    consumer.await.expect("consumer task");
}

/// The filter still has a job: another *session's* events are not this
/// session's, and admitting the wildcard must not have widened it to everything.
#[tokio::test]
async fn another_sessions_events_are_still_filtered_out() {
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    let consumer = tokio::spawn(session_event_consumer(
        "my-session".to_string(),
        event_rx,
        msg_tx,
        None,
    ));

    for session_id in ["someone-elses-session", "*"] {
        event_tx
            .send(crucible_daemon::SessionEvent {
                session_id: session_id.to_string(),
                event_type: "stream_gap".to_string(),
                data: serde_json::json!({ "dropped": 1 }),
            })
            .unwrap();
    }

    // The wildcard one is sent second, so receiving exactly one message proves
    // the first was dropped — no timeout needed to establish the absence.
    let first = recv_within(&mut msg_rx).await;
    assert!(matches!(first, ChatAppMsg::Error(_)), "{first:?}");

    drop(event_tx);
    consumer.await.expect("consumer task");
    assert!(
        msg_rx.recv().await.is_none(),
        "the foreign session's marker must not have been rendered"
    );
}
