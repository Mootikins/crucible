//! Edge case and error handling tests for the benchmarking framework
//!
//! This module tests error conditions, boundary cases, and exceptional
//! scenarios to ensure the framework is robust and reliable.

use std::path::{Path, PathBuf};
use std::fs;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use anyhow::{Result, anyhow};

use crate::benchmark_utils::*;
use crate::performance_reporter::*;
use crate::benchmark_runner::*;

#[cfg(test)]
mod tests {
    use super::*;

    // Test extreme data sizes and boundary conditions
    mod boundary_tests {
        use super::*;

        #[test]
        fn test_zero_datasets() {
            let generator = TestDataGenerator::new().unwrap();

            let empty_docs = generator.generate_documents(0, 10);
            assert_eq!(empty_docs.len(), 0, "Zero documents should be handled");

            let empty_events = generator.generate_events(0, &["test"]);
            assert_eq!(empty_events.len(), 0, "Zero events should be handled");

            let empty_files = generator.create_test_files(0, 10).unwrap();
            assert_eq!(empty_files.len(), 0, "Zero files should be handled");
        }

        #[test]
        fn test_maximum_single_document() {
            let generator = TestDataGenerator::new().unwrap();

            // Test very large single document
            let large_docs = generator.generate_documents(1, 10240); // 10MB document
            assert_eq!(large_docs.len(), 1, "Should handle large single document");
            assert_eq!(large_docs[0].content.len(), 10240 * 1024, "Document should be correct size");
        }

        #[test]
        fn test_single_items() {
            let generator = TestDataGenerator::new().unwrap();

            let single_doc = generator.generate_documents(1, 1);
            assert_eq!(single_doc.len(), 1, "Should handle single document");
            assert_eq!(single_doc[0].id, "doc_0", "Single document should have correct ID");

            let single_event = generator.generate_events(1, &["test"]);
            assert_eq!(single_event.len(), 1, "Should handle single event");
            assert_eq!(single_event[0].id, "event_0", "Single event should have correct ID");

            let single_files = generator.create_test_files(1, 1).unwrap();
            assert_eq!(single_files.len(), 1, "Should handle single file");
            assert!(single_files[0].exists(), "Single file should exist");
        }

        #[test]
        fn test_extreme_event_types() {
            let generator = TestDataGenerator::new().unwrap();

            // Empty event types
            let empty_type_events = generator.generate_events(10, &[]);
            assert_eq!(empty_type_events.len(), 0, "Empty event types should produce no events");

            // Single event type
            let single_type_events = generator.generate_events(10, &["only_type"]);
            assert_eq!(single_type_events.len(), 10, "Single event type should work");
            for event in &single_type_events {
                assert_eq!(event.event_type, "only_type", "All events should have same type");
            }

            // Many event types
            let many_types = vec!["type1", "type2", "type3", "type4", "type5", "type6", "type7", "type8", "type9", "type10"];
            let many_type_events = generator.generate_events(100, &many_types);
            assert_eq!(many_type_events.len(), 100, "Many event types should work");
        }

        #[test]
        fn test_extreme_benchmark_configurations() {
            // Zero iterations
            let zero_config = BenchmarkConfig {
                small_dataset: 0,
                medium_dataset: 0,
                large_dataset: 0,
                iterations: 0,
                warmup_iterations: 0,
                sample_size: 0,
            };
            assert_eq!(zero_config.iterations, 0, "Zero iterations should be allowed");

            // Very large iterations
            let large_config = BenchmarkConfig {
                small_dataset: 1,
                medium_dataset: 1,
                large_dataset: 1,
                iterations: u32::MAX,
                warmup_iterations: u32::MAX,
                sample_size: u32::MAX,
            };
            assert_eq!(large_config.iterations, u32::MAX, "Max iterations should be allowed");

            // Minimum values
            let min_config = BenchmarkConfig {
                small_dataset: 1,
                medium_dataset: 1,
                large_dataset: 1,
                iterations: 1,
                warmup_iterations: 0,
                sample_size: 1,
            };
            assert_eq!(min_config.iterations, 1, "Minimum iterations should work");
        }

        #[test]
        fn test_extreme_performance_metrics() {
            // Zero values
            let zero_metric = create_metric(
                "zero_metric".to_string(),
                "test".to_string(),
                0.0,
                "ms".to_string(),
                1,
                1,
            );
            assert_eq!(zero_metric.value, 0.0, "Zero value should be allowed");

            // Maximum values
            let max_metric = create_metric(
                "max_metric".to_string(),
                "test".to_string(),
                f64::MAX,
                "ms".to_string(),
                1,
                1,
            );
            assert_eq!(max_metric.value, f64::MAX, "Max value should be allowed");

            // Minimum positive values
            let min_metric = create_metric(
                "min_metric".to_string(),
                "test".to_string(),
                f64::MIN_POSITIVE,
                "ms".to_string(),
                1,
                1,
            );
            assert_eq!(min_metric.value, f64::MIN_POSITIVE, "Min positive value should be allowed");

            // Infinity values (should be handled gracefully)
            let inf_metric = create_metric(
                "inf_metric".to_string(),
                "test".to_string(),
                f64::INFINITY,
                "ms".to_string(),
                1,
                1,
            );
            assert!(inf_metric.value.is_infinite(), "Infinite value should be stored");

            // NaN values (should be handled gracefully)
            let nan_metric = create_metric(
                "nan_metric".to_string(),
                "test".to_string(),
                f64::NAN,
                "ms".to_string(),
                1,
                1,
            );
            assert!(nan_metric.value.is_nan(), "NaN value should be stored");
        }
    }

    // Test error conditions and failure scenarios
    mod error_handling_tests {
        use super::*;

        #[test]
        fn test_invalid_file_operations() {
            let generator = TestDataGenerator::new().unwrap();

            // Test creating files in invalid location
            let invalid_path = PathBuf::from("/invalid/path/that/does/not/exist");

            // We can't easily test this with our current setup since TestDataGenerator
            // uses temp directories, but we can test related error scenarios

            // Test reading non-existent files
            let non_existent = generator.temp_dir().join("non_existent_file.txt");
            assert!(!non_existent.exists(), "Non-existent file should not exist");

            let read_result = fs::read_to_string(&non_existent);
            assert!(read_result.is_err(), "Reading non-existent file should fail");
        }

        #[test]
        fn test_corrupted_data_handling() {
            // Test handling of corrupted JSON data
            let corrupted_json = "{ invalid json content";
            let parse_result: Result<serde_json::Value, _> = serde_json::from_str(corrupted_json);
            assert!(parse_result.is_err(), "Corrupted JSON should fail to parse");

            // Test handling of incomplete CSV data
            let incomplete_csv = "name,value\nitem1,";
            let csv_lines: Vec<&str> = incomplete_csv.lines().collect();
            assert_eq!(csv_lines.len(), 2, "Should parse incomplete CSV");

            let incomplete_line = csv_lines[1];
            let fields: Vec<&str> = incomplete_line.split(',').collect();
            assert_eq!(fields.len(), 2, "Should split incomplete line");
            assert!(fields[1].is_empty(), "Missing value should be empty string");
        }

        #[test]
        fn test_memory_exhaustion_simulation() {
            // Test behavior when memory is constrained
            let large_allocations: Vec<Vec<u8>> = (0..100)
                .map(|_| vec![0u8; 1024 * 1024]) // 1MB allocations
                .collect();

            assert_eq!(large_allocations.len(), 100, "Should handle many allocations");

            // Test dropping large allocations
            drop(large_allocations);

            // Should be able to allocate again after cleanup
            let new_allocation = vec![0u8; 1024 * 1024];
            assert_eq!(new_allocation.len(), 1024 * 1024, "Should allocate after cleanup");
        }

        #[test]
        fn test_concurrent_access_conflicts() {
            use std::sync::atomic::{AtomicUsize, Ordering};

            let counter = Arc::new(AtomicUsize::new(0));
            let handles: Vec<_> = (0..10)
                .map(|_| {
                    let counter = Arc::clone(&counter);
                    std::thread::spawn(move || {
                        for _ in 0..1000 {
                            counter.fetch_add(1, Ordering::SeqCst);
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }

            assert_eq!(counter.load(Ordering::SeqCst), 10000, "Concurrent access should work correctly");
        }

        #[test]
        fn test_timeout_scenarios() {
            // Test operation timeouts
            let start_time = Instant::now();

            // Simulate a long-running operation
            std::thread::sleep(Duration::from_millis(100));

            let elapsed = start_time.elapsed();
            assert!(elapsed >= Duration::from_millis(100), "Should measure elapsed time correctly");
            assert!(elapsed < Duration::from_millis(200), "Should not take too long");

            // Test timeout handling
            let timeout = Duration::from_millis(50);
            let operation_start = Instant::now();

            // Simulate operation that might timeout
            std::thread::sleep(Duration::from_millis(10));

            if operation_start.elapsed() > timeout {
                panic!("Operation should not timeout");
            }
        }

        #[test]
        fn test_invalid_configuration_handling() {
            // Test invalid export formats
            let mut invalid_config = BenchmarkRunnerConfig::default();
            invalid_config.export_formats = vec!["invalid_format".to_string()];

            let runner = BenchmarkRunner::new(invalid_config);
            assert!(!runner.config.export_formats.is_empty(), "Invalid config should still create runner");

            // Test invalid output directory
            let mut invalid_path_config = BenchmarkRunnerConfig::default();
            invalid_path_config.output_dir = "/root/invalid/path".to_string();

            let invalid_runner = BenchmarkRunner::new(invalid_path_config);
            assert_eq!(invalid_runner.config.output_dir, "/root/invalid/path", "Invalid path should be preserved");
        }
    }

    // Test resource exhaustion and recovery
    mod resource_exhaustion_tests {
        use super::*;

        #[test]
        fn test_file_descriptor_exhaustion_simulation() {
            // This test simulates file descriptor usage but doesn't actually exhaust them
            // as that would cause system instability

            let generator = TestDataGenerator::new().unwrap();
            let mut file_handles = Vec::new();

            // Create many files (but not enough to exhaust system limits)
            for i in 0..100 {
                let files = generator.create_test_files(1, 1).unwrap();
                file_handles.extend(files);
            }

            assert_eq!(file_handles.len(), 100, "Should create many files");

            // Verify all files exist and can be read
            for file_path in &file_handles {
                assert!(file_path.exists(), "All files should exist");
                let content = fs::read_to_string(file_path).unwrap();
                assert_eq!(content.len(), 1024, "All files should have correct content");
            }

            // Clean up
            drop(file_handles);
        }

        #[test]
        fn test_memory_pressure_scenarios() {
            // Test behavior under memory pressure
            let memory_intensive_data: Vec<Vec<u8>> = (0..50)
                .map(|i| vec![i as u8; 1024 * 1024]) // 1MB each, total 50MB
                .collect();

            // System should still respond normally
            let small_allocation = vec![0u8; 1024];
            assert_eq!(small_allocation.len(), 1024, "Should still be able to allocate small memory");

            // Test that we can still create benchmark components
            let generator = TestDataGenerator::new().unwrap();
            let docs = generator.generate_documents(10, 1);
            assert_eq!(docs.len(), 10, "Should still generate documents under memory pressure");

            // Clean up
            drop(memory_intensive_data);
        }

        #[test]
        fn test_cpu_saturation_simulation() {
            // Test behavior under CPU load
            let start_time = Instant::now();

            // CPU-intensive computation
            let mut result = 0u64;
            for i in 0..1_000_000 {
                result = result.wrapping_add(i * i);
            }

            let computation_time = start_time.elapsed();
            assert!(computation_time > Duration::from_millis(0), "Computation should take time");

            // System should still be responsive
            let generator = TestDataGenerator::new().unwrap();
            let docs = generator.generate_documents(10, 1);
            assert_eq!(docs.len(), 10, "Should still generate documents under CPU load");
        }

        #[test]
        fn test_disk_space_simulation() {
            let generator = TestDataGenerator::new().unwrap();

            // Create large files to consume disk space (but not excessive)
            let large_files = generator.create_test_files(10, 1024).unwrap(); // 10MB each, 100MB total

            assert_eq!(large_files.len(), 10, "Should create large files");

            for file_path in &large_files {
                let metadata = fs::metadata(file_path).unwrap();
                assert_eq!(metadata.len(), 1024 * 1024, "Files should be correct size");
            }

            // Test that we can still create more files
            let additional_files = generator.create_test_files(5, 1).unwrap();
            assert_eq!(additional_files.len(), 5, "Should still create files after large allocations");
        }
    }

    // Test malformed input handling
    mod malformed_input_tests {
        use super::*;

        #[test]
        fn test_malformed_metric_data() {
            let mut reporter = PerformanceReporter::new();

            // Create metrics with problematic values
            let problematic_metrics = vec![
                create_metric("empty_name".to_string(), "".to_string(), 100.0, "ms".to_string(), 1, 1),
                create_metric("negative_value".to_string(), "test".to_string(), -100.0, "ms".to_string(), 1, 1),
                create_metric("empty_unit".to_string(), "test".to_string(), 100.0, "".to_string(), 1, 1),
                create_metric("very_long_name".to_string(), "test".to_string(), 100.0, "ms".to_string(), 1, 1),
            ];

            // Make one with a very long name
            let mut very_long_metric = create_metric("test".to_string(), "test".to_string(), 100.0, "ms".to_string(), 1, 1);
            very_long_metric.name = "x".repeat(10000);
            problematic_metrics.push(very_long_metric);

            // Create suite with problematic metrics
            let system_info = create_system_info();
            let problematic_suite = BenchmarkSuite {
                name: "Problematic Suite".to_string(),
                version: "1.0.0".to_string(),
                commit_hash: "test".to_string(),
                timestamp: chrono::Utc::now(),
                system_info,
                metrics: problematic_metrics,
            };

            // Should handle problematic data gracefully
            reporter.add_suite(problematic_suite);

            let report = reporter.generate_comprehensive_report();
            assert!(!report.is_empty(), "Should generate report despite problematic data");
        }

        #[test]
        fn test_unicode_and_special_characters() {
            let generator = TestDataGenerator::new().unwrap();

            // Test Unicode characters in document content
            let unicode_content = "测试 Unicode 🚀 Content with émojis and spëcial charactërs";

            // This would require modifying the TestDataGenerator to accept custom content
            // For now, we test that the system handles Unicode in metadata

            let mut suite = BenchmarkSuite {
                name: "Unicode Test Suite 测试".to_string(),
                version: "1.0.0 🚀".to_string(),
                commit_hash: "test_émojis".to_string(),
                timestamp: chrono::Utc::now(),
                system_info: create_system_info(),
                metrics: Vec::new(),
            };

            let unicode_metric = create_metric(
                "测试_metric_🚀".to_string(),
                "unicode_category_émojis".to_string(),
                100.0,
                "ms".to_string(),
                1,
                1,
            );

            suite.metrics.push(unicode_metric);

            let mut reporter = PerformanceReporter::new();
            reporter.add_suite(suite);

            let report = reporter.generate_comprehensive_report();
            assert!(report.contains("测试"), "Report should contain Chinese characters");
            assert!(report.contains("🚀"), "Report should contain emoji");
            assert!(report.contains("émojis"), "Report should contain accented characters");
        }

        #[test]
        fn test_extremely_long_strings() {
            let very_long_string = "x".repeat(1_000_000);

            let mut suite = BenchmarkSuite {
                name: very_long_string.clone(),
                version: very_long_string.clone(),
                commit_hash: very_long_string.clone(),
                timestamp: chrono::Utc::now(),
                system_info: create_system_info(),
                metrics: Vec::new(),
            };

            let long_metric = create_metric(
                very_long_string.clone(),
                very_long_string.clone(),
                100.0,
                very_long_string.clone(),
                1,
                1,
            );

            suite.metrics.push(long_metric);

            let mut reporter = PerformanceReporter::new();
            reporter.add_suite(suite);

            // Should handle very long strings without crashing
            let report = reporter.generate_comprehensive_report();
            assert!(!report.is_empty(), "Should handle very long strings");

            // Report might be truncated for practical reasons, but shouldn't crash
            assert!(report.len() > 1000, "Report should be substantial");
        }

        #[test]
        fn test_null_and_control_characters() {
            let null_string = "test\0string";
            let control_string = "test\n\r\tstring";

            let mut suite = BenchmarkSuite {
                name: null_string.to_string(),
                version: control_string.to_string(),
                commit_hash: "test".to_string(),
                timestamp: chrono::Utc::now(),
                system_info: create_system_info(),
                metrics: Vec::new(),
            };

            let control_metric = create_metric(
                control_string.to_string(),
                null_string.to_string(),
                100.0,
                "ms".to_string(),
                1,
                1,
            );

            suite.metrics.push(control_metric);

            let mut reporter = PerformanceReporter::new();
            reporter.add_suite(suite);

            // Should handle control characters
            let report = reporter.generate_comprehensive_report();
            assert!(!report.is_empty(), "Should handle control characters");
        }
    }

    // Test concurrent failures and race conditions
    mod concurrency_failure_tests {
        use super::*;
        use std::sync::atomic::{AtomicBool, Ordering};

        #[test]
        fn test_concurrent_report_generation() {
            let reporter = Arc::new(Mutex::new(PerformanceReporter::new()));
            let system_info = create_system_info();

            // Add a suite to the reporter
            {
                let mut rep = reporter.lock().unwrap();
                let suite = BenchmarkSuite {
                    name: "Test Suite".to_string(),
                    version: "1.0.0".to_string(),
                    commit_hash: "test".to_string(),
                    timestamp: chrono::Utc::now(),
                    system_info,
                    metrics: vec![create_metric("test".to_string(), "test".to_string(), 100.0, "ms".to_string(), 1, 1)],
                };
                rep.add_suite(suite);
            }

            let error_occurred = Arc::new(AtomicBool::new(false));

            // Spawn multiple threads trying to generate reports concurrently
            let handles: Vec<_> = (0..5)
                .map(|_| {
                    let reporter = Arc::clone(&reporter);
                    let error_occurred = Arc::clone(&error_occurred);

                    std::thread::spawn(move || {
                        for _ in 0..10 {
                            let report = {
                                let rep = reporter.lock().unwrap();
                                rep.generate_comprehensive_report()
                            };

                            if report.is_empty() {
                                error_occurred.store(true, Ordering::SeqCst);
                                break;
                            }
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }

            assert!(!error_occurred.load(Ordering::SeqCst), "No errors should occur during concurrent report generation");
        }

        #[test]
        fn test_concurrent_file_operations() {
            let temp_dir = TempDir::new().unwrap();
            let file_path = temp_dir.path().join("concurrent_test.txt");

            let error_occurred = Arc::new(AtomicBool::new(false));

            // Spawn multiple threads trying to write to the same file
            let handles: Vec<_> = (0..10)
                .map(|i| {
                    let file_path = file_path.clone();
                    let error_occurred = Arc::clone(&error_occurred);

                    std::thread::spawn(move || {
                        for j in 0..10 {
                            let content = format!("Thread {}, iteration {}\n", i, j);
                            if let Err(_) = fs::write(&file_path, content) {
                                // File write conflicts are expected in this test
                                // We're just testing that the system handles them gracefully
                            }
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }

            // The file should exist (though its content is indeterminate due to race conditions)
            assert!(file_path.exists(), "File should exist after concurrent operations");
        }

        #[test]
        fn test_resource_cleanup_under_pressure() {
            let resources_created = Arc::new(AtomicUsize::new(0));
            let resources_cleaned = Arc::new(AtomicUsize::new(0));

            let handles: Vec<_> = (0..10)
                .map(|_| {
                    let resources_created = Arc::clone(&resources_created);
                    let resources_cleaned = Arc::clone(&resources_cleaned);

                    std::thread::spawn(move || {
                        for _ in 0..100 {
                            // Create resource
                            resources_created.fetch_add(1, Ordering::SeqCst);

                            // Simulate some work
                            std::thread::sleep(Duration::from_micros(1));

                            // Clean up resource
                            resources_cleaned.fetch_add(1, Ordering::SeqCst);
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }

            let created = resources_created.load(Ordering::SeqCst);
            let cleaned = resources_cleaned.load(Ordering::SeqCst);

            assert_eq!(created, 1000, "Should create 1000 resources");
            assert_eq!(cleaned, 1000, "Should clean up all resources");
        }
    }

    // Test recovery and resilience
    mod recovery_tests {
        use super::*;

        #[test]
        fn test_partial_data_recovery() {
            let mut reporter = PerformanceReporter::new();

            // Add partial/corrupted data
            let partial_suite = BenchmarkSuite {
                name: "Partial Suite".to_string(),
                version: "1.0.0".to_string(),
                commit_hash: "".to_string(), // Empty commit hash
                timestamp: chrono::Utc::now(),
                system_info: SystemInfo {
                    os: "".to_string(), // Empty OS info
                    arch: "".to_string(), // Empty arch
                    cpu_cores: 0, // Invalid CPU count
                    memory_gb: 0.0, // Invalid memory
                    rust_version: "".to_string(),
                    compiler_flags: "".to_string(),
                },
                metrics: vec![
                    create_metric("test".to_string(), "test".to_string(), 100.0, "ms".to_string(), 1, 1),
                ],
            };

            reporter.add_suite(partial_suite);

            // Should still generate a report
            let report = reporter.generate_comprehensive_report();
            assert!(!report.is_empty(), "Should recover from partial data");

            // Report should contain what data is available
            assert!(report.contains("Partial Suite"), "Should contain suite name");
            assert!(report.contains("test"), "Should contain available metric");
        }

        #[test]
        fn test_system_graceful_degradation() {
            // Test that the system degrades gracefully when resources are constrained

            // Simulate memory constraints by using large allocations
            let _large_allocation = vec![0u8; 100 * 1024 * 1024]; // 100MB

            // System should still be able to create basic components
            let generator = TestDataGenerator::new();
            assert!(generator.is_ok(), "Should create generator under memory pressure");

            let generator = generator.unwrap();
            let docs = generator.generate_documents(10, 1);
            assert_eq!(docs.len(), 10, "Should generate documents under memory pressure");

            // Should still be able to create reports
            let mut reporter = PerformanceReporter::new();
            let system_info = create_system_info();
            let suite = BenchmarkSuite {
                name: "Stress Test Suite".to_string(),
                version: "1.0.0".to_string(),
                commit_hash: "stress_test".to_string(),
                timestamp: chrono::Utc::now(),
                system_info,
                metrics: vec![create_metric("stress_test".to_string(), "test".to_string(), 100.0, "ms".to_string(), 1, 1)],
            };

            reporter.add_suite(suite);
            let report = reporter.generate_comprehensive_report();
            assert!(!report.is_empty(), "Should generate report under stress");
        }

        #[test]
        fn test_error_boundary_isolation() {
            // Test that errors in one component don't affect others

            let mut reporter = PerformanceReporter::new();

            // Add a normal suite
            let normal_suite = BenchmarkSuite {
                name: "Normal Suite".to_string(),
                version: "1.0.0".to_string(),
                commit_hash: "normal".to_string(),
                timestamp: chrono::Utc::now(),
                system_info: create_system_info(),
                metrics: vec![create_metric("normal".to_string(), "test".to_string(), 100.0, "ms".to_string(), 1, 1)],
            };

            reporter.add_suite(normal_suite);

            // Add a problematic suite
            let problematic_suite = BenchmarkSuite {
                name: "Problematic Suite".to_string(),
                version: "1.0.0".to_string(),
                commit_hash: "problematic".to_string(),
                timestamp: chrono::Utc::now(),
                system_info: create_system_info(),
                metrics: vec![
                    create_metric("problematic".to_string(), "test".to_string(), f64::NAN, "ms".to_string(), 1, 1),
                ],
            };

            reporter.add_suite(problematic_suite);

            // Should still generate report with both suites
            let report = reporter.generate_comprehensive_report();
            assert!(!report.is_empty(), "Should handle mixed normal and problematic data");

            assert!(report.contains("Normal Suite"), "Should contain normal suite");
            assert!(report.contains("Problematic Suite"), "Should contain problematic suite");
        }

        #[test]
        fn test_data_corruption_resilience() {
            // Test resilience to various forms of data corruption

            let temp_dir = TempDir::new().unwrap();
            let corrupted_file = temp_dir.path().join("corrupted.json");

            // Write corrupted JSON
            fs::write(&corrupted_file, "{ invalid json content").unwrap();

            // Try to read it
            let read_result = fs::read_to_string(&corrupted_file);
            assert!(read_result.is_ok(), "Should read file even if content is corrupted");

            let content = read_result.unwrap();
            let parse_result: Result<serde_json::Value, _> = serde_json::from_str(&content);
            assert!(parse_result.is_err(), "Should detect JSON corruption");

            // Test CSV corruption
            let corrupted_csv = temp_dir.path().join("corrupted.csv");
            fs::write(&corrupted_csv, "name,value\nitem1,100\nitem2,invalid_number").unwrap();

            let csv_content = fs::read_to_string(&corrupted_csv).unwrap();
            let lines: Vec<&str> = csv_content.lines().collect();
            assert_eq!(lines.len(), 3, "Should read corrupted CSV");

            // Try to parse the numeric value
            let invalid_number = lines[2].split(',').nth(1).unwrap();
            let parse_result: Result<f64, _> = invalid_number.parse();
            assert!(parse_result.is_err(), "Should detect invalid number in CSV");
        }
    }

    // Test extreme timing and performance edge cases
    mod timing_edge_cases {
        use super::*;

        #[test]
        fn test_zero_duration_measurements() {
            let monitor = ResourceMonitor::new();
            let zero_duration = monitor.elapsed();

            // Should handle zero/near-zero durations
            assert!(zero_duration >= Duration::ZERO, "Zero duration should be handled");
        }

        #[test]
        fn test_very_long_duration_measurements() {
            let start_time = Instant::now();

            // Simulate a very long operation (shortened for test)
            std::thread::sleep(Duration::from_millis(100));

            let long_duration = start_time.elapsed();

            // Should handle long durations correctly
            assert!(long_duration >= Duration::from_millis(100), "Long duration should be measured correctly");
            assert!(long_duration < Duration::from_secs(1), "Should not be excessively long");
        }

        #[test]
        fn test_high_frequency_measurements() {
            let mut measurements = Vec::new();

            // Take many rapid measurements
            for _ in 0..1000 {
                let start = Instant::now();
                let measurement = start.elapsed();
                measurements.push(measurement);
            }

            assert_eq!(measurements.len(), 1000, "Should handle many rapid measurements");

            // Most measurements should be very small
            let small_measurements = measurements.iter()
                .filter(|&&d| d < Duration::from_micros(100))
                .count();

            assert!(small_measurements > 900, "Most measurements should be very small");
        }

        #[test]
        fn test_timer_overflows() {
            // Test that timers handle large values without overflow
            let large_duration = Duration::from_nanos(u64::MAX);

            // Duration arithmetic should not panic
            let _half_duration = large_duration / 2;

            // Should handle very large durations in benchmarks
            let large_metric = create_metric(
                "large_duration".to_string(),
                "test".to_string(),
                large_duration.as_nanos() as f64,
                "ns".to_string(),
                1,
                1,
            );

            assert!(large_metric.value > 0.0, "Large duration should be stored as positive value");
        }
    }
}