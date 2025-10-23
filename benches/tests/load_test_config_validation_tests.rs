//! Comprehensive unit tests for LoadTestConfig validation and edge cases
//!
//! Specialized testing for configuration validation ensuring robust
//! handling of various configuration scenarios and edge cases

use std::time::{Duration, Instant};
use serde_json;

#[cfg(test)]
mod load_test_config_validation_tests {
    use super::*;

    /// Test LoadTestConfig creation with valid parameters
    #[test]
    fn test_valid_config_creation() {
        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Valid Test Config".to_string(),
            duration: Duration::from_secs(60),
            concurrency: 10,
            ramp_up_time: Duration::from_secs(10),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.6,
                medium_ratio: 0.3,
                complex_ratio: 0.1,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 50.0,
                max_response_time_ms: 200,
            },
        };

        // Verify all fields are set correctly
        assert_eq!(config.name, "Valid Test Config");
        assert_eq!(config.duration, Duration::from_secs(60));
        assert_eq!(config.concurrency, 10);
        assert_eq!(config.ramp_up_time, Duration::from_secs(10));
        assert_eq!(config.tool_distribution.simple_ratio, 0.6);
        assert_eq!(config.tool_distribution.medium_ratio, 0.3);
        assert_eq!(config.tool_distribution.complex_ratio, 0.1);
        assert_eq!(config.resource_limits.max_memory_mb, 100);
        assert_eq!(config.resource_limits.max_cpu_percent, 50.0);
        assert_eq!(config.resource_limits.max_response_time_ms, 200);
    }

    /// Test LoadTestConfig with minimal valid values
    #[test]
    fn test_minimal_valid_config() {
        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Minimal Test".to_string(),
            duration: Duration::from_millis(1), // Smallest non-zero duration
            concurrency: 1, // Minimum concurrency
            ramp_up_time: Duration::from_millis(1), // Smallest ramp-up
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0, // All simple tools
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 1, // Minimal memory limit
                max_cpu_percent: 0.1, // Minimal CPU limit
                max_response_time_ms: 1, // Minimal response time limit
            },
        };

        // Should accept minimal values
        assert_eq!(config.name, "Minimal Test");
        assert_eq!(config.concurrency, 1);
        assert_eq!(config.tool_distribution.simple_ratio, 1.0);
        assert_eq!(config.tool_distribution.medium_ratio, 0.0);
        assert_eq!(config.tool_distribution.complex_ratio, 0.0);
    }

    /// Test LoadTestConfig with maximum reasonable values
    #[test]
    fn test_maximum_valid_config() {
        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Maximum Test".to_string(),
            duration: Duration::from_secs(3600), // 1 hour
            concurrency: 1000, // High concurrency
            ramp_up_time: Duration::from_secs(300), // 5 minutes ramp-up
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.33,
                medium_ratio: 0.33,
                complex_ratio: 0.34,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 32768, // 32GB
                max_cpu_percent: 100.0,
                max_response_time_ms: 300000, // 5 minutes
            },
        };

        // Should handle large values
        assert_eq!(config.duration, Duration::from_secs(3600));
        assert_eq!(config.concurrency, 1000);
        assert_eq!(config.ramp_up_time, Duration::from_secs(300));
    }

    /// Test LoadTestConfig with empty name
    #[test]
    fn test_empty_name_config() {
        let config = crate::load_testing_framework::LoadTestConfig {
            name: "".to_string(),
            duration: Duration::from_secs(60),
            concurrency: 10,
            ramp_up_time: Duration::from_secs(10),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.6,
                medium_ratio: 0.3,
                complex_ratio: 0.1,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 50.0,
                max_response_time_ms: 200,
            },
        };

        // Should accept empty name (though not recommended)
        assert_eq!(config.name, "");
    }

    /// Test LoadTestConfig with very long name
    #[test]
    fn test_very_long_name_config() {
        let long_name = "A".repeat(1000);
        let config = crate::load_testing_framework::LoadTestConfig {
            name: long_name.clone(),
            duration: Duration::from_secs(60),
            concurrency: 10,
            ramp_up_time: Duration::from_secs(10),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.6,
                medium_ratio: 0.3,
                complex_ratio: 0.1,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 50.0,
                max_response_time_ms: 200,
            },
        };

        // Should handle very long names
        assert_eq!(config.name, long_name);
        assert_eq!(config.name.len(), 1000);
    }

    /// Test LoadTestConfig with zero duration
    #[test]
    fn test_zero_duration_config() {
        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Zero Duration Test".to_string(),
            duration: Duration::ZERO,
            concurrency: 10,
            ramp_up_time: Duration::from_secs(10),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 50.0,
                max_response_time_ms: 200,
            },
        };

        // Should handle zero duration (framework will need to handle this case)
        assert_eq!(config.duration, Duration::ZERO);
    }

    /// Test LoadTestConfig with zero concurrency
    #[test]
    fn test_zero_concurrency_config() {
        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Zero Concurrency Test".to_string(),
            duration: Duration::from_secs(60),
            concurrency: 0,
            ramp_up_time: Duration::from_secs(10),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 50.0,
                max_response_time_ms: 200,
            },
        };

        // Should handle zero concurrency (framework will need to handle this case)
        assert_eq!(config.concurrency, 0);
    }

    /// Test LoadTestConfig with zero ramp-up time
    #[test]
    fn test_zero_ramp_up_config() {
        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Zero Ramp Up Test".to_string(),
            duration: Duration::from_secs(60),
            concurrency: 10,
            ramp_up_time: Duration::ZERO,
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 1.0,
                medium_ratio: 0.0,
                complex_ratio: 0.0,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: 50.0,
                max_response_time_ms: 200,
            },
        };

        // Should handle zero ramp-up time
        assert_eq!(config.ramp_up_time, Duration::ZERO);
    }

    /// Test LoadTestConfig serialization
    #[test]
    fn test_config_serialization() {
        let config = crate::load_testing_framework::LoadTestConfig {
            name: "Serialization Test".to_string(),
            duration: Duration::from_secs(120),
            concurrency: 25,
            ramp_up_time: Duration::from_secs(15),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.5,
                medium_ratio: 0.3,
                complex_ratio: 0.2,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 200,
                max_cpu_percent: 75.0,
                max_response_time_ms: 300,
            },
        };

        // Test JSON serialization
        let json_result = serde_json::to_string(&config);
        assert!(json_result.is_ok(), "Config should serialize to JSON");

        let json_str = json_result.unwrap();

        // Test JSON deserialization
        let deserialized_result: Result<crate::load_testing_framework::LoadTestConfig, _> = serde_json::from_str(&json_str);
        assert!(deserialized_result.is_ok(), "Config should deserialize from JSON");

        let deserialized_config = deserialized_result.unwrap();

        // Verify deserialized config matches original
        assert_eq!(deserialized_config.name, config.name);
        assert_eq!(deserialized_config.duration, config.duration);
        assert_eq!(deserialized_config.concurrency, config.concurrency);
        assert_eq!(deserialized_config.ramp_up_time, config.ramp_up_time);
        assert_eq!(deserialized_config.tool_distribution.simple_ratio, config.tool_distribution.simple_ratio);
        assert_eq!(deserialized_config.tool_distribution.medium_ratio, config.tool_distribution.medium_ratio);
        assert_eq!(deserialized_config.tool_distribution.complex_ratio, config.tool_distribution.complex_ratio);
        assert_eq!(deserialized_config.resource_limits.max_memory_mb, config.resource_limits.max_memory_mb);
        assert_eq!(deserialized_config.resource_limits.max_cpu_percent, config.resource_limits.max_cpu_percent);
        assert_eq!(deserialized_config.resource_limits.max_response_time_ms, config.resource_limits.max_response_time_ms);
    }

    /// Test LoadTestConfig with special characters in name
    #[test]
    fn test_special_characters_name() {
        let special_names = vec![
            "Test with spaces".to_string(),
            "Test-with-dashes".to_string(),
            "Test_with_underscores".to_string(),
            "Test.with.dots".to_string(),
            "测试中文".to_string(), // Chinese characters
            "🚀 Rocket Test".to_string(), // Emoji
            "Test\nwith\nnewlines".to_string(),
            "Test\twith\ttabs".to_string(),
            "Test\"with\"quotes".to_string(),
            "Test\\with\\backslashes".to_string(),
        ];

        for name in special_names {
            let config = crate::load_testing_framework::LoadTestConfig {
                name: name.clone(),
                duration: Duration::from_secs(60),
                concurrency: 10,
                ramp_up_time: Duration::from_secs(10),
                tool_distribution: crate::load_testing_framework::ToolDistribution {
                    simple_ratio: 1.0,
                    medium_ratio: 0.0,
                    complex_ratio: 0.0,
                },
                resource_limits: crate::load_testing_framework::ResourceLimits {
                    max_memory_mb: 100,
                    max_cpu_percent: 50.0,
                    max_response_time_ms: 200,
                },
            };

            // Should handle special characters
            assert_eq!(config.name, name);

            // Should serialize and deserialize correctly
            let json = serde_json::to_string(&config).unwrap();
            let deserialized: crate::load_testing_framework::LoadTestConfig = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.name, name);
        }
    }
}

#[cfg(test)]
mod tool_distribution_validation_tests {
    use super::*;

    /// Test valid tool distribution configurations
    #[test]
    fn test_valid_tool_distributions() {
        let valid_distributions = vec![
            (1.0, 0.0, 0.0), // All simple
            (0.0, 1.0, 0.0), // All medium
            (0.0, 0.0, 1.0), // All complex
            (0.5, 0.3, 0.2), // Mixed
            (0.33, 0.33, 0.34), // Nearly equal
            (0.1, 0.1, 0.8), // Complex-heavy
            (0.8, 0.1, 0.1), // Simple-heavy
            (0.3, 0.4, 0.3), // Medium-heavy
        ];

        for (simple, medium, complex) in valid_distributions {
            let distribution = crate::load_testing_framework::ToolDistribution {
                simple_ratio: simple,
                medium_ratio: medium,
                complex_ratio: complex,
            };

            // Verify distribution sums to 1.0 (within floating point tolerance)
            let total = distribution.simple_ratio + distribution.medium_ratio + distribution.complex_ratio;
            assert!((total - 1.0).abs() < 0.001,
                   "Distribution ({}, {}, {}) should sum to 1.0, got {}", simple, medium, complex, total);

            // Verify individual values are valid
            assert!(distribution.simple_ratio >= 0.0 && distribution.simple_ratio <= 1.0);
            assert!(distribution.medium_ratio >= 0.0 && distribution.medium_ratio <= 1.0);
            assert!(distribution.complex_ratio >= 0.0 && distribution.complex_ratio <= 1.0);
        }
    }

    /// Test tool distribution with floating point edge cases
    #[test]
    fn test_floating_point_edge_cases() {
        let edge_cases = vec![
            (0.0, 0.0, 1.0),
            (1.0, 0.0, 0.0),
            (0.0, 1.0, 0.0),
            (0.999999, 0.000001, 0.0), // Very small values
            (0.3333333, 0.3333333, 0.3333334), // Floating point precision
            (0.1, 0.2, 0.7), // Standard case
        ];

        for (simple, medium, complex) in edge_cases {
            let distribution = crate::load_testing_framework::ToolDistribution {
                simple_ratio: simple,
                medium_ratio: medium,
                complex_ratio: complex,
            };

            let total = distribution.simple_ratio + distribution.medium_ratio + distribution.complex_ratio;
            assert!((total - 1.0).abs() < 0.00001, // Tighter tolerance for edge cases
                   "Edge case distribution ({}, {}, {}) should sum to 1.0", simple, medium, complex);
        }
    }

    /// Test tool distribution serialization
    #[test]
    fn test_tool_distribution_serialization() {
        let distribution = crate::load_testing_framework::ToolDistribution {
            simple_ratio: 0.6,
            medium_ratio: 0.3,
            complex_ratio: 0.1,
        };

        // Test serialization
        let json = serde_json::to_string(&distribution).unwrap();
        assert!(json.contains("simple_ratio"));
        assert!(json.contains("medium_ratio"));
        assert!(json.contains("complex_ratio"));

        // Test deserialization
        let deserialized: crate::load_testing_framework::ToolDistribution = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.simple_ratio, 0.6);
        assert_eq!(deserialized.medium_ratio, 0.3);
        assert_eq!(deserialized.complex_ratio, 0.1);
    }

    /// Test tool distribution with extreme values
    #[test]
    fn test_extreme_tool_distributions() {
        let extreme_cases = vec![
            (0.999999, 0.0000005, 0.0000005), // Almost all simple
            (0.0000005, 0.999999, 0.0000005), // Almost all medium
            (0.0000005, 0.0000005, 0.999999), // Almost all complex
            (0.000001, 0.000001, 0.999998), // Very small values
        ];

        for (simple, medium, complex) in extreme_cases {
            let distribution = crate::load_testing_framework::ToolDistribution {
                simple_ratio: simple,
                medium_ratio: medium,
                complex_ratio: complex,
            };

            let total = distribution.simple_ratio + distribution.medium_ratio + distribution.complex_ratio;
            assert!((total - 1.0).abs() < 0.00001,
                   "Extreme distribution should sum to 1.0: ({}, {}, {}) = {}", simple, medium, complex, total);
        }
    }
}

#[cfg(test)]
mod resource_limits_validation_tests {
    use super::*;

    /// Test valid resource limits configurations
    #[test]
    fn test_valid_resource_limits() {
        let valid_limits = vec![
            (1, 0.1, 1),      // Minimal values
            (100, 50.0, 100), // Standard values
            (1024, 100.0, 1000), // High values
            (32768, 100.0, 300000), // Very high values
        ];

        for (memory, cpu, response_time) in valid_limits {
            let limits = crate::load_testing_framework::ResourceLimits {
                max_memory_mb: memory,
                max_cpu_percent: cpu,
                max_response_time_ms: response_time,
            };

            // Verify limits are set correctly
            assert_eq!(limits.max_memory_mb, memory);
            assert_eq!(limits.max_cpu_percent, cpu);
            assert_eq!(limits.max_response_time_ms, response_time);

            // Verify limits are reasonable
            assert!(limits.max_memory_mb > 0);
            assert!(limits.max_cpu_percent > 0.0);
            assert!(limits.max_response_time_ms > 0);
        }
    }

    /// Test resource limits with zero values
    #[test]
    fn test_zero_resource_limits() {
        let limits = crate::load_testing_framework::ResourceLimits {
            max_memory_mb: 0,
            max_cpu_percent: 0.0,
            max_response_time_ms: 0,
        };

        // Should handle zero limits (framework may need to handle these cases)
        assert_eq!(limits.max_memory_mb, 0);
        assert_eq!(limits.max_cpu_percent, 0.0);
        assert_eq!(limits.max_response_time_ms, 0);
    }

    /// Test resource limits with very high values
    #[test]
    fn test_very_high_resource_limits() {
        let limits = crate::load_testing_framework::ResourceLimits {
            max_memory_mb: u64::MAX / 1024 / 1024, // Maximum reasonable memory in MB
            max_cpu_percent: 1000.0, // Unusually high but allowed
            max_response_time_ms: u64::MAX, // Maximum value
        };

        // Should handle very high limits
        assert!(limits.max_memory_mb > 0);
        assert!(limits.max_cpu_percent > 100.0);
        assert!(limits.max_response_time_ms > 0);
    }

    /// Test resource limits with fractional CPU values
    #[test]
    fn test_fractional_cpu_limits() {
        let fractional_values = vec![0.1, 0.5, 0.75, 12.5, 33.33, 99.99];

        for cpu_percent in fractional_values {
            let limits = crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 100,
                max_cpu_percent: cpu_percent,
                max_response_time_ms: 200,
            };

            assert_eq!(limits.max_cpu_percent, cpu_percent);
            assert!(limits.max_cpu_percent > 0.0);
        }
    }

    /// Test resource limits serialization
    #[test]
    fn test_resource_limits_serialization() {
        let limits = crate::load_testing_framework::ResourceLimits {
            max_memory_mb: 512,
            max_cpu_percent: 75.5,
            max_response_time_ms: 1500,
        };

        // Test serialization
        let json = serde_json::to_string(&limits).unwrap();
        assert!(json.contains("max_memory_mb"));
        assert!(json.contains("max_cpu_percent"));
        assert!(json.contains("max_response_time_ms"));

        // Test deserialization
        let deserialized: crate::load_testing_framework::ResourceLimits = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.max_memory_mb, 512);
        assert_eq!(deserialized.max_cpu_percent, 75.5);
        assert_eq!(deserialized.max_response_time_ms, 1500);
    }
}

#[cfg(test)]
mod config_integration_tests {
    use super::*;

    /// Test LoadTestConfig with various duration and ramp-up combinations
    #[test]
    fn test_duration_ramp_up_combinations() {
        let combinations = vec![
            (Duration::from_secs(60), Duration::from_secs(10)),    // Normal
            (Duration::from_secs(10), Duration::from_secs(20)),    // Ramp-up longer than duration
            (Duration::from_secs(10), Duration::from_secs(10)),    // Equal duration and ramp-up
            (Duration::from_secs(100), Duration::from_secs(1)),    // Very short ramp-up
            (Duration::from_secs(1), Duration::from_secs(100)),    // Very long ramp-up
            (Duration::from_millis(100), Duration::from_millis(50)), // Milliseconds
        ];

        for (duration, ramp_up) in combinations {
            let config = crate::load_testing_framework::LoadTestConfig {
                name: format!("Duration-RampUp Test - {:?}-{:?}", duration, ramp_up),
                duration,
                concurrency: 10,
                ramp_up_time: ramp_up,
                tool_distribution: crate::load_testing_framework::ToolDistribution {
                    simple_ratio: 1.0,
                    medium_ratio: 0.0,
                    complex_ratio: 0.0,
                },
                resource_limits: crate::load_testing_framework::ResourceLimits {
                    max_memory_mb: 100,
                    max_cpu_percent: 50.0,
                    max_response_time_ms: 200,
                },
            };

            // Should handle various duration/ramp-up combinations
            assert_eq!(config.duration, duration);
            assert_eq!(config.ramp_up_time, ramp_up);

            // Should serialize correctly
            let json = serde_json::to_string(&config).unwrap();
            let deserialized: crate::load_testing_framework::LoadTestConfig = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.duration, duration);
            assert_eq!(deserialized.ramp_up_time, ramp_up);
        }
    }

    /// Test LoadTestConfig with various concurrency levels
    #[test]
    fn test_various_concurrency_levels() {
        let concurrency_levels = vec![0, 1, 2, 5, 10, 50, 100, 1000, 10000];

        for concurrency in concurrency_levels {
            let config = crate::load_testing_framework::LoadTestConfig {
                name: format!("Concurrency Test - {}", concurrency),
                duration: Duration::from_secs(60),
                concurrency,
                ramp_up_time: Duration::from_secs(10),
                tool_distribution: crate::load_testing_framework::ToolDistribution {
                    simple_ratio: 1.0,
                    medium_ratio: 0.0,
                    complex_ratio: 0.0,
                },
                resource_limits: crate::load_testing_framework::ResourceLimits {
                    max_memory_mb: 100,
                    max_cpu_percent: 50.0,
                    max_response_time_ms: 200,
                },
            };

            // Should handle various concurrency levels
            assert_eq!(config.concurrency, concurrency);

            // Should handle very high concurrency (framework may need to limit this)
            assert!(config.concurrency >= 0);
        }
    }

    /// Test LoadTestConfig with extreme resource limits and tool distributions
    #[test]
    fn test_extreme_configurations() {
        let extreme_configs = vec![
            // Very low resource limits with simple tools
            crate::load_testing_framework::LoadTestConfig {
                name: "Low Resources - Simple".to_string(),
                duration: Duration::from_secs(10),
                concurrency: 1,
                ramp_up_time: Duration::from_millis(100),
                tool_distribution: crate::load_testing_framework::ToolDistribution {
                    simple_ratio: 1.0,
                    medium_ratio: 0.0,
                    complex_ratio: 0.0,
                },
                resource_limits: crate::load_testing_framework::ResourceLimits {
                    max_memory_mb: 1,
                    max_cpu_percent: 1.0,
                    max_response_time_ms: 1,
                },
            },
            // Very high resource limits with complex tools
            crate::load_testing_framework::LoadTestConfig {
                name: "High Resources - Complex".to_string(),
                duration: Duration::from_secs(3600),
                concurrency: 1000,
                ramp_up_time: Duration::from_secs(300),
                tool_distribution: crate::load_testing_framework::ToolDistribution {
                    simple_ratio: 0.0,
                    medium_ratio: 0.0,
                    complex_ratio: 1.0,
                },
                resource_limits: crate::load_testing_framework::ResourceLimits {
                    max_memory_mb: 32768,
                    max_cpu_percent: 100.0,
                    max_response_time_ms: 300000,
                },
            },
        ];

        for config in extreme_configs {
            // Should handle extreme configurations
            assert!(!config.name.is_empty());
            assert!(config.duration > Duration::ZERO);
            assert!(config.concurrency >= 0);

            // Should serialize correctly
            let json = serde_json::to_string(&config).unwrap();
            let deserialized: crate::load_testing_framework::LoadTestConfig = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.name, config.name);
        }
    }

    /// Test LoadTestConfig with negative values (if somehow set)
    #[test]
    fn test_negative_values_handling() {
        // Note: This test verifies that negative values are stored correctly
        // even though they might not be logically valid for load testing

        let config_with_negatives = crate::load_testing_framework::LoadTestConfig {
            name: "Negative Values Test".to_string(),
            duration: Duration::from_secs(60),
            concurrency: 10,
            ramp_up_time: Duration::from_secs(10),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: -0.1, // Negative ratio (invalid but should be stored)
                medium_ratio: 0.6,
                complex_ratio: 0.5, // This makes total > 1.0
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: -100, // Negative memory
                max_cpu_percent: -50.0, // Negative CPU
                max_response_time_ms: -200, // Negative response time
            },
        };

        // Should store negative values (validation would happen elsewhere)
        assert_eq!(config_with_negatives.tool_distribution.simple_ratio, -0.1);
        assert_eq!(config_with_negatives.resource_limits.max_memory_mb, -100);
        assert_eq!(config_with_negatives.resource_limits.max_cpu_percent, -50.0);
        assert_eq!(config_with_negatives.resource_limits.max_response_time_ms, -200);

        // Should still serialize and deserialize
        let json = serde_json::to_string(&config_with_negatives).unwrap();
        let deserialized: crate::load_testing_framework::LoadTestConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.tool_distribution.simple_ratio, -0.1);
        assert_eq!(deserialized.resource_limits.max_memory_mb, -100);
    }

    /// Test LoadTestConfig clone functionality
    #[test]
    fn test_config_clone() {
        let original = crate::load_testing_framework::LoadTestConfig {
            name: "Clone Test".to_string(),
            duration: Duration::from_secs(120),
            concurrency: 25,
            ramp_up_time: Duration::from_secs(15),
            tool_distribution: crate::load_testing_framework::ToolDistribution {
                simple_ratio: 0.4,
                medium_ratio: 0.4,
                complex_ratio: 0.2,
            },
            resource_limits: crate::load_testing_framework::ResourceLimits {
                max_memory_mb: 512,
                max_cpu_percent: 75.0,
                max_response_time_ms: 500,
            },
        };

        let cloned = original.clone();

        // Verify all fields are cloned correctly
        assert_eq!(cloned.name, original.name);
        assert_eq!(cloned.duration, original.duration);
        assert_eq!(cloned.concurrency, original.concurrency);
        assert_eq!(cloned.ramp_up_time, original.ramp_up_time);
        assert_eq!(cloned.tool_distribution.simple_ratio, original.tool_distribution.simple_ratio);
        assert_eq!(cloned.tool_distribution.medium_ratio, original.tool_distribution.medium_ratio);
        assert_eq!(cloned.tool_distribution.complex_ratio, original.tool_distribution.complex_ratio);
        assert_eq!(cloned.resource_limits.max_memory_mb, original.resource_limits.max_memory_mb);
        assert_eq!(cloned.resource_limits.max_cpu_percent, original.resource_limits.max_cpu_percent);
        assert_eq!(cloned.resource_limits.max_response_time_ms, original.resource_limits.max_response_time_ms);

        // Verify they are independent (change original, cloned should remain unchanged)
        let mut modified_original = original;
        modified_original.name = "Modified".to_string();
        assert_ne!(cloned.name, modified_original.name);
    }
}