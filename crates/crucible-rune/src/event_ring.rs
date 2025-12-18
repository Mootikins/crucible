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
//! ## Overflow Handling
//!
//! When the ring buffer is full and about to overwrite old events, an optional
//! callback can be triggered to flush those events (e.g., to kiln storage).
//! Use `with_overflow_callback` to configure this behavior.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::event_ring::EventRing;
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

/// Callback type for overflow events.
///
/// Called when the ring buffer is about to overwrite events. The callback
/// receives a slice of events that are about to be evicted.
pub type OverflowCallback<E> = Arc<dyn Fn(&[Arc<E>]) + Send + Sync>;

/// Pre-allocated ring buffer for events.
///
/// Events are stored as `Arc<E>` allowing multiple handlers to reference
/// the same event without copying. The buffer wraps around when full,
/// overwriting the oldest events.
///
/// # Overflow Callbacks
///
/// When the buffer wraps around, an optional callback can be invoked to
/// flush events before they're overwritten. This enables persisting events
/// to storage (e.g., kiln markdown files) before losing them.
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
    /// Optional callback for overflow events
    overflow_callback: RwLock<Option<OverflowCallback<E>>>,
    /// Number of events to flush when overflow occurs (batch size)
    overflow_batch_size: usize,
    /// Sequence number of the last flushed event (exclusive upper bound)
    flushed_seq: AtomicU64,
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

        // Default overflow batch size: flush 1/4 of capacity at a time
        let overflow_batch_size = capacity / 4;

        Self {
            buffer: buffer.into_boxed_slice(),
            capacity,
            mask,
            write_seq: AtomicU64::new(0),
            overflow_callback: RwLock::new(None),
            overflow_batch_size,
            flushed_seq: AtomicU64::new(0),
        }
    }

    /// Set the overflow callback.
    ///
    /// This callback is invoked when the ring buffer is about to overwrite
    /// events that haven't been flushed yet. The callback receives a slice
    /// of events to be flushed.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// ring.set_overflow_callback(Arc::new(|events| {
    ///     for event in events {
    ///         persist_to_kiln(&*event);
    ///     }
    /// }));
    /// ```
    pub fn set_overflow_callback(&self, callback: OverflowCallback<E>) {
        *self.overflow_callback.write() = Some(callback);
    }

    /// Clear the overflow callback.
    pub fn clear_overflow_callback(&self) {
        *self.overflow_callback.write() = None;
    }

    /// Set the batch size for overflow flushing.
    ///
    /// When overflow occurs, this many events are flushed at once.
    /// Default is capacity / 4.
    pub fn set_overflow_batch_size(&mut self, size: usize) {
        self.overflow_batch_size = size.max(1);
    }

    /// Get the current overflow batch size.
    pub fn overflow_batch_size(&self) -> usize {
        self.overflow_batch_size
    }

    /// Get the sequence number up to which events have been flushed.
    pub fn flushed_sequence(&self) -> u64 {
        self.flushed_seq.load(Ordering::SeqCst)
    }

    /// Mark events up to (exclusive) the given sequence as flushed.
    ///
    /// This is typically called after successfully persisting events to storage.
    pub fn mark_flushed(&self, up_to_seq: u64) {
        self.flushed_seq.fetch_max(up_to_seq, Ordering::SeqCst);
    }

    /// Check if the ring is about to overflow unflushed events.
    ///
    /// Returns true if pushing a new event would overwrite an unflushed event.
    pub fn would_overflow_unflushed(&self) -> bool {
        let current = self.write_seq.load(Ordering::SeqCst);
        let flushed = self.flushed_seq.load(Ordering::SeqCst);

        // If we've written more than capacity events and the oldest
        // unflushed event would be overwritten
        if current >= self.capacity as u64 {
            let oldest_seq = current - self.capacity as u64;
            oldest_seq < flushed
        } else {
            false
        }
    }

    /// Get events that need to be flushed before they're overwritten.
    ///
    /// Returns events from flushed_seq to the oldest valid sequence that
    /// would be overwritten on the next push, up to overflow_batch_size.
    pub fn events_to_flush(&self) -> Vec<Arc<E>> {
        let current = self.write_seq.load(Ordering::SeqCst);
        let flushed = self.flushed_seq.load(Ordering::SeqCst);

        // Nothing written yet
        if current == 0 {
            return Vec::new();
        }

        // Calculate the range of events that need flushing
        let oldest_valid = if current > self.capacity as u64 {
            current - self.capacity as u64
        } else {
            0
        };

        // Start from where we left off flushing
        let start = flushed.max(oldest_valid);

        // End at current or batch size limit
        let end = (start + self.overflow_batch_size as u64).min(current);

        if start >= end {
            return Vec::new();
        }

        // Collect events
        self.range(start, end).collect()
    }

    /// Push an event into the ring buffer.
    ///
    /// Returns the sequence number assigned to this event.
    /// If the buffer is full, the oldest event is overwritten.
    ///
    /// If an overflow callback is registered and unflushed events would be
    /// overwritten, the callback is invoked with the events to flush before
    /// the new event is written.
    pub fn push(&self, event: E) -> u64 {
        // Check if we need to flush before overwriting
        self.maybe_flush_on_overflow();

        let seq = self.write_seq.fetch_add(1, Ordering::SeqCst);
        let idx = self.index(seq);

        let mut slot = self.buffer[idx].write();
        *slot = Some(Arc::new(event));

        seq
    }

    /// Flush events if overflow would lose unflushed data.
    ///
    /// This is called automatically by `push`, but can also be called
    /// manually to proactively flush events.
    pub fn maybe_flush_on_overflow(&self) {
        let current = self.write_seq.load(Ordering::SeqCst);
        let flushed = self.flushed_seq.load(Ordering::SeqCst);

        // Check if we're about to wrap around and lose unflushed events
        if current >= self.capacity as u64 {
            let would_overwrite_seq = current - self.capacity as u64;

            // If we would overwrite an unflushed event, trigger callback
            if would_overwrite_seq >= flushed {
                // We have unflushed events that would be lost
                if let Some(callback) = self.overflow_callback.read().as_ref() {
                    let events = self.events_to_flush();
                    if !events.is_empty() {
                        callback(&events);
                        // Mark as flushed
                        let new_flushed = flushed + events.len() as u64;
                        self.flushed_seq.store(new_flushed, Ordering::SeqCst);
                    }
                }
            }
        }
    }

    /// Force flush all unflushed events up to the current write position.
    ///
    /// This is useful for explicit persistence without waiting for overflow.
    /// Returns the number of events flushed.
    pub fn flush_all_unflushed(&self) -> usize {
        let current = self.write_seq.load(Ordering::SeqCst);
        let flushed = self.flushed_seq.load(Ordering::SeqCst);

        if flushed >= current {
            return 0;
        }

        let oldest_valid = if current > self.capacity as u64 {
            current - self.capacity as u64
        } else {
            0
        };

        let start = flushed.max(oldest_valid);
        let events: Vec<_> = self.range(start, current).collect();

        if events.is_empty() {
            return 0;
        }

        if let Some(callback) = self.overflow_callback.read().as_ref() {
            callback(&events);
        }

        let count = events.len();
        self.flushed_seq.store(current, Ordering::SeqCst);
        count
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
        } else if current <= self.capacity as u64 {
            0
        } else {
            current - self.capacity as u64
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

    // ─────────────────────────────────────────────────────────────────────────
    // Overflow callback tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_overflow_callback_set_and_clear() {
        let ring: EventRing<i32> = EventRing::new(4);

        let flushed = Arc::new(std::sync::Mutex::new(Vec::new()));
        let flushed_clone = Arc::clone(&flushed);

        ring.set_overflow_callback(Arc::new(move |events: &[Arc<i32>]| {
            let mut f = flushed_clone.lock().unwrap();
            for e in events {
                f.push(**e);
            }
        }));

        ring.clear_overflow_callback();

        // Fill and overflow without triggering callback
        for i in 0..8 {
            ring.push(i);
        }

        let flushed = flushed.lock().unwrap();
        assert!(
            flushed.is_empty(),
            "Callback should not be called after clear"
        );
    }

    #[test]
    fn test_overflow_callback_triggered_on_wrap() {
        let ring: EventRing<i32> = EventRing::new(4);

        let flushed = Arc::new(std::sync::Mutex::new(Vec::new()));
        let flushed_clone = Arc::clone(&flushed);

        ring.set_overflow_callback(Arc::new(move |events: &[Arc<i32>]| {
            let mut f = flushed_clone.lock().unwrap();
            for e in events {
                f.push(**e);
            }
        }));

        // Fill buffer (0, 1, 2, 3) - no overflow yet
        for i in 0..4 {
            ring.push(i);
        }

        let flushed_before = flushed.lock().unwrap().len();
        assert_eq!(flushed_before, 0, "No overflow yet");

        // Now push more - this should trigger overflow callback
        ring.push(4); // Would overwrite 0, but flushed_seq=0, so flush events first

        let flushed_after = flushed.lock().unwrap();
        assert!(
            !flushed_after.is_empty(),
            "Overflow callback should have been called"
        );
        // Should have flushed event 0 (the one about to be overwritten)
        assert!(
            flushed_after.contains(&0),
            "Event 0 should have been flushed"
        );
    }

    #[test]
    fn test_mark_flushed_prevents_callback() {
        let ring: EventRing<i32> = EventRing::new(4);

        let callback_count = Arc::new(AtomicU64::new(0));
        let callback_count_clone = Arc::clone(&callback_count);

        ring.set_overflow_callback(Arc::new(move |_events: &[Arc<i32>]| {
            callback_count_clone.fetch_add(1, Ordering::SeqCst);
        }));

        // Fill buffer
        for i in 0..4 {
            ring.push(i);
        }

        // Mark all as flushed
        ring.mark_flushed(4);

        // Now overflow - callback should NOT be called
        ring.push(100);

        assert_eq!(
            callback_count.load(Ordering::SeqCst),
            0,
            "Callback should not be called when events are already flushed"
        );
    }

    #[test]
    fn test_flushed_sequence() {
        let ring: EventRing<i32> = EventRing::new(8);

        assert_eq!(ring.flushed_sequence(), 0);

        ring.mark_flushed(5);
        assert_eq!(ring.flushed_sequence(), 5);

        // mark_flushed uses fetch_max, so only increases
        ring.mark_flushed(3);
        assert_eq!(ring.flushed_sequence(), 5);

        ring.mark_flushed(10);
        assert_eq!(ring.flushed_sequence(), 10);
    }

    #[test]
    fn test_events_to_flush() {
        let ring: EventRing<i32> = EventRing::new(4);

        // Empty ring
        assert!(ring.events_to_flush().is_empty());

        // Add some events
        for i in 0..4 {
            ring.push(i);
        }

        // All events are unflushed
        let to_flush = ring.events_to_flush();
        assert!(!to_flush.is_empty());
        assert_eq!(*to_flush[0], 0);

        // Mark some as flushed
        ring.mark_flushed(2);

        let to_flush = ring.events_to_flush();
        // Now should start from 2
        assert_eq!(*to_flush[0], 2);
    }

    #[test]
    fn test_flush_all_unflushed() {
        let ring: EventRing<i32> = EventRing::new(8);

        let flushed = Arc::new(std::sync::Mutex::new(Vec::new()));
        let flushed_clone = Arc::clone(&flushed);

        ring.set_overflow_callback(Arc::new(move |events: &[Arc<i32>]| {
            let mut f = flushed_clone.lock().unwrap();
            for e in events {
                f.push(**e);
            }
        }));

        // Add events
        for i in 0..5 {
            ring.push(i);
        }

        // Force flush all
        let count = ring.flush_all_unflushed();
        assert_eq!(count, 5);

        let flushed = flushed.lock().unwrap();
        assert_eq!(*flushed, vec![0, 1, 2, 3, 4]);

        // flushed_seq should be updated
        assert_eq!(ring.flushed_sequence(), 5);
    }

    #[test]
    fn test_flush_all_unflushed_no_callback() {
        let ring: EventRing<i32> = EventRing::new(8);

        for i in 0..5 {
            ring.push(i);
        }

        // No callback set - should still update flushed_seq
        let count = ring.flush_all_unflushed();
        assert_eq!(count, 5);
        assert_eq!(ring.flushed_sequence(), 5);
    }

    #[test]
    fn test_overflow_batch_size() {
        let mut ring: EventRing<i32> = EventRing::new(16);

        // Default batch size is capacity / 4 = 4
        assert_eq!(ring.overflow_batch_size(), 4);

        ring.set_overflow_batch_size(2);
        assert_eq!(ring.overflow_batch_size(), 2);

        // Minimum is 1
        ring.set_overflow_batch_size(0);
        assert_eq!(ring.overflow_batch_size(), 1);
    }

    #[test]
    fn test_multiple_overflow_flushes() {
        let ring: EventRing<i32> = EventRing::new(4);

        let flushed = Arc::new(std::sync::Mutex::new(Vec::new()));
        let flushed_clone = Arc::clone(&flushed);

        ring.set_overflow_callback(Arc::new(move |events: &[Arc<i32>]| {
            let mut f = flushed_clone.lock().unwrap();
            for e in events {
                f.push(**e);
            }
        }));

        // Push many events to cause multiple overflows
        for i in 0..12 {
            ring.push(i);
        }

        let flushed = flushed.lock().unwrap();
        // Should have flushed events as they were about to be overwritten
        // Events 0-7 should have been flushed (8-11 are current)
        for i in 0..8 {
            assert!(flushed.contains(&i), "Event {i} should have been flushed");
        }
    }
}
