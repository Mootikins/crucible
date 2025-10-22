//! Plugin event subscription system tests
//!
//! Comprehensive test suite for the plugin event subscription system covering
//! all components including system lifecycle, subscription management, filtering,
//! delivery, security, performance, and integration scenarios.

pub mod common;
pub mod system_tests;
pub mod registry_tests;
pub mod filter_tests;
pub mod delivery_tests;
pub mod bridge_tests;
pub mod api_tests;
pub mod config_tests;
pub mod security_tests;
pub mod performance_tests;
pub mod integration_tests;

// Re-export common test utilities for easier access
pub use common::*;

/// Test configuration and constants
pub mod test_config {
    /// Default test timeout in milliseconds
    pub const DEFAULT_TIMEOUT_MS: u64 = 5000;

    /// Performance test timeout in milliseconds
    pub const PERF_TIMEOUT_MS: u64 = 30000;

    /// Large dataset size for performance tests
    pub const LARGE_DATASET_SIZE: usize = 10000;

    /// Concurrent operation count for stress tests
    pub const CONCURRENT_OPS: usize = 1000;

    /// Number of test events for performance tests
    pub const PERF_EVENT_COUNT: usize = 100000;

    /// Maximum test retries for flaky tests
    pub const MAX_TEST_RETRIES: u32 = 3;

    /// Test backoff delay in milliseconds
    pub const TEST_BACKOFF_MS: u64 = 100;
}

/// Custom test macros for common patterns
#[macro_export]
macro_rules! assert_timeout {
    ($future:expr, $timeout_ms:expr, $msg:expr) => {
        match tokio::time::timeout(
            std::time::Duration::from_millis($timeout_ms),
            $future
        ).await {
            Ok(result) => result,
            Err(_) => panic!("Test timed out after {}ms: {}", $timeout_ms, $msg),
        }
    };
}

#[macro_export]
macro_rules! assert_performance {
    ($start:expr, $max_ms:expr, $operation:expr) => {
        let duration = $start.elapsed();
        assert!(
            duration.as_millis() <= $max_ms,
            "Operation '{}' took {:?}ms, expected <= {}ms",
            $operation,
            duration.as_millis(),
            $max_ms
        );
    };
}

#[macro_export]
macro_rules! retry_test {
    ($test_expr:expr, $max_retries:expr, $backoff_ms:expr) => {
        {
            let mut last_error = None;
            for attempt in 0..=$max_retries {
                match (async { $test_expr }).await {
                    Ok(result) => {
                        if attempt > 0 {
                            println!("Test succeeded on attempt {}", attempt + 1);
                        }
                        break Ok(result);
                    }
                    Err(e) => {
                        last_error = Some(e);
                        if attempt < $max_retries {
                            tokio::time::sleep(std::time::Duration::from_millis($backoff_ms)).await;
                        }
                    }
                }
            }
            Err(last_error.unwrap())
        }
    };
}