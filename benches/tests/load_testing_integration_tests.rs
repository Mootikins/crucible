//! Integration tests for ScriptEngine load testing framework
//!
//! End-to-end testing of load testing workflows and framework integration

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::path::Path;
use tokio::runtime::Runtime;

#[cfg(test)]
mod end_to_end_tests {
    use super::*;

    /// Test complete load testing workflow
    #[tokio::test]
    async fn test_complete_load_testing_workflow() {
        // This test validates the entire load testing workflow
        // from configuration through execution and results analysis

        // Create load tester
        let tester = create_test_load_tester().await;

        // Create a minimal test configuration
        let config = LoadTestConfig {
            name: "E2E Test Workflow".to_string(),
            duration: Duration::from_secs(5), // Short duration for testing
            concurrency: 3,
            ramp_up_time: Duration::from_secs(1),
            tool_distribution: ToolDistribution {
                simple_ratio: 0.7,
                medium_ratio: 0.2,
                complex_ratio: 0.1,
            },
            resource_limits: ResourceLimits {
                max_memory_mb: 50,
                max_cpu_percent: 25.0,
                max_response_time_ms: 100,
            },
        };

        // Execute load test
        let results = tester.run_load_test(config).await;

        // Validate results
        assert!(!results.test_name.is_empty());
        assert!(results.duration > Duration::ZERO);
        assert!(results.total_operations > 0);
        assert!(results.successful_operations > 0);
        assert!(results.throughput_ops_per_sec > 0.0);
        assert!(results.error_rate >= 0.0 && results.error_rate <= 1.0);
    }

    /// Test load testing with different tool distributions
    #[tokio::test]
    async fn test_different_tool_distributions() {
        let tester = create_test_load_tester().await;

        let distributions = vec![
            ("All Simple", ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            }),
            ("All Medium", ToolDistribution {
                simple_ratio: 0.0,
                medium_ratio: 1.0,
                complex_ratio: 0.0,
            }),
            ("All Complex", ToolDistribution {
                simple_ratio: 0.0,
                medium_ratio: 0.0,
                complex_ratio: 1.0,
            }),
            ("Mixed", ToolDistribution {
                simple_ratio: 0.5,
                medium_ratio: 0.3,
                complex_ratio: 0.2,
            }),
        ];

        for (name, distribution) in distributions {
            let config = LoadTestConfig {
                name: format!("Distribution Test - {}", name),
                duration: Duration::from_secs(3),
                concurrency: 2,
                ramp_up_time: Duration::from_millis(500),
                tool_distribution: distribution,
                resource_limits: ResourceLimits {
                    max_memory_mb: 25,
                    max_cpu_percent: 20.0,
                    max_response_time_ms: 50,
                },
            };

            let results = tester.run_load_test(config).await;

            // Basic validation that the test ran
            assert!(results.total_operations > 0, "No operations for distribution: {}", name);
            assert!(results.successful_operations > 0, "No successful operations for distribution: {}", name);
        }
    }

    /// Test load testing with varying concurrency levels
    #[tokio::test]
    async fn test_varying_concurrency_levels() {
        let tester = create_test_load_tester().await;

        let concurrency_levels = vec![1, 2, 5];

        for concurrency in concurrency_levels {
            let config = LoadTestConfig {
                name: format!("Concurrency Test - {}", concurrency),
                duration: Duration::from_secs(3),
                concurrency,
                ramp_up_time: Duration::from_millis(500),
                tool_distribution: ToolDistribution {
                    simple_ratio: 0.8,
                    medium_ratio: 0.15,
                    complex_ratio: 0.05,
                },
                resource_limits: ResourceLimits {
                    max_memory_mb: 50,
                    max_cpu_percent: 50.0,
                    max_response_time_ms: 100,
                },
            };

            let results = tester.run_load_test(config).await;

            // Verify results make sense for the concurrency level
            assert!(results.total_operations >= concurrency,
                   "Expected at least {} operations for concurrency {}", concurrency, concurrency);

            // Higher concurrency should generally yield higher throughput
            println!("Concurrency {}: {:.2} ops/sec", concurrency, results.throughput_ops_per_sec);
        }
    }

    /// Test error handling and recovery
    #[tokio::test]
    async fn test_error_handling_and_recovery() {
        let tester = create_test_load_tester().await;

        // Create a configuration that might stress the system
        let config = LoadTestConfig {
            name: "Error Handling Test".to_string(),
            duration: Duration::from_secs(5),
            concurrency: 10,
            ramp_up_time: Duration::from_millis(100),
            tool_distribution: ToolDistribution {
                simple_ratio: 0.3,
                medium_ratio: 0.4,
                complex_ratio: 0.3,
            },
            resource_limits: ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 90.0,
                max_response_time_ms: 1000,
            },
        };

        let results = tester.run_load_test(config).await;

        // Should handle errors gracefully
        assert!(results.total_operations > 0);

        // Error rate should be reasonable (might be 0 in mock environment)
        assert!(results.error_rate <= 1.0);

        // Even with errors, we should get some successful operations
        assert!(results.successful_operations > 0);
    }

    /// Test resource limit validation
    #[tokio::test]
    async fn test_resource_limit_validation() {
        let tester = create_test_load_tester().await;

        // Test with very strict resource limits
        let config = LoadTestConfig {
            name: "Resource Limit Test".to_string(),
            duration: Duration::from_secs(3),
            concurrency: 5,
            ramp_up_time: Duration::from_millis(200),
            tool_distribution: ToolDistribution {
                simple_ratio: 0.9,
                medium_ratio: 0.1,
                complex_ratio: 0.0,
            },
            resource_limits: ResourceLimits {
                max_memory_mb: 10,  // Very low limit
                max_cpu_percent: 15.0,  // Very low limit
                max_response_time_ms: 50,  // Very low limit
            },
        };

        let results = tester.run_load_test(config).await;

        // Should still complete but possibly with reduced performance
        assert!(results.total_operations > 0);

        // Response times should be reasonable even with limits
        assert!(results.average_response_time < Duration::from_millis(1000));
    }

    /// Test metrics collection accuracy
    #[tokio::test]
    async fn test_metrics_collection_accuracy() {
        let tester = create_test_load_tester().await;

        let config = LoadTestConfig {
            name: "Metrics Accuracy Test".to_string(),
            duration: Duration::from_secs(2),
            concurrency: 3,
            ramp_up_time: Duration::from_millis(100),
            tool_distribution: ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: ResourceLimits {
                max_memory_mb: 25,
                max_cpu_percent: 30.0,
                max_response_time_ms: 100,
            },
        };

        let results = tester.run_load_test(config).await;

        // Validate metrics consistency
        assert!(results.successful_operations + results.failed_operations == results.total_operations);

        if results.total_operations > 0 {
            let calculated_error_rate = results.failed_operations as f64 / results.total_operations as f64;
            assert!((calculated_error_rate - results.error_rate).abs() < 0.001);
        }

        // Validate time series data
        assert!(!results.time_series_data.is_empty());

        // Validate resource metrics
        assert!(results.resource_metrics.peak_memory_mb >= 0.0);
        assert!(results.resource_metrics.average_memory_mb >= 0.0);
        assert!(results.resource_metrics.peak_cpu_percent >= 0.0);
        assert!(results.resource_metrics.average_cpu_percent >= 0.0);
    }

    /// Test framework cleanup and resource management
    #[tokio::test]
    async fn test_framework_cleanup_and_resource_management() {
        // Run multiple tests to verify proper cleanup
        let tester = create_test_load_tester().await;

        for i in 0..3 {
            let config = LoadTestConfig {
                name: format!("Cleanup Test {}", i),
                duration: Duration::from_millis(500),
                concurrency: 2,
                ramp_up_time: Duration::from_millis(100),
                tool_distribution: ToolDistribution {
                    simple_ratio: 1.0,
                    medium_ratio: 0.0,
                    complex_ratio: 0.0,
                },
                resource_limits: ResourceLimits {
                    max_memory_mb: 20,
                    max_cpu_percent: 25.0,
                    max_response_time_ms: 50,
                },
            };

            let results = tester.run_load_test(config).await;

            // Each test should complete successfully
            assert!(results.total_operations > 0);
            assert!(results.successful_operations > 0);
        }
    }

    async fn create_test_load_tester() -> crate::load_testing_framework::ScriptEngineLoadTester {
        crate::load_testing_framework::ScriptEngineLoadTester::new()
    }
}

#[cfg(test)]
mod benchmark_integration_tests {
    use super::*;

    /// Test that all benchmark functions are callable
    #[test]
    fn test_benchmark_function_availability() {
        // This test ensures all benchmark functions are properly defined
        let benchmark_file = std::fs::read_to_string("benches/script_engine_load_tests.rs")
            .expect("Failed to read benchmark file");

        // Check for all expected benchmark functions
        let expected_benchmarks = vec![
            "bench_concurrent_tool_execution",
            "bench_sustained_load",
            "bench_mixed_workload",
            "bench_resource_usage_under_load",
            "bench_error_handling_under_load",
        ];

        for benchmark in expected_benchmarks {
            assert!(benchmark_file.contains(&format!("fn {}", benchmark)),
                   "Missing benchmark function: {}", benchmark);
        }

        // Check for proper benchmark group registration
        assert!(benchmark_file.contains("criterion_group!"));
        assert!(benchmark_file.contains("load_tests"));
        assert!(benchmark_file.contains("criterion_main!"));
    }

    /// Test benchmark compilation with different features
    #[test]
    fn test_benchmark_compilation_with_features() {
        // Test that benchmarks compile with all required features
        let output = std::process::Command::new("cargo")
            .args(&["check", "--bench", "script_engine_load_tests", "--all-features"])
            .output()
            .expect("Failed to run cargo check with features");

        assert!(output.status.success(),
               "Benchmark compilation with features failed: {}",
               String::from_utf8_lossy(&output.stderr));
    }

    /// Test benchmark dependencies
    #[test]
    fn test_benchmark_dependencies() {
        let output = std::process::Command::new("cargo")
            .args(&["tree", "--package", "crucible-benchmarks", "--format", "{p}"])
            .output()
            .expect("Failed to check dependencies");

        let dependencies = String::from_utf8_lossy(&output.stdout);

        // Check for required benchmark dependencies
        let required_deps = vec![
            "criterion",
            "tokio",
            "futures",
            "rand",
            "serde",
            "serde_json",
        ];

        for dep in required_deps {
            assert!(dependencies.contains(dep), "Missing dependency: {}", dep);
        }
    }
}

#[cfg(test)]
mod configuration_tests {
    use super::*;

    /// Test configuration validation
    #[test]
    fn test_configuration_validation() {
        // Test valid configuration
        let valid_config = LoadTestConfig {
            name: "Valid Config".to_string(),
            duration: Duration::from_secs(60),
            concurrency: 10,
            ramp_up_time: Duration::from_secs(10),
            tool_distribution: ToolDistribution {
                simple_ratio: 0.5,
                medium_ratio: 0.3,
                complex_ratio: 0.2,
            },
            resource_limits: ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 50.0,
                max_response_time_ms: 200,
            },
        };

        // Validate tool distribution sums to 1.0
        let total = valid_config.tool_distribution.simple_ratio +
                   valid_config.tool_distribution.medium_ratio +
                   valid_config.tool_distribution.complex_ratio;
        assert!((total - 1.0).abs() < 0.001);

        // Validate reasonable values
        assert!(valid_config.concurrency > 0);
        assert!(valid_config.duration > Duration::ZERO);
        assert!(valid_config.ramp_up_time <= valid_config.duration);
        assert!(valid_config.resource_limits.max_memory_mb > 0);
        assert!(valid_config.resource_limits.max_cpu_percent > 0.0);
        assert!(valid_config.resource_limits.max_response_time_ms > 0);
    }

    /// Test predefined configurations
    #[test]
    fn test_predefined_configurations() {
        // Test that all predefined configurations are valid
        let configs = vec![
            ("light", || crate::load_testing_framework::configurations::light_load_test()),
            ("medium", || crate::load_testing_framework::configurations::medium_load_test()),
            ("heavy", || crate::load_testing_framework::configurations::heavy_load_test()),
            ("stress", || crate::load_testing_framework::configurations::stress_test()),
        ];

        for (name, config_fn) in configs {
            let config = config_fn();

            // Basic validation
            assert!(!config.name.is_empty(), "Configuration {} has empty name", name);
            assert!(config.duration > Duration::ZERO, "Configuration {} has zero duration", name);
            assert!(config.concurrency > 0, "Configuration {} has zero concurrency", name);

            // Validate tool distribution
            let total = config.tool_distribution.simple_ratio +
                       config.tool_distribution.medium_ratio +
                       config.tool_distribution.complex_ratio;
            assert!((total - 1.0).abs() < 0.001, "Configuration {} has invalid tool distribution", name);

            // Validate resource limits
            assert!(config.resource_limits.max_memory_mb > 0, "Configuration {} has invalid memory limit", name);
            assert!(config.resource_limits.max_cpu_percent > 0.0, "Configuration {} has invalid CPU limit", name);
            assert!(config.resource_limits.max_response_time_ms > 0, "Configuration {} has invalid response time limit", name);
        }
    }
}

// Import required types from the load testing framework
use crate::load_testing_framework::*;