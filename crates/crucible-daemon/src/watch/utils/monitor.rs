//! Performance monitoring for the file watching system.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Performance monitor for tracking file watching metrics.
pub struct PerformanceMonitor {
    /// Total events processed
    total_events: AtomicU64,
    /// Total processing time
    total_processing_time: AtomicU64, // Stored in nanoseconds
    /// Event processing times history
    processing_times: VecDeque<Duration>,
    /// Maximum size of history
    max_history_size: usize,
    /// Events processed per second
    events_per_second: AtomicUsize,
    /// Last calculation time
    last_calculation: Instant,
    /// Events in the last second
    events_in_last_second: AtomicUsize,
    /// Maximum memory usage observed
    max_memory_usage: AtomicUsize,
    /// Current memory usage estimate
    current_memory_usage: AtomicUsize,
}

impl PerformanceMonitor {
    /// Create a new performance monitor.
    pub fn new() -> Self {
        Self {
            total_events: AtomicU64::new(0),
            total_processing_time: AtomicU64::new(0),
            processing_times: VecDeque::with_capacity(1000),
            max_history_size: 1000,
            events_per_second: AtomicUsize::new(0),
            last_calculation: Instant::now(),
            events_in_last_second: AtomicUsize::new(0),
            max_memory_usage: AtomicUsize::new(0),
            current_memory_usage: AtomicUsize::new(0),
        }
    }

    /// Record that an event was processed.
    pub fn record_event_processed(&mut self, processing_time: Duration) {
        let processing_time_nanos = processing_time.as_nanos() as u64;
        self.total_events.fetch_add(1, Ordering::Relaxed);
        self.total_processing_time
            .fetch_add(processing_time_nanos, Ordering::Relaxed);

        // Add to history
        self.processing_times.push_back(processing_time);
        if self.processing_times.len() > self.max_history_size {
            self.processing_times.pop_front();
        }

        // Update events per second
        self.update_events_per_second();

        // Update memory usage estimate
        self.update_memory_usage(processing_time);
    }

    /// Update events per second calculation.
    fn update_events_per_second(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_calculation) >= Duration::from_secs(1) {
            let events = self.events_in_last_second.swap(0, Ordering::Relaxed);
            self.events_per_second.store(events, Ordering::Relaxed);
            self.last_calculation = now;
        }
        self.events_in_last_second.fetch_add(1, Ordering::Relaxed);
    }

    /// Update memory usage estimate.
    fn update_memory_usage(&mut self, processing_time: Duration) {
        // Simple heuristic: processing time correlates with memory usage
        let estimated_usage = (processing_time.as_millis() as usize) * 1024; // 1KB per ms
        self.current_memory_usage
            .store(estimated_usage, Ordering::Relaxed);

        let current_max = self.max_memory_usage.load(Ordering::Relaxed);
        if estimated_usage > current_max {
            self.max_memory_usage
                .store(estimated_usage, Ordering::Relaxed);
        }
    }
}
