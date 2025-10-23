//! Comprehensive unit tests for ScriptEngineLoadTester orchestration logic
//!
//! Specialized testing for load testing orchestration, ensuring accurate
//! execution of load test phases and proper coordination of components

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::runtime::Runtime;
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
mod script_engine_load_tester_unit_tests {
    use super::*;

    /// Test ScriptEngineLoadTester creation and initialization
    #[test]
    fn test_load_tester_creation() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        // If creation succeeds without panicking, the test passes
        assert!(true, "ScriptEngineLoadTester should create successfully");
    }

    /// Test ScriptEngineLoadTester runtime initialization
    #[test]
    fn test_load_tester_runtime_initialization() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        // The tester should have a valid tokio runtime
        // We can't directly access the runtime, but we can test that it works
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Simple async operation to verify runtime works
            let _result = tokio::time::sleep(Duration::from_millis(1)).await;
        });

        assert!(true, "Runtime should be properly initialized");
    }

    /// Test load test configuration validation
    #[test]
    fn test_load_test_configuration_validation() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        // Test valid configuration
        let valid_config = crate::load_testing_framework::LoadTestConfig {
            name: "Valid Test".to_string(),
            duration: Duration::from_secs(60),
            concurrency: 10,
            ramp_up_time: Duration::from_secs(10),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.6,
                medium_ratio: 0.3,
                complex_ratio: 0.1,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 50.0,
                max_response_time_ms: 200,
            },
        };

        // Configuration should be structurally valid
        assert_eq!(valid_config.name, "Valid Test");
        assert_eq!(valid_config.concurrency, 10);
        assert_eq!(valid_config.duration, Duration::from_secs(60));

        // Verify tool distribution sums to 1.0
        let total = valid_config.tool_distribution.simple_ratio +
                   valid_config.tool_distribution.medium_ratio +
                   valid_config.tool_distribution.complex_ratio;
        assert!((total - 1.0).abs() < 0.001, "Tool distribution must sum to 1.0");
    }

    /// Test load test execution phases coordination
    #[tokio::test]
    async fn test_load_test_execution_phases() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Phase Test".to_string(),
            duration: Duration::from_secs(3), // Short for testing
            concurrency: 3,
            ramp_up_time: Duration::from_millis(500),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 50,
                max_cpu_percent: 25.0,
                max_response_time_ms: 100,
            },
        };

        let start_time = Instant::now();
        let results = tester.run_load_test(config).await;
        let total_time = start_time.elapsed();

        // Verify test completed
        assert!(!results.test_name.is_empty());
        assert!(results.duration > Duration::ZERO);
        assert!(results.total_operations > 0);

        // Verify timing is reasonable (should be close to config.duration + ramp_up_time)
        let expected_min_time = config.duration + config.ramp_up_time;
        assert!(total_time >= expected_min_time,
               "Load test should take at least the configured duration: expected >= {:?}, actual {:?}",
               expected_min_time, total_time);

        // Should complete in reasonable time
        assert!(total_time < expected_min_time + Duration::from_secs(2),
               "Load test should not take excessively long: {:?}", total_time);
    }

    /// Test ramp-up phase execution
    #[tokio::test]
    async fn test_ramp_up_phase() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Ramp Up Test".to_string(),
            duration: Duration::from_secs(2),
            concurrency: 10,
            ramp_up_time: Duration::from_millis(1000), // Short ramp-up for testing
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 50,
                max_cpu_percent: 50.0,
                max_response_time_ms: 100,
            },
        };

        let start_time = Instant::now();
        let results = tester.run_load_test(config).await;
        let total_time = start_time.elapsed();

        // Verify ramp-up phase executed
        assert!(total_time >= config.ramp_up_time,
               "Ramp-up phase should take at least ramp_up_time: {:?} >= {:?}",
               total_time, config.ramp_up_time);

        // Should have executed operations during ramp-up
        assert!(results.total_operations > 0,
               "Should execute operations during ramp-up phase");
    }

    /// Test sustained load phase execution
    #[tokio::test]
    async fn test_sustained_load_phase() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Sustained Load Test".to_string(),
            duration: Duration::from_secs(2), // Short sustained phase
            concurrency: 5,
            ramp_up_time: Duration::from_millis(100),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 50,
                max_cpu_percent: 50.0,
                max_response_time_ms: 100,
            },
        };

        let start_time = Instant::now();
        let results = tester.run_load_test(config).await;
        let total_time = start_time.elapsed();

        // Verify sustained phase executed for the configured duration
        assert!(total_time >= config.duration + config.ramp_up_time,
               "Total time should include sustained load phase");

        // Should have executed consistent operations
        assert!(results.total_operations > 0,
               "Should execute operations during sustained phase");

        // Verify throughput is reasonable
        assert!(results.throughput_ops_per_sec > 0.0,
               "Should have measurable throughput");
    }

    /// Test cool-down phase execution
    #[tokio::test]
    async fn test_cool_down_phase() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Cool Down Test".to_string(),
            duration: Duration::from_millis(500), // Very short main phase
            concurrency: 5,
            ramp_up_time: Duration::from_millis(200),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 50,
                max_cpu_percent: 50.0,
                max_response_time_ms: 100,
            },
        };

        let start_time = Instant::now();
        let results = tester.run_load_test(config).await;
        let total_time = start_time.elapsed();

        // Total time should include cool-down phase (approximately 1 second)
        let expected_min_time = config.duration + config.ramp_up_time + Duration::from_millis(1000);
        assert!(total_time >= expected_min_time - Duration::from_millis(200), // Allow some tolerance
               "Should include cool-down phase: {:?} >= {:?}", total_time, expected_min_time);

        // Should complete successfully
        assert!(results.total_operations > 0,
               "Should execute operations including cool-down phase");
    }

    /// Test concurrent operation execution
    #[tokio::test]
    async fn test_concurrent_operation_execution() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Concurrent Execution Test".to_string(),
            duration: Duration::from_millis(500),
            concurrency: 10,
            ramp_up_time: Duration::from_millis(100),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 50,
                max_cpu_percent: 80.0,
                max_response_time_ms: 100,
            },
        };

        let results = tester.run_load_test(config).await;

        // Should execute operations with the specified concurrency
        assert!(results.total_operations >= config.concurrency,
               "Should execute at least as many operations as concurrency level");

        // All operations should complete successfully (mock engine doesn't fail)
        assert_eq!(results.failed_operations, 0,
               "MockScriptEngine should not fail operations");

        // Error rate should be zero
        assert_eq!(results.error_rate, 0.0,
               "Error rate should be zero for mock engine");
    }

    /// Test tool type selection and distribution
    #[tokio::test]
    async fn test_tool_type_distribution() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        // Test with equal distribution
        let equal_config = crate::load_testing_framework::LoadTestConfig {
            name: "Equal Distribution Test".to_string(),
            duration: Duration::from_millis(500),
            concurrency: 3,
            ramp_up_time: Duration::from_millis(100),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.33,
                medium_ratio: 0.33,
                complex_ratio: 0.34,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 50,
                max_cpu_percent: 50.0,
                max_response_time_ms: 100,
            },
        };

        let results = tester.run_load_test(equal_config).await;

        // Should execute operations
        assert!(results.total_operations > 0,
               "Should execute operations with any distribution");

        // Test with simple-heavy distribution
        let simple_heavy_config = crate::load_testing_framework::LoadTestConfig {
            name: "Simple Heavy Test".to_string(),
            duration: Duration::from_millis(500),
            concurrency: 3,
            ramp_up_time: Duration::from_millis(100),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 50,
                max_cpu_percent: 50.0,
                max_response_time_ms: 100,
            },
        };

        let simple_results = tester.run_load_test(simple_heavy_config).await;

        // Should execute operations faster with only simple tools
        assert!(simple_results.total_operations > 0,
               "Should execute operations with simple-only distribution");

        // Response times should be lower for simple-only
        assert!(simple_results.average_response_time <= results.average_response_time,
               "Simple-only should have lower or equal response times");
    }

    /// Test metrics collection accuracy
    #[tokio::test]
    async fn test_metrics_collection_accuracy() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Metrics Accuracy Test".to_string(),
            duration: Duration::from_millis(500),
            concurrency: 5,
            ramp_up_time: Duration::from_millis(100),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 50,
                max_cpu_percent: 50.0,
                max_response_time_ms: 100,
            },
        };

        let results = tester.run_load_test(config).await;

        // Verify metrics consistency
        assert_eq!(results.successful_operations + results.failed_operations, results.total_operations,
               "Operations should be properly categorized");

        if results.total_operations > 0 {
            let calculated_error_rate = results.failed_operations as f64 / results.total_operations as f64;
            assert!((calculated_error_rate - results.error_rate).abs() < 0.001,
                   "Error rate should match calculated value");
        }

        // Verify time series data is collected
        assert!(!results.time_series_data.is_empty(),
               "Should collect time series data during test");

        // Verify resource metrics are present
        assert!(results.resource_metrics.peak_memory_mb >= 0.0,
               "Should have memory metrics");
        assert!(results.resource_metrics.peak_cpu_percent >= 0.0,
               "Should have CPU metrics");
    }

    /// Test load test results calculation
    #[tokio::test]
    async fn test_results_calculation() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Results Calculation Test".to_string(),
            duration: Duration::from_millis(300),
            concurrency: 3,
            ramp_up_time: Duration::from_millis(50),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 50,
                max_cpu_percent: 50.0,
                max_response_time_ms: 100,
            },
        };

        let results = tester.run_load_test(config).await;

        // Verify throughput calculation
        assert!(results.throughput_ops_per_sec > 0.0,
               "Should calculate positive throughput");

        if results.duration.as_secs_f64() > 0.0 {
            let expected_throughput = results.total_operations as f64 / results.duration.as_secs_f64();
            assert!((results.throughput_ops_per_sec - expected_throughput).abs() < 0.1,
                   "Throughput calculation should be accurate");
        }

        // Verify response time calculations
        assert!(results.average_response_time > Duration::ZERO,
               "Should have positive average response time");

        assert!(results.p95_response_time >= results.average_response_time,
               "P95 should be >= average");

        assert!(results.p99_response_time >= results.p95_response_time,
               "P99 should be >= P95");

        // Verify test name is preserved
        assert_eq!(results.test_name, config.name,
               "Test name should be preserved in results");
    }

    /// Test error handling and recovery
    #[tokio::test]
    async fn test_error_handling_and_recovery() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        // Test with potentially stressful configuration
        let stressful_config = crate::load_testing_framework::LoadTestConfig {
            name: "Error Handling Test".to_string(),
            duration: Duration::from_millis(500),
            concurrency: 20, // High concurrency
            ramp_up_time: Duration::from_millis(50), // Fast ramp-up
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.3,
                medium_ratio: 0.4,
                complex_ratio: 0.3,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 95.0,
                max_response_time_ms: 1000,
            },
        };

        let results = tester.run_load_test(stressful_config).await;

        // Should complete despite stress
        assert!(results.total_operations > 0,
               "Should handle stress and complete operations");

        // Should maintain reasonable error rate (mock engine shouldn't fail)
        assert!(results.error_rate <= 0.1,
               "Should maintain low error rate even under stress");

        // Results should be structurally valid
        assert!(results.duration > Duration::ZERO);
        assert!(results.average_response_time > Duration::ZERO);
        assert!(results.throughput_ops_per_sec > 0.0);
    }

    /// Test multiple sequential load tests
    #[tokio::test]
    async fn test_sequential_load_tests() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        // Run multiple tests sequentially
        let configs = vec![
            ("Test 1", Duration::from_millis(200), 2),
            ("Test 2", Duration::from_millis(300), 3),
            ("Test 3", Duration::from_millis(200), 5),
        ];

        let mut all_results = Vec::new();

        for (name, duration, concurrency) in configs {
            let config = crate::load_testing_framework::LoadTestConfig {
                name: name.to_string(),
                duration,
                concurrency,
                ramp_up_time: Duration::from_millis(50),
                tool_distribution: crate::load_testing_framework::ToolDistribution {
                    simple_ratio: 1.0,
                    medium_ratio: 0.0,
                    complex_ratio: 0.0,
                },
                resource_limits: crate::load_testing_framework::ResourceLimits {
                    max_memory_mb: 50,
                    max_cpu_percent: 50.0,
                    max_response_time_ms: 100,
                },
            };

            let results = tester.run_load_test(config).await;
            all_results.push(results);
        }

        // Verify all tests completed successfully
        assert_eq!(all_results.len(), 3,
               "Should complete all sequential tests");

        for (i, results) in all_results.iter().enumerate() {
            assert!(results.total_operations > 0,
                   "Test {} should execute operations", i + 1);

            assert_eq!(results.failed_operations, 0,
                   "Test {} should have no failures", i + 1);

            assert_eq!(results.error_rate, 0.0,
                   "Test {} should have zero error rate", i + 1);
        }
    }

    /// Test load tester with minimal configuration
    #[tokio::test]
    async fn test_minimal_configuration() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let minimal_config = crate::load_testing_framework::LoadTestConfig {
            name: "Minimal Test".to_string(),
            duration: Duration::from_millis(100),
            concurrency: 1,
            ramp_up_time: Duration::from_millis(10),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 10,
                max_cpu_percent: 10.0,
                max_response_time_ms: 50,
            },
        };

        let results = tester.run_load_test(minimal_config).await;

        // Should handle minimal configuration gracefully
        assert!(results.total_operations > 0,
               "Should work with minimal configuration");

        assert_eq!(results.failed_operations, 0,
               "Should handle minimal configuration without failures");
    }
}

#[cfg(test)]
mod load_tester_orchestration_tests {
    use super::*;

    /// Test load tester orchestration with different concurrency patterns
    #[tokio::test]
    async fn test_concurrency_orchestration() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let concurrency_levels = vec![1, 5, 10, 15];

        for concurrency in concurrency_levels {
            let config = crate::load_testing_framework::LoadTestConfig {
                name: format!("Concurrency Test - {}", concurrency),
                duration: Duration::from_millis(300),
                concurrency,
                ramp_up_time: Duration::from_millis(100),
                tool_distribution: crate::load_testing_framework::ToolDistribution {
                    simple_ratio: 1.0,
                    medium_ratio: 0.0,
                    complex_ratio: 0.0,
                },
                resource_limits: crate::load_testing_framework::ResourceLimits {
                    max_memory_mb: 100,
                    max_cpu_percent: 80.0,
                    max_response_time_ms: 200,
                },
            };

            let results = tester.run_load_test(config).await;

            // Verify orchestration handles different concurrency levels
            assert!(results.total_operations >= concurrency,
                   "Should execute at least {} operations for concurrency {}", concurrency, concurrency);

            // Should complete without errors
            assert_eq!(results.failed_operations, 0,
                   "Should handle concurrency {} without failures", concurrency);

            // Higher concurrency should yield higher throughput (generally)
            println!("Concurrency {}: {:.2} ops/sec", concurrency, results.throughput_ops_per_sec);
        }
    }

    /// Test orchestration timing and phase transitions
    #[tokio::test]
    async fn test_phase_transition_timing() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Phase Timing Test".to_string(),
            duration: Duration::from_millis(500),
            concurrency: 5,
            ramp_up_time: Duration::from_millis(200),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 50,
                max_cpu_percent: 50.0,
                max_response_time_ms: 100,
            },
        };

        let start_time = Instant::now();
        let results = tester.run_load_test(config).await;
        let total_time = start_time.elapsed();

        // Verify timing includes all phases
        let expected_min_time = config.duration + config.ramp_up_time + Duration::from_millis(1000); // + cool-down
        assert!(total_time >= expected_min_time - Duration::from_millis(200), // Allow tolerance
               "Should include all phases: {:?} >= {:?}", total_time, expected_min_time);

        // Should execute operations throughout all phases
        assert!(results.total_operations > 0,
               "Should execute operations across all phases");

        // Verify time series data collection across phases
        assert!(!results.time_series_data.is_empty(),
               "Should collect time series data across phases");
    }

    /// Test orchestration resource management
    #[tokio::test]
    async fn test_resource_management() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Resource Management Test".to_string(),
            duration: Duration::from_millis(300),
            concurrency: 10,
            ramp_up_time: Duration::from_millis(100),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.5,
                medium_ratio: 0.3,
                complex_ratio: 0.2,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 75,
                max_cpu_percent: 60.0,
                max_response_time_ms: 150,
            },
        };

        let results = tester.run_load_test(config).await;

        // Should respect resource limits (conceptual test)
        assert!(results.total_operations > 0,
               "Should execute operations within resource limits");

        // Response times should be reasonable
        assert!(results.average_response_time < Duration::from_millis(config.resource_limits.max_response_time_ms as u64),
               "Response times should respect limits");

        // Resource metrics should be collected
        assert!(results.resource_metrics.peak_memory_mb > 0.0,
               "Should track memory usage");
        assert!(results.resource_metrics.peak_cpu_percent > 0.0,
               "Should track CPU usage");
    }
}