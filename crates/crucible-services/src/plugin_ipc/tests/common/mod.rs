//! # Common Test Utilities
//!
//! Shared utilities, mocks, and helpers for testing IPC protocol components.

pub mod mocks;
pub mod fixtures;
pub mod helpers;

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

// Re-export common types for convenience
pub use fixtures::*;
pub use mocks::*;
pub use helpers::*;

/// Test timeout values
pub const TEST_TIMEOUT_MS: u64 = 5000;
pub const SHORT_TIMEOUT_MS: u64 = 100;
pub const LONG_TIMEOUT_MS: u64 = 30000;

/// Common test data sizes
pub const SMALL_MESSAGE_SIZE: usize = 1024;
pub const MEDIUM_MESSAGE_SIZE: usize = 64 * 1024;
pub const LARGE_MESSAGE_SIZE: usize = 1024 * 1024;

/// Test configuration
#[derive(Debug, Clone)]
pub struct TestConfig {
    pub timeout_duration: Duration,
    pub max_retries: u32,
    pub enable_logging: bool,
    pub mock_failures: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            timeout_duration: Duration::from_millis(TEST_TIMEOUT_MS),
            max_retries: 3,
            enable_logging: true,
            mock_failures: false,
        }
    }
}

/// Test context for sharing state between tests
#[derive(Debug)]
pub struct TestContext {
    pub config: TestConfig,
    pub temp_dir: Option<String>,
    pub test_id: String,
    pub start_time: std::time::Instant,
}

impl TestContext {
    pub fn new(config: TestConfig) -> Self {
        Self {
            config,
            temp_dir: None,
            test_id: Uuid::new_v4().to_string(),
            start_time: std::time::Instant::now(),
        }
    }

    pub fn with_id(test_id: String) -> Self {
        Self {
            config: TestConfig::default(),
            temp_dir: None,
            test_id,
            start_time: std::time::Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn create_temp_dir(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        let temp_path = format!("/tmp/crucible_test_{}", self.test_id);
        std::fs::create_dir_all(&temp_path)?;
        self.temp_dir = Some(temp_path.clone());
        Ok(temp_path)
    }

    pub fn cleanup(&mut self) {
        if let Some(temp_dir) = &self.temp_dir {
            let _ = std::fs::remove_dir_all(temp_dir);
        }
    }
}

/// Async test wrapper with timeout
pub async fn with_timeout<F, T>(duration: Duration, future: F) -> Result<T, &'static str>
where
    F: std::future::Future<Output = T>,
{
    match tokio::time::timeout(duration, future).await {
        Ok(result) => Ok(result),
        Err(_) => Err("Test timed out"),
    }
}

/// Macro for async tests with default timeout
#[macro_export]
macro_rules! async_test {
    ($test_name:ident, $test_body:block) => {
        #[tokio::test]
        async fn $test_name() {
            let timeout = Duration::from_millis($crate::tests::common::TEST_TIMEOUT_MS);
            match $crate::tests::common::with_timeout(timeout, async move $test_body).await {
                Ok(_) => {},
                Err(e) => panic!("Test failed: {}", e),
            }
        }
    };
}

/// Performance measurement utilities
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub duration: Duration,
    pub operations_per_second: f64,
    pub throughput_bytes_per_second: f64,
    pub memory_usage_mb: f64,
}

impl PerformanceMetrics {
    pub fn measure<F, Fut>(operation: F) -> impl std::future::Future<Output = (F::Output, Self)>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future,
    {
        async move {
            let start = std::time::Instant::now();
            let result = operation().await;
            let duration = start.elapsed();

            let metrics = Self {
                duration,
                operations_per_second: 1.0 / duration.as_secs_f64(),
                throughput_bytes_per_second: 0.0, // To be calculated based on context
                memory_usage_mb: 0.0, // To be measured if needed
            };

            (result, metrics)
        }
    }

    pub fn measure_throughput(&mut self, bytes_transferred: usize) {
        self.throughput_bytes_per_second = bytes_transferred as f64 / self.duration.as_secs_f64();
    }
}

/// Property-based testing utilities
pub struct PropertyTestConfig {
    pub num_cases: u32,
    pub max_size: usize,
    pub seed: Option<u64>,
}

impl Default for PropertyTestConfig {
    fn default() -> Self {
        Self {
            num_cases: 100,
            max_size: 1024 * 1024,
            seed: None,
        }
    }
}

/// Concurrent test utilities
pub async fn run_concurrent<F, Fut>(
    num_tasks: usize,
    task_factory: impl Fn(usize) -> F,
) -> Vec<Fut::Output>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    let mut handles = Vec::new();

    for i in 0..num_tasks {
        let handle = tokio::spawn(task_factory(i)());
        handles.push(handle);
    }

    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => panic!("Task panicked: {}", e),
        }
    }

    results
}

/// Error assertion utilities
pub trait IpcErrorExt {
    fn is_protocol_error(&self) -> bool;
    fn is_auth_error(&self) -> bool;
    fn is_connection_error(&self) -> bool;
    fn is_message_error(&self) -> bool;
    fn is_plugin_error(&self) -> bool;
    fn has_error_code(&self, code: &str) -> bool;
}

impl IpcErrorExt for crate::plugin_ipc::error::IpcError {
    fn is_protocol_error(&self) -> bool {
        matches!(self, crate::plugin_ipc::error::IpcError::Protocol { .. })
    }

    fn is_auth_error(&self) -> bool {
        matches!(self, crate::plugin_ipc::error::IpcError::Authentication { .. })
    }

    fn is_connection_error(&self) -> bool {
        matches!(self, crate::plugin_ipc::error::IpcError::Connection { .. })
    }

    fn is_message_error(&self) -> bool {
        matches!(self, crate::plugin_ipc::error::IpcError::Message { .. })
    }

    fn is_plugin_error(&self) -> bool {
        matches!(self, crate::plugin_ipc::error::IpcError::Plugin { .. })
    }

    fn has_error_code(&self, code: &str) -> bool {
        match self {
            crate::plugin_ipc::error::IpcError::Protocol { code: protocol_code, .. } => {
                format!("{:?}", protocol_code).contains(code)
            }
            crate::plugin_ipc::error::IpcError::Authentication { code: auth_code, .. } => {
                format!("{:?}", auth_code).contains(code)
            }
            crate::plugin_ipc::error::IpcError::Connection { code: conn_code, .. } => {
                format!("{:?}", conn_code).contains(code)
            }
            crate::plugin_ipc::error::IpcError::Message { code: msg_code, .. } => {
                format!("{:?}", msg_code).contains(code)
            }
            crate::plugin_ipc::error::IpcError::Plugin { code: plugin_code, .. } => {
                format!("{:?}", plugin_code).contains(code)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_context_creation() {
        let config = TestConfig::default();
        let context = TestContext::new(config);
        assert!(!context.test_id.is_empty());
        assert_eq!(context.elapsed().as_secs(), 0);
    }

    #[test]
    fn test_performance_metrics() {
        let runtime = tokio::runtime::Runtime::new().unwrap();

        let (result, metrics) = runtime.block_on(async {
            PerformanceMetrics::measure(|| async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                42
            }).await
        });

        assert_eq!(result, 42);
        assert!(metrics.duration.as_millis() >= 10);
        assert!(metrics.operations_per_second > 0.0);
    }

    async_test!(test_async_timeout_wrapper, {
        tokio::time::sleep(Duration::from_millis(10)).await;
        "success"
    });

    async_test!(test_concurrent_execution, {
        let results = run_concurrent(5, |i| || async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            i * 2
        }).await;

        assert_eq!(results.len(), 5);
        assert_eq!(results[0], 0);
        assert_eq!(results[4], 8);
        results.len()
    });
}