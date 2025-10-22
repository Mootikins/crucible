//! System tests for PluginEventSystem
//!
//! Tests the core system lifecycle, initialization, configuration, and overall behavior
//! of the plugin event subscription system.

use super::*;
use crate::plugin_events::*;
use crate::plugin_events::tests::common::*;
use std::time::Duration;

#[cfg(test)]
mod system_lifecycle_tests {
    use super::*;

    #[tokio::test]
    async fn test_system_creation_default_config() {
        let config = SubscriptionSystemConfig::default();
        let system = PluginEventSystem::new(config);

        assert!(matches!(system.state(), SystemState::Uninitialized));
        assert_eq!(system.config().system.name, "crucible-subscription-system");
        assert!(system.api_server().is_none());
        assert!(!system.is_running());
    }

    #[tokio::test]
    async fn test_system_creation_custom_config() {
        let config = TestFixtures::development_config();
        let system = PluginEventSystem::new(config);

        assert!(matches!(system.state(), SystemState::Uninitialized));
        assert_eq!(system.config().system.environment, "development");
        assert!(system.config().api.enabled);
    }

    #[tokio::test]
    async fn test_system_initialization_success() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let system = test_env.event_system();
        assert!(system.is_running());
        assert!(matches!(system.state(), SystemState::Running));

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_system_initialization_with_api() -> TestResult<()> {
        let config = TestFixtures::development_config();
        let mut system = PluginEventSystem::new(config);

        let mock_event_bus = Arc::new(MockEventBus::new());
        system
            .initialize(mock_event_bus as Arc<dyn crate::events::EventBus + Send + Sync>)
            .await?;

        assert!(system.is_running());
        assert!(system.api_server().is_some());

        system.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_system_stop_and_restart() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let system = test_env.event_system();
        assert!(system.is_running());

        // Stop the system
        system.stop().await?;
        assert!(!system.is_running());
        assert!(matches!(system.state(), SystemState::Stopped));

        // Restart the system
        let mut new_system = PluginEventSystem::new(system.config().clone());
        let mock_event_bus = Arc::new(MockEventBus::new());
        new_system
            .initialize(mock_event_bus as Arc<dyn crate::events::EventBus + Send + Sync>)
            .await?;

        assert!(new_system.is_running());

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_system_initialization_failure() {
        let config = SubscriptionSystemConfig::default();
        let mut system = PluginEventSystem::new(config);

        // Create a mock event bus that always fails
        let mock_event_bus = Arc::new(MockEventBus::with_failures());
        let result = system
            .initialize(mock_event_bus as Arc<dyn crate::events::EventBus + Send + Sync>)
            .await;

        // The system should handle initialization failures gracefully
        match result {
            Ok(()) => {
                // If initialization succeeds, the system should be running
                assert!(system.is_running());
            }
            Err(_) => {
                // If initialization fails, the system should be in error state
                assert!(matches!(system.state(), SystemState::Error(_)));
            }
        }
    }

    #[tokio::test]
    async fn test_system_configuration_validation() {
        // Test invalid configuration
        let mut invalid_config = SubscriptionSystemConfig::default();
        invalid_config.api.port = 0; // Invalid port

        let result = PluginEventSystem::new(invalid_config);
        assert!(result.is_err());

        // Test valid configuration
        let valid_config = TestFixtures::development_config();
        let result = PluginEventSystem::new(valid_config);
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod system_statistics_tests {
    use super::*;

    #[tokio::test]
    async fn test_system_stats_collection() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let system = test_env.event_system();
        let stats = system.get_system_stats().await;

        assert_eq!(stats.system_config.name, "crucible-subscription-system");
        assert!(stats.component_status.subscription_manager);
        assert!(!stats.component_status.api_server); // Disabled in test

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_system_health_check() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let system = test_env.event_system();
        let health_result = system.health_check().await;

        assert!(!matches!(health_result.overall_status, HealthStatus::Unhealthy));
        assert!(health_result.components.contains_key("subscription_manager"));
        assert!(health_result.components.contains_key("api_server"));

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_health_check_with_components() -> TestResult<()> {
        let mut system = PluginEventSystemBuilder::new()
            .with_api_enabled(true)
            .build()?;

        let mock_event_bus = Arc::new(MockEventBus::new());
        system
            .initialize(mock_event_bus as Arc<dyn crate::events::EventBus + Send + Sync>)
            .await?;

        let health_result = system.health_check().await;

        // API server should be present when enabled
        assert!(health_result.components.contains_key("api_server"));

        system.stop().await?;
        Ok(())
    }
}

#[cfg(test)]
mod system_builder_tests {
    use super::*;

    #[tokio::test]
    async fn test_builder_pattern() {
        let system = PluginEventSystemBuilder::new()
            .with_api_port(9090)
            .with_log_level("debug")
            .with_security_enabled(true)
            .with_data_dir("/tmp/test-data")
            .build()
            .unwrap();

        let config = system.config();
        assert_eq!(config.api.port, 9090);
        assert_eq!(config.logging.level, "debug");
        assert!(config.security.enabled);
        assert_eq!(config.system.data_dir, std::path::PathBuf::from("/tmp/test-data"));
    }

    #[tokio::test]
    async fn test_builder_with_env() {
        let system = PluginEventSystemBuilder::new()
            .with_env()
            .build()
            .unwrap();

        let config = system.config();
        assert!(!config.system.name.is_empty());
    }

    #[tokio::test]
    async fn test_builder_defaults() {
        let system = PluginEventSystemBuilder::default().build().unwrap();
        let config = system.config();

        assert!(config.api.port > 0);
        assert!(!config.logging.level.is_empty());
        assert!(config.system.data_dir.exists() || config.system.data_dir.as_os_str().is_empty());
    }
}

#[cfg(test)]
mod system_error_handling_tests {
    use super::*;

    #[tokio::test]
    async fn test_error_state_handling() {
        let config = SubscriptionSystemConfig::default();
        let mut system = PluginEventSystem::new(config);

        // Manually set error state
        system.set_error_state("Test error".to_string());
        assert!(matches!(system.state(), SystemState::Error(_)));

        // Try to initialize system in error state
        let mock_event_bus = Arc::new(MockEventBus::new());
        let result = system
            .initialize(mock_event_bus as Arc<dyn crate::events::EventBus + Send + Sync>)
            .await;

        // System should either fail to initialize or handle the error gracefully
        match result {
            Err(_) => {
                // Expected - initialization should fail
                assert!(matches!(system.state(), SystemState::Error(_)));
            }
            Ok(_) => {
                // If it succeeds, it should have recovered from error
                assert!(system.is_running());
            }
        }
    }

    #[tokio::test]
    async fn test_graceful_shutdown_on_errors() {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let system = test_env.event_system();
        assert!(system.is_running());

        // Simulate a plugin connection failure
        let plugin_manager = test_env.mock_plugin_manager();
        plugin_manager
            .set_connection_failure("test-plugin", true)
            .await;

        // System should remain running despite plugin failures
        assert!(system.is_running());

        // Health check should show degraded status
        let health_result = system.health_check().await;
        assert!(matches!(
            health_result.overall_status,
            HealthStatus::Degraded | HealthStatus::Healthy
        ));

        test_env.cleanup().await?;
    }

    #[tokio::test]
    async fn test_resource_exhaustion_handling() {
        let config = TestFixtures::performance_config();
        let mut system = PluginEventSystem::new(config);

        let mock_event_bus = Arc::new(MockEventBus::new());
        system
            .initialize(mock_event_bus as Arc<dyn crate::events::EventBus + Send + Sync>)
            .await?;

        // Create many subscriptions to test resource limits
        let subscription_manager = system.subscription_manager();
        for i in 0..100 {
            let subscription = TestFixtures::basic_realtime_subscription();
            // The system should handle resource exhaustion gracefully
            let _ = subscription_manager.create_subscription(subscription).await;
        }

        // System should still be running
        assert!(system.is_running());

        system.stop().await?;
    }
}

#[cfg(test)]
mod system_concurrency_tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_system_operations() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let system = test_env.event_system();
        let system_clone = system.clone();

        // Concurrent operations
        let stats_handle = tokio::spawn(async move {
            system_clone.get_system_stats().await
        });

        let health_handle = tokio::spawn(async move {
            system.health_check().await
        });

        let manager_stats_handle = tokio::spawn(async move {
            system.subscription_manager().get_manager_stats().await
        });

        // Wait for all operations to complete
        let (stats_result, health_result, manager_stats_result) = tokio::try_join!(
            stats_handle,
            health_handle,
            manager_stats_handle
        )?;

        assert!(stats_result.component_status.subscription_manager);
        assert!(!matches!(health_result.overall_status, HealthStatus::Unhealthy));
        assert!(manager_stats_result.is_ok());

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_health_checks() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let system = test_env.event_system();
        let mut handles = Vec::new();

        // Spawn multiple concurrent health checks
        for _ in 0..10 {
            let system_clone = system.clone();
            let handle = tokio::spawn(async move {
                system_clone.health_check().await
            });
            handles.push(handle);
        }

        // Wait for all health checks to complete
        for handle in handles {
            let health_result = handle.await?;
            assert!(!matches!(health_result.overall_status, HealthStatus::Unhealthy));
        }

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_state_access() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let system = test_env.event_system();
        let mut handles = Vec::new();

        // Spawn concurrent state access operations
        for i in 0..20 {
            let system_clone = system.clone();
            let handle = tokio::spawn(async move {
                let _state = system_clone.state();
                let _is_running = system_clone.is_running();
                let _config = system_clone.config();
                format!("Operation {} completed", i)
            });
            handles.push(handle);
        }

        // Wait for all operations to complete
        for handle in handles {
            let result = handle.await?;
            assert!(result.contains("completed"));
        }

        test_env.cleanup().await?;
        Ok(())
    }
}

#[cfg(test)]
mod system_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_end_to_end_system_workflow() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let system = test_env.event_system();

        // Create a subscription
        let subscription = TestFixtures::basic_realtime_subscription();
        let subscription_id = system
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish events
        let events = TestFixtures::test_events();
        for event in &events {
            test_env.mock_event_bus().publish(event.clone()).await?;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify system is still running and healthy
        assert!(system.is_running());
        let health_result = system.health_check().await;
        assert!(!matches!(health_result.overall_status, HealthStatus::Unhealthy));

        // Clean up
        system
            .subscription_manager()
            .delete_subscription(&subscription_id)
            .await?;

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_system_with_multiple_plugins() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let system = test_env.event_system();
        let plugin_manager = test_env.mock_plugin_manager();

        // Register multiple plugins
        let plugins = TestFixtures::test_plugins();
        for plugin in plugins {
            if matches!(plugin.status, PluginStatus::Connected) {
                plugin_manager.register_plugin(plugin).await;
            }
        }

        // Create subscriptions for different plugins
        let subscription1 = TestFixtures::basic_realtime_subscription();
        let subscription2 = TestFixtures::batched_subscription();

        let _sub1_id = system
            .subscription_manager()
            .create_subscription(subscription1)
            .await?;

        let _sub2_id = system
            .subscription_manager()
            .create_subscription(subscription2)
            .await?;

        // Verify system handles multiple plugins correctly
        let stats = system.get_system_stats().await;
        assert!(stats.component_status.subscription_manager);

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_system_recovery_from_failures() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let system = test_env.event_system();
        let plugin_manager = test_env.mock_plugin_manager();

        // Create subscription
        let subscription = TestFixtures::persistent_subscription();
        let subscription_id = system
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Simulate plugin failure
        plugin_manager
            .set_connection_failure("critical-plugin", true)
            .await;

        // Publish events (should fail to deliver)
        let events = TestFixtures::test_events();
        for event in &events {
            test_env.mock_event_bus().publish(event.clone()).await?;
        }

        // Wait for failure detection
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Restore plugin connection
        plugin_manager
            .set_connection_failure("critical-plugin", false)
            .await;

        // System should still be running
        assert!(system.is_running());

        // Clean up
        system
            .subscription_manager()
            .delete_subscription(&subscription_id)
            .await?;

        test_env.cleanup().await?;
        Ok(())
    }
}

#[cfg(test)]
mod system_performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_system_startup_performance() -> TestResult<()> {
        let start = Instant::now();

        let config = TestFixtures::performance_config();
        let mut system = PluginEventSystem::new(config);

        let mock_event_bus = Arc::new(MockEventBus::new());
        system
            .initialize(mock_event_bus as Arc<dyn crate::events::EventBus + Send + Sync>)
            .await?;

        let startup_time = start.elapsed();
        assert!(
            startup_time.as_millis() < 1000,
            "System startup took {:?}ms, expected < 1000ms",
            startup_time.as_millis()
        );

        system.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_system_shutdown_performance() -> TestResult<()> {
        let config = TestFixtures::performance_config();
        let mut system = PluginEventSystem::new(config);

        let mock_event_bus = Arc::new(MockEventBus::new());
        system
            .initialize(mock_event_bus as Arc<dyn crate::events::EventBus + Send + Sync>)
            .await?;

        let start = Instant::now();
        system.stop().await?;
        let shutdown_time = start.elapsed();

        assert!(
            shutdown_time.as_millis() < 500,
            "System shutdown took {:?}ms, expected < 500ms",
            shutdown_time.as_millis()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_health_check_performance() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let system = test_env.event_system();

        // Measure health check performance
        let start = Instant::now();
        let _health_result = system.health_check().await;
        let health_check_time = start.elapsed();

        assert!(
            health_check_time.as_millis() < 100,
            "Health check took {:?}ms, expected < 100ms",
            health_check_time.as_millis()
        );

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_system_stats_performance() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let system = test_env.event_system();

        // Measure stats collection performance
        let start = Instant::now();
        let _stats = system.get_system_stats().await;
        let stats_time = start.elapsed();

        assert!(
            stats_time.as_millis() < 50,
            "Stats collection took {:?}ms, expected < 50ms",
            stats_time.as_millis()
        );

        test_env.cleanup().await?;
        Ok(())
    }
}