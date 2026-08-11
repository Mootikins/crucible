use crate::protocol::SessionEventMessage;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::sync::broadcast;

static SESSION_SEQ_COUNTERS: OnceLock<DashMap<String, Arc<AtomicU64>>> = OnceLock::new();

fn session_seq_counters() -> &'static DashMap<String, Arc<AtomicU64>> {
    SESSION_SEQ_COUNTERS.get_or_init(DashMap::new)
}

fn session_seq_counter(session_id: &str) -> Arc<AtomicU64> {
    session_seq_counters()
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(AtomicU64::new(0)))
        .clone()
}

pub(crate) fn stamp_event(
    mut event: SessionEventMessage,
    seq_counter: &AtomicU64,
) -> SessionEventMessage {
    let seq = seq_counter.fetch_add(1, Ordering::SeqCst) + 1;
    event.seq = Some(seq);
    event.with_timestamp()
}

pub(crate) fn emit_event(
    event_tx: &broadcast::Sender<SessionEventMessage>,
    event: SessionEventMessage,
) -> bool {
    let seq_counter = session_seq_counter(&event.session_id);
    event_tx
        .send(stamp_event(event, seq_counter.as_ref()))
        .is_ok()
}

/// Drop a finished session's sequence counter.
///
/// Without this the map grows one entry per session for the daemon's whole
/// lifetime — it is a global `static`, so no `Drop` anywhere reaches it.
///
/// Call it **last** in teardown. `cleanup_session` fires a turn's `cancel_tx`
/// and spawns three tasks that may still emit, and a late event re-creating the
/// counter at 1 is harmless — a restarted count on a dead session costs nothing,
/// where dropping the counter early would hand a live turn a duplicate `seq`.
pub(crate) fn forget_session(session_id: &str) {
    if let Some(map) = SESSION_SEQ_COUNTERS.get() {
        map.remove(session_id);
    }
}

/// Whether `session_id` still holds a sequence counter.
///
/// Read by `AgentManager::session_residue`: this map is a process-global
/// `static`, so no `Drop` anywhere reaches it and it is invisible to any
/// cleanup test that only inspects `AgentManager`'s fields.
pub(crate) fn has_seq_counter(session_id: &str) -> bool {
    SESSION_SEQ_COUNTERS
        .get()
        .is_some_and(|map| map.contains_key(session_id))
}

#[cfg(test)]
pub fn reset_seq_counters() {
    if let Some(map) = SESSION_SEQ_COUNTERS.get() {
        map.clear();
    }
}
