//! Event queue for managing file events with backpressure handling.

use crate::{error::Result, events::FileEvent};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use tracing::debug;

/// Bounded event queue with backpressure handling.
///
/// When the queue is full, it automatically drops the oldest event to make room
/// for new events (DropOldest strategy).
pub struct EventQueue {
    /// Internal queue storage
    queue: VecDeque<FileEvent>,
    /// Maximum capacity
    capacity: usize,
    /// Number of events dropped due to overflow
    dropped_events: std::sync::atomic::AtomicU64,
    /// Total events processed
    processed_events: std::sync::atomic::AtomicU64,
    /// Current queue size (for atomic access)
    size: std::sync::atomic::AtomicUsize,
}

impl EventQueue {
    /// Create a new event queue with the specified capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity),
            capacity,
            dropped_events: AtomicU64::new(0),
            processed_events: AtomicU64::new(0),
            size: AtomicUsize::new(0),
        }
    }

    /// Push an event to the queue.
    pub async fn push(&mut self, event: FileEvent) -> Result<()> {
        if self.len() < self.capacity {
            self.queue.push_back(event);
            self.size.fetch_add(1, Ordering::Relaxed);
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
            // Size remains the same
            self.dropped_events.fetch_add(1, Ordering::Relaxed);
            Ok(())
        } else {
            // Queue is empty but capacity is 0, drop new event
            self.dropped_events.fetch_add(1, Ordering::Relaxed);
            Err(crate::error::Error::QueueFull(self.capacity))
        }
    }

    /// Pop an event from the front of the queue.
    #[allow(dead_code)]
    pub fn pop(&mut self) -> Option<FileEvent> {
        if let Some(event) = self.queue.pop_front() {
            self.size.fetch_sub(1, Ordering::Relaxed);
            self.processed_events.fetch_add(1, Ordering::Relaxed);
            Some(event)
        } else {
            None
        }
    }

    /// Drain all events from the queue.
    pub fn drain_all(&mut self) -> Vec<FileEvent> {
        let events: Vec<FileEvent> = self.queue.drain(..).collect();
        let count = events.len();
        self.size.fetch_sub(count, Ordering::Relaxed);
        self.processed_events
            .fetch_add(count as u64, Ordering::Relaxed);
        events
    }

    /// Get the current number of events in the queue.
    pub fn len(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    /// Check if the queue is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if the queue is full.
    #[allow(dead_code)]
    pub fn is_full(&self) -> bool {
        self.len() >= self.capacity
    }

    /// Get the queue capacity.
    #[allow(dead_code)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the fill ratio (0.0 to 1.0).
    pub fn fill_ratio(&self) -> f64 {
        if self.capacity == 0 {
            1.0
        } else {
            self.len() as f64 / self.capacity as f64
        }
    }

    /// Get queue statistics.
    pub fn get_stats(&self) -> QueueStats {
        QueueStats {
            current_size: self.len(),
            capacity: self.capacity,
            processed: self.processed_events.load(Ordering::Relaxed),
            dropped: self.dropped_events.load(Ordering::Relaxed),
            fill_ratio: self.fill_ratio(),
        }
    }

    /// Reset statistics.
    #[allow(dead_code)]
    pub fn reset_stats(&mut self) {
        self.processed_events.store(0, Ordering::Relaxed);
        self.dropped_events.store(0, Ordering::Relaxed);
    }

    /// Resize the queue capacity.
    #[allow(dead_code)]
    pub fn resize(&mut self, new_capacity: usize) {
        if new_capacity < self.len() {
            // Need to drop some events
            let drop_count = self.len() - new_capacity;
            for _ in 0..drop_count {
                self.queue.pop_front();
            }
            self.size.store(new_capacity, Ordering::Relaxed);
        }

        self.capacity = new_capacity;
        debug!("Resized queue to capacity: {}", new_capacity);
    }
}

/// Statistics for the event queue.
#[derive(Debug, Clone)]
pub struct QueueStats {
    /// Current queue size
    pub current_size: usize,
    /// Maximum capacity
    pub capacity: usize,
    /// Number of events processed
    pub processed: u64,
    /// Number of events dropped
    pub dropped: u64,
    /// Fill ratio (0.0 to 1.0)
    pub fill_ratio: f64,
}
