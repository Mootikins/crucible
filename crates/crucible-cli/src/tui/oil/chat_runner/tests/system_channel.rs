//! The daemon's system session reaches the TUI.
//!
//! A proposal belongs to no user session, so the daemon sends
//! `proposal_changed` on the system session (`"system"`). The TUI stream
//! filter kept only the own session and the wildcard, so it dropped each
//! system event. These tests hold the two halves: the filter lets a system
//! event through, and the translation makes a message of it.

use super::super::*;
use tokio::sync::mpsc;

const PROPOSAL_ID: &str = "0b8f4a0e-7c1d-4c55-9a39-5d1f0a2e6b11";

#[test]
fn a_proposal_changed_event_reaches_the_app() {
    let msgs = session_event_to_chat_msgs(
        crucible_core::protocol::SystemPayload::PROPOSAL_CHANGED,
        &serde_json::json!({ "id": PROPOSAL_ID }),
    );

    match msgs.as_slice() {
        [ChatAppMsg::ProposalChanged(id)] => assert_eq!(id.to_string(), PROPOSAL_ID),
        other => panic!("expected one proposal change, got {other:?}"),
    }
}

#[tokio::test]
async fn a_system_event_passes_the_filter() {
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let consumer = tokio::spawn(session_event_consumer(
        "my-session".to_string(),
        event_rx,
        msg_tx,
        None,
    ));

    event_tx
        .send(crucible_daemon::SessionEvent::new(
            crucible_daemon::event_map::SYSTEM_SESSION.to_string(),
            crucible_core::protocol::SystemPayload::PROPOSAL_CHANGED.to_string(),
            serde_json::json!({ "id": PROPOSAL_ID }),
        ))
        .unwrap();
    drop(event_tx);
    consumer.await.expect("consumer task");

    match msg_rx.recv().await {
        Some(ChatAppMsg::ProposalChanged(id)) => assert_eq!(id.to_string(), PROPOSAL_ID),
        other => panic!("the filter dropped the system event: {other:?}"),
    }
}
