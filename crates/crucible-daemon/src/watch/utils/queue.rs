//! Event queue for managing file events with backpressure handling.

use crate::watch::{
    error::Result,
    events::{FileEvent, FileEventKind},
};
use std::collections::VecDeque;
use tracing::debug;

/// Bounded event queue with backpressure handling.
///
/// When the queue is full, it automatically drops the oldest event to make room
/// for new events (DropOldest strategy). A drop is not silent: the next drain
/// ends with one [`FileEventKind::Rescan`], so a consumer that mirrors the
/// watched tree reads it again instead of keeping a stale copy.
pub struct EventQueue {
    /// Internal queue storage
    queue: VecDeque<FileEvent>,
    /// Maximum capacity
    capacity: usize,
    /// Whether an event was dropped since the last drain.
    dropped: bool,
}

impl EventQueue {
    /// Create a new event queue with the specified capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity),
            capacity,
            dropped: false,
        }
    }

    /// Push an event to the queue.
    pub async fn push(&mut self, event: FileEvent) -> Result<()> {
        if self.len() < self.capacity {
            self.queue.push_back(event);
            Ok(())
        } else {
            // Handle backpressure
            self.handle_backpressure(event).await
        }
    }

    /// Handle backpressure by dropping the oldest event.
    async fn handle_backpressure(&mut self, event: FileEvent) -> Result<()> {
        if let Some(removed) = self.queue.pop_front() {
            debug!(
                "Dropping oldest event due to queue overflow: {:?}",
                removed.kind
            );
            self.queue.push_back(event);
            self.dropped = true;
            Ok(())
        } else {
            // Queue is empty but capacity is 0, drop new event
            Err(crate::watch::error::Error::QueueFull(self.capacity))
        }
    }

    /// Drain all events from the queue.
    pub fn drain_all(&mut self) -> Vec<FileEvent> {
        let mut events: Vec<FileEvent> = self.queue.drain(..).collect();
        if std::mem::take(&mut self.dropped) {
            events.push(FileEvent::new(
                FileEventKind::Rescan,
                std::path::PathBuf::new(),
            ));
        }
        events
    }

    /// Get the current number of events in the queue.
    pub fn len(&self) -> usize {
        self.queue.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watch::{Error, FileEventKind};

    #[tokio::test]
    async fn overflow_keeps_the_newest_events_in_order_and_drain_resets_length() {
        let mut queue = EventQueue::new(2);
        let events: Vec<_> = ["first.md", "second.md", "third.md"]
            .into_iter()
            .map(|path| FileEvent::new(FileEventKind::Modified, path.into()))
            .collect();
        for event in &events {
            queue.push(event.clone()).await.unwrap();
        }
        assert_eq!(queue.len(), 2);
        let drained = queue.drain_all();
        assert_eq!(drained[..2], events[1..]);
        assert_eq!(
            drained.last().map(|event| &event.kind),
            Some(&FileEventKind::Rescan),
            "a drain after a drop ends with a rescan, so the loss is not silent"
        );
        assert_eq!(drained.len(), 3);
        assert_eq!(queue.len(), 0);
        assert!(queue.drain_all().is_empty());
        queue.push(events[0].clone()).await.unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.drain_all(), events[..1]);
    }

    #[tokio::test]
    async fn zero_capacity_refuses_an_event_without_retaining_it() {
        let mut queue = EventQueue::new(0);
        let result = queue
            .push(FileEvent::new(FileEventKind::Created, "note.md".into()))
            .await;
        assert!(matches!(result, Err(Error::QueueFull(0))));
        assert_eq!(queue.len(), 0);
        assert!(queue.drain_all().is_empty());
    }
}
