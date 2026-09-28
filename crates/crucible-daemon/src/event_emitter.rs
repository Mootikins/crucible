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
    // This lock orders stamping, folding, journal enqueue and live
    // publication together.
    sessions: Mutex<HashMap<String, SessionStream>>,
}

/// What the bus keeps for one session: its seq counter and the fold of its
/// transcript. The fold reads each event in seq order, because the lock that
/// stamps the event also folds it.
#[derive(Default)]
struct SessionStream {
    seq: u64,
    fold: crucible_core::transcript::TranscriptFold,
    /// The fold started from the stored log. A fold that an event started
    /// holds only the events since then.
    seeded: bool,
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
        let (bus, _journal) = Self::journaled_channel(capacity);
        let receiver = bus.subscribe();
        (bus, receiver)
    }

    /// Production constructs the bus and its lossless persistence consumer
    /// together. No global lookup or later journal attachment is required.
    pub(crate) fn journaled_channel(
        capacity: usize,
    ) -> (Self, crate::lossless_queue::Receiver<SessionEventMessage>) {
        let (live, _) = broadcast::channel(capacity);
        let (journal, reader) = crate::lossless_queue::channel();
        let bus = Self(Arc::new(Inner {
            live,
            journal,
            sessions: Mutex::new(HashMap::new()),
        }));
        (bus, reader)
    }

    /// A wait for the consumer of the journal. See
    /// [`crate::session_manager::SessionManager::settle_history`].
    pub(crate) fn journal_waiter(&self) -> crate::lossless_queue::Waiter {
        self.0.journal.waiter()
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
        let mut sessions = self.lock();
        let stream = sessions.entry(event.session_id.clone()).or_default();
        if stamp {
            // A name that no payload enum declares decodes as `UnknownEvent`
            // in every client. Build events with `SessionEventMessage::typed`.
            debug_assert!(
                crucible_core::protocol::Group::of(&event.event).is_some(),
                "the EventBus refuses the undeclared event name `{}`",
                event.event
            );
            event = stamp_event(event, &mut stream.seq);
        }
        // The journal stores the event without its ops: a reader of the log
        // folds the events again.
        self.0.journal.send(event.clone());
        event.transcript = stream.fold.apply(&event);
        let received = self.0.live.send(event).is_ok();
        drop(sessions);
        received
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, SessionStream>> {
        self.0
            .sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Start the fold of `session_id` from its stored log, when the bus has
    /// no fold for it yet. A client reads its snapshot from that log, so the
    /// next ops must continue the same fold.
    pub(crate) fn seed_transcript(
        &self,
        session_id: &str,
        fold: impl FnOnce() -> crucible_core::transcript::TranscriptFold,
    ) {
        let mut sessions = self.lock();
        if !sessions.contains_key(session_id) {
            sessions.insert(
                session_id.to_owned(),
                SessionStream {
                    seq: 0,
                    fold: fold(),
                    seeded: true,
                },
            );
        }
    }

    /// The live fold of `session_id`, when it started from the stored log.
    ///
    /// It holds each event that the bus folded, the text deltas too, which
    /// the log does not store. A client that reads this snapshot while a turn
    /// runs can apply the next live ops to it.
    pub(crate) fn transcript(
        &self,
        session_id: &str,
    ) -> Option<crucible_core::transcript::Transcript> {
        let sessions = self.lock();
        let stream = sessions.get(session_id).filter(|stream| stream.seeded)?;
        Some(stream.fold.snapshot())
    }

    /// Continue the seq of `session_id` above `persisted`, the highest seq in
    /// its log. A new daemon process, or a session that cleanup retired,
    /// starts with no counter. Without this seed its next event would take a
    /// seq that the log and a client cursor already hold, and a client would
    /// drop the event as a duplicate. The seed never lowers a live counter.
    pub(crate) fn seed_session(&self, session_id: &str, persisted: u64) {
        let mut sessions = self.lock();
        let stream = sessions.entry(session_id.to_owned()).or_default();
        stream.seq = stream.seq.max(persisted);
    }

    /// Retire last in session cleanup. A late emitter may start a counter again
    /// on that dead session; retirement must never reset a live turn's count.
    pub(crate) fn forget_session(&self, session_id: &str) {
        self.lock().remove(session_id);
    }

    pub(crate) fn has_seq_counter(&self, session_id: &str) -> bool {
        self.lock().contains_key(session_id)
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
        let (bus, mut journal) = EventBus::journaled_channel(512);
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
            let mut received = live.try_recv().expect("every event broadcast");
            assert_eq!(stored.seq, Some(seq));
            // The live copy also carries the ops of the fold.
            received.transcript.clear();
            assert_eq!(
                serde_json::to_value(&*stored).unwrap(),
                serde_json::to_value(received).unwrap()
            );
        }
        assert!(journal.try_recv().is_none());
        assert!(live.try_recv().is_err());
    }

    /// The live copy of an event carries the ops of the transcript fold; the
    /// journal copy does not, because a reader of the log folds it again.
    #[test]
    fn the_live_copy_carries_the_fold_and_the_journal_copy_does_not() {
        let (bus, mut journal) = EventBus::journaled_channel(8);
        let mut live = bus.subscribe();
        bus.emit(SessionEventMessage::user_message("s", "m1", "hello"));

        let stored = journal.try_recv().expect("journaled");
        assert!(stored.transcript.is_empty());
        assert!(!serde_json::to_string(&*stored)
            .unwrap()
            .contains("transcript"));

        let received = live.try_recv().expect("broadcast");
        let mut transcript = crucible_core::transcript::Transcript::default();
        assert!(received.transcript.iter().all(|op| transcript.apply(op)));
        assert_eq!(transcript.items[0].id, "m1");
    }

    /// A seeded fold continues the stored log: the next turn's ops fit the
    /// snapshot of that log, and its ids do not restart.
    #[test]
    fn a_seeded_fold_continues_the_stored_log() {
        let stored = [
            SessionEventMessage::user_message("s", "m1", "one"),
            SessionEventMessage::user_message("s", "m2", "two"),
        ];
        let snapshot = crucible_core::transcript::TranscriptFold::of_events(&stored);
        let (bus, _journal) = EventBus::journaled_channel(8);
        let mut live = bus.subscribe();
        bus.seed_transcript("s", || {
            crucible_core::transcript::TranscriptFold::from_events(&stored)
        });
        bus.emit(SessionEventMessage::text_delta("s", "answer"));

        let mut transcript = snapshot;
        let received = live.try_recv().expect("broadcast");
        assert!(received.transcript.iter().all(|op| transcript.apply(op)));
        assert_eq!(transcript.items.len(), 3);
        assert_eq!(
            transcript.items[2].id, "m2-seg-0",
            "the answer joins the open turn"
        );
    }

    #[tokio::test]
    async fn the_journal_outlives_live_readers_and_preserves_recorded_metadata() {
        let (bus, mut journal) = EventBus::journaled_channel(1);
        let waiter = bus.journal_waiter();
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

    #[test]
    fn a_seed_continues_above_the_log_and_never_lowers_a_live_counter() {
        let (bus, mut events) = EventBus::channel(8);
        bus.seed_session("restarted", 41);
        bus.emit(SessionEventMessage::text_delta(
            "restarted",
            "after restart",
        ));
        assert_eq!(events.try_recv().unwrap().seq, Some(42));
        bus.seed_session("restarted", 7);
        bus.emit(SessionEventMessage::text_delta("restarted", "later"));
        assert_eq!(events.try_recv().unwrap().seq, Some(43));
    }

    #[test]
    #[should_panic(expected = "undeclared event name")]
    #[cfg(debug_assertions)]
    fn an_undeclared_event_name_is_refused() {
        let (bus, _events) = EventBus::channel(8);
        bus.emit(SessionEventMessage::new(
            "s",
            "an_event_no_payload_declares",
            serde_json::json!({}),
        ));
    }
}
