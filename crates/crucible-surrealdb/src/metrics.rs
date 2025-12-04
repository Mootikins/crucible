//! Essential Metrics Collection for Queue-Based Database Architecture
//!
//! This module provides centralized metrics collection for monitoring the health
//! and performance of the queue-based database system.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tracing::debug;

/// Health status levels for the system
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    /// System is operating normally
    Healthy,
    /// System has some issues but is still functional
    Degraded,
    /// System has serious issues and may not be functional
    Critical,
}

/// Comprehensive health report
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// Current health status
    pub status: HealthStatus,
    /// Whether the system is considered healthy (bool for backward compatibility)
    pub is_healthy: bool,
    /// System uptime
    pub uptime: Duration,
    /// Current metrics snapshot
    pub metrics: SystemMetricsSnapshot,
    /// When the report was generated
    pub timestamp: Instant,
    /// Recommendations based on current state
    pub recommendations: Vec<String>,
}

/// Centralized metrics collector for the database system
#[derive(Debug)]
pub struct SystemMetrics {
    /// Queue depth (current number of queued transactions)
    pub queue_depth: AtomicU64,

    /// Total number of transactions processed
    pub total_processed: AtomicU64,

    /// Number of successful transactions
    pub successful_transactions: AtomicU64,

    /// Number of failed transactions
    pub failed_transactions: AtomicU64,

    /// Total processing time in milliseconds (for calculating average)
    pub total_processing_time_ms: AtomicU64,

    /// Current processing rate (transactions per second) - uses RwLock for f64
    pub processing_rate_tps: RwLock<f64>,

    /// Current error rate (percentage) - uses RwLock for f64
    pub error_rate_percent: RwLock<f64>,

    /// Timestamp when metrics collection started
    start_time: Instant,

    /// Channel sender for broadcasting metrics updates
    metrics_sender: watch::Sender<SystemMetricsSnapshot>,
}

/// Snapshot of current system metrics
#[derive(Debug, Clone)]
pub struct SystemMetricsSnapshot {
    /// Current queue depth
    pub queue_depth: u64,

    /// Total number of transactions processed
    pub total_processed: u64,

    /// Number of successful transactions
    pub successful_transactions: u64,

    /// Number of failed transactions
    pub failed_transactions: u64,

    /// Average processing time in milliseconds
    pub avg_processing_time_ms: f64,

    /// Current processing rate (transactions per second)
    pub processing_rate_tps: f64,

    /// Current error rate (percentage)
    pub error_rate_percent: f64,

    /// System uptime
    pub uptime: Duration,

    /// Current timestamp
    pub timestamp: Instant,
}

impl Default for SystemMetrics {
    fn default() -> Self {
        let (metrics_sender, _) = watch::channel(SystemMetricsSnapshot::default());

        Self {
            queue_depth: AtomicU64::new(0),
            total_processed: AtomicU64::new(0),
            successful_transactions: AtomicU64::new(0),
            failed_transactions: AtomicU64::new(0),
            total_processing_time_ms: AtomicU64::new(0),
            processing_rate_tps: RwLock::new(0.0),
            error_rate_percent: RwLock::new(0.0),
            start_time: Instant::now(),
            metrics_sender,
        }
    }
}

impl Default for SystemMetricsSnapshot {
    fn default() -> Self {
        Self {
            queue_depth: 0,
            total_processed: 0,
            successful_transactions: 0,
            failed_transactions: 0,
            avg_processing_time_ms: 0.0,
            processing_rate_tps: 0.0,
            error_rate_percent: 0.0,
            uptime: Duration::from_secs(0),
            timestamp: Instant::now(),
        }
    }
}

impl SystemMetrics {
    /// Create a new metrics collector
    pub fn new() -> Arc<Self> {
        let metrics = Arc::new(Self::default());

        // Start background task to periodically update calculated metrics
        tokio::spawn({
            let metrics_clone = Arc::clone(&metrics);
            Self::start_metrics_updater(metrics_clone)
        });

        metrics
    }

    /// Record a successful transaction
    pub fn record_success(&self, processing_time_ms: u64) {
        self.total_processed.fetch_add(1, Ordering::Relaxed);
        self.successful_transactions.fetch_add(1, Ordering::Relaxed);
        self.total_processing_time_ms
            .fetch_add(processing_time_ms, Ordering::Relaxed);

        debug!("Recorded successful transaction: {}ms", processing_time_ms);
    }

    /// Record a failed transaction
    pub fn record_failure(&self, processing_time_ms: u64) {
        self.total_processed.fetch_add(1, Ordering::Relaxed);
        self.failed_transactions.fetch_add(1, Ordering::Relaxed);
        self.total_processing_time_ms
            .fetch_add(processing_time_ms, Ordering::Relaxed);

        debug!("Recorded failed transaction: {}ms", processing_time_ms);
    }

    /// Update queue depth
    pub fn update_queue_depth(&self, depth: u64) {
        self.queue_depth.store(depth, Ordering::Relaxed);
    }

    /// Get current metrics snapshot
    pub fn get_snapshot(&self) -> SystemMetricsSnapshot {
        let total_processed = self.total_processed.load(Ordering::Relaxed);
        let successful = self.successful_transactions.load(Ordering::Relaxed);
        let failed = self.failed_transactions.load(Ordering::Relaxed);
        let total_time = self.total_processing_time_ms.load(Ordering::Relaxed);

        let avg_processing_time = if total_processed > 0 {
            total_time as f64 / total_processed as f64
        } else {
            0.0
        };

        let error_rate = if total_processed > 0 {
            (failed as f64 / total_processed as f64) * 100.0
        } else {
            0.0
        };

        SystemMetricsSnapshot {
            queue_depth: self.queue_depth.load(Ordering::Relaxed),
            total_processed,
            successful_transactions: successful,
            failed_transactions: failed,
            avg_processing_time_ms: avg_processing_time,
            processing_rate_tps: *self.processing_rate_tps.read().unwrap(),
            error_rate_percent: error_rate,
            uptime: self.start_time.elapsed(),
            timestamp: Instant::now(),
        }
    }

    /// Subscribe to metrics updates
    pub fn subscribe(&self) -> watch::Receiver<SystemMetricsSnapshot> {
        self.metrics_sender.subscribe()
    }

    /// Start background metrics updater
    async fn start_metrics_updater(metrics: Arc<Self>) {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        let mut last_processed = 0u64;
        let mut last_update_time = Instant::now();

        loop {
            interval.tick().await;

            let current_time = Instant::now();
            let current_processed = metrics.total_processed.load(Ordering::Relaxed);

            // Calculate processing rate
            let time_diff = current_time.duration_since(last_update_time).as_secs_f64();
            let processing_diff = current_processed.saturating_sub(last_processed);

            let processing_rate = if time_diff > 0.0 {
                processing_diff as f64 / time_diff
            } else {
                0.0
            };

            *metrics.processing_rate_tps.write().unwrap() = processing_rate;

            // Update error rate
            let total = current_processed;
            let failed = metrics.failed_transactions.load(Ordering::Relaxed);

            let error_rate = if total > 0 {
                (failed as f64 / total as f64) * 100.0
            } else {
                0.0
            };

            *metrics.error_rate_percent.write().unwrap() = error_rate;

            // Send updated snapshot
            let snapshot = metrics.get_snapshot();
            if metrics.metrics_sender.send(snapshot).is_err() {
                debug!("Metrics update channel closed - stopping updater");
                break;
            }

            last_processed = current_processed;
            last_update_time = current_time;
        }
    }

    /// Get formatted metrics summary
    pub fn get_formatted_summary(&self) -> String {
        let snapshot = self.get_snapshot();

        format!(
            "System Metrics Summary:\n\
             ├─ Queue Depth: {}\n\
             ├─ Total Processed: {}\n\
             ├─ Successful: {}\n\
             ├─ Failed: {}\n\
             ├─ Success Rate: {:.1}%\n\
             ├─ Error Rate: {:.1}%\n\
             ├─ Avg Processing Time: {:.1}ms\n\
             ├─ Processing Rate: {:.1} TPS\n\
             └─ Uptime: {:?}",
            snapshot.queue_depth,
            snapshot.total_processed,
            snapshot.successful_transactions,
            snapshot.failed_transactions,
            100.0 - snapshot.error_rate_percent,
            snapshot.error_rate_percent,
            snapshot.avg_processing_time_ms,
            snapshot.processing_rate_tps,
            snapshot.uptime
        )
    }

    /// Check if system is healthy (comprehensive health check)
    pub fn is_healthy(&self) -> bool {
        let (_, is_healthy) = self.get_health_status();
        is_healthy
    }

    /// Get comprehensive health status with detailed information
    pub fn get_health_status(&self) -> (HealthStatus, bool) {
        let snapshot = self.get_snapshot();
        let mut issues = Vec::new();
        let mut status = HealthStatus::Healthy;

        // Check error rate
        if snapshot.error_rate_percent >= 50.0 {
            issues.push(format!(
                "High error rate: {:.1}%",
                snapshot.error_rate_percent
            ));
            status = HealthStatus::Critical;
        } else if snapshot.error_rate_percent >= 20.0 {
            issues.push(format!(
                "Elevated error rate: {:.1}%",
                snapshot.error_rate_percent
            ));
            if status == HealthStatus::Healthy {
                status = HealthStatus::Degraded;
            }
        }

        // Check processing time
        if snapshot.avg_processing_time_ms >= 10000.0 {
            issues.push(format!(
                "Very slow processing: {:.1}ms average",
                snapshot.avg_processing_time_ms
            ));
            status = HealthStatus::Critical;
        } else if snapshot.avg_processing_time_ms >= 5000.0 {
            issues.push(format!(
                "Slow processing: {:.1}ms average",
                snapshot.avg_processing_time_ms
            ));
            if status == HealthStatus::Healthy {
                status = HealthStatus::Degraded;
            }
        }

        // Check processing rate
        if snapshot.processing_rate_tps < 0.0 {
            issues.push("Negative processing rate detected".to_string());
            status = HealthStatus::Critical;
        } else if snapshot.total_processed > 100 && snapshot.processing_rate_tps < 0.1 {
            issues.push(format!(
                "Very low processing rate: {:.2} TPS",
                snapshot.processing_rate_tps
            ));
            if status == HealthStatus::Healthy {
                status = HealthStatus::Degraded;
            }
        }

        // Check queue depth
        if snapshot.queue_depth > 1000 {
            issues.push(format!("High queue depth: {}", snapshot.queue_depth));
            status = HealthStatus::Critical;
        } else if snapshot.queue_depth > 500 {
            issues.push(format!("Elevated queue depth: {}", snapshot.queue_depth));
            if status == HealthStatus::Healthy {
                status = HealthStatus::Degraded;
            }
        }

        let _status_message = if issues.is_empty() {
            "All systems operational".to_string()
        } else {
            issues.join("; ")
        };

        (status.clone(), status == HealthStatus::Healthy)
    }

    /// Get detailed health report
    pub fn get_health_report(&self) -> HealthReport {
        let snapshot = self.get_snapshot();
        let (status, is_healthy) = self.get_health_status();

        HealthReport {
            status: status.clone(),
            is_healthy,
            uptime: snapshot.uptime,
            metrics: snapshot.clone(),
            timestamp: snapshot.timestamp,
            recommendations: self.generate_health_recommendations(&snapshot, status),
        }
    }

    /// Generate health recommendations based on current state
    fn generate_health_recommendations(
        &self,
        snapshot: &SystemMetricsSnapshot,
        status: HealthStatus,
    ) -> Vec<String> {
        let mut recommendations = Vec::new();

        match status {
            HealthStatus::Critical => {
                recommendations.push("Immediate investigation required".to_string());
                if snapshot.error_rate_percent >= 50.0 {
                    recommendations
                        .push("Check database connectivity and logs for errors".to_string());
                }
                if snapshot.avg_processing_time_ms >= 10000.0 {
                    recommendations.push(
                        "Consider scaling database resources or optimizing queries".to_string(),
                    );
                }
                if snapshot.queue_depth > 1000 {
                    recommendations
                        .push("Pause new transactions and increase consumer capacity".to_string());
                }
            }
            HealthStatus::Degraded => {
                recommendations.push("Monitor system closely".to_string());
                if snapshot.error_rate_percent >= 20.0 {
                    recommendations.push("Review recent error patterns".to_string());
                }
                if snapshot.avg_processing_time_ms >= 5000.0 {
                    recommendations.push("Consider performance optimization".to_string());
                }
                if snapshot.queue_depth > 500 {
                    recommendations.push("Monitor queue growth rate".to_string());
                }
            }
            HealthStatus::Healthy => {
                if snapshot.total_processed > 0 {
                    recommendations.push("System operating normally".to_string());
                } else {
                    recommendations.push("No transactions processed yet".to_string());
                }
            }
        }

        recommendations
    }
}

/// Global metrics instance using OnceLock for thread-safe lazy initialization
static GLOBAL_METRICS: std::sync::OnceLock<Arc<SystemMetrics>> = std::sync::OnceLock::new();

/// Get or create global metrics instance
pub fn get_global_metrics() -> Arc<SystemMetrics> {
    GLOBAL_METRICS.get_or_init(SystemMetrics::new).clone()
}

/// Convenience functions for global metrics
pub fn record_transaction_success(processing_time_ms: u64) {
    get_global_metrics().record_success(processing_time_ms);
}

pub fn record_transaction_failure(processing_time_ms: u64) {
    get_global_metrics().record_failure(processing_time_ms);
}

pub fn update_queue_depth(depth: u64) {
    get_global_metrics().update_queue_depth(depth);
}

pub fn get_system_health() -> (bool, String) {
    let metrics = get_global_metrics();
    let (status, is_healthy) = metrics.get_health_status();
    let status_message = match status {
        HealthStatus::Healthy => "System is healthy".to_string(),
        HealthStatus::Degraded => "System performance is degraded".to_string(),
        HealthStatus::Critical => "System has critical issues".to_string(),
    };
    (is_healthy, status_message)
}

/// Get comprehensive health report
pub fn get_system_health_report() -> HealthReport {
    get_global_metrics().get_health_report()
}

/// Get current health status
pub fn get_health_status() -> HealthStatus {
    let (status, _) = get_global_metrics().get_health_status();
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_metrics_collection() {
        let metrics = SystemMetrics::new();

        // Record some transactions
        metrics.record_success(100);
        metrics.record_success(200);
        metrics.record_failure(150);

        // Wait a bit for async updates
        sleep(Duration::from_millis(100)).await;

        let snapshot = metrics.get_snapshot();

        assert_eq!(snapshot.total_processed, 3);
        assert_eq!(snapshot.successful_transactions, 2);
        assert_eq!(snapshot.failed_transactions, 1);
        assert_eq!(snapshot.avg_processing_time_ms, 150.0);
        assert!((snapshot.error_rate_percent - 33.33).abs() < 0.1);
    }

    #[tokio::test]
    async fn test_health_check() {
        let metrics = SystemMetrics::new();

        // Initially healthy
        assert!(metrics.is_healthy());

        // Add many failures to make it unhealthy
        for _ in 0..10 {
            metrics.record_failure(100);
        }

        sleep(Duration::from_millis(100)).await;

        // Should be unhealthy due to high error rate
        assert!(!metrics.is_healthy());
    }

    #[tokio::test]
    async fn test_global_metrics() {
        // Test global metrics initialization
        let metrics1 = get_global_metrics();
        let metrics2 = get_global_metrics();

        // Should be the same instance
        assert!(Arc::ptr_eq(&metrics1, &metrics2));

        // Test recording
        record_transaction_success(100);
        record_transaction_failure(50);

        // Wait a bit for async updates
        tokio::time::sleep(Duration::from_millis(100)).await;

        let snapshot = metrics1.get_snapshot();
        assert_eq!(snapshot.total_processed, 2);
        assert_eq!(snapshot.successful_transactions, 1);
        assert_eq!(snapshot.failed_transactions, 1);
    }
}
