//! # IPC Metrics and Monitoring
//!
//! Comprehensive metrics collection, monitoring, and observability for the plugin IPC system.
//! Includes performance metrics, health monitoring, distributed tracing, and alerting.

use crate::plugin_ipc::error::IpcResult;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Comprehensive metrics collector for IPC operations
#[derive(Debug)]
pub struct MetricsCollector {
    /// Performance metrics
    performance_metrics: Arc<RwLock<PerformanceMetrics>>,
    /// Health metrics
    health_metrics: Arc<RwLock<HealthMetrics>>,
    /// Error metrics
    error_metrics: Arc<RwLock<ErrorMetrics>>,
    /// Resource metrics
    resource_metrics: Arc<RwLock<ResourceMetrics>>,
    /// Business metrics
    business_metrics: Arc<RwLock<BusinessMetrics>>,
    /// Historical data
    historical_data: Arc<RwLock<HistoricalData>>,
    /// Configuration
    config: MetricsConfig,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(config: MetricsConfig) -> Self {
        Self {
            performance_metrics: Arc::new(RwLock::new(PerformanceMetrics::new())),
            health_metrics: Arc::new(RwLock::new(HealthMetrics::new())),
            error_metrics: Arc::new(RwLock::new(ErrorMetrics::new())),
            resource_metrics: Arc::new(RwLock::new(ResourceMetrics::new())),
            business_metrics: Arc::new(RwLock::new(BusinessMetrics::new())),
            historical_data: Arc::new(RwLock::new(HistoricalData::new(config.history_retention))),
            config,
        }
    }

    /// Record a message sent
    pub async fn record_message_sent(&self, endpoint: &str, size_bytes: usize, duration: Duration) {
        let mut metrics = self.performance_metrics.write().await;
        metrics.total_messages_sent += 1;
        metrics.total_bytes_sent += size_bytes as u64;
        metrics.send_durations.push(duration);

        // Keep only recent durations for percentile calculation
        if metrics.send_durations.len() > 1000 {
            metrics.send_durations.pop_front();
        }

        // Update endpoint-specific metrics
        let endpoint_metrics = metrics.endpoint_metrics.entry(endpoint.to_string()).or_insert_with(EndpointMetrics::new);
        endpoint_metrics.messages_sent += 1;
        endpoint_metrics.bytes_sent += size_bytes as u64;
        endpoint_metrics.avg_send_duration = duration;
        endpoint_metrics.last_activity = Instant::now();
    }

    /// Record a message received
    pub async fn record_message_received(&self, endpoint: &str, size_bytes: usize, duration: Duration) {
        let mut metrics = self.performance_metrics.write().await;
        metrics.total_messages_received += 1;
        metrics.total_bytes_received += size_bytes as u64;
        metrics.receive_durations.push(duration);

        // Keep only recent durations
        if metrics.receive_durations.len() > 1000 {
            metrics.receive_durations.pop_front();
        }

        // Update endpoint-specific metrics
        let endpoint_metrics = metrics.endpoint_metrics.entry(endpoint.to_string()).or_insert_with(EndpointMetrics::new);
        endpoint_metrics.messages_received += 1;
        endpoint_metrics.bytes_received += size_bytes as u64;
        endpoint_metrics.avg_receive_duration = duration;
        endpoint_metrics.last_activity = Instant::now();
    }

    /// Record an error
    pub async fn record_error(&self, error_type: &str, error_code: &str, endpoint: Option<&str>) {
        let mut metrics = self.error_metrics.write().await;
        metrics.total_errors += 1;

        // Update error type metrics
        let type_metrics = metrics.error_types.entry(error_type.to_string()).or_insert_with(ErrorTypeMetrics::new);
        type_metrics.count += 1;
        type_metrics.last_occurrence = Instant::now();

        // Update error code metrics
        let code_metrics = metrics.error_codes.entry(error_code.to_string()).or_insert_with(ErrorCodeMetrics::new);
        code_metrics.count += 1;
        code_metrics.last_occurrence = Instant::now();

        // Update endpoint error metrics
        if let Some(ep) = endpoint {
            let endpoint_metrics = metrics.endpoint_errors.entry(ep.to_string()).or_insert_with(EndpointErrorMetrics::new);
            endpoint_metrics.error_count += 1;
            endpoint_metrics.last_error = Instant::now();
        }
    }

    /// Update connection metrics
    pub async fn update_connection_metrics(&self, active_connections: u32, total_connections: u32) {
        let mut metrics = self.performance_metrics.write().await;
        metrics.active_connections = active_connections;
        metrics.total_connections = total_connections;
    }

    /// Update health status
    pub async fn update_health_status(&self, component: &str, status: HealthStatus, message: Option<String>) {
        let mut metrics = self.health_metrics.write().await;
        metrics.component_health.insert(component.to_string(), ComponentHealth {
            status,
            message,
            last_updated: SystemTime::now(),
        });
    }

    /// Update resource usage
    pub async fn update_resource_usage(&self, usage: ResourceUsage) {
        let mut metrics = self.resource_metrics.write().await;
        metrics.current_usage = usage;
        metrics.last_updated = SystemTime::now();

        // Update historical data
        if self.config.enable_history {
            let mut history = self.historical_data.write().await;
            history.add_resource_usage(usage);
        }
    }

    /// Record plugin execution
    pub async fn record_plugin_execution(&self, plugin_id: &str, duration: Duration, success: bool) {
        let mut metrics = self.business_metrics.write().await;

        let plugin_metrics = metrics.plugin_metrics.entry(plugin_id.to_string()).or_insert_with(PluginMetrics::new);
        plugin_metrics.total_executions += 1;
        plugin_metrics.total_duration += duration;
        plugin_metrics.last_execution = SystemTime::now();

        if success {
            plugin_metrics.successful_executions += 1;
        } else {
            plugin_metrics.failed_executions += 1;
        }
    }

    /// Get comprehensive metrics snapshot
    pub async fn get_metrics_snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            performance: self.performance_metrics.read().await.clone(),
            health: self.health_metrics.read().await.clone(),
            errors: self.error_metrics.read().await.clone(),
            resources: self.resource_metrics.read().await.clone(),
            business: self.business_metrics.read().await.clone(),
            timestamp: SystemTime::now(),
        }
    }

    /// Get performance summary
    pub async fn get_performance_summary(&self) -> PerformanceSummary {
        let metrics = self.performance_metrics.read().await;

        let avg_send_duration = if metrics.send_durations.is_empty() {
            Duration::ZERO
        } else {
            let total: Duration = metrics.send_durations.iter().sum();
            total / metrics.send_durations.len() as u32
        };

        let avg_receive_duration = if metrics.receive_durations.is_empty() {
            Duration::ZERO
        } else {
            let total: Duration = metrics.receive_durations.iter().sum();
            total / metrics.receive_durations.len() as u32
        };

        PerformanceSummary {
            messages_per_second: self.calculate_messages_per_second(&metrics).await,
            bytes_per_second: self.calculate_bytes_per_second(&metrics).await,
            avg_send_duration,
            avg_receive_duration,
            p95_send_duration: self.calculate_percentile(&metrics.send_durations, 0.95),
            p99_send_duration: self.calculate_percentile(&metrics.send_durations, 0.99),
            p95_receive_duration: self.calculate_percentile(&metrics.receive_durations, 0.95),
            p99_receive_duration: self.calculate_percentile(&metrics.receive_durations, 0.99),
            active_connections: metrics.active_connections,
            total_connections: metrics.total_connections,
        }
    }

    /// Get health summary
    pub async fn get_health_summary(&self) -> HealthSummary {
        let metrics = self.health_metrics.read().await;
        let unhealthy_components = metrics.component_health
            .iter()
            .filter(|(_, health)| health.status != HealthStatus::Healthy)
            .count();

        HealthSummary {
            overall_status: if unhealthy_components == 0 {
                HealthStatus::Healthy
            } else if unhealthy_components < metrics.component_health.len() / 2 {
                HealthStatus::Degraded
            } else {
                HealthStatus::Unhealthy
            },
            total_components: metrics.component_health.len(),
            healthy_components: metrics.component_health.len() - unhealthy_components,
            unhealthy_components,
            last_check: metrics.last_health_check,
        }
    }

    /// Reset all metrics
    pub async fn reset_metrics(&self) {
        *self.performance_metrics.write().await = PerformanceMetrics::new();
        *self.health_metrics.write().await = HealthMetrics::new();
        *self.error_metrics.write().await = ErrorMetrics::new();
        *self.resource_metrics.write().await = ResourceMetrics::new();
        *self.business_metrics.write().await = BusinessMetrics::new();
    }

    // Private helper methods

    async fn calculate_messages_per_second(&self, metrics: &PerformanceMetrics) -> f64 {
        if metrics.send_durations.is_empty() {
            0.0
        } else {
            let duration = metrics.send_durations.iter().sum::<Duration>();
            let total_messages = metrics.total_messages_sent + metrics.total_messages_received;
            if duration.as_secs_f64() > 0.0 {
                total_messages as f64 / duration.as_secs_f64()
            } else {
                0.0
            }
        }
    }

    async fn calculate_bytes_per_second(&self, metrics: &PerformanceMetrics) -> f64 {
        let total_bytes = metrics.total_bytes_sent + metrics.total_bytes_received;
        if metrics.send_durations.is_empty() {
            0.0
        } else {
            let duration = metrics.send_durations.iter().sum::<Duration>();
            if duration.as_secs_f64() > 0.0 {
                total_bytes as f64 / duration.as_secs_f64()
            } else {
                0.0
            }
        }
    }

    fn calculate_percentile(&self, durations: &VecDeque<Duration>, percentile: f64) -> Duration {
        if durations.is_empty() {
            return Duration::ZERO;
        }

        let mut sorted_durations: Vec<Duration> = durations.iter().copied().collect();
        sorted_durations.sort();

        let index = ((sorted_durations.len() as f64 - 1.0) * percentile).round() as usize;
        sorted_durations[index]
    }
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_messages_sent: u64,
    pub total_messages_received: u64,
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub send_durations: VecDeque<Duration>,
    pub receive_durations: VecDeque<Duration>,
    pub active_connections: u32,
    pub total_connections: u32,
    pub endpoint_metrics: HashMap<String, EndpointMetrics>,
    pub last_updated: SystemTime,
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        Self {
            total_messages_sent: 0,
            total_messages_received: 0,
            total_bytes_sent: 0,
            total_bytes_received: 0,
            send_durations: VecDeque::new(),
            receive_durations: VecDeque::new(),
            active_connections: 0,
            total_connections: 0,
            endpoint_metrics: HashMap::new(),
            last_updated: SystemTime::now(),
        }
    }
}

/// Endpoint-specific metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointMetrics {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub avg_send_duration: Duration,
    pub avg_receive_duration: Duration,
    pub last_activity: Instant,
    pub error_rate: f64,
}

impl EndpointMetrics {
    pub fn new() -> Self {
        Self {
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            avg_send_duration: Duration::ZERO,
            avg_receive_duration: Duration::ZERO,
            last_activity: Instant::now(),
            error_rate: 0.0,
        }
    }
}

/// Health metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetrics {
    pub component_health: HashMap<String, ComponentHealth>,
    pub last_health_check: SystemTime,
    pub uptime: Duration,
}

impl HealthMetrics {
    pub fn new() -> Self {
        Self {
            component_health: HashMap::new(),
            last_health_check: SystemTime::now(),
            uptime: Duration::ZERO,
        }
    }
}

/// Component health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub status: HealthStatus,
    pub message: Option<String>,
    pub last_updated: SystemTime,
}

/// Health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Error metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMetrics {
    pub total_errors: u64,
    pub error_types: HashMap<String, ErrorTypeMetrics>,
    pub error_codes: HashMap<String, ErrorCodeMetrics>,
    pub endpoint_errors: HashMap<String, EndpointErrorMetrics>,
    pub last_updated: SystemTime,
}

impl ErrorMetrics {
    pub fn new() -> Self {
        Self {
            total_errors: 0,
            error_types: HashMap::new(),
            error_codes: HashMap::new(),
            endpoint_errors: HashMap::new(),
            last_updated: SystemTime::now(),
        }
    }
}

/// Error type metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorTypeMetrics {
    pub count: u64,
    pub last_occurrence: Instant,
}

impl ErrorTypeMetrics {
    pub fn new() -> Self {
        Self {
            count: 0,
            last_occurrence: Instant::now(),
        }
    }
}

/// Error code metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorCodeMetrics {
    pub count: u64,
    pub last_occurrence: Instant,
}

impl ErrorCodeMetrics {
    pub fn new() -> Self {
        Self {
            count: 0,
            last_occurrence: Instant::now(),
        }
    }
}

/// Endpoint error metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointErrorMetrics {
    pub error_count: u64,
    pub last_error: Instant,
}

impl EndpointErrorMetrics {
    pub fn new() -> Self {
        Self {
            error_count: 0,
            last_error: Instant::now(),
        }
    }
}

/// Resource metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMetrics {
    pub current_usage: ResourceUsage,
    pub peak_usage: ResourceUsage,
    pub last_updated: SystemTime,
}

impl ResourceMetrics {
    pub fn new() -> Self {
        Self {
            current_usage: ResourceUsage::default(),
            peak_usage: ResourceUsage::default(),
            last_updated: SystemTime::now(),
        }
    }
}

/// Resource usage information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceUsage {
    pub memory_bytes: u64,
    pub cpu_percentage: f64,
    pub disk_bytes: u64,
    pub network_bytes: u64,
    pub open_files: u32,
    pub active_threads: u32,
    pub goroutines: u32, // For Go plugins
}

/// Business metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessMetrics {
    pub plugin_metrics: HashMap<String, PluginMetrics>,
    pub total_plugins: u32,
    pub active_plugins: u32,
    pub last_updated: SystemTime,
}

impl BusinessMetrics {
    pub fn new() -> Self {
        Self {
            plugin_metrics: HashMap::new(),
            total_plugins: 0,
            active_plugins: 0,
            last_updated: SystemTime::now(),
        }
    }
}

/// Plugin-specific metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetrics {
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub total_duration: Duration,
    pub last_execution: SystemTime,
    pub success_rate: f64,
}

impl PluginMetrics {
    pub fn new() -> Self {
        Self {
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            total_duration: Duration::ZERO,
            last_execution: SystemTime::UNIX_EPOCH,
            success_rate: 1.0,
        }
    }
}

/// Historical data storage
#[derive(Debug)]
pub struct HistoricalData {
    resource_usage_history: VecDeque<(SystemTime, ResourceUsage)>,
    max_entries: usize,
}

impl HistoricalData {
    pub fn new(max_entries: usize) -> Self {
        Self {
            resource_usage_history: VecDeque::new(),
            max_entries,
        }
    }

    pub fn add_resource_usage(&mut self, usage: ResourceUsage) {
        self.resource_usage_history.push_back((SystemTime::now(), usage));

        // Remove old entries if we exceed the limit
        while self.resource_usage_history.len() > self.max_entries {
            self.resource_usage_history.pop_front();
        }
    }

    pub fn get_resource_usage_history(&self) -> Vec<(SystemTime, ResourceUsage)> {
        self.resource_usage_history.iter().cloned().collect()
    }
}

/// Comprehensive metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub performance: PerformanceMetrics,
    pub health: HealthMetrics,
    pub errors: ErrorMetrics,
    pub resources: ResourceMetrics,
    pub business: BusinessMetrics,
    pub timestamp: SystemTime,
}

/// Performance summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSummary {
    pub messages_per_second: f64,
    pub bytes_per_second: f64,
    pub avg_send_duration: Duration,
    pub avg_receive_duration: Duration,
    pub p95_send_duration: Duration,
    pub p99_send_duration: Duration,
    pub p95_receive_duration: Duration,
    pub p99_receive_duration: Duration,
    pub active_connections: u32,
    pub total_connections: u32,
}

/// Health summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSummary {
    pub overall_status: HealthStatus,
    pub total_components: usize,
    pub healthy_components: usize,
    pub unhealthy_components: usize,
    pub last_check: SystemTime,
}

/// Metrics configuration
#[derive(Debug, Clone)]
pub struct MetricsConfig {
    pub enable_history: bool,
    pub history_retention: usize,
    pub collection_interval: Duration,
    pub export_enabled: bool,
    pub export_format: ExportFormat,
    pub alerting_enabled: bool,
    pub alert_thresholds: AlertThresholds,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enable_history: true,
            history_retention: 1000,
            collection_interval: Duration::from_secs(10),
            export_enabled: false,
            export_format: ExportFormat::Json,
            alerting_enabled: true,
            alert_thresholds: AlertThresholds::default(),
        }
    }
}

/// Export format for metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportFormat {
    Json,
    Prometheus,
    Csv,
}

/// Alert thresholds
#[derive(Debug, Clone)]
pub struct AlertThresholds {
    pub error_rate_threshold: f64,
    pub response_time_threshold: Duration,
    pub cpu_threshold: f64,
    pub memory_threshold: f64,
    pub connection_threshold: u32,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            error_rate_threshold: 0.05, // 5%
            response_time_threshold: Duration::from_millis(1000),
            cpu_threshold: 80.0, // 80%
            memory_threshold: 90.0, // 90%
            connection_threshold: 100,
        }
    }
}

/// Metrics exporter for different formats
pub struct MetricsExporter {
    config: MetricsConfig,
}

impl MetricsExporter {
    pub fn new(config: MetricsConfig) -> Self {
        Self { config }
    }

    /// Export metrics in the configured format
    pub async fn export(&self, snapshot: &MetricsSnapshot) -> IpcResult<String> {
        match self.config.export_format {
            ExportFormat::Json => self.export_json(snapshot).await,
            ExportFormat::Prometheus => self.export_prometheus(snapshot).await,
            ExportFormat::Csv => self.export_csv(snapshot).await,
        }
    }

    async fn export_json(&self, snapshot: &MetricsSnapshot) -> IpcResult<String> {
        serde_json::to_string_pretty(snapshot)
            .map_err(|e| crate::plugin_ipc::error::IpcError::Configuration {
                message: format!("Failed to serialize metrics to JSON: {}", e),
                code: crate::plugin_ipc::error::ConfigErrorCode::SerializationFailed,
                config_key: None,
            })
    }

    async fn export_prometheus(&self, snapshot: &MetricsSnapshot) -> IpcResult<String> {
        let mut output = String::new();

        // Prometheus format metrics
        output.push_str("# HELP ipc_messages_total Total number of IPC messages\n");
        output.push_str("# TYPE ipc_messages_total counter\n");
        output.push_str(&format!("ipc_messages_sent_total {}\n", snapshot.performance.total_messages_sent));
        output.push_str(&format!("ipc_messages_received_total {}\n", snapshot.performance.total_messages_received));

        output.push_str("# HELP ipc_bytes_total Total bytes transferred\n");
        output.push_str("# TYPE ipc_bytes_total counter\n");
        output.push_str(&format!("ipc_bytes_sent_total {}\n", snapshot.performance.total_bytes_sent));
        output.push_str(&format!("ipc_bytes_received_total {}\n", snapshot.performance.total_bytes_received));

        output.push_str("# HELP ipc_connections Active connections\n");
        output.push_str("# TYPE ipc_connections gauge\n");
        output.push_str(&format!("ipc_connections_active {}\n", snapshot.performance.active_connections));

        output.push_str("# HELP ipc_errors_total Total errors\n");
        output.push_str("# TYPE ipc_errors_total counter\n");
        output.push_str(&format!("ipc_errors_total {}\n", snapshot.errors.total_errors));

        output.push_str("# HELP ipc_memory_bytes Memory usage in bytes\n");
        output.push_str("# TYPE ipc_memory_bytes gauge\n");
        output.push_str(&format!("ipc_memory_bytes_current {}\n", snapshot.resources.current_usage.memory_bytes));

        output.push_str("# HELP ipc_cpu_percentage CPU usage percentage\n");
        output.push_str("# TYPE ipc_cpu_percentage gauge\n");
        output.push_str(&format!("ipc_cpu_percentage_current {}\n", snapshot.resources.current_usage.cpu_percentage));

        Ok(output)
    }

    async fn export_csv(&self, snapshot: &MetricsSnapshot) -> IpcResult<String> {
        let mut output = String::new();

        // CSV header
        output.push_str("timestamp,messages_sent,messages_received,bytes_sent,bytes_received,active_connections,errors,cpu_percentage,memory_bytes\n");

        // Data row
        output.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            snapshot.timestamp.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            snapshot.performance.total_messages_sent,
            snapshot.performance.total_messages_received,
            snapshot.performance.total_bytes_sent,
            snapshot.performance.total_bytes_received,
            snapshot.performance.active_connections,
            snapshot.errors.total_errors,
            snapshot.resources.current_usage.cpu_percentage,
            snapshot.resources.current_usage.memory_bytes
        ));

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collector_creation() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config);

        let snapshot = collector.get_metrics_snapshot().await;
        assert_eq!(snapshot.performance.total_messages_sent, 0);
        assert_eq!(snapshot.errors.total_errors, 0);
    }

    #[tokio::test]
    async fn test_record_message_sent() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config);

        collector.record_message_sent("test_endpoint", 1024, Duration::from_millis(100)).await;

        let snapshot = collector.get_metrics_snapshot().await;
        assert_eq!(snapshot.performance.total_messages_sent, 1);
        assert_eq!(snapshot.performance.total_bytes_sent, 1024);
        assert_eq!(snapshot.performance.send_durations.len(), 1);
    }

    #[tokio::test]
    async fn test_record_error() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config);

        collector.record_error("connection", "timeout", Some("test_endpoint")).await;

        let snapshot = collector.get_metrics_snapshot().await;
        assert_eq!(snapshot.errors.total_errors, 1);
        assert!(snapshot.errors.error_types.contains_key("connection"));
        assert!(snapshot.errors.error_codes.contains_key("timeout"));
        assert!(snapshot.errors.endpoint_errors.contains_key("test_endpoint"));
    }

    #[tokio::test]
    async fn test_update_health_status() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config);

        collector.update_health_status("test_component", HealthStatus::Healthy, None).await;

        let snapshot = collector.get_metrics_snapshot().await;
        assert!(snapshot.health.component_health.contains_key("test_component"));
        assert_eq!(snapshot.health.component_health["test_component"].status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_performance_summary() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config);

        // Record some performance data
        for i in 0..10 {
            collector.record_message_sent("test_endpoint", 100 * i, Duration::from_millis(50 + i * 10)).await;
            collector.record_message_received("test_endpoint", 100 * i, Duration::from_millis(30 + i * 5)).await;
        }

        let summary = collector.get_performance_summary().await;
        assert!(summary.messages_per_second > 0.0);
        assert!(summary.bytes_per_second > 0.0);
        assert_eq!(summary.active_connections, 0); // Not updated in this test
    }

    #[tokio::test]
    async fn test_health_summary() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config);

        // Add healthy components
        collector.update_health_status("comp1", HealthStatus::Healthy, None).await;
        collector.update_health_status("comp2", HealthStatus::Healthy, None).await;

        // Add unhealthy component
        collector.update_health_status("comp3", HealthStatus::Unhealthy, Some("Error".to_string())).await;

        let summary = collector.get_health_summary().await;
        assert_eq!(summary.total_components, 3);
        assert_eq!(summary.healthy_components, 2);
        assert_eq!(summary.unhealthy_components, 1);
        assert_eq!(summary.overall_status, HealthStatus::Degraded); // 1/3 unhealthy
    }

    #[tokio::test]
    async fn test_plugin_metrics() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config);

        collector.record_plugin_execution("test_plugin", Duration::from_millis(100), true).await;
        collector.record_plugin_execution("test_plugin", Duration::from_millis(150), false).await;

        let snapshot = collector.get_metrics_snapshot().await;
        let plugin_metrics = snapshot.business.plugin_metrics.get("test_plugin").unwrap();
        assert_eq!(plugin_metrics.total_executions, 2);
        assert_eq!(plugin_metrics.successful_executions, 1);
        assert_eq!(plugin_metrics.failed_executions, 1);
        assert_eq!(plugin_metrics.success_rate, 0.5);
    }

    #[test]
    fn test_metrics_config() {
        let config = MetricsConfig::default();
        assert!(config.enable_history);
        assert_eq!(config.history_retention, 1000);
        assert_eq!(config.collection_interval, Duration::from_secs(10));
        assert!(!config.export_enabled);
        assert!(matches!(config.export_format, ExportFormat::Json));
        assert!(config.alerting_enabled);
    }

    #[test]
    fn test_alert_thresholds() {
        let thresholds = AlertThresholds::default();
        assert_eq!(thresholds.error_rate_threshold, 0.05);
        assert_eq!(thresholds.response_time_threshold, Duration::from_millis(1000));
        assert_eq!(thresholds.cpu_threshold, 80.0);
        assert_eq!(thresholds.memory_threshold, 90.0);
        assert_eq!(thresholds.connection_threshold, 100);
    }

    #[tokio::test]
    async fn test_metrics_exporter() {
        let config = MetricsConfig {
            export_enabled: true,
            export_format: ExportFormat::Json,
            ..Default::default()
        };

        let exporter = MetricsExporter::new(config);

        let snapshot = MetricsSnapshot {
            performance: PerformanceMetrics::new(),
            health: HealthMetrics::new(),
            errors: ErrorMetrics::new(),
            resources: ResourceMetrics::new(),
            business: BusinessMetrics::new(),
            timestamp: SystemTime::now(),
        };

        let exported = exporter.export(&snapshot).await.unwrap();
        assert!(exported.contains("\"performance\""));
        assert!(exported.contains("\"health\""));
        assert!(exported.contains("\"errors\""));
    }

    #[tokio::test]
    async fn test_prometheus_export() {
        let config = MetricsConfig {
            export_enabled: true,
            export_format: ExportFormat::Prometheus,
            ..Default::default()
        };

        let exporter = MetricsExporter::new(config);

        let snapshot = MetricsSnapshot {
            performance: PerformanceMetrics {
                total_messages_sent: 100,
                total_messages_received: 95,
                total_bytes_sent: 10240,
                total_bytes_received: 9728,
                active_connections: 5,
                ..Default::default()
            },
            health: HealthMetrics::new(),
            errors: ErrorMetrics {
                total_errors: 2,
                ..Default::default()
            },
            resources: ResourceMetrics {
                current_usage: ResourceUsage {
                    memory_bytes: 1024 * 1024 * 100, // 100MB
                    cpu_percentage: 25.5,
                    ..Default::default()
                },
                ..Default::default()
            },
            business: BusinessMetrics::new(),
            timestamp: SystemTime::now(),
        };

        let exported = exporter.export(&snapshot).await.unwrap();
        assert!(exported.contains("# HELP ipc_messages_total"));
        assert!(exported.contains("ipc_messages_sent_total 100"));
        assert!(exported.contains("ipc_memory_bytes_current 104857600"));
        assert!(exported.contains("ipc_cpu_percentage_current 25.5"));
    }

    #[tokio::test]
    async fn test_historical_data() {
        let mut history = HistoricalData::new(3);

        let usage1 = ResourceUsage {
            memory_bytes: 1000,
            cpu_percentage: 50.0,
            ..Default::default()
        };

        let usage2 = ResourceUsage {
            memory_bytes: 2000,
            cpu_percentage: 60.0,
            ..Default::default()
        };

        let usage3 = ResourceUsage {
            memory_bytes: 3000,
            cpu_percentage: 70.0,
            ..Default::default()
        };

        let usage4 = ResourceUsage {
            memory_bytes: 4000,
            cpu_percentage: 80.0,
            ..Default::default()
        };

        history.add_resource_usage(usage1.clone());
        history.add_resource_usage(usage2.clone());
        history.add_resource_usage(usage3.clone());
        history.add_resource_usage(usage4);

        let stored = history.get_resource_usage_history();
        assert_eq!(stored.len(), 3); // Should have removed the first entry

        // The oldest entry should be usage2, usage3, usage4
        assert_eq!(stored[0].1.memory_bytes, 2000);
        assert_eq!(stored[1].1.memory_bytes, 3000);
        assert_eq!(stored[2].1.memory_bytes, 4000);
    }

    #[tokio::test]
    async fn test_reset_metrics() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config);

        // Record some data
        collector.record_message_sent("test", 100, Duration::from_millis(50)).await;
        collector.record_error("test", "error", None).await;

        // Verify data was recorded
        let snapshot = collector.get_metrics_snapshot().await;
        assert_eq!(snapshot.performance.total_messages_sent, 1);
        assert_eq!(snapshot.errors.total_errors, 1);

        // Reset metrics
        collector.reset_metrics().await;

        // Verify metrics are reset
        let snapshot = collector.get_metrics_snapshot().await;
        assert_eq!(snapshot.performance.total_messages_sent, 0);
        assert_eq!(snapshot.errors.total_errors, 0);
    }
}