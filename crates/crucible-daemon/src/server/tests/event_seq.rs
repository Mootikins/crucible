//! `seq` must be universal, because a gap marker no client can act on is not a
//! signal.
//!
//! `SessionEventMessage.seq` is stamped by `EventBus::emit` and by
//! nothing else. Every site that reached for `event_tx.send` directly shipped
//! `seq: None`, so a client could not check contiguity even if it wanted to —
//! one `None` in a stream makes the whole stream unverifiable. These tests pin
//! emitted events carry a seq. The private sender prevents bypassing publication.
use crate::test_support::temp_session_manager;

use super::*;
use crate::protocol::SessionEventMessage;

/// Every event's `seq`, in arrival order, with a readable panic on the first
/// `None` — which is the failure mode being closed, so it must name the event.
fn seqs(events: &[SessionEventMessage]) -> Vec<u64> {
    events
        .iter()
        .map(|e| {
            e.seq.unwrap_or_else(|| {
                panic!(
                    "event `{}` on session `{}` carries no seq: it bypassed \
                     EventBus::emit, so this client's stream cannot be \
                     gap-checked",
                    e.event, e.session_id
                )
            })
        })
        .collect()
}

/// Contiguity is per `session_id`, not global: each key has its own counter, so
/// two sessions interleaving on one broadcast channel both count from 1.
/// Wildcard-addressed events (`WILDCARD_SESSION`) form their own stream under
/// the key `"*"` for the same reason.
fn assert_contiguous_from_one(seqs: &[u64]) {
    let expected: Vec<u64> = (1..=seqs.len() as u64).collect();
    assert_eq!(
        seqs,
        &expected[..],
        "per-session seq must be contiguous from 1 so a client can detect a gap \
         by arithmetic alone"
    );
}

/// A UI style push is the other shape: not a turn, not a batch, and addressed
/// to a session that may have no turn stream at all.
#[tokio::test]
async fn ui_style_broadcasts_carry_a_contiguous_seq() {
    let (event_tx, _) = crate::EventBus::channel(64);
    let mut event_rx = event_tx.subscribe();

    let km = Arc::new(KilnManager::new());
    let sm = temp_session_manager();
    let agents = test_agent_manager(km, sm, event_tx.clone(), None);

    // A session id nothing else in this process has used, so the counter it
    // allocates genuinely starts at 1.
    let session_id = "seq-probe-ui-style";
    crate::server::ui_broadcast::broadcast_style_changed(&event_tx, &agents, session_id);
    crate::server::ui_broadcast::broadcast_exprs_changed(&event_tx, &agents, session_id);

    let mut events = Vec::new();
    for _ in 0..2 {
        events.push(event_rx.recv().await.expect("event channel closed"));
    }

    assert_contiguous_from_one(&seqs(&events));
}

/// Two sessions on one channel each count from 1 — the property that makes
/// per-session contiguity checkable at all.
#[tokio::test]
async fn seq_is_per_session_not_per_channel() {
    let (event_tx, _) = crate::EventBus::channel(64);
    let mut event_rx = event_tx.subscribe();

    for _ in 0..2 {
        for session in ["seq-probe-a", "seq-probe-b"] {
            event_tx.emit(SessionEventMessage::text_delta(session, "x"));
        }
    }

    let mut got: Vec<(String, u64)> = Vec::new();
    for _ in 0..4 {
        let e = event_rx.recv().await.expect("event channel closed");
        let seq = e.seq.expect("emit_event stamps");
        got.push((e.session_id, seq));
    }

    assert_eq!(
        got,
        vec![
            ("seq-probe-a".to_string(), 1),
            ("seq-probe-b".to_string(), 1),
            ("seq-probe-a".to_string(), 2),
            ("seq-probe-b".to_string(), 2),
        ]
    );
}
