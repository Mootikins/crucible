//! Integration tests
//!
//! End-to-end tests for the plugin event subscription system
//! including multi-plugin scenarios, complex workflows, and real-world usage patterns.

use super::*;
use crate::plugin_events::*;
use crate::plugin_events::tests::common::*;
use std::time::Duration;

#[cfg(test)]
mod end_to_end_workflow_tests {
    use super::*;

    #[tokio::test]
    async fn test_complete_subscription_lifecycle() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // 1. Create subscription
        let subscription = TestFixtures::basic_realtime_subscription();
        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // 2. Publish events
        let events = TestFixtures::test_events();
        for event in &events {
            test_env.mock_event_bus().publish(event.clone()).await?;
        }

        // 3. Wait for delivery
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 4. Verify system health
        let health_result = test_env.event_system().health_check().await;
        assert!(!matches!(health_result.overall_status, HealthStatus::Unhealthy));

        // 5. Update subscription
        let mut updated_subscription = test_env
            .event_system()
            .subscription_manager()
            .get_subscription(&subscription_id)
            .await
            .unwrap();
        updated_subscription.name = "Updated Subscription".to_string();
        let update_result = test_env
            .event_system()
            .subscription_manager()
            .update_subscription(updated_subscription)
            .await?;
        assert!(update_result);

        // 6. Publish more events
        for event in &events {
            test_env.mock_event_bus().publish(event.clone()).await?;
        }

        // 7. Delete subscription
        let delete_result = test_env
            .event_system()
            .subscription_manager()
            .delete_subscription(&subscription_id)
            .await?;
        assert!(delete_result.is_ok());

        // 8. Verify cleanup
        let retrieved = test_env
            .event_system()
            .subscription_manager()
            .get_subscription(&subscription_id)
            .await;
        assert!(retrieved.is_none());

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_multi_plugin_event_distribution() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register multiple plugins
        let plugins = TestFixtures::test_plugins();
        for plugin in &plugins {
            if matches!(plugin.status, PluginStatus::Connected) {
                test_env.mock_plugin_manager().register_plugin(plugin.clone()).await;
            }
        }

        // Create subscriptions for different plugins
        let mut subscription_ids = Vec::new();
        for plugin in &plugins {
            if matches!(plugin.status, PluginStatus::Connected) {
                let mut subscription = TestFixtures::basic_realtime_subscription();
                subscription.plugin_id = plugin.plugin_id.clone();
                let id = test_env
                    .event_system()
                    .subscription_manager()
                    .create_subscription(subscription)
                    .await?;
                subscription_ids.push(id);
            }
        }

        // Publish various events
        let events = TestFixtures::test_events();
        for event in &events {
            test_env.mock_event_bus().publish(event.clone()).await?;
        }

        // Wait for distribution
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify distribution across plugins
        let mut total_delivered = 0;
        for plugin in &plugins {
            if matches!(plugin.status, PluginStatus::Connected) {
                let delivered = test_env
                    .mock_plugin_manager()
                    .delivered_count(&plugin.plugin_id)
                    .await;
                total_delivered += delivered;
                assert!(delivered > 0, "Plugin {} should have received events", plugin.plugin_id);
            }
        }

        assert!(total_delivered > 0, "Events should have been delivered to plugins");

        // Clean up
        for subscription_id in subscription_ids {
            let _ = test_env
                .event_system()
                .subscription_manager()
                .delete_subscription(&subscription_id)
                .await;
        }

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_complex_filtering_workflow() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Create subscription with complex filters
        let mut subscription = TestFixtures::conditional_subscription();
        subscription.filters = vec![
            crate::events::EventFilter::Pattern("event.priority >= 'High'".to_string()),
            crate::events::EventFilter::Pattern("event.source.id matches r'^(daemon|service)-.*'".to_string()),
        ];

        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish mixed events
        let events = TestFixtures::test_events();
        let mut matching_events = 0;
        for event in &events {
            test_env.mock_event_bus().publish(event.clone()).await?;
            if event.priority >= crate::events::EventPriority::High {
                matching_events += 1;
            }
        }

        // Wait for filtering and delivery
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Verify filtering worked (should receive fewer events than published)
        let delivered_count = test_env
            .mock_plugin_manager()
            .delivered_count("test-plugin")
            .await;

        assert!(
            delivered_count <= matching_events,
            "Delivered events ({}) should not exceed matching events ({})",
            delivered_count,
            matching_events
        );

        // Clean up
        test_env
            .event_system()
            .subscription_manager()
            .delete_subscription(&subscription_id)
            .await?;

        test_env.cleanup().await?;
        Ok(())
    }
}

#[cfg(test)]
mod reliability_tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_connection_recovery() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Create persistent subscription
        let subscription = TestFixtures::persistent_subscription();
        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish events while plugin is connected
        for i in 0..10 {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4();
            event.metadata.insert("phase".to_string(), "connected".to_string());
            event.metadata.insert("index".to_string(), i.to_string());
            test_env.mock_event_bus().publish(event).await?;
        }

        // Wait for delivery
        tokio::time::sleep(Duration::from_millis(100)).await;
        let delivered_while_connected = test_env
            .mock_plugin_manager()
            .delivered_count("test-plugin")
            .await;

        // Disconnect plugin
        test_env.mock_plugin_manager().unregister_plugin("test-plugin").await;

        // Publish events while plugin is disconnected
        for i in 0..10 {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4();
            event.metadata.insert("phase".to_string(), "disconnected".to_string());
            event.metadata.insert("index".to_string(), i.to_string());
            test_env.mock_event_bus().publish(event).await?;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Reconnect plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Wait for recovery and delivery of stored events
        tokio::time::sleep(Duration::from_millis(300)).await;

        let final_delivered = test_env
            .mock_plugin_manager()
            .delivered_count("test-plugin")
            .await;

        // Should have delivered events from both phases
        assert!(
            final_delivered > delivered_while_connected,
            "Should have delivered events after reconnection"
        );

        // System should still be healthy
        let health_result = test_env.event_system().health_check().await;
        assert!(!matches!(health_result.overall_status, HealthStatus::Unhealthy));

        // Clean up
        test_env
            .event_system()
            .subscription_manager()
            .delete_subscription(&subscription_id)
            .await?;

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_system_resilience_under_load() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register multiple plugins
        for i in 0..10 {
            let plugin_info = TestFixtures::test_plugin_info(
                &format!("test-plugin-{}", i),
                PluginStatus::Connected,
            );
            test_env.mock_plugin_manager().register_plugin(plugin_info).await;
        }

        // Create subscriptions
        let mut subscription_ids = Vec::new();
        for i in 0..10 {
            let mut subscription = TestFixtures::basic_realtime_subscription();
            subscription.plugin_id = format!("test-plugin-{}", i);
            let id = test_env
                .event_system()
                .subscription_manager()
                .create_subscription(subscription)
                .await?;
            subscription_ids.push(id);
        }

        // Apply sustained load
        let start = Instant::now();
        let load_duration = Duration::from_secs(5);

        while start.elapsed() < load_duration {
            // Create and delete subscriptions
            for i in 0..5 {
                let subscription = TestFixtures::basic_realtime_subscription();
                let subscription_id = test_env
                    .event_system()
                    .subscription_manager()
                    .create_subscription(subscription)
                    .await?;
                // Immediately delete
                let _ = test_env
                    .event_system()
                    .subscription_manager()
                    .delete_subscription(&subscription_id)
                    .await;
            }

            // Publish events
            for _ in 0..50 {
                let event = TestFixtures::system_startup_event();
                test_env.mock_event_bus().publish(event).await?;
            }

            // Simulate some plugin failures
            if rand::random::<f32>() < 0.1 {
                let plugin_id = format!("test-plugin-{}", rand::random::<usize>() % 10);
                test_env.mock_plugin_manager()
                    .set_delivery_failure(&plugin_id, true)
                    .await;

                // Restore after a short time
                tokio::time::sleep(Duration::from_millis(10)).await;
                test_env.mock_plugin_manager()
                    .set_delivery_failure(&plugin_id, false)
                    .await;
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // System should still be healthy after load
        let health_result = test_env.event_system().health_check().await;
        assert!(!matches!(health_result.overall_status, HealthStatus::Unhealthy));

        // Verify some events were delivered
        let mut total_delivered = 0;
        for i in 0..10 {
            total_delivered += test_env
                .mock_plugin_manager()
                .delivered_count(&format!("test-plugin-{}", i))
                .await;
        }
        assert!(total_delivered > 0, "Some events should have been delivered");

        // Clean up
        for subscription_id in subscription_ids {
            let _ = test_env
                .event_system()
                .subscription_manager()
                .delete_subscription(&subscription_id)
                .await;
        }

        test_env.cleanup().await?;
        Ok(())
    }
}

#[cfg(test)]
mod real_world_scenario_tests {
    use super::*;

    #[tokio::test]
    async fn test_monitoring_system_scenario() -> TestResult<()> {
        // Simulate a monitoring system with multiple plugins
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register monitoring plugins
        let monitoring_plugins = vec![
            ("alert-manager", PluginStatus::Connected),
            ("metrics-collector", PluginStatus::Connected),
            ("log-aggregator", PluginStatus::Connected),
            ("dashboard-updater", PluginStatus::Connected),
        ];

        for (plugin_id, status) in monitoring_plugins {
            let plugin_info = TestFixtures::test_plugin_info(plugin_id, status);
            test_env.mock_plugin_manager().register_plugin(plugin_info).await;
        }

        // Create specialized subscriptions for each plugin
        let alert_subscription = generators::test_subscription(
            "alert-manager",
            "Critical Alerts",
            SubscriptionType::Realtime,
        );

        let metrics_subscription = generators::test_subscription(
            "metrics-collector",
            "System Metrics",
            SubscriptionType::Batched {
                interval_seconds: 10,
                max_batch_size: 100,
            },
        );

        let log_subscription = generators::test_subscription(
            "log-aggregator",
            "All Events",
            SubscriptionType::Persistent {
                max_stored_events: 50000,
                ttl: Duration::from_secs(3600),
            },
        );

        let dashboard_subscription = generators::test_subscription(
            "dashboard-updater",
            "High Priority Events",
            SubscriptionType::Priority {
                min_priority: crate::events::EventPriority::High,
                delivery_method: Box::new(SubscriptionType::Realtime),
            },
        );

        let subscriptions = vec![
            alert_subscription,
            metrics_subscription,
            log_subscription,
            dashboard_subscription,
        ];

        let mut subscription_ids = Vec::new();
        for subscription in subscriptions {
            let id = test_env
                .event_system()
                .subscription_manager()
                .create_subscription(subscription)
                .await?;
            subscription_ids.push(id);
        }

        // Simulate system events over time
        let event_types = vec![
            TestFixtures::system_startup_event(),
            TestFixtures::service_started_event(),
            TestFixtures::database_query_event(),
            TestFixtures::security_alert_event(),
            TestFixtures::resource_warning_event(),
            TestFixtures::network_error_event(),
        ];

        // Simulate event stream
        for cycle in 0..10 {
            for (i, base_event) in event_types.iter().enumerate() {
                let mut event = base_event.clone();
                event.id = uuid::Uuid::new_v4();
                event.timestamp = chrono::Utc::now() + chrono::Duration::seconds(cycle as i64);
                event.metadata.insert("cycle".to_string(), cycle.to_string());
                event.metadata.insert("type_index".to_string(), i.to_string());

                test_env.mock_event_bus().publish(event).await?;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Verify monitoring system is working
        let health_result = test_env.event_system().health_check().await;
        assert!(!matches!(health_result.overall_status, HealthStatus::Unhealthy));

        // Verify different plugins received appropriate events
        let alert_delivered = test_env
            .mock_plugin_manager()
            .delivered_count("alert-manager")
            .await;
        let metrics_delivered = test_env
            .mock_plugin_manager()
            .delivered_count("metrics-collector")
            .await;
        let log_delivered = test_env
            .mock_plugin_manager()
            .delivered_count("log-aggregator")
            .await;
        let dashboard_delivered = test_env
            .mock_plugin_manager()
            .delivered_count("dashboard-updater")
            .await;

        assert!(alert_delivered > 0, "Alert manager should have received critical events");
        assert!(metrics_delivered > 0, "Metrics collector should have received events");
        assert!(log_delivered > 0, "Log aggregator should have received events");
        assert!(dashboard_delivered > 0, "Dashboard should have received high priority events");

        // Clean up
        for subscription_id in subscription_ids {
            let _ = test_env
                .event_system()
                .subscription_manager()
                .delete_subscription(&subscription_id)
                .await;
        }

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_plugin_lifecycle_management() -> TestResult<()> {
        // Test dynamic plugin registration/deregistration
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let mut plugin_ids = Vec::new();
        let mut subscription_ids = Vec::new();

        // Phase 1: Register initial plugins
        for i in 0..5 {
            let plugin_id = format!("dynamic-plugin-{}", i);
            plugin_ids.push(plugin_id.clone());

            let plugin_info = TestFixtures::test_plugin_info(&plugin_id, PluginStatus::Connected);
            test_env.mock_plugin_manager().register_plugin(plugin_info).await;

            // Create subscription for plugin
            let subscription = generators::test_subscription(
                &plugin_id,
                &format!("Subscription for {}", plugin_id),
                SubscriptionType::Realtime,
            );
            let subscription_id = test_env
                .event_system()
                .subscription_manager()
                .create_subscription(subscription)
                .await?;
            subscription_ids.push(subscription_id);
        }

        // Publish events
        for _ in 0..20 {
            let event = TestFixtures::system_startup_event();
            test_env.mock_event_bus().publish(event).await?;
        }

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Phase 2: Unregister some plugins
        for i in 0..2 {
            test_env.mock_plugin_manager().unregister_plugin(&plugin_ids[i]).await;
        }

        // Publish more events
        for _ in 0..20 {
            let event = TestFixtures::service_started_event();
            test_env.mock_event_bus().publish(event).await?;
        }

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Phase 3: Register new plugins
        for i in 5..7 {
            let plugin_id = format!("dynamic-plugin-{}", i);
            plugin_ids.push(plugin_id.clone());

            let plugin_info = TestFixtures::test_plugin_info(&plugin_id, PluginStatus::Connected);
            test_env.mock_plugin_manager().register_plugin(plugin_info).await;

            let subscription = generators::test_subscription(
                &plugin_id,
                &format!("Subscription for {}", plugin_id),
                SubscriptionType::Realtime,
            );
            let subscription_id = test_env
                .event_system()
                .subscription_manager()
                .create_subscription(subscription)
                .await?;
            subscription_ids.push(subscription_id);
        }

        // Publish final events
        for _ in 0..20 {
            let event = TestFixtures::file_created_event();
            test_env.mock_event_bus().publish(event).await?;
        }

        tokio::time::sleep(Duration::from_millis(200)).await;

        // System should remain healthy throughout
        let health_result = test_env.event_system().health_check().await;
        assert!(!matches!(health_result.overall_status, HealthStatus::Unhealthy));

        // Verify remaining plugins received events
        let mut total_delivered = 0;
        for i in 2..7 {
            if i < plugin_ids.len() {
                let plugin_id = &plugin_ids[i];
                let delivered = test_env
                    .mock_plugin_manager()
                    .delivered_count(plugin_id)
                    .await;
                total_delivered += delivered;
            }
        }
        assert!(total_delivered > 0, "Remaining plugins should have received events");

        // Clean up
        for subscription_id in subscription_ids {
            let _ = test_env
                .event_system()
                .subscription_manager()
                .delete_subscription(&subscription_id)
                .await;
        }

        test_env.cleanup().await?;
        Ok(())
    }
}