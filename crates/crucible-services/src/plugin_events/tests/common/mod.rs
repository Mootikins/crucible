//! Common test utilities, mocks, and helpers for plugin event subscription tests

pub mod mocks;
pub mod fixtures;
pub mod helpers;

// Re-export commonly used items
pub use mocks::*;
pub use fixtures::*;
pub use helpers::*;

use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::Utc;
use uuid::Uuid;

/// Test result type with detailed error information
pub type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Test context for sharing data between test functions
#[derive(Debug, Clone)]
pub struct TestContext {
    /// Unique test identifier
    pub test_id: String,

    /// Test creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Test configuration
    pub config: TestConfig,

    /// Shared test state
    pub state: Arc<Mutex<TestState>>,
}

impl TestContext {
    /// Create a new test context
    pub fn new(test_name: &str) -> Self {
        Self {
            test_id: format!("{}-{}", test_name, Uuid::new_v4()),
            created_at: Utc::now(),
            config: TestConfig::default(),
            state: Arc::new(Mutex::new(TestState::new())),
        }
    }

    /// Create test context with custom configuration
    pub fn with_config(test_name: &str, config: TestConfig) -> Self {
        Self {
            test_id: format!("{}-{}", test_name, Uuid::new_v4()),
            created_at: Utc::now(),
            config,
            state: Arc::new(Mutex::new(TestState::new())),
        }
    }
}

/// Test configuration
#[derive(Debug, Clone)]
pub struct TestConfig {
    /// Enable detailed logging
    pub verbose: bool,

    /// Test timeout in milliseconds
    pub timeout_ms: u64,

    /// Enable performance tracking
    pub track_performance: bool,

    /// Number of test events to generate
    pub event_count: usize,

    /// Enable stress testing
    pub stress_test: bool,

    /// Maximum concurrent operations
    pub max_concurrent: usize,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            verbose: false,
            timeout_ms: 5000,
            track_performance: true,
            event_count: 100,
            stress_test: false,
            max_concurrent: 10,
        }
    }
}

/// Shared test state for tracking test progress
#[derive(Debug, Default)]
pub struct TestState {
    /// Events generated during test
    pub events_generated: usize,

    /// Events processed during test
    pub events_processed: usize,

    /// Test errors encountered
    pub errors: Vec<String>,

    /// Performance metrics
    pub performance_metrics: std::collections::HashMap<String, f64>,

    /// Custom test data
    pub custom_data: std::collections::HashMap<String, String>,
}

impl TestState {
    /// Create new test state
    pub fn new() -> Self {
        Self::default()
    }

    /// Add error to test state
    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }

    /// Increment event counter
    pub fn increment_events_generated(&mut self) {
        self.events_generated += 1;
    }

    /// Increment processed counter
    pub fn increment_events_processed(&mut self) {
        self.events_processed += 1;
    }

    /// Set performance metric
    pub fn set_metric(&mut self, name: String, value: f64) {
        self.performance_metrics.insert(name, value);
    }

    /// Get error count
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Check if test has errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Performance measurement helper
pub struct PerformanceTimer {
    start_time: std::time::Instant,
    name: String,
}

impl PerformanceTimer {
    /// Create new performance timer
    pub fn new(name: String) -> Self {
        Self {
            start_time: std::time::Instant::now(),
            name,
        }
    }

    /// Stop timer and return elapsed time
    pub fn stop(self) -> std::time::Duration {
        let elapsed = self.start_time.elapsed();
        tracing::debug!("Timer '{}' took {:?}", self.name, elapsed);
        elapsed
    }

    /// Stop timer and return elapsed milliseconds
    pub fn stop_ms(self) -> u64 {
        self.stop().as_millis() as u64
    }
}

/// Test assertion helpers
pub mod assertions {
    use super::*;
    use crate::plugin_events::types::*;

    /// Assert that two timestamps are close within tolerance
    pub fn assert_timestamp_close(
        actual: chrono::DateTime<chrono::Utc>,
        expected: chrono::DateTime<chrono::Utc>,
        tolerance_ms: i64,
    ) {
        let diff = (actual - expected).num_milliseconds().abs();
        assert!(
            diff <= tolerance_ms,
            "Timestamp difference {}ms exceeds tolerance {}ms",
            diff,
            tolerance_ms
        );
    }

    /// Assert subscription status
    pub fn assert_subscription_status(
        subscription: &SubscriptionConfig,
        expected_status: SubscriptionStatus,
    ) {
        assert_eq!(
            subscription.status,
            expected_status,
            "Expected subscription status {:?}, got {:?}",
            expected_status,
            subscription.status
        );
    }

    /// Assert subscription has required permissions
    pub fn assert_subscription_has_permission(
        subscription: &SubscriptionConfig,
        required_permission: &EventPermission,
    ) {
        assert!(
            subscription.auth_context.permissions.contains(required_permission),
            "Subscription does not have required permission: {:?}",
            required_permission
        );
    }

    /// Assert performance metric meets threshold
    pub fn assert_performance_metric(
        state: &TestState,
        metric_name: &str,
        max_value: f64,
    ) {
        if let Some(value) = state.performance_metrics.get(metric_name) {
            assert!(
                *value <= max_value,
                "Performance metric '{}' = {} exceeds maximum {}",
                metric_name,
                value,
                max_value
            );
        } else {
            panic!("Performance metric '{}' not found", metric_name);
        }
    }

    /// Assert error-free test execution
    pub fn assert_no_errors(state: &TestState) {
        if state.has_errors() {
            let error_list = state.errors.join("\n");
            panic!("Test encountered {} errors:\n{}", state.error_count(), error_list);
        }
    }
}

/// Test data generators
pub mod generators {
    use super::*;
    use crate::plugin_events::types::*;
    use crate::events::{DaemonEvent, EventPriority, EventType, EventSource};
    use std::collections::HashMap;

    /// Generate test subscription
    pub fn test_subscription(
        plugin_id: &str,
        name: &str,
        subscription_type: SubscriptionType,
    ) -> SubscriptionConfig {
        let auth_context = AuthContext::new(
            plugin_id.to_string(),
            vec![EventPermission {
                scope: PermissionScope::Plugin,
                event_types: vec![],
                categories: vec![],
                sources: vec![],
                max_priority: Some(EventPriority::Normal),
            }],
        );

        SubscriptionConfig::new(
            plugin_id.to_string(),
            name.to_string(),
            subscription_type,
            auth_context,
        )
    }

    /// Generate test daemon event
    pub fn test_daemon_event(event_type: EventType, priority: EventPriority) -> DaemonEvent {
        DaemonEvent {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type,
            source: EventSource {
                id: "test-source".to_string(),
                name: "Test Source".to_string(),
                version: "1.0.0".to_string(),
                metadata: HashMap::new(),
            },
            priority,
            correlation_id: Some(Uuid::new_v4()),
            causation_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Generate multiple test events
    pub fn generate_test_events(count: usize) -> Vec<DaemonEvent> {
        (0..count)
            .map(|i| {
                let event_type = match i % 6 {
                    0 => EventType::System(crate::events::SystemEvent::Startup),
                    1 => EventType::Service(crate::events::ServiceEvent::Started),
                    2 => EventType::Filesystem(crate::events::FilesystemEvent::Created),
                    3 => EventType::Database(crate::events::DatabaseEvent::Query),
                    4 => EventType::External(crate::events::ExternalEvent::Webhook),
                    _ => EventType::Custom(format!("test-event-{}", i)),
                };

                let priority = match i % 4 {
                    0 => EventPriority::Low,
                    1 => EventPriority::Normal,
                    2 => EventPriority::High,
                    _ => EventPriority::Critical,
                };

                test_daemon_event(event_type, priority)
            })
            .collect()
    }

    /// Generate test filter
    pub fn test_filter(filter_expression: &str) -> crate::events::EventFilter {
        crate::events::EventFilter::Pattern(filter_expression.to_string())
    }

    /// Generate test auth context with specific permissions
    pub fn test_auth_context(
        principal: &str,
        permissions: Vec<EventPermission>,
    ) -> AuthContext {
        AuthContext::new(principal.to_string(), permissions)
    }
}

/// Async test utilities
pub mod async_utils {
    use super::*;
    use std::time::Duration;

    /// Wait for condition to be true with timeout
    pub async fn wait_for_condition<F, Fut>(
        condition: F,
        timeout_ms: u64,
        check_interval_ms: u64,
    ) -> TestResult<()>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let timeout = Duration::from_millis(timeout_ms);
        let interval = Duration::from_millis(check_interval_ms);

        let start = std::time::Instant::now();

        loop {
            if condition().await {
                return Ok(());
            }

            if start.elapsed() > timeout {
                return Err("Condition not met within timeout".into());
            }

            tokio::time::sleep(interval).await;
        }
    }

    /// Run multiple futures concurrently and collect results
    pub async fn run_concurrent<F, T, E>(
        futures: Vec<F>,
        max_concurrent: usize,
    ) -> Vec<Result<T, E>>
    where
        F: std::future::Future<Output = Result<T, E>> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
    {
        use futures::stream::{self, StreamExt};

        stream::iter(futures)
            .buffer_unordered(max_concurrent)
            .collect()
            .await
    }

    /// Measure execution time of an async operation
    pub async fn measure_async<F, T>(future: F) -> (T, Duration)
    where
        F: std::future::Future<Output = T>,
    {
        let start = std::time::Instant::now();
        let result = future.await;
        let elapsed = start.elapsed();
        (result, elapsed)
    }
}