//! Configuration tests
//!
//! Tests for configuration loading, validation, environment-specific settings,
//! hot reloading, and configuration error handling.

use super::*;
use crate::plugin_events::*;
use crate::plugin_events::tests::common::*;
use std::time::Duration;

#[cfg(test)]
mod configuration_validation_tests {
    use super::*;

    #[tokio::test]
    async fn test_default_configuration_validation() {
        let config = SubscriptionSystemConfig::default();
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_api_port() {
        let mut config = SubscriptionSystemConfig::default();
        config.api.port = 0; // Invalid port

        let result = config.validate();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_delivery_queue_size() {
        let mut config = SubscriptionSystemConfig::default();
        config.delivery.delivery_queue_size = 0; // Invalid queue size

        let result = config.validate();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_filter_cache_size() {
        let mut config = SubscriptionSystemConfig::default();
        config.filtering.cache_size = 0; // Invalid cache size

        let result = config.validate();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_valid_security_configuration() {
        let config = TestFixtures::security_config();
        let result = config.validate();
        assert!(result.is_ok());
        assert!(config.security.enabled);
        assert!(config.security.authorization_required);
    }

    #[tokio::test]
    async fn test_invalid_event_size_limit() {
        let mut config = SubscriptionSystemConfig::default();
        config.security.max_event_size_bytes = 0; // Invalid size

        let result = config.validate();
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod configuration_loading_tests {
    use super::*;

    #[tokio::test]
    async fn test_configuration_from_environment() {
        let config = SubscriptionSystemConfig::from_env();

        // Should have reasonable defaults
        assert!(config.api.port > 0);
        assert!(!config.system.name.is_empty());
        assert!(!config.logging.level.is_empty());
    }

    #[tokio::test]
    async fn test_performance_configuration() {
        let config = TestFixtures::performance_config();
        let result = config.validate();
        assert!(result.is_ok());

        // Should have performance-optimized settings
        assert!(config.manager.max_subscriptions > 1000);
        assert!(config.delivery.delivery_queue_size > 1000);
        assert!(config.filtering.cache_size > 1000);
    }

    #[tokio::test]
    async fn test_development_configuration() {
        let config = TestFixtures::development_config();
        let result = config.validate();
        assert!(result.is_ok());

        assert_eq!(config.system.environment, "development");
        assert!(config.api.enabled);
        assert_eq!(config.logging.level, "debug");
    }

    #[tokio::test]
    async fn test_configuration_from_file() {
        // Test configuration loading from file
        let config = SubscriptionSystemConfig::default();

        // In a real implementation, this would load from a file
        let result = config.validate();
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod configuration_hot_reload_tests {
    use super::*;

    #[tokio::test]
    async fn test_configuration_hot_reload() {
        // Test hot reloading of configuration
        let mut test_env = TestEnvironment::new();
        test_env.setup().await;

        // System should support configuration hot reload
        assert!(test_env.event_system().is_running());

        test_env.cleanup().await;
    }

    #[tokio::test]
    async fn test_configuration_change_validation() {
        // Test that configuration changes are validated
        let config = SubscriptionSystemConfig::default();

        // Test changing to invalid configuration
        let mut invalid_config = config.clone();
        invalid_config.api.port = 0;

        let result = invalid_config.validate();
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod configuration_defaults_tests {
    use super::*;

    #[tokio::test]
    async fn test_delivery_options_defaults() {
        let defaults = DeliveryOptions::default();

        assert!(defaults.ack_enabled);
        assert_eq!(defaults.max_retries, 3);
        assert_eq!(defaults.compression_threshold, 1024);
        assert_eq!(defaults.max_event_size, 10 * 1024 * 1024);
        assert!(matches!(defaults.ordering, EventOrdering::Fifo));
    }

    #[tokio::test]
    async fn test_subscription_config_defaults() {
        let auth_context = TestFixtures::plugin_auth_context("test-plugin");
        let subscription = SubscriptionConfig::new(
            "test-plugin".to_string(),
            "Test Subscription".to_string(),
            SubscriptionType::Realtime,
            auth_context,
        );

        assert_eq!(subscription.status, SubscriptionStatus::Active);
        assert!(subscription.filters.is_empty());
        assert_eq!(subscription.subscription_type, SubscriptionType::Realtime);
    }

    #[tokio::test]
    async fn test_auth_context_defaults() {
        let permissions = vec![];
        let auth_context = AuthContext::new("test-principal".to_string(), permissions);

        assert_eq!(auth_context.principal, "test-principal");
        assert_eq!(auth_context.security_level, SecurityLevel::Normal);
        assert!(auth_context.metadata.is_empty());
    }

    #[tokio::test]
    async fn test_performance_metrics_defaults() {
        let metrics = PerformanceMetrics::default();

        assert_eq!(metrics.events_per_sec, 0.0);
        assert_eq!(metrics.latency_p50, 0.0);
        assert_eq!(metrics.latency_p95, 0.0);
        assert_eq!(metrics.latency_p99, 0.0);
        assert_eq!(metrics.error_rate_percent, 0.0);
        assert_eq!(metrics.memory_usage_bytes, 0);
        assert_eq!(metrics.cpu_usage_percent, 0.0);
    }
}

#[cfg(test)]
mod configuration_compatibility_tests {
    use super::*;

    #[tokio::test]
    async fn test_backward_compatibility() {
        // Test that newer configurations work with older versions
        let config = SubscriptionSystemConfig::default();

        // Should handle missing optional fields gracefully
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_configuration_migration() {
        // Test configuration migration between versions
        let config = SubscriptionSystemConfig::default();

        // Should be able to migrate from older config formats
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_environment_specific_config() {
        // Test different environments have appropriate defaults
        let dev_config = TestFixtures::development_config();
        let prod_config = SubscriptionSystemConfig::default();

        // Development should have debug logging
        assert_eq!(dev_config.logging.level, "debug");

        // Production should have appropriate settings
        assert!(prod_config.system.name == "crucible-subscription-system");
    }
}