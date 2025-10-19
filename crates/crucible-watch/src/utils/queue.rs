//! Event queue for managing file events with backpressure handling.

use crate::{events::FileEvent, error::Result};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use tracing::{debug, warn};

/// Bounded event queue with backpressure handling.
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
    /// Backpressure strategy
    backpressure_strategy: BackpressureStrategy,
}

/// Strategy for handling backpressure when queue is full.
#[derive(Debug, Clone)]
pub enum BackpressureStrategy {
    /// Drop new events
    DropNew,
    /// Drop oldest events
    DropOldest,
    /// Block until space is available
    Block,
    /// Drop events with lowest priority
    DropLowPriority,
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
            backpressure_strategy: BackpressureStrategy::DropOldest,
        }
    }

    /// Set the backpressure strategy.
    pub fn with_backpressure_strategy(mut self, strategy: BackpressureStrategy) -> Self {
        self.backpressure_strategy = strategy;
        self
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

    /// Handle backpressure based on the configured strategy.
    async fn handle_backpressure(&mut self, event: FileEvent) -> Result<()> {
        match self.backpressure_strategy {
            BackpressureStrategy::DropNew => {
                self.dropped_events.fetch_add(1, Ordering::Relaxed);
                warn!("Dropping new event due to queue overflow: {:?}", event.kind);
                Err(crate::error::Error::QueueFull(self.capacity))
            }
            BackpressureStrategy::DropOldest => {
                if let Some(removed) = self.queue.pop_front() {
                    debug!("Dropping oldest event due to queue overflow: {:?}", removed.kind);
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
            BackpressureStrategy::Block => {
                // In a real implementation, this would use async waiting
                // For now, we'll just wait for space to become available
                while self.len() >= self.capacity {
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
                self.queue.push_back(event);
                self.size.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            BackpressureStrategy::DropLowPriority => {
                // Find and remove the lowest priority event
                if let Some(lowest_priority_index) = self.find_lowest_priority_index() {
                    self.queue.remove(lowest_priority_index);
                    self.queue.push_back(event);
                    self.dropped_events.fetch_add(1, Ordering::Relaxed);
                    debug!("Dropped low priority event due to queue overflow");
                    Ok(())
                } else {
                    // No suitable event to drop, use drop new strategy
                    self.dropped_events.fetch_add(1, Ordering::Relaxed);
                    Err(crate::error::Error::QueueFull(self.capacity))
                }
            }
        }
    }

    /// Find the index of the lowest priority event.
    fn find_lowest_priority_index(&self) -> Option<usize> {
        if self.queue.is_empty() {
            return None;
        }

        let mut lowest_index = 0;
        let mut lowest_priority = self.get_event_priority(&self.queue[0]);

        for (index, event) in self.queue.iter().enumerate().skip(1) {
            let priority = self.get_event_priority(event);
            if priority < lowest_priority {
                lowest_priority = priority;
                lowest_index = index;
            }
        }

        Some(lowest_index)
    }

    /// Get priority value for an event (higher = more important).
    fn get_event_priority(&self, event: &FileEvent) -> u8 {
        if crate::utils::EventUtils::is_high_priority(event) {
            10 // High priority
        } else {
            1 // Normal priority
        }
    }

    /// Pop an event from the front of the queue.
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
        self.processed_events.fetch_add(count as u64, Ordering::Relaxed);
        events
    }

    /// Get the current number of events in the queue.
    pub fn len(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    /// Check if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if the queue is full.
    pub fn is_full(&self) -> bool {
        self.len() >= self.capacity
    }

    /// Get the queue capacity.
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
    pub fn reset_stats(&mut self) {
        self.processed_events.store(0, Ordering::Relaxed);
        self.dropped_events.store(0, Ordering::Relaxed);
    }

    /// Resize the queue capacity.
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

impl Default for BackpressureStrategy {
    fn default() -> Self {
        Self::DropOldest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{FileEvent, FileEventKind};
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_basic_queue_operations() {
        let mut queue = EventQueue::new(3);

        assert_eq!(queue.len(), 0);
        assert!(queue.is_empty());

        let event = FileEvent::new(FileEventKind::Created, PathBuf::from("test.txt"));
        queue.push(event.clone()).await.unwrap();

        assert_eq!(queue.len(), 1);
        assert!(!queue.is_empty());

        let popped = queue.pop();
        assert_eq!(popped.unwrap().path, event.path);
        assert!(queue.is_empty());
    }

    #[tokio::test]
    async fn test_queue_overflow_drop_oldest() {
        let mut queue = EventQueue::new(2)
            .with_backpressure_strategy(BackpressureStrategy::DropOldest);

        let event1 = FileEvent::new(FileEventKind::Created, PathBuf::from("test1.txt"));
        let event2 = FileEvent::new(FileEventKind::Created, PathBuf::from("test2.txt"));
        let event3 = FileEvent::new(FileEventKind::Created, PathBuf::from("test3.txt"));

        queue.push(event1).await.unwrap();
        queue.push(event2).await.unwrap();
        queue.push(event3).await.unwrap(); // Should drop event1

        assert_eq!(queue.len(), 2);

        let first = queue.pop().unwrap();
        let second = queue.pop().unwrap();

        assert_eq!(first.path, PathBuf::from("test2.txt"));
        assert_eq!(second.path, PathBuf::from("test3.txt"));

        let stats = queue.get_stats();
        assert_eq!(stats.dropped, 1);
    }

    #[tokio::test]
    async fn test_queue_overflow_drop_new() {
        let mut queue = EventQueue::new(2)
            .with_backpressure_strategy(BackpressureStrategy::DropNew);

        let event1 = FileEvent::new(FileEventKind::Created, PathBuf::from("test1.txt"));
        let event2 = FileEvent::new(FileEventKind::Created, PathBuf::from("test2.txt"));
        let event3 = FileEvent::new(FileEventKind::Created, PathBuf::from("test3.txt"));

        queue.push(event1).await.unwrap();
        queue.push(event2).await.unwrap();
        let result = queue.push(event3).await; // Should fail

        assert!(result.is_err());
        assert_eq!(queue.len(), 2);

        let stats = queue.get_stats();
        assert_eq!(stats.dropped, 1);
    }

    #[tokio::test]
    async fn test_drain_all() {
        let mut queue = EventQueue::new(5);

        for i in 1..=3 {
            let event = FileEvent::new(FileEventKind::Created, PathBuf::from(format!("test{}.txt", i)));
            queue.push(event).await.unwrap();
        }

        assert_eq!(queue.len(), 3);

        let events = queue.drain_all();
        assert_eq!(events.len(), 3);
        assert!(queue.is_empty());
    }

    #[tokio::test]
    async fn test_priority_dropping() {
        let mut queue = EventQueue::new(2)
            .with_backpressure_strategy(BackpressureStrategy::DropLowPriority);

        // Add low priority event
        let low_event = FileEvent::new(FileEventKind::Modified, PathBuf::from("test.exe"));
        queue.push(low_event).await.unwrap();

        // Add high priority event
        let high_event = FileEvent::new(FileEventKind::Deleted, PathBuf::from("test.md"));
        queue.push(high_event).await.unwrap();

        // Add another event, should drop the low priority one
        let another_event = FileEvent::new(FileEventKind::Created, PathBuf::from("test2.md"));
        queue.push(another_event).await.unwrap();

        assert_eq!(queue.len(), 2);

        let stats = queue.get_stats();
        assert_eq!(stats.dropped, 1);
    }

    #[test]
    fn test_queue_stats() {
        let mut queue = EventQueue::new(10);
        let stats = queue.get_stats();

        assert_eq!(stats.current_size, 0);
        assert_eq!(stats.capacity, 10);
        assert_eq!(stats.processed, 0);
        assert_eq!(stats.dropped, 0);
        assert_eq!(stats.fill_ratio, 0.0);
    }

    #[test]
    fn test_queue_resize() {
        let mut queue = EventQueue::new(5);

        // Simulate some events
        queue.size.store(3, Ordering::Relaxed);

        queue.resize(2);
        assert_eq!(queue.capacity(), 2);
        assert_eq!(queue.len(), 2); // Should have dropped one event

        queue.resize(10);
        assert_eq!(queue.capacity(), 10);
        assert_eq!(queue.len(), 2); // Should not have gained events
    }
}