//! Event Ring Buffer for Session Events
//!
//! A minimal Disruptor-style ring buffer for event processing. Events are stored
//! in a pre-allocated buffer and accessed via `Arc<E>` references (cheap clone).
//!
//! ## Design
//!
//! The ring buffer serves as an in-memory event log, enabling:
//! - Multiple handlers to read the same event without copies
//! - Event replay for debugging and recovery
//! - Efficient history queries
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_core::events::EventRing;
//!
//! let ring: EventRing<String> = EventRing::new(1024);
//!
//! // Push events
//! let seq1 = ring.push("event1".to_string());
//! let seq2 = ring.push("event2".to_string());
//!
//! // Get event by sequence number (returns Arc<E>)
//! if let Some(event) = ring.get(seq1) {
//!     println!("Event: {}", *event);
//! }
//!
//! // Replay a range of events
//! for event in ring.range(seq1, seq2 + 1) {
//!     println!("Replaying: {}", *event);
//! }
//! ```

use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Pre-allocated ring buffer for events.
///
/// Events are stored as `Arc<E>` allowing multiple handlers to reference
/// the same event without copying. The buffer wraps around when full,
/// overwriting the oldest events.
///
/// # Thread Safety
///
/// The ring buffer is thread-safe:
/// - `push` uses atomic sequence numbers for ordering
/// - `get` and `range` can be called concurrently with `push`
/// - Old events may be overwritten during concurrent access
///
/// # Capacity
///
/// Capacity is always rounded up to the next power of two for efficient
/// index calculation using bitwise AND instead of modulo.
pub struct EventRing<E> {
    /// Pre-allocated buffer slots
    buffer: Box<[RwLock<Option<Arc<E>>>]>,
    /// Capacity (always power of two)
    capacity: usize,
    /// Bitmask for efficient index calculation (capacity - 1)
    mask: usize,
    /// Next sequence number to write
    write_seq: AtomicU64,
}

impl<E> EventRing<E> {
    /// Create a new ring buffer with the given capacity.
    ///
    /// Capacity is rounded up to the next power of two.
    ///
    /// # Panics
    ///
    /// Panics if capacity is 0.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "EventRing capacity must be > 0");

        // Round up to next power of two
        let capacity = capacity.next_power_of_two();
        let mask = capacity - 1;

        // Pre-allocate buffer with None values
        let buffer: Vec<RwLock<Option<Arc<E>>>> =
            (0..capacity).map(|_| RwLock::new(None)).collect();

        Self {
            buffer: buffer.into_boxed_slice(),
            capacity,
            mask,
            write_seq: AtomicU64::new(0),
        }
    }

    /// Push an event into the ring buffer.
    ///
    /// Returns the sequence number assigned to this event.
    /// If the buffer is full, the oldest event is overwritten.
    pub fn push(&self, event: E) -> u64 {
        let seq = self.write_seq.fetch_add(1, Ordering::SeqCst);
        let idx = self.index(seq);

        let mut slot = self.buffer[idx].write();
        *slot = Some(Arc::new(event));

        seq
    }

    /// Get an event by sequence number.
    ///
    /// Returns `None` if:
    /// - The sequence number has not been written yet
    /// - The event has been overwritten (wrapped around)
    ///
    /// Returns `Some(Arc<E>)` - cloning the Arc is cheap.
    pub fn get(&self, seq: u64) -> Option<Arc<E>> {
        // Check if sequence is in valid range
        let current = self.write_seq.load(Ordering::SeqCst);

        // Not yet written
        if seq >= current {
            return None;
        }

        // Check if overwritten (wrapped around)
        if current > self.capacity as u64 && seq < current - self.capacity as u64 {
            return None;
        }

        let idx = self.index(seq);
        let slot = self.buffer[idx].read();
        slot.clone()
    }

    /// Iterate over events in the range [from, to).
    ///
    /// Events that have been overwritten are skipped.
    /// This is a "best effort" replay - concurrent writes may affect results.
    pub fn range(&self, from: u64, to: u64) -> impl Iterator<Item = Arc<E>> + '_ {
        (from..to).filter_map(move |seq| self.get(seq))
    }

    /// Get the current write sequence number.
    ///
    /// This is the next sequence number that will be assigned.
    pub fn write_sequence(&self) -> u64 {
        self.write_seq.load(Ordering::SeqCst)
    }

    /// Get the capacity of the ring buffer.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the number of events currently in the buffer.
    ///
    /// This is min(write_seq, capacity).
    pub fn len(&self) -> usize {
        let seq = self.write_seq.load(Ordering::SeqCst) as usize;
        seq.min(self.capacity)
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.write_seq.load(Ordering::SeqCst) == 0
    }

    /// Get the oldest valid sequence number.
    ///
    /// Returns the sequence number of the oldest event that hasn't been
    /// overwritten, or 0 if no events have been written.
    pub fn oldest_sequence(&self) -> u64 {
        let current = self.write_seq.load(Ordering::SeqCst);
        if current == 0 {
            0
        } else {
            current.saturating_sub(self.capacity as u64)
        }
    }

    /// Get the newest valid sequence number.
    ///
    /// Returns `None` if no events have been written.
    pub fn newest_sequence(&self) -> Option<u64> {
        let current = self.write_seq.load(Ordering::SeqCst);
        if current == 0 {
            None
        } else {
            Some(current - 1)
        }
    }

    /// Iterate over all valid events from oldest to newest.
    pub fn iter(&self) -> impl Iterator<Item = Arc<E>> + '_ {
        let oldest = self.oldest_sequence();
        let newest = self.write_seq.load(Ordering::SeqCst);
        self.range(oldest, newest)
    }

    /// Calculate buffer index from sequence number.
    #[inline]
    fn index(&self, seq: u64) -> usize {
        (seq as usize) & self.mask
    }
}

impl<E> std::fmt::Debug for EventRing<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventRing")
            .field("capacity", &self.capacity)
            .field("write_seq", &self.write_seq.load(Ordering::SeqCst))
            .field("len", &self.len())
            .finish()
    }
}

// EventRing is Send + Sync if E is Send + Sync
unsafe impl<E: Send + Sync> Send for EventRing<E> {}
unsafe impl<E: Send + Sync> Sync for EventRing<E> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_rounds_to_power_of_two() {
        let ring: EventRing<i32> = EventRing::new(100);
        assert_eq!(ring.capacity(), 128); // Next power of two

        let ring: EventRing<i32> = EventRing::new(64);
        assert_eq!(ring.capacity(), 64); // Already power of two

        let ring: EventRing<i32> = EventRing::new(1);
        assert_eq!(ring.capacity(), 1);
    }

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn test_new_zero_capacity_panics() {
        let _: EventRing<i32> = EventRing::new(0);
    }

    #[test]
    fn test_push_and_get() {
        let ring: EventRing<String> = EventRing::new(8);

        let seq0 = ring.push("event0".to_string());
        let seq1 = ring.push("event1".to_string());
        let seq2 = ring.push("event2".to_string());

        assert_eq!(seq0, 0);
        assert_eq!(seq1, 1);
        assert_eq!(seq2, 2);

        assert_eq!(*ring.get(seq0).unwrap(), "event0");
        assert_eq!(*ring.get(seq1).unwrap(), "event1");
        assert_eq!(*ring.get(seq2).unwrap(), "event2");
    }

    #[test]
    fn test_get_unwritten_returns_none() {
        let ring: EventRing<i32> = EventRing::new(8);

        ring.push(1);
        ring.push(2);

        // Sequence 5 not written yet
        assert!(ring.get(5).is_none());
    }

    #[test]
    fn test_wrap_around() {
        let ring: EventRing<i32> = EventRing::new(4);

        // Fill buffer
        for i in 0..4 {
            ring.push(i);
        }

        assert_eq!(*ring.get(0).unwrap(), 0);
        assert_eq!(*ring.get(3).unwrap(), 3);

        // Overwrite oldest
        ring.push(100);
        ring.push(101);

        // Old events should be gone
        assert!(ring.get(0).is_none());
        assert!(ring.get(1).is_none());

        // New events accessible
        assert_eq!(*ring.get(4).unwrap(), 100);
        assert_eq!(*ring.get(5).unwrap(), 101);

        // Events 2, 3 still there
        assert_eq!(*ring.get(2).unwrap(), 2);
        assert_eq!(*ring.get(3).unwrap(), 3);
    }

    #[test]
    fn test_range() {
        let ring: EventRing<i32> = EventRing::new(8);

        for i in 0..5 {
            ring.push(i);
        }

        let events: Vec<i32> = ring.range(1, 4).map(|arc| *arc).collect();
        assert_eq!(events, vec![1, 2, 3]);
    }

    #[test]
    fn test_range_skips_overwritten() {
        let ring: EventRing<i32> = EventRing::new(4);

        // Fill and overflow
        for i in 0..6 {
            ring.push(i);
        }

        // Range 0..6 but 0, 1 are overwritten
        let events: Vec<i32> = ring.range(0, 6).map(|arc| *arc).collect();
        assert_eq!(events, vec![2, 3, 4, 5]);
    }

    #[test]
    fn test_write_sequence() {
        let ring: EventRing<i32> = EventRing::new(8);

        assert_eq!(ring.write_sequence(), 0);

        ring.push(1);
        assert_eq!(ring.write_sequence(), 1);

        ring.push(2);
        ring.push(3);
        assert_eq!(ring.write_sequence(), 3);
    }

    #[test]
    fn test_len_and_is_empty() {
        let ring: EventRing<i32> = EventRing::new(4);

        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);

        ring.push(1);
        assert!(!ring.is_empty());
        assert_eq!(ring.len(), 1);

        ring.push(2);
        ring.push(3);
        ring.push(4);
        assert_eq!(ring.len(), 4);

        // Overflow
        ring.push(5);
        ring.push(6);
        assert_eq!(ring.len(), 4); // Still capped at capacity
    }

    #[test]
    fn test_oldest_and_newest_sequence() {
        let ring: EventRing<i32> = EventRing::new(4);

        assert_eq!(ring.oldest_sequence(), 0);
        assert_eq!(ring.newest_sequence(), None);

        ring.push(1);
        assert_eq!(ring.oldest_sequence(), 0);
        assert_eq!(ring.newest_sequence(), Some(0));

        ring.push(2);
        ring.push(3);
        ring.push(4);
        assert_eq!(ring.oldest_sequence(), 0);
        assert_eq!(ring.newest_sequence(), Some(3));

        // Overflow - oldest moves forward
        ring.push(5);
        ring.push(6);
        assert_eq!(ring.oldest_sequence(), 2);
        assert_eq!(ring.newest_sequence(), Some(5));
    }

    #[test]
    fn test_iter() {
        let ring: EventRing<i32> = EventRing::new(4);

        for i in 0..6 {
            ring.push(i);
        }

        // Should only iterate valid events (2, 3, 4, 5)
        let events: Vec<i32> = ring.iter().map(|arc| *arc).collect();
        assert_eq!(events, vec![2, 3, 4, 5]);
    }

    #[test]
    fn test_arc_cheap_clone() {
        let ring: EventRing<String> = EventRing::new(8);

        ring.push("hello".to_string());

        let arc1 = ring.get(0).unwrap();
        let arc2 = ring.get(0).unwrap();

        // Both Arcs point to same data
        assert!(Arc::ptr_eq(&arc1, &arc2));
    }

    #[test]
    fn test_debug_impl() {
        let ring: EventRing<i32> = EventRing::new(8);
        ring.push(1);
        ring.push(2);

        let debug = format!("{:?}", ring);
        assert!(debug.contains("EventRing"));
        assert!(debug.contains("capacity: 8"));
        assert!(debug.contains("write_seq: 2"));
        assert!(debug.contains("len: 2"));
    }

    #[test]
    fn test_concurrent_push_and_get() {
        use std::thread;

        let ring = Arc::new(EventRing::new(1024));

        // Spawn writer thread
        let ring_writer = Arc::clone(&ring);
        let writer = thread::spawn(move || {
            for i in 0..500 {
                ring_writer.push(i);
            }
        });

        // Spawn reader thread
        let ring_reader = Arc::clone(&ring);
        let reader = thread::spawn(move || {
            let mut read_count = 0;
            for seq in 0..500u64 {
                // Try multiple times as writer may not have caught up
                for _ in 0..100 {
                    if ring_reader.get(seq).is_some() {
                        read_count += 1;
                        break;
                    }
                    thread::yield_now();
                }
            }
            read_count
        });

        writer.join().unwrap();
        let read_count = reader.join().unwrap();

        // Should have read most events (timing dependent, but should get many)
        assert!(read_count > 400, "Only read {read_count} events");
    }
}
