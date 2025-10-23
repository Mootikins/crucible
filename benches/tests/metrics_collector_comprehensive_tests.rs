//! Comprehensive unit tests for MetricsCollector accuracy and performance
//!
//! Specialized testing for metrics collection ensuring accurate tracking,
//! statistical calculations, and high-performance measurement

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
mod metrics_collector_accuracy_tests {
    use super::*;

    /// Test MetricsCollector initialization and reset
    #[test]
    fn test_metrics_collector_initialization() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();

        // Initial state should be empty
        assert_eq!(collector.get_operations_per_second(), 0.0,
                   "Initial operations per second should be 0");
        assert_eq!(collector.get_average_response_time(), Duration::ZERO,
                   "Initial average response time should be zero");

        // Reset should maintain clean state
        collector.reset();
        assert_eq!(collector.get_operations_per_second(), 0.0,
                   "Reset operations per second should be 0");
        assert_eq!(collector.get_average_response_time(), Duration::ZERO,
                   "Reset average response time should be zero");
    }

    /// Test single operation recording
    #[test]
    fn test_single_operation_recording() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        // Record one operation
        collector.record_operation(
            Duration::from_millis(100),
            crate::load_testing_framework::ToolComplexity::Simple,
            true
        );

        // Should have positive operations per second
        let ops_per_sec = collector.get_operations_per_second();
        assert!(ops_per_sec > 0.0,
               "Should have positive ops/sec after recording operation");

        // Average response time should match recorded value
        assert_eq!(collector.get_average_response_time(), Duration::from_millis(100),
                   "Average response time should match recorded operation");
    }

    /// Test multiple operation recording with varying durations
    #[test]
    fn test_multiple_operation_recording() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        let durations = vec![
            Duration::from_millis(50),
            Duration::from_millis(100),
            Duration::from_millis(150),
            Duration::from_millis(200),
            Duration::from_millis(250),
        ];

        let complexities = vec![
            crate::load_testing_framework::ToolComplexity::Simple,
            crate::load_testing_framework::ToolComplexity::Medium,
            crate::load_testing_framework::ToolComplexity::Complex,
        ];

        // Record multiple operations
        for (i, &duration) in durations.iter().enumerate() {
            let complexity = complexities[i % complexities.len()];
            collector.record_operation(duration, complexity, true);
        }

        // Verify average response time calculation
        let expected_average = durations.iter().sum::<Duration>() / durations.len() as u32;
        assert_eq!(collector.get_average_response_time(), expected_average,
                   "Average response time should be calculated correctly");

        // Should have positive operations per second
        assert!(collector.get_operations_per_second() > 0.0,
               "Should have positive ops/sec with multiple operations");
    }

    /// Test operations per second calculation accuracy
    #[test]
    fn test_operations_per_second_calculation() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        // Record operations immediately after reset
        for i in 0..10 {
            collector.record_operation(
                Duration::from_millis(10 + i * 10),
                crate::load_testing_framework::ToolComplexity::Simple,
                true
            );
        }

        let ops_per_sec = collector.get_operations_per_second();
        assert!(ops_per_sec > 0.0,
               "Should calculate positive ops/sec");

        // The exact value depends on timing, but should be reasonable
        assert!(ops_per_sec < 10000.0,
               "Ops/sec should be reasonable, not excessively high: {}", ops_per_sec);
    }

    /// Test time series data recording
    #[test]
    fn test_time_series_data_recording() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        // Record multiple time series data points
        let data_points = vec![
            crate::load_testing_framework::TimeSeriesDataPoint {
                timestamp: Instant::now(),
                operations_per_sec: 100.0,
                average_response_time: Duration::from_millis(50),
                memory_usage_mb: 25.0,
                cpu_percent: 30.0,
                active_connections: 5,
            },
            crate::load_testing_framework::TimeSeriesDataPoint {
                timestamp: Instant::now(),
                operations_per_sec: 150.0,
                average_response_time: Duration::from_millis(75),
                memory_usage_mb: 30.0,
                cpu_percent: 45.0,
                active_connections: 8,
            },
            crate::load_testing_framework::TimeSeriesDataPoint {
                timestamp: Instant::now(),
                operations_per_sec: 120.0,
                average_response_time: Duration::from_millis(60),
                memory_usage_mb: 28.0,
                cpu_percent: 35.0,
                active_connections: 6,
            },
        ];

        for data_point in data_points {
            collector.record_time_series_data_point(data_point);
        }

        let retrieved_data = collector.get_time_series_data();
        assert_eq!(retrieved_data.len(), 3,
                   "Should retrieve all recorded time series data");

        // Verify data integrity
        for (i, data_point) in retrieved_data.iter().enumerate() {
            assert!(data_point.operations_per_sec > 0.0,
                   "Time series point {} should have positive ops/sec", i);
            assert!(data_point.memory_usage_mb > 0.0,
                   "Time series point {} should have positive memory usage", i);
            assert!(data_point.cpu_percent >= 0.0,
                   "Time series point {} should have non-negative CPU usage", i);
            assert!(data_point.active_connections > 0,
                   "Time series point {} should have positive connections", i);
        }
    }

    /// Test resource metrics generation
    #[test]
    fn test_resource_metrics_generation() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        // Record some operations
        for i in 0..20 {
            collector.record_operation(
                Duration::from_millis(10 + i * 5),
                crate::load_testing_framework::ToolComplexity::Simple,
                true
            );
        }

        // Record time series data
        collector.record_time_series_data_point(crate::load_testing_framework::TimeSeriesDataPoint {
            timestamp: Instant::now(),
            operations_per_sec: 100.0,
            average_response_time: Duration::from_millis(50),
            memory_usage_mb: 50.0,
            cpu_percent: 60.0,
            active_connections: 10,
        });

        let resource_metrics = collector.get_resource_metrics();

        // Verify resource metrics are reasonable
        assert!(resource_metrics.peak_memory_mb > 0.0,
               "Should have positive peak memory");
        assert!(resource_metrics.average_memory_mb > 0.0,
               "Should have positive average memory");
        assert!(resource_metrics.peak_cpu_percent >= 0.0,
               "Should have non-negative peak CPU");
        assert!(resource_metrics.average_cpu_percent >= 0.0,
               "Should have non-negative average CPU");
        assert!(resource_metrics.memory_growth_rate >= 0.0,
               "Should have non-negative memory growth rate");

        // Peak should be >= average
        assert!(resource_metrics.peak_memory_mb >= resource_metrics.average_memory_mb,
               "Peak memory should be >= average memory");
        assert!(resource_metrics.peak_cpu_percent >= resource_metrics.average_cpu_percent,
               "Peak CPU should be >= average CPU");
    }

    /// Test metrics reset functionality
    #[test]
    fn test_metrics_reset() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();

        // Record some data
        collector.record_operation(
            Duration::from_millis(100),
            crate::load_testing_framework::ToolComplexity::Simple,
            true
        );

        collector.record_time_series_data_point(crate::load_testing_framework::TimeSeriesDataPoint {
            timestamp: Instant::now(),
            operations_per_sec: 100.0,
            average_response_time: Duration::from_millis(50),
            memory_usage_mb: 25.0,
            cpu_percent: 30.0,
            active_connections: 5,
        });

        // Verify data exists
        assert!(collector.get_operations_per_second() > 0.0,
               "Should have data before reset");
        assert!(!collector.get_time_series_data().is_empty(),
               "Should have time series data before reset");

        // Reset and verify data is cleared
        collector.reset();

        assert_eq!(collector.get_average_response_time(), Duration::ZERO,
                   "Average response time should be zero after reset");

        let time_series_data = collector.get_time_series_data();
        assert!(time_series_data.is_empty() || time_series_data.len() == 0,
               "Time series data should be cleared after reset");
    }

    /// Test metrics with different tool complexities
    #[test]
    fn test_metrics_with_different_complexities() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        let complexities = vec![
            crate::load_testing_framework::ToolComplexity::Simple,
            crate::load_testing_framework::ToolComplexity::Medium,
            crate::load_testing_framework::ToolComplexity::Complex,
        ];

        let durations = vec![
            Duration::from_millis(10),  // Simple
            Duration::from_millis(50),  // Medium
            Duration::from_millis(100), // Complex
        ];

        // Record operations with different complexities
        for (complexity, duration) in complexities.iter().zip(durations.iter()) {
            collector.record_operation(*duration, *complexity, true);
        }

        // Average should account for all complexities
        let expected_average = durations.iter().sum::<Duration>() / durations.len() as u32;
        assert_eq!(collector.get_average_response_time(), expected_average,
                   "Average should account for different complexities");

        // Should have positive operations per second
        assert!(collector.get_operations_per_second() > 0.0,
               "Should calculate ops/sec for mixed complexity operations");
    }

    /// Test metrics with failed operations
    #[test]
    fn test_metrics_with_failed_operations() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        // Record both successful and failed operations
        collector.record_operation(
            Duration::from_millis(50),
            crate::load_testing_framework::ToolComplexity::Simple,
            true
        );

        collector.record_operation(
            Duration::from_millis(100),
            crate::load_testing_framework::ToolComplexity::Medium,
            false
        );

        collector.record_operation(
            Duration::from_millis(75),
            crate::load_testing_framework::ToolComplexity::Simple,
            true
        );

        // Should calculate average including failed operations
        let expected_average = Duration::from_millis((50 + 100 + 75) / 3);
        assert_eq!(collector.get_average_response_time(), expected_average,
                   "Average should include both successful and failed operations");

        // Should have positive operations per second for all operations
        assert!(collector.get_operations_per_second() > 0.0,
               "Should count all operations including failures");
    }

    /// Test metrics calculation under high frequency operations
    #[test]
    fn test_high_frequency_operations() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        let num_operations = 1000;

        // Record many operations quickly
        for i in 0..num_operations {
            collector.record_operation(
                Duration::from_micros(100 + (i % 1000)),
                crate::load_testing_framework::ToolComplexity::Simple,
                true
            );
        }

        // Should handle high frequency gracefully
        assert!(collector.get_operations_per_second() > 0.0,
               "Should handle high frequency operations");

        let avg_response_time = collector.get_average_response_time();
        assert!(avg_response_time > Duration::ZERO,
               "Should calculate average for high frequency operations");
        assert!(avg_response_time < Duration::from_millis(10),
               "Average should be reasonable for high frequency operations");

        // Should still collect resource metrics
        let resource_metrics = collector.get_resource_metrics();
        assert!(resource_metrics.peak_memory_mb > 0.0,
               "Should collect resource metrics under high frequency");
    }
}

#[cfg(test)]
mod metrics_collector_performance_tests {
    use super::*;

    /// Test MetricsCollector performance with large datasets
    #[test]
    fn test_metrics_collector_performance() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        let num_operations = 10000;

        // Test recording performance
        let start = Instant::now();
        for i in 0..num_operations {
            let duration = Duration::from_micros(100 + (i % 1000));
            let complexity = match i % 3 {
                0 => crate::load_testing_framework::ToolComplexity::Simple,
                1 => crate::load_testing_framework::ToolComplexity::Medium,
                _ => crate::load_testing_framework::ToolComplexity::Complex,
            };
            collector.record_operation(duration, complexity, i % 10 != 0); // 10% failure rate
        }
        let recording_duration = start.elapsed();

        // Verify recording performance is acceptable
        assert!(recording_duration < Duration::from_millis(100),
               "Recording {} operations should take < 100ms, took {:?}",
               num_operations, recording_duration);

        // Test calculation performance
        let start = Instant::now();
        let ops_per_sec = collector.get_operations_per_second();
        let avg_response_time = collector.get_average_response_time();
        let resource_metrics = collector.get_resource_metrics();
        let calculation_duration = start.elapsed();

        // Verify calculation performance is acceptable
        assert!(calculation_duration < Duration::from_millis(10),
               "Calculations should take < 10ms, took {:?}", calculation_duration);

        // Verify results are reasonable
        assert!(ops_per_sec > 0.0, "Should calculate positive ops/sec");
        assert!(avg_response_time > Duration::ZERO, "Should calculate positive avg response time");
        assert!(resource_metrics.peak_memory_mb > 0.0, "Should calculate resource metrics");
    }

    /// Test time series data collection performance
    #[test]
    fn test_time_series_performance() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        let num_data_points = 1000;

        // Test time series recording performance
        let start = Instant::now();
        for i in 0..num_data_points {
            let data_point = crate::load_testing_framework::TimeSeriesDataPoint {
                timestamp: Instant::now(),
                operations_per_sec: 100.0 + (i as f64 * 0.1),
                average_response_time: Duration::from_millis(50 + (i % 100)),
                memory_usage_mb: 50.0 + (i as f64 * 0.01),
                cpu_percent: 30.0 + (i as f64 * 0.05),
                active_connections: 5 + (i % 20),
            };
            collector.record_time_series_data_point(data_point);
        }
        let recording_duration = start.elapsed();

        // Verify recording performance
        assert!(recording_duration < Duration::from_millis(50),
               "Recording {} time series points should take < 50ms, took {:?}",
               num_data_points, recording_duration);

        // Test retrieval performance
        let start = Instant::now();
        let retrieved_data = collector.get_time_series_data();
        let retrieval_duration = start.elapsed();

        // Verify retrieval performance
        assert!(retrieval_duration < Duration::from_millis(10),
               "Retrieving {} time series points should take < 10ms, took {:?}",
               num_data_points, retrieval_duration);

        assert_eq!(retrieved_data.len(), num_data_points,
                   "Should retrieve all recorded time series data");
    }

    /// Test memory usage efficiency
    #[test]
    fn test_memory_usage_efficiency() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        // Record a large dataset
        let num_operations = 50000;
        for i in 0..num_operations {
            collector.record_operation(
                Duration::from_micros(100 + (i % 10000)),
                crate::load_testing_framework::ToolComplexity::Simple,
                true
            );
        }

        // Record time series data
        let num_time_series = 1000;
        for i in 0..num_time_series {
            collector.record_time_series_data_point(crate::load_testing_framework::TimeSeriesDataPoint {
                timestamp: Instant::now(),
                operations_per_sec: 100.0 + i as f64,
                average_response_time: Duration::from_millis(50 + i),
                memory_usage_mb: 50.0 + i as f64 * 0.1,
                cpu_percent: 30.0 + i as f64 * 0.05,
                active_connections: 5 + i % 10,
            });
        }

        // Calculations should still be fast with large datasets
        let start = Instant::now();
        let _ops_per_sec = collector.get_operations_per_second();
        let _avg_response_time = collector.get_average_response_time();
        let _resource_metrics = collector.get_resource_metrics();
        let _time_series = collector.get_time_series_data();
        let calculation_duration = start.elapsed();

        assert!(calculation_duration < Duration::from_millis(50),
               "Calculations should be fast even with large datasets: {:?}", calculation_duration);
    }

    /// Test concurrent metrics collection
    #[test]
    fn test_concurrent_metrics_collection() {
        use std::sync::Arc;
        use std::thread;

        let collector = Arc::new(Mutex::new(crate::load_testing_framework::MetricsCollector::new()));
        {
            collector.lock().unwrap().reset();
        }

        let num_threads = 10;
        let operations_per_thread = 1000;

        let start = Instant::now();

        let handles: Vec<_> = (0..num_threads)
            .map(|thread_id| {
                let collector = Arc::clone(&collector);
                thread::spawn(move || {
                    for i in 0..operations_per_thread {
                        let duration = Duration::from_micros(100 + (thread_id * 1000 + i) % 10000);
                        let complexity = match (thread_id + i) % 3 {
                            0 => crate::load_testing_framework::ToolComplexity::Simple,
                            1 => crate::load_testing_framework::ToolComplexity::Medium,
                            _ => crate::load_testing_framework::ToolComplexity::Complex,
                        };

                        collector.lock().unwrap().record_operation(duration, complexity, true);
                    }
                })
            })
            .collect();

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        let total_duration = start.elapsed();

        // Verify concurrent performance
        let expected_operations = num_threads * operations_per_thread;
        let ops_per_sec = collector.lock().unwrap().get_operations_per_second();

        assert!(ops_per_sec > 0.0,
               "Should handle concurrent operations, ops/sec: {}", ops_per_sec);

        assert!(total_duration < Duration::from_secs(5),
               "Concurrent operations should complete in reasonable time: {:?}", total_duration);

        // Verify data integrity
        let avg_response_time = collector.lock().unwrap().get_average_response_time();
        assert!(avg_response_time > Duration::ZERO,
               "Should calculate average correctly from concurrent operations");
    }
}

#[cfg(test)]
mod metrics_collector_edge_cases_tests {
    use super::*;

    /// Test metrics with zero operations
    #[test]
    fn test_zero_operations_metrics() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        // No operations recorded
        assert_eq!(collector.get_operations_per_second(), 0.0,
                   "Ops/sec should be 0 with no operations");
        assert_eq!(collector.get_average_response_time(), Duration::ZERO,
                   "Average response time should be 0 with no operations");

        // Resource metrics should still be available
        let resource_metrics = collector.get_resource_metrics();
        assert!(resource_metrics.peak_memory_mb >= 0.0,
               "Resource metrics should be available even with no operations");
    }

    /// Test metrics with single operation
    #[test]
    fn test_single_operation_metrics() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        collector.record_operation(
            Duration::from_millis(123),
            crate::load_testing_framework::ToolComplexity::Simple,
            true
        );

        assert!(collector.get_operations_per_second() > 0.0,
               "Ops/sec should be positive with single operation");
        assert_eq!(collector.get_average_response_time(), Duration::from_millis(123),
                   "Average should equal single operation duration");
    }

    /// Test metrics with very small durations
    #[test]
    fn test_very_small_durations() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        let small_durations = vec![
            Duration::from_nanos(1),
            Duration::from_nanos(10),
            Duration::from_nanos(100),
            Duration::from_micros(1),
            Duration::from_micros(10),
        ];

        for duration in small_durations {
            collector.record_operation(
                duration,
                crate::load_testing_framework::ToolComplexity::Simple,
                true
            );
        }

        let avg_response_time = collector.get_average_response_time();
        assert!(avg_response_time > Duration::ZERO,
               "Should handle very small durations");

        assert!(collector.get_operations_per_second() > 0.0,
               "Should calculate ops/sec with small durations");
    }

    /// Test metrics with very large durations
    #[test]
    fn test_very_large_durations() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        let large_durations = vec![
            Duration::from_secs(1),
            Duration::from_secs(10),
            Duration::from_secs(60),
        ];

        for duration in large_durations {
            collector.record_operation(
                duration,
                crate::load_testing_framework::ToolComplexity::Complex,
                true
            );
        }

        let avg_response_time = collector.get_average_response_time();
        assert!(avg_response_time >= Duration::from_secs(1),
               "Should handle large durations");

        assert!(collector.get_operations_per_second() > 0.0,
               "Should calculate ops/sec with large durations");
    }

    /// Test metrics with mixed success/failure rates
    #[test]
    fn test_mixed_success_failure_rates() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        let total_operations = 100;
        let failure_rates = vec![0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];

        for failure_rate in failure_rates {
            collector.reset();

            let num_failures = (total_operations as f64 * failure_rate) as usize;

            for i in 0..total_operations {
                let success = i >= num_failures;
                collector.record_operation(
                    Duration::from_millis(50),
                    crate::load_testing_framework::ToolComplexity::Simple,
                    success
                );
            }

            // Should handle all failure rates
            assert!(collector.get_operations_per_second() > 0.0,
                   "Should calculate ops/sec with failure rate {:.1}", failure_rate);

            let avg_response_time = collector.get_average_response_time();
            assert!(avg_response_time > Duration::ZERO,
                   "Should calculate average with failure rate {:.1}", failure_rate);
        }
    }

    /// Test time series data with edge case values
    #[test]
    fn test_time_series_edge_cases() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        // Test with zero values
        collector.record_time_series_data_point(crate::load_testing_framework::TimeSeriesDataPoint {
            timestamp: Instant::now(),
            operations_per_sec: 0.0,
            average_response_time: Duration::ZERO,
            memory_usage_mb: 0.0,
            cpu_percent: 0.0,
            active_connections: 0,
        });

        // Test with very high values
        collector.record_time_series_data_point(crate::load_testing_framework::TimeSeriesDataPoint {
            timestamp: Instant::now(),
            operations_per_sec: 1000000.0,
            average_response_time: Duration::from_secs(1),
            memory_usage_mb: 1024.0,
            cpu_percent: 100.0,
            active_connections: 1000,
        });

        // Test with negative values (if they occur)
        collector.record_time_series_data_point(crate::load_testing_framework::TimeSeriesDataPoint {
            timestamp: Instant::now(),
            operations_per_sec: -10.0,
            average_response_time: Duration::from_millis(-10),
            memory_usage_mb: -5.0,
            cpu_percent: -1.0,
            active_connections: 1,
        });

        let time_series_data = collector.get_time_series_data();
        assert_eq!(time_series_data.len(), 3,
                   "Should handle edge case time series values");

        // Should still work with edge case values
        let resource_metrics = collector.get_resource_metrics();
        assert!(resource_metrics.peak_memory_mb >= 0.0,
               "Should handle edge case values gracefully");
    }

    /// Test metrics collection under rapid reset cycles
    #[test]
    fn test_rapid_reset_cycles() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();

        // Test multiple rapid reset cycles
        for cycle in 0..10 {
            collector.reset();

            // Record some data
            for i in 0..10 {
                collector.record_operation(
                    Duration::from_millis(10 + i),
                    crate::load_testing_framework::ToolComplexity::Simple,
                    true
                );
            }

            // Verify metrics are correct for this cycle
            assert!(collector.get_operations_per_second() > 0.0,
                   "Cycle {} should have positive ops/sec", cycle);

            let avg_response_time = collector.get_average_response_time();
            assert!(avg_response_time > Duration::ZERO,
                   "Cycle {} should have positive average response time", cycle);

            let expected_average = Duration::from_millis((10 + 19) / 2); // Average of 10-19ms
            assert_eq!(avg_response_time, expected_average,
                       "Cycle {} average should be correct", cycle);
        }
    }

    /// Test metrics calculation accuracy under time pressure
    #[test]
    fn test_time_pressure_accuracy() {
        let mut collector = crate::load_testing_framework::MetricsCollector::new();
        collector.reset();

        let num_operations = 1000;
        let expected_average = Duration::from_millis(75); // Target average

        // Record operations with controlled timing
        for i in 0..num_operations {
            // Create a distribution around the expected average
            let variation = (i % 50) as i64 - 25; // -25 to +25
            let duration = expected_average + Duration::from_millis(variation as u64);

            collector.record_operation(
                duration,
                crate::load_testing_framework::ToolComplexity::Simple,
                true
            );

            // Add small delays to simulate real-time pressure
            if i % 100 == 0 {
                std::thread::sleep(Duration::from_micros(100));
            }
        }

        // Calculate actual average
        let actual_average = collector.get_average_response_time();

        // Should be close to expected average (within reasonable tolerance)
        let difference = if actual_average > expected_average {
            actual_average - expected_average
        } else {
            expected_average - actual_average
        };

        assert!(difference < Duration::from_millis(5),
               "Average should be accurate under time pressure: expected {:?}, actual {:?}, diff {:?}",
               expected_average, actual_average, difference);

        assert!(collector.get_operations_per_second() > 0.0,
               "Should maintain accuracy under time pressure");
    }
}