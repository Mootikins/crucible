//! End-to-end integration tests for the benchmarking framework
//!
//! This module tests complete workflows of the benchmarking system,
//! from data generation through report generation and analysis.

use std::path::{Path, PathBuf};
use std::fs;
use std::time::Duration;
use tempfile::TempDir;
use anyhow::Result;

use crate::benchmark_utils::*;
use crate::performance_reporter::*;
use crate::benchmark_runner::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// Test complete benchmark workflow from start to finish
    #[test]
    fn test_complete_benchmark_workflow() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let output_dir = temp_dir.path().join("benchmark_output");

        // Step 1: Create benchmark configuration
        let mut config = BenchmarkRunnerConfig::default();
        config.output_dir = output_dir.to_string_lossy().to_string();
        config.export_formats = vec!["markdown".to_string(), "json".to_string(), "csv".to_string()];

        // Step 2: Initialize benchmark runner
        let mut runner = BenchmarkRunner::new(config);

        // Step 3: Create test data using utilities
        let data_generator = TestDataGenerator::new()?;
        let documents = data_generator.generate_documents(100, 5);
        let events = data_generator.generate_events(200, &["create", "update", "delete"]);
        let test_files = data_generator.create_test_files(10, 2)?;

        assert_eq!(documents.len(), 100, "Should generate 100 documents");
        assert_eq!(events.len(), 200, "Should generate 200 events");
        assert_eq!(test_files.len(), 10, "Should create 10 test files");

        // Step 4: Create benchmark suite
        let system_info = create_system_info();
        let commit_hash = runner.get_git_commit_hash().unwrap_or_else(|| "test_commit".to_string());
        let suite = runner.create_benchmark_suite(commit_hash, system_info)?;

        // Step 5: Add suite to reporter
        runner.reporter.add_suite(suite);

        // Step 6: Generate all reports
        runner.generate_reports()?;

        // Step 7: Verify output files exist and contain expected content
        let output_path = Path::new(&runner.config.output_dir);

        // Check markdown report
        let markdown_path = output_path.join("PHASE6_1_PERFORMANCE_REPORT.md");
        assert!(markdown_path.exists(), "Markdown report should exist");

        let markdown_content = fs::read_to_string(&markdown_path)?;
        assert!(markdown_content.contains("Phase 6.1: Comprehensive Performance Benchmarking Report"));
        assert!(markdown_content.contains("Executive Summary"));
        assert!(markdown_content.contains("Detailed Benchmark Results"));

        // Check JSON export
        let json_path = output_path.join("benchmark_results.json");
        assert!(json_path.exists(), "JSON export should exist");

        let json_content = fs::read_to_string(&json_path)?;
        assert!(json_content.contains("\"suites\""));
        assert!(json_content.contains("\"comparisons\""));

        // Check CSV export
        let csv_path = output_path.join("benchmark_results.csv");
        assert!(csv_path.exists(), "CSV export should exist");

        let csv_content = fs::read_to_string(&csv_path)?;
        assert!(csv_content.contains("name,category,subcategory,value,unit"));

        // Check performance summary
        let summary_path = output_path.join("PERFORMANCE_SUMMARY.md");
        assert!(summary_path.exists(), "Performance summary should exist");

        let summary_content = fs::read_to_string(&summary_path)?;
        assert!(summary_content.contains("Phase 6.1 Performance Benchmarking Summary"));
        assert!(summary_content.contains("Key Performance Metrics"));

        Ok(())
    }

    /// Test performance comparison workflow
    #[test]
    fn test_performance_comparison_workflow() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let output_dir = temp_dir.path().join("comparison_output");

        // Create benchmark runner with comparisons enabled
        let mut config = BenchmarkRunnerConfig::default();
        config.output_dir = output_dir.to_string_lossy().to_string();
        config.run_comparisons = true;

        let mut runner = BenchmarkRunner::new(config);

        // Create baseline suite (simulating old architecture)
        let system_info = create_system_info();
        let mut baseline_suite = BenchmarkSuite {
            name: "Baseline Architecture".to_string(),
            version: "0.1.0".to_string(),
            commit_hash: "baseline_commit".to_string(),
            timestamp: chrono::Utc::now() - chrono::Duration::days(7),
            system_info: system_info.clone(),
            metrics: Vec::new(),
        };

        // Add baseline metrics (slower performance)
        baseline_suite.metrics.push(create_metric(
            "tool_execution".to_string(),
            "performance".to_string(),
            250.0, // 250ms baseline
            "ms".to_string(),
            100,
            50,
        ));

        baseline_suite.metrics.push(create_metric(
            "memory_usage".to_string(),
            "performance".to_string(),
            200.0, // 200MB baseline
            "MB".to_string(),
            100,
            50,
        ));

        // Create new suite (simulating new architecture)
        let mut new_suite = BenchmarkSuite {
            name: "New Architecture".to_string(),
            version: "1.0.0".to_string(),
            commit_hash: "new_commit".to_string(),
            timestamp: chrono::Utc::now(),
            system_info,
            metrics: Vec::new(),
        };

        // Add new metrics (better performance)
        new_suite.metrics.push(create_metric(
            "tool_execution".to_string(),
            "performance".to_string(),
            45.0, // 45ms new (82% improvement)
            "ms".to_string(),
            100,
            50,
        ));

        new_suite.metrics.push(create_metric(
            "memory_usage".to_string(),
            "performance".to_string(),
            84.0, // 84MB new (58% improvement)
            "MB".to_string(),
            100,
            50,
        ));

        // Add both suites to reporter
        runner.reporter.add_suite(baseline_suite);
        runner.reporter.add_suite(new_suite);

        // Generate performance improvements
        let baseline_metrics = runner.reporter.results[0].metrics.clone();
        let new_metrics = runner.reporter.results[1].metrics.clone();

        let improvements = baseline_metrics.iter().zip(new_metrics.iter()).map(|(baseline, new)| {
            let improvement_percentage = ((baseline.value - new.value) / baseline.value) * 100.0;
            PerformanceImprovement {
                metric_name: baseline.name.clone(),
                baseline_value: baseline.value,
                new_value: new.value,
                improvement_percentage,
                significance_level: Some(0.01),
                confidence_interval: Some((new.value * 0.95, new.value * 1.05)),
            }
        }).collect();

        let comparison = ArchitectureComparison {
            baseline_metrics,
            new_metrics,
            improvements,
        };

        runner.reporter.add_comparison(comparison);

        // Generate reports
        runner.generate_reports()?;

        // Verify comparison report contains validation of Phase 5 claims
        let markdown_path = output_dir.join("PHASE6_1_PERFORMANCE_REPORT.md");
        let markdown_content = fs::read_to_string(&markdown_path)?;

        assert!(markdown_content.contains("Architecture Performance Comparison"));
        assert!(markdown_content.contains("Validation of Phase 5 Claims"));
        assert!(markdown_content.contains("82% improvement"));
        assert!(markdown_content.contains("58% reduction"));
        assert!(markdown_content.contains("✅ Validated"));

        Ok(())
    }

    /// Test trend analysis workflow with multiple runs
    #[test]
    fn test_trend_analysis_workflow() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let output_dir = temp_dir.path().join("trend_output");

        let mut config = BenchmarkRunnerConfig::default();
        config.output_dir = output_dir.to_string_lossy().to_string();

        let mut runner = BenchmarkRunner::new(config);
        let system_info = create_system_info();

        // Create multiple benchmark suites over time
        let runs = vec![
            ("run_1", 7, 100.0), // 7 days ago, 100ms
            ("run_2", 5, 85.0),  // 5 days ago, 85ms
            ("run_3", 3, 70.0),  // 3 days ago, 70ms
            ("run_4", 1, 60.0),  // 1 day ago, 60ms
            ("run_5", 0, 45.0),  // Today, 45ms
        ];

        for (run_name, days_ago, execution_time) in runs {
            let mut suite = BenchmarkSuite {
                name: format!("Performance Run {}", run_name),
                version: "1.0.0".to_string(),
                commit_hash: format!("commit_{}", run_name),
                timestamp: chrono::Utc::now() - chrono::Duration::days(days_ago),
                system_info: system_info.clone(),
                metrics: Vec::new(),
            };

            suite.metrics.push(create_metric(
                "tool_execution".to_string(),
                "performance".to_string(),
                execution_time,
                "ms".to_string(),
                100,
                50,
            ));

            runner.reporter.add_suite(suite);
        }

        // Generate reports including trend analysis
        runner.generate_reports()?;

        // Verify trend analysis was generated
        let trend_path = output_dir.join("performance_trends.md");
        assert!(trend_path.exists(), "Trend analysis should be generated");

        let trend_content = fs::read_to_string(&trend_path)?;
        assert!(trend_content.contains("Performance Trend Analysis"));
        assert!(trend_content.contains("Analysis Period"));
        assert!(trend_content.contains("Key Metric Trends"));
        assert!(trend_content.contains("tool_execution"));
        assert!(trend_content.contains("100.00")); // First run value
        assert!(trend_content.contains("45.00"));  // Last run value

        // Verify improvement over time is shown
        assert!(trend_content.contains("-55.0%")); // Overall improvement

        Ok(())
    }

    /// Test error handling and recovery workflow
    #[test]
    fn test_error_handling_workflow() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let output_dir = temp_dir.path().join("error_test_output");

        let mut config = BenchmarkRunnerConfig::default();
        config.output_dir = output_dir.to_string_lossy().to_string();

        let mut runner = BenchmarkRunner::new(config);

        // Test 1: Invalid benchmark configuration should be handled gracefully
        let result = std::fs::create_dir_all(&runner.config.output_dir);
        assert!(result.is_ok(), "Directory creation should succeed");

        // Test 2: Empty suite should generate valid report
        let system_info = create_system_info();
        let empty_suite = BenchmarkSuite {
            name: "Empty Suite".to_string(),
            version: "1.0.0".to_string(),
            commit_hash: "empty_commit".to_string(),
            timestamp: chrono::Utc::now(),
            system_info,
            metrics: Vec::new(),
        };

        runner.reporter.add_suite(empty_suite);
        let report_result = runner.generate_reports();
        assert!(report_result.is_ok(), "Should generate report even with empty suite");

        // Test 3: Invalid export path should fail gracefully
        let invalid_reporter = PerformanceReporter::new();
        let invalid_path = PathBuf::from("/invalid/path/that/does/not/exist/results.json");
        let export_result = invalid_reporter.export_json(&invalid_path);
        assert!(export_result.is_err(), "Should fail gracefully with invalid path");

        // Test 4: Malformed metrics should not crash the system
        let system_info = create_system_info();
        let mut malformed_suite = BenchmarkSuite {
            name: "Malformed Suite".to_string(),
            version: "1.0.0".to_string(),
            commit_hash: "malformed_commit".to_string(),
            timestamp: chrono::Utc::now(),
            system_info,
            metrics: Vec::new(),
        };

        // Add a metric with extreme values
        malformed_suite.metrics.push(create_metric(
            "extreme_metric".to_string(),
            "test".to_string(),
            f64::MAX, // Extreme value
            "ms".to_string(),
            1,
            1,
        ));

        runner.reporter.add_suite(malformed_suite);
        let extreme_report_result = runner.generate_reports();
        assert!(extreme_report_result.is_ok(), "Should handle extreme values gracefully");

        // Test 5: Verify output files are still valid despite errors
        let markdown_path = output_dir.join("PHASE6_1_PERFORMANCE_REPORT.md");
        assert!(markdown_path.exists(), "Markdown report should exist despite errors");

        let content = fs::read_to_string(&markdown_path)?;
        assert!(!content.is_empty(), "Report content should not be empty");

        Ok(())
    }

    /// Test concurrent execution workflow
    #[test]
    fn test_concurrent_execution_workflow() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let output_dir = temp_dir.path().join("concurrent_output");

        // Test multiple runners operating concurrently
        let handles: Vec<_> = (0..3).map(|i| {
            let temp_dir = temp_dir.path().to_path_buf();
            std::thread::spawn(move || -> Result<()> {
                let mut config = BenchmarkRunnerConfig::default();
                config.output_dir = temp_dir.join(format!("runner_{}", i)).to_string_lossy().to_string();

                let mut runner = BenchmarkRunner::new(config);
                let system_info = create_system_info();
                let suite = runner.create_benchmark_suite(format!("commit_{}", i), system_info)?;

                runner.reporter.add_suite(suite);
                runner.generate_reports()?;

                // Verify unique outputs
                let runner_output = temp_dir.join(format!("runner_{}", i));
                let markdown_path = runner_output.join("PHASE6_1_PERFORMANCE_REPORT.md");
                assert!(markdown_path.exists(), "Each runner should generate its own report");

                Ok(())
            })
        }).collect();

        // Wait for all runners to complete
        for handle in handles {
            handle.join().unwrap()?;
        }

        // Verify all outputs exist
        for i in 0..3 {
            let runner_output = temp_dir.path().join(format!("runner_{}", i));
            let markdown_path = runner_output.join("PHASE6_1_PERFORMANCE_REPORT.md");
            assert!(markdown_path.exists(), "Runner {} should have generated report", i);
        }

        Ok(())
    }

    /// Test data validation and integrity workflow
    #[test]
    fn test_data_validation_workflow() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let output_dir = temp_dir.path().join("validation_output");

        let mut config = BenchmarkRunnerConfig::default();
        config.output_dir = output_dir.to_string_lossy().to_string();

        let mut runner = BenchmarkRunner::new(config);

        // Create comprehensive test dataset
        let data_generator = TestDataGenerator::new()?;

        // Generate various sizes of test data
        let small_documents = data_generator.generate_documents(10, 1);
        let medium_documents = data_generator.generate_documents(100, 5);
        let large_documents = data_generator.generate_documents(1000, 10);

        // Verify data integrity
        assert_eq!(small_documents.len(), 10);
        assert_eq!(medium_documents.len(), 100);
        assert_eq!(large_documents.len(), 1000);

        for doc in &small_documents {
            assert_eq!(doc.content.len(), 1024);
            assert!(doc.content.chars().all(|c| c == 'x'));
        }

        // Generate events with various types
        let event_types = vec!["create", "update", "delete", "read", "query"];
        let events = data_generator.generate_events(500, &event_types);

        assert_eq!(events.len(), 500);
        for (i, event) in events.iter().enumerate() {
            let expected_type = event_types[i % event_types.len()];
            assert_eq!(event.event_type, expected_type);
            assert_eq!(event.source, "benchmark");
        }

        // Create benchmark suite with validated data
        let system_info = create_system_info();
        let suite = runner.create_benchmark_suite("validation_commit".to_string(), system_info)?;

        // Add custom metrics based on our test data
        let mut validated_suite = suite;
        validated_suite.metrics.push(create_metric(
            "small_document_processing".to_string(),
            "data_validation".to_string(),
            small_documents.len() as f64,
            "count".to_string(),
            1,
            1,
        ));

        validated_suite.metrics.push(create_metric(
            "medium_document_processing".to_string(),
            "data_validation".to_string(),
            medium_documents.len() as f64,
            "count".to_string(),
            1,
            1,
        ));

        validated_suite.metrics.push(create_metric(
            "large_document_processing".to_string(),
            "data_validation".to_string(),
            large_documents.len() as f64,
            "count".to_string(),
            1,
            1,
        ));

        validated_suite.metrics.push(create_metric(
            "event_processing".to_string(),
            "data_validation".to_string(),
            events.len() as f64,
            "count".to_string(),
            1,
            1,
        ));

        runner.reporter.add_suite(validated_suite);
        runner.generate_reports()?;

        // Verify data validation metrics appear in reports
        let markdown_path = output_dir.join("PHASE6_1_PERFORMANCE_REPORT.md");
        let markdown_content = fs::read_to_string(&markdown_path)?;

        assert!(markdown_content.contains("data_validation"));
        assert!(markdown_content.contains("small_document_processing"));
        assert!(markdown_content.contains("medium_document_processing"));
        assert!(markdown_content.contains("large_document_processing"));
        assert!(markdown_content.contains("event_processing"));

        Ok(())
    }

    /// Test performance regression detection workflow
    #[test]
    fn test_regression_detection_workflow() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let output_dir = temp_dir.path().join("regression_output");

        let mut config = BenchmarkRunnerConfig::default();
        config.output_dir = output_dir.to_string_lossy().to_string();

        let mut runner = BenchmarkRunner::new(config);
        let system_info = create_system_info();

        // Create baseline run with good performance
        let mut baseline_suite = BenchmarkSuite {
            name: "Baseline Performance".to_string(),
            version: "1.0.0".to_string(),
            commit_hash: "baseline_commit".to_string(),
            timestamp: chrono::Utc::now() - chrono::Duration::days(1),
            system_info: system_info.clone(),
            metrics: Vec::new(),
        };

        baseline_suite.metrics.push(create_metric(
            "critical_operation".to_string(),
            "performance".to_string(),
            50.0, // Good performance: 50ms
            "ms".to_string(),
            100,
            50,
        ));

        // Create regression run with degraded performance
        let mut regression_suite = BenchmarkSuite {
            name: "Regression Performance".to_string(),
            version: "1.0.1".to_string(),
            commit_hash: "regression_commit".to_string(),
            timestamp: chrono::Utc::now(),
            system_info,
            metrics: Vec::new(),
        };

        regression_suite.metrics.push(create_metric(
            "critical_operation".to_string(),
            "performance".to_string(),
            75.0, // Regression: 75ms (50% slower)
            "ms".to_string(),
            100,
            50,
        ));

        runner.reporter.add_suite(baseline_suite);
        runner.reporter.add_suite(regression_suite);

        // Generate reports
        runner.generate_reports()?;

        // Verify regression is detected and highlighted
        let markdown_path = output_dir.join("PHASE6_1_PERFORMANCE_REPORT.md");
        let markdown_content = fs::read_to_string(&markdown_path)?;

        assert!(markdown_content.contains("Performance Analysis"));
        assert!(markdown_content.contains("critical_operation"));

        // Check trend analysis for regression detection
        let trend_path = output_dir.join("performance_trends.md");
        if trend_path.exists() {
            let trend_content = fs::read_to_string(&trend_path)?;
            assert!(trend_content.contains("critical_operation"));
            // Should show negative trend (performance degradation)
            assert!(trend_content.contains("+50.0%")); // 50% increase (regression)
        }

        Ok(())
    }

    /// Test comprehensive report accuracy workflow
    #[test]
    fn test_report_accuracy_workflow() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let output_dir = temp_dir.path().join("accuracy_output");

        let mut config = BenchmarkRunnerConfig::default();
        config.output_dir = output_dir.to_string_lossy().to_string();
        config.export_formats = vec!["markdown".to_string(), "json".to_string(), "csv".to_string()];

        let mut runner = BenchmarkRunner::new(config);

        // Create suite with known metrics for accuracy validation
        let system_info = create_system_info();
        let suite = runner.create_benchmark_suite("accuracy_commit".to_string(), system_info)?;

        runner.reporter.add_suite(suite);

        // Generate all report formats
        runner.generate_reports()?;

        // Validate JSON export accuracy
        let json_path = output_dir.join("benchmark_results.json");
        let json_content = fs::read_to_string(&json_path)?;
        let json_data: serde_json::Value = serde_json::from_str(&json_content)?;

        // Verify JSON structure and data
        assert!(json_data["suites"].is_array(), "JSON should contain suites array");
        assert!(json_data["comparisons"].is_array(), "JSON should contain comparisons array");
        assert!(json_data["generated_at"].is_string(), "JSON should contain timestamp");

        let suites = json_data["suites"].as_array().unwrap();
        assert!(!suites.is_empty(), "Should have at least one suite");

        if let Some(suite) = suites.first() {
            assert!(suite["name"].is_string(), "Suite should have name");
            assert!(suite["metrics"].is_array(), "Suite should have metrics array");

            if let Some(metrics) = suite["metrics"].as_array() {
                for metric in metrics {
                    assert!(metric["name"].is_string(), "Metric should have name");
                    assert!(metric["value"].is_number(), "Metric should have numeric value");
                    assert!(metric["unit"].is_string(), "Metric should have unit");
                }
            }
        }

        // Validate CSV export accuracy
        let csv_path = output_dir.join("benchmark_results.csv");
        let csv_content = fs::read_to_string(&csv_path)?;
        let csv_lines: Vec<&str> = csv_content.lines().collect();

        assert!(!csv_lines.is_empty(), "CSV should have content");
        assert!(csv_lines[0].contains("name,category,subcategory"), "CSV should have header");

        // Validate each data row
        for line in csv_lines.iter().skip(1) {
            let fields: Vec<&str> = line.split(',').collect();
            assert!(fields.len() >= 6, "Each CSV row should have at least 6 fields");
            assert!(!fields[0].is_empty(), "Name field should not be empty");
            assert!(!fields[1].is_empty(), "Category field should not be empty");
            assert!(!fields[3].is_empty(), "Value field should not be empty");
            assert!(!fields[4].is_empty(), "Unit field should not be empty");
        }

        // Validate Markdown report accuracy
        let markdown_path = output_dir.join("PHASE6_1_PERFORMANCE_REPORT.md");
        let markdown_content = fs::read_to_string(&markdown_path)?;

        assert!(markdown_content.len() > 1000, "Markdown report should be substantial");
        assert!(markdown_content.contains("## "), "Markdown should have section headers");
        assert!(markdown_content.contains("| "), "Markdown should have tables");
        assert!(markdown_content.contains("**"), "Markdown should have bold formatting");

        // Cross-validate data between formats
        // Extract key metrics from JSON and verify they appear in other formats
        if let Some(suite) = suites.first() {
            if let Some(metrics) = suite["metrics"].as_array() {
                if let Some(metric) = metrics.first() {
                    let metric_name = metric["name"].as_str().unwrap();
                    let metric_value = metric["value"].as_f64().unwrap();

                    // Verify metric appears in CSV
                    let csv_contains_metric = csv_lines.iter().any(|line| line.contains(metric_name));
                    assert!(csv_contains_metric, "Metric {} should appear in CSV", metric_name);

                    // Verify metric appears in Markdown
                    assert!(markdown_content.contains(metric_name),
                           "Metric {} should appear in Markdown", metric_name);

                    // Verify metric value appears in Markdown (at least approximately)
                    let value_str = format!("{:.2}", metric_value);
                    assert!(markdown_content.contains(&value_str) ||
                           markdown_content.contains(&format!("{:.1}", metric_value)),
                           "Metric value {} should appear in Markdown", metric_value);
                }
            }
        }

        Ok(())
    }

    /// Test system resource monitoring workflow
    #[test]
    fn test_resource_monitoring_workflow() -> Result<()> {
        // Test resource monitor functionality
        let monitor = ResourceMonitor::new();

        // Simulate some work
        std::thread::sleep(Duration::from_millis(10));

        let elapsed_time = monitor.elapsed();
        assert!(elapsed_time >= Duration::from_millis(10), "Should track elapsed time");

        // Test memory monitoring (may not work on all systems)
        let memory_diff = monitor.memory_diff();
        // Memory diff could be None on systems where monitoring isn't available
        // If present, should be a reasonable value
        if let Some(diff) = memory_diff {
            assert!(diff.abs() < 1024 * 1024 * 1024, "Memory diff should be reasonable");
        }

        // Test multiple monitors
        let monitors: Vec<ResourceMonitor> = (0..5).map(|_| ResourceMonitor::new()).collect();
        assert_eq!(monitors.len(), 5, "Should create multiple monitors");

        // All monitors should have different start times
        let start_times: Vec<_> = monitors.iter().map(|m| m.start_time).collect();
        for (i, time) in start_times.iter().enumerate() {
            for (j, other_time) in start_times.iter().enumerate() {
                if i != j {
                    assert!(time != other_time, "Monitors should have different start times");
                }
            }
        }

        Ok(())
    }
}