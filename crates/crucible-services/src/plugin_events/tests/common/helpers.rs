//! Helper functions and utilities for plugin event subscription tests

use super::*;
use crate::plugin_events::types::*;
use crate::plugin_events::error::*;
use crate::plugin_events::{PluginEventSystem, PluginEventSystemBuilder};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Mutex};

/// Test environment setup and teardown utilities
pub struct TestEnvironment {
    /// Test event system
    pub event_system: Option<PluginEventSystem>,

    /// Mock event bus
    pub mock_event_bus: Option<Arc<MockEventBus>>,

    /// Mock plugin connection manager
    pub mock_plugin_manager: Option<Arc<MockPluginConnectionManager>>,

    /// Test cleanup tasks
    cleanup_tasks: Vec<Box<dyn FnOnce() + Send>>,
}

impl TestEnvironment {
    /// Create new test environment
    pub fn new() -> Self {
        Self {
            event_system: None,
            mock_event_bus: None,
            mock_plugin_manager: None,
            cleanup_tasks: Vec::new(),
        }
    }

    /// Setup test environment with default components
    pub async fn setup(&mut self) -> TestResult<()> {
        // Create mock event bus
        let mock_event_bus = Arc::new(MockEventBus::new());
        self.mock_event_bus = Some(mock_event_bus.clone());

        // Create mock plugin manager
        let mock_plugin_manager = Arc::new(MockPluginConnectionManager::new());
        self.mock_plugin_manager = Some(mock_plugin_manager.clone());

        // Create event system with test configuration
        let mut event_system = PluginEventSystemBuilder::new()
            .with_api_enabled(false) // Disable API for testing
            .with_config(TestFixtures::development_config())
            .build()?;

        // Initialize the event system
        event_system
            .initialize(mock_event_bus.clone() as Arc<dyn crate::events::EventBus + Send + Sync>)
            .await?;

        self.event_system = Some(event_system);

        Ok(())
    }

    /// Setup test environment with failures (for error testing)
    pub async fn setup_with_failures(&mut self) -> TestResult<()> {
        let mock_event_bus = Arc::new(MockEventBus::with_failures());
        self.mock_event_bus = Some(mock_event_bus.clone());

        let mock_plugin_manager = Arc::new(MockPluginConnectionManager::new());
        self.mock_plugin_manager = Some(mock_plugin_manager.clone());

        let mut event_system = PluginEventSystemBuilder::new()
            .with_api_enabled(false)
            .build()?;

        event_system
            .initialize(mock_event_bus.clone() as Arc<dyn crate::events::EventBus + Send + Sync>)
            .await?;

        self.event_system = Some(event_system);

        Ok(())
    }

    /// Cleanup test environment
    pub async fn cleanup(mut self) -> TestResult<()> {
        // Stop event system
        if let Some(event_system) = &self.event_system {
            event_system.stop().await?;
        }

        // Run cleanup tasks
        for task in self.cleanup_tasks {
            task();
        }

        Ok(())
    }

    /// Add cleanup task
    pub fn add_cleanup_task<F>(&mut self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.cleanup_tasks.push(Box::new(task));
    }

    /// Get event system reference
    pub fn event_system(&self) -> &PluginEventSystem {
        self.event_system.as_ref().unwrap()
    }

    /// Get mock event bus reference
    pub fn mock_event_bus(&self) -> &Arc<MockEventBus> {
        self.mock_event_bus.as_ref().unwrap()
    }

    /// Get mock plugin manager reference
    pub fn mock_plugin_manager(&self) -> &Arc<MockPluginConnectionManager> {
        self.mock_plugin_manager.as_ref().unwrap()
    }
}

impl Default for TestEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

/// Performance measurement utilities
pub struct PerformanceTracker {
    measurements: Arc<RwLock<Vec<PerformanceMeasurement>>>,
    current_operation: Arc<Mutex<Option<String>>>,
}

#[derive(Debug, Clone)]
pub struct PerformanceMeasurement {
    pub operation: String,
    pub duration: Duration,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl PerformanceTracker {
    /// Create new performance tracker
    pub fn new() -> Self {
        Self {
            measurements: Arc::new(RwLock::new(Vec::new())),
            current_operation: Arc::new(Mutex::new(None)),
        }
    }

    /// Start measuring an operation
    pub async fn start_operation(&self, operation: String) -> PerformanceTimer {
        *self.current_operation.lock().await = Some(operation.clone());
        PerformanceTimer::new(operation, self.measurements.clone())
    }

    /// Record a measurement
    pub async fn record_measurement(&self, measurement: PerformanceMeasurement) {
        self.measurements.write().await.push(measurement);
    }

    /// Get all measurements
    pub async fn get_measurements(&self) -> Vec<PerformanceMeasurement> {
        self.measurements.read().await.clone()
    }

    /// Get measurements for specific operation
    pub async fn get_measurements_for_operation(&self, operation: &str) -> Vec<PerformanceMeasurement> {
        self.measurements
            .read()
            .await
            .iter()
            .filter(|m| m.operation == operation)
            .cloned()
            .collect()
    }

    /// Calculate statistics for an operation
    pub async fn calculate_stats(&self, operation: &str) -> Option<PerformanceStats> {
        let measurements = self.get_measurements_for_operation(operation).await;
        if measurements.is_empty() {
            return None;
        }

        let durations: Vec<Duration> = measurements.iter().map(|m| m.duration).collect();
        let total_duration: Duration = durations.iter().sum();
        let avg_duration = total_duration / durations.len() as u32;

        let mut sorted_durations = durations.clone();
        sorted_durations.sort();

        let p50 = sorted_durations[sorted_durations.len() / 2];
        let p95 = sorted_durations[(sorted_durations.len() as f64 * 0.95) as usize];
        let p99 = sorted_durations[(sorted_durations.len() as f64 * 0.99) as usize];

        Some(PerformanceStats {
            operation: operation.to_string(),
            count: measurements.len(),
            total_duration,
            avg_duration,
            min_duration: *sorted_durations.first().unwrap(),
            max_duration: *sorted_durations.last().unwrap(),
            p50_duration: p50,
            p95_duration: p95,
            p99_duration: p99,
            operations_per_second: 1_000_000_000.0 / avg_duration.as_nanos() as f64,
        })
    }

    /// Clear all measurements
    pub async fn clear(&self) {
        self.measurements.write().await.clear();
    }
}

#[derive(Debug, Clone)]
pub struct PerformanceStats {
    pub operation: String,
    pub count: usize,
    pub total_duration: Duration,
    pub avg_duration: Duration,
    pub min_duration: Duration,
    pub max_duration: Duration,
    pub p50_duration: Duration,
    pub p95_duration: Duration,
    pub p99_duration: Duration,
    pub operations_per_second: f64,
}

/// Performance timer implementation
pub struct PerformanceTimer {
    operation: String,
    start_time: Instant,
    measurements: Arc<RwLock<Vec<PerformanceMeasurement>>>,
}

impl PerformanceTimer {
    /// Create new performance timer
    pub fn new(
        operation: String,
        measurements: Arc<RwLock<Vec<PerformanceMeasurement>>>,
    ) -> Self {
        Self {
            operation,
            start_time: Instant::now(),
            measurements,
        }
    }

    /// Stop the timer and record the measurement
    pub async fn stop(self) -> Duration {
        let duration = self.start_time.elapsed();
        let measurement = PerformanceMeasurement {
            operation: self.operation.clone(),
            duration,
            timestamp: chrono::Utc::now(),
            metadata: std::collections::HashMap::new(),
        };

        self.measurements.write().await.push(measurement);
        duration
    }
}

impl Drop for PerformanceTimer {
    fn drop(&mut self) {
        // Record measurement if timer wasn't explicitly stopped
        let duration = self.start_time.elapsed();
        let measurement = PerformanceMeasurement {
            operation: self.operation.clone(),
            duration,
            timestamp: chrono::Utc::now(),
            metadata: std::collections::HashMap::new(),
        };

        // Use a blocking write to avoid async issues in Drop
        if let Ok(mut measurements) = self.measurements.try_write() {
            measurements.push(measurement);
        }
    }
}

/// Event generation utilities
pub struct EventGenerator {
    base_counter: Arc<Mutex<u64>>,
}

impl EventGenerator {
    /// Create new event generator
    pub fn new() -> Self {
        Self {
            base_counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Generate a sequence of events
    pub async fn generate_sequence(&self, count: usize) -> Vec<DaemonEvent> {
        let mut events = Vec::with_capacity(count);
        let mut counter = self.base_counter.lock().await;

        for i in 0..count {
            let event = self.create_event(*counter + i as u64).await;
            events.push(event);
        }

        *counter += count as u64;
        events
    }

    /// Generate events with specific pattern
    pub async fn generate_with_pattern<F>(&self, count: usize, pattern_fn: F) -> Vec<DaemonEvent>
    where
        F: Fn(u64) -> crate::events::EventType,
    {
        let mut events = Vec::with_capacity(count);
        let mut counter = self.base_counter.lock().await;

        for i in 0..count {
            let event_type = pattern_fn(*counter + i as u64);
            let event = self.create_event_with_type(*counter + i as u64, event_type).await;
            events.push(event);
        }

        *counter += count as u64;
        events
    }

    /// Create single event
    async fn create_event(&self, id: u64) -> DaemonEvent {
        let event_type = match id % 6 {
            0 => crate::events::EventType::System(crate::events::SystemEvent::Startup),
            1 => crate::events::EventType::Service(crate::events::ServiceEvent::Started {
                service_id: format!("service-{}", id),
                service_type: "TestService".to_string(),
            }),
            2 => crate::events::EventType::Filesystem(crate::events::FilesystemEvent::Created {
                path: format!("/tmp/test-{}.txt", id),
                size: 1024,
                file_type: "regular".to_string(),
            }),
            3 => crate::events::EventType::Database(crate::events::DatabaseEvent::Query {
                query: format!("SELECT * FROM test WHERE id = {}", id),
                duration_ms: 100,
                rows_affected: 1,
            }),
            4 => crate::events::EventType::External(crate::events::ExternalEvent::Webhook {
                url: "https://example.com/webhook".to_string(),
                method: "POST".to_string(),
                status_code: Some(200),
            }),
            _ => crate::events::EventType::Custom(format!("test-event-{}", id)),
        };

        self.create_event_with_type(id, event_type).await
    }

    /// Create event with specific type
    async fn create_event_with_type(&self, id: u64, event_type: crate::events::EventType) -> DaemonEvent {
        DaemonEvent {
            id: uuid::Uuid::from_u64_pair(0, id),
            timestamp: chrono::Utc::now() + chrono::Duration::milliseconds(id as i64),
            event_type,
            source: crate::events::EventSource {
                id: format!("test-source-{}", id % 10),
                name: format!("Test Source {}", id % 10),
                version: "1.0.0".to_string(),
                metadata: std::collections::HashMap::new(),
            },
            priority: match id % 4 {
                0 => crate::events::EventPriority::Low,
                1 => crate::events::EventPriority::Normal,
                2 => crate::events::EventPriority::High,
                _ => crate::events::EventPriority::Critical,
            },
            correlation_id: Some(uuid::Uuid::new_v4()),
            causation_id: None,
            metadata: {
                let mut map = std::collections::HashMap::new();
                map.insert("generator_id".to_string(), id.to_string());
                map
            },
        }
    }
}

/// Subscription testing utilities
pub struct SubscriptionTestHelper {
    subscription_counter: Arc<Mutex<u64>>,
}

impl SubscriptionTestHelper {
    /// Create new subscription test helper
    pub fn new() -> Self {
        Self {
            subscription_counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Create test subscription with generated ID
    pub async fn create_test_subscription(
        &self,
        plugin_id: &str,
        subscription_type: SubscriptionType,
    ) -> SubscriptionConfig {
        let mut counter = self.subscription_counter.lock().await;
        *counter += 1;

        let auth_context = AuthContext::new(
            plugin_id.to_string(),
            vec![EventPermission {
                scope: PermissionScope::Plugin,
                event_types: vec![],
                categories: vec![],
                sources: vec![],
                max_priority: None,
            }],
        );

        SubscriptionConfig::new(
            plugin_id.to_string(),
            format!("Test Subscription {}", *counter),
            subscription_type,
            auth_context,
        )
    }

    /// Create subscription with custom permissions
    pub async fn create_subscription_with_permissions(
        &self,
        plugin_id: &str,
        subscription_type: SubscriptionType,
        permissions: Vec<EventPermission>,
    ) -> SubscriptionConfig {
        let mut counter = self.subscription_counter.lock().await;
        *counter += 1;

        let auth_context = AuthContext::new(plugin_id.to_string(), permissions);

        SubscriptionConfig::new(
            plugin_id.to_string(),
            format!("Test Subscription {}", *counter),
            subscription_type,
            auth_context,
        )
    }

    /// Wait for subscription to be processed
    pub async fn wait_for_subscription_processing(
        &self,
        event_system: &PluginEventSystem,
        timeout_ms: u64,
    ) -> TestResult<()> {
        let start = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout {
            let stats = event_system.subscription_manager().get_manager_stats().await;
            if stats.active_subscriptions > 0 {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        Err("Subscription processing timed out".into())
    }
}

/// Assertion utilities for test validation
pub struct TestAssertions;

impl TestAssertions {
    /// Assert that event delivery meets performance requirements
    pub fn assert_delivery_performance(
        stats: &PerformanceStats,
        max_avg_duration_ms: u64,
        min_ops_per_sec: f64,
    ) {
        let avg_duration_ms = stats.avg_duration.as_millis() as u64;
        assert!(
            avg_duration_ms <= max_avg_duration_ms,
            "Average delivery duration {}ms exceeds maximum {}ms",
            avg_duration_ms,
            max_avg_duration_ms
        );

        assert!(
            stats.operations_per_second >= min_ops_per_sec,
            "Operations per second {} below minimum {}",
            stats.operations_per_second,
            min_ops_per_sec
        );
    }

    /// Assert that event ordering is preserved
    pub fn assert_event_ordering(events: &[DaemonEvent], expected_ordering: EventOrdering) {
        match expected_ordering {
            EventOrdering::Fifo => {
                for i in 1..events.len() {
                    assert!(
                        events[i].timestamp >= events[i - 1].timestamp,
                        "FIFO ordering violated: event {} ({}) is before event {} ({})",
                        i,
                        events[i].timestamp,
                        i - 1,
                        events[i - 1].timestamp
                    );
                }
            }
            EventOrdering::Priority => {
                let priorities: Vec<_> = events.iter().map(|e| &e.priority).collect();
                for i in 1..priorities.len() {
                    assert!(
                        priorities[i] <= priorities[i - 1],
                        "Priority ordering violated: event {} ({:?}) should come before event {} ({:?})",
                        i,
                        priorities[i],
                        i - 1,
                        priorities[i - 1]
                    );
                }
            }
            EventOrdering::None => {
                // No ordering guarantees to check
            }
            EventOrdering::Causal => {
                // Check causation chains (simplified check)
                for i in 1..events.len() {
                    if let Some(causation_id) = events[i].causation_id {
                        let found_cause = events[..i]
                            .iter()
                            .any(|e| e.id == causation_id);
                        assert!(
                            found_cause,
                            "Causal ordering violated: causation event not found before dependent event"
                        );
                    }
                }
            }
        }
    }

    /// Assert that subscription filtering works correctly
    pub fn assert_subscription_filtering(
        subscription: &SubscriptionConfig,
        events: &[DaemonEvent],
        expected_matches: usize,
    ) {
        let actual_matches = events
            .iter()
            .filter(|e| subscription.matches_event(e))
            .count();

        assert_eq!(
            actual_matches,
            expected_matches,
            "Expected {} events to match subscription, got {}",
            expected_matches,
            actual_matches
        );
    }

    /// Assert that error handling works correctly
    pub fn assert_error_handling<T>(result: Result<T, SubscriptionError>, expected_error_type: &str) {
        match result {
            Err(error) => {
                let error_string = format!("{}", error);
                assert!(
                    error_string.contains(expected_error_type),
                    "Expected error containing '{}', got: {}",
                    expected_error_type,
                    error_string
                );
            }
            Ok(_) => panic!("Expected error, but operation succeeded"),
        }
    }

    /// Assert that system maintains consistency under load
    pub fn assert_system_consistency(
        initial_state: &std::collections::HashMap<String, u64>,
        final_state: &std::collections::HashMap<String, u64>,
        allowed_increases: &[&str],
    ) {
        for (key, &initial_value) in initial_state {
            let final_value = final_state.get(key).unwrap_or(&initial_value);
            if allowed_increases.contains(&key.as_str()) {
                assert!(
                    final_value >= initial_value,
                    "Metric {} should not decrease: {} -> {}",
                    key,
                    initial_value,
                    final_value
                );
            } else {
                assert_eq!(
                    final_value, initial_value,
                    "Metric {} should remain unchanged: {} vs {}",
                    key, initial_value, final_value
                );
            }
        }
    }
}

/// Concurrent testing utilities
pub struct ConcurrencyTester {
    thread_count: usize,
    operations_per_thread: usize,
}

impl ConcurrencyTester {
    /// Create new concurrency tester
    pub fn new(thread_count: usize, operations_per_thread: usize) -> Self {
        Self {
            thread_count,
            operations_per_thread,
        }
    }

    /// Run concurrent operations and measure results
    pub async fn run_concurrent_operations<F, T, E>(
        &self,
        operation: F,
    ) -> Vec<Result<T, E>>
    where
        F: Fn(usize) -> Result<T, E> + Send + Sync + 'static,
        T: Send + 'static,
        E: Send + 'static,
    {
        let operation = Arc::new(operation);
        let mut handles = Vec::new();

        for thread_id in 0..self.thread_count {
            let operation = Arc::clone(&operation);
            let handle = tokio::task::spawn_blocking(move || {
                let mut results = Vec::with_capacity(self.operations_per_thread);
                for op_id in 0..self.operations_per_thread {
                    let result = operation(thread_id * self.operations_per_thread + op_id);
                    results.push(result);
                }
                results
            });
            handles.push(handle);
        }

        let mut all_results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(thread_results) => all_results.extend(thread_results),
                Err(e) => {
                    // Handle thread panic
                    tracing::error!("Thread panicked: {:?}", e);
                }
            }
        }

        all_results
    }

    /// Run concurrent async operations
    pub async fn run_concurrent_async<F, Fut, T, E>(
        &self,
        operation: F,
    ) -> Vec<Result<T, E>>
    where
        F: Fn(usize) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<T, E>> + Send,
        T: Send + 'static,
        E: Send + 'static,
    {
        let mut handles = Vec::new();

        for thread_id in 0..self.thread_count {
            let handle = tokio::spawn(async move {
                let mut results = Vec::with_capacity(self.operations_per_thread);
                for op_id in 0..self.operations_per_thread {
                    let result = operation(thread_id * self.operations_per_thread + op_id).await;
                    results.push(result);
                }
                results
            });
            handles.push(handle);
        }

        let mut all_results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(thread_results) => all_results.extend(thread_results),
                Err(e) => {
                    tracing::error!("Async task panicked: {:?}", e);
                }
            }
        }

        all_results
    }
}