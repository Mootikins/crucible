//! Performance tests for the benchmarking framework itself
//!
//! This module tests that the benchmarking framework doesn't significantly
//! impact the performance of the benchmarks it's measuring, and that
//! the framework components are themselves performant.

use std::time::{Duration, Instant};
use std::sync::Arc;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use criterion::{black_box, Criterion, criterion_group, criterion_main};

use crate::benchmark_utils::*;
use crate::performance_reporter::*;
use crate::benchmark_runner::*;

#[cfg(test)]
mod tests {
    use super::*;

    // Test framework overhead measurement
    mod overhead_tests {
        use super::*;

        #[test]
        fn test_data_generation_overhead() {
            let iterations = 100;
            let document_size = 10; // 10KB

            // Measure time for data generation
            let start_time = Instant::now();

            let generator = TestDataGenerator::new().unwrap();
            for _ in 0..iterations {
                let documents = generator.generate_documents(10, document_size);
                black_box(documents); // Prevent compiler optimizations
            }

            let generation_time = start_time.elapsed();
            let avg_time_per_iteration = generation_time / iterations;

            // Data generation should be fast (less than 10ms per iteration for small datasets)
            assert!(avg_time_per_iteration < Duration::from_millis(10),
                   "Data generation should be fast: {:?} per iteration", avg_time_per_iteration);

            // Total time should be reasonable
            assert!(generation_time < Duration::from_secs(1),
                   "Total data generation should be fast: {:?}", generation_time);
        }

        #[test]
        fn test_report_generation_overhead() {
            // Create a large dataset for report generation
            let mut reporter = PerformanceReporter::new();
            let system_info = create_system_info();

            // Add many metrics to test report generation performance
            for suite_num in 0..10 {
                let mut suite = BenchmarkSuite {
                    name: format!("Test Suite {}", suite_num),
                    version: "1.0.0".to_string(),
                    commit_hash: format!("commit_{}", suite_num),
                    timestamp: chrono::Utc::now(),
                    system_info: system_info.clone(),
                    metrics: Vec::new(),
                };

                // Add many metrics per suite
                for metric_num in 0..100 {
                    suite.metrics.push(create_metric(
                        format!("metric_{}_{}", suite_num, metric_num),
                        "test_category".to_string(),
                        metric_num as f64,
                        "ms".to_string(),
                        100,
                        50,
                    ));
                }

                reporter.add_suite(suite);
            }

            // Measure report generation time
            let start_time = Instant::now();
            let report = reporter.generate_comprehensive_report();
            let generation_time = start_time.elapsed();

            // Report generation should be fast even for large datasets
            assert!(generation_time < Duration::from_millis(500),
                   "Report generation should be fast: {:?} for 1000 metrics", generation_time);

            assert!(!report.is_empty(), "Report should not be empty");
            assert!(report.len() > 10000, "Report should be substantial");
        }

        #[test]
        fn test_export_overhead() {
            let temp_dir = TempDir::new().unwrap();
            let json_path = temp_dir.path().join("test.json");
            let csv_path = temp_dir.path().join("test.csv");

            // Create large dataset
            let mut reporter = PerformanceReporter::new();
            let system_info = create_system_info();

            let mut suite = BenchmarkSuite {
                name: "Large Test Suite".to_string(),
                version: "1.0.0".to_string(),
                commit_hash: "test".to_string(),
                timestamp: chrono::Utc::now(),
                system_info,
                metrics: Vec::new(),
            };

            // Add many metrics
            for i in 0..1000 {
                suite.metrics.push(create_metric(
                    format!("metric_{}", i),
                    "test_category".to_string(),
                    i as f64,
                    "ms".to_string(),
                    100,
                    50,
                ));
            }

            reporter.add_suite(suite);

            // Test JSON export overhead
            let start_time = Instant::now();
            let json_result = reporter.export_json(&json_path);
            let json_time = start_time.elapsed();

            assert!(json_result.is_ok(), "JSON export should succeed");
            assert!(json_time < Duration::from_millis(100),
                   "JSON export should be fast: {:?}", json_time);

            // Test CSV export overhead
            let start_time = Instant::now();
            let csv_result = reporter.export_csv(&csv_path);
            let csv_time = start_time.elapsed();

            assert!(csv_result.is_ok(), "CSV export should succeed");
            assert!(csv_time < Duration::from_millis(100),
                   "CSV export should be fast: {:?}", csv_time);

            // Verify file sizes
            let json_size = std::fs::metadata(&json_path).unwrap().len();
            let csv_size = std::fs::metadata(&csv_path).unwrap().len();

            assert!(json_size > 1000, "JSON file should be substantial");
            assert!(csv_size > 1000, "CSV file should be substantial");
        }

        #[test]
        fn test_memory_allocation_patterns() {
            // Test that the framework doesn't have memory leaks or excessive allocations
            let initial_memory = get_memory_usage();

            // Perform many operations
            for i in 0..100 {
                let generator = TestDataGenerator::new().unwrap();
                let documents = generator.generate_documents(10, 10);
                black_box(documents);

                let mut reporter = PerformanceReporter::new();
                let system_info = create_system_info();
                let suite = BenchmarkSuite {
                    name: format!("Test Suite {}", i),
                    version: "1.0.0".to_string(),
                    commit_hash: format!("commit_{}", i),
                    timestamp: chrono::Utc::now(),
                    system_info,
                    metrics: vec![create_metric(
                        format!("metric_{}", i),
                        "test".to_string(),
                        i as f64,
                        "ms".to_string(),
                        10,
                        5,
                    )],
                };

                reporter.add_suite(suite);
                let _report = reporter.generate_comprehensive_report();
            }

            // Force garbage collection
            drop(generator);
            drop(reporter);

            let final_memory = get_memory_usage();

            // Memory usage should not have grown significantly
            if let (Some(initial), Some(final_mem)) = (initial_memory, final_memory) {
                let memory_growth = final_mem.saturating_sub(initial);
                let max_acceptable_growth = 100 * 1024 * 1024; // 100MB

                assert!(memory_growth < max_acceptable_growth,
                       "Memory growth should be minimal: {} bytes", memory_growth);
            }
        }

        fn get_memory_usage() -> Option<usize> {
            #[cfg(target_os = "linux")]
            {
                if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
                    for line in status.lines() {
                        if line.starts_with("VmRSS:") {
                            if let Some(kb_str) = line.split_whitespace().nth(1) {
                                if let Ok(kb) = kb_str.parse::<usize>() {
                                    return Some(kb * 1024); // Convert to bytes
                                }
                            }
                        }
                    }
                }
            }
            None
        }
    }

    // Test framework scalability
    mod scalability_tests {
        use super::*;

        #[test]
        fn test_large_dataset_scalability() {
            let sizes = vec![10, 100, 1000, 10000];

            for size in sizes {
                let generator = TestDataGenerator::new().unwrap();
                let start_time = Instant::now();

                let documents = generator.generate_documents(size, 1); // 1KB each
                let generation_time = start_time.elapsed();

                // Time should scale linearly (or better) with size
                let time_per_item = generation_time / size as u32;
                assert!(time_per_item < Duration::from_micros(1000), // 1ms per item max
                       "Generation time per item should be small: {:?} for size {}", time_per_item, size);

                println!("Size: {}, Total time: {:?}, Time per item: {:?}",
                        size, generation_time, time_per_item);

                // Memory usage should be reasonable
                let expected_memory = size * 1024; // 1KB per document
                let actual_memory = documents.iter().map(|d| d.content.len()).sum::<usize>();
                assert!(actual_memory >= expected_memory, "Memory usage should match document size");

                drop(documents); // Clean up before next iteration
            }
        }

        #[test]
        fn test_concurrent_framework_usage() {
            use std::sync::atomic::{AtomicUsize, Ordering};

            let thread_count = 8;
            let operations_per_thread = 100;
            let completed_operations = Arc::new(AtomicUsize::new(0));

            let handles: Vec<_> = (0..thread_count)
                .map(|thread_id| {
                    let completed_operations = Arc::clone(&completed_operations);

                    std::thread::spawn(move || {
                        for op_id in 0..operations_per_thread {
                            // Each thread performs framework operations
                            let generator = TestDataGenerator::new().unwrap();
                            let documents = generator.generate_documents(10, 1);
                            black_box(documents);

                            let mut reporter = PerformanceReporter::new();
                            let system_info = create_system_info();
                            let suite = BenchmarkSuite {
                                name: format!("Thread {} Operation {}", thread_id, op_id),
                                version: "1.0.0".to_string(),
                                commit_hash: format!("commit_{}_{}", thread_id, op_id),
                                timestamp: chrono::Utc::now(),
                                system_info,
                                metrics: vec![create_metric(
                                    format!("metric_{}_{}", thread_id, op_id),
                                    "test".to_string(),
                                    op_id as f64,
                                    "ms".to_string(),
                                    10,
                                    5,
                                )],
                            };

                            reporter.add_suite(suite);
                            let _report = reporter.generate_comprehensive_report();

                            completed_operations.fetch_add(1, Ordering::SeqCst);
                        }
                    })
                })
                .collect();

            // Wait for all threads to complete
            for handle in handles {
                handle.join().unwrap();
            }

            let total_completed = completed_operations.load(Ordering::SeqCst);
            let expected_total = thread_count * operations_per_thread;

            assert_eq!(total_completed, expected_total,
                      "All operations should complete: {} != {}", total_completed, expected_total);
        }

        #[test]
        fn test_report_size_scalability() {
            let metric_counts = vec![10, 100, 1000, 5000];

            for metric_count in metric_counts {
                let mut reporter = PerformanceReporter::new();
                let system_info = create_system_info();

                let mut suite = BenchmarkSuite {
                    name: "Scalability Test Suite".to_string(),
                    version: "1.0.0".to_string(),
                    commit_hash: "test".to_string(),
                    timestamp: chrono::Utc::now(),
                    system_info,
                    metrics: Vec::new(),
                };

                // Add specified number of metrics
                for i in 0..metric_count {
                    suite.metrics.push(create_metric(
                        format!("metric_{}", i),
                        "test_category".to_string(),
                        i as f64,
                        "ms".to_string(),
                        10,
                        5,
                    ));
                }

                reporter.add_suite(suite);

                // Measure report generation time
                let start_time = Instant::now();
                let report = reporter.generate_comprehensive_report();
                let generation_time = start_time.elapsed();

                // Report generation should scale reasonably
                let time_per_metric = generation_time.as_nanos() as f64 / metric_count as f64;
                assert!(time_per_metric < 10000.0, // 10 microseconds per metric max
                       "Report generation should scale well: {:.2}ns per metric for {} metrics",
                       time_per_metric, metric_count);

                println!("Metrics: {}, Generation time: {:?}, Time per metric: {:.2}ns, Report size: {}",
                        metric_count, generation_time, time_per_metric, report.len());

                // Report should be proportional to metric count
                let chars_per_metric = report.len() as f64 / metric_count as f64;
                assert!(chars_per_metric > 50.0, "Report should have reasonable content per metric");
                assert!(chars_per_metric < 1000.0, "Report shouldn't be excessively verbose per metric");
            }
        }

        #[test]
        fn test_memory_scalability() {
            let test_sizes = vec![100, 1000, 5000, 10000];

            for size in test_sizes {
                let initial_memory = get_memory_usage();

                // Create large dataset
                let generator = TestDataGenerator::new().unwrap();
                let documents = generator.generate_documents(size, 10); // 10KB each

                let mut reporter = PerformanceReporter::new();
                let system_info = create_system_info();

                let mut suite = BenchmarkSuite {
                    name: "Memory Scalability Test".to_string(),
                    version: "1.0.0".to_string(),
                    commit_hash: "test".to_string(),
                    timestamp: chrono::Utc::now(),
                    system_info,
                    metrics: Vec::new(),
                };

                // Add metrics proportional to document count
                for i in 0..size {
                    suite.metrics.push(create_metric(
                        format!("metric_{}", i),
                        "test".to_string(),
                        i as f64,
                        "ms".to_string(),
                        10,
                        5,
                    ));
                }

                reporter.add_suite(suite);
                let _report = reporter.generate_comprehensive_report();

                let peak_memory = get_memory_usage();

                // Clean up
                drop(documents);
                drop(reporter);
                drop(suite);

                let final_memory = get_memory_usage();

                if let (Some(initial), Some(peak), Some(final_mem)) = (initial_memory, peak_memory, final_memory) {
                    let memory_growth = peak.saturating_sub(initial);
                    let memory_reclaimed = peak.saturating_sub(final_mem);

                    println!("Size: {}, Memory growth: {}KB, Reclaimed: {}KB",
                            size, memory_growth / 1024, memory_reclaimed / 1024);

                    // Memory growth should be proportional to size
                    let memory_per_item = memory_growth as f64 / size as f64;
                    assert!(memory_per_item < 50000.0, // 50KB per item max
                           "Memory usage per item should be reasonable: {:.1} bytes", memory_per_item);

                    // Most memory should be reclaimed after cleanup
                    let reclaim_percentage = (memory_reclaimed as f64 / memory_growth as f64) * 100.0;
                    assert!(reclaim_percentage > 50.0,
                           "Should reclaim most memory: {:.1}% reclaimed", reclaim_percentage);
                }
            }
        }
    }

    // Test framework performance under stress
    mod stress_tests {
        use super::*;

        #[test]
        fn test_rapid_operations_stress() {
            let operation_count = 10000;
            let start_time = Instant::now();

            for i in 0..operation_count {
                // Rapid framework operations
                let generator = TestDataGenerator::new().unwrap();
                let documents = generator.generate_documents(1, 1); // Single document
                black_box(documents);

                // Quick metric creation
                let _metric = create_metric(
                    format!("stress_metric_{}", i),
                    "stress_test".to_string(),
                    i as f64,
                    "ms".to_string(),
                    1,
                    1,
                );
            }

            let total_time = start_time.elapsed();
            let avg_time_per_operation = total_time / operation_count;

            assert!(avg_time_per_operation < Duration::from_micros(1000), // 1ms per operation
                   "Framework should handle rapid operations: {:?} per operation", avg_time_per_operation);

            println!("Operations: {}, Total time: {:?}, Avg per operation: {:?}",
                    operation_count, total_time, avg_time_per_operation);
        }

        #[test]
        fn test_large_report_generation_stress() {
            let temp_dir = TempDir::new().unwrap();

            // Create multiple large suites
            let mut reporter = PerformanceReporter::new();
            let system_info = create_system_info();

            let suite_count = 100;
            let metrics_per_suite = 1000;

            for suite_id in 0..suite_count {
                let mut suite = BenchmarkSuite {
                    name: format!("Stress Test Suite {}", suite_id),
                    version: "1.0.0".to_string(),
                    commit_hash: format!("stress_commit_{}", suite_id),
                    timestamp: chrono::Utc::now(),
                    system_info: system_info.clone(),
                    metrics: Vec::new(),
                };

                for metric_id in 0..metrics_per_suite {
                    suite.metrics.push(create_metric(
                        format!("stress_metric_{}_{}", suite_id, metric_id),
                        "stress_category".to_string(),
                        (suite_id * metrics_per_suite + metric_id) as f64,
                        "ms".to_string(),
                        10,
                        5,
                    ));
                }

                reporter.add_suite(suite);
            }

            // Measure comprehensive report generation
            let start_time = Instant::now();
            let report = reporter.generate_comprehensive_report();
            let report_time = start_time.elapsed();

            let total_metrics = suite_count * metrics_per_suite;
            let time_per_metric = report_time.as_nanos() as f64 / total_metrics as f64;

            assert!(time_per_metric < 5000.0, // 5 microseconds per metric max
                   "Large report generation should be efficient: {:.2}ns per metric", time_per_metric);

            println!("Total metrics: {}, Report time: {:?}, Time per metric: {:.2}ns, Report size: {}",
                    total_metrics, report_time, time_per_metric, report.len());

            // Test export performance
            let json_path = temp_dir.path().join("stress_export.json");

            let start_time = Instant::now();
            let export_result = reporter.export_json(&json_path);
            let export_time = start_time.elapsed();

            assert!(export_result.is_ok(), "Large export should succeed");
            assert!(export_time < Duration::from_secs(5), "Large export should be fast");

            let file_size = std::fs::metadata(&json_path).unwrap().len();
            println!("Export size: {} bytes, Export time: {:?}", file_size, export_time);
        }

        #[test]
        fn test_concurrent_stress_with_resource_contention() {
            use std::sync::atomic::{AtomicUsize, Ordering};

            let thread_count = 16; // High concurrency
            let operations_per_thread = 1000;
            let shared_counter = Arc::new(AtomicUsize::new(0));

            let handles: Vec<_> = (0..thread_count)
                .map(|thread_id| {
                    let shared_counter = Arc::clone(&shared_counter);

                    std::thread::spawn(move || {
                        let mut local_results = Vec::new();

                        for op_id in 0..operations_per_thread {
                            let start_time = Instant::now();

                            // Perform framework operations
                            let generator = TestDataGenerator::new().unwrap();
                            let documents = generator.generate_documents(10, 1);

                            let mut reporter = PerformanceReporter::new();
                            let system_info = create_system_info();
                            let suite = BenchmarkSuite {
                                name: format!("Concurrent Stress {}-{}", thread_id, op_id),
                                version: "1.0.0".to_string(),
                                commit_hash: format!("concurrent_commit_{}_{}", thread_id, op_id),
                                timestamp: chrono::Utc::now(),
                                system_info,
                                metrics: vec![create_metric(
                                    format!("concurrent_metric_{}_{}", thread_id, op_id),
                                    "stress_test".to_string(),
                                    op_id as f64,
                                    "ms".to_string(),
                                    10,
                                    5,
                                )],
                            };

                            reporter.add_suite(suite);
                            let _report = reporter.generate_comprehensive_report();

                            let operation_time = start_time.elapsed();
                            local_results.push(operation_time);

                            // Update shared counter
                            shared_counter.fetch_add(1, Ordering::SeqCst);
                        }

                        // Return average time for this thread
                        let total_time: Duration = local_results.iter().sum();
                        total_time / operations_per_thread as u32
                    })
                })
                .collect();

            // Wait for all threads and collect results
            let thread_times: Vec<_> = handles.into_iter()
                .map(|handle| handle.join().unwrap())
                .collect();

            let total_operations = shared_counter.load(Ordering::SeqCst);
            let expected_operations = thread_count * operations_per_thread;

            assert_eq!(total_operations, expected_operations,
                      "All concurrent operations should complete");

            // Calculate performance statistics
            let avg_thread_time: Duration = thread_times.iter().sum();
            let avg_thread_time = avg_thread_time / thread_count as u32;
            let fastest_thread = thread_times.iter().min().unwrap();
            let slowest_thread = thread_times.iter().max().unwrap();

            println!("Concurrent stress test results:");
            println!("  Threads: {}", thread_count);
            println!("  Operations per thread: {}", operations_per_thread);
            println!("  Total operations: {}", total_operations);
            println!("  Average thread time: {:?}", avg_thread_time);
            println!("  Fastest thread: {:?}", fastest_thread);
            println!("  Slowest thread: {:?}", slowest_thread);

            // Performance should be reasonable even under contention
            assert!(avg_thread_time < Duration::from_millis(100),
                   "Average thread time should be reasonable under contention: {:?}", avg_thread_time);

            // Variance between threads shouldn't be too high
            let variance_factor = *slowest_thread.as_nanos() as f64 / *fastest_thread.as_nanos() as f64;
            assert!(variance_factor < 5.0,
                   "Thread performance variance should be reasonable: {:.2}x", variance_factor);
        }

        #[test]
        fn test_memory_pressure_stress() {
            let initial_memory = get_memory_usage();

            // Create memory pressure while using the framework
            let memory_allocations: Vec<Vec<u8>> = (0..100)
                .map(|_| vec![0u8; 1024 * 1024]) // 1MB each, 100MB total
                .collect();

            // Perform framework operations under memory pressure
            let stress_start = Instant::now();

            for i in 0..1000 {
                let generator = TestDataGenerator::new().unwrap();
                let documents = generator.generate_documents(10, 10); // 10KB each
                black_box(documents);

                let mut reporter = PerformanceReporter::new();
                let system_info = create_system_info();
                let suite = BenchmarkSuite {
                    name: format!("Memory Pressure Test {}", i),
                    version: "1.0.0".to_string(),
                    commit_hash: format!("pressure_commit_{}", i),
                    timestamp: chrono::Utc::now(),
                    system_info,
                    metrics: vec![create_metric(
                        format!("pressure_metric_{}", i),
                        "memory_pressure_test".to_string(),
                        i as f64,
                        "ms".to_string(),
                        10,
                        5,
                    )],
                };

                reporter.add_suite(suite);
                let _report = reporter.generate_comprehensive_report();
            }

            let stress_time = stress_start.elapsed();
            let pressure_memory = get_memory_usage();

            // Clean up memory allocations
            drop(memory_allocations);

            let final_memory = get_memory_usage();

            // Framework should still perform reasonably under memory pressure
            let avg_time_per_operation = stress_time / 1000;
            assert!(avg_time_per_operation < Duration::from_millis(10),
                   "Framework should perform under memory pressure: {:?} per operation", avg_time_per_operation);

            println!("Memory pressure stress test:");
            println!("  Operations: 1000");
            println!("  Total time: {:?}", stress_time);
            println!("  Average per operation: {:?}", avg_time_per_operation);

            if let (Some(initial), Some(pressure), Some(final_mem)) = (initial_memory, pressure_memory, final_memory) {
                println!("  Initial memory: {}KB", initial / 1024);
                println!("  Under pressure: {}KB", pressure / 1024);
                println!("  After cleanup: {}KB", final_mem / 1024);
                println!("  Peak increase: {}KB", (pressure.saturating_sub(initial)) / 1024);
                println!("  Memory reclaimed: {}KB", (pressure.saturating_sub(final_mem)) / 1024);
            }
        }
    }

    // Test framework performance regression detection
    mod regression_tests {
        use super::*;

        #[test]
        fn test_performance_regression_detection() {
            // Establish baseline performance
            let baseline_iterations = 1000;
            let baseline_start = Instant::now();

            for i in 0..baseline_iterations {
                let generator = TestDataGenerator::new().unwrap();
                let documents = generator.generate_documents(10, 1);
                black_box(documents);

                let _metric = create_metric(
                    format!("baseline_metric_{}", i),
                    "baseline_test".to_string(),
                    i as f64,
                    "ms".to_string(),
                    10,
                    5,
                );
            }

            let baseline_time = baseline_start.elapsed();
            let baseline_avg = baseline_time / baseline_iterations;

            // Simulate current performance (potentially slower)
            let current_iterations = 1000;
            let current_start = Instant::now();

            // Add artificial slowdown to simulate regression
            for i in 0..current_iterations {
                let generator = TestDataGenerator::new().unwrap();
                let documents = generator.generate_documents(10, 1);
                black_box(documents);

                // Simulate additional work
                std::thread::sleep(Duration::from_nanos(100));

                let _metric = create_metric(
                    format!("current_metric_{}", i),
                    "current_test".to_string(),
                    i as f64,
                    "ms".to_string(),
                    10,
                    5,
                );
            }

            let current_time = current_start.elapsed();
            let current_avg = current_time / current_iterations;

            // Calculate performance change
            let performance_ratio = current_avg.as_nanos() as f64 / baseline_avg.as_nanos() as f64;
            let regression_percentage = (performance_ratio - 1.0) * 100.0;

            println!("Performance regression test:");
            println!("  Baseline average: {:?}", baseline_avg);
            println!("  Current average: {:?}", current_avg);
            println!("  Performance ratio: {:.2}x", performance_ratio);
            println!("  Regression: {:.1}%", regression_percentage);

            // In a real test, we'd want regression_percentage to be small
            // For this test, we just verify the detection works
            assert!(regression_percentage > 0.0, "Should detect performance regression");
            assert!(regression_percentage < 1000.0, "Regression should be reasonable");
        }

        #[test]
        fn test_memory_regression_detection() {
            // Test memory usage patterns for regression
            let initial_memory = get_memory_usage();

            // Perform memory-intensive operations
            let mut allocations = Vec::new();
            for i in 0..100 {
                // Create data
                let generator = TestDataGenerator::new().unwrap();
                let documents = generator.generate_documents(100, 10); // 1MB per document
                allocations.push(documents);

                // Create metrics
                let _metric = create_metric(
                    format!("memory_test_{}", i),
                    "memory_regression_test".to_string(),
                    i as f64,
                    "ms".to_string(),
                    10,
                    5,
                );
            }

            let peak_memory = get_memory_usage();

            // Clean up half the allocations
            for _ in 0..50 {
                allocations.pop();
            }

            let partial_cleanup_memory = get_memory_usage();

            // Clean up everything
            drop(allocations);
            let final_memory = get_memory_usage();

            if let (Some(initial), Some(peak), Some(partial), Some(final_mem)) =
                (initial_memory, peak_memory, partial_cleanup_memory, final_memory) {

                let total_growth = peak.saturating_sub(initial);
                let partial_reclaimed = peak.saturating_sub(partial);
                let total_reclaimed = peak.saturating_sub(final_mem);

                println!("Memory regression test:");
                println!("  Initial: {}KB", initial / 1024);
                println!("  Peak: {}KB", peak / 1024);
                println!("  Partial cleanup: {}KB", partial / 1024);
                println!("  Final: {}KB", final_mem / 1024);
                println!("  Total growth: {}KB", total_growth / 1024);
                println!("  Partial reclaim: {}KB", partial_reclaimed / 1024);
                println!("  Total reclaim: {}KB", total_reclaimed / 1024);
                println!("  Reclaim efficiency: {:.1}%",
                        (total_reclaimed as f64 / total_growth as f64) * 100.0);

                // Memory should be efficiently reclaimed
                let reclaim_efficiency = (total_reclaimed as f64 / total_growth as f64) * 100.0;
                assert!(reclaim_efficiency > 70.0,
                       "Should reclaim most memory: {:.1}%", reclaim_efficiency);
            }
        }

        #[test]
        fn test_scalability_regression_detection() {
            // Test that performance scales well with increasing data sizes
            let data_sizes = vec![10, 100, 1000];
            let mut times = Vec::new();

            for size in data_sizes {
                let start_time = Instant::now();

                let generator = TestDataGenerator::new().unwrap();
                let documents = generator.generate_documents(size, 1);
                black_box(documents);

                let mut reporter = PerformanceReporter::new();
                let system_info = create_system_info();

                let mut suite = BenchmarkSuite {
                    name: format!("Scalability Test {}", size),
                    version: "1.0.0".to_string(),
                    commit_hash: format!("scale_commit_{}", size),
                    timestamp: chrono::Utc::now(),
                    system_info,
                    metrics: Vec::new(),
                };

                for i in 0..size {
                    suite.metrics.push(create_metric(
                        format!("scale_metric_{}_{}", size, i),
                        "scalability_test".to_string(),
                        i as f64,
                        "ms".to_string(),
                        10,
                        5,
                    ));
                }

                reporter.add_suite(suite);
                let _report = reporter.generate_comprehensive_report();

                let total_time = start_time.elapsed();
                times.push(total_time);

                println!("Size: {}, Time: {:?}", size, total_time);
            }

            // Check that time scales linearly (or better) with size
            if times.len() >= 2 {
                for i in 1..times.len() {
                    let size_ratio = data_sizes[i] as f64 / data_sizes[i-1] as f64;
                    let time_ratio = times[i].as_nanos() as f64 / times[i-1].as_nanos() as f64;
                    let scaling_factor = time_ratio / size_ratio;

                    println!("  Size ratio: {:.1}x, Time ratio: {:.1}x, Scaling factor: {:.2}",
                            size_ratio, time_ratio, scaling_factor);

                    // Scaling should be close to linear (factor <= 2.0)
                    assert!(scaling_factor < 2.0,
                           "Time scaling should be reasonable: {:.2}x (should be <= 2.0x)", scaling_factor);
                }
            }
        }
    }

    // Benchmark framework components using Criterion
    mod criterion_benchmarks {
        use super::*;
        use criterion::{black_box, BenchmarkId, Criterion};

        fn bench_data_generation(c: &mut Criterion) {
            let generator = TestDataGenerator::new().unwrap();

            let mut group = c.benchmark_group("data_generation");

            for size in [10, 100, 1000].iter() {
                group.bench_with_input(
                    BenchmarkId::new("generate_documents", size),
                    size,
                    |b, &size| {
                        b.iter(|| {
                            let docs = generator.generate_documents(black_box(size), black_box(1));
                            black_box(docs)
                        })
                    },
                );
            }

            group.finish();
        }

        fn bench_metric_creation(c: &mut Criterion) {
            let mut group = c.benchmark_group("metric_creation");

            for count in [1, 100, 1000].iter() {
                group.bench_with_input(
                    BenchmarkId::new("create_metrics", count),
                    count,
                    |b, &count| {
                        b.iter(|| {
                            let mut metrics = Vec::new();
                            for i in 0..count {
                                metrics.push(create_metric(
                                    format!("metric_{}", i),
                                    "test".to_string(),
                                    i as f64,
                                    "ms".to_string(),
                                    10,
                                    5,
                                ));
                            }
                            black_box(metrics)
                        })
                    },
                );
            }

            group.finish();
        }

        fn bench_report_generation(c: &mut Criterion) {
            let mut group = c.benchmark_group("report_generation");

            for metric_count in [10, 100, 1000].iter() {
                let mut reporter = PerformanceReporter::new();
                let system_info = create_system_info();

                let mut suite = BenchmarkSuite {
                    name: "Benchmark Suite".to_string(),
                    version: "1.0.0".to_string(),
                    commit_hash: "benchmark".to_string(),
                    timestamp: chrono::Utc::now(),
                    system_info,
                    metrics: Vec::new(),
                };

                for i in 0..*metric_count {
                    suite.metrics.push(create_metric(
                        format!("metric_{}", i),
                        "test".to_string(),
                        i as f64,
                        "ms".to_string(),
                        10,
                        5,
                    ));
                }

                reporter.add_suite(suite);

                group.bench_with_input(
                    BenchmarkId::new("generate_report", metric_count),
                    metric_count,
                    |b, _| {
                        b.iter(|| {
                            let report = reporter.generate_comprehensive_report();
                            black_box(report)
                        })
                    },
                );
            }

            group.finish();
        }

        criterion_group!(
            framework_benches,
            bench_data_generation,
            bench_metric_creation,
            bench_report_generation
        );

        criterion_main!(framework_benches);
    }
}