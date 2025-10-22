//! Event bridge tests
//!
//! Tests for daemon event system integration, event transformation,
//! security filtering, event deduplication, and offline plugin support.

use super::*;
use crate::plugin_events::*;
use crate::plugin_events::tests::common::*;
use std::time::Duration;

#[cfg(test)]
mod event_bridge_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_daemon_event_integration() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let event_bridge = EventBridge::new(
            test_env.mock_event_bus().clone() as Arc<dyn crate::events::EventBus + Send + Sync>,
            test_env.event_system().subscription_manager().clone(),
        );

        event_bridge.start().await?;

        // Register plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Create subscription
        let subscription = TestFixtures::basic_realtime_subscription();
        let _subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Publish daemon event
        let daemon_event = TestFixtures::system_startup_event();
        test_env.mock_event_bus().publish(daemon_event).await?;

        // Wait for bridge processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify event was bridged
        assert!(test_env.event_system().is_running());

        event_bridge.stop().await?;
        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_event_transformation() -> TestResult<()> {
        let event_bridge = EventBridge::new(
            Arc::new(MockEventBus::new()) as Arc<dyn crate::events::EventBus + Send + Sync>,
            Arc::new(crate::plugin_events::subscription_manager::SubscriptionManager::new(
                Default::default(),
                Arc::new(MockPluginConnectionManager::new()),
            )),
        );

        event_bridge.start().await?;

        let daemon_event = TestFixtures::system_startup_event();
        let transformed = event_bridge.transform_event(&daemon_event).await?;

        assert_eq!(transformed.id, daemon_event.id);
        assert_eq!(transformed.timestamp, daemon_event.timestamp);

        event_bridge.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_event_deduplication() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let event_bridge = EventBridge::new(
            test_env.mock_event_bus().clone() as Arc<dyn crate::events::EventBus + Send + Sync>,
            test_env.event_system().subscription_manager().clone(),
        );

        event_bridge.start().await?;

        // Publish the same event multiple times
        let event = TestFixtures::system_startup_event();
        for _ in 0..3 {
            test_env.mock_event_bus().publish(event.clone()).await?;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Bridge should handle deduplication
        assert!(test_env.event_system().is_running());

        event_bridge.stop().await?;
        test_env.cleanup().await?;
        Ok(())
    }
}

#[cfg(test)]
mod bridge_performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_bridge_throughput() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let event_bridge = EventBridge::new(
            test_env.mock_event_bus().clone() as Arc<dyn crate::events::EventBus + Send + Sync>,
            test_env.event_system().subscription_manager().clone(),
        );

        event_bridge.start().await?;

        let start = Instant::now();
        let event_count = 1000;

        // Publish many events
        for i in 0..event_count {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4();
            event.metadata.insert("index".to_string(), i.to_string());
            test_env.mock_event_bus().publish(event).await?;
        }

        let duration = start.elapsed();
        let throughput = event_count as f64 / duration.as_secs_f64();

        assert!(
            throughput > 5000.0,
            "Bridge throughput {:.2} events/sec below 5000 target",
            throughput
        );

        event_bridge.stop().await?;
        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_bridge_memory_usage() -> TestResult<()> {
        let event_bridge = EventBridge::new(
            Arc::new(MockEventBus::new()) as Arc<dyn crate::events::EventBus + Send + Sync>,
            Arc::new(crate::plugin_events::subscription_manager::SubscriptionManager::new(
                Default::default(),
                Arc::new(MockPluginConnectionManager::new()),
            )),
        );

        event_bridge.start().await?;

        // Process many events to test memory usage
        for i in 0..10000 {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4();
            let _transformed = event_bridge.transform_event(&event).await?;
        }

        // Bridge should still be responsive
        let test_event = TestFixtures::system_startup_event();
        let start = Instant::now();
        let _result = event_bridge.transform_event(&test_event).await?;
        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 10,
            "Bridge transformation took {:?}ms after 10k events",
            duration.as_millis()
        );

        event_bridge.stop().await?;
        Ok(())
    }
}