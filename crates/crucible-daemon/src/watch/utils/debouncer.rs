//! Event debouncing for reducing event spam.

use crate::watch::traits::DebounceConfig;
use crate::watch::FileEvent;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, trace};

/// Debouncer for reducing event spam by grouping similar events.
pub struct Debouncer {
    /// Debounce delay
    delay: Duration,
    /// Pending events by deduplication key
    pending_events: HashMap<String, PendingEvent>,
    /// Maximum number of events to batch
    max_batch_size: usize,
    /// Whether to deduplicate identical events
    deduplicate: bool,
    /// Last cleanup time
    last_cleanup: Instant,
}

/// Information about a pending event.
#[derive(Debug, Clone)]
struct PendingEvent {
    /// The event itself
    event: FileEvent,
    /// Time when this event was first seen
    first_seen: Instant,
    /// Time when this event should be emitted
    emit_time: Instant,
    /// Number of times this event has been updated
    update_count: u32,
}

impl Debouncer {
    /// Create a new debouncer from a debounce configuration.
    pub fn new(config: DebounceConfig) -> Self {
        Self {
            delay: Duration::from_millis(config.delay_ms),
            pending_events: HashMap::new(),
            max_batch_size: config.max_batch_size,
            deduplicate: config.deduplicate,
            last_cleanup: Instant::now(),
        }
    }

    /// Process an incoming event.
    pub async fn process_event(&mut self, event: FileEvent) -> Vec<FileEvent> {
        trace!("Processing event: {:?}", event.kind);

        // Cleanup old events periodically
        if self.last_cleanup.elapsed() >= Duration::from_secs(10) {
            self.cleanup_old_events().await;
            self.last_cleanup = Instant::now();
        }

        // If debounce delay is zero or very small, emit events immediately
        if self.delay.as_millis() == 0 {
            debug!("Zero delay - emitting event immediately");
            return vec![event];
        }

        let key = if self.deduplicate {
            crate::watch::utils::EventUtils::deduplication_key(&event)
        } else {
            // Use unique key if deduplication is disabled
            format!(
                "{}:{}",
                event.path.display(),
                event.timestamp.timestamp_nanos_opt().unwrap_or(0)
            )
        };

        let now = Instant::now();
        let emit_time = now + self.delay;

        // First, check if any existing events are ready to be emitted
        let ready_event = self.emit_ready_events(now).await;

        match self.pending_events.get_mut(&key) {
            Some(pending) => {
                // Update existing pending event
                pending.event = event;
                pending.emit_time = emit_time;
                pending.update_count += 1;

                debug!(
                    "Updated pending event: {} (updates: {})",
                    key, pending.update_count
                );
                ready_event
            }
            None => {
                // New event - add to pending
                let pending = PendingEvent {
                    event,
                    first_seen: now,
                    emit_time,
                    update_count: 0,
                };

                self.pending_events.insert(key.clone(), pending);
                debug!("Added pending event: {}", key);

                ready_event
            }
        }
    }

    /// Check for and emit events that are ready to be processed.
    pub async fn check_ready_events(&mut self, now: Instant) -> Vec<FileEvent> {
        self.emit_ready_events(now).await
    }

    /// Emit every event whose delay has elapsed.
    ///
    /// Returns ALL of them. This used to return `Option<FileEvent>` — the
    /// oldest one — while `retain` below had already removed every ready
    /// event from `pending_events`. Two events becoming ready in the same
    /// tick therefore lost one PERMANENTLY: it was gone from the pending map
    /// and never returned to a caller. `max_batch_size` is 100, so the batch
    /// arm did not cover it either; the loss window was 2..=100 events.
    ///
    /// The symptom was a note that never reached the index, with no error and
    /// no retry, at a few percent of writes on an idle machine and more under
    /// load — more concurrent writes mean more events ripening together.
    async fn emit_ready_events(&mut self, now: Instant) -> Vec<FileEvent> {
        let mut ready_events = Vec::new();

        // Find events that are ready to emit
        self.pending_events.retain(|key, pending| {
            if pending.emit_time <= now {
                ready_events.push((key.clone(), pending.clone()));
                false // Remove from pending
            } else {
                true // Keep in pending
            }
        });

        if ready_events.is_empty() {
            return Vec::new();
        }

        // Sort by emit time to maintain order
        ready_events.sort_by_key(|(_, pending)| pending.emit_time);

        // Batch events if there are many
        if ready_events.len() > self.max_batch_size {
            self.emit_batched_events(ready_events)
                .await
                .into_iter()
                .collect()
        } else {
            ready_events
                .into_iter()
                .map(|(_, pending)| pending.event)
                .collect()
        }
    }

    /// Emit a batch of events as a single batch event.
    async fn emit_batched_events(
        &mut self,
        events: Vec<(String, PendingEvent)>,
    ) -> Option<FileEvent> {
        let mut batch_events = Vec::new();

        for (_key, pending) in events {
            batch_events.push(pending.event);
        }

        let batch_len = batch_events.len();

        // Create a batch event
        let batch_event = FileEvent::new(
            crate::watch::events::FileEventKind::Batch(batch_events),
            std::path::PathBuf::new(),
        );

        debug!("Emitting batch event with {} sub-events", batch_len);
        Some(batch_event)
    }

    /// Clean up old pending events.
    async fn cleanup_old_events(&mut self) {
        let now = Instant::now();
        let initial_count = self.pending_events.len();

        // Remove events that are older than 5x the debounce delay
        let max_age = self.delay * 5;
        self.pending_events
            .retain(|_key, pending| now.duration_since(pending.first_seen) < max_age);

        let removed = initial_count - self.pending_events.len();
        if removed > 0 {
            debug!("Cleaned up {} old pending events", removed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watch::events::FileEventKind;
    use std::path::PathBuf;

    fn modified(path: &str) -> FileEvent {
        FileEvent::new(FileEventKind::Modified, PathBuf::from(path))
    }

    /// Two writes to DIFFERENT files inside one debounce window ripen
    /// together. Both must come back.
    ///
    /// This is the note-never-indexed bug. `emit_ready_events` took every
    /// ready event out of `pending_events`, then returned only the oldest —
    /// so the second file's event was gone from the map and never handed to a
    /// caller. No error, no retry: the note simply never reached the index.
    #[tokio::test]
    async fn every_event_ready_in_one_tick_is_emitted() {
        let mut debouncer = Debouncer::new(DebounceConfig::new(10));

        assert!(debouncer
            .process_event(modified("/kiln/a.md"))
            .await
            .is_empty());
        assert!(debouncer
            .process_event(modified("/kiln/b.md"))
            .await
            .is_empty());

        // Both delays have elapsed.
        let ready = debouncer
            .check_ready_events(Instant::now() + Duration::from_millis(50))
            .await;

        let mut paths: Vec<String> = ready.iter().map(|e| e.path.display().to_string()).collect();
        paths.sort();
        assert_eq!(
            paths,
            vec!["/kiln/a.md".to_string(), "/kiln/b.md".to_string()],
            "every ready event must be emitted; dropping one loses a write forever"
        );
    }

    /// Nothing ripe yet means nothing emitted, and the events stay pending
    /// rather than being consumed.
    #[tokio::test]
    async fn an_unripe_event_stays_pending() {
        let mut debouncer = Debouncer::new(DebounceConfig::new(500));

        assert!(debouncer
            .process_event(modified("/kiln/a.md"))
            .await
            .is_empty());
        assert!(debouncer
            .check_ready_events(Instant::now())
            .await
            .is_empty());

        let ready = debouncer
            .check_ready_events(Instant::now() + Duration::from_millis(600))
            .await;
        assert_eq!(ready.len(), 1, "the event must survive until it is ripe");
    }
}
