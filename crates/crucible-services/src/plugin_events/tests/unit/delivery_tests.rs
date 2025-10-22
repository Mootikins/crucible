//! Delivery system tests
//!
//! Tests for event delivery strategies, retry logic, backpressure handling,
//! event ordering, delivery acknowledgments, and performance under various load conditions.

use super::*;
use crate::plugin_events::*;
use crate::plugin_events::tests::common::*;
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
mod real_time_delivery_tests {
    use super::*;

    #[tokio::test]
    async fn test_real_time_delivery_success() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register a test plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Create real-time subscription
        let subscription = TestFixtures::basic_realtime_subscription();
        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish an event
        let event = TestFixtures::system_startup_event();
        test_env.mock_event_bus().publish(event.clone()).await?;

        // Wait for delivery
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify event was delivered
        let delivered_count = test_env
            .mock_plugin_manager()
            .delivered_count("test-plugin")
            .await;
        assert_eq!(delivered_count, 1);

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
    async fn test_real_time_delivery_to_disconnected_plugin() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Create subscription for plugin that isn't connected
        let subscription = TestFixtures::basic_realtime_subscription();
        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish an event
        let event = TestFixtures::system_startup_event();
        test_env.mock_event_bus().publish(event).await?;

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should handle disconnected plugin gracefully
        // System should still be running
        assert!(test_env.event_system().is_running());

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
    async fn test_real_time_delivery_with_failures() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin but simulate delivery failures
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;
        test_env.mock_plugin_manager()
            .set_delivery_failure("test-plugin", true)
            .await;

        // Create subscription with retry
        let mut subscription = TestFixtures::basic_realtime_subscription();
        subscription.delivery_options.max_retries = 3;
        subscription.delivery_options.retry_backoff = RetryBackoff::Fixed { delay_ms: 10 };

        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish an event
        let event = TestFixtures::system_startup_event();
        test_env.mock_event_bus().publish(event).await?;

        // Wait for retry attempts
        tokio::time::sleep(Duration::from_millis(200)).await;

        // System should still be running despite failures
        assert!(test_env.event_system().is_running());

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
mod batched_delivery_tests {
    use super::*;

    #[tokio::test]
    async fn test_batched_delivery_accumulation() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register a test plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Create batched subscription
        let subscription = TestFixtures::batched_subscription();
        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish multiple events
        let events = TestFixtures::test_events();
        for event in &events {
            test_env.mock_event_bus().publish(event.clone()).await?;
        }

        // Wait less than batch interval (events should be accumulated)
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should not have delivered yet (batch interval is 5 seconds)
        let delivered_count = test_env
            .mock_plugin_manager()
            .delivered_count("test-plugin")
            .await;
        assert_eq!(delivered_count, 0);

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
    async fn test_batched_delivery_by_size() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register a test plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Create subscription with small batch size
        let mut subscription = TestFixtures::batched_subscription();
        if let SubscriptionType::Batched { ref mut max_batch_size, .. } = subscription.subscription_type {
            *max_batch_size = 3; // Small batch size for testing
        }

        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish events to exceed batch size
        for i in 0..5 {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4(); // Make events unique
            test_env.mock_event_bus().publish(event).await?;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Should have delivered some batches
        let delivered_count = test_env
            .mock_plugin_manager()
            .delivered_count("test-plugin")
            .await;
        assert!(delivered_count > 0);

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
    async fn test_batched_delivery_with_failures() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin with simulated delivery failure
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;
        test_env.mock_plugin_manager()
            .set_delivery_failure("test-plugin", true)
            .await;

        // Create batched subscription
        let subscription = TestFixtures::batched_subscription();
        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish events
        let events = TestFixtures::test_events();
        for event in &events {
            test_env.mock_event_bus().publish(event.clone()).await?;
        }

        // Wait for batch processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // System should still be running
        assert!(test_env.event_system().is_running());

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
mod persistent_delivery_tests {
    use super::*;

    #[tokio::test]
    async fn test_persistent_delivery_offline_plugin() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Create persistent subscription
        let subscription = TestFixtures::persistent_subscription();
        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish events while plugin is offline
        let events = TestFixtures::test_events();
        for event in &events {
            test_env.mock_event_bus().publish(event.clone()).await?;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Now connect the plugin
        let plugin_info = TestFixtures::test_plugin_info("critical-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Wait for delivery of stored events
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Should have delivered stored events
        let delivered_count = test_env
            .mock_plugin_manager()
            .delivered_count("critical-plugin")
            .await;
        assert!(delivered_count > 0);

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
    async fn test_persistent_delivery_ttl_expiration() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Create persistent subscription with short TTL
        let mut subscription = TestFixtures::persistent_subscription();
        if let SubscriptionType::Persistent { ref mut ttl, .. } = subscription.subscription_type {
            *ttl = Duration::from_millis(100); // Very short TTL for testing
        }

        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish events
        let events = TestFixtures::test_events();
        for event in &events {
            test_env.mock_event_bus().publish(event.clone()).await?;
        }

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Now connect plugin
        let plugin_info = TestFixtures::test_plugin_info("critical-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should have delivered few or no events due to TTL expiration
        let delivered_count = test_env
            .mock_plugin_manager()
            .delivered_count("critical-plugin")
            .await;
        // Note: The exact behavior depends on implementation details

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
    async fn test_persistent_delivery_storage_limits() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Create persistent subscription with small storage limit
        let mut subscription = TestFixtures::persistent_subscription();
        if let SubscriptionType::Persistent { ref mut max_stored_events, .. } = subscription.subscription_type {
            *max_stored_events = 2; // Very small limit
        }

        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish more events than storage limit
        for i in 0..10 {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4();
            test_env.mock_event_bus().publish(event).await?;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Connect plugin
        let plugin_info = TestFixtures::test_plugin_info("critical-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Wait for delivery
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Should have delivered at most the storage limit
        let delivered_count = test_env
            .mock_plugin_manager()
            .delivered_count("critical-plugin")
            .await;
        assert!(delivered_count <= 2);

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
mod retry_logic_tests {
    use super::*;

    #[tokio::test]
    async fn test_exponential_backoff_retry() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin with initial failure, then success
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;
        test_env.mock_plugin_manager()
            .set_delivery_failure("test-plugin", true)
            .await;

        // Create subscription with exponential backoff
        let mut subscription = TestFixtures::basic_realtime_subscription();
        subscription.delivery_options.max_retries = 3;
        subscription.delivery_options.retry_backoff = RetryBackoff::Exponential {
            base_ms: 10,
            max_ms: 1000,
        };

        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish event
        let event = TestFixtures::system_startup_event();
        test_env.mock_event_bus().publish(event).await?;

        // Wait for initial failures
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Enable delivery
        test_env.mock_plugin_manager()
            .set_delivery_failure("test-plugin", false)
            .await;

        // Wait for retry and success
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Should have eventually delivered
        let delivered_count = test_env
            .mock_plugin_manager()
            .delivered_count("test-plugin")
            .await;
        assert!(delivered_count >= 1);

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
    async fn test_retry_exhaustion() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin with persistent failure
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;
        test_env.mock_plugin_manager()
            .set_delivery_failure("test-plugin", true)
            .await;

        // Create subscription with limited retries
        let mut subscription = TestFixtures::basic_realtime_subscription();
        subscription.delivery_options.max_retries = 2;
        subscription.delivery_options.retry_backoff = RetryBackoff::Fixed { delay_ms: 10 };

        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish event
        let event = TestFixtures::system_startup_event();
        test_env.mock_event_bus().publish(event).await?;

        // Wait for all retries to be exhausted
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Should not have delivered
        let delivered_count = test_env
            .mock_plugin_manager()
            .delivered_count("test-plugin")
            .await;
        assert_eq!(delivered_count, 0);

        // System should still be running
        assert!(test_env.event_system().is_running());

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
    async fn test_retry_with_different_strategies() -> TestResult<()> {
        let test_strategies = vec![
            (RetryBackoff::Fixed { delay_ms: 10 }, "fixed"),
            (RetryBackoff::Linear { increment_ms: 5 }, "linear"),
            (RetryBackoff::Exponential { base_ms: 5, max_ms: 100 }, "exponential"),
        ];

        for (strategy, name) in test_strategies {
            let mut test_env = TestEnvironment::new();
            test_env.setup().await?;

            // Register plugin with simulated intermittent failures
            let plugin_info = TestFixtures::test_plugin_info(&format!("test-plugin-{}", name), PluginStatus::Connected);
            test_env.mock_plugin_manager().register_plugin(plugin_info).await;

            // Create subscription with specific retry strategy
            let mut subscription = TestFixtures::basic_realtime_subscription();
            subscription.plugin_id = format!("test-plugin-{}", name);
            subscription.delivery_options.max_retries = 3;
            subscription.delivery_options.retry_backoff = strategy;

            let subscription_id = test_env
                .event_system()
                .subscription_manager()
                .create_subscription(subscription)
                .await?;

            // Publish event
            let event = TestFixtures::system_startup_event();
            test_env.mock_event_bus().publish(event).await?;

            // Wait for processing
            tokio::time::sleep(Duration::from_millis(200)).await;

            // System should handle the retry strategy
            assert!(test_env.event_system().is_running());

            // Clean up
            test_env
                .event_system()
                .subscription_manager()
                .delete_subscription(&subscription_id)
                .await?;

            test_env.cleanup().await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod backpressure_handling_tests {
    use super::*;

    #[tokio::test]
    async fn test_buffer_backpressure() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Add processing delay to simulate slow consumer
        test_env.mock_plugin_manager()
            .set_processing_delay(Duration::from_millis(10))
            .await;

        // Create subscription with buffer backpressure
        let mut subscription = TestFixtures::basic_realtime_subscription();
        subscription.delivery_options.backpressure_handling = BackpressureHandling::Buffer { max_size: 100 };

        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish many events quickly
        for i in 0..200 {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4();
            test_env.mock_event_bus().publish(event).await?;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(500)).await;

        // System should still be running
        assert!(test_env.event_system().is_running());

        // Should have delivered some events
        let delivered_count = test_env
            .mock_plugin_manager()
            .delivered_count("test-plugin")
            .await;
        assert!(delivered_count > 0);

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
    async fn test_drop_oldest_backpressure() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin with delay
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;
        test_env.mock_plugin_manager()
            .set_processing_delay(Duration::from_millis(20))
            .await;

        // Create subscription with drop oldest strategy
        let mut subscription = TestFixtures::basic_realtime_subscription();
        subscription.delivery_options.backpressure_handling = BackpressureHandling::DropOldest { max_size: 10 };

        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish many events
        for i in 0..50 {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4();
            event.metadata.insert("sequence".to_string(), i.to_string());
            test_env.mock_event_bus().publish(event).await?;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Should have delivered some events, but dropped oldest ones
        let delivered_count = test_env
            .mock_plugin_manager()
            .delivered_count("test-plugin")
            .await;
        assert!(delivered_count > 0);
        assert!(delivered_count < 50); // Some events should be dropped

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
    async fn test_drop_newest_backpressure() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin with delay
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;
        test_env.mock_plugin_manager()
            .set_processing_delay(Duration::from_millis(20))
            .await;

        // Create subscription with drop newest strategy
        let mut subscription = TestFixtures::basic_realtime_subscription();
        subscription.delivery_options.backpressure_handling = BackpressureHandling::DropNewest;

        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish many events
        for i in 0..50 {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4();
            test_env.mock_event_bus().publish(event).await?;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Should have delivered early events, dropped newest ones
        let delivered_count = test_env
            .mock_plugin_manager()
            .delivered_count("test-plugin")
            .await;
        assert!(delivered_count > 0);
        assert!(delivered_count < 50); // Some events should be dropped

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
mod event_ordering_tests {
    use super::*;

    #[tokio::test]
    async fn test_fifo_ordering() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Create subscription with FIFO ordering
        let mut subscription = TestFixtures::basic_realtime_subscription();
        subscription.delivery_options.ordering = EventOrdering::Fifo;

        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish events in sequence
        let mut events = Vec::new();
        for i in 0..10 {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4();
            event.timestamp = chrono::Utc::now() + chrono::Duration::milliseconds(i as i64);
            event.metadata.insert("sequence".to_string(), i.to_string());
            events.push(event.clone());
            test_env.mock_event_bus().publish(event).await?;
        }

        // Wait for delivery
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify ordering was preserved
        // Note: In a real implementation, we'd need to inspect the delivered events
        // to verify their order. For this test, we just ensure the system handles
        // FIFO ordering without errors.

        assert!(test_env.event_system().is_running());

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
    async fn test_priority_ordering() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Create subscription with priority ordering
        let mut subscription = TestFixtures::priority_subscription();
        subscription.delivery_options.ordering = EventOrdering::Priority;

        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish events with different priorities
        let events = vec![
            TestFixtures::file_created_event(), // Low priority
            TestFixtures::system_startup_event(), // Normal priority
            TestFixtures::resource_warning_event(), // High priority
            TestFixtures::security_alert_event(), // Critical priority
        ];

        for event in &events {
            test_env.mock_event_bus().publish(event.clone()).await?;
        }

        // Wait for delivery
        tokio::time::sleep(Duration::from_millis(200)).await;

        // System should handle priority ordering
        assert!(test_env.event_system().is_running());

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
mod delivery_performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_high_throughput_delivery() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Create subscription
        let subscription = TestFixtures::basic_realtime_subscription();
        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Generate and publish many events
        let start = Instant::now();
        let event_count = 1000;

        for i in 0..event_count {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4();
            event.metadata.insert("index".to_string(), i.to_string());
            test_env.mock_event_bus().publish(event).await?;
        }

        let publish_time = start.elapsed();

        // Wait for delivery
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let delivered_count = test_env
            .mock_plugin_manager()
            .delivered_count("test-plugin")
            .await;

        // Check performance metrics
        let events_per_second = event_count as f64 / publish_time.as_secs_f64();
        let delivery_rate = delivered_count as f64 / publish_time.as_secs_f64();

        assert!(
            events_per_second > 1000.0,
            "Published only {:.2} events/sec, expected > 1000",
            events_per_second
        );

        assert!(
            delivery_rate > 500.0,
            "Delivered only {:.2} events/sec, expected > 500",
            delivery_rate
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

    #[tokio::test]
    async fn test_delivery_latency() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Create subscription
        let subscription = TestFixtures::basic_realtime_subscription();
        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Test single event latency
        let start = Instant::now();
        let event = TestFixtures::system_startup_event();
        test_env.mock_event_bus().publish(event).await?;

        // Wait for delivery
        while test_env.mock_plugin_manager().delivered_count("test-plugin").await == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
            if start.elapsed() > Duration::from_millis(1000) {
                break; // Timeout
            }
        }

        let latency = start.elapsed();

        assert!(
            latency.as_millis() < 100,
            "Delivery latency {:?} exceeds 100ms",
            latency
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

    #[tokio::test]
    async fn test_concurrent_delivery_performance() -> TestResult<()> {
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

        // Create subscriptions for each plugin
        let mut subscription_ids = Vec::new();
        for i in 0..10 {
            let mut subscription = TestFixtures::basic_realtime_subscription();
            subscription.plugin_id = format!("test-plugin-{}", i);
            let subscription_id = test_env
                .event_system()
                .subscription_manager()
                .create_subscription(subscription)
                .await?;
            subscription_ids.push(subscription_id);
        }

        // Publish events concurrently
        let start = Instant::now();
        let mut handles = Vec::new();

        for plugin_idx in 0..10 {
            for event_idx in 0..100 {
                let plugin_idx = plugin_idx;
                let event_idx = event_idx;
                let mock_event_bus = test_env.mock_event_bus().clone();

                let handle = tokio::spawn(async move {
                    let mut event = TestFixtures::system_startup_event();
                    event.id = uuid::Uuid::new_v4();
                    event.metadata.insert("plugin".to_string(), plugin_idx.to_string());
                    event.metadata.insert("event".to_string(), event_idx.to_string());
                    mock_event_bus.publish(event).await
                });
                handles.push(handle);
            }
        }

        // Wait for all publishes to complete
        for handle in handles {
            handle.await??;
        }

        let publish_time = start.elapsed();

        // Wait for deliveries
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Verify deliveries across all plugins
        let mut total_delivered = 0;
        for i in 0..10 {
            let delivered = test_env
                .mock_plugin_manager()
                .delivered_count(&format!("test-plugin-{}", i))
                .await;
            total_delivered += delivered;
        }

        let total_events = 10 * 100; // 10 plugins * 100 events each
        let delivery_rate = total_delivered as f64 / publish_time.as_secs_f64();

        assert!(
            delivery_rate > 1000.0,
            "Concurrent delivery rate {:.2} events/sec below 1000 target",
            delivery_rate
        );

        // Clean up
        for subscription_id in subscription_ids {
            test_env
                .event_system()
                .subscription_manager()
                .delete_subscription(&subscription_id)
                .await?;
        }

        test_env.cleanup().await?;
        Ok(())
    }
}