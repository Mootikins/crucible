//! Test utilities and mocks for daemon integration tests

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use chrono::Utc;
use async_trait::async_trait;
use anyhow::Result;

// Import crucible-services types
use crucible_services::events::{
    EventRouter, EventBus, EventHandler, EventResult,
    ServiceRegistration, RoutingRule, RoutingStatistics
};
use crucible_services::types::ServiceHealth;

use crate::coordinator::{ServiceInfo, DaemonHealth};
use crate::events::{DaemonEvent, EventBuilder};

/// Mock event router for testing
#[derive(Clone)]
pub struct MockEventRouter {
    pub events_sent: Arc<RwLock<Vec<crucible_services::events::core::DaemonEvent>>>,
    pub routing_failures: Arc<RwLock<bool>>,
    pub routing_stats: Arc<RwLock<HashMap<String, u64>>>,
    pub service_registrations: Arc<RwLock<HashMap<String, ServiceRegistration>>>,
}

impl MockEventRouter {
    pub fn new() -> Self {
        Self {
            events_sent: Arc::new(RwLock::new(Vec::new())),
            routing_failures: Arc::new(RwLock::new(false)),
            routing_stats: Arc::new(RwLock::new(HashMap::new())),
            service_registrations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_events_sent(&self) -> Vec<crucible_services::events::core::DaemonEvent> {
        self.events_sent.read().await.clone()
    }

    pub async fn set_routing_failure(&self, should_fail: bool) {
        *self.routing_failures.write().await = should_fail;
    }

    pub async fn get_routing_stats(&self) -> HashMap<String, u64> {
        self.routing_stats.read().await.clone()
    }

    pub async fn clear_events(&self) {
        self.events_sent.write().await.clear();
    }

    pub async fn get_service_registrations(&self) -> HashMap<String, ServiceRegistration> {
        self.service_registrations.read().await.clone()
    }
}

#[async_trait]
impl EventRouter for MockEventRouter {
    async fn register_service(&self, registration: ServiceRegistration) -> EventResult<()> {
        let mut registrations = self.service_registrations.write().await;
        registrations.insert(registration.service_id.clone(), registration);
        Ok(())
    }

    async fn unregister_service(&self, service_id: &str) -> EventResult<()> {
        let mut registrations = self.service_registrations.write().await;
        registrations.remove(service_id);
        Ok(())
    }

    async fn route_event(&self, event: crucible_services::events::core::DaemonEvent) -> EventResult<()> {
        let should_fail = *self.routing_failures.read().await;
        if should_fail {
            return Err(anyhow::anyhow!("Mock routing failure for testing"));
        }

        let mut events = self.events_sent.write().await;
        events.push(event.clone());

        // Update stats
        let mut stats = self.routing_stats.write().await;
        *stats.entry("total_events_routed".to_string()).or_insert(0) += 1;
        *stats.entry("events_routed_last_minute".to_string()).or_insert(0) += 1;

        Ok(())
    }

    async fn add_routing_rule(&self, rule: RoutingRule) -> EventResult<()> {
        Ok(())
    }

    async fn remove_routing_rule(&self, rule_id: &str) -> EventResult<()> {
        Ok(())
    }

    async fn get_routing_stats(&self) -> EventResult<RoutingStatistics> {
        let stats = self.routing_stats.read().await;
        Ok(RoutingStatistics {
            total_events_routed: stats.get("total_events_routed").copied().unwrap_or(0),
            events_routed_last_minute: stats.get("events_routed_last_minute").copied().unwrap_or(0),
            events_routed_last_hour: stats.get("events_routed_last_hour").copied().unwrap_or(0),
            error_rate: 0.0,
            average_routing_time_ms: 1.0,
            service_stats: HashMap::new(),
        })
    }

    async fn update_service_health(&self, service_id: &str, health: ServiceHealth) -> EventResult<()> {
        Ok(())
    }

    async fn test_routing(&self, event: &crucible_services::events::core::DaemonEvent) -> EventResult<Vec<String>> {
        Ok(vec!["test_service".to_string()])
    }
}

/// Mock event bus for testing
#[derive(Clone)]
pub struct MockEventBus {
    pub subscribers: Arc<RwLock<Vec<Arc<dyn EventHandler>>>>,
    pub events_published: Arc<RwLock<Vec<crucible_services::events::core::DaemonEvent>>>,
    pub publish_failures: Arc<RwLock<bool>>,
}

impl MockEventBus {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(Vec::new())),
            events_published: Arc::new(RwLock::new(Vec::new())),
            publish_failures: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn get_events_published(&self) -> Vec<crucible_services::events::core::DaemonEvent> {
        self.events_published.read().await.clone()
    }

    pub async fn set_publish_failure(&self, should_fail: bool) {
        *self.publish_failures.write().await = should_fail;
    }

    pub async fn clear_events(&self) {
        self.events_published.write().await.clear();
    }

    pub async fn get_subscriber_count(&self) -> usize {
        self.subscribers.read().await.len()
    }
}

#[async_trait]
impl EventBus for MockEventBus {
    async fn publish(&self, event: crucible_services::events::core::DaemonEvent) -> EventResult<()> {
        let should_fail = *self.publish_failures.read().await;
        if should_fail {
            return Err(anyhow::anyhow!("Mock publish failure for testing"));
        }

        let mut events = self.events_published.write().await;
        events.push(event.clone());

        // Notify subscribers
        let subscribers = self.subscribers.read().await;
        for subscriber in subscribers.iter() {
            let _ = subscriber.handle_event(event.clone()).await;
        }

        Ok(())
    }

    async fn subscribe(&self, handler: Arc<dyn EventHandler>) -> EventResult<()> {
        let mut subscribers = self.subscribers.write().await;
        subscribers.push(handler);
        Ok(())
    }

    async fn unsubscribe(&self, handler_id: &str) -> EventResult<()> {
        let mut subscribers = self.subscribers.write().await;
        subscribers.retain(|h| h.name() != handler_id);
        Ok(())
    }

    async fn get_subscribers(&self) -> EventResult<Vec<String>> {
        let subscribers = self.subscribers.read().await;
        Ok(subscribers.iter().map(|h| h.name().to_string()).collect())
    }
}

/// Mock event handler for testing
pub struct MockEventHandler {
    pub name: String,
    pub events_handled: Arc<RwLock<Vec<crucible_services::events::core::DaemonEvent>>>,
    pub handle_failures: Arc<RwLock<bool>>,
    pub priority: u32,
}

impl MockEventHandler {
    pub fn new(name: &str, priority: u32) -> Self {
        Self {
            name: name.to_string(),
            events_handled: Arc::new(RwLock::new(Vec::new())),
            handle_failures: Arc::new(RwLock::new(false)),
            priority,
        }
    }

    pub async fn get_events_handled(&self) -> Vec<crucible_services::events::core::DaemonEvent> {
        self.events_handled.read().await.clone()
    }

    pub async fn set_handle_failure(&self, should_fail: bool) {
        *self.handle_failures.write().await = should_fail;
    }

    pub async fn clear_events(&self) {
        self.events_handled.write().await.clear();
    }
}

#[async_trait]
impl EventHandler for MockEventHandler {
    async fn handle_event(&self, event: crucible_services::events::core::DaemonEvent) -> EventResult<()> {
        let should_fail = *self.handle_failures.read().await;
        if should_fail {
            return Err(anyhow::anyhow!("Mock handler failure for testing"));
        }

        let mut events = self.events_handled.write().await;
        events.push(event);
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> u32 {
        self.priority
    }
}

/// Test data builders
pub struct TestDataBuilder;

impl TestDataBuilder {
    /// Create a test filesystem event
    pub fn create_filesystem_event(path: &str) -> DaemonEvent {
        DaemonEvent::Filesystem(crate::events::FilesystemEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: crate::events::FilesystemEventType::Created,
            path: std::path::PathBuf::from(path),
            metadata: crate::events::FileMetadata::default(),
            data: HashMap::new(),
        })
    }

    /// Create a test service registration event
    pub fn create_service_registration_event(service_id: &str, service_type: &str) -> DaemonEvent {
        DaemonEvent::new(
            crucible_services::events::core::EventType::Service(
                crucible_services::events::core::ServiceEventType::ServiceRegistered {
                    service_id: service_id.to_string(),
                    service_type: service_type.to_string(),
                }
            ),
            crucible_services::events::core::EventSource::service("test-source".to_string()),
            crucible_services::events::core::EventPayload::json(serde_json::json!({
                "test": true,
                "timestamp": Utc::now().to_rfc3339()
            }))
        )
    }

    /// Create a test service unregistration event
    pub fn create_service_unregistration_event(service_id: &str) -> DaemonEvent {
        DaemonEvent::new(
            crucible_services::events::core::EventType::Service(
                crucible_services::events::core::ServiceEventType::ServiceUnregistered {
                    service_id: service_id.to_string(),
                }
            ),
            crucible_services::events::core::EventSource::service("test-source".to_string()),
            crucible_services::events::core::EventPayload::json(serde_json::json!({
                "timestamp": Utc::now().to_rfc3339()
            }))
        )
    }

    /// Create a test health check event
    pub fn create_health_check_event(service_id: &str, status: &str) -> DaemonEvent {
        DaemonEvent::new(
            crucible_services::events::core::EventType::Service(
                crucible_services::events::core::ServiceEventType::HealthCheck {
                    service_id: service_id.to_string(),
                    status: status.to_string(),
                }
            ),
            crucible_services::events::core::EventSource::service(service_id.to_string()),
            crucible_services::events::core::EventPayload::json(serde_json::json!({
                "health": "check",
                "timestamp": Utc::now().to_rfc3339()
            }))
        )
    }

    /// Create a test system startup event
    pub fn create_system_startup_event(version: &str) -> DaemonEvent {
        DaemonEvent::new(
            crucible_services::events::core::EventType::System(
                crucible_services::events::core::SystemEventType::DaemonStarted {
                    version: version.to_string(),
                }
            ),
            crucible_services::events::core::EventSource::service("daemon".to_string()),
            crucible_services::events::core::EventPayload::json(serde_json::json!({
                "startup_time": Utc::now().to_rfc3339(),
                "features": vec!["test", "mock"]
            }))
        )
    }

    /// Create a test custom event
    pub fn create_custom_event(event_type: &str, data: serde_json::Value) -> DaemonEvent {
        DaemonEvent::new(
            crucible_services::events::core::EventType::Custom(event_type.to_string()),
            crucible_services::events::core::EventSource::service("test".to_string()),
            crucible_services::events::core::EventPayload::json(data)
        )
    }

    /// Create a test service info
    pub fn create_service_info(service_id: &str, service_type: &str) -> ServiceInfo {
        ServiceInfo {
            service_id: service_id.to_string(),
            service_type: service_type.to_string(),
            instance_id: format!("{}-{}", service_id, Uuid::new_v4()),
            endpoint: Some(format!("http://localhost:8080/{}", service_id)),
            health: crucible_services::types::ServiceHealth {
                status: crucible_services::types::ServiceStatus::Healthy,
                message: Some("Test service is healthy".to_string()),
                details: HashMap::new(),
                last_check: Utc::now(),
            },
            last_seen: Utc::now(),
            capabilities: vec!["test_capability".to_string(), "mock_capability".to_string()],
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("test".to_string(), "true".to_string());
                meta.insert("mock".to_string(), "true".to_string());
                meta
            },
        }
    }

    /// Create a test daemon health
    pub fn create_daemon_health() -> DaemonHealth {
        DaemonHealth {
            status: crucible_services::types::ServiceStatus::Healthy,
            uptime_seconds: 3600,
            events_processed: 1000,
            services_connected: 5,
            last_health_check: Utc::now(),
            metrics: {
                let mut metrics = HashMap::new();
                metrics.insert("memory_usage_mb".to_string(), 100.0);
                metrics.insert("cpu_usage_percent".to_string(), 25.0);
                metrics
            },
            errors: vec![],
        }
    }
}

/// Test assertion utilities
pub struct TestAssertions;

impl TestAssertions {
    /// Assert that an event was routed
    pub async fn assert_event_routed(
        router: &MockEventRouter,
        expected_event_type: &str
    ) -> Result<()> {
        let events = router.get_events_sent().await;
        assert!(!events.is_empty(), "No events were routed");

        let matching_events: Vec<_> = events.iter()
            .filter(|e| format!("{:?}", e.event_type).contains(expected_event_type))
            .collect();

        assert!(!matching_events.is_empty(),
               "No events of type '{}' were routed. Found: {:?}",
               expected_event_type,
               events.iter().map(|e| format!("{:?}", e.event_type)).collect::<Vec<_>>());

        Ok(())
    }

    /// Assert that a service was discovered
    pub async fn assert_service_discovered(
        coordinator: &crate::coordinator::DataCoordinator,
        service_id: &str
    ) -> Result<()> {
        let services = coordinator.get_discovered_services().await;
        assert!(services.contains_key(service_id),
               "Service '{}' was not discovered. Found: {:?}",
               service_id,
               services.keys().collect::<Vec<_>>());

        Ok(())
    }

    /// Assert that daemon health is in expected state
    pub async fn assert_daemon_health_status(
        coordinator: &crate::coordinator::DataCoordinator,
        expected_status: crucible_services::types::ServiceStatus
    ) -> Result<()> {
        let health = coordinator.get_daemon_health().await;
        assert_eq!(health.status, expected_status,
                  "Expected daemon health status {:?}, got {:?}",
                  expected_status, health.status);

        Ok(())
    }

    /// Assert that events were processed
    pub async fn assert_events_processed(
        coordinator: &crate::coordinator::DataCoordinator,
        min_count: u64
    ) -> Result<()> {
        let health = coordinator.get_daemon_health().await;
        assert!(health.events_processed >= min_count,
               "Expected at least {} events processed, got {}",
               min_count, health.events_processed);

        Ok(())
    }

    /// Assert that no critical errors occurred
    pub async fn assert_no_critical_errors(
        coordinator: &crate::coordinator::DataCoordinator
    ) -> Result<()> {
        let health = coordinator.get_daemon_health().await;

        let critical_errors: Vec<_> = health.errors.iter()
            .filter(|e| e.contains("critical") || e.contains("fatal") || e.contains("panic"))
            .collect();

        assert!(critical_errors.is_empty(),
               "Found critical errors: {:?}", critical_errors);

        Ok(())
    }
}

/// Performance testing utilities
pub struct PerformanceTestUtils;

impl PerformanceTestUtils {
    /// Measure execution time of an async operation
    pub async fn measure_time<F, Fut>(operation: F) -> (Fut::Output, std::time::Duration)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future,
    {
        let start = std::time::Instant::now();
        let result = operation().await;
        let elapsed = start.elapsed();
        (result, elapsed)
    }

    /// Run load test with specified parameters
    pub async fn run_load_test<F, Fut>(
        operation: F,
        concurrent_tasks: usize,
        operations_per_task: usize
    ) -> Vec<std::time::Duration>
    where
        F: Fn(usize) -> Fut + Clone + Send + Sync + 'static,
        Fut: std::future::Future + Send,
    {
        let mut handles = Vec::new();
        let durations = Arc::new(RwLock::new(Vec::new()));

        for task_id in 0..concurrent_tasks {
            let operation = operation.clone();
            let durations = durations.clone();

            let handle = tokio::spawn(async move {
                let mut task_durations = Vec::new();

                for i in 0..operations_per_task {
                    let (result, duration) = Self::measure_time(|| operation(task_id * 1000 + i)).await;
                    task_durations.push(duration);
                    // Prevent overwhelming the system
                    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                }

                let mut durations_guard = durations.write().await;
                durations_guard.extend(task_durations);
            });

            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            let _ = handle.await;
        }

        let durations_guard = durations.read().await;
        durations_guard.clone()
    }

    /// Calculate performance statistics
    pub fn calculate_stats(durations: &[std::time::Duration]) -> PerformanceStats {
        if durations.is_empty() {
            return PerformanceStats::default();
        }

        let total: std::time::Duration = durations.iter().sum();
        let average = total / durations.len() as u32;

        let min = durations.iter().min().unwrap();
        let max = durations.iter().max().unwrap();

        // Calculate median
        let mut sorted_durations = durations.to_vec();
        sorted_durations.sort();
        let median = if sorted_durations.len() % 2 == 0 {
            let mid = sorted_durations.len() / 2;
            (sorted_durations[mid - 1] + sorted_durations[mid]) / 2
        } else {
            sorted_durations[sorted_durations.len() / 2]
        };

        // Calculate 95th percentile
        let p95_index = (sorted_durations.len() as f64 * 0.95) as usize;
        let p95 = sorted_durations.get(p95_index).unwrap_or(max);

        PerformanceStats {
            total_operations: durations.len(),
            total_duration: total,
            average_duration: average,
            min_duration: *min,
            max_duration: *max,
            median_duration: median,
            p95_duration: *p95,
            operations_per_second: durations.len() as f64 / total.as_secs_f64(),
        }
    }
}

/// Performance statistics
#[derive(Debug, Clone)]
pub struct PerformanceStats {
    pub total_operations: usize,
    pub total_duration: std::time::Duration,
    pub average_duration: std::time::Duration,
    pub min_duration: std::time::Duration,
    pub max_duration: std::time::Duration,
    pub median_duration: std::time::Duration,
    pub p95_duration: std::time::Duration,
    pub operations_per_second: f64,
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self {
            total_operations: 0,
            total_duration: std::time::Duration::ZERO,
            average_duration: std::time::Duration::ZERO,
            min_duration: std::time::Duration::ZERO,
            max_duration: std::time::Duration::ZERO,
            median_duration: std::time::Duration::ZERO,
            p95_duration: std::time::Duration::ZERO,
            operations_per_second: 0.0,
        }
    }
}

/// Async test utilities
pub struct AsyncTestUtils;

impl AsyncTestUtils {
    /// Wait for a condition to become true with timeout
    pub async fn wait_for_condition<F, Fut>(
        condition: F,
        timeout_ms: u64,
        poll_interval_ms: u64
    ) -> Result<()>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let timeout = tokio::time::sleep(tokio::time::Duration::from_millis(timeout_ms));
        tokio::pin!(timeout);

        loop {
            if condition().await {
                return Ok(());
            }

            tokio::select! {
                _ = &mut timeout => {
                    return Err(anyhow::anyhow!("Condition not met within timeout"));
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(poll_interval_ms)) => {
                    continue;
                }
            }
        }
    }

    /// Wait for async operations to complete
    pub async fn wait_for_async(duration_ms: u64) {
        tokio::time::sleep(tokio::time::Duration::from_millis(duration_ms)).await;
    }

    /// Run async operation with timeout
    pub async fn with_timeout<F, T>(
        future: F,
        timeout_ms: u64
    ) -> Result<T>
    where
        F: std::future::Future<Output = T>,
    {
        match tokio::time::timeout(
            tokio::time::Duration::from_millis(timeout_ms),
            future
        ).await {
            Ok(result) => Ok(result),
            Err(_) => Err(anyhow::anyhow!("Operation timed out")),
        }
    }
}