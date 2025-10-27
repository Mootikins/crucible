//! Event debouncing for reducing event spam.

use crate::FileEvent;
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
    /// Create a new debouncer with the specified delay.
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            pending_events: HashMap::new(),
            max_batch_size: 100,
            deduplicate: true,
            last_cleanup: Instant::now(),
        }
    }

    /// Set the maximum batch size.
    #[allow(dead_code)]
    pub fn with_max_batch_size(mut self, size: usize) -> Self {
        self.max_batch_size = size;
        self
    }

    /// Enable or disable deduplication.
    #[allow(dead_code)]
    pub fn with_deduplication(mut self, enabled: bool) -> Self {
        self.deduplicate = enabled;
        self
    }

    /// Process an incoming event.
    pub async fn process_event(&mut self, event: FileEvent) -> Option<FileEvent> {
        trace!("Processing event: {:?}", event.kind);

        // Cleanup old events periodically
        if self.last_cleanup.elapsed() >= Duration::from_secs(10) {
            self.cleanup_old_events().await;
            self.last_cleanup = Instant::now();
        }

        // If debounce delay is zero or very small, emit events immediately
        if self.delay.as_millis() == 0 {
            debug!("Zero delay - emitting event immediately");
            return Some(event);
        }

        let key = if self.deduplicate {
            crate::utils::EventUtils::deduplication_key(&event)
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
    pub async fn check_ready_events(&mut self, now: Instant) -> Option<FileEvent> {
        self.emit_ready_events(now).await
    }

    /// Emit events that are ready to be processed.
    async fn emit_ready_events(&mut self, now: Instant) -> Option<FileEvent> {
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
            return None;
        }

        // Sort by emit time to maintain order
        ready_events.sort_by_key(|(_, pending)| pending.emit_time);

        // Batch events if there are many
        if ready_events.len() > self.max_batch_size {
            self.emit_batched_events(ready_events).await
        } else {
            // Emit the oldest event
            Some(ready_events.into_iter().next().unwrap().1.event)
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
            crate::events::FileEventKind::Batch(batch_events),
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

    /// Force emit all pending events immediately.
    #[allow(dead_code)]
    pub async fn flush(&mut self) -> Vec<FileEvent> {
        let _now = Instant::now();
        let mut events = Vec::new();

        // Collect all pending events
        for (_key, pending) in self.pending_events.drain() {
            events.push(pending.event);
        }

        debug!("Flushed {} pending events", events.len());
        events
    }

    /// Get the number of pending events.
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.pending_events.len()
    }

    /// Get statistics about the debouncer.
    #[allow(dead_code)]
    pub fn get_stats(&self) -> DebouncerStats {
        DebouncerStats {
            pending_events: self.pending_events.len(),
            delay_ms: self.delay.as_millis(),
            max_batch_size: self.max_batch_size,
            deduplication_enabled: self.deduplicate,
        }
    }
}

/// Statistics for the debouncer.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DebouncerStats {
    /// Number of pending events
    pub pending_events: usize,
    /// Debounce delay in milliseconds
    pub delay_ms: u128,
    /// Maximum batch size
    pub max_batch_size: usize,
    /// Whether deduplication is enabled
    pub deduplication_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileEvent, FileEventKind};
    use std::path::PathBuf;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_basic_debouncing() {
        let mut debouncer = Debouncer::new(Duration::from_millis(100));

        let event = FileEvent::new(FileEventKind::Modified, PathBuf::from("test.txt"));

        // First event should not be emitted immediately
        let result = debouncer.process_event(event.clone()).await;
        assert!(result.is_none());
        assert_eq!(debouncer.pending_count(), 1);

        // Wait for debounce delay
        sleep(Duration::from_millis(150)).await;

        // Process another event to trigger emission of the first
        let event2 = FileEvent::new(FileEventKind::Created, PathBuf::from("test2.txt"));
        let result = debouncer.process_event(event2).await;

        // The first event should now be emitted
        assert!(result.is_some());
        assert_eq!(result.unwrap().path, PathBuf::from("test.txt"));
    }

    #[tokio::test]
    async fn test_deduplication() {
        let mut debouncer = Debouncer::new(Duration::from_millis(100));

        let event1 = FileEvent::new(FileEventKind::Modified, PathBuf::from("test.txt"));
        let event2 = FileEvent::new(FileEventKind::Modified, PathBuf::from("test.txt"));

        // Process two identical events
        let result1 = debouncer.process_event(event1).await;
        assert!(result1.is_none());

        let result2 = debouncer.process_event(event2).await;
        assert!(result2.is_none());

        // Should still only have one pending event
        assert_eq!(debouncer.pending_count(), 1);
    }

    #[tokio::test]
    async fn test_flush() {
        let mut debouncer = Debouncer::new(Duration::from_millis(1000));

        let event = FileEvent::new(FileEventKind::Modified, PathBuf::from("test.txt"));
        debouncer.process_event(event).await;

        assert_eq!(debouncer.pending_count(), 1);

        let flushed = debouncer.flush().await;
        assert_eq!(flushed.len(), 1);
        assert_eq!(debouncer.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_cleanup() {
        let mut debouncer = Debouncer::new(Duration::from_millis(10));

        let event = FileEvent::new(FileEventKind::Modified, PathBuf::from("test.txt"));
        debouncer.process_event(event).await;

        assert_eq!(debouncer.pending_count(), 1);

        // Wait long enough for the event to be considered old (5x debounce delay = 50ms)
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Force cleanup by advancing time
        debouncer.last_cleanup = Instant::now() - Duration::from_secs(11);
        debouncer.cleanup_old_events().await;

        assert_eq!(debouncer.pending_count(), 0);
    }

    #[test]
    fn test_debouncer_stats() {
        let debouncer = Debouncer::new(Duration::from_millis(100))
            .with_max_batch_size(50)
            .with_deduplication(false);

        let stats = debouncer.get_stats();
        assert_eq!(stats.pending_events, 0);
        assert_eq!(stats.delay_ms, 100);
        assert_eq!(stats.max_batch_size, 50);
        assert!(!stats.deduplication_enabled);
    }
}
