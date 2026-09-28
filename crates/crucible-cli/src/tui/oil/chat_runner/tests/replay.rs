use super::super::*;
use tokio::sync::mpsc;

/// The consumer forwards the transcript ops of an event before its other
/// messages, so the end of a turn that the same event carries seals the
/// segment that the ops just wrote. A delegation reaches the TUI this way:
/// it is a transcript item, not a message of its own.
#[tokio::test]
async fn the_consumer_forwards_the_ops_of_an_event_before_its_other_messages() {
    use serde_json::json;
    use tokio::time::{timeout, Duration};

    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let session = "test-session-ops".to_string();
    let consumer = tokio::spawn(session_event_consumer(
        session.clone(),
        event_rx,
        msg_tx,
        None,
    ));

    // The daemon's event bus folds each event and puts the ops on it.
    let mut fold = crucible_core::transcript::TranscriptFold::new();
    for (name, data) in [
        (
            "delegation_spawned",
            json!({ "delegation_id": "d1", "prompt": "test prompt", "target_agent": "opencode" }),
        ),
        (
            "message_complete",
            json!({ "message_id": "m1", "full_response": "done" }),
        ),
    ] {
        let mut event = crucible_daemon::SessionEvent::new(session.clone(), name, data);
        event.transcript = fold.apply(&event);
        event_tx.send(event).unwrap();
    }

    let mut msgs = Vec::new();
    for _ in 0..3 {
        msgs.push(
            timeout(Duration::from_secs(1), msg_rx.recv())
                .await
                .expect("a message in time")
                .expect("the channel is open"),
        );
    }
    let mut msgs = msgs.into_iter();
    match msgs.next().unwrap() {
        ChatAppMsg::Transcript { ops, .. } => {
            let mut transcript = crucible_core::transcript::Transcript::default();
            assert!(ops.iter().all(|op| transcript.apply(op)));
            assert!(matches!(
                &transcript.items[0].body,
                crucible_core::transcript::ItemBody::Delegation { delegation_id, .. }
                    if delegation_id == "d1"
            ));
        }
        other => panic!("expected the ops of the delegation, got {other:?}"),
    }
    assert!(matches!(msgs.next(), Some(ChatAppMsg::Transcript { .. })));
    assert!(matches!(msgs.next(), Some(ChatAppMsg::StreamComplete)));

    drop(event_tx);
    consumer.await.unwrap();
}
