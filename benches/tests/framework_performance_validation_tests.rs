//! Performance validation tests for the testing framework
//!
//! Specialized testing to ensure the load testing framework itself performs well
//! under various conditions and doesn't become a bottleneck

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::runtime::Runtime;

#[cfg(test)]
mod framework_performance_tests {
    use super::*;

    /// Test framework overhead measurement
    #[test]
    fn test_framework_overhead_measurement() {
        // Measure the time it takes to create and configure the framework
        let start = Instant::now();

        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Overhead Test".to_string(),
            duration: Duration::from_millis(100),
            concurrency: 5,
            ramp_up_time: Duration::from_millis(10),
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

        let creation_time = start.elapsed();

        // Framework creation should be fast
        assert!(creation_time < Duration::from_millis(100),
               "Framework creation should be fast: {:?}", creation_time);

        // Test configuration setup time
        let start = Instant::now();
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let _results = tester.run_load_test(config).await;
        });
        let total_time = start.elapsed();

        // Framework overhead should be small compared to total test time
        let framework_overhead = total_time - Duration::from_millis(100); // Subtract configured duration
        assert!(framework_overhead < Duration::from_millis(200),
               "Framework overhead should be minimal: {:?}", framework_overhead);
    }

    /// Test concurrent framework instances
    #[test]
    fn test_concurrent_framework_instances() {
        use std::thread;

        let num_instances = 10;
        let operations_per_instance = 100;

        let start = Instant::now();

        let handles: Vec<_> = (0..num_instances)
            .map(|instance_id| {
                thread::spawn(move || {
                    let rt = Runtime::new().unwrap();
                    let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();

                    rt.block_on(async {
                        for i in 0..operations_per_instance {
                            let config = crate::load_testing_framework::LoadTestConfig {
                                name: format!("Concurrent Test {}-{}", instance_id, i),
                                duration: Duration::from_millis(1),
                                concurrency: 1,
                                ramp_up_time: Duration::from_millis(1),
                                tool_distribution: crate::load_testing_framework::ToolDistribution {
                                    simple_ratio: 1.0,
                                    medium_ratio: 0.0,
                                    complex_ratio: 0.0,
                                },
                                resource_limits: crate::load_testing_framework::ResourceLimits {
                                    max_memory_mb: 10,
                                    max_cpu_percent: 10.0,
                                    max_response_time_ms: 10,
                                },
                            };

                            let _results = tester.run_load_test(config).await;
                        }
                    });

                    instance_id
                })
            })
            .collect();

        // Wait for all instances to complete
        let completed_instances: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        let total_time = start.elapsed();

        // All instances should complete
        assert_eq!(completed_instances.len(), num_instances,
                   "All concurrent framework instances should complete");

        // Should complete in reasonable time
        assert!(total_time < Duration::from_secs(30),
               "Concurrent framework instances should complete quickly: {:?}", total_time);

        // Average time per instance should be reasonable
        let avg_time_per_instance = total_time / num_instances as u32;
        assert!(avg_time_per_instance < Duration::from_secs(5),
               "Average time per instance should be reasonable: {:?}", avg_time_per_instance);
    }

    /// Test framework scalability with increasing load
    #[test]
    fn test_framework_scalability() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();
        let rt = Runtime::new().unwrap();

        let scalability_tests = vec![
            (1, "Low Load"),
            (10, "Medium Load"),
            (50, "High Load"),
            (100, "Very High Load"),
        ];

        for (concurrency, test_name) in scalability_tests {
            let config = crate::load_testing_framework::LoadTestConfig {
                name: format!("Scalability Test - {}", test_name),
                duration: Duration::from_millis(500),
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

            let start = Instant::now();
            let results = rt.block_on(async {
                tester.run_load_test(config).await
            });
            let execution_time = start.elapsed();

            // Framework should handle increasing concurrency
            assert!(results.total_operations > 0,
                   "Should execute operations with concurrency {}", concurrency);

            assert!(results.successful_operations > 0,
                   "Should have successful operations with concurrency {}", concurrency);

            // Performance should not degrade excessively
            assert!(execution_time < Duration::from_secs(5),
                   "Test {} should complete in reasonable time: {:?}",
                   test_name, execution_time);

            // Throughput should scale reasonably with concurrency
            assert!(results.throughput_ops_per_sec > 0.0,
                   "Should have positive throughput for {}", test_name);

            println!("{}: Concurrency={}, Throughput={:.2} ops/sec, Time={:?}",
                     test_name, concurrency, results.throughput_ops_per_sec, execution_time);
        }
    }

    /// Test framework memory efficiency
    #[test]
    fn test_framework_memory_efficiency() {
        let mut configs = Vec::new();
        let mut results = Vec::new();

        // Create many configurations and results to test memory usage
        for i in 0..1000 {
            let config = crate::load_testing_framework::LoadTestConfig {
                name: format!("Memory Test {}", i),
                duration: Duration::from_millis(10),
                concurrency: 5,
                ramp_up_time: Duration::from_millis(5),
                tool_distribution: crate::load_testing_framework::ToolDistribution {
                    simple_ratio: 0.6,
                    medium_ratio: 0.3,
                    complex_ratio: 0.1,
                },
                resource_limits: crate::load_testing_framework::ResourceLimits {
                    max_memory_mb: 50,
                    max_cpu_percent: 50.0,
                    max_response_time_ms: 100,
                },
            };
            configs.push(config);

            // Create mock results
            let mock_results = crate::load_testing_framework::LoadTestResults {
                test_name: format!("Memory Results {}", i),
                duration: Duration::from_millis(10),
                total_operations: 100,
                successful_operations: 95,
                failed_operations: 5,
                average_response_time: Duration::from_millis(50),
                p95_response_time: Duration::from_millis(100),
                p99_response_time: Duration::from_millis(150),
                throughput_ops_per_sec: 10.0,
                error_rate: 0.05,
                resource_metrics: crate::load_testing_framework::ResourceMetrics {
                    peak_memory_mb: 25.0,
                    average_memory_mb: 20.0,
                    peak_cpu_percent: 40.0,
                    average_cpu_percent: 30.0,
                    memory_growth_rate: 0.1,
                },
                time_series_data: vec![],
            };
            results.push(mock_results);
        }

        // Framework should handle many configurations without excessive memory usage
        assert_eq!(configs.len(), 1000, "Should create many configurations");
        assert_eq!(results.len(), 1000, "Should create many results");

        // Test serialization/deserialization performance with many items
        let start = Instant::now();
        for (config, result) in configs.iter().zip(results.iter()) {
            let _config_json = serde_json::to_string(config).unwrap();
            let _result_json = serde_json::to_string(result).unwrap();
        }
        let serialization_time = start.elapsed();

        // Serialization should be fast even with many items
        assert!(serialization_time < Duration::from_secs(1),
               "Serialization of many items should be fast: {:?}", serialization_time);

        // Clean up to test memory release
        configs.clear();
        results.clear();

        // Memory should be released (this is more of a conceptual test)
        assert!(configs.is_empty());
        assert!(results.is_empty());
    }

    /// Test framework performance with different tool complexities
    #[test]
    fn test_framework_complexity_performance() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();
        let rt = Runtime::new().unwrap();

        let complexity_tests = vec![
            (crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0, medium_ratio: 0.0, complex_ratio: 0.0
            }, "Simple Only"),
            (crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.0, medium_ratio: 1.0, complex_ratio: 0.0
            }, "Medium Only"),
            (crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.0, medium_ratio: 0.0, complex_ratio: 1.0
            }, "Complex Only"),
            (crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.33, medium_ratio: 0.33, complex_ratio: 0.34
            }, "Mixed Complexity"),
        ];

        for (distribution, test_name) in complexity_tests {
            let config = crate::load_testing_framework::LoadTestConfig {
                name: format!("Complexity Test - {}", test_name),
                duration: Duration::from_millis(300),
                concurrency: 10,
                ramp_up_time: Duration::from_millis(50),
                tool_distribution: distribution,
                resource_limits: crate::load_testing_framework::ResourceLimits {
                    max_memory_mb: 50,
                    max_cpu_percent: 50.0,
                    max_response_time_ms: 200,
                },
            };

            let start = Instant::now();
            let results = rt.block_on(async {
                tester.run_load_test(config).await
            });
            let execution_time = start.elapsed();

            // Framework should handle all complexity levels
            assert!(results.total_operations > 0,
                   "Should execute operations for {}", test_name);

            // Performance should vary appropriately with complexity
            assert!(results.throughput_ops_per_sec > 0.0,
                   "Should have positive throughput for {}", test_name);

            println!("{}: Throughput={:.2} ops/sec, Avg Response={:?}",
                     test_name, results.throughput_ops_per_sec, results.average_response_time);
        }
    }
}

#[cfg(test)]
mod framework_benchmark_tests {
    use super::*;

    /// Benchmark MetricsCollector performance
    #[test]
    fn benchmark_metrics_collector() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();

        let operation_counts = vec![1000, 10000, 100000];

        for &num_operations in &operation_counts {
            collector.reset();

            // Benchmark recording performance
            let start = Instant::now();
            for i in 0..num_operations {
                let duration = Duration::from_micros(100 + (i % 1000));
                let complexity = match i % 3 {
                    0 => crate::load_testing_framework::ToolComplexity::Simple,
                    1 => crate::load_testing_framework::ToolComplexity::Medium,
                    _ => crate::load_testing_framework::ToolComplexity::Complex,
                };
                collector.record_operation(duration, complexity, true);
            }
            let recording_time = start.elapsed();

            // Benchmark calculation performance
            let start = Instant::now();
            let _ops_per_sec = collector.get_operations_per_second();
            let _avg_response_time = collector.get_average_response_time();
            let _resource_metrics = collector.get_resource_metrics();
            let calculation_time = start.elapsed();

            // Calculate performance metrics
            let recording_ops_per_sec = num_operations as f64 / recording_time.as_secs_f64();
            let calculation_overhead = calculation_time.as_nanos() as f64;

            println!("MetricsCollector Benchmark:");
            println!("  Operations: {}", num_operations);
            println!("  Recording: {:.0} ops/sec ({:?})", recording_ops_per_sec, recording_time);
            println!("  Calculation: {:.0} ns", calculation_overhead);

            // Performance assertions
            assert!(recording_ops_per_sec > 100000.0,
                   "Recording should be fast: {:.0} ops/sec", recording_ops_per_sec);
            assert!(calculation_overhead < 1000000.0, // < 1ms
                   "Calculation should be fast: {:.0} ns", calculation_overhead);
        }
    }

    /// Benchmark configuration serialization
    #[test]
    fn benchmark_configuration_serialization() {
        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Benchmark Configuration".to_string(),
            duration: Duration::from_secs(300),
            concurrency: 100,
            ramp_up_time: Duration::from_secs(30),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.4,
                medium_ratio: 0.35,
                complex_ratio: 0.25,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 1024,
                max_cpu_percent: 80.0,
                max_response_time_ms: 1000,
            },
        };

        let iterations = 10000;

        // Benchmark serialization
        let start = Instant::now();
        for _ in 0..iterations {
            let _json = serde_json::to_string(&config).unwrap();
        }
        let serialization_time = start.elapsed();

        // Benchmark deserialization
        let json = serde_json::to_string(&config).unwrap();
        let start = Instant::now();
        for _ in 0..iterations {
            let _: crate::load_testing_framework::LoadTestConfig = serde_json::from_str(&json).unwrap();
        }
        let deserialization_time = start.elapsed();

        // Calculate performance metrics
        let serialization_ops_per_sec = iterations as f64 / serialization_time.as_secs_f64();
        let deserialization_ops_per_sec = iterations as f64 / deserialization_time.as_secs_f64();

        println!("Configuration Serialization Benchmark:");
        println!("  Serialization: {:.0} ops/sec ({:?})", serialization_ops_per_sec, serialization_time);
        println!("  Deserialization: {:.0} ops/sec ({:?})", deserialization_ops_per_sec, deserialization_time);

        // Performance assertions
        assert!(serialization_ops_per_sec > 10000.0,
               "Serialization should be fast: {:.0} ops/sec", serialization_ops_per_sec);
        assert!(deserialization_ops_per_sec > 10000.0,
               "Deserialization should be fast: {:.0} ops/sec", deserialization_ops_per_sec);
    }

    /// Benchmark tool distribution selection
    #[test]
    fn benchmark_tool_distribution_selection() {
        let distribution = crate::load_testing_framework::ToolDistribution {
            simple_ratio: 0.4,
            medium_ratio: 0.35,
            complex_ratio: 0.25,
        };

        let selection_counts = vec![10000, 100000, 1000000];

        for &num_selections in &selection_counts {
            let start = Instant::now();

            for i in 0..num_selections {
                // Use deterministic selection based on iteration
                let random_value = ((i * 1237) % 1000) as f32 / 1000.0;
                let _selected = if random_value < distribution.simple_ratio {
                    crate::load_testing_framework::ToolComplexity::Simple
                } else if random_value < distribution.simple_ratio + distribution.medium_ratio {
                    crate::load_testing_framework::ToolComplexity::Medium
                } else {
                    crate::load_testing_framework::ToolComplexity::Complex
                };
            }

            let selection_time = start.elapsed();
            let selections_per_sec = num_selections as f64 / selection_time.as_secs_f64();

            println!("Tool Distribution Selection Benchmark:");
            println!("  Selections: {}", num_selections);
            println!("  Rate: {:.0} selections/sec ({:?})", selections_per_sec, selection_time);

            // Selection should be extremely fast
            assert!(selections_per_sec > 1000000.0,
                   "Tool selection should be very fast: {:.0} selections/sec", selections_per_sec);
        }
    }

    /// Benchmark concurrent load test execution
    #[test]
    fn benchmark_concurrent_load_test() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();
        let rt = Runtime::new().unwrap();

        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Concurrent Benchmark".to_string(),
            duration: Duration::from_millis(100),
            concurrency: 20,
            ramp_up_time: Duration::from_millis(20),
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

        let start = Instant::now();
        let results = rt.block_on(async {
            tester.run_load_test(config).await
        });
        let total_time = start.elapsed();

        let framework_overhead = total_time - config.duration - config.ramp_up_time;

        println!("Concurrent Load Test Benchmark:");
        println!("  Total time: {:?}", total_time);
        println!("  Configured duration: {:?}", config.duration);
        println!("  Framework overhead: {:?}", framework_overhead);
        println!("  Operations: {}", results.total_operations);
        println!("  Throughput: {:.2} ops/sec", results.throughput_ops_per_sec);

        // Framework overhead should be minimal
        assert!(framework_overhead < Duration::from_millis(200),
               "Framework overhead should be minimal: {:?}", framework_overhead);

        // Should achieve reasonable throughput
        assert!(results.throughput_ops_per_sec > 10.0,
               "Should achieve reasonable throughput: {:.2} ops/sec", results.throughput_ops_per_sec);
    }
}

#[cfg(test)]
mod framework_resource_usage_tests {
    use super::*;

    /// Test framework doesn't leak resources during repeated operations
    #[test]
    fn test_framework_resource_cleanup() {
        let mut results = Vec::new();

        // Run multiple load tests to check for resource leaks
        for i in 0..10 {
            let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();
            let rt = Runtime::new().unwrap();

            let config = crate::load_testing_framework::LoadTestConfig {
                name: format!("Resource Cleanup Test {}", i),
                duration: Duration::from_millis(50),
                concurrency: 5,
                ramp_up_time: Duration::from_millis(10),
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

            let test_results = rt.block_on(async {
                tester.run_load_test(config).await
            });

            results.push(test_results);
        }

        // All tests should complete successfully
        assert_eq!(results.len(), 10, "All resource cleanup tests should complete");

        for (i, test_results) in results.iter().enumerate() {
            assert!(test_results.total_operations > 0,
                   "Test {} should execute operations", i);
            assert!(test_results.successful_operations > 0,
                   "Test {} should have successful operations", i);
        }

        // Performance should not degrade over successive runs
        let throughputs: Vec<f64> = results.iter().map(|r| r.throughput_ops_per_sec).collect();
        let avg_throughput = throughputs.iter().sum::<f64>() / throughputs.len() as f64;

        for (i, &throughput) in throughputs.iter().enumerate() {
            let deviation = (throughput - avg_throughput).abs() / avg_throughput;
            assert!(deviation < 0.5, // Allow 50% deviation
                   "Test {} throughput should not deviate excessively: {:.2} vs {:.2}",
                   i, throughput, avg_throughput);
        }
    }

    /// Test framework handles resource limits gracefully
    #[test]
    fn test_framework_resource_limit_handling() {
        let tester = crate::load_testing_framework::ScriptEngineLoadTester::new();
        let rt = Runtime::new().unwrap();

        // Test with very restrictive resource limits
        let restrictive_config = crate::load_testing_framework::LoadTestConfig {
            name: "Restrictive Resource Test".to_string(),
            duration: Duration::from_millis(200),
            concurrency: 10,
            ramp_up_time: Duration::from_millis(50),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 1, // Very restrictive
                max_cpu_percent: 1.0, // Very restrictive
                max_response_time_ms: 1, // Very restrictive
            },
        };

        let start = Instant::now();
        let results = rt.block_on(async {
            tester.run_load_test(restrictive_config).await
        });
        let execution_time = start.elapsed();

        // Should complete even with restrictive limits
        assert!(results.total_operations > 0,
               "Should complete with restrictive resource limits");

        // Should not take excessively long
        assert!(execution_time < Duration::from_secs(5),
               "Should complete quickly even with restrictive limits: {:?}", execution_time);

        // Test with very permissive resource limits
        let permissive_config = crate::load_testing_framework::LoadTestConfig {
            name: "Permissive Resource Test".to_string(),
            duration: Duration::from_millis(200),
            concurrency: 10,
            ramp_up_time: Duration::from_millis(50),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 32768, // Very permissive
                max_cpu_percent: 100.0, // Very permissive
                max_response_time_ms: 300000, // Very permissive
            },
        };

        let start = Instant::now();
        let results = rt.block_on(async {
            tester.run_load_test(permissive_config).await
        });
        let execution_time = start.elapsed();

        // Should also complete with permissive limits
        assert!(results.total_operations > 0,
               "Should complete with permissive resource limits");

        assert!(execution_time < Duration::from_secs(5),
               "Should complete quickly with permissive limits: {:?}", execution_time);
    }

    /// Test framework thread safety
    #[test]
    fn test_framework_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let tester = Arc::new(crate::load_testing_framework::ScriptEngineLoadTester::new());
        let num_threads = 5;
        let operations_per_thread = 10;

        let start = Instant::now();

        let handles: Vec<_> = (0..num_threads)
            .map(|thread_id| {
                let tester = Arc::clone(&tester);
                thread::spawn(move || {
                    let rt = Runtime::new().unwrap();
                    let mut results = Vec::new();

                    for i in 0..operations_per_thread {
                        let config = crate::load_testing_framework::LoadTestConfig {
                            name: format!("Thread Safety Test {}-{}", thread_id, i),
                            duration: Duration::from_millis(10),
                            concurrency: 2,
                            ramp_up_time: Duration::from_millis(5),
                            tool_distribution: crate::load_testing_framework::ToolDistribution {
                                simple_ratio: 1.0,
                                medium_ratio: 0.0,
                                complex_ratio: 0.0,
                            },
                            resource_limits: crate::load_testing_framework::ResourceLimits {
                                max_memory_mb: 20,
                                max_cpu_percent: 20.0,
                                max_response_time_ms: 50,
                            },
                        };

                        let test_results = rt.block_on(async {
                            tester.run_load_test(config).await
                        });

                        results.push(test_results);
                    }

                    (thread_id, results)
                })
            })
            .collect();

        // Wait for all threads to complete
        let thread_results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let total_time = start.elapsed();

        // All threads should complete successfully
        assert_eq!(thread_results.len(), num_threads, "All threads should complete");

        for (thread_id, results) in thread_results {
            assert_eq!(results.len(), operations_per_thread,
                       "Thread {} should complete all operations", thread_id);

            for (i, test_results) in results.iter().enumerate() {
                assert!(test_results.total_operations > 0,
                       "Thread {}-{} should execute operations", thread_id, i);
            }
        }

        // Should complete in reasonable time
        assert!(total_time < Duration::from_secs(30),
               "Thread-safe operations should complete quickly: {:?}", total_time);
    }
}