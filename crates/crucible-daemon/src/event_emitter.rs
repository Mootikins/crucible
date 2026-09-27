//! The daemon's publication owner: sequencing, lossless journal and live ring.
use crate::protocol::SessionEventMessage;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct EventBus(Arc<Inner>);

struct Inner {
    live: broadcast::Sender<SessionEventMessage>,
    journal: crate::lossless_queue::Sender<SessionEventMessage>,
    // This lock orders stamping, journal enqueue and live publication together.
    sequences: Mutex<HashMap<String, u64>>,
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("receivers", &self.0.live.receiver_count())
            .finish_non_exhaustive()
    }
}

impl EventBus {
    /// A live bus without a persistence consumer, for detached runtimes/fixtures.
    /// The journal reader is dropped, so publications cannot accumulate there.
    pub fn channel(capacity: usize) -> (Self, broadcast::Receiver<SessionEventMessage>) {
        let (bus, _waiter, _journal) = Self::journaled_channel(capacity);
        let receiver = bus.subscribe();
        (bus, receiver)
    }

    /// Production constructs the bus and its lossless persistence consumer
    /// together. No global lookup or later journal attachment is required.
    pub(crate) fn journaled_channel(
        capacity: usize,
    ) -> (
        Self,
        crate::lossless_queue::Waiter,
        crate::lossless_queue::Receiver<SessionEventMessage>,
    ) {
        let (live, _) = broadcast::channel(capacity);
        let (journal, reader) = crate::lossless_queue::channel();
        let waiter = journal.waiter();
        let bus = Self(Arc::new(Inner {
            live,
            journal,
            sequences: Mutex::new(HashMap::new()),
        }));
        (bus, waiter, reader)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SessionEventMessage> {
        self.0.live.subscribe()
    }

    pub fn receiver_count(&self) -> usize {
        self.0.live.receiver_count()
    }

    /// Returns whether a live receiver exists. The journal receives the event
    /// independently of live readers and cannot lag out of the broadcast ring.
    pub fn emit(&self, event: SessionEventMessage) -> bool {
        self.publish(event, true)
    }

    /// Reproduce a recording without changing its sequence or timestamp.
    pub(crate) fn publish_recorded(&self, event: SessionEventMessage) -> bool {
        self.publish(event, false)
    }

    fn publish(&self, mut event: SessionEventMessage, stamp: bool) -> bool {
        let mut sequences = self
            .0
            .sequences
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if stamp {
            let counter = sequences.entry(event.session_id.clone()).or_default();
            event = stamp_event(event, counter);
        }
        self.0.journal.send(event.clone());
        let received = self.0.live.send(event).is_ok();
        drop(sequences);
        received
    }

    /// Retire last in session cleanup. A late emitter may start a counter again
    /// on that dead session; retirement must never reset a live turn's count.
    pub(crate) fn forget_session(&self, session_id: &str) {
        self.0
            .sequences
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(session_id);
    }

    pub(crate) fn has_seq_counter(&self, session_id: &str) -> bool {
        self.0
            .sequences
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(session_id)
    }
}

pub(crate) fn stamp_event(
    mut event: SessionEventMessage,
    counter: &mut u64,
) -> SessionEventMessage {
    *counter += 1;
    event.seq = Some(*counter);
    event.with_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_buses_start_the_same_session_at_one() {
        let (first, mut first_events) = EventBus::channel(4);
        let (second, mut second_events) = EventBus::channel(4);
        first.emit(SessionEventMessage::text_delta("shared-id", "first"));
        second.emit(SessionEventMessage::text_delta("shared-id", "second"));
        assert_eq!(first_events.try_recv().unwrap().seq, Some(1));
        assert_eq!(second_events.try_recv().unwrap().seq, Some(1));
    }
    #[test]
    fn concurrent_publications_have_one_journal_and_live_order() {
        let (bus, _waiter, mut journal) = EventBus::journaled_channel(512);
        let mut live = bus.subscribe();
        let start = std::sync::Barrier::new(4);
        std::thread::scope(|scope| {
            for worker in 0..4 {
                let bus = &bus;
                let start = &start;
                scope.spawn(move || {
                    start.wait();
                    for index in 0..100 {
                        assert!(bus.emit(SessionEventMessage::text_delta(
                            "concurrent",
                            format!("{worker}:{index}"),
                        )));
                    }
                });
            }
        });
        for seq in 1..=400 {
            let stored = journal.try_recv().expect("every event journaled");
            let received = live.try_recv().expect("every event broadcast");
            assert_eq!(stored.seq, Some(seq));
            assert_eq!(
                serde_json::to_value(&*stored).unwrap(),
                serde_json::to_value(received).unwrap()
            );
        }
        assert!(journal.try_recv().is_none());
        assert!(live.try_recv().is_err());
    }

    #[tokio::test]
    async fn the_journal_outlives_live_readers_and_preserves_recorded_metadata() {
        let (bus, waiter, mut journal) = EventBus::journaled_channel(1);
        assert!(!bus.emit(SessionEventMessage::text_delta("stored", "first")));
        let first = journal.try_recv().expect("journal with no live readers");
        assert_eq!(first.seq, Some(1));
        assert!(first.timestamp.is_some());
        drop(first);
        let mut recorded = SessionEventMessage::text_delta("stored", "recorded");
        recorded.seq = Some(91);
        recorded.timestamp = Some("2000-01-01T00:00:00Z".parse().unwrap());
        assert!(!bus.publish_recorded(recorded.clone()));
        let stored = journal.try_recv().unwrap();
        assert_eq!(
            serde_json::to_value(&*stored).unwrap(),
            serde_json::to_value(recorded).unwrap()
        );
        drop(stored);
        tokio::time::timeout(std::time::Duration::from_secs(1), waiter.wait())
            .await
            .unwrap();
        assert!(!bus.emit(SessionEventMessage::text_delta("stored", "next live")));
        assert_eq!(journal.try_recv().unwrap().seq, Some(2));
    }

    #[test]
    fn retirement_is_local_and_a_late_emitter_can_restart_a_dead_session() {
        let (bus, mut events) = EventBus::channel(8);
        let clone = bus.clone();
        bus.emit(SessionEventMessage::text_delta("dead", "before"));
        bus.emit(SessionEventMessage::text_delta("live", "before"));
        assert!(bus.has_seq_counter("dead"));
        bus.forget_session("dead");
        assert!(!bus.has_seq_counter("dead"));
        assert!(bus.has_seq_counter("live"));
        clone.emit(SessionEventMessage::text_delta("dead", "late"));
        clone.emit(SessionEventMessage::text_delta("live", "after"));
        let seqs: Vec<_> = (0..4).map(|_| events.try_recv().unwrap().seq).collect();
        assert_eq!(seqs, [Some(1), Some(1), Some(1), Some(2)]);
    }
}
