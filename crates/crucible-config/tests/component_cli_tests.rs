//! Tests for CLI component configuration
//!
//! TDD approach: Write failing tests first, then implement functionality.
//! These tests verify system-aware defaults and CPU-friendly optimizations.

use crucible_config::{CliComponentConfig, SystemCapabilities};
use std::path::PathBuf;

#[cfg(test)]
mod cli_component_tests {
    use super::*;

    #[test]
    fn test_cli_component_default_configuration() {
        // Test that system-aware CLI configuration is created successfully
        let config = CliComponentConfig::system_aware().expect("System-aware config should be created");

        assert!(config.enabled, "CLI component should be enabled by default");
        assert!(config.paths.kiln_path.is_some(), "Kiln path should have a default");
        assert!(config.interface.show_progress, "Progress should be shown by default");
        assert!(config.user_interaction.auto_complete, "Auto-complete should be enabled by default");
    }

    #[test]
    fn test_path_config_system_aware_defaults() {
        // Test that path configuration adapts to system capabilities
        let capabilities = SystemCapabilities::detect().expect("System detection should succeed");
        let config = CliComponentConfig::with_system_capabilities(&capabilities);

        // On systems with limited memory, cache paths should be more conservative
        let expected_cache_size_mb = if capabilities.total_memory_gb() < 4.0 {
            50  // Conservative for low-memory systems
        } else if capabilities.total_memory_gb() < 8.0 {
            100 // Moderate for mid-range systems
        } else {
            200 // Generous for high-memory systems
        };

        // This will fail until we implement system-aware defaults
        assert_eq!(
            config.custom.get("cache_size_mb"),
            Some(&serde_json::Value::Number(expected_cache_size_mb.into())),
            "Cache size should adapt to available memory"
        );
    }

    #[test]
    fn test_cpu_optimized_command_timeout() {
        // Test that command timeouts scale with CPU performance
        let capabilities = SystemCapabilities::detect().expect("System detection should succeed");
        let config = CliComponentConfig::with_system_capabilities(&capabilities);

        // Faster CPUs should get shorter timeouts (more responsive)
        // Slower CPUs should get longer timeouts (more time to complete)
        let expected_timeout_seconds = if capabilities.cpu_info.core_count >= 8 {
            30  // Fast systems get shorter timeouts
        } else if capabilities.cpu_info.core_count >= 4 {
            60  // Mid-range systems get moderate timeouts
        } else {
            120 // Slow systems get generous timeouts
        };

        // This will fail until we implement CPU-aware timeouts
        assert_eq!(
            config.interface.command_timeout_seconds,
            expected_timeout_seconds,
            "Command timeout should scale with CPU performance"
        );
    }

    #[test]
    fn test_memory_optimized_batch_sizes() {
        // Test that batch sizes are optimized for available memory
        let capabilities = SystemCapabilities::detect().expect("System detection should succeed");
        let config = CliComponentConfig::with_system_capabilities(&capabilities);

        let expected_batch_size = if capabilities.available_memory_gb() < 2.0 {
            5   // Very small batches for low-memory systems
        } else if capabilities.available_memory_gb() < 4.0 {
            10  // Small batches for moderate systems
        } else {
            20  // Larger batches for memory-rich systems
        };

        // This will fail until we implement memory-aware batch sizes
        assert_eq!(
            config.custom.get("batch_size"),
            Some(&serde_json::Value::Number(expected_batch_size.into())),
            "Batch size should adapt to available memory"
        );
    }

    #[test]
    fn test_concurrent_operations_limit() {
        // Test that concurrent operations are limited by CPU cores
        let capabilities = SystemCapabilities::detect().expect("System detection should succeed");
        let config = CliComponentConfig::with_system_capabilities(&capabilities);

        let expected_max_concurrent = std::cmp::max(
            1,
            std::cmp::min(capabilities.cpu_info.core_count, 4)
        );

        // This will fail until we implement CPU-aware concurrency limits
        assert_eq!(
            config.custom.get("max_concurrent_operations"),
            Some(&serde_json::Value::Number(expected_max_concurrent.into())),
            "Concurrent operations should be limited by CPU cores"
        );
    }

    #[test]
    fn test_low_memory_mode_detection() {
        // Test automatic low-memory mode detection
        let capabilities = SystemCapabilities::detect().expect("System detection should succeed");
        let config = CliComponentConfig::with_system_capabilities(&capabilities);

        let is_low_memory = capabilities.total_memory_gb() < 4.0 ||
                           capabilities.available_memory_gb() < 1.0;

        if is_low_memory {
            assert!(!config.interface.show_progress, "Progress should be disabled in low memory mode");
            assert_eq!(config.custom.get("memory_saver_mode"), Some(&serde_json::Value::Bool(true)),
                      "Memory saver mode should be enabled on low-memory systems");
        }
    }

    #[test]
    fn test_disk_space_aware_caching() {
        // Test that caching is adjusted based on available disk space
        let capabilities = SystemCapabilities::detect().expect("System detection should succeed");
        let config = CliComponentConfig::with_system_capabilities(&capabilities);

        let should_disable_cache = capabilities.available_disk_gb() < 1.0;

        if should_disable_cache {
            assert_eq!(config.custom.get("cache_disabled"), Some(&serde_json::Value::Bool(true)),
                      "Cache should be disabled when disk space is very low");
        }
    }

    #[test]
    fn test_cli_config_serialization() {
        // Test that CLI configuration can be serialized/deserialized
        let config = CliComponentConfig::default();

        // Test JSON serialization
        let json_str = serde_json::to_string_pretty(&config)
            .expect("CLI config should be serializable to JSON");

        let deserialized: CliComponentConfig = serde_json::from_str(&json_str)
            .expect("CLI config should be deserializable from JSON");

        assert_eq!(config.enabled, deserialized.enabled);
        assert_eq!(config.interface.verbose, deserialized.interface.verbose);
        assert_eq!(config.user_interaction.confirm_destructive, deserialized.user_interaction.confirm_destructive);
    }

    #[test]
    fn test_custom_configuration_overrides() {
        // Test that users can override system-aware defaults
        let mut config = CliComponentConfig::default();

        // User should be able to override any default
        config.interface.command_timeout_seconds = 300;
        config.interface.verbose = true;

        assert_eq!(config.interface.command_timeout_seconds, 300);
        assert!(config.interface.verbose);
    }

    #[test]
    fn test_path_resolution() {
        // Test that paths are properly resolved and validated
        let config = CliComponentConfig::default();

        if let Some(kiln_path) = &config.paths.kiln_path {
            assert!(!kiln_path.as_os_str().is_empty(), "Kiln path should not be empty");
        }

        if let Some(cache_path) = &config.paths.cache_path {
            assert!(!cache_path.as_os_str().is_empty(), "Cache path should not be empty");
        }
    }
}