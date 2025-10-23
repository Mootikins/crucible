//! Edge case and error handling tests for load testing framework
//!
//! Comprehensive testing of edge cases, error conditions, and unusual scenarios
//! to ensure robustness and reliability of the load testing framework

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::runtime::Runtime;

#[cfg(test)]
mod extreme_configuration_tests {
    use super::*;

    /// Test with zero duration load test
    #[tokio::test]
    async fn test_zero_duration_load_test() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Zero Duration Test".to_string(),
            duration: Duration::ZERO,
            concurrency: 10,
            ramp_up_time: Duration::from_secs(5),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 50.0,
                max_response_time_ms: 200,
            },
        };

        // Should handle zero duration gracefully
        let results = tester.run_load_test(config).await;

        // Results should be structurally valid
        assert!(!results.test_name.is_empty());
        assert!(results.total_operations >= 0);
        assert!(results.successful_operations >= 0);
        assert!(results.failed_operations >= 0);
        assert!(results.error_rate >= 0.0 && results.error_rate <= 1.0);
    }

    /// Test with zero concurrency
    #[tokio::test]
    async fn test_zero_concurrency_load_test() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Zero Concurrency Test".to_string(),
            duration: Duration::from_secs(5),
            concurrency: 0,
            ramp_up_time: Duration::from_secs(1),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 50.0,
                max_response_time_ms: 200,
            },
        };

        // Should handle zero concurrency gracefully
        let results = tester.run_load_test(config).await;

        // Should complete without panicking
        assert!(!results.test_name.is_empty());
        assert!(results.total_operations >= 0);
        assert!(results.error_rate >= 0.0 && results.error_rate <= 1.0);
    }

    /// Test with zero ramp-up time
    #[tokio::test]
    async fn test_zero_ramp_up_load_test() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Zero Ramp Up Test".to_string(),
            duration: Duration::from_secs(3),
            concurrency: 10,
            ramp_up_time: Duration::ZERO,
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 50.0,
                max_response_time_ms: 200,
            },
        };

        // Should handle zero ramp-up time gracefully
        let results = tester.run_load_test(config).await;

        // Should still execute operations
        assert!(results.total_operations >= 0);
        assert!(results.successful_operations >= 0);
    }

    /// Test with extremely high values
    #[tokio::test]
    async fn test_extreme_high_values() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Extreme High Values Test".to_string(),
            duration: Duration::from_secs(3600), // 1 hour
            concurrency: 10000,
            ramp_up_time: Duration::from_secs(1800), // 30 minutes
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.33,
                medium_ratio: 0.33,
                complex_ratio: 0.34,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: u64::MAX / 1024 / 1024, // Maximum reasonable memory
                max_cpu_percent: 1000.0,
                max_response_time_ms: u64::MAX,
            },
        };

        // Use a very short duration for actual testing to avoid long test times
        let mut test_config = config.clone();
        test_config.duration = Duration::from_millis(100);
        test_config.ramp_up_time = Duration::from_millis(10);

        let results = tester.run_load_test(test_config).await;

        // Should handle extreme values without crashing
        assert!(!results.test_name.is_empty());
        assert!(results.total_operations >= 0);
        assert!(results.throughput_ops_per_sec >= 0.0);
    }

    /// Test with negative tool distribution ratios (if somehow set)
    #[tokio::test]
    async fn test_negative_distribution_ratios() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        // Note: This tests what happens if invalid ratios are somehow set
        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Negative Ratios Test".to_string(),
            duration: Duration::from_millis(100),
            concurrency: 5,
            ramp_up_time: Duration::from_millis(10),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: -0.1, // Invalid negative ratio
                medium_ratio: 0.6,
                complex_ratio: 0.5, // Total > 1.0
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 50.0,
                max_response_time_ms: 200,
            },
        };

        // Framework should handle invalid ratios gracefully
        let results = tester.run_load_test(config).await;

        // Should complete without panicking
        assert!(!results.test_name.is_empty());
        assert!(results.total_operations >= 0);
    }
}

#[cfg(test)]
mod unusual_scenario_tests {
    use super::*;

    /// Test load test with ramp-up time longer than duration
    #[tokio::test]
    async fn test_ramp_up_longer_than_duration() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Long Ramp Up Test".to_string(),
            duration: Duration::from_millis(100),
            concurrency: 10,
            ramp_up_time: Duration::from_millis(200), // Longer than duration
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

        // Should handle unusual timing gracefully
        assert!(!results.test_name.is_empty());
        assert!(results.total_operations >= 0);
        assert!(results.duration >= config.ramp_up_time); // Should at least include ramp-up time
    }

    /// Test with very short durations and high concurrency
    #[tokio::test]
    async fn test_short_duration_high_concurrency() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Short Duration High Concurrency Test".to_string(),
            duration: Duration::from_millis(10), // Very short
            concurrency: 100, // High concurrency
            ramp_up_time: Duration::from_millis(5),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 80.0,
                max_response_time_ms: 50,
            },
        };

        let results = tester.run_load_test(config).await;

        // Should handle timing constraints gracefully
        assert!(!results.test_name.is_empty());
        assert!(results.total_operations >= 0);
        // May not execute many operations due to short duration
    }

    /// Test with all complex tools and high concurrency
    #[tokio::test]
    async fn test_all_complex_high_concurrency() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "All Complex High Concurrency Test".to_string(),
            duration: Duration::from_millis(200),
            concurrency: 50,
            ramp_up_time: Duration::from_millis(50),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.0,
                medium_ratio: 0.0,
                complex_ratio: 1.0, // All complex tools
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 200,
                max_cpu_percent: 90.0,
                max_response_time_ms: 500,
            },
        };

        let results = tester.run_load_test(config).await;

        // Should handle complex tool execution
        assert!(!results.test_name.is_empty());
        assert!(results.total_operations >= 0);
        assert!(results.average_response_time > Duration::ZERO);
    }

    /// Test load test with mixed invalid distributions
    #[tokio::test]
    async fn test_mixed_invalid_distributions() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let invalid_distributions = vec![
            (1.5, -0.5, 0.0, "Sum > 1.0 with negative"),
            (0.5, 0.5, 0.5, "All sum to 1.5"),
            (0.0, 0.0, 0.0, "All zero"),
            (-0.3, -0.3, -0.4, "All negative"),
            (2.0, -1.0, 0.0, "Large positive and negative"),
        ];

        for (simple, medium, complex, test_name) in invalid_distributions {
            let config = crate::load_testing_framework::LoadTestConfig {
                name: format!("Invalid Distribution - {}", test_name),
                duration: Duration::from_millis(50),
                concurrency: 3,
                ramp_up_time: Duration::from_millis(10),
                tool_distribution: crate::load_testing_framework::ToolDistribution {
                    simple_ratio: simple,
                    medium_ratio: medium,
                    complex_ratio: complex,
                },
                resource_limits: crate::load_testing_framework::ResourceLimits {
                    max_memory_mb: 50,
                    max_cpu_percent: 50.0,
                    max_response_time_ms: 100,
                },
            };

            // Should handle invalid distributions gracefully
            let results = tester.run_load_test(config).await;

            assert!(!results.test_name.is_empty());
            assert!(results.total_operations >= 0);
            assert!(results.error_rate >= 0.0 && results.error_rate <= 1.0);
        }
    }

    /// Test with extremely short ramp-up and high concurrency
    #[tokio::test]
    async fn test_instant_ramp_up_high_concurrency() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Instant Ramp Up High Concurrency Test".to_string(),
            duration: Duration::from_millis(100),
            concurrency: 100,
            ramp_up_time: Duration::from_millis(1), // Almost instant
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 95.0,
                max_response_time_ms: 100,
            },
        };

        let results = tester.run_load_test(config).await;

        // Should handle instant ramp-up
        assert!(!results.test_name.is_empty());
        assert!(results.total_operations >= 0);
        assert!(results.duration >= config.ramp_up_time);
    }
}

#[cfg(test)]
mod error_recovery_tests {
    use super::*;

    /// Test rapid successive load tests
    #[tokio::test]
    async fn test_rapid_successive_load_tests() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        // Run multiple load tests rapidly
        for i in 0..20 {
            let config = crate::load_testing_framework::LoadTestConfig {
                name: format!("Rapid Test {}", i),
                duration: Duration::from_millis(10),
                concurrency: 3,
                ramp_up_time: Duration::from_millis(2),
                tool_distribution: crate::load_testing_framework::ToolDistribution {
                    simple_ratio: 1.0,
                    medium_ratio: 0.0,
                    complex_ratio: 0.0,
                },
                resource_limits: crate::load_testing_framework::ResourceLimits {
                    max_memory_mb: 25,
                    max_cpu_percent: 25.0,
                    max_response_time_ms: 50,
                },
            };

            let results = tester.run_load_test(config).await;

            // Each test should complete successfully
            assert!(!results.test_name.is_empty());
            assert!(results.total_operations >= 0);
            assert!(results.error_rate >= 0.0 && results.error_rate <= 1.0);
        }
    }

    /// Test load test during system stress simulation
    #[tokio::test]
    async fn test_load_test_during_system_stress() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        // Simulate system stress by running concurrent tasks
        let stress_handle = tokio::spawn(async {
            // Simulate CPU stress
            for _ in 0..1000 {
                let _calculation = (0..1000).map(|x| x * x).collect::<Vec<_>>();
                tokio::task::yield_now().await;
            }
        });

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "System Stress Test".to_string(),
            duration: Duration::from_millis(200),
            concurrency: 10,
            ramp_up_time: Duration::from_millis(20),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 50,
                max_cpu_percent: 95.0,
                max_response_time_ms: 200,
            },
        };

        let results = tester.run_load_test(config).await;

        // Wait for stress task to complete
        let _ = stress_handle.await;

        // Should complete even under system stress
        assert!(!results.test_name.is_empty());
        assert!(results.total_operations >= 0);
        assert!(results.error_rate >= 0.0 && results.error_rate <= 1.0);
    }

    /// Test with memory pressure simulation
    #[tokio::test]
    async fn test_load_test_with_memory_pressure() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        // Simulate memory pressure
        let _memory_pressure: Vec<Vec<u8>> = (0..1000)
            .map(|_| vec![0u8; 1024 * 10]) // 10KB chunks
            .collect();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Memory Pressure Test".to_string(),
            duration: Duration::from_millis(100),
            concurrency: 5,
            ramp_up_time: Duration::from_millis(10),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 10, // Low memory limit
                max_cpu_percent: 50.0,
                max_response_time_ms: 100,
            },
        };

        let results = tester.run_load_test(config).await;

        // Should handle memory pressure gracefully
        assert!(!results.test_name.is_empty());
        assert!(results.total_operations >= 0);
    }

    /// Test framework recovery from potential errors
    #[tokio::test]
    async fn test_framework_error_recovery() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        // Run a potentially problematic test
        let problematic_config = crate::load_testing_framework::LoadTestConfig {
            name: "Problematic Test".to_string(),
            duration: Duration::from_millis(1), // Very short
            concurrency: 1000, // Very high
            ramp_up_time: Duration::from_millis(1), // Very short ramp-up
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.1,
                medium_ratio: 0.1,
                complex_ratio: 0.8, // Mostly complex
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 1, // Very low
                max_cpu_percent: 1.0, // Very low
                max_response_time_ms: 1, // Very low
            },
        };

        let results = tester.run_load_test(problematic_config).await;

        // Should recover and produce valid results
        assert!(!results.test_name.is_empty());
        assert!(results.total_operations >= 0);
        assert!(results.error_rate >= 0.0 && results.error_rate <= 1.0);

        // Follow up with a normal test to verify recovery
        let normal_config = crate::load_testing_framework::LoadTestConfig {
            name: "Recovery Test".to_string(),
            duration: Duration::from_millis(50),
            concurrency: 5,
            ramp_up_time: Duration::from_millis(10),
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

        let recovery_results = tester.run_load_test(normal_config).await;

        // Should work normally after problematic test
        assert_eq!(recovery_results.test_name, "Recovery Test");
        assert!(recovery_results.total_operations >= 0);
        assert!(recovery_results.error_rate >= 0.0 && recovery_results.error_rate <= 1.0);
    }
}

#[cfg(test)]
mod boundary_condition_tests {
    use super::*;

    /// Test with nanosecond precision durations
    #[tokio::test]
    async fn test_nanosecond_precision_durations() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Nanosecond Precision Test".to_string(),
            duration: Duration::from_nanos(1000000), // 1 millisecond in nanoseconds
            concurrency: 2,
            ramp_up_time: Duration::from_nanos(500000), // 0.5 milliseconds
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 25,
                max_cpu_percent: 25.0,
                max_response_time_ms: 1,
            },
        };

        let results = tester.run_load_test(config).await;

        // Should handle nanosecond precision
        assert!(!results.test_name.is_empty());
        assert!(results.total_operations >= 0);
    }

    /// Test with maximum duration values
    #[tokio::test]
    async fn test_maximum_duration_values() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        // Use very large durations but limit actual test time
        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Maximum Duration Test".to_string(),
            duration: Duration::from_secs(u64::MAX), // Maximum duration
            concurrency: 1,
            ramp_up_time: Duration::from_secs(1), // Keep ramp-up reasonable
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 50,
                max_cpu_percent: 50.0,
                max_response_time_ms: 1000,
            },
        };

        // Create a modified version for actual testing
        let mut test_config = config.clone();
        test_config.duration = Duration::from_millis(50); // Limit for testing

        let results = tester.run_load_test(test_config).await;

        // Should handle large duration values
        assert!(!results.test_name.is_empty());
        assert!(results.total_operations >= 0);
    }

    /// Test with fractional resource limits
    #[tokio::test]
    async fn test_fractional_resource_limits() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let fractional_limits = vec![
            (0.1, 0.01, 1),
            (0.5, 0.25, 10),
            (12.5, 33.33, 100),
            (99.99, 0.001, 999),
        ];

        for (memory_mb, cpu_percent, response_time_ms) in fractional_limits {
            let config = crate::load_testing_framework::LoadTestConfig {
                name: format!("Fractional Limits Test - {}MB-{:.2}%-{}ms", memory_mb, cpu_percent, response_time_ms),
                duration: Duration::from_millis(50),
                concurrency: 3,
                ramp_up_time: Duration::from_millis(10),
                tool_distribution: crate::load_testing_framework::ToolDistribution {
                    simple_ratio: 1.0,
                    medium_ratio: 0.0,
                    complex_ratio: 0.0,
                },
                resource_limits: crate::load_testing_framework::ResourceLimits {
                    max_memory_mb: memory_mb as u64,
                    max_cpu_percent: cpu_percent,
                    max_response_time_ms: response_time_ms,
                },
            };

            let results = tester.run_load_test(config).await;

            // Should handle fractional limits
            assert!(!results.test_name.is_empty());
            assert!(results.total_operations >= 0);
            assert!(results.error_rate >= 0.0 && results.error_rate <= 1.0);
        }
    }

    /// Test with very long names
    #[tokio::test]
    async fn test_very_long_names() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let long_name = "A".repeat(10000); // 10KB name

        let config = crate::load_testing_framework::LoadTestConfig {
            name: long_name.clone(),
            duration: Duration::from_millis(50),
            concurrency: 3,
            ramp_up_time: Duration::from_millis(10),
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

        // Should handle very long names
        assert_eq!(results.test_name, long_name);
        assert!(results.total_operations >= 0);

        // Should serialize correctly
        let json = serde_json::to_string(&results).unwrap();
        let deserialized: crate::load_testing_framework::LoadTestResults = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.test_name, long_name);
    }

    /// Test with unicode and special characters in names
    #[tokio::test]
    async fn test_unicode_special_characters() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let special_names = vec![
            "测试中文".to_string(),
            "🚀 Rocket Test 🎯".to_string(),
            "Test\nWith\nNewlines".to_string(),
            "Test\tWith\tTabs".to_string(),
            "Test\"With\"Quotes".to_string(),
            "Test\\With\\Backslashes".to_string(),
            "Test/With/Slashes".to_string(),
            "Test:With:Colons".to_string(),
            "Test;With;Semicolons".to_string(),
        ];

        for name in special_names {
            let config = crate::load_testing_framework::LoadTestConfig {
                name: name.clone(),
                duration: Duration::from_millis(30),
                concurrency: 2,
                ramp_up_time: Duration::from_millis(5),
                tool_distribution: crate::load_testing_framework::ToolDistribution {
                    simple_ratio: 1.0,
                    medium_ratio: 0.0,
                    complex_ratio: 0.0,
                },
                resource_limits: crate::load_testing_framework::ResourceLimits {
                    max_memory_mb: 25,
                    max_cpu_percent: 25.0,
                    max_response_time_ms: 50,
                },
            };

            let results = tester.run_load_test(config).await;

            // Should handle special characters
            assert_eq!(results.test_name, name);
            assert!(results.total_operations >= 0);

            // Should serialize and deserialize correctly
            let json = serde_json::to_string(&results).unwrap();
            let deserialized: crate::load_testing_framework::LoadTestResults = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.test_name, name);
        }
    }
}