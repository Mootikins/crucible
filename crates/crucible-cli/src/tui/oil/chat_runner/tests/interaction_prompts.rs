//! The live consumer opens each prompt of its session once.
//!
//! A prompt can reach the client twice: in the pending list that the client
//! reads when it attaches, and as the `interaction_requested` event of the
//! same prompt. Stored history and a replay carry old prompts, which are
//! answered or gone, so those paths open none.

use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use crucible_core::interaction::{AskRequest, InteractionEvent, InteractionRequest};
use crucible_daemon::SessionEvent;
use tokio::sync::mpsc;

use crate::tui::oil::chat_app::ChatAppMsg;
use crate::tui::oil::chat_runner::{live_session_event_consumer, session_event_consumer};

fn ask() -> InteractionRequest {
    InteractionRequest::Ask(AskRequest::new("Which branch?"))
}

fn requested(session: &str, request_id: &str) -> SessionEvent {
    SessionEvent::new(
        session,
        "interaction_requested",
        serde_json::json!({ "request_id": request_id, "request": ask() }),
    )
}

/// Feed `events` through `consume`, and return the ids of the prompts that
/// it opened, in order.
async fn opened_prompts(
    events: Vec<SessionEvent>,
    consume: impl FnOnce(
        mpsc::UnboundedReceiver<SessionEvent>,
        mpsc::UnboundedSender<ChatAppMsg>,
    ) -> tokio::task::JoinHandle<()>,
) -> Vec<String> {
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
    for event in events {
        event_tx.send(event).unwrap();
    }
    drop(event_tx);
    consume(event_rx, msg_tx).await.unwrap();
    let mut opened = Vec::new();
    while let Ok(msg) = msg_rx.try_recv() {
        if let ChatAppMsg::OpenInteraction { request_id, .. } = msg {
            opened.push(request_id);
        }
    }
    opened
}

#[tokio::test]
async fn a_live_session_opens_each_of_its_prompts_once() {
    let pending = vec![InteractionEvent {
        request_id: "waiting".into(),
        request: ask(),
    }];
    let opened = opened_prompts(
        vec![
            // The subscription delivers the pending prompt again.
            requested("chat-1", "waiting"),
            requested("chat-1", "new"),
            // A prompt of another session is not this client's to answer.
            requested("chat-2", "other"),
        ],
        |event_rx, msg_tx| {
            tokio::spawn(live_session_event_consumer(
                "chat-1".into(),
                event_rx,
                pending,
                msg_tx,
                Arc::new(AtomicUsize::new(0)),
            ))
        },
    )
    .await;
    assert_eq!(opened, ["waiting", "new"]);
}

#[tokio::test]
async fn stored_history_opens_no_prompt() {
    let opened = opened_prompts(vec![requested("chat-1", "old")], |event_rx, msg_tx| {
        tokio::spawn(session_event_consumer(
            "chat-1".into(),
            event_rx,
            msg_tx,
            None,
        ))
    })
    .await;
    assert!(opened.is_empty(), "{opened:?}");
}
