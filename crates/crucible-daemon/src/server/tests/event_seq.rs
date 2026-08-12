//! `seq` must be universal, because a gap marker no client can act on is not a
//! signal.
//!
//! `SessionEventMessage.seq` is stamped by `event_emitter::emit_event` and by
//! nothing else. Every site that reached for `event_tx.send` directly shipped
//! `seq: None`, so a client could not check contiguity even if it wanted to —
//! one `None` in a stream makes the whole stream unverifiable. These tests pin
//! both halves: the events themselves carry a seq, and no new bypass can appear
//! without the lint below going red.

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
                     event_emitter::emit_event, so this client's stream cannot be \
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
    let (event_tx, _) = broadcast::channel(64);
    let mut event_rx = event_tx.subscribe();

    let km = Arc::new(KilnManager::new());
    let sm = Arc::new(SessionManager::new());
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
    let (event_tx, _) = broadcast::channel(64);
    let mut event_rx = event_tx.subscribe();

    for _ in 0..2 {
        for session in ["seq-probe-a", "seq-probe-b"] {
            crate::event_emitter::emit_event(
                &event_tx,
                SessionEventMessage::text_delta(session, "x"),
            );
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

/// Files allowed to call `event_tx.send` directly, each with the reason.
///
/// Every entry is checked to still contain a direct send, so an entry that stops
/// being needed fails this test instead of quietly widening the hole.
const DIRECT_SEND_ALLOWED: &[(&str, &str)] = &[
    (
        "event_emitter.rs",
        "the one place that may: it stamps seq first",
    ),
    (
        "replay.rs",
        "re-emits a recording's own recorded seq; stamping would renumber history",
    ),
    (
        "rpc_client/client/mod.rs",
        "an mpsc relay of already-stamped daemon events, not the broadcast",
    ),
];

/// The lint that keeps `seq` universal.
///
/// A behavioural test can only cover the sites that exist today; roughly a dozen
/// bypasses accumulated one at a time, each invisible because nothing failed. So
/// the check is on the source: outside the allowlist, no production file in this
/// crate may reach for `event_tx.send`.
///
/// Honest about its limit — it matches the identifier `event_tx`, so a sender
/// bound to another name slips through. It catches the pattern every existing
/// bypass actually used, which is the class that has been recurring.
///
/// The pattern tolerates the line break rustfmt inserts before `.send(` when the
/// call is long; matching the one-line form only would let a reformat hide a
/// bypass.
#[test]
fn no_production_code_bypasses_emit_event() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let direct_send = regex::Regex::new(r"event_tx\s*\.\s*send\(").expect("static pattern");

    let mut offenders: Vec<String> = Vec::new();
    let mut allowlist_hits: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for path in rust_files(&src) {
        let rel = path
            .strip_prefix(&src)
            .expect("walked from src")
            .to_string_lossy()
            .replace('\\', "/");

        // Test fixtures push raw events on purpose — a fixture asserting on an
        // unstamped event is testing the transport, not the emitter.
        if rel.starts_with("server/tests/") {
            continue;
        }

        let text = std::fs::read_to_string(&path).expect("read source");
        if !direct_send.is_match(&text) {
            continue;
        }

        match DIRECT_SEND_ALLOWED.iter().find(|(f, _)| rel.ends_with(f)) {
            Some((f, _)) => {
                allowlist_hits.insert(f);
            }
            None => {
                for m in direct_send.find_iter(&text) {
                    offenders.push(format!("{rel}:{}", text[..m.start()].lines().count()));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these sites send on the event broadcast without stamping a seq, which \
         makes every client's stream ungappable; route them through \
         `crate::event_emitter::emit_event`:\n  {}",
        offenders.join("\n  ")
    );

    for (file, reason) in DIRECT_SEND_ALLOWED {
        assert!(
            allowlist_hits.contains(file),
            "allowlist entry `{file}` ({reason}) no longer contains a direct \
             `event_tx.send(` — delete the entry rather than leaving the hole open"
        );
    }
}

fn rust_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).expect("read_dir").flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "rs") {
                out.push(p);
            }
        }
    }
    out
}
