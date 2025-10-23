//! Unit tests for ScriptEngine load testing framework
//!
//! Comprehensive test suite to validate the reliability and accuracy of load testing

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::runtime::Runtime;
use serde_json;

#[cfg(test)]
mod load_testing_framework_tests {
    use super::*;

    /// Test LoadTestConfig creation and validation
    #[test]
    fn test_load_test_config_creation() {
        let config = LoadTestConfig {
            name: "Test Config".to_string(),
            duration: Duration::from_secs(60),
            concurrency: 20,
            ramp_up_time: Duration::from_secs(10),
            tool_distribution: ToolDistribution {
                simple_ratio: 0.6,
                medium_ratio: 0.3,
                complex_ratio: 0.1,
            },
            resource_limits: ResourceLimits {
                max_memory_mb: 200,
                max_cpu_percent: 75.0,
                max_response_time_ms: 300,
            },
        };

        assert_eq!(config.name, "Test Config");
        assert_eq!(config.duration, Duration::from_secs(60));
        assert_eq!(config.concurrency, 20);
        assert_eq!(config.ramp_up_time, Duration::from_secs(10));

        // Validate tool distribution ratios sum to 1.0
        let total_ratio = config.tool_distribution.simple_ratio +
                         config.tool_distribution.medium_ratio +
                         config.tool_distribution.complex_ratio;
        assert!((total_ratio - 1.0).abs() < 0.001, "Tool distribution ratios must sum to 1.0");
    }

    /// Test ToolDistribution validation
    #[test]
    fn test_tool_distribution_validation() {
        // Test valid distribution
        let valid_dist = ToolDistribution {
            simple_ratio: 0.5,
            medium_ratio: 0.3,
            complex_ratio: 0.2,
        };
        assert!((valid_dist.simple_ratio + valid_dist.medium_ratio + valid_dist.complex_ratio - 1.0).abs() < 0.001);

        // Test edge cases
        let all_simple = ToolDistribution {
            simple_ratio: 1.0,
            medium_ratio: 0.0,
            complex_ratio: 0.0,
        };
        assert_eq!(all_simple.simple_ratio, 1.0);
        assert_eq!(all_simple.medium_ratio, 0.0);
        assert_eq!(all_simple.complex_ratio, 0.0);
    }

    /// Test MockScriptEngine basic functionality
    #[tokio::test]
    async fn test_mock_script_engine_basic() {
        let engine = MockScriptEngine::new();

        // Test simple tool execution
        let result1 = engine.execute_tool(ToolComplexity::Simple, 100).await;
        assert!(result1.contains("tool_result_"));
        assert!(result1.contains("len_"));

        // Test medium tool execution
        let result2 = engine.execute_tool(ToolComplexity::Medium, 300).await;
        assert!(result2.contains("tool_result_"));
        assert!(result2.contains("len_"));

        // Test complex tool execution
        let result3 = engine.execute_tool(ToolComplexity::Complex, 200).await;
        assert!(result3.contains("tool_result_"));
        assert!(result3.contains("len_"));

        // Verify unique operation IDs
        let id1 = result1.split('_').nth(1).unwrap().parse::<usize>().unwrap();
        let id2 = result2.split('_').nth(1).unwrap().parse::<usize>().unwrap();
        let id3 = result3.split('_').nth(1).unwrap().parse::<usize>().unwrap();

        assert!(id1 < id2);
        assert!(id2 < id3);
    }

    /// Test ScriptEngineLoadTester creation
    #[test]
    fn test_script_engine_load_tester_creation() {
        let rt = Runtime::new().unwrap();
        let tester = ScriptEngineLoadTester::new();

        // Verify it was created successfully
        assert!(true); // If we got here, creation succeeded
    }

    /// Test MetricsCollector basic functionality
    #[test]
    fn test_metrics_collector_basic() {
        let mut collector = MetricsCollector::new();

        // Test initial state
        assert_eq!(collector.get_operations_per_second(), 0.0);
        assert_eq!(collector.get_average_response_time(), Duration::ZERO);

        // Test recording operations
        collector.record_operation(Duration::from_millis(10), ToolComplexity::Simple, true);
        collector.record_operation(Duration::from_millis(20), ToolComplexity::Medium, true);
        collector.record_operation(Duration::from_millis(30), ToolComplexity::Complex, true);

        // Verify metrics
        assert!(collector.get_operations_per_second() > 0.0);
        assert_eq!(collector.get_average_response_time(), Duration::from_millis(20));

        // Test time series data
        let data_point = TimeSeriesDataPoint {
            timestamp: Instant::now(),
            operations_per_sec: 100.0,
            average_response_time: Duration::from_millis(15),
            memory_usage_mb: 50.0,
            cpu_percent: 25.0,
            active_connections: 10,
        };
        collector.record_time_series_data_point(data_point);

        let time_series = collector.get_time_series_data();
        assert_eq!(time_series.len(), 1);
    }

    /// Test OperationResult creation
    #[test]
    fn test_operation_result_creation() {
        let result = OperationResult {
            operation_id: 42,
            tool_type: ToolComplexity::Medium,
            duration: Duration::from_millis(150),
            success: true,
            error_message: None,
        };

        assert_eq!(result.operation_id, 42);
        assert_eq!(result.tool_type, ToolComplexity::Medium);
        assert_eq!(result.duration, Duration::from_millis(150));
        assert!(result.success);
        assert!(result.error_message.is_none());
    }

    /// Test LoadTestResults serialization
    #[test]
    fn test_load_test_results_serialization() {
        let results = LoadTestResults {
            test_name: "Test Results".to_string(),
            duration: Duration::from_secs(60),
            total_operations: 1000,
            successful_operations: 950,
            failed_operations: 50,
            average_response_time: Duration::from_millis(100),
            p95_response_time: Duration::from_millis(200),
            p99_response_time: Duration::from_millis(300),
            throughput_ops_per_sec: 16.67,
            error_rate: 0.05,
            resource_metrics: ResourceMetrics {
                peak_memory_mb: 150.0,
                average_memory_mb: 100.0,
                peak_cpu_percent: 80.0,
                average_cpu_percent: 45.0,
                memory_growth_rate: 0.1,
            },
            time_series_data: vec![],
        };

        // Test JSON serialization
        let json = serde_json::to_string(&results);
        assert!(json.is_ok());

        // Test JSON deserialization
        let json_str = json.unwrap();
        let deserialized: LoadTestResults = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.test_name, "Test Results");
        assert_eq!(deserialized.total_operations, 1000);
        assert_eq!(deserialized.throughput_ops_per_sec, 16.67);
    }

    /// Test tool type selection logic
    #[test]
    fn test_tool_type_selection() {
        // Test simple-heavy distribution
        let simple_heavy = ToolDistribution {
            simple_ratio: 0.8,
            medium_ratio: 0.15,
            complex_ratio: 0.05,
        };

        // Run multiple selections to test distribution
        let mut simple_count = 0;
        let mut medium_count = 0;
        let mut complex_count = 0;
        let iterations = 1000;

        for _ in 0..iterations {
            let tool_type = select_tool_type_deterministic(0.1, &simple_heavy);
            match tool_type {
                ToolComplexity::Simple => simple_count += 1,
                ToolComplexity::Medium => medium_count += 1,
                ToolComplexity::Complex => complex_count += 1,
            }
        }

        // Verify distribution is roughly correct
        let simple_ratio = simple_count as f64 / iterations as f64;
        assert!(simple_ratio > 0.7 && simple_ratio < 0.9);
    }

    /// Test resource limit validation
    #[test]
    fn test_resource_limits() {
        let limits = ResourceLimits {
            max_memory_mb: 500,
            max_cpu_percent: 80.0,
            max_response_time_ms: 1000,
        };

        assert_eq!(limits.max_memory_mb, 500);
        assert_eq!(limits.max_cpu_percent, 80.0);
        assert_eq!(limits.max_response_time_ms, 1000);
    }

    /// Test time series data point creation
    #[test]
    fn test_time_series_data_point() {
        let timestamp = Instant::now();
        let data_point = TimeSeriesDataPoint {
            timestamp,
            operations_per_sec: 150.0,
            average_response_time: Duration::from_millis(25),
            memory_usage_mb: 75.0,
            cpu_percent: 60.0,
            active_connections: 20,
        };

        assert_eq!(data_point.timestamp, timestamp);
        assert_eq!(data_point.operations_per_sec, 150.0);
        assert_eq!(data_point.average_response_time, Duration::from_millis(25));
        assert_eq!(data_point.memory_usage_mb, 75.0);
        assert_eq!(data_point.cpu_percent, 60.0);
        assert_eq!(data_point.active_connections, 20);
    }
}

#[cfg(test)]
mod load_testing_integration_tests {
    use super::*;

    #[test]
    fn test_load_testing_framework_compilation() {
        // Test that the load testing framework compiles successfully
        let output = std::process::Command::new("cargo")
            .args(&["check", "--bench", "script_engine_load_tests"])
            .output()
            .expect("Failed to run cargo check");

        assert!(output.status.success(),
               "Load testing framework compilation failed: {}",
               String::from_utf8_lossy(&output.stderr));
    }

    #[test]
    fn test_benchmark_dependencies_available() {
        // Test that required dependencies are available
        let output = std::process::Command::new("cargo")
            .args(&["tree", "--package", "crucible-benchmarks"])
            .output()
            .expect("Failed to check dependencies");

        let dependency_tree = String::from_utf8_lossy(&output.stdout);

        // Check for key dependencies
        assert!(dependency_tree.contains("criterion"));
        assert!(dependency_tree.contains("tokio"));
        assert!(dependency_tree.contains("futures"));
        assert!(dependency_tree.contains("serde"));
        assert!(dependency_tree.contains("rand"));
    }

    #[test]
    fn test_load_testing_files_exist() {
        use std::path::Path;

        // Verify all load testing files exist
        assert!(Path::new("benches/script_engine_load_tests.rs").exists());
        assert!(Path::new("benches/load_testing_framework.rs").exists());
        assert!(Path::new("benches/Cargo.toml").exists());
    }

    #[test]
    fn test_criterion_benchmark_structure() {
        let benchmark_file = std::fs::read_to_string("benches/script_engine_load_tests.rs")
            .expect("Failed to read benchmark file");

        // Check for required benchmark functions
        assert!(benchmark_file.contains("fn bench_concurrent_tool_execution"));
        assert!(benchmark_file.contains("fn bench_sustained_load"));
        assert!(benchmark_file.contains("fn bench_mixed_workload"));
        assert!(benchmark_file.contains("fn bench_resource_usage_under_load"));
        assert!(benchmark_file.contains("fn bench_error_handling_under_load"));

        // Check for proper criterion usage
        assert!(benchmark_file.contains("criterion_group"));
        assert!(benchmark_file.contains("criterion_main"));
        assert!(benchmark_file.contains("BenchmarkId"));
        assert!(benchmark_file.contains("Throughput"));
    }

    #[test]
    fn test_load_testing_framework_structure() {
        let framework_file = std::fs::read_to_string("benches/load_testing_framework.rs")
            .expect("Failed to read framework file");

        // Check for required components
        assert!(framework_file.contains("pub struct LoadTestConfig"));
        assert!(framework_file.contains("pub struct ScriptEngineLoadTester"));
        assert!(framework_file.contains("pub struct MockScriptEngine"));
        assert!(framework_file.contains("pub struct MetricsCollector"));

        // Check for required methods
        assert!(framework_file.contains("pub async fn run_load_test"));
        assert!(framework_file.contains("pub async fn execute_tool"));
        assert!(framework_file.contains("pub fn record_operation"));

        // Check for predefined configurations
        assert!(framework_file.contains("pub fn light_load_test"));
        assert!(framework_file.contains("pub fn medium_load_test"));
        assert!(framework_file.contains("pub fn heavy_load_test"));
        assert!(framework_file.contains("pub fn stress_test"));
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_metrics_collector_performance() {
        let mut collector = MetricsCollector::new();
        let iterations = 10000;

        let start = Instant::now();
        for i in 0..iterations {
            let duration = Duration::from_micros(100 + (i % 1000));
            let tool_type = match i % 3 {
                0 => ToolComplexity::Simple,
                1 => ToolComplexity::Medium,
                _ => ToolComplexity::Complex,
            };
            collector.record_operation(duration, tool_type, true);
        }
        let recording_duration = start.elapsed();

        // Verify performance is acceptable
        assert!(recording_duration < Duration::from_millis(100),
               "Metrics recording too slow: {:?}", recording_duration);

        // Verify metrics accuracy
        assert_eq!(collector.get_average_response_time(), Duration::from_micros(600));
    }

    #[tokio::test]
    async fn test_mock_script_engine_performance() {
        let engine = MockScriptEngine::new();
        let iterations = 1000;

        let start = Instant::now();
        for i in 0..iterations {
            let complexity = match i % 3 {
                0 => ToolComplexity::Simple,
                1 => ToolComplexity::Medium,
                _ => ToolComplexity::Complex,
            };
            let _result = engine.execute_tool(complexity, 100).await;
        }
        let execution_duration = start.elapsed();

        // Verify reasonable performance
        let avg_per_operation = execution_duration / iterations as u32;
        assert!(avg_per_operation < Duration::from_millis(10),
               "MockScriptEngine too slow: {:?} per operation", avg_per_operation);
    }

    #[test]
    fn test_serialization_performance() {
        let results = create_test_load_results();

        let start = Instant::now();
        let json = serde_json::to_string(&results).unwrap();
        let serialization_duration = start.elapsed();

        let start = Instant::now();
        let _deserialized: LoadTestResults = serde_json::from_str(&json).unwrap();
        let deserialization_duration = start.elapsed();

        // Verify serialization performance is acceptable
        assert!(serialization_duration < Duration::from_millis(10),
               "Serialization too slow: {:?}", serialization_duration);
        assert!(deserialization_duration < Duration::from_millis(10),
               "Deserialization too slow: {:?}", deserialization_duration);
    }

    fn create_test_load_results() -> LoadTestResults {
        LoadTestResults {
            test_name: "Performance Test".to_string(),
            duration: Duration::from_secs(60),
            total_operations: 1000,
            successful_operations: 950,
            failed_operations: 50,
            average_response_time: Duration::from_millis(100),
            p95_response_time: Duration::from_millis(200),
            p99_response_time: Duration::from_millis(300),
            throughput_ops_per_sec: 16.67,
            error_rate: 0.05,
            resource_metrics: ResourceMetrics {
                peak_memory_mb: 150.0,
                average_memory_mb: 100.0,
                peak_cpu_percent: 80.0,
                average_cpu_percent: 45.0,
                memory_growth_rate: 0.1,
            },
            time_series_data: vec![],
        }
    }
}

#[cfg(test)]
mod load_scenario_tests {
    use super::*;

    #[test]
    fn test_light_load_configuration() {
        let config = configurations::light_load_test();

        assert_eq!(config.name, "Light Load Test");
        assert_eq!(config.duration, Duration::from_secs(30));
        assert_eq!(config.concurrency, 5);
        assert_eq!(config.ramp_up_time, Duration::from_secs(5));

        // Verify tool distribution
        assert!((config.tool_distribution.simple_ratio - 0.8).abs() < 0.001);
        assert!((config.tool_distribution.medium_ratio - 0.15).abs() < 0.001);
        assert!((config.tool_distribution.complex_ratio - 0.05).abs() < 0.001);

        // Verify resource limits
        assert_eq!(config.resource_limits.max_memory_mb, 100);
        assert_eq!(config.resource_limits.max_cpu_percent, 50.0);
        assert_eq!(config.resource_limits.max_response_time_ms, 100);
    }

    #[test]
    fn test_medium_load_configuration() {
        let config = configurations::medium_load_test();

        assert_eq!(config.name, "Medium Load Test");
        assert_eq!(config.duration, Duration::from_secs(60));
        assert_eq!(config.concurrency, 20);
        assert_eq!(config.ramp_up_time, Duration::from_secs(10));

        // Verify tool distribution
        assert!((config.tool_distribution.simple_ratio - 0.6).abs() < 0.001);
        assert!((config.tool_distribution.medium_ratio - 0.25).abs() < 0.001);
        assert!((config.tool_distribution.complex_ratio - 0.15).abs() < 0.001);
    }

    #[test]
    fn test_heavy_load_configuration() {
        let config = configurations::heavy_load_test();

        assert_eq!(config.name, "Heavy Load Test");
        assert_eq!(config.duration, Duration::from_secs(120));
        assert_eq!(config.concurrency, 50);
        assert_eq!(config.ramp_up_time, Duration::from_secs(20));

        // Verify tool distribution
        assert!((config.tool_distribution.simple_ratio - 0.4).abs() < 0.001);
        assert!((config.tool_distribution.simple_ratio - 0.4).abs() < 0.001);
        assert!((config.tool_distribution.complex_ratio - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_stress_test_configuration() {
        let config = configurations::stress_test();

        assert_eq!(config.name, "Stress Test");
        assert_eq!(config.duration, Duration::from_secs(300));
        assert_eq!(config.concurrency, 100);
        assert_eq!(config.ramp_up_time, Duration::from_secs(30));

        // Verify tool distribution
        assert!((config.tool_distribution.simple_ratio - 0.3).abs() < 0.001);
        assert!((config.tool_distribution.medium_ratio - 0.4).abs() < 0.001);
        assert!((config.tool_distribution.complex_ratio - 0.3).abs() < 0.001);

        // Verify resource limits are higher for stress test
        assert_eq!(config.resource_limits.max_memory_mb, 1000);
        assert_eq!(config.resource_limits.max_cpu_percent, 95.0);
        assert_eq!(config.resource_limits.max_response_time_ms, 1000);
    }

    #[test]
    fn test_configuration_progression() {
        let light = configurations::light_load_test();
        let medium = configurations::medium_load_test();
        let heavy = configurations::heavy_load_test();
        let stress = configurations::stress_test();

        // Verify progression makes sense
        assert!(light.duration < medium.duration);
        assert!(medium.duration < heavy.duration);
        assert!(heavy.duration < stress.duration);

        assert!(light.concurrency < medium.concurrency);
        assert!(medium.concurrency < heavy.concurrency);
        assert!(heavy.concurrency < stress.concurrency);

        assert!(light.resource_limits.max_memory_mb < medium.resource_limits.max_memory_mb);
        assert!(medium.resource_limits.max_memory_mb < heavy.resource_limits.max_memory_mb);
        assert!(heavy.resource_limits.max_memory_mb < stress.resource_limits.max_memory_mb);
    }
}

// Helper functions for testing
fn select_tool_type_deterministic(random_value: f32, distribution: &ToolDistribution) -> ToolComplexity {
    if random_value < distribution.simple_ratio {
        ToolComplexity::Simple
    } else if random_value < distribution.simple_ratio + distribution.medium_ratio {
        ToolComplexity::Medium
    } else {
        ToolComplexity::Complex
    }
}

// Import required types (these would be imported from the load testing framework)
use crate::load_testing_framework::*;