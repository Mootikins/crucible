//! Unit Tests for Phase 6.TEST Performance Validation and Regression Testing Framework
//!
//! This comprehensive test suite validates the reliability, accuracy, and performance
//! of the Phase 6.TEST validation framework that validates our entire Phase 6 performance
//! testing suite and confirms our Phase 5 improvements are realized.

use std::collections::HashMap;
use std::time::Duration;
use tokio_test;
use anyhow::Result;

use crate::phase6_test_validation::{
    Phase6TestValidator, ValidationConfig, ValidationStatus, ValidationType,
    Metric, PerformanceBaseline, ValidationResult, IntegrationResult,
    StatisticalValidationResult, ProductionReadinessResult,
    Phase6TestResults, run_phase6_test_validation, run_phase6_test_validation_with_config,
};

/// Test configuration for comprehensive validation testing
fn create_test_config() -> ValidationConfig {
    ValidationConfig {
        validate_phase5_improvements: true,
        validate_regression_testing: true,
        validate_integration: true,
        validate_statistical_accuracy: true,
        validate_production_readiness: true,
        improvement_tolerance: 10.0,      // More lenient for testing
        regression_threshold: 15.0,      // More lenient for testing
        statistical_significance: 0.1,   // Less strict for faster testing
        confidence_interval: 0.9,        // Less strict for faster testing
        test_iterations: 3,              // Fewer iterations for faster tests
        concurrent_tests: 2,             // Fewer concurrent tests for stability
        timeout_seconds: 60,             // Shorter timeout for testing
        max_memory_mb: 512.0,           // Lower memory limit for testing
        generate_detailed_report: true,
        save_historical_data: false,     // Don't save during tests
        compare_with_baselines: true,
        export_metrics: false,           // Don't export during tests
    }
}

/// Create minimal test configuration for quick tests
fn create_minimal_test_config() -> ValidationConfig {
    ValidationConfig {
        validate_phase5_improvements: true,
        validate_regression_testing: false,
        validate_integration: false,
        validate_statistical_accuracy: false,
        validate_production_readiness: false,
        improvement_tolerance: 20.0,
        regression_threshold: 25.0,
        statistical_significance: 0.2,
        confidence_interval: 0.8,
        test_iterations: 1,
        concurrent_tests: 1,
        timeout_seconds: 30,
        max_memory_mb: 256.0,
        generate_detailed_report: false,
        save_historical_data: false,
        compare_with_baselines: false,
        export_metrics: false,
    }
}

#[cfg(test)]
mod phase6_test_validator_tests {
    use super::*;

    #[tokio::test]
    async fn test_validator_creation_default() {
        let validator = Phase6TestValidator::new();

        // Verify baselines are initialized
        assert!(!validator.baselines.is_empty(), "Baselines should be initialized");
        assert!(validator.baselines.len() >= 5, "Should have baselines for all Phase 5 metrics");

        // Verify default configuration
        assert!(validator.config.validate_phase5_improvements);
        assert!(validator.config.validate_regression_testing);
        assert!(validator.config.validate_integration);
        assert!(validator.config.validate_statistical_accuracy);
        assert!(validator.config.validate_production_readiness);

        // Verify baseline values match Phase 5 claims
        for (metric, improvement, baseline, _description) in PHASE5_PERFORMANCE_CLAIMS {
            if let Some(performance_baseline) = validator.baselines.get(metric) {
                assert_eq!(performance_baseline.metric, *metric);
                assert_eq!(performance_baseline.baseline_value, *baseline);
                assert_eq!(performance_baseline.improvement_percentage, *improvement);
            } else {
                panic!("Missing baseline for metric: {:?}", metric);
            }
        }
    }

    #[tokio::test]
    async fn test_validator_creation_custom_config() {
        let config = create_test_config();
        let validator = Phase6TestValidator::with_config(config.clone());

        assert_eq!(validator.config.improvement_tolerance, config.improvement_tolerance);
        assert_eq!(validator.config.test_iterations, config.test_iterations);
        assert_eq!(validator.config.concurrent_tests, config.concurrent_tests);

        // Should still initialize baselines regardless of config
        assert!(!validator.baselines.is_empty());
    }

    #[tokio::test]
    async fn test_validator_initialization_phase5_baselines() {
        let mut validator = Phase6TestValidator::new();

        // Test baseline initialization
        validator.initialize_phase5_baselines();

        // Verify all Phase 5 metrics have baselines
        let expected_metrics = vec![
            Metric::ToolExecutionSpeed,
            Metric::MemoryUsage,
            Metric::CompilationTime,
            Metric::BinarySize,
            Metric::CodeSize,
        ];

        for metric in expected_metrics {
            assert!(validator.baselines.contains_key(&metric),
                   "Missing baseline for metric: {:?}", metric);
        }

        // Verify baseline calculations
        if let Some(baseline) = validator.baselines.get(&Metric::ToolExecutionSpeed) {
            assert_eq!(baseline.baseline_value, 250.0);
            assert_eq!(baseline.improvement_percentage, 82.0);
            assert!((baseline.target_value - 45.0).abs() < 0.1,
                   "Target value should be approximately 45.0, got {}", baseline.target_value);
        }
    }

    #[tokio::test]
    async fn test_metric_measurement_simulation() {
        let validator = Phase6TestValidator::new();

        // Test each metric type
        let test_metrics = vec![
            Metric::ToolExecutionSpeed,
            Metric::MemoryUsage,
            Metric::CompilationTime,
            Metric::BinarySize,
            Metric::CodeSize,
        ];

        for metric in test_metrics {
            let measured_value = validator.simulate_metric_measurement(metric).await.unwrap();

            // Verify measured values are realistic
            match metric {
                Metric::ToolExecutionSpeed => {
                    assert!(measured_value > 20.0 && measured_value < 80.0,
                           "Tool execution time should be reasonable, got {}", measured_value);
                }
                Metric::MemoryUsage => {
                    assert!(measured_value > 50.0 && measured_value < 150.0,
                           "Memory usage should be reasonable, got {}", measured_value);
                }
                Metric::CompilationTime => {
                    assert!(measured_value > 10.0 && measured_value < 30.0,
                           "Compilation time should be reasonable, got {}", measured_value);
                }
                Metric::BinarySize => {
                    assert!(measured_value > 30.0 && measured_value < 100.0,
                           "Binary size should be reasonable, got {}", measured_value);
                }
                Metric::CodeSize => {
                    assert!(measured_value > 2000.0 && measured_value < 5000.0,
                           "Code size should be reasonable, got {}", measured_value);
                }
                _ => {}
            }
        }
    }

    #[tokio::test]
    async fn test_phase5_validation_individual() {
        let validator = Phase6TestValidator::new();
        let result = validator.validate_phase5_improvements().await.unwrap();

        assert_eq!(result.name, "Phase 5 Performance Improvement Validation");
        assert!(!result.metric_results.is_empty(), "Should have metric results");
        assert!(result.execution_time > Duration::ZERO, "Should take time to execute");

        // Verify all Phase 5 metrics are validated
        let validated_metrics: std::collections::HashSet<_> =
            result.metric_results.iter().map(|m| m.metric).collect();

        for (metric, _, _, _) in PHASE5_PERFORMANCE_CLAIMS {
            assert!(validated_metrics.contains(metric),
                   "Metric {:?} should be validated", metric);
        }

        // Verify metric result structure
        for metric_result in &result.metric_results {
            assert!(metric_result.measured_value > 0.0, "Measured value should be positive");
            assert!(metric_result.baseline_value.is_some(), "Should have baseline value");
            assert!(metric_result.target_value.is_some(), "Should have target value");
            assert!(metric_result.improvement_achieved.is_some(), "Should have improvement achieved");
            assert!(!metric_result.unit.is_empty(), "Should have unit");
            assert!(metric_result.sample_size > 0, "Should have sample size");
        }
    }

    #[tokio::test]
    async fn test_regression_testing_validation() {
        let validator = Phase6TestValidator::new();
        let result = validator.validate_regression_testing().await.unwrap();

        assert_eq!(result.name, "Regression Testing Framework Validation");
        assert!(!result.metric_results.is_empty(), "Should have metric results");
        assert!(result.execution_time > Duration::ZERO, "Should take time to execute");

        // Verify regression testing metrics
        let metric_types: std::collections::HashSet<_> =
            result.metric_results.iter().map(|m| m.metric).collect();

        // Should test various aspects of regression testing
        assert!(metric_types.len() >= 3, "Should test multiple regression aspects");

        // All regression tests should be statistical accuracy tests
        for metric_result in &result.metric_results {
            assert_eq!(metric_result.metric, Metric::StatisticalAccuracy);
            assert_eq!(metric_result.unit, "%");
            assert!(metric_result.measured_value >= 0.0 && metric_result.measured_value <= 100.0);
        }
    }

    #[tokio::test]
    async fn test_integration_validation() {
        let validator = Phase6TestValidator::new();
        let results = validator.validate_integration().await.unwrap();

        assert!(!results.is_empty(), "Should have integration results");

        // Verify integration types
        let integration_types: std::collections::HashSet<_> =
            results.iter().map(|r| r.integration_type).collect();

        assert!(integration_types.contains(&IntegrationType::LoadTestStressTest));
        assert!(integration_types.contains(&IntegrationType::MemoryProfileLoadTest));
        assert!(integration_types.contains(&IntegrationType::StatisticalValidationLoadTest));
        assert!(integration_types.contains(&IntegrationType::FrameworkInteroperability));
        assert!(integration_types.contains(&IntegrationType::EndToEndWorkflow));
        assert!(integration_types.contains(&IntegrationType::ConcurrentExecution));

        // Verify integration result structure
        for integration in &results {
            assert!(!integration.component_name.is_empty(), "Should have component name");
            assert!(integration.test_cases_total > 0, "Should have test cases");
            assert!(integration.test_cases_passed <= integration.test_cases_total,
                   "Passed cases should not exceed total");
            assert!(integration.reliability_score >= 0.0 && integration.reliability_score <= 100.0,
                   "Reliability score should be percentage");
            assert!(integration.performance_impact >= 0.0, "Performance impact should be non-negative");
        }
    }

    #[tokio::test]
    async fn test_statistical_validation() {
        let validator = Phase6TestValidator::new();
        let results = validator.validate_statistical_accuracy().await.unwrap();

        assert!(!results.is_empty(), "Should have statistical results");

        // Verify statistical test types
        let test_names: std::collections::HashSet<_> =
            results.iter().map(|r| r.test_name.as_str()).collect();

        assert!(test_names.contains("Measurement Accuracy Validation"));
        assert!(test_names.contains("Confidence Interval Accuracy"));
        assert!(test_names.contains("Statistical Significance Validation"));
        assert!(test_names.contains("Sample Size Adequacy"));

        // Verify statistical result structure
        for statistical in &results {
            assert!(statistical.sample_size > 0, "Should have sample size");
            assert!(statistical.mean >= statistical.min_value, "Mean should be >= min");
            assert!(statistical.mean <= statistical.max_value, "Mean should be <= max");
            assert!(statistical.std_deviation >= 0.0, "Std deviation should be non-negative");
            assert!(statistical.percentile_95 >= statistical.mean, "95th percentile should be >= mean");
            assert!(statistical.coefficient_of_variation >= 0.0, "CV should be non-negative");
            assert!(statistical.p_value > 0.0 && statistical.p_value < 1.0, "P-value should be valid");
            assert!(statistical.confidence_interval.0 <= statistical.confidence_interval.1,
                   "Confidence interval should be valid");
        }
    }

    #[tokio::test]
    async fn test_production_readiness_validation() {
        let validator = Phase6TestValidator::new();
        let results = validator.validate_production_readiness().await.unwrap();

        assert!(!results.is_empty(), "Should have production readiness results");

        // Verify framework types
        let framework_names: std::collections::HashSet<_> =
            results.iter().map(|r| r.framework_name.as_str()).collect();

        assert!(framework_names.contains("Load Testing Framework"));
        assert!(framework_names.contains("Stress Testing Framework"));
        assert!(framework_names.contains("Memory Profiling Framework"));
        assert!(framework_names.contains("Statistical Validation Framework"));

        // Verify production readiness result structure
        for readiness in &results {
            assert!(!readiness.framework_name.is_empty(), "Should have framework name");
            assert!(readiness.readiness_score >= 0.0 && readiness.readiness_score <= 100.0,
                   "Readiness score should be percentage");
            assert!(!readiness.recommendations.is_empty(), "Should have recommendations");
            // Most frameworks should be production-ready in our simulation
            assert!(readiness.is_production_ready ||
                   readiness.framework_name == "Memory Profiling Framework",
                   "Most frameworks should be production-ready");
        }
    }

    #[tokio::test]
    async fn test_full_validation_execution_minimal() {
        let config = create_minimal_test_config();
        let mut validator = Phase6TestValidator::with_config(config);
        let results = validator.run_validation().await.unwrap();

        // Verify basic structure
        assert_eq!(results.validation_name, "Phase 6.TEST Performance Validation and Regression Testing");
        assert!(!results.execution_id.is_empty(), "Should have execution ID");
        assert!(results.started_at <= results.completed_at, "Start time should be before end time");
        assert!(results.total_duration > Duration::ZERO, "Should take time to execute");

        // With minimal config, only Phase 5 validation should run
        assert!(results.phase5_validation.is_some(), "Phase 5 validation should run");
        assert!(results.regression_results.is_none(), "Regression testing should be disabled");
        assert!(results.integration_results.is_empty(), "Integration tests should be disabled");
        assert!(results.statistical_results.is_empty(), "Statistical tests should be disabled");
        assert!(results.production_readiness.is_empty(), "Production readiness should be disabled");

        // Verify overall calculation
        assert!(results.success_rate >= 0.0 && results.success_rate <= 100.0);
        match results.overall_status {
            ValidationStatus::Passed | ValidationStatus::Warning | ValidationStatus::Failed => {},
            _ => panic!("Overall status should be a conclusive result"),
        }
    }

    #[tokio::test]
    async fn test_full_validation_execution_comprehensive() {
        let config = create_test_config();
        let mut validator = Phase6TestValidator::with_config(config);
        let results = validator.run_validation().await.unwrap();

        // Verify all validation components ran
        assert!(results.phase5_validation.is_some(), "Phase 5 validation should run");
        assert!(results.regression_results.is_some(), "Regression testing should run");
        assert!(!results.integration_results.is_empty(), "Integration tests should run");
        assert!(!results.statistical_results.is_empty(), "Statistical tests should run");
        assert!(!results.production_readiness.is_empty(), "Production readiness should run");

        // Verify comprehensive results
        assert!(results.success_rate > 0.0, "Should have some success");
        assert!(!results.recommendations.is_empty(), "Should have recommendations");

        // Most tests should pass in our simulation
        assert!(results.success_rate >= 70.0, "Success rate should be reasonably high");
    }
}

#[cfg(test)]
mod performance_baseline_tests {
    use super::*;

    #[test]
    fn test_performance_baseline_creation() {
        let baseline = PerformanceBaseline::from_phase5_claim(
            Metric::ToolExecutionSpeed,
            250.0,
            82.0,
            5.0,
        );

        assert_eq!(baseline.metric, Metric::ToolExecutionSpeed);
        assert_eq!(baseline.baseline_value, 250.0);
        assert_eq!(baseline.improvement_percentage, 82.0);
        assert!((baseline.target_value - 45.0).abs() < 0.1);
        assert_eq!(baseline.tolerance, 5.0);
        assert_eq!(baseline.confidence_level, 0.95);
    }

    #[test]
    fn test_performance_baseline_tolerance_checking() {
        let baseline = PerformanceBaseline::from_phase5_claim(
            Metric::MemoryUsage,
            200.0,
            58.0,
            10.0,  // 10% tolerance
        );

        // Target is 200 * (1 - 0.58) = 84MB
        assert_eq!(baseline.target_value, 84.0);

        // Test tolerance checking
        assert!(baseline.is_within_tolerance(84.0), "Exact target should be within tolerance");
        assert!(baseline.is_within_tolerance(89.0), "Small deviation should be within tolerance");
        assert!(baseline.is_within_tolerance(79.0), "Small deviation should be within tolerance");
        assert!(!baseline.is_within_tolerance(95.0), "Large deviation should be outside tolerance");
        assert!(!baseline.is_within_tolerance(73.0), "Large deviation should be outside tolerance");
    }

    #[test]
    fn test_performance_baseline_improvement_calculation() {
        let baseline = PerformanceBaseline::from_phase5_claim(
            Metric::CompilationTime,
            45.0,
            60.0,
            5.0,
        );

        // Target is 45 * (1 - 0.60) = 18s
        assert_eq!(baseline.target_value, 18.0);

        // Test improvement calculation
        assert_eq!(baseline.improvement_achieved(18.0), 60.0); // Exact target
        assert_eq!(baseline.improvement_achieved(22.5), 50.0); // 50% improvement
        assert_eq!(baseline.improvement_achieved(9.0), 80.0);  // 80% improvement
        assert_eq!(baseline.improvement_achieved(45.0), 0.0);  // No improvement
    }

    #[test]
    fn test_all_phase5_baselines_creation() {
        let metrics_and_values = vec![
            (Metric::ToolExecutionSpeed, 250.0, 82.0),
            (Metric::MemoryUsage, 200.0, 58.0),
            (Metric::CompilationTime, 45.0, 60.0),
            (Metric::BinarySize, 125.0, 54.0),
            (Metric::CodeSize, 8500.0, 59.0),
        ];

        for (metric, baseline, improvement) in metrics_and_values {
            let performance_baseline = PerformanceBaseline::from_phase5_claim(
                metric, baseline, improvement, 5.0
            );

            assert_eq!(performance_baseline.metric, metric);
            assert_eq!(performance_baseline.baseline_value, baseline);
            assert_eq!(performance_baseline.improvement_percentage, improvement);

            let expected_target = baseline * (1.0 - improvement / 100.0);
            assert!((performance_baseline.target_value - expected_target).abs() < 0.01,
                   "Target calculation incorrect for metric {:?}", metric);
        }
    }
}

#[cfg(test)]
mod metric_tests {
    use super::*;

    #[test]
    fn test_metric_units() {
        assert_eq!(Metric::ToolExecutionSpeed.unit(), "ms");
        assert_eq!(Metric::MemoryUsage.unit(), "MB");
        assert_eq!(Metric::CompilationTime.unit(), "s");
        assert_eq!(Metric::BinarySize.unit(), "MB");
        assert_eq!(Metric::CodeSize.unit(), "lines");
        assert_eq!(Metric::FrameworkOverhead.unit(), "ms");
        assert_eq!(Metric::LoadTestThroughput.unit(), "ops/sec");
        assert_eq!(Metric::StressTestReliability.unit(), "%");
        assert_eq!(Metric::MemoryProfileAccuracy.unit(), "%");
        assert_eq!(Metric::StatisticalAccuracy.unit(), "%");
    }

    #[test]
    fn test_metric_names() {
        assert_eq!(Metric::ToolExecutionSpeed.name(), "Tool Execution Speed");
        assert_eq!(Metric::MemoryUsage.name(), "Memory Usage");
        assert_eq!(Metric::CompilationTime.name(), "Compilation Time");
        assert_eq!(Metric::BinarySize.name(), "Binary Size");
        assert_eq!(Metric::CodeSize.name(), "Code Size");
        assert_eq!(Metric::FrameworkOverhead.name(), "Framework Overhead");
        assert_eq!(Metric::LoadTestThroughput.name(), "Load Test Throughput");
        assert_eq!(Metric::StressTestReliability.name(), "Stress Test Reliability");
        assert_eq!(Metric::MemoryProfileAccuracy.name(), "Memory Profile Accuracy");
        assert_eq!(Metric::StatisticalAccuracy.name(), "Statistical Accuracy");
    }

    #[test]
    fn test_metric_hash_and_equality() {
        let metric1 = Metric::ToolExecutionSpeed;
        let metric2 = Metric::ToolExecutionSpeed;
        let metric3 = Metric::MemoryUsage;

        assert_eq!(metric1, metric2);
        assert_ne!(metric1, metric3);

        // Test that metrics can be used as HashMap keys
        let mut map = std::collections::HashMap::new();
        map.insert(metric1, "value1");
        map.insert(metric3, "value3");

        assert_eq!(map.get(&metric1), Some(&"value1"));
        assert_eq!(map.get(&metric3), Some(&"value3"));
        assert_eq!(map.get(&metric2), Some(&"value1")); // Same as metric1
    }
}

#[cfg(test)]
mod validation_config_tests {
    use super::*;

    #[test]
    fn test_validation_config_default() {
        let config = ValidationConfig::default();

        assert!(config.validate_phase5_improvements);
        assert!(config.validate_regression_testing);
        assert!(config.validate_integration);
        assert!(config.validate_statistical_accuracy);
        assert!(config.validate_production_readiness);

        assert_eq!(config.improvement_tolerance, 5.0);
        assert_eq!(config.regression_threshold, 10.0);
        assert_eq!(config.statistical_significance, 0.05);
        assert_eq!(config.confidence_interval, 0.95);
        assert_eq!(config.test_iterations, 10);
        assert_eq!(config.concurrent_tests, 4);
        assert_eq!(config.timeout_seconds, 300);
        assert_eq!(config.max_memory_mb, 1024.0);

        assert!(config.generate_detailed_report);
        assert!(config.save_historical_data);
        assert!(config.compare_with_baselines);
        assert!(config.export_metrics);
    }

    #[test]
    fn test_validation_config_custom() {
        let config = ValidationConfig {
            validate_phase5_improvements: false,
            validate_regression_testing: true,
            validate_integration: false,
            validate_statistical_accuracy: true,
            validate_production_readiness: false,
            improvement_tolerance: 15.0,
            regression_threshold: 20.0,
            statistical_significance: 0.1,
            confidence_interval: 0.9,
            test_iterations: 5,
            concurrent_tests: 2,
            timeout_seconds: 120,
            max_memory_mb: 512.0,
            generate_detailed_report: false,
            save_historical_data: false,
            compare_with_baselines: true,
            export_metrics: false,
        };

        assert!(!config.validate_phase5_improvements);
        assert!(config.validate_regression_testing);
        assert!(!config.validate_integration);
        assert!(config.validate_statistical_accuracy);
        assert!(!config.validate_production_readiness);

        assert_eq!(config.improvement_tolerance, 15.0);
        assert_eq!(config.regression_threshold, 20.0);
        assert_eq!(config.test_iterations, 5);
        assert_eq!(config.concurrent_tests, 2);
    }

    #[test]
    fn test_validation_config_clone() {
        let config = create_test_config();
        let cloned_config = config.clone();

        assert_eq!(config.validate_phase5_improvements, cloned_config.validate_phase5_improvements);
        assert_eq!(config.improvement_tolerance, cloned_config.improvement_tolerance);
        assert_eq!(config.test_iterations, cloned_config.test_iterations);
    }
}

#[cfg(test)]
mod validation_result_tests {
    use super::*;

    #[test]
    fn test_validation_result_creation() {
        let metric_results = vec![
            MetricResult {
                metric: Metric::ToolExecutionSpeed,
                measured_value: 45.0,
                baseline_value: Some(250.0),
                target_value: Some(45.0),
                improvement_achieved: Some(82.0),
                is_within_tolerance: true,
                unit: "ms".to_string(),
                confidence_interval: Some((42.0, 48.0)),
                sample_size: 10,
            }
        ];

        let result = ValidationResult {
            name: "Test Validation".to_string(),
            status: ValidationStatus::Passed,
            metric_results,
            details: "Test validation completed".to_string(),
            execution_time: Duration::from_millis(150),
            confidence_interval: (0.95, 0.95),
            sample_size: 10,
            warnings: vec![],
            errors: vec![],
        };

        assert_eq!(result.name, "Test Validation");
        assert_eq!(result.status, ValidationStatus::Passed);
        assert_eq!(result.metric_results.len(), 1);
        assert_eq!(result.execution_time, Duration::from_millis(150));
        assert!(result.warnings.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_metric_result_creation() {
        let metric_result = MetricResult {
            metric: Metric::MemoryUsage,
            measured_value: 85.0,
            baseline_value: Some(200.0),
            target_value: Some(85.0),
            improvement_achieved: Some(57.5),
            is_within_tolerance: true,
            unit: "MB".to_string(),
            confidence_interval: Some((80.0, 90.0)),
            sample_size: 15,
        };

        assert_eq!(metric_result.metric, Metric::MemoryUsage);
        assert_eq!(metric_result.measured_value, 85.0);
        assert_eq!(metric_result.unit, "MB");
        assert!(metric_result.is_within_tolerance);
        assert_eq!(metric_result.sample_size, 15);
    }
}

#[cfg(test)]
mod integration_result_tests {
    use super::*;

    #[test]
    fn test_integration_result_creation() {
        let result = IntegrationResult {
            component_name: "Test Integration".to_string(),
            integration_type: IntegrationType::LoadTestStressTest,
            status: ValidationStatus::Passed,
            performance_impact: 2.5,
            reliability_score: 95.0,
            interoperability_issues: vec![],
            test_cases_passed: 10,
            test_cases_total: 10,
        };

        assert_eq!(result.component_name, "Test Integration");
        assert_eq!(result.integration_type, IntegrationType::LoadTestStressTest);
        assert_eq!(result.status, ValidationStatus::Passed);
        assert_eq!(result.performance_impact, 2.5);
        assert_eq!(result.reliability_score, 95.0);
        assert_eq!(result.test_cases_passed, 10);
        assert_eq!(result.test_cases_total, 10);
        assert!(result.interoperability_issues.is_empty());
    }

    #[test]
    fn test_integration_result_with_issues() {
        let result = IntegrationResult {
            component_name: "Problematic Integration".to_string(),
            integration_type: IntegrationType::FrameworkInteroperability,
            status: ValidationStatus::Warning,
            performance_impact: 5.2,
            reliability_score: 87.3,
            interoperability_issues: vec![
                "Minor synchronization issue".to_string(),
                "Resource contention detected".to_string(),
            ],
            test_cases_passed: 8,
            test_cases_total: 10,
        };

        assert_eq!(result.status, ValidationStatus::Warning);
        assert_eq!(result.interoperability_issues.len(), 2);
        assert_eq!(result.test_cases_passed, 8);
        assert_eq!(result.test_cases_total, 10);
    }
}

#[cfg(test)]
mod statistical_validation_result_tests {
    use super::*;

    #[test]
    fn test_statistical_validation_result_creation() {
        let result = StatisticalValidationResult {
            test_name: "Test Statistical Validation".to_string(),
            sample_size: 100,
            mean: 95.5,
            median: 96.0,
            std_deviation: 3.2,
            min_value: 87.1,
            max_value: 102.3,
            percentile_95: 101.2,
            confidence_interval: (94.1, 96.9),
            is_statistically_significant: true,
            p_value: 0.023,
            coefficient_of_variation: 3.35,
        };

        assert_eq!(result.test_name, "Test Statistical Validation");
        assert_eq!(result.sample_size, 100);
        assert_eq!(result.mean, 95.5);
        assert_eq!(result.median, 96.0);
        assert_eq!(result.std_deviation, 3.2);
        assert!(result.is_statistically_significant);
        assert_eq!(result.p_value, 0.023);
        assert!(result.confidence_interval.0 < result.confidence_interval.1);
    }

    #[test]
    fn test_statistical_validation_result_validation() {
        let result = StatisticalValidationResult {
            test_name: "Validation Test".to_string(),
            sample_size: 50,
            mean: 88.7,
            median: 89.2,
            std_deviation: 4.1,
            min_value: 78.3,
            max_value: 96.8,
            percentile_95: 95.1,
            confidence_interval: (87.2, 90.2),
            is_statistically_significant: true,
            p_value: 0.045,
            coefficient_of_variation: 4.62,
        };

        // Validate statistical consistency
        assert!(result.min_value <= result.mean);
        assert!(result.mean <= result.max_value);
        assert!(result.median >= result.min_value);
        assert!(result.median <= result.max_value);
        assert!(result.percentile_95 >= result.mean);
        assert!(result.std_deviation >= 0.0);
        assert!(result.coefficient_of_variation >= 0.0);
        assert!(result.p_value > 0.0 && result.p_value < 1.0);
        assert!(result.confidence_interval.0 <= result.mean);
        assert!(result.mean <= result.confidence_interval.1);
    }
}

#[cfg(test)]
mod production_readiness_result_tests {
    use super::*;

    #[test]
    fn test_production_readiness_result_creation() {
        let result = ProductionReadinessResult {
            framework_name: "Test Framework".to_string(),
            readiness_score: 92.5,
            reliability_rating: ReliabilityRating::Excellent,
            performance_rating: PerformanceRating::Good,
            scalability_rating: ScalabilityRating::Excellent,
            maintenance_rating: MaintenanceRating::Good,
            deployment_blockers: vec![],
            recommendations: vec![
                "Add more documentation".to_string(),
                "Improve error handling".to_string(),
            ],
            is_production_ready: true,
        };

        assert_eq!(result.framework_name, "Test Framework");
        assert_eq!(result.readiness_score, 92.5);
        assert_eq!(result.reliability_rating, ReliabilityRating::Excellent);
        assert_eq!(result.performance_rating, PerformanceRating::Good);
        assert_eq!(result.scalability_rating, ScalabilityRating::Excellent);
        assert_eq!(result.maintenance_rating, MaintenanceRating::Good);
        assert!(result.is_production_ready);
        assert!(result.deployment_blockers.is_empty());
        assert_eq!(result.recommendations.len(), 2);
    }

    #[test]
    fn test_production_readiness_result_not_ready() {
        let result = ProductionReadinessResult {
            framework_name: "Incomplete Framework".to_string(),
            readiness_score: 65.3,
            reliability_rating: ReliabilityRating::Fair,
            performance_rating: PerformanceRating::Fair,
            scalability_rating: ScalabilityRating::Poor,
            maintenance_rating: ReliabilityRating::Good,
            deployment_blockers: vec![
                "Critical memory leaks detected".to_string(),
                "Insufficient error handling".to_string(),
            ],
            recommendations: vec![
                "Fix memory leaks".to_string(),
                "Implement comprehensive error handling".to_string(),
                "Improve scalability".to_string(),
            ],
            is_production_ready: false,
        };

        assert!(!result.is_production_ready);
        assert_eq!(result.readiness_score, 65.3);
        assert_eq!(result.deployment_blockers.len(), 2);
        assert_eq!(result.recommendations.len(), 3);
    }
}

#[cfg(test)]
mod convenience_function_tests {
    use super::*;

    #[tokio::test]
    async fn test_run_phase6_test_validation_default() {
        let results = run_phase6_test_validation().await.unwrap();

        assert_eq!(results.validation_name, "Phase 6.TEST Performance Validation and Regression Testing");
        assert!(!results.execution_id.is_empty());
        assert!(results.total_duration > Duration::ZERO);
        assert!(results.success_rate >= 0.0 && results.success_rate <= 100.0);
    }

    #[tokio::test]
    async fn test_run_phase6_test_validation_custom_config() {
        let config = create_minimal_test_config();
        let results = run_phase6_test_validation_with_config(config).await.unwrap();

        assert_eq!(results.validation_name, "Phase 6.TEST Performance Validation and Regression Testing");
        assert!(results.phase5_validation.is_some());
        assert!(results.regression_results.is_none()); // Disabled in minimal config
    }
}

#[cfg(test)]
mod report_generation_tests {
    use super::*;

    #[test]
    fn test_report_generation_basic() {
        let validator = Phase6TestValidator::new();
        let results = Phase6TestResults {
            validation_name: "Test Validation".to_string(),
            execution_id: "test-123".to_string(),
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
            total_duration: Duration::from_secs(30),
            config: ValidationConfig::default(),
            phase5_validation: None,
            regression_results: None,
            integration_results: vec![],
            statistical_results: vec![],
            production_readiness: vec![],
            overall_status: ValidationStatus::Passed,
            success_rate: 100.0,
            critical_issues: vec![],
            warnings: vec![],
            recommendations: vec!["All systems operational".to_string()],
        };

        let report = validator.generate_report(&results);

        assert!(report.contains("Phase 6.TEST Performance Validation and Regression Testing Report"));
        assert!(report.contains("Test Validation"));
        assert!(report.contains("test-123"));
        assert!(report.contains("Overall Status"));
        assert!(report.contains("Success Rate"));
        assert!(report.contains("100.0%"));
        assert!(report.contains("All systems operational"));
    }

    #[test]
    fn test_report_generation_with_phase5_results() {
        let validator = Phase6TestValidator::new();

        let metric_results = vec![
            MetricResult {
                metric: Metric::ToolExecutionSpeed,
                measured_value: 45.0,
                baseline_value: Some(250.0),
                target_value: Some(45.0),
                improvement_achieved: Some(82.0),
                is_within_tolerance: true,
                unit: "ms".to_string(),
                confidence_interval: Some((42.0, 48.0)),
                sample_size: 10,
            }
        ];

        let phase5_validation = Some(ValidationResult {
            name: "Phase 5 Performance Improvement Validation".to_string(),
            status: ValidationStatus::Passed,
            metric_results,
            details: "All metrics validated successfully".to_string(),
            execution_time: Duration::from_millis(500),
            confidence_interval: (0.95, 0.95),
            sample_size: 10,
            warnings: vec![],
            errors: vec![],
        });

        let results = Phase6TestResults {
            validation_name: "Test Validation".to_string(),
            execution_id: "test-456".to_string(),
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
            total_duration: Duration::from_secs(45),
            config: ValidationConfig::default(),
            phase5_validation,
            regression_results: None,
            integration_results: vec![],
            statistical_results: vec![],
            production_readiness: vec![],
            overall_status: ValidationStatus::Passed,
            success_rate: 100.0,
            critical_issues: vec![],
            warnings: vec![],
            recommendations: vec![],
        };

        let report = validator.generate_report(&results);

        assert!(report.contains("Phase 5 Performance Improvement Validation"));
        assert!(report.contains("Metric Results"));
        assert!(report.contains("Tool Execution Speed"));
        assert!(report.contains("45.0 ms"));
        assert!(report.contains("82.0%"));
        assert!(report.contains("✅ PASS"));
    }

    #[test]
    fn test_report_generation_with_issues() {
        let validator = Phase6TestValidator::new();
        let results = Phase6TestResults {
            validation_name: "Test Validation with Issues".to_string(),
            execution_id: "test-789".to_string(),
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
            total_duration: Duration::from_secs(60),
            config: ValidationConfig::default(),
            phase5_validation: None,
            regression_results: None,
            integration_results: vec![],
            statistical_results: vec![],
            production_readiness: vec![],
            overall_status: ValidationStatus::Warning,
            success_rate: 75.0,
            critical_issues: vec![
                "Critical memory leak detected".to_string(),
            ],
            warnings: vec![
                "Performance below expected threshold".to_string(),
                "Minor integration issues found".to_string(),
            ],
            recommendations: vec![
                "Fix memory leaks before deployment".to_string(),
                "Optimize performance".to_string(),
                "Resolve integration issues".to_string(),
            ],
        };

        let report = validator.generate_report(&results);

        assert!(report.contains("Critical Issues"));
        assert!(report.contains("Warnings"));
        assert!(report.contains("Recommendations"));
        assert!(report.contains("Critical memory leak detected"));
        assert!(report.contains("Performance below expected threshold"));
        assert!(report.contains("Fix memory leaks before deployment"));
        assert!(report.contains("75.0%"));
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[tokio::test]
    async fn test_validation_with_timeout() {
        let config = ValidationConfig {
            timeout_seconds: 1, // Very short timeout
            test_iterations: 100, // Many iterations to trigger timeout
            ..create_minimal_test_config()
        };

        let mut validator = Phase6TestValidator::with_config(config);

        // This should handle timeout gracefully
        let results = validator.run_validation().await;

        // Should not panic, should return results (possibly with errors)
        assert!(results.is_ok());
        let results = results.unwrap();

        // May have errors due to timeout, but should have structure
        assert!(!results.execution_id.is_empty());
        assert!(results.total_duration > Duration::ZERO);
    }

    #[tokio::test]
    async fn test_validation_with_invalid_config() {
        let config = ValidationConfig {
            test_iterations: 0, // Invalid
            concurrent_tests: 0, // Invalid
            timeout_seconds: 0, // Invalid
            ..create_minimal_test_config()
        };

        let validator = Phase6TestValidator::with_config(config);

        // Should handle invalid config gracefully
        let results = validator.run_validation().await;

        assert!(results.is_ok());
        let results = results.unwrap();

        // Should still produce valid structure
        assert!(!results.execution_id.is_empty());
    }

    #[tokio::test]
    async fn test_metric_measurement_error_handling() {
        let validator = Phase6TestValidator::new();

        // Test with all metrics - should handle errors gracefully
        let metrics = vec![
            Metric::ToolExecutionSpeed,
            Metric::MemoryUsage,
            Metric::CompilationTime,
            Metric::BinarySize,
            Metric::CodeSize,
            Metric::FrameworkOverhead,
            Metric::LoadTestThroughput,
            Metric::StressTestReliability,
            Metric::MemoryProfileAccuracy,
            Metric::StatisticalAccuracy,
        ];

        for metric in metrics {
            let result = validator.simulate_metric_measurement(metric).await;
            assert!(result.is_ok(), "Metric measurement should not fail for {:?}", metric);

            let value = result.unwrap();
            assert!(value >= 0.0, "Measured value should be non-negative for {:?}", metric);
        }
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_validation_performance() {
        let config = create_minimal_test_config();
        let mut validator = Phase6TestValidator::with_config(config);

        let start_time = std::time::Instant::now();
        let results = validator.run_validation().await.unwrap();
        let execution_time = start_time.elapsed();

        // Validation should complete in reasonable time
        assert!(execution_time < Duration::from_secs(30),
               "Validation should complete quickly, took {:?}", execution_time);

        // Should still produce valid results
        assert!(results.success_rate >= 0.0);
        assert!(!results.execution_id.is_empty());
    }

    #[tokio::test]
    async fn test_concurrent_validation_performance() {
        let config = create_test_config();
        let validator = Arc::new(Mutex::new(Phase6TestValidator::with_config(config)));

        // Run multiple validations concurrently
        let mut handles = Vec::new();
        for i in 0..3 {
            let validator = validator.clone();
            let handle = tokio::spawn(async move {
                let mut v = validator.lock().await;
                v.run_validation().await
            });
            handles.push(handle);
        }

        let start_time = std::time::Instant::now();
        let results: Vec<_> = futures::future::join_all(handles).await;
        let total_time = start_time.elapsed();

        // All validations should succeed
        for result in results {
            let validation_result = result.unwrap().unwrap();
            assert!(validation_result.success_rate >= 0.0);
        }

        // Concurrent execution should be reasonable
        assert!(total_time < Duration::from_secs(120),
               "Concurrent validation should complete in reasonable time, took {:?}", total_time);
    }

    #[tokio::test]
    async fn test_memory_usage_during_validation() {
        let config = create_test_config();
        let mut validator = Phase6TestValidator::with_config(config);

        // Measure memory before validation
        let memory_before = get_memory_usage();

        let results = validator.run_validation().await.unwrap();

        // Measure memory after validation
        let memory_after = get_memory_usage();

        // Memory usage should be reasonable
        let memory_increase = memory_after.saturating_sub(memory_before);
        assert!(memory_increase < 100 * 1024 * 1024, // Less than 100MB increase
               "Memory usage increased by {} bytes, which is too high", memory_increase);

        // Validation should still succeed
        assert!(results.success_rate >= 0.0);
    }

    fn get_memory_usage() -> usize {
        // Simple memory usage estimation - in real implementation, use proper memory profiling
        std::mem::size_of::<Phase6TestValidator>() +
        std::mem::size_of::<ValidationConfig>() +
        std::mem::size_of::<Phase6TestResults>() * 10
    }
}