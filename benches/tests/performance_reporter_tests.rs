//! Unit tests for performance reporter with statistical validation
//!
//! This module tests the performance analysis, reporting, and statistical
//! calculation components of the benchmarking framework.

use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;
use chrono::Utc;
use crate::performance_reporter::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_metric(name: &str, category: &str, value: f64, unit: &str) -> BenchmarkMetric {
        BenchmarkMetric {
            name: name.to_string(),
            category: category.to_string(),
            subcategory: None,
            value,
            unit: unit.to_string(),
            iterations: 100,
            sample_size: 50,
            std_deviation: Some(value * 0.1), // 10% std deviation
            min_value: Some(value * 0.8),
            max_value: Some(value * 1.2),
            percentile_95: Some(value * 1.1),
            memory_usage_mb: Some(value / 100.0), // Arbitrary memory usage
            timestamp: Utc::now(),
        }
    }

    fn create_test_system_info() -> SystemInfo {
        SystemInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_cores: 8,
            memory_gb: 16.0,
            rust_version: "1.70.0".to_string(),
            compiler_flags: "-O3".to_string(),
        }
    }

    fn create_test_suite() -> BenchmarkSuite {
        let mut suite = BenchmarkSuite {
            name: "Test Suite".to_string(),
            version: "1.0.0".to_string(),
            commit_hash: "abc123".to_string(),
            timestamp: Utc::now(),
            system_info: create_test_system_info(),
            metrics: Vec::new(),
        };

        // Add various test metrics
        suite.metrics.push(create_test_metric("simple_tool", "script_engine", 45.0, "ms"));
        suite.metrics.push(create_test_metric("complex_tool", "script_engine", 350.0, "ms"));
        suite.metrics.push(create_test_metric("cli_startup", "cli", 50.0, "ms"));
        suite.metrics.push(create_test_metric("event_routing", "daemon", 25.0, "ms"));
        suite.metrics.push(create_test_metric("binary_size", "system", 58.0 * 1024.0 * 1024.0, "bytes"));
        suite.metrics.push(create_test_metric("memory_usage", "system", 85.0, "MB"));

        suite
    }

    #[test]
    fn test_performance_reporter_creation() {
        let reporter = PerformanceReporter::new();
        assert!(reporter.results.is_empty(), "New reporter should have no results");
        assert!(reporter.comparisons.is_empty(), "New reporter should have no comparisons");
    }

    #[test]
    fn test_performance_reporter_default() {
        let reporter = PerformanceReporter::default();
        assert!(reporter.results.is_empty(), "Default reporter should have no results");
        assert!(reporter.comparisons.is_empty(), "Default reporter should have no comparisons");
    }

    #[test]
    fn test_add_suite() {
        let mut reporter = PerformanceReporter::new();
        let suite = create_test_suite();

        reporter.add_suite(suite);

        assert_eq!(reporter.results.len(), 1, "Should have one suite after adding");
        assert_eq!(reporter.results[0].name, "Test Suite");
        assert_eq!(reporter.results[0].metrics.len(), 6);
    }

    #[test]
    fn test_add_comparison() {
        let mut reporter = PerformanceReporter::new();
        let comparison = ArchitectureComparison {
            baseline_metrics: vec![create_test_metric("test", "category", 100.0, "ms")],
            new_metrics: vec![create_test_metric("test", "category", 50.0, "ms")],
            improvements: vec![PerformanceImprovement {
                metric_name: "test".to_string(),
                baseline_value: 100.0,
                new_value: 50.0,
                improvement_percentage: 50.0,
                significance_level: Some(0.05),
                confidence_interval: Some((45.0, 55.0)),
            }],
        };

        reporter.add_comparison(comparison);

        assert_eq!(reporter.comparisons.len(), 1, "Should have one comparison after adding");
        assert_eq!(reporter.comparisons[0].improvements.len(), 1);
    }

    #[test]
    fn test_generate_comprehensive_report() {
        let mut reporter = PerformanceReporter::new();
        reporter.add_suite(create_test_suite());

        let report = reporter.generate_comprehensive_report();

        assert!(report.contains("Phase 6.1: Comprehensive Performance Benchmarking Report"),
                "Report should contain title");
        assert!(report.contains("Executive Summary"), "Report should contain executive summary");
        assert!(report.contains("Detailed Benchmark Results"), "Report should contain detailed results");
        assert!(report.contains("Performance Analysis"), "Report should contain performance analysis");
        assert!(report.contains("Optimization Recommendations"), "Report should contain recommendations");
        assert!(report.contains("Statistical Analysis"), "Report should contain statistical analysis");
    }

    #[test]
    fn test_generate_executive_summary() {
        let mut reporter = PerformanceReporter::new();
        let suite = create_test_suite();
        reporter.add_suite(suite);

        let summary = reporter.generate_executive_summary();

        assert!(summary.contains("Test Suite v1.0.0"), "Summary should contain suite name and version");
        assert!(summary.contains("abc123"), "Summary should contain commit hash");
        assert!(summary.contains("linux (x86_64)"), "Summary should contain system info");
        assert!(summary.contains("8 cores"), "Summary should contain CPU cores");
        assert!(summary.contains("16.0GB RAM"), "Summary should contain memory");
        assert!(summary.contains("Total Benchmarks: 6"), "Summary should contain total benchmarks");
    }

    #[test]
    fn test_generate_architecture_comparison() {
        let mut reporter = PerformanceReporter::new();

        // Add a comparison
        let comparison = ArchitectureComparison {
            baseline_metrics: vec![
                create_test_metric("tool_execution", "performance", 100.0, "ms"),
                create_test_metric("memory", "performance", 200.0, "MB"),
            ],
            new_metrics: vec![
                create_test_metric("tool_execution", "performance", 18.0, "ms"), // 82% improvement
                create_test_metric("memory", "performance", 84.0, "MB"), // 58% improvement
            ],
            improvements: vec![
                PerformanceImprovement {
                    metric_name: "tool_execution".to_string(),
                    baseline_value: 100.0,
                    new_value: 18.0,
                    improvement_percentage: 82.0,
                    significance_level: Some(0.01),
                    confidence_interval: Some((15.0, 21.0)),
                },
                PerformanceImprovement {
                    metric_name: "memory".to_string(),
                    baseline_value: 200.0,
                    new_value: 84.0,
                    improvement_percentage: 58.0,
                    significance_level: Some(0.01),
                    confidence_interval: Some((80.0, 88.0)),
                },
            ],
        };

        reporter.add_comparison(comparison);
        let comparison_section = reporter.generate_architecture_comparison();

        assert!(comparison_section.contains("Performance Improvements"), "Should contain improvements header");
        assert!(comparison_section.contains("tool_execution"), "Should contain tool execution metric");
        assert!(comparison_section.contains("memory"), "Should contain memory metric");
        assert!(comparison_section.contains("82.0%"), "Should contain 82% improvement");
        assert!(comparison_section.contains("58.0%"), "Should contain 58% improvement");
        assert!(comparison_section.contains("Validation of Phase 5 Claims"), "Should contain validation section");
        assert!(comparison_section.contains("✅ Validated"), "Should contain validation checkmarks");
    }

    #[test]
    fn test_generate_detailed_results() {
        let mut reporter = PerformanceReporter::new();
        reporter.add_suite(create_test_suite());

        let detailed = reporter.generate_detailed_results();

        assert!(detailed.contains("script_engine"), "Should contain script engine category");
        assert!(detailed.contains("cli"), "Should contain CLI category");
        assert!(detailed.contains("daemon"), "Should contain daemon category");
        assert!(detailed.contains("system"), "Should contain system category");

        // Check table structure
        assert!(detailed.contains("| Benchmark | Value | Unit | Memory (MB) | Iterations |"),
                "Should contain table header");
        assert!(detailed.contains("simple_tool"), "Should contain simple tool metric");
        assert!(detailed.contains("45.00"), "Should contain metric value");
        assert!(detailed.contains("ms"), "Should contain unit");
    }

    #[test]
    fn test_generate_performance_analysis() {
        let mut reporter = PerformanceReporter::new();
        let suite = create_test_suite();
        reporter.add_suite(suite);

        let analysis = reporter.generate_performance_analysis();

        assert!(analysis.contains("Performance Characteristics"), "Should contain characteristics header");
        assert!(analysis.contains("Performance Bottlenecks"), "Should contain bottlenecks header");

        // Should analyze timing metrics
        assert!(analysis.contains("script_engine"), "Should analyze script engine performance");
        assert!(analysis.contains("cli"), "Should analyze CLI performance");
        assert!(analysis.contains("daemon"), "Should analyze daemon performance");
    }

    #[test]
    fn test_generate_recommendations() {
        let reporter = PerformanceReporter::new();
        let recommendations = reporter.generate_recommendations();

        assert!(recommendations.contains("Performance Optimization Recommendations"),
                "Should contain recommendations header");
        assert!(recommendations.contains("High Priority"), "Should contain high priority section");
        assert!(recommendations.contains("Medium Priority"), "Should contain medium priority section");
        assert!(recommendations.contains("Low Priority"), "Should contain low priority section");
        assert!(recommendations.contains("Continuous Monitoring"), "Should contain monitoring section");

        // Check specific recommendations
        assert!(recommendations.contains("optimizing the slowest 10%"), "Should mention slowest optimization");
        assert!(recommendations.contains("memory pooling"), "Should mention memory pooling");
        assert!(recommendations.contains("lazy loading"), "Should mention lazy loading");
        assert!(recommendations.contains("caching strategies"), "Should mention caching");
    }

    #[test]
    fn test_generate_statistical_analysis() {
        let mut reporter = PerformanceReporter::new();
        let suite = create_test_suite();
        reporter.add_suite(suite);

        let stats = reporter.generate_statistical_analysis();

        assert!(stats.contains("Statistical Summary"), "Should contain statistical summary header");
        assert!(stats.contains("Benchmark Reliability"), "Should contain reliability section");

        // Check statistical calculations
        assert!(stats.contains("Mean:"), "Should contain mean calculation");
        assert!(stats.contains("Median:"), "Should contain median calculation");
        assert!(stats.contains("Standard Deviation:"), "Should contain std deviation");
        assert!(stats.contains("Range:"), "Should contain range");
        assert!(stats.contains("Total Samples:"), "Should contain total samples");
        assert!(stats.contains("Average Sample Size:"), "Should contain average sample size");
        assert!(stats.contains("Confidence Level: 95%"), "Should contain confidence level");
    }

    #[test]
    fn test_export_json() {
        let mut reporter = PerformanceReporter::new();
        reporter.add_suite(create_test_suite());

        let temp_dir = TempDir::new().unwrap();
        let json_path = temp_dir.path().join("test_results.json");

        let result = reporter.export_json(&json_path);
        assert!(result.is_ok(), "JSON export should succeed");

        let content = std::fs::read_to_string(&json_path).unwrap();
        assert!(content.contains("\"suites\":"), "JSON should contain suites array");
        assert!(content.contains("\"comparisons\":"), "JSON should contain comparisons array");
        assert!(content.contains("\"generated_at\":"), "JSON should contain generated timestamp");
        assert!(content.contains("Test Suite"), "JSON should contain suite name");
    }

    #[test]
    fn test_export_csv() {
        let mut reporter = PerformanceReporter::new();
        reporter.add_suite(create_test_suite());

        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("test_results.csv");

        let result = reporter.export_csv(&csv_path);
        assert!(result.is_ok(), "CSV export should succeed");

        let content = std::fs::read_to_string(&csv_path).unwrap();

        // Check CSV header
        assert!(content.contains("name,category,subcategory,value,unit,iterations,sample_size"),
                "CSV should contain header");

        // Check data rows
        assert!(content.contains("simple_tool,script_engine"), "CSV should contain simple tool data");
        assert!(content.contains("complex_tool,script_engine"), "CSV should contain complex tool data");
        assert!(content.contains("cli_startup,cli"), "CSV should contain CLI startup data");
    }

    #[test]
    fn test_generate_trend_analysis_insufficient_data() {
        let reporter = PerformanceReporter::new();
        let trends = reporter.generate_trend_analysis();

        assert!(trends.contains("Insufficient data for trend analysis"),
                "Should indicate insufficient data when only one run");
    }

    #[test]
    fn test_generate_trend_analysis_with_data() {
        let mut reporter = PerformanceReporter::new();

        // Add two suites with different timestamps
        let mut suite1 = create_test_suite();
        suite1.timestamp = Utc::now() - chrono::Duration::days(7);
        suite1.metrics[0].value = 100.0; // Different value for trending

        let mut suite2 = create_test_suite();
        suite2.metrics[0].value = 50.0; // Improved value

        reporter.add_suite(suite1);
        reporter.add_suite(suite2);

        let trends = reporter.generate_trend_analysis();

        assert!(trends.contains("Performance Trend Analysis"), "Should contain trend analysis header");
        assert!(trends.contains("Analysis Period"), "Should contain analysis period");
        assert!(trends.contains("Key Metric Trends"), "Should contain key metrics section");
        assert!(trends.contains("| Metric | First Run | Latest Run | Change % |"),
                "Should contain trend table header");
    }

    #[test]
    fn test_create_system_info() {
        let system_info = create_system_info();

        assert!(!system_info.os.is_empty(), "OS should not be empty");
        assert!(!system_info.arch.is_empty(), "Architecture should not be empty");
        assert!(system_info.cpu_cores > 0, "CPU cores should be positive");
        assert!(system_info.memory_gb > 0.0, "Memory should be positive");
        assert!(!system_info.rust_version.is_empty(), "Rust version should not be empty");
        assert!(!system_info.compiler_flags.is_empty(), "Compiler flags should not be empty");
    }

    #[test]
    fn test_create_metric() {
        let metric = create_metric(
            "test_metric".to_string(),
            "test_category".to_string(),
            100.0,
            "ms".to_string(),
            50,
            25,
        );

        assert_eq!(metric.name, "test_metric");
        assert_eq!(metric.category, "test_category");
        assert_eq!(metric.value, 100.0);
        assert_eq!(metric.unit, "ms");
        assert_eq!(metric.iterations, 50);
        assert_eq!(metric.sample_size, 25);
        assert!(metric.subcategory.is_none());
        assert!(metric.std_deviation.is_none());
        assert!(metric.min_value.is_none());
        assert!(metric.max_value.is_none());
        assert!(metric.percentile_95.is_none());
        assert!(metric.memory_usage_mb.is_none());
    }

    #[test]
    fn test_statistical_calculations() {
        let mut reporter = PerformanceReporter::new();
        let suite = create_test_suite();
        reporter.add_suite(suite);

        let stats = reporter.generate_statistical_analysis();

        // Verify that statistical calculations are present and reasonable
        assert!(stats.contains("Mean:"), "Should calculate mean");
        assert!(stats.contains("Median:"), "Should calculate median");
        assert!(stats.contains("Standard Deviation:"), "Should calculate standard deviation");

        // Parse the mean value and verify it's reasonable
        let mean_line = stats.lines().find(|line| line.contains("Mean:")).unwrap();
        let mean_value: f64 = mean_line.split(":").nth(1).unwrap().trim().trim_end_matches("ms").parse().unwrap();
        assert!(mean_value > 0.0, "Mean should be positive");
        assert!(mean_value < 1000.0, "Mean should be reasonable (< 1000ms for our test data)");
    }

    #[test]
    fn test_performance_improvement_validation() {
        let mut reporter = PerformanceReporter::new();

        // Add a comparison with Phase 5 target improvements
        let comparison = ArchitectureComparison {
            baseline_metrics: vec![create_test_metric("tool_execution", "performance", 250.0, "ms")],
            new_metrics: vec![create_test_metric("tool_execution", "performance", 45.0, "ms")],
            improvements: vec![PerformanceImprovement {
                metric_name: "tool_execution".to_string(),
                baseline_value: 250.0,
                new_value: 45.0,
                improvement_percentage: 82.0, // Exactly the Phase 5 claim
                significance_level: Some(0.01),
                confidence_interval: Some((40.0, 50.0)),
            }],
        };

        reporter.add_comparison(comparison);
        let comparison_section = reporter.generate_architecture_comparison();

        // Should validate the 82% improvement claim
        assert!(comparison_section.contains("Tool Execution Speed: Claimed 82%, Measured 82.0% ✅ Validated"),
                "Should validate 82% improvement when exactly matching");
    }

    #[test]
    fn test_performance_improvement_near_miss() {
        let mut reporter = PerformanceReporter::new();

        // Add a comparison with slightly lower than target improvement
        let comparison = ArchitectureComparison {
            baseline_metrics: vec![create_test_metric("tool_execution", "performance", 250.0, "ms")],
            new_metrics: vec![create_test_metric("tool_execution", "performance", 50.0, "ms")],
            improvements: vec![PerformanceImprovement {
                metric_name: "tool_execution".to_string(),
                baseline_value: 250.0,
                new_value: 50.0,
                improvement_percentage: 80.0, // Slightly less than 82% target
                significance_level: Some(0.01),
                confidence_interval: Some((45.0, 55.0)),
            }],
        };

        reporter.add_comparison(comparison);
        let comparison_section = reporter.generate_architecture_comparison();

        // Should show warning for near miss (within 90% of target)
        assert!(comparison_section.contains("Tool Execution Speed: Claimed 82%, Measured 80.0% ⚠️  Difference"),
                "Should show warning for near-miss improvement");
    }

    #[test]
    fn test_error_handling_invalid_export_path() {
        let reporter = PerformanceReporter::new();
        let invalid_path = PathBuf::from("/invalid/path/that/does/not/exist/results.json");

        let result = reporter.export_json(&invalid_path);
        assert!(result.is_err(), "Should fail when trying to export to invalid path");
    }

    #[test]
    fn test_empty_report_generation() {
        let reporter = PerformanceReporter::new();
        let report = reporter.generate_comprehensive_report();

        // Should still generate a report even with no data
        assert!(report.contains("Phase 6.1: Comprehensive Performance Benchmarking Report"),
                "Should generate report header even with no data");
        assert!(report.contains("Executive Summary"), "Should contain executive summary section");
    }

    #[test]
    fn test_large_dataset_handling() {
        let mut reporter = PerformanceReporter::new();
        let mut suite = create_test_suite();

        // Add many metrics to test large dataset handling
        for i in 0..1000 {
            suite.metrics.push(create_test_metric(
                &format!("metric_{}", i),
                "large_test_category",
                i as f64,
                "ms"
            ));
        }

        reporter.add_suite(suite);

        let report = reporter.generate_comprehensive_report();
        assert!(!report.is_empty(), "Should handle large datasets without issues");

        let detailed = reporter.generate_detailed_results();
        assert!(detailed.contains("large_test_category"), "Should handle large categories");
    }

    #[test]
    fn test_memory_usage_analysis() {
        let mut reporter = PerformanceReporter::new();
        let suite = create_test_suite();
        reporter.add_suite(suite);

        let summary = reporter.generate_executive_summary();

        // Should analyze memory usage across metrics
        assert!(summary.contains("Average Memory Usage"), "Should contain memory usage analysis");
    }

    #[test]
    fn test_bottleneck_identification() {
        let mut reporter = PerformanceReporter::new();
        let mut suite = create_test_suite();

        // Add some slow benchmarks to test bottleneck identification
        suite.metrics.push(create_test_metric("slow_benchmark", "performance", 500.0, "ms"));
        suite.metrics.push(create_test_metric("very_slow_benchmark", "performance", 1000.0, "ms"));

        reporter.add_suite(suite);

        let analysis = reporter.generate_performance_analysis();
        assert!(analysis.contains("Performance Bottlenecks"), "Should contain bottlenecks section");
        assert!(analysis.contains("slow_benchmark"), "Should identify slow benchmarks");
        assert!(analysis.contains("Consider optimization"), "Should suggest optimization");
    }
}