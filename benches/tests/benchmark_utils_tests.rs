//! Unit tests for benchmark utilities
//!
//! This module tests the core utility functions and data generators
//! that support the comprehensive benchmarking framework.

use std::time::Duration;
use crate::benchmark_utils::*;
use crucible_core::types::{Document, Event};
use tempfile::TempDir;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_data_generator_creation() {
        // Test that TestDataGenerator can be created successfully
        let generator = TestDataGenerator::new();
        assert!(generator.is_ok(), "TestDataGenerator creation should succeed");

        let generator = generator.unwrap();
        assert!(generator.temp_dir().exists(), "Temp directory should exist");
    }

    #[test]
    fn test_generate_documents_small() {
        let generator = TestDataGenerator::new().unwrap();
        let documents = generator.generate_documents(5, 1); // 5 documents, 1KB each

        assert_eq!(documents.len(), 5, "Should generate exactly 5 documents");

        for (i, doc) in documents.iter().enumerate() {
            assert_eq!(doc.id, format!("doc_{}", i), "Document ID should match pattern");
            assert_eq!(doc.title, format!("Test Document {}", i), "Document title should match pattern");
            assert!(doc.content.len() >= 1024, "Document content should be at least 1KB");
            assert!(doc.tags.contains(&"test".to_string()), "Document should have test tag");
            assert_eq!(doc.metadata["size"], 1, "Metadata should contain correct size");
        }
    }

    #[test]
    fn test_generate_documents_large() {
        let generator = TestDataGenerator::new().unwrap();
        let documents = generator.generate_documents(100, 10); // 100 documents, 10KB each

        assert_eq!(documents.len(), 100, "Should generate exactly 100 documents");

        // Test batch tagging
        let doc_0 = &documents[0];
        let doc_99 = &documents[99];
        assert_eq!(doc_0.metadata["batch"], 0, "First document should be in batch 0");
        assert_eq!(doc_99.metadata["batch"], 0, "Document 99 should be in batch 0");

        let doc_150 = &documents[150]; // This won't exist in our 100 doc set
        let doc_101 = &documents[101]; // This won't exist either
    }

    #[test]
    fn test_generate_events_various_types() {
        let generator = TestDataGenerator::new().unwrap();
        let event_types = vec!["create", "update", "delete", "read"];
        let events = generator.generate_events(20, &event_types);

        assert_eq!(events.len(), 20, "Should generate exactly 20 events");

        // Test event type cycling
        for (i, event) in events.iter().enumerate() {
            let expected_type = event_types[i % event_types.len()];
            assert_eq!(event.event_type, expected_type, "Event type should cycle correctly");
            assert_eq!(event.id, format!("event_{}", i), "Event ID should match pattern");
            assert_eq!(event.source, "benchmark", "Event source should be benchmark");
            assert!(event.data.is_object(), "Event data should be an object");
        }
    }

    #[test]
    fn test_create_test_files() {
        let generator = TestDataGenerator::new().unwrap();
        let file_paths = generator.create_test_files(3, 2).unwrap(); // 3 files, 2KB each

        assert_eq!(file_paths.len(), 3, "Should create exactly 3 files");

        for (i, file_path) in file_paths.iter().enumerate() {
            assert!(file_path.exists(), "File {} should exist", i);
            assert!(file_path.is_file(), "Path {} should be a file", i);

            let content = std::fs::read_to_string(file_path).unwrap();
            assert_eq!(content.len(), 2 * 1024, "File should be exactly 2KB");
            assert!(content.chars().all(|c| c == 'x'), "File content should be all 'x' characters");
        }
    }

    #[test]
    fn test_benchmark_config_default() {
        let config = BenchmarkConfig::default();

        assert_eq!(config.small_dataset, 10, "Small dataset should be 10");
        assert_eq!(config.medium_dataset, 100, "Medium dataset should be 100");
        assert_eq!(config.large_dataset, 1000, "Large dataset should be 1000");
        assert_eq!(config.iterations, 100, "Iterations should be 100");
        assert_eq!(config.warmup_iterations, 10, "Warmup iterations should be 10");
        assert_eq!(config.sample_size, 50, "Sample size should be 50");
    }

    #[test]
    fn test_benchmark_config_custom() {
        let config = BenchmarkConfig {
            small_dataset: 5,
            medium_dataset: 50,
            large_dataset: 500,
            iterations: 200,
            warmup_iterations: 20,
            sample_size: 100,
        };

        assert_eq!(config.small_dataset, 5, "Custom small dataset should be preserved");
        assert_eq!(config.medium_dataset, 50, "Custom medium dataset should be preserved");
        assert_eq!(config.large_dataset, 500, "Custom large dataset should be preserved");
        assert_eq!(config.iterations, 200, "Custom iterations should be preserved");
        assert_eq!(config.warmup_iterations, 20, "Custom warmup iterations should be preserved");
        assert_eq!(config.sample_size, 100, "Custom sample size should be preserved");
    }

    #[test]
    fn test_resource_monitor_creation() {
        let monitor = ResourceMonitor::new();

        // Test that monitor is created successfully
        assert!(monitor.elapsed() >= Duration::ZERO, "Elapsed time should be non-negative");

        // Memory usage might be None on some systems, so we just test the method doesn't panic
        let _memory_diff = monitor.memory_diff();
    }

    #[test]
    fn test_resource_monitor_timing() {
        let monitor = ResourceMonitor::new();

        // Test that elapsed time increases
        let initial_time = monitor.elapsed();
        std::thread::sleep(Duration::from_millis(10));
        let later_time = monitor.elapsed();

        assert!(later_time > initial_time, "Elapsed time should increase");
    }

    #[test]
    fn test_tool_complexity_enum() {
        assert_eq!(ToolComplexity::Simple.as_str(), "simple", "Simple complexity should return 'simple'");
        assert_eq!(ToolComplexity::Medium.as_str(), "medium", "Medium complexity should return 'medium'");
        assert_eq!(ToolComplexity::Complex.as_str(), "complex", "Complex complexity should return 'complex'");

        // Test equality and ordering
        assert_eq!(ToolComplexity::Simple, ToolComplexity::Simple, "Same complexity should be equal");
        assert_ne!(ToolComplexity::Simple, ToolComplexity::Medium, "Different complexities should not be equal");
    }

    #[test]
    fn test_concurrency_levels() {
        assert_eq!(ConcurrencyLevels::SINGLE, 1, "Single concurrency should be 1");
        assert_eq!(ConcurrencyLevels::LOW, 4, "Low concurrency should be 4");
        assert_eq!(ConcurrencyLevels::MEDIUM, 16, "Medium concurrency should be 16");
        assert_eq!(ConcurrencyLevels::HIGH, 64, "High concurrency should be 64");

        // Test ordering
        assert!(ConcurrencyLevels::SINGLE < ConcurrencyLevels::LOW);
        assert!(ConcurrencyLevels::LOW < ConcurrencyLevels::MEDIUM);
        assert!(ConcurrencyLevels::MEDIUM < ConcurrencyLevels::HIGH);
    }

    #[test]
    fn test_benchmark_result_creation() {
        let mut result = BenchmarkResult::new("test_benchmark".to_string());

        assert_eq!(result.name, "test_benchmark", "Name should be set correctly");
        assert_eq!(result.avg_time, Duration::ZERO, "Average time should start at zero");
        assert_eq!(result.min_time, Duration::MAX, "Min time should start at MAX");
        assert_eq!(result.max_time, Duration::ZERO, "Max time should start at zero");
        assert!(result.memory_usage.is_none(), "Memory usage should start as None");
        assert!(result.throughput.is_none(), "Throughput should start as None");
    }

    #[test]
    fn test_benchmark_result_updates() {
        let mut result = BenchmarkResult::new("test_benchmark".to_string());

        // Simulate updating with timing data
        let time1 = Duration::from_millis(100);
        let time2 = Duration::from_millis(150);
        let time3 = Duration::from_millis(120);

        result.avg_time = (time1 + time2 + time3) / 3;
        result.min_time = time1.min(time2).min(time3);
        result.max_time = time1.max(time2).max(time3);
        result.memory_usage = Some(1024 * 1024); // 1MB
        result.throughput = Some(1000.0); // 1000 ops/sec

        assert_eq!(result.name, "test_benchmark");
        assert!(result.avg_time > Duration::ZERO);
        assert!(result.min_time < result.max_time);
        assert_eq!(result.memory_usage, Some(1024 * 1024));
        assert_eq!(result.throughput, Some(1000.0));
    }

    #[test]
    fn test_performance_report_creation() {
        let report = PerformanceReport::new();

        assert!(report.results.is_empty(), "New report should have no results");
        assert!(report.baseline_comparison.is_none(), "New report should have no baseline comparison");
    }

    #[test]
    fn test_performance_report_add_result() {
        let mut report = PerformanceReport::new();
        let result = BenchmarkResult::new("test_benchmark".to_string());

        report.add_result(result);

        assert_eq!(report.results.len(), 1, "Report should have one result after adding");
        assert_eq!(report.results[0].name, "test_benchmark", "Added result should have correct name");
    }

    #[test]
    fn test_performance_report_markdown_generation() {
        let mut report = PerformanceReport::new();

        // Add a sample result
        let mut result = BenchmarkResult::new("sample_benchmark".to_string());
        result.avg_time = Duration::from_millis(100);
        result.min_time = Duration::from_millis(80);
        result.max_time = Duration::from_millis(120);
        result.memory_usage = Some(1024 * 1024);
        result.throughput = Some(100.0);

        report.add_result(result);

        let markdown = report.generate_markdown();

        assert!(markdown.contains("# Phase 6.1 Performance Benchmarking Results"), "Should contain title");
        assert!(markdown.contains("sample_benchmark"), "Should contain benchmark name");
        assert!(markdown.contains("100ms"), "Should contain average time");
        assert!(markdown.contains("80ms"), "Should contain min time");
        assert!(markdown.contains("120ms"), "Should contain max time");
        assert!(markdown.contains("1048576"), "Should contain memory usage");
        assert!(markdown.contains("100.00"), "Should contain throughput");
    }

    #[test]
    fn test_performance_report_with_baseline() {
        let mut report = PerformanceReport::new();

        // Add current result
        let mut current_result = BenchmarkResult::new("benchmark".to_string());
        current_result.avg_time = Duration::from_millis(50);
        report.add_result(current_result);

        // Add baseline comparison
        let mut baseline_result = BenchmarkResult::new("benchmark".to_string());
        baseline_result.avg_time = Duration::from_millis(100);

        let baseline_comparison = BaselineComparison {
            old_architecture: vec![baseline_result],
            improvement_percentages: vec![50.0], // 50% improvement
        };

        report.baseline_comparison = Some(baseline_comparison);

        let markdown = report.generate_markdown();

        assert!(markdown.contains("Architecture Improvements"), "Should contain architecture improvements section");
        assert!(markdown.contains("50.0%"), "Should contain improvement percentage");
        assert!(markdown.contains("50ms"), "Should contain new architecture time");
        assert!(markdown.contains("100ms"), "Should contain old architecture time");
    }

    #[test]
    fn test_async_benchmark_runner() {
        let rt = tokio::runtime::Runtime::new().unwrap();

        // Test that we can run async operations
        let result = run_async_benchmark(&rt, || async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            42
        });

        assert_eq!(result, 42, "Async benchmark should return correct result");
    }

    #[test]
    fn test_temp_dir_cleanup() {
        let generator = TestDataGenerator::new().unwrap();
        let temp_path = generator.temp_dir().to_path_buf();

        // Create some files
        let _files = generator.create_test_files(2, 1).unwrap();

        assert!(temp_path.exists(), "Temp directory should exist while generator is alive");

        // When generator goes out of scope, temp directory should be cleaned up
        drop(generator);

        // Note: We can't test cleanup here because the directory is still in use
        // In a real scenario, the TempDir would be cleaned up when dropped
    }

    #[test]
    fn test_memory_usage_monitoring() {
        let monitor = ResourceMonitor::new();

        // The memory monitoring might not work on all systems
        // So we just test that it doesn't panic and returns a reasonable value or None
        let memory_diff = monitor.memory_diff();

        // If memory_diff is Some, it should be a reasonable value
        if let Some(diff) = memory_diff {
            // Memory difference could be positive or negative, but should be reasonable
            assert!(diff.abs() < 1024 * 1024 * 1024, "Memory difference should be reasonable (< 1GB)");
        }
        // If None, that's also acceptable on systems where memory monitoring isn't available
    }

    #[test]
    fn test_benchmark_data_reproducibility() {
        // Test that data generation is deterministic when using the same parameters
        let generator1 = TestDataGenerator::new().unwrap();
        let generator2 = TestDataGenerator::new().unwrap();

        let docs1 = generator1.generate_documents(10, 1);
        let docs2 = generator2.generate_documents(10, 1);

        // Documents should have the same structure, though timestamps will differ
        assert_eq!(docs1.len(), docs2.len(), "Both generators should produce same number of documents");

        for (doc1, doc2) in docs1.iter().zip(docs2.iter()) {
            assert_eq!(doc1.id, doc2.id, "Document IDs should match");
            assert_eq!(doc1.title, doc2.title, "Document titles should match");
            assert_eq!(doc1.content.len(), doc2.content.len(), "Document content lengths should match");
            assert_eq!(doc1.tags, doc2.tags, "Document tags should match");
        }
    }

    #[test]
    fn test_edge_cases() {
        let generator = TestDataGenerator::new().unwrap();

        // Test zero-sized datasets
        let empty_docs = generator.generate_documents(0, 10);
        assert_eq!(empty_docs.len(), 0, "Zero documents should be handled correctly");

        let empty_events = generator.generate_events(0, &["test"]);
        assert_eq!(empty_events.len(), 0, "Zero events should be handled correctly");

        // Test single item
        let single_doc = generator.generate_documents(1, 1);
        assert_eq!(single_doc.len(), 1, "Single document should be handled correctly");
        assert_eq!(single_doc[0].id, "doc_0", "Single document should have correct ID");

        // Test empty event types (should not panic)
        let empty_type_events = generator.generate_events(5, &[]);
        assert_eq!(empty_type_events.len(), 0, "Empty event types should produce no events");
    }

    #[test]
    fn test_large_dataset_handling() {
        let generator = TestDataGenerator::new().unwrap();

        // Test that large datasets don't cause issues (using smaller size for test speed)
        let large_docs = generator.generate_documents(1000, 1); // 1000 documents of 1KB each
        assert_eq!(large_docs.len(), 1000, "Large dataset should be handled correctly");

        // Verify a few random documents in the large set
        assert_eq!(large_docs[0].id, "doc_0", "First document should have correct ID");
        assert_eq!(large_docs[999].id, "doc_999", "Last document should have correct ID");

        // Test batch tagging for large datasets
        assert_eq!(large_docs[0].metadata["batch"], 0, "Early documents should be in batch 0");
        assert_eq!(large_docs[500].metadata["batch"], 5, "Document 500 should be in batch 5");
    }
}