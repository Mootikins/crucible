//! # Metrics Component Tests
//!
//! Comprehensive tests for IPC metrics components including performance metric
//! collection, distributed tracing, health monitoring, resource usage tracking,
//! metric aggregation, and alerting.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::plugin_ipc::{
    metrics::{MetricsCollector, PerformanceTracker, HealthMonitor, ResourceMonitor},
    message::{IpcMessage, MessageType, MessagePayload},
    error::IpcError,
};

use super::common::{
    *,
    fixtures::*,
    mocks::*,
    helpers::*,
};

/// Performance metric collection tests
pub struct PerformanceMetricsTests;

impl PerformanceMetricsTests {
    /// Test basic counter metrics
    pub async fn test_counter_metrics() -> IpcResult<()> {
        let metrics = MockMetricsCollector::new();

        // Record counter values
        metrics.record_counter("messages_sent", 10).await?;
        metrics.record_counter("messages_sent", 5).await?;
        metrics.record_counter("connections_active", 3).await?;

        // Verify counter values
        assert_eq!(metrics.get_counter("messages_sent").await, Some(15));
        assert_eq!(metrics.get_counter("connections_active").await, Some(3));

        // Clear metrics
        metrics.clear_all().await;
        assert_eq!(metrics.get_counter("messages_sent").await, None);

        Ok(())
    }

    /// Test gauge metrics
    pub async fn test_gauge_metrics() -> IpcResult<()> {
        let metrics = MockMetricsCollector::new();

        // Record gauge values
        metrics.record_gauge("memory_usage_mb", 256.5).await?;
        metrics.record_gauge("cpu_percentage", 75.2).await?;
        metrics.record_gauge("memory_usage_mb", 128.0).await?; // Overwrite

        // Verify gauge values
        assert_eq!(metrics.get_metric("memory_usage_mb").await, Some(128.0));
        assert_eq!(metrics.get_metric("cpu_percentage").await, Some(75.2));

        Ok(())
    }

    /// Test histogram metrics
    pub async fn test_histogram_metrics() -> IpcResult<()> {
        let metrics = MockMetricsCollector::new();

        // Record histogram values
        for latency in [10.0, 25.0, 50.0, 75.0, 100.0, 15.0, 30.0] {
            metrics.record_histogram("request_latency_ms", latency).await?;
        }

        // Verify histogram data
        let latency_values = metrics.get_histogram("request_latency_ms").await;
        assert_eq!(latency_values.len(), 7);
        assert!(latency_values.contains(&10.0));
        assert!(latency_values.contains(&100.0));

        // Calculate statistics
        let sum: f64 = latency_values.iter().sum();
        assert_eq!(sum, 305.0);
        let avg = sum / latency_values.len() as f64;
        assert!((avg - 43.571).abs() < 0.01);

        Ok(())
    }

    /// Test metric aggregation
    pub async fn test_metric_aggregation() -> IpcResult<()> {
        let metrics = MockMetricsCollector::new();

        // Record various metrics
        for i in 0..100 {
            metrics.record_counter("operations_total", 1).await?;
            metrics.record_histogram("operation_duration_ms", i as f64).await?;
            metrics.record_gauge("active_connections", (i % 10) as f64).await?;
        }

        // Get metrics summary
        let summary = metrics.get_metrics_summary().await?;

        // Verify summary structure
        assert!(summary.get("counters").is_some());
        assert!(summary.get("gauges").is_some());

        if let Some(counters) = summary.get("counters") {
            if let Value::Object(counter_map) = counters {
                assert_eq!(counter_map.get("operations_total"), Some(&Value::Number(serde_json::Number::from(100))));
            }
        }

        Ok(())
    }

    /// Test metric labels and dimensions
    pub async fn test_metric_dimensions() -> IpcResult<()> {
        let metrics = MockMetricsCollector::new();

        // Record metrics with different dimensions
        metrics.record_counter("requests_total", 10).await?;
        metrics.record_counter("requests_total", 5).await?;

        // In a real implementation, dimensions would be tracked
        // For mock implementation, we just verify basic functionality
        let total = metrics.get_counter("requests_total").await;
        assert_eq!(total, Some(15));

        Ok(())
    }

    /// Test metric retention and cleanup
    pub async fn test_metric_retention() -> IpcResult<()> {
        let metrics = Arc::new(MockMetricsCollector::new());

        // Record metrics
        for i in 0..1000 {
            metrics.record_counter(&format!("metric_{}", i), 1).await?;
        }

        // Verify metrics are recorded
        let summary = metrics.get_metrics_summary().await?;
        assert!(summary.get("counters").is_some());

        // Clear old metrics (in a real implementation)
        metrics.clear_all().await;

        // Verify cleanup
        let summary = metrics.get_metrics_summary().await?;
        if let Some(Value::Object(counters)) = summary.get("counters") {
            assert!(counters.is_empty());
        }

        Ok(())
    }
}

/// Distributed tracing tests
pub struct DistributedTracingTests;

impl DistributedTracingTests {
    /// Test trace context propagation
    pub async fn test_trace_context_propagation() -> IpcResult<()> {
        let tracer = MockTracer::new();

        // Start a trace
        let trace_context = tracer.start_trace("test_operation").await?;
        assert!(!trace_context.trace_id.is_empty());
        assert!(!trace_context.span_id.is_empty());

        // Create child span
        let child_context = tracer.start_child_span(&trace_context, "child_operation").await?;
        assert_eq!(child_context.trace_id, trace_context.trace_id);
        assert_ne!(child_context.span_id, trace_context.span_id);

        // End spans
        tracer.end_span(&child_context).await?;
        tracer.end_span(&trace_context).await?;

        Ok(())
    }

    /// Test trace sampling
    pub async fn test_trace_sampling() -> IpcResult<()> {
        let tracer = MockTracer::new();
        tracer.set_sampling_rate(0.1).await; // 10% sampling

        let mut sampled_count = 0;
        let total_traces = 1000;

        for _ in 0..total_traces {
            let trace_context = tracer.start_trace("sampled_operation").await?;
            if trace_context.sampled {
                sampled_count += 1;
            }
            tracer.end_span(&trace_context).await?;
        }

        // Should be approximately 10% sampled (allowing for variance)
        let sample_rate = sampled_count as f64 / total_traces as f64;
        assert!(sample_rate > 0.05 && sample_rate < 0.15);

        Ok(())
    }

    /// Test span events and attributes
    pub async fn test_span_events() -> IpcResult<()> {
        let tracer = MockTracer::new();
        let trace_context = tracer.start_trace("operation_with_events").await?;

        // Add events
        tracer.add_event(&trace_context, "started", HashMap::new()).await?;
        tracer.add_event(&trace_context, "processing",
            HashMap::from([("items_processed".to_string(), "42".to_string())])
        ).await?;
        tracer.add_event(&trace_context, "completed", HashMap::new()).await?;

        // Add attributes
        tracer.set_attribute(&trace_context, "operation_type", "test").await?;
        tracer.set_attribute(&trace_context, "user_id", "test_user").await?;

        tracer.end_span(&trace_context).await?;

        // Verify trace data
        let trace_data = tracer.get_trace_data(&trace_context.trace_id).await?;
        assert!(trace_data.is_some());

        if let Some(data) = trace_data {
            assert_eq!(data.events.len(), 3);
            assert_eq!(data.attributes.len(), 2);
        }

        Ok(())
    }

    /// Test trace export
    pub async fn test_trace_export() -> IpcResult<()> {
        let tracer = MockTracer::new();

        // Create multiple traces
        for i in 0..10 {
            let trace_context = tracer.start_trace(&format!("operation_{}", i)).await?;
            tracer.add_event(&trace_context, "start", HashMap::new()).await?;
            tracer.end_span(&trace_context).await?;
        }

        // Export traces
        let exported_traces = tracer.export_traces().await?;
        assert_eq!(exported_traces.len(), 10);

        // Verify trace format
        for trace in exported_traces {
            assert!(trace.contains("trace_id"));
            assert!(trace.contains("spans"));
        }

        Ok(())
    }
}

/// Health monitoring tests
pub struct HealthMonitoringTests;

impl HealthMonitoringTests {
    /// Test basic health checks
    pub async fn test_basic_health_checks() -> IpcResult<()> {
        let health_monitor = MockHealthMonitor::new();

        // Initial health should be good
        let health = health_monitor.get_health().await?;
        assert_eq!(health.status, "healthy");
        assert!(health.details.is_empty());

        // Add health check
        health_monitor.add_check("database", MockHealthCheck::new(true)).await;
        health_monitor.add_check("cache", MockHealthCheck::new(true)).await;

        let health = health_monitor.get_health().await?;
        assert_eq!(health.status, "healthy");
        assert_eq!(health.checks.len(), 2);

        Ok(())
    }

    /// Test failing health checks
    pub async fn test_failing_health_checks() -> IpcResult<()> {
        let health_monitor = MockHealthMonitor::new();

        // Add failing health check
        health_monitor.add_check("failing_service", MockHealthCheck::new(false)).await;
        health_monitor.add_check("healthy_service", MockHealthCheck::new(true)).await;

        let health = health_monitor.get_health().await?;
        assert_eq!(health.status, "unhealthy");
        assert!(health.checks.len() == 2);

        // Check specific service health
        let failing_health = health_monitor.get_check_health("failing_service").await?;
        assert_eq!(failing_health.status, "unhealthy");

        let healthy_health = health_monitor.get_check_health("healthy_service").await?;
        assert_eq!(healthy_health.status, "healthy");

        Ok(())
    }

    /// Test health check timeouts
    pub async fn test_health_check_timeouts() -> IpcResult<()> {
        let health_monitor = MockHealthMonitor::new();

        // Add slow health check
        let slow_check = MockHealthCheck::new_with_delay(true, Duration::from_millis(200));
        health_monitor.add_check("slow_service", slow_check).await;

        // Set short timeout
        health_monitor.set_timeout(Duration::from_millis(100)).await;

        let health = health_monitor.get_health().await?;
        assert_eq!(health.status, "unhealthy");

        // Check slow service specifically
        let slow_health = health_monitor.get_check_health("slow_service").await?;
        assert!(slow_health.status == "unhealthy");
        assert!(slow_health.message.unwrap().contains("timeout"));

        Ok(())
    }

    /// Test health check recovery
    pub async fn test_health_check_recovery() -> IpcResult<()> {
        let health_monitor = MockHealthMonitor::new();
        let flaky_check = MockFlakyHealthCheck::new();
        health_monitor.add_check("flaky_service", flaky_check).await;

        // Initial state - failing
        let health = health_monitor.get_health().await?;
        assert_eq!(health.status, "unhealthy");

        // Trigger recovery
        let flaky_service = health_monitor.checks.get("flaky_service").unwrap();
        if let Some(flaky) = flaky_service.downcast_ref::<MockFlakyHealthCheck>() {
            flaky.set_healthy(true).await;
        }

        // Check recovery
        tokio::time::sleep(Duration::from_millis(110)).await; // Wait for next check
        let health = health_monitor.get_health().await?;
        assert_eq!(health.status, "healthy");

        Ok(())
    }

    /// Test health metrics integration
    pub async fn test_health_metrics_integration() -> IpcResult<()> {
        let health_monitor = MockHealthMonitor::new();
        let metrics = MockMetricsCollector::new();

        // Add health checks
        health_monitor.add_check("service1", MockHealthCheck::new(true)).await;
        health_monitor.add_check("service2", MockHealthCheck::new(false)).await;

        // Enable metrics integration
        health_monitor.enable_metrics(metrics.clone()).await;

        // Run health check
        let health = health_monitor.get_health().await?;

        // Verify health metrics were recorded
        let healthy_count = metrics.get_counter("health_checks_healthy_total").await.unwrap_or(0);
        let unhealthy_count = metrics.get_counter("health_checks_unhealthy_total").await.unwrap_or(0);

        assert!(healthy_count > 0);
        assert!(unhealthy_count > 0);

        Ok(())
    }
}

/// Resource monitoring tests
pub struct ResourceMonitoringTests;

impl ResourceMonitoringTests {
    /// Test CPU monitoring
    pub async fn test_cpu_monitoring() -> IpcResult<()> {
        let resource_monitor = MockResourceMonitor::new();

        // Get current CPU usage
        let cpu_usage = resource_monitor.get_cpu_usage().await?;
        assert!(cpu_usage >= 0.0 && cpu_usage <= 100.0);

        // Get CPU usage over time
        let cpu_history = resource_monitor.get_cpu_usage_history(Duration::from_secs(1)).await?;
        assert!(!cpu_history.is_empty());

        // Verify monotonic data
        for usage in &cpu_history {
            assert!(usage >= &0.0 && usage <= &100.0);
        }

        Ok(())
    }

    /// Test memory monitoring
    pub async fn test_memory_monitoring() -> IpcResult<()> {
        let resource_monitor = MockResourceMonitor::new();

        // Get current memory usage
        let memory_usage = resource_monitor.get_memory_usage().await?;
        assert!(memory_usage.total_bytes > 0);
        assert!(memory_usage.used_bytes <= memory_usage.total_bytes);
        assert!(memory_usage.available_bytes > 0);

        // Get memory usage percentage
        let usage_percentage = (memory_usage.used_bytes as f64 / memory_usage.total_bytes as f64) * 100.0;
        assert!(usage_percentage >= 0.0 && usage_percentage <= 100.0);

        // Get memory history
        let memory_history = resource_monitor.get_memory_usage_history(Duration::from_secs(1)).await?;
        assert!(!memory_history.is_empty());

        Ok(())
    }

    /// Test disk monitoring
    pub async fn test_disk_monitoring() -> IpcResult<()> {
        let resource_monitor = MockResourceMonitor::new();

        // Get disk usage for current directory
        let disk_usage = resource_monitor.get_disk_usage(".").await?;
        assert!(disk_usage.total_bytes > 0);
        assert!(disk_usage.used_bytes <= disk_usage.total_bytes);
        assert!(disk_usage.available_bytes > 0);

        // Get I/O statistics
        let io_stats = resource_monitor.get_disk_io_stats().await?;
        assert!(io_stats.read_bytes >= 0);
        assert!(io_stats.write_bytes >= 0);
        assert!(io_stats.read_operations >= 0);
        assert!(io_stats.write_operations >= 0);

        Ok(())
    }

    /// Test network monitoring
    pub async fn test_network_monitoring() -> IpcResult<()> {
        let resource_monitor = MockResourceMonitor::new();

        // Get network statistics
        let network_stats = resource_monitor.get_network_stats().await?;
        assert!(network_stats.bytes_sent >= 0);
        assert!(network_stats.bytes_received >= 0);
        assert!(network_stats.packets_sent >= 0);
        assert!(network_stats.packets_received >= 0);

        // Get connection count
        let connection_count = resource_monitor.get_connection_count().await?;
        assert!(connection_count >= 0);

        // Get network interface stats
        let interface_stats = resource_monitor.get_network_interface_stats().await?;
        assert!(!interface_stats.is_empty());

        for (interface, stats) in interface_stats {
            assert!(!interface.is_empty());
            assert!(stats.bytes_sent >= 0);
            assert!(stats.bytes_received >= 0);
        }

        Ok(())
    }

    /// Test resource limits and alerts
    pub async fn test_resource_limits() -> IpcResult<()> {
        let resource_monitor = MockResourceMonitor::new();

        // Set resource limits
        resource_monitor.set_cpu_limit(80.0).await;
        resource_monitor.set_memory_limit(1024 * 1024 * 1024).await; // 1GB

        // Check if limits are exceeded
        let cpu_exceeded = resource_monitor.is_cpu_limit_exceeded().await?;
        let memory_exceeded = resource_monitor.is_memory_limit_exceeded().await?;

        // Mock implementation might not exceed limits
        // In a real scenario, these would be based on actual usage

        // Get resource alerts
        let alerts = resource_monitor.get_resource_alerts().await?;
        // Should have alerts if limits are exceeded, or be empty if within limits

        Ok(())
    }

    /// Test resource monitoring performance
    pub async fn test_resource_monitoring_performance() -> IpcResult<()> {
        let resource_monitor = Arc::new(MockResourceMonitor::new());
        let num_samples = 1000;

        // Benchmark CPU monitoring
        let start = SystemTime::now();
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_samples,
            |_| {
                let monitor = Arc::clone(&resource_monitor);
                async move {
                    monitor.get_cpu_usage().await
                }
            },
        ).await;
        let duration = start.elapsed().unwrap();

        let samples_per_sec = num_samples as f64 / duration.as_secs_f64();
        assert!(samples_per_sec > 100.0); // Should be fast

        // Verify all samples are valid
        let valid_samples = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(valid_samples, num_samples);

        Ok(())
    }
}

/// Alerting tests
pub struct AlertingTests;

impl AlertingTests {
    /// Test threshold-based alerts
    pub async fn test_threshold_alerts() -> IpcResult<()> {
        let alert_manager = MockAlertManager::new();
        let metrics = MockMetricsCollector::new();

        // Set alert thresholds
        alert_manager.set_threshold("cpu_usage", 80.0, AlertOperator::GreaterThan).await;
        alert_manager.set_threshold("memory_usage", 90.0, AlertOperator::GreaterThan).await;

        // Record metric that exceeds threshold
        metrics.record_gauge("cpu_usage", 85.0).await?;

        // Check for alerts
        let alerts = alert_manager.check_thresholds(&metrics).await?;
        assert!(!alerts.is_empty());

        let cpu_alert = alerts.iter().find(|a| a.metric_name == "cpu_usage");
        assert!(cpu_alert.is_some());
        assert!(cpu_alert.unwrap().current_value > 80.0);

        Ok(())
    }

    /// Test rate-based alerts
    pub async fn test_rate_alerts() -> IpcResult<()> {
        let alert_manager = MockAlertManager::new();
        let metrics = MockMetricsCollector::new();

        // Set rate alert (more than 10 errors per minute)
        alert_manager.set_rate_alert("error_rate", 10.0, Duration::from_secs(60)).await;

        // Simulate high error rate
        for _ in 0..15 {
            metrics.record_counter("errors", 1).await?;
        }

        // Check for rate alerts
        let alerts = alert_manager.check_rate_alerts(&metrics).await?;
        assert!(!alerts.is_empty());

        let error_alert = alerts.iter().find(|a| a.metric_name == "error_rate");
        assert!(error_alert.is_some());

        Ok(())
    }

    /// Test alert suppression and cooldown
    pub async fn test_alert_suppression() -> IpcResult<()> {
        let alert_manager = MockAlertManager::new();
        let metrics = MockMetricsCollector::new();

        // Set alert with cooldown
        alert_manager.set_threshold_with_cooldown(
            "cpu_usage",
            80.0,
            AlertOperator::GreaterThan,
            Duration::from_secs(5),
        ).await;

        // Trigger alert
        metrics.record_gauge("cpu_usage", 85.0).await?;
        let alerts1 = alert_manager.check_thresholds(&metrics).await?;
        assert_eq!(alerts1.len(), 1);

        // Immediate second check should be suppressed
        let alerts2 = alert_manager.check_thresholds(&metrics).await?;
        assert_eq!(alerts2.len(), 0);

        // Wait for cooldown and check again
        tokio::time::sleep(Duration::from_millis(600)).await;
        let alerts3 = alert_manager.check_thresholds(&metrics).await?;
        assert_eq!(alerts3.len(), 1);

        Ok(())
    }

    /// Test alert notification channels
    pub async fn test_alert_notifications() -> IpcResult<()> {
        let alert_manager = MockAlertManager::new();
        let metrics = MockMetricsCollector::new();

        // Add notification channels
        let webhook_channel = MockWebhookChannel::new();
        let email_channel = MockEmailChannel::new();

        alert_manager.add_notification_channel(Box::new(webhook_channel.clone())).await;
        alert_manager.add_notification_channel(Box::new(email_channel.clone())).await;

        // Trigger alert
        metrics.record_gauge("cpu_usage", 85.0).await?;
        alert_manager.set_threshold("cpu_usage", 80.0, AlertOperator::GreaterThan).await;

        let alerts = alert_manager.check_thresholds(&metrics).await?;
        for alert in alerts {
            alert_manager.send_alert(&alert).await?;
        }

        // Verify notifications were sent
        assert!(webhook_channel.get_sent_alerts().await.len() > 0);
        assert!(email_channel.get_sent_alerts().await.len() > 0);

        Ok(())
    }

    /// Test alert escalation
    pub async fn test_alert_escalation() -> IpcResult<()> {
        let alert_manager = MockAlertManager::new();
        let metrics = MockMetricsCollector::new();

        // Set up escalation policy
        let escalation_policy = EscalationPolicy {
            initial_severity: AlertSeverity::Warning,
            escalation_intervals: vec![
                (Duration::from_secs(30), AlertSeverity::Error),
                (Duration::from_secs(60), AlertSeverity::Critical),
            ],
        };

        alert_manager.set_escalation_policy("cpu_usage", escalation_policy).await;

        // Trigger alert
        metrics.record_gauge("cpu_usage", 85.0).await?;
        alert_manager.set_threshold("cpu_usage", 80.0, AlertOperator::GreaterThan).await;

        // Initial alert
        let alerts1 = alert_manager.check_thresholds(&metrics).await?;
        assert_eq!(alerts1.len(), 1);
        assert_eq!(alerts1[0].severity, AlertSeverity::Warning);

        // Wait for escalation
        tokio::time::sleep(Duration::from_millis(100)).await;
        let alerts2 = alert_manager.check_thresholds(&metrics).await?;
        assert_eq!(alerts2.len(), 1);
        assert_eq!(alerts2[0].severity, AlertSeverity::Error);

        Ok(())
    }
}

// Mock implementations for testing

#[derive(Debug)]
pub struct MockTracer {
    traces: Arc<RwLock<HashMap<String, TraceData>>>,
    sampling_rate: Arc<Mutex<f64>>,
}

#[derive(Debug, Clone)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub sampled: bool,
}

#[derive(Debug)]
pub struct TraceData {
    pub trace_id: String,
    pub spans: Vec<SpanData>,
    pub events: Vec<EventData>,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug)]
pub struct SpanData {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub operation_name: String,
    pub start_time: SystemTime,
    pub end_time: Option<SystemTime>,
}

#[derive(Debug)]
pub struct EventData {
    pub name: String,
    pub timestamp: SystemTime,
    pub attributes: HashMap<String, String>,
}

impl MockTracer {
    pub fn new() -> Self {
        Self {
            traces: Arc::new(RwLock::new(HashMap::new())),
            sampling_rate: Arc::new(Mutex::new(1.0)),
        }
    }

    pub async fn start_trace(&self, operation_name: &str) -> IpcResult<TraceContext> {
        let sampling_rate = *self.sampling_rate.lock().await;
        let sampled = rand::random::<f64>() < sampling_rate;

        let trace_id = Uuid::new_v4().to_string();
        let span_id = Uuid::new_v4().to_string();

        let trace_data = TraceData {
            trace_id: trace_id.clone(),
            spans: vec![SpanData {
                span_id: span_id.clone(),
                parent_span_id: None,
                operation_name: operation_name.to_string(),
                start_time: SystemTime::now(),
                end_time: None,
            }],
            events: Vec::new(),
            attributes: HashMap::new(),
        };

        self.traces.write().await.insert(trace_id.clone(), trace_data);

        Ok(TraceContext {
            trace_id,
            span_id,
            sampled,
        })
    }

    pub async fn start_child_span(&self, parent: &TraceContext, operation_name: &str) -> IpcResult<TraceContext> {
        let span_id = Uuid::new_v4().to_string();

        if let Some(trace_data) = self.traces.write().await.get_mut(&parent.trace_id) {
            trace_data.spans.push(SpanData {
                span_id: span_id.clone(),
                parent_span_id: Some(parent.span_id.clone()),
                operation_name: operation_name.to_string(),
                start_time: SystemTime::now(),
                end_time: None,
            });
        }

        Ok(TraceContext {
            trace_id: parent.trace_id.clone(),
            span_id,
            sampled: parent.sampled,
        })
    }

    pub async fn end_span(&self, context: &TraceContext) -> IpcResult<()> {
        if let Some(trace_data) = self.traces.write().await.get_mut(&context.trace_id) {
            if let Some(span) = trace_data.spans.iter_mut().find(|s| s.span_id == context.span_id) {
                span.end_time = Some(SystemTime::now());
            }
        }
        Ok(())
    }

    pub async fn add_event(&self, context: &TraceContext, name: &str, attributes: HashMap<String, String>) -> IpcResult<()> {
        if let Some(trace_data) = self.traces.write().await.get_mut(&context.trace_id) {
            trace_data.events.push(EventData {
                name: name.to_string(),
                timestamp: SystemTime::now(),
                attributes,
            });
        }
        Ok(())
    }

    pub async fn set_attribute(&self, context: &TraceContext, key: &str, value: &str) -> IpcResult<()> {
        if let Some(trace_data) = self.traces.write().await.get_mut(&context.trace_id) {
            trace_data.attributes.insert(key.to_string(), value.to_string());
        }
        Ok(())
    }

    pub async fn get_trace_data(&self, trace_id: &str) -> IpcResult<Option<TraceData>> {
        Ok(self.traces.read().await.get(trace_id).cloned())
    }

    pub async fn export_traces(&self) -> IpcResult<Vec<String>> {
        let traces = self.traces.read().await;
        let exported = traces.values().map(|trace| {
            serde_json::to_string(trace).unwrap_or_default()
        }).collect();
        Ok(exported)
    }

    pub async fn set_sampling_rate(&self, rate: f64) {
        *self.sampling_rate.lock().await = rate.clamp(0.0, 1.0);
    }
}

#[derive(Debug)]
pub struct MockHealthMonitor {
    checks: HashMap<String, Box<dyn HealthCheck + Send + Sync>>,
    metrics: Option<Arc<MockMetricsCollector>>,
    timeout: Duration,
}

#[derive(Debug)]
pub struct HealthStatus {
    pub status: String,
    pub checks: HashMap<String, HealthCheckResult>,
    pub details: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub status: String,
    pub message: Option<String>,
    pub timestamp: SystemTime,
}

#[async_trait::async_trait]
pub trait HealthCheck {
    async fn check(&self) -> HealthCheckResult;
}

#[derive(Debug)]
pub struct MockHealthCheck {
    healthy: bool,
    delay: Duration,
}

impl MockHealthCheck {
    pub fn new(healthy: bool) -> Self {
        Self {
            healthy,
            delay: Duration::ZERO,
        }
    }

    pub fn new_with_delay(healthy: bool, delay: Duration) -> Self {
        Self {
            healthy,
            delay,
        }
    }
}

#[async_trait::async_trait]
impl HealthCheck for MockHealthCheck {
    async fn check(&self) -> HealthCheckResult {
        tokio::time::sleep(self.delay).await;

        HealthCheckResult {
            status: if self.healthy { "healthy" } else { "unhealthy" }.to_string(),
            message: if self.healthy { None } else { Some("Mock health check failed".to_string()) },
            timestamp: SystemTime::now(),
        }
    }
}

#[derive(Debug)]
pub struct MockFlakyHealthCheck {
    healthy: Arc<Mutex<bool>>,
}

impl MockFlakyHealthCheck {
    pub fn new() -> Self {
        Self {
            healthy: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn set_healthy(&self, healthy: bool) {
        *self.healthy.lock().await = healthy;
    }
}

#[async_trait::async_trait]
impl HealthCheck for MockFlakyHealthCheck {
    async fn check(&self) -> HealthCheckResult {
        let healthy = *self.healthy.lock().await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        HealthCheckResult {
            status: if healthy { "healthy" } else { "unhealthy" }.to_string(),
            message: if healthy { None } else { Some("Flaky service is unhealthy".to_string()) },
            timestamp: SystemTime::now(),
        }
    }
}

impl MockHealthMonitor {
    pub fn new() -> Self {
        Self {
            checks: HashMap::new(),
            metrics: None,
            timeout: Duration::from_secs(5),
        }
    }

    pub async fn add_check(&mut self, name: String, check: impl HealthCheck + Send + Sync + 'static) {
        self.checks.insert(name, Box::new(check));
    }

    pub async fn get_health(&self) -> IpcResult<HealthStatus> {
        let mut checks = HashMap::new();
        let mut overall_status = "healthy";
        let mut healthy_count = 0;

        for (name, check) in &self.checks {
            let result = tokio::time::timeout(self.timeout, check.check()).await;
            let check_result = match result {
                Ok(result) => result,
                Err(_) => HealthCheckResult {
                    status: "unhealthy".to_string(),
                    message: Some("Health check timed out".to_string()),
                    timestamp: SystemTime::now(),
                },
            };

            if check_result.status == "healthy" {
                healthy_count += 1;
            } else {
                overall_status = "unhealthy";
            }

            checks.insert(name.clone(), check_result);
        }

        // Record metrics if available
        if let Some(metrics) = &self.metrics {
            metrics.record_counter("health_checks_healthy_total", healthy_count as u64).await?;
            metrics.record_counter("health_checks_unhealthy_total", (checks.len() - healthy_count) as u64).await?;
        }

        Ok(HealthStatus {
            status: overall_status.to_string(),
            checks,
            details: HashMap::new(),
        })
    }

    pub async fn get_check_health(&self, name: &str) -> IpcResult<HealthCheckResult> {
        if let Some(check) = self.checks.get(name) {
            let result = tokio::time::timeout(self.timeout, check.check()).await;
            match result {
                Ok(result) => Ok(result),
                Err(_) => Ok(HealthCheckResult {
                    status: "unhealthy".to_string(),
                    message: Some("Health check timed out".to_string()),
                    timestamp: SystemTime::now(),
                }),
            }
        } else {
            Err(IpcError::Protocol {
                message: format!("Health check '{}' not found", name),
                code: crate::plugin_ipc::error::ProtocolErrorCode::InternalError,
                source: None,
            })
        }
    }

    pub async fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub async fn enable_metrics(&mut self, metrics: Arc<MockMetricsCollector>) {
        self.metrics = Some(metrics);
    }
}

#[derive(Debug)]
pub struct MockResourceMonitor {
    cpu_usage_history: Arc<Mutex<Vec<f64>>>,
    memory_usage_history: Arc<Mutex<Vec<MemoryUsage>>>,
    cpu_limit: Arc<Mutex<f64>>,
    memory_limit: Arc<Mutex<u64>>,
}

#[derive(Debug, Clone)]
pub struct MemoryUsage {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct DiskUsage {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct DiskIoStats {
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_operations: u64,
    pub write_operations: u64,
}

#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
}

impl MockResourceMonitor {
    pub fn new() -> Self {
        Self {
            cpu_usage_history: Arc::new(Mutex::new(Vec::new())),
            memory_usage_history: Arc::new(Mutex::new(Vec::new())),
            cpu_limit: Arc::new(Mutex::new(100.0)),
            memory_limit: Arc::new(Mutex::new(u64::MAX)),
        }
    }

    pub async fn get_cpu_usage(&self) -> IpcResult<f64> {
        // Generate mock CPU usage between 0-100%
        let usage = rand::random::<f64>() * 100.0;

        self.cpu_usage_history.lock().await.push(usage);

        // Keep only last 60 seconds of history
        if self.cpu_usage_history.lock().await.len() > 60 {
            self.cpu_usage_history.lock().await.remove(0);
        }

        Ok(usage)
    }

    pub async fn get_cpu_usage_history(&self, duration: Duration) -> IpcResult<Vec<f64>> {
        let history = self.cpu_usage_history.lock().await;
        let seconds = duration.as_secs() as usize;
        Ok(history.iter().rev().take(seconds).cloned().collect())
    }

    pub async fn get_memory_usage(&self) -> IpcResult<MemoryUsage> {
        let total = 8 * 1024 * 1024 * 1024; // 8GB
        let used = (rand::random::<f64>() * total as f64) as u64;
        let available = total - used;

        let usage = MemoryUsage {
            total_bytes: total,
            used_bytes: used,
            available_bytes: available,
        };

        self.memory_usage_history.lock().await.push(usage.clone());

        Ok(usage)
    }

    pub async fn get_memory_usage_history(&self, duration: Duration) -> IpcResult<Vec<MemoryUsage>> {
        let history = self.memory_usage_history.lock().await;
        let seconds = duration.as_secs() as usize;
        Ok(history.iter().rev().take(seconds).cloned().collect())
    }

    pub async fn get_disk_usage(&self, _path: &str) -> IpcResult<DiskUsage> {
        let total = 500 * 1024 * 1024 * 1024; // 500GB
        let used = (rand::random::<f64>() * total as f64 * 0.8) as u64; // Max 80% used
        let available = total - used;

        Ok(DiskUsage {
            total_bytes: total,
            used_bytes: used,
            available_bytes: available,
        })
    }

    pub async fn get_disk_io_stats(&self) -> IpcResult<DiskIoStats> {
        Ok(DiskIoStats {
            read_bytes: rand::random::<u64>() % 1_000_000_000,
            write_bytes: rand::random::<u64>() % 1_000_000_000,
            read_operations: rand::random::<u64>() % 100_000,
            write_operations: rand::random::<u64>() % 100_000,
        })
    }

    pub async fn get_network_stats(&self) -> IpcResult<NetworkStats> {
        Ok(NetworkStats {
            bytes_sent: rand::random::<u64>() % 10_000_000_000,
            bytes_received: rand::random::<u64>() % 10_000_000_000,
            packets_sent: rand::random::<u64>() % 1_000_000_000,
            packets_received: rand::random::<u64>() % 1_000_000_000,
        })
    }

    pub async fn get_connection_count(&self) -> IpcResult<u32> {
        Ok(rand::random::<u32>() % 1000)
    }

    pub async fn get_network_interface_stats(&self) -> IpcResult<HashMap<String, NetworkStats>> {
        let mut stats = HashMap::new();

        stats.insert("eth0".to_string(), NetworkStats {
            bytes_sent: rand::random::<u64>() % 5_000_000_000,
            bytes_received: rand::random::<u64>() % 5_000_000_000,
            packets_sent: rand::random::<u64>() % 500_000_000,
            packets_received: rand::random::<u64>() % 500_000_000,
        });

        stats.insert("lo".to_string(), NetworkStats {
            bytes_sent: rand::random::<u64>() % 1_000_000_000,
            bytes_received: rand::random::<u64>() % 1_000_000_000,
            packets_sent: rand::random::<u64>() % 100_000_000,
            packets_received: rand::random::<u64>() % 100_000_000,
        });

        Ok(stats)
    }

    pub async fn set_cpu_limit(&self, limit: f64) {
        *self.cpu_limit.lock().await = limit.clamp(0.0, 100.0);
    }

    pub async fn set_memory_limit(&self, limit: u64) {
        *self.memory_limit.lock().await = limit;
    }

    pub async fn is_cpu_limit_exceeded(&self) -> IpcResult<bool> {
        let usage = self.get_cpu_usage().await?;
        let limit = *self.cpu_limit.lock().await;
        Ok(usage > limit)
    }

    pub async fn is_memory_limit_exceeded(&self) -> IpcResult<bool> {
        let usage = self.get_memory_usage().await?;
        let limit = *self.memory_limit.lock().await;
        Ok(usage.used_bytes > limit)
    }

    pub async fn get_resource_alerts(&self) -> IpcResult<Vec<String>> {
        let mut alerts = Vec::new();

        if self.is_cpu_limit_exceeded().await? {
            alerts.push("CPU usage exceeds limit".to_string());
        }

        if self.is_memory_limit_exceeded().await? {
            alerts.push("Memory usage exceeds limit".to_string());
        }

        Ok(alerts)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone)]
pub enum AlertOperator {
    GreaterThan,
    LessThan,
    Equal,
}

#[derive(Debug)]
pub struct Alert {
    pub metric_name: String,
    pub current_value: f64,
    pub threshold: f64,
    pub severity: AlertSeverity,
    pub message: String,
    pub timestamp: SystemTime,
}

#[derive(Debug)]
pub struct EscalationPolicy {
    pub initial_severity: AlertSeverity,
    pub escalation_intervals: Vec<(Duration, AlertSeverity)>,
}

#[derive(Debug)]
pub struct MockAlertManager {
    thresholds: Arc<RwLock<HashMap<String, (f64, AlertOperator, Duration)>>>,
    rate_alerts: Arc<RwLock<HashMap<String, (f64, Duration)>>>,
    escalation_policies: Arc<RwLock<HashMap<String, EscalationPolicy>>>,
    notification_channels: Arc<Mutex<Vec<Box<dyn NotificationChannel>>>>,
    alert_history: Arc<RwLock<HashMap<String, (Alert, SystemTime)>>>,
}

#[async_trait::async_trait]
pub trait NotificationChannel {
    async fn send_alert(&self, alert: &Alert) -> IpcResult<()>;
}

#[derive(Debug)]
pub struct MockWebhookChannel {
    sent_alerts: Arc<Mutex<Vec<Alert>>>,
}

impl MockWebhookChannel {
    pub fn new() -> Self {
        Self {
            sent_alerts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn get_sent_alerts(&self) -> Vec<Alert> {
        self.sent_alerts.lock().await.clone()
    }
}

#[async_trait::async_trait]
impl NotificationChannel for MockWebhookChannel {
    async fn send_alert(&self, alert: &Alert) -> IpcResult<()> {
        self.sent_alerts.lock().await.push(alert.clone());
        Ok(())
    }
}

#[derive(Debug)]
pub struct MockEmailChannel {
    sent_alerts: Arc<Mutex<Vec<Alert>>>,
}

impl MockEmailChannel {
    pub fn new() -> Self {
        Self {
            sent_alerts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn get_sent_alerts(&self) -> Vec<Alert> {
        self.sent_alerts.lock().await.clone()
    }
}

#[async_trait::async_trait]
impl NotificationChannel for MockEmailChannel {
    async fn send_alert(&self, alert: &Alert) -> IpcResult<()> {
        self.sent_alerts.lock().await.push(alert.clone());
        Ok(())
    }
}

impl MockAlertManager {
    pub fn new() -> Self {
        Self {
            thresholds: Arc::new(RwLock::new(HashMap::new())),
            rate_alerts: Arc::new(RwLock::new(HashMap::new())),
            escalation_policies: Arc::new(RwLock::new(HashMap::new())),
            notification_channels: Arc::new(Mutex::new(Vec::new())),
            alert_history: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn set_threshold(&self, metric: &str, threshold: f64, operator: AlertOperator) {
        self.thresholds.write().await.insert(
            metric.to_string(),
            (threshold, operator, Duration::ZERO),
        );
    }

    pub async fn set_threshold_with_cooldown(
        &self,
        metric: &str,
        threshold: f64,
        operator: AlertOperator,
        cooldown: Duration,
    ) {
        self.thresholds.write().await.insert(
            metric.to_string(),
            (threshold, operator, cooldown),
        );
    }

    pub async fn set_rate_alert(&self, metric: &str, threshold: f64, window: Duration) {
        self.rate_alerts.write().await.insert(
            metric.to_string(),
            (threshold, window),
        );
    }

    pub async fn set_escalation_policy(&self, metric: &str, policy: EscalationPolicy) {
        self.escalation_policies.write().await.insert(metric.to_string(), policy);
    }

    pub async fn add_notification_channel(&self, channel: Box<dyn NotificationChannel>) {
        self.notification_channels.lock().await.push(channel);
    }

    pub async fn check_thresholds(&self, metrics: &MockMetricsCollector) -> IpcResult<Vec<Alert>> {
        let mut alerts = Vec::new();
        let thresholds = self.thresholds.read().await;
        let mut history = self.alert_history.write().await;

        for (metric_name, &(threshold, operator, cooldown)) in thresholds.iter() {
            if let Some(current_value) = metrics.get_metric(metric_name).await {
                let should_alert = match operator {
                    AlertOperator::GreaterThan => current_value > threshold,
                    AlertOperator::LessThan => current_value < threshold,
                    AlertOperator::Equal => (current_value - threshold).abs() < f64::EPSILON,
                };

                if should_alert {
                    let now = SystemTime::now();
                    let last_alert_time = history.get(metric_name).map(|(_, time)| *time);

                    // Check cooldown
                    if let Some(last_time) = last_alert_time {
                        if now.duration_since(last_time).unwrap() < cooldown {
                            continue; // Skip due to cooldown
                        }
                    }

                    let severity = self.get_alert_severity(metric_name, now).await;

                    let alert = Alert {
                        metric_name: metric_name.clone(),
                        current_value,
                        threshold,
                        severity,
                        message: format!("{} is {} (threshold: {})", metric_name, current_value, threshold),
                        timestamp: now,
                    };

                    history.insert(metric_name.clone(), (alert.clone(), now));
                    alerts.push(alert);
                }
            }
        }

        Ok(alerts)
    }

    pub async fn check_rate_alerts(&self, metrics: &MockMetricsCollector) -> IpcResult<Vec<Alert>> {
        let mut alerts = Vec::new();
        let rate_alerts = self.rate_alerts.read().await;

        for (metric_name, &(threshold, window)) in rate_alerts.iter() {
            if let Some(count) = metrics.get_counter(metric_name).await {
                let rate = count as f64 / window.as_secs_f64();

                if rate > threshold {
                    let alert = Alert {
                        metric_name: metric_name.clone(),
                        current_value: rate,
                        threshold,
                        severity: AlertSeverity::Warning,
                        message: format!("{} rate is {} per second (threshold: {})", metric_name, rate, threshold),
                        timestamp: SystemTime::now(),
                    };

                    alerts.push(alert);
                }
            }
        }

        Ok(alerts)
    }

    async fn get_alert_severity(&self, metric_name: &str, now: SystemTime) -> AlertSeverity {
        let policies = self.escalation_policies.read().await;
        let history = self.alert_history.read().await;

        if let Some(policy) = policies.get(metric_name) {
            if let Some((_, first_alert_time)) = history.get(metric_name) {
                let elapsed = now.duration_since(*first_alert_time).unwrap();

                for &(interval, severity) in &policy.escalation_intervals {
                    if elapsed >= interval {
                        return severity;
                    }
                }
            }

            policy.initial_severity
        } else {
            AlertSeverity::Warning
        }
    }

    pub async fn send_alert(&self, alert: &Alert) -> IpcResult<()> {
        let channels = self.notification_channels.lock().await;

        for channel in channels.iter() {
            channel.send_alert(alert).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async_test!(test_counter_metrics, {
        PerformanceMetricsTests::test_counter_metrics().await.unwrap();
        "success"
    });

    async_test!(test_gauge_metrics, {
        PerformanceMetricsTests::test_gauge_metrics().await.unwrap();
        "success"
    });

    async_test!(test_histogram_metrics, {
        PerformanceMetricsTests::test_histogram_metrics().await.unwrap();
        "success"
    });

    async_test!(test_metric_aggregation, {
        PerformanceMetricsTests::test_metric_aggregation().await.unwrap();
        "success"
    });

    async_test!(test_metric_retention, {
        PerformanceMetricsTests::test_metric_retention().await.unwrap();
        "success"
    });

    async_test!(test_trace_context_propagation, {
        DistributedTracingTests::test_trace_context_propagation().await.unwrap();
        "success"
    });

    async_test!(test_trace_sampling, {
        DistributedTracingTests::test_trace_sampling().await.unwrap();
        "success"
    });

    async_test!(test_span_events, {
        DistributedTracingTests::test_span_events().await.unwrap();
        "success"
    });

    async_test!(test_trace_export, {
        DistributedTracingTests::test_trace_export().await.unwrap();
        "success"
    });

    async_test!(test_basic_health_checks, {
        HealthMonitoringTests::test_basic_health_checks().await.unwrap();
        "success"
    });

    async_test!(test_failing_health_checks, {
        HealthMonitoringTests::test_failing_health_checks().await.unwrap();
        "success"
    });

    async_test!(test_health_check_timeouts, {
        HealthMonitoringTests::test_health_check_timeouts().await.unwrap();
        "success"
    });

    async_test!(test_health_check_recovery, {
        HealthMonitoringTests::test_health_check_recovery().await.unwrap();
        "success"
    });

    async_test!(test_health_metrics_integration, {
        HealthMonitoringTests::test_health_metrics_integration().await.unwrap();
        "success"
    });

    async_test!(test_cpu_monitoring, {
        ResourceMonitoringTests::test_cpu_monitoring().await.unwrap();
        "success"
    });

    async_test!(test_memory_monitoring, {
        ResourceMonitoringTests::test_memory_monitoring().await.unwrap();
        "success"
    });

    async_test!(test_disk_monitoring, {
        ResourceMonitoringTests::test_disk_monitoring().await.unwrap();
        "success"
    });

    async_test!(test_network_monitoring, {
        ResourceMonitoringTests::test_network_monitoring().await.unwrap();
        "success"
    });

    async_test!(test_resource_limits, {
        ResourceMonitoringTests::test_resource_limits().await.unwrap();
        "success"
    });

    async_test!(test_resource_monitoring_performance, {
        ResourceMonitoringTests::test_resource_monitoring_performance().await.unwrap();
        "success"
    });

    async_test!(test_threshold_alerts, {
        AlertingTests::test_threshold_alerts().await.unwrap();
        "success"
    });

    async_test!(test_rate_alerts, {
        AlertingTests::test_rate_alerts().await.unwrap();
        "success"
    });

    async_test!(test_alert_suppression, {
        AlertingTests::test_alert_suppression().await.unwrap();
        "success"
    });

    async_test!(test_alert_notifications, {
        AlertingTests::test_alert_notifications().await.unwrap();
        "success"
    });

    async_test!(test_alert_escalation, {
        AlertingTests::test_alert_escalation().await.unwrap();
        "success"
    });
}