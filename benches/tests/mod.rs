//! Main test module for the benchmarking framework
//!
//! This module organizes and exports all test modules for the comprehensive
//! benchmarking framework, ensuring proper test structure and execution.

// Import all test modules
pub mod benchmark_utils_tests;
pub mod performance_reporter_tests;
pub mod benchmark_runner_tests;
pub mod individual_benchmark_tests;
pub mod benchmark_integration_tests;
pub mod edge_case_error_tests;
pub mod framework_performance_tests;

// Load testing framework test modules
pub mod script_engine_load_tests;
pub mod load_testing_integration_tests;
pub mod load_test_runner;
pub mod mock_script_engine_comprehensive_tests;
pub mod script_engine_load_tester_tests;
pub mod metrics_collector_comprehensive_tests;
pub mod load_test_config_validation_tests;
pub mod tool_distribution_algorithm_tests;
pub mod framework_performance_validation_tests;
pub mod load_testing_edge_case_tests;

// Re-export commonly used test utilities
pub use crate::benchmark_utils::*;
pub use crate::performance_reporter::*;
pub use crate::benchmark_runner::*;

/// Test configuration and utilities
pub mod test_config {
    use std::path::{Path, PathBuf};
    use std::fs;
    use std::time::Duration;
    use tempfile::TempDir;
    use anyhow::Result;

    /// Default test configuration
    pub fn default_test_config() -> crate::benchmark_runner::BenchmarkRunnerConfig {
        crate::benchmark_runner::BenchmarkRunnerConfig {
            output_dir: "test_benchmark_results".to_string(),
            run_comparisons: true,
            generate_plots: false, // Disable plots for tests
            export_formats: vec!["markdown".to_string(), "json".to_string()],
            iterations: Some(10), // Small values for fast tests
            sample_size: Some(5),
        }
    }

    /// Create temporary test directory
    pub fn create_test_dir() -> Result<TempDir> {
        TempDir::new().map_err(|e| anyhow::anyhow!("Failed to create test directory: {}", e))
    }

    /// Create test benchmark suite with known metrics
    pub fn create_test_suite(name: &str, metric_count: usize) -> crate::performance_reporter::BenchmarkSuite {
        let system_info = create_system_info();
        let mut suite = crate::performance_reporter::BenchmarkSuite {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            commit_hash: "test_commit".to_string(),
            timestamp: chrono::Utc::now(),
            system_info,
            metrics: Vec::new(),
        };

        for i in 0..metric_count {
            suite.metrics.push(create_metric(
                format!("test_metric_{}", i),
                "test_category".to_string(),
                (i + 1) as f64 * 10.0,
                "ms".to_string(),
                10,
                5,
            ));
        }

        suite
    }

    /// Verify test output files exist and have expected content
    pub fn verify_test_outputs(output_dir: &Path) -> Result<()> {
        let markdown_path = output_dir.join("PHASE6_1_PERFORMANCE_REPORT.md");
        let json_path = output_dir.join("benchmark_results.json");
        let summary_path = output_dir.join("PERFORMANCE_SUMMARY.md");

        // Check that files exist
        assert!(markdown_path.exists(), "Markdown report should exist");
        assert!(json_path.exists(), "JSON export should exist");
        assert!(summary_path.exists(), "Performance summary should exist");

        // Check that files have content
        let markdown_content = fs::read_to_string(&markdown_path)?;
        assert!(!markdown_content.is_empty(), "Markdown report should not be empty");
        assert!(markdown_content.contains("Phase 6.1"), "Markdown should contain phase identifier");

        let json_content = fs::read_to_string(&json_path)?;
        assert!(!json_content.is_empty(), "JSON export should not be empty");
        assert!(json_content.contains("\"suites\""), "JSON should contain suites");

        let summary_content = fs::read_to_string(&summary_path)?;
        assert!(!summary_content.is_empty(), "Summary should not be empty");
        assert!(summary_content.contains("Performance"), "Summary should contain performance");

        Ok(())
    }

    /// Performance test assertion helpers
    pub mod performance_asserts {
        use std::time::Duration;

        /// Assert that an operation completes within the expected time
        pub fn assert_performance_under(actual: Duration, expected_max: Duration, operation: &str) {
            assert!(
                actual <= expected_max,
                "Operation '{}' took {:?}, expected <= {:?}",
                operation, actual, expected_max
            );
        }

        /// Assert that memory growth is within acceptable bounds
        pub fn assert_memory_growth_within(
            initial: Option<usize>,
            final_mem: Option<usize>,
            max_growth_mb: usize,
            context: &str,
        ) {
            if let (Some(initial), Some(final_mem)) = (initial, final_mem) {
                let growth_bytes = final_mem.saturating_sub(initial);
                let growth_mb = growth_bytes / (1024 * 1024);
                assert!(
                    growth_mb <= max_growth_mb,
                    "Memory growth for '{}' was {}MB, expected <= {}MB",
                    context, growth_mb, max_growth_mb
                );
            }
        }

        /// Assert that scaling is approximately linear or better
        pub fn assert_linear_scaling(
            size_ratio: f64,
            time_ratio: f64,
            max_scaling_factor: f64,
            context: &str,
        ) {
            let scaling_factor = time_ratio / size_ratio;
            assert!(
                scaling_factor <= max_scaling_factor,
                "Scaling for '{}' was {:.2}x (time_ratio {:.2} / size_ratio {:.2}), expected <= {:.2}x",
                context, scaling_factor, time_ratio, size_ratio, max_scaling_factor
            );
        }
    }

    /// Mock data generators for testing
    pub mod mock_data {
        use crate::benchmark_utils::*;
        use serde_json::json;

        /// Create mock performance metrics with predictable values
        pub fn create_mock_metrics(count: usize) -> Vec<crate::performance_reporter::BenchmarkMetric> {
            (0..count).map(|i| {
                crate::performance_reporter::BenchmarkMetric {
                    name: format!("mock_metric_{}", i),
                    category: format!("category_{}", i % 5),
                    subcategory: Some(format!("subcategory_{}", i % 3)),
                    value: (i + 1) as f64 * 10.0,
                    unit: "ms".to_string(),
                    iterations: 100,
                    sample_size: 50,
                    std_deviation: Some(5.0),
                    min_value: Some((i + 1) as f64 * 8.0),
                    max_value: Some((i + 1) as f64 * 12.0),
                    percentile_95: Some((i + 1) as f64 * 11.0),
                    memory_usage_mb: Some((i + 1) as f64 * 0.1),
                    timestamp: chrono::Utc::now(),
                }
            }).collect()
        }

        /// Create mock system information
        pub fn create_mock_system_info() -> crate::performance_reporter::SystemInfo {
            crate::performance_reporter::SystemInfo {
                os: "test_os".to_string(),
                arch: "test_arch".to_string(),
                cpu_cores: 8,
                memory_gb: 16.0,
                rust_version: "1.70.0-test".to_string(),
                compiler_flags: "-O3 -test".to_string(),
            }
        }
    }

    /// Test environment setup and cleanup
    pub mod test_env {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        static TEST_ENV_SETUP: AtomicBool = AtomicBool::new(false);

        /// Ensure test environment is properly set up
        pub fn ensure_test_env() {
            if !TEST_ENV_SETUP.load(Ordering::SeqCst) {
                // Set up test environment (e.g., logging, temp directories)
                TEST_ENV_SETUP.store(true, Ordering::SeqCst);
            }
        }

        /// Clean up test environment
        pub fn cleanup_test_env() {
            // Clean up any global test state
            if TEST_ENV_SETUP.load(Ordering::SeqCst) {
                TEST_ENV_SETUP.store(false, Ordering::SeqCst);
            }
        }
    }
}

/// Integration test runner utilities
pub mod integration_runner {
    use super::test_config::*;
    use anyhow::Result;

    /// Run complete benchmark integration test
    pub fn run_complete_integration_test() -> Result<()> {
        test_env::ensure_test_env();

        let temp_dir = create_test_dir()?;
        let output_dir = temp_dir.path().join("integration_test");

        // Create test configuration
        let mut config = default_test_config();
        config.output_dir = output_dir.to_string_lossy().to_string();

        // Initialize runner
        let mut runner = crate::benchmark_runner::BenchmarkRunner::new(config);

        // Create test data
        let data_generator = TestDataGenerator::new()?;
        let _documents = data_generator.generate_documents(50, 5);
        let _events = data_generator.generate_events(100, &["test"]);

        // Create and add test suite
        let system_info = create_system_info();
        let suite = create_test_suite("Integration Test Suite", 10);
        runner.reporter.add_suite(suite);

        // Generate reports
        runner.generate_reports()?;

        // Verify outputs
        verify_test_outputs(&output_dir)?;

        test_env::cleanup_test_env();
        Ok(())
    }

    /// Run performance regression test
    pub fn run_performance_regression_test() -> Result<()> {
        test_env::ensure_test_env();

        // This would run the framework performance tests and compare against baseline
        // For now, we'll just verify the test infrastructure works

        let temp_dir = create_test_dir()?;
        let data_generator = TestDataGenerator::new()?;

        // Measure performance of key operations
        let start = std::time::Instant::now();
        let _documents = data_generator.generate_documents(100, 10);
        let generation_time = start.elapsed();

        // Performance should be reasonable
        assert!(
            generation_time < std::time::Duration::from_millis(100),
            "Document generation should be fast: {:?}", generation_time
        );

        test_env::cleanup_test_env();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_configuration() {
        let config = default_test_config();
        assert_eq!(config.output_dir, "test_benchmark_results");
        assert!(config.run_comparisons);
        assert!(!config.generate_plots);
        assert_eq!(config.export_formats.len(), 2);
    }

    #[test]
    fn test_test_suite_creation() {
        let suite = create_test_suite("Test Suite", 5);
        assert_eq!(suite.name, "Test Suite");
        assert_eq!(suite.metrics.len(), 5);

        for (i, metric) in suite.metrics.iter().enumerate() {
            assert_eq!(metric.name, format!("test_metric_{}", i));
            assert_eq!(metric.value, (i + 1) as f64 * 10.0);
        }
    }

    #[test]
    fn test_mock_data_generation() {
        let metrics = mock_data::create_mock_metrics(10);
        assert_eq!(metrics.len(), 10);

        for (i, metric) in metrics.iter().enumerate() {
            assert_eq!(metric.name, format!("mock_metric_{}", i));
            assert_eq!(metric.category, format!("category_{}", i % 5));
        }

        let system_info = mock_data::create_mock_system_info();
        assert_eq!(system_info.os, "test_os");
        assert_eq!(system_info.cpu_cores, 8);
    }

    #[test]
    fn test_performance_asserts() {
        use performance_asserts::*;

        let short_duration = Duration::from_millis(10);
        let long_duration = Duration::from_millis(100);

        // Should pass
        assert_performance_under(short_duration, Duration::from_millis(50), "test operation");

        // Should fail (uncomment to see assertion failure)
        // assert_performance_under(long_duration, Duration::from_millis(50), "slow operation");

        // Memory growth test
        let initial_mem = Some(100 * 1024 * 1024); // 100MB
        let final_mem = Some(105 * 1024 * 1024);   // 105MB
        assert_memory_growth_within(initial_mem, final_mem, 10, "test operation");

        // Scaling test
        assert_linear_scaling(10.0, 15.0, 2.0, "test scaling");
    }

    #[test]
    fn test_integration_runner() {
        integration_runner::run_complete_integration_test().unwrap();
        integration_runner::run_performance_regression_test().unwrap();
    }
}