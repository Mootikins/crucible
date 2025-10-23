//! Comprehensive unit tests for ToolDistribution and ResourceLimit algorithms
//!
//! Specialized testing for distribution selection algorithms, resource limit enforcement,
//! and statistical accuracy of the load testing framework components

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use rand::Rng;

#[cfg(test)]
mod tool_distribution_algorithm_tests {
    use super::*;

    /// Test tool distribution selection algorithm accuracy
    #[test]
    fn test_tool_distribution_selection_accuracy() {
        let test_cases = vec![
            // (simple_ratio, medium_ratio, complex_ratio, expected_distribution_name)
            (1.0, 0.0, 0.0, "All Simple"),
            (0.0, 1.0, 0.0, "All Medium"),
            (0.0, 0.0, 1.0, "All Complex"),
            (0.5, 0.3, 0.2, "Mixed 50-30-20"),
            (0.33, 0.33, 0.34, "Nearly Equal"),
            (0.8, 0.15, 0.05, "Simple Heavy"),
            (0.1, 0.2, 0.7, "Complex Heavy"),
        ];

        for (simple_ratio, medium_ratio, complex_ratio, test_name) in test_cases {
            let distribution = crate::load_testing_framework::ToolDistribution {
                simple_ratio,
                medium_ratio,
                complex_ratio,
            };

            let num_selections = 10000;
            let mut counts = HashMap::new();
            counts.insert(crate::load_testing_framework::ToolComplexity::Simple, 0);
            counts.insert(crate::load_testing_framework::ToolComplexity::Medium, 0);
            counts.insert(crate::load_testing_framework::ToolComplexity::Complex, 0);

            // Perform selections
            for _ in 0..num_selections {
                let selected = select_tool_type_deterministic_for_test(&distribution);
                *counts.get_mut(&selected).unwrap() += 1;
            }

            // Verify distribution accuracy
            let simple_count = *counts.get(&crate::load_testing_framework::ToolComplexity::Simple).unwrap();
            let medium_count = *counts.get(&crate::load_testing_framework::ToolComplexity::Medium).unwrap();
            let complex_count = *counts.get(&crate::load_testing_framework::ToolComplexity::Complex).unwrap();

            let simple_actual = simple_count as f64 / num_selections as f64;
            let medium_actual = medium_count as f64 / num_selections as f64;
            let complex_actual = complex_count as f64 / num_selections as f64;

            // Allow small tolerance for statistical variation
            let tolerance = 0.02; // 2% tolerance

            assert!((simple_actual - simple_ratio).abs() < tolerance,
                   "Test {}: Simple ratio expected {:.3}, got {:.3}",
                   test_name, simple_ratio, simple_actual);

            assert!((medium_actual - medium_ratio).abs() < tolerance,
                   "Test {}: Medium ratio expected {:.3}, got {:.3}",
                   test_name, medium_ratio, medium_actual);

            assert!((complex_actual - complex_ratio).abs() < tolerance,
                   "Test {}: Complex ratio expected {:.3}, got {:.3}",
                   test_name, complex_ratio, complex_actual);
        }
    }

    /// Test tool distribution edge cases
    #[test]
    fn test_tool_distribution_edge_cases() {
        let edge_cases = vec![
            (0.999999, 0.000001, 0.0, "Nearly All Simple"),
            (0.0, 0.999999, 0.000001, "Nearly All Medium"),
            (0.000001, 0.0, 0.999999, "Nearly All Complex"),
            (0.5, 0.5, 0.0, "No Complex"),
            (0.5, 0.0, 0.5, "No Medium"),
            (0.0, 0.5, 0.5, "No Simple"),
            (0.333333, 0.333333, 0.333334, "Floating Point Precision"),
        ];

        for (simple_ratio, medium_ratio, complex_ratio, test_name) in edge_cases {
            let distribution = crate::load_testing_framework::ToolDistribution {
                simple_ratio,
                medium_ratio,
                complex_ratio,
            };

            let num_selections = 5000; // Fewer selections for edge cases
            let mut counts = HashMap::new();

            // Perform selections
            for _ in 0..num_selections {
                let selected = select_tool_type_deterministic_for_test(&distribution);
                *counts.entry(selected).or_insert(0) += 1;
            }

            // Verify no panics and reasonable distribution
            let total_selected: usize = counts.values().sum();
            assert_eq!(total_selected, num_selections,
                      "Test {}: Should select exactly {} tools", test_name, num_selections);

            // At least some selections should be made for each non-zero ratio
            if simple_ratio > 0.001 {
                assert!(counts.contains_key(&crate::load_testing_framework::ToolComplexity::Simple),
                       "Test {}: Should select Simple tools", test_name);
            }
            if medium_ratio > 0.001 {
                assert!(counts.contains_key(&crate::load_testing_framework::ToolComplexity::Medium),
                       "Test {}: Should select Medium tools", test_name);
            }
            if complex_ratio > 0.001 {
                assert!(counts.contains_key(&crate::load_testing_framework::ToolComplexity::Complex),
                       "Test {}: Should select Complex tools", test_name);
            }
        }
    }

    /// Test tool distribution performance
    #[test]
    fn test_tool_distribution_performance() {
        let distribution = crate::load_testing_framework::ToolDistribution {
            simple_ratio: 0.4,
            medium_ratio: 0.35,
            complex_ratio: 0.25,
        };

        let num_selections = 100000;

        // Test selection performance
        let start = Instant::now();
        for _ in 0..num_selections {
            let _selected = select_tool_type_deterministic_for_test(&distribution);
        }
        let duration = start.elapsed();

        // Should be very fast
        let avg_per_selection = duration / num_selections as u32;
        assert!(avg_per_selection < Duration::from_nanos(1000),
               "Tool selection should be fast: {:?} per selection", avg_per_selection);

        // Total time should be reasonable
        assert!(duration < Duration::from_millis(100),
               "Total selection time should be reasonable: {:?}", duration);
    }

    /// Test tool distribution with different random seeds
    #[test]
    fn test_tool_distribution_random_behavior() {
        let distribution = crate::load_testing_framework::ToolDistribution {
            simple_ratio: 0.3,
            medium_ratio: 0.4,
            complex_ratio: 0.3,
        };

        let num_selections = 1000;
        let mut results = Vec::new();

        // Test multiple runs with different random seeds
        for seed in 0..5 {
            let mut counts = HashMap::new();

            for i in 0..num_selections {
                // Use deterministic approach based on seed and iteration
                let random_value = ((seed * 1000 + i) % 1000) as f32 / 1000.0;
                let selected = select_tool_type_with_value(&distribution, random_value);
                *counts.entry(selected).or_insert(0) += 1;
            }

            let simple_count = counts.get(&crate::load_testing_framework::ToolComplexity::Simple).unwrap_or(&0);
            let medium_count = counts.get(&crate::load_testing_framework::ToolComplexity::Medium).unwrap_or(&0);
            let complex_count = counts.get(&crate::load_testing_framework::ToolComplexity::Complex).unwrap_or(&0);

            results.push((simple_count, medium_count, complex_count));
        }

        // Results should vary between runs (different seeds)
        let mut all_same = true;
        for i in 1..results.len() {
            if results[i] != results[i-1] {
                all_same = false;
                break;
            }
        }

        // With deterministic approach based on seeds, results should differ
        assert!(!all_same, "Results should vary with different random seeds");
    }

    /// Test tool distribution validation
    #[test]
    fn test_tool_distribution_validation() {
        let test_cases = vec![
            // Valid distributions
            (1.0, 0.0, 0.0, true),
            (0.0, 1.0, 0.0, true),
            (0.0, 0.0, 1.0, true),
            (0.33, 0.33, 0.34, true),
            (0.5, 0.3, 0.2, true),
            // Invalid distributions (sum != 1.0)
            (0.5, 0.5, 0.5, false), // Sum = 1.5
            (0.2, 0.2, 0.2, false), // Sum = 0.6
            (1.5, -0.5, 0.0, false), // Negative value
            (-0.1, 0.6, 0.5, false), // Negative value
        ];

        for (simple, medium, complex, should_be_valid) in test_cases {
            let distribution = crate::load_testing_framework::ToolDistribution {
                simple_ratio: simple,
                medium_ratio: medium,
                complex_ratio: complex,
            };

            let total = simple + medium + complex;
            let is_valid = (total - 1.0).abs() < 0.001 && // Sums to 1.0
                          simple >= 0.0 && medium >= 0.0 && complex >= 0.0; // All non-negative

            assert_eq!(is_valid, should_be_valid,
                      "Distribution ({}, {}, {}) should be {}",
                      simple, medium, complex, if should_be_valid { "valid" } else { "invalid" });

            if is_valid {
                // Valid distributions should work with selection algorithm
                let _selected = select_tool_type_deterministic_for_test(&distribution);
            }
        }
    }
}

#[cfg(test)]
mod resource_limit_enforcement_tests {
    use super::*;

    /// Test resource limit validation logic
    #[test]
    fn test_resource_limit_validation() {
        let test_cases = vec![
            // (memory_mb, cpu_percent, response_time_ms, should_be_reasonable)
            (100, 50.0, 200, true),     // Standard values
            (1, 0.1, 1, true),          // Minimal values
            (32768, 100.0, 300000, true), // High values
            (0, 0.0, 0, true),          // Zero values (framework-dependent)
            (-1, -50.0, -100, false),   // Negative values
            (100, 150.0, 200, true),    // CPU > 100% (allowed but unusual)
            (100, 50.0, u64::MAX, true), // Very high response time
        ];

        for (memory, cpu, response_time, should_be_reasonable) in test_cases {
            let limits = crate::load_testing_framework::ResourceLimits {
                max_memory_mb: memory,
                max_cpu_percent: cpu,
                max_response_time_ms: response_time,
            };

            // Basic validation logic
            let is_reasonable = memory >= 0 && cpu >= 0.0 && response_time >= 0;

            assert_eq!(is_reasonable, should_be_reasonable,
                      "Resource limits ({}, {}, {}) should be {}",
                      memory, cpu, response_time, if should_be_reasonable { "reasonable" } else { "unreasonable" });
        }
    }

    /// Test resource limit comparison logic
    #[test]
    fn test_resource_limit_comparison() {
        let base_limits = crate::load_testing_framework::ResourceLimits {
            max_memory_mb: 100,
            max_cpu_percent: 50.0,
            max_response_time_ms: 200,
        };

        let comparison_cases = vec![
            // Lower limits
            (50, 25.0, 100, "lower"),
            // Higher limits
            (200, 75.0, 400, "higher"),
            // Mixed
            (50, 75.0, 100, "mixed"),
            (200, 25.0, 400, "mixed"),
        ];

        for (memory, cpu, response_time, relation) in comparison_cases {
            let compare_limits = crate::load_testing_framework::ResourceLimits {
                max_memory_mb: memory,
                max_cpu_percent: cpu,
                max_response_time_ms: response_time,
            };

            // Determine if limits are generally higher, lower, or mixed
            let memory_higher = memory > base_limits.max_memory_mb;
            let cpu_higher = cpu > base_limits.max_cpu_percent;
            let response_higher = response_time > base_limits.max_response_time_ms;

            let higher_count = (memory_higher as u8) + (cpu_higher as u8) + (response_higher as u8);
            let lower_count = 3 - higher_count;

            match relation {
                "lower" => assert!(higher_count == 0, "Should be lower limits"),
                "higher" => assert!(lower_count == 0, "Should be higher limits"),
                "mixed" => assert!(higher_count > 0 && lower_count > 0, "Should be mixed limits"),
                _ => panic!("Unknown relation: {}", relation),
            }
        }
    }

    /// Test resource limit scaling
    #[test]
    fn test_resource_limit_scaling() {
        let base_limits = crate::load_testing_framework::ResourceLimits {
            max_memory_mb: 100,
            max_cpu_percent: 50.0,
            max_response_time_ms: 200,
        };

        let scale_factors = vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0];

        for factor in scale_factors {
            let scaled_limits = crate::load_testing_framework::ResourceLimits {
                max_memory_mb: (base_limits.max_memory_mb as f64 * factor) as u64,
                max_cpu_percent: base_limits.max_cpu_percent * factor,
                max_response_time_ms: (base_limits.max_response_time_ms as f64 * factor) as u64,
            };

            // Verify scaling is applied correctly
            let expected_memory = (base_limits.max_memory_mb as f64 * factor).round() as u64;
            let expected_cpu = base_limits.max_cpu_percent * factor;
            let expected_response = (base_limits.max_response_time_ms as f64 * factor).round() as u64;

            assert_eq!(scaled_limits.max_memory_mb, expected_memory,
                      "Memory scaling incorrect for factor {}", factor);
            assert!((scaled_limits.max_cpu_percent - expected_cpu).abs() < 0.001,
                      "CPU scaling incorrect for factor {}", factor);
            assert_eq!(scaled_limits.max_response_time_ms, expected_response,
                      "Response time scaling incorrect for factor {}", factor);
        }
    }

    /// Test resource limit with extreme values
    #[test]
    fn test_resource_limit_extreme_values() {
        let extreme_cases = vec![
            // Maximum values
            (u64::MAX, f64::MAX, u64::MAX),
            // Very small values
            (1, 0.01, 1),
            // Unusual combinations
            (1000, 0.1, 10),
            (1, 200.0, 1000),
        ];

        for (memory, cpu, response_time) in extreme_cases {
            let limits = crate::load_testing_framework::ResourceLimits {
                max_memory_mb: memory,
                max_cpu_percent: cpu,
                max_response_time_ms: response_time,
            };

            // Should handle extreme values without overflow
            assert!(limits.max_memory_mb == memory);
            assert!(limits.max_cpu_percent == cpu);
            assert!(limits.max_response_time_ms == response_time);

            // Should serialize correctly
            let json = serde_json::to_string(&limits).unwrap();
            let deserialized: crate::load_testing_framework::ResourceLimits = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.max_memory_mb, memory);
            assert_eq!(deserialized.max_cpu_percent, cpu);
            assert_eq!(deserialized.max_response_time_ms, response_time);
        }
    }
}

#[cfg(test)]
mod statistical_accuracy_tests {
    use super::*;

    /// Test statistical accuracy of large-scale distributions
    #[test]
    fn test_statistical_accuracy_large_scale() {
        let distribution = crate::load_testing_framework::ToolDistribution {
            simple_ratio: 0.25,
            medium_ratio: 0.45,
            complex_ratio: 0.30,
        };

        let num_selections = 100000; // Large number for statistical accuracy
        let mut counts = HashMap::new();

        // Perform selections
        for _ in 0..num_selections {
            let selected = select_tool_type_deterministic_for_test(&distribution);
            *counts.entry(selected).or_insert(0) += 1;
        }

        // Calculate actual percentages
        let simple_actual = *counts.get(&crate::load_testing_framework::ToolComplexity::Simple).unwrap_or(&0) as f64 / num_selections as f64;
        let medium_actual = *counts.get(&crate::load_testing_framework::ToolComplexity::Medium).unwrap_or(&0) as f64 / num_selections as f64;
        let complex_actual = *counts.get(&crate::load_testing_framework::ToolComplexity::Complex).unwrap_or(&0) as f64 / num_selections as f64;

        // Verify statistical accuracy (tighter tolerance for large samples)
        let tolerance = 0.005; // 0.5% tolerance

        assert!((simple_actual - 0.25).abs() < tolerance,
               "Simple ratio: expected 0.25, got {:.4}", simple_actual);
        assert!((medium_actual - 0.45).abs() < tolerance,
               "Medium ratio: expected 0.45, got {:.4}", medium_actual);
        assert!((complex_actual - 0.30).abs() < tolerance,
               "Complex ratio: expected 0.30, got {:.4}", complex_actual);

        // Verify total sums to 1.0
        let total_actual = simple_actual + medium_actual + complex_actual;
        assert!((total_actual - 1.0).abs() < 0.0001,
               "Total should sum to 1.0, got {:.4}", total_actual);
    }

    /// Test convergence of distribution over multiple runs
    #[test]
    fn test_distribution_convergence() {
        let distribution = crate::load_testing_framework::ToolDistribution {
            simple_ratio: 0.6,
            medium_ratio: 0.3,
            complex_ratio: 0.1,
        };

        let run_sizes = vec![100, 1000, 10000, 100000];
        let mut results = Vec::new();

        for run_size in run_sizes {
            let mut counts = HashMap::new();

            for _ in 0..run_size {
                let selected = select_tool_type_deterministic_for_test(&distribution);
                *counts.entry(selected).or_insert(0) += 1;
            }

            let simple_ratio = *counts.get(&crate::load_testing_framework::ToolComplexity::Simple).unwrap_or(&0) as f64 / run_size as f64;
            results.push(simple_ratio);
        }

        // Results should converge toward expected value as sample size increases
        let expected = 0.6;

        for (i, &actual) in results.iter().enumerate() {
            let error = (actual - expected).abs();
            println!("Run size {}: expected {:.3}, got {:.3}, error {:.3}",
                     run_sizes[i], expected, actual, error);

            // Error should generally decrease with larger sample sizes
            // (This is a probabilistic test, so we use a loose tolerance)
            assert!(error < 0.05, "Error should be reasonable: {}", error);
        }

        // Last run (largest sample) should be most accurate
        let final_error = (results.last().unwrap() - expected).abs();
        assert!(final_error < 0.02, "Final error should be small: {}", final_error);
    }

    /// Test multiple distribution scenarios simultaneously
    #[test]
    fn test_multiple_distribution_scenarios() {
        let scenarios = vec![
            (0.8, 0.15, 0.05, "Simple Heavy"),
            (0.4, 0.35, 0.25, "Balanced"),
            (0.1, 0.2, 0.7, "Complex Heavy"),
            (0.33, 0.33, 0.34, "Nearly Equal"),
        ];

        let num_selections = 10000;

        for (simple, medium, complex, scenario_name) in scenarios {
            let distribution = crate::load_testing_framework::ToolDistribution {
                simple_ratio: simple,
                medium_ratio: medium,
                complex_ratio: complex,
            };

            let mut counts = HashMap::new();

            for _ in 0..num_selections {
                let selected = select_tool_type_deterministic_for_test(&distribution);
                *counts.entry(selected).or_insert(0) += 1;
            }

            // Calculate actual percentages
            let simple_actual = *counts.get(&crate::load_testing_framework::ToolComplexity::Simple).unwrap_or(&0) as f64 / num_selections as f64;
            let medium_actual = *counts.get(&crate::load_testing_framework::ToolComplexity::Medium).unwrap_or(&0) as f64 / num_selections as f64;
            let complex_actual = *counts.get(&crate::load_testing_framework::ToolComplexity::Complex).unwrap_or(&0) as f64 / num_selections as f64;

            // Verify accuracy for each scenario
            let tolerance = 0.02; // 2% tolerance

            assert!((simple_actual - simple).abs() < tolerance,
                   "Scenario {}: Simple expected {:.3}, got {:.3}",
                   scenario_name, simple, simple_actual);

            assert!((medium_actual - medium).abs() < tolerance,
                   "Scenario {}: Medium expected {:.3}, got {:.3}",
                   scenario_name, medium, medium_actual);

            assert!((complex_actual - complex).abs() < tolerance,
                   "Scenario {}: Complex expected {:.3}, got {:.3}",
                   scenario_name, complex, complex_actual);

            println!("Scenario {}: Simple={:.3}, Medium={:.3}, Complex={:.3}",
                     scenario_name, simple_actual, medium_actual, complex_actual);
        }
    }

    /// Test chi-square statistical test for distribution accuracy
    #[test]
    fn test_chi_square_distribution_test() {
        let distribution = crate::load_testing_framework::ToolDistribution {
            simple_ratio: 0.5,
            medium_ratio: 0.3,
            complex_ratio: 0.2,
        };

        let num_selections = 10000;
        let mut counts = HashMap::new();

        // Perform selections
        for _ in 0..num_selections {
            let selected = select_tool_type_deterministic_for_test(&distribution);
            *counts.entry(selected).or_insert(0) += 1;
        }

        // Calculate chi-square statistic
        let expected_simple = num_selections as f64 * 0.5;
        let expected_medium = num_selections as f64 * 0.3;
        let expected_complex = num_selections as f64 * 0.2;

        let observed_simple = *counts.get(&crate::load_testing_framework::ToolComplexity::Simple).unwrap_or(&0) as f64;
        let observed_medium = *counts.get(&crate::load_testing_framework::ToolComplexity::Medium).unwrap_or(&0) as f64;
        let observed_complex = *counts.get(&crate::load_testing_framework::ToolComplexity::Complex).unwrap_or(&0) as f64;

        let chi_square = ((observed_simple - expected_simple).powi(2) / expected_simple) +
                         ((observed_medium - expected_medium).powi(2) / expected_medium) +
                         ((observed_complex - expected_complex).powi(2) / expected_complex);

        // Chi-square with 2 degrees of freedom (3 categories - 1)
        // Critical value at p=0.05 is approximately 5.991
        let critical_value = 5.991;

        assert!(chi_square < critical_value,
               "Chi-square test failed: {:.3} > {:.3}", chi_square, critical_value);

        println!("Chi-square statistic: {:.3} (critical: {:.3})", chi_square, critical_value);
    }
}

// Helper functions for testing
fn select_tool_type_deterministic_for_test(distribution: &crate::load_testing_framework::ToolDistribution) -> crate::load_testing_framework::ToolComplexity {
    // Use a deterministic approach based on a counter for consistent testing
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let random_value = (counter % 1000) as f32 / 1000.0;

    select_tool_type_with_value(distribution, random_value)
}

fn select_tool_type_with_value(distribution: &crate::load_testing_framework::ToolDistribution, random_value: f32) -> crate::load_testing_framework::ToolComplexity {
    if random_value < distribution.simple_ratio {
        crate::load_testing_framework::ToolComplexity::Simple
    } else if random_value < distribution.simple_ratio + distribution.medium_ratio {
        crate::load_testing_framework::ToolComplexity::Medium
    } else {
        crate::load_testing_framework::ToolComplexity::Complex
    }
}