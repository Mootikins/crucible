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

#[cfg(test)]
pub fn reset_seq_counters() {
    if let Some(map) = SESSION_SEQ_COUNTERS.get() {
        map.clear();
    }
}
