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

    /// Create a performance monitor with custom history size.
    #[allow(dead_code)]
    pub fn with_history_size(max_history_size: usize) -> Self {
        Self {
            max_history_size,
            ..Self::new()
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

    /// Get current performance statistics.
    pub fn get_stats(&self) -> PerformanceStats {
        let total_events = self.total_events.load(Ordering::Relaxed);
        let total_time_nanos = self.total_processing_time.load(Ordering::Relaxed);
        let events_per_second = self.events_per_second.load(Ordering::Relaxed);
        let current_memory = self.current_memory_usage.load(Ordering::Relaxed);
        let max_memory = self.max_memory_usage.load(Ordering::Relaxed);

        let avg_processing_time_ms = if total_events > 0 {
            (total_time_nanos / total_events) as f64 / 1_000_000.0
        } else {
            0.0
        };

        let (p50, p95, p99) = self.calculate_percentiles();

        PerformanceStats {
            total_events,
            avg_processing_time_ms,
            events_per_second,
            current_memory_usage_bytes: current_memory,
            max_memory_usage_bytes: max_memory,
            p50_processing_time_ms: p50,
            p95_processing_time_ms: p95,
            p99_processing_time_ms: p99,
        }
    }

    /// Calculate processing time percentiles.
    fn calculate_percentiles(&self) -> (f64, f64, f64) {
        if self.processing_times.is_empty() {
            return (0.0, 0.0, 0.0);
        }

        let mut times: Vec<f64> = self
            .processing_times
            .iter()
            .map(|d| d.as_millis() as f64)
            .collect();
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let len = times.len();
        let p50_idx = (len as f64 * 0.5) as usize;
        let p95_idx = (len as f64 * 0.95) as usize;
        let p99_idx = (len as f64 * 0.99) as usize;

        let p50 = times.get(p50_idx).unwrap_or(&0.0);
        let p95 = times.get(p95_idx.min(len - 1)).unwrap_or(&0.0);
        let p99 = times.get(p99_idx.min(len - 1)).unwrap_or(&0.0);

        (*p50, *p95, *p99)
    }

    /// Reset all statistics.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.total_events.store(0, Ordering::Relaxed);
        self.total_processing_time.store(0, Ordering::Relaxed);
        self.processing_times.clear();
        self.events_per_second.store(0, Ordering::Relaxed);
        self.events_in_last_second.store(0, Ordering::Relaxed);
        self.max_memory_usage.store(0, Ordering::Relaxed);
        self.current_memory_usage.store(0, Ordering::Relaxed);
        self.last_calculation = Instant::now();
    }

    /// Check if performance is degraded.
    #[allow(dead_code)]
    pub fn is_performance_degraded(&self) -> bool {
        let stats = self.get_stats();

        // Consider performance degraded if:
        // - Average processing time > 100ms
        // - Events per second < 10
        // - P95 processing time > 500ms
        stats.avg_processing_time_ms > 100.0
            || stats.events_per_second < 10
            || stats.p95_processing_time_ms > 500.0
    }

    /// Get performance recommendations.
    #[allow(dead_code)]
    pub fn get_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();
        let stats = self.get_stats();

        if stats.avg_processing_time_ms > 100.0 {
            recommendations.push(
                "Average processing time is high (>100ms). Consider optimizing handlers or reducing batch sizes.".to_string()
            );
        }

        if stats.events_per_second < 10 {
            recommendations.push(
                "Low events per second rate. Consider increasing debounce delay or reducing event filtering.".to_string()
            );
        }

        if stats.p95_processing_time_ms > 500.0 {
            recommendations.push(
                "P95 processing time is very high (>500ms). Check for blocking operations in handlers.".to_string()
            );
        }

        if stats.current_memory_usage_bytes > 100 * 1024 * 1024 {
            // 100MB
            recommendations.push(
                "High memory usage detected. Consider reducing queue sizes or increasing processing frequency.".to_string()
            );
        }

        if recommendations.is_empty() {
            recommendations.push("Performance looks good.".to_string());
        }

        recommendations
    }
}

/// Performance statistics snapshot.
#[derive(Debug, Clone)]
pub struct PerformanceStats {
    /// Total events processed
    pub total_events: u64,
    /// Average processing time in milliseconds
    pub avg_processing_time_ms: f64,
    /// Events processed per second
    pub events_per_second: usize,
    /// Current memory usage in bytes
    pub current_memory_usage_bytes: usize,
    /// Maximum memory usage observed in bytes
    pub max_memory_usage_bytes: usize,
    /// 50th percentile processing time in milliseconds
    pub p50_processing_time_ms: f64,
    /// 95th percentile processing time in milliseconds
    pub p95_processing_time_ms: f64,
    /// 99th percentile processing time in milliseconds
    pub p99_processing_time_ms: f64,
}

impl PerformanceStats {
    /// Check if the statistics indicate good performance.
    pub fn is_good_performance(&self) -> bool {
        self.avg_processing_time_ms < 50.0
            && self.events_per_second > 50
            && self.p95_processing_time_ms < 200.0
            && self.current_memory_usage_bytes < 50 * 1024 * 1024 // 50MB
    }

    /// Get a performance score (0-100).
    pub fn performance_score(&self) -> u8 {
        let mut score = 100u8;

        // Penalize high processing times
        if self.avg_processing_time_ms > 100.0 {
            score = score.saturating_sub(30);
        } else if self.avg_processing_time_ms > 50.0 {
            score = score.saturating_sub(15);
        }

        // Penalize low throughput
        if self.events_per_second < 10 {
            score = score.saturating_sub(30);
        } else if self.events_per_second < 50 {
            score = score.saturating_sub(15);
        }

        // Penalize high P95 times
        if self.p95_processing_time_ms > 500.0 {
            score = score.saturating_sub(20);
        } else if self.p95_processing_time_ms > 200.0 {
            score = score.saturating_sub(10);
        }

        // Penalize high memory usage
        if self.current_memory_usage_bytes > 100 * 1024 * 1024 {
            score = score.saturating_sub(20);
        } else if self.current_memory_usage_bytes > 50 * 1024 * 1024 {
            score = score.saturating_sub(10);
        }

        score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_monitor_basic() {
        let mut monitor = PerformanceMonitor::new();

        monitor.record_event_processed(Duration::from_millis(10));
        monitor.record_event_processed(Duration::from_millis(20));
        monitor.record_event_processed(Duration::from_millis(30));

        let stats = monitor.get_stats();
        assert_eq!(stats.total_events, 3);
        assert!(stats.avg_processing_time_ms > 10.0 && stats.avg_processing_time_ms < 30.0);
    }

    #[test]
    fn test_performance_monitor_percentiles() {
        let mut monitor = PerformanceMonitor::new();

        // Add events with known processing times
        for i in 1..=100 {
            monitor.record_event_processed(Duration::from_millis(i));
        }

        let stats = monitor.get_stats();

        // P50 should be around 50ms
        assert!(stats.p50_processing_time_ms > 40.0 && stats.p50_processing_time_ms < 60.0);

        // P95 should be around 95ms
        assert!(stats.p95_processing_time_ms > 85.0 && stats.p95_processing_time_ms < 105.0);
    }

    #[test]
    fn test_performance_score() {
        let stats = PerformanceStats {
            total_events: 1000,
            avg_processing_time_ms: 25.0,
            events_per_second: 100,
            current_memory_usage_bytes: 10 * 1024 * 1024, // 10MB
            max_memory_usage_bytes: 15 * 1024 * 1024,
            p50_processing_time_ms: 20.0,
            p95_processing_time_ms: 80.0,
            p99_processing_time_ms: 150.0,
        };

        assert!(stats.is_good_performance());
        assert_eq!(stats.performance_score(), 100);
    }

    #[test]
    fn test_performance_recommendations() {
        let mut monitor = PerformanceMonitor::new();

        // Add some slow events
        for _ in 0..10 {
            monitor.record_event_processed(Duration::from_millis(150));
        }

        let recommendations = monitor.get_recommendations();
        assert!(!recommendations.is_empty());
        assert!(recommendations[0].contains("high"));
    }

    #[test]
    fn test_performance_degraded_detection() {
        let mut monitor = PerformanceMonitor::new();

        // Add very slow events
        for _ in 0..10 {
            monitor.record_event_processed(Duration::from_millis(200));
        }

        assert!(monitor.is_performance_degraded());
    }

    #[test]
    fn test_memory_usage_tracking() {
        let mut monitor = PerformanceMonitor::new();

        monitor.record_event_processed(Duration::from_millis(100)); // ~100KB estimate

        let stats = monitor.get_stats();
        assert!(stats.current_memory_usage_bytes > 0);
        assert!(stats.max_memory_usage_bytes > 0);
    }
}
