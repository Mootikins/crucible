use crate::protocol::SessionEventMessage;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, PoisonError, RwLock};
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

/// Stamp `event` with its seq and a timestamp, then publish it.
///
/// Returns whether a broadcast receiver exists. A journal of the channel (see
/// [`attach_journal`]) gets the event in any case.
pub(crate) fn emit_event(
    event_tx: &broadcast::Sender<SessionEventMessage>,
    event: SessionEventMessage,
) -> bool {
    let seq_counter = session_seq_counter(&event.session_id);
    publish(event_tx, event, Some(seq_counter.as_ref()))
}

/// Publish an event that already has its seq and timestamp.
///
/// Only a replay uses this: it reproduces a recording, so the recorded seq
/// must stay. A live event goes through [`emit_event`].
pub(crate) fn publish_recorded(
    event_tx: &broadcast::Sender<SessionEventMessage>,
    event: SessionEventMessage,
) -> bool {
    publish(event_tx, event, None)
}

fn publish(
    event_tx: &broadcast::Sender<SessionEventMessage>,
    event: SessionEventMessage,
    seq_counter: Option<&AtomicU64>,
) -> bool {
    let stamp = |event| match seq_counter {
        Some(counter) => stamp_event(event, counter),
        None => event,
    };
    match journal_of(event_tx) {
        // The seq stamp, the journal send and the broadcast happen under the
        // journal's lock, so the seq order, the stored order and the
        // broadcast order are one order.
        Some(journal) => {
            journal
                .send_then(
                    || {
                        let event = stamp(event);
                        (event.clone(), event)
                    },
                    |event| event_tx.send(event).is_ok(),
                )
                .1
        }
        None => event_tx.send(stamp(event)).is_ok(),
    }
}

/// The journals of the event channels that have one.
///
/// A journal is the storage side of one event channel. The broadcast ring is
/// lossy by design: a receiver that falls behind loses the oldest events and
/// gets a gap marker. A client can recover from that, because it can read the
/// history again. The writer of the history cannot, so it reads a journal and
/// not the ring. The journal gets every event of the channel, in the
/// broadcast order, and drops none (see [`crate::lossless_queue`]).
///
/// Keyed by the channel, because the callers of [`emit_event`] hold only a
/// `broadcast::Sender`, and a journal must not depend on each of about ninety
/// callers passing it. A weak key does not keep a channel open. The list has
/// one entry for each daemon in the process, so a scan is cheap.
type Journal = crate::lossless_queue::Sender<SessionEventMessage>;

static JOURNALS: RwLock<Vec<(broadcast::WeakSender<SessionEventMessage>, Journal)>> =
    RwLock::new(Vec::new());

fn journal_of(event_tx: &broadcast::Sender<SessionEventMessage>) -> Option<Journal> {
    JOURNALS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|(channel, _)| {
            channel
                .upgrade()
                .is_some_and(|channel| channel.same_channel(event_tx))
        })
        .map(|(_, journal)| journal.clone())
}

/// Give `event_tx` a journal. From now on each event on the channel also goes
/// to the returned reader, in order and without loss. The waiter returns when
/// the reader finished each event published before the wait.
///
/// A second call for one channel replaces the first journal.
pub(crate) fn attach_journal(
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> (
    crate::lossless_queue::Waiter,
    crate::lossless_queue::Receiver<SessionEventMessage>,
) {
    let (journal, reader) = crate::lossless_queue::channel();
    let waiter = journal.waiter();
    let mut journals = JOURNALS.write().unwrap_or_else(PoisonError::into_inner);
    journals.retain(|(channel, _)| {
        channel
            .upgrade()
            .is_some_and(|channel| !channel.same_channel(event_tx))
    });
    journals.push((event_tx.downgrade(), journal));
    (waiter, reader)
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
