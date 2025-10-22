//! Performance tests
//!
//! Comprehensive performance testing for the plugin event subscription system
//! including subscription creation, event delivery, filtering, and concurrency performance.

use super::*;
use crate::plugin_events::*;
use crate::plugin_events::tests::common::*;
use std::time::Duration;

#[cfg(test)]
mod subscription_performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_subscription_creation_performance() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let tracker = PerformanceTracker::new();
        let start = tracker.start_operation("subscription_creation".to_string()).await;

        // Create many subscriptions
        for i in 0..1000 {
            let mut subscription = TestFixtures::basic_realtime_subscription();
            subscription.name = format!("Perf Test {}", i);
            subscription.plugin_id = format!("plugin-{}", i % 100);

            let _subscription_id = test_env
                .event_system()
                .subscription_manager()
                .create_subscription(subscription)
                .await?;
        }

        let duration = start.await;
        let avg_time = duration.as_millis() as f64 / 1000.0;

        assert!(
            avg_time < 10.0,
            "Average subscription creation time {:.2}ms exceeds 10ms target",
            avg_time
        );

        let stats = tracker.calculate_stats("subscription_creation").await.unwrap();
        assert!(stats.operations_per_second > 100.0);

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_subscription_deletion_performance() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Create subscriptions first
        let mut subscription_ids = Vec::new();
        for i in 0..1000 {
            let subscription = TestFixtures::basic_realtime_subscription();
            let id = test_env
                .event_system()
                .subscription_manager()
                .create_subscription(subscription)
                .await?;
            subscription_ids.push(id);
        }

        // Measure deletion performance
        let tracker = PerformanceTracker::new();
        let start = tracker.start_operation("subscription_deletion".to_string()).await;

        for subscription_id in subscription_ids {
            test_env
                .event_system()
                .subscription_manager()
                .delete_subscription(&subscription_id)
                .await?;
        }

        let duration = start.await;
        let avg_time = duration.as_millis() as f64 / 1000.0;

        assert!(
            avg_time < 5.0,
            "Average subscription deletion time {:.2}ms exceeds 5ms target",
            avg_time
        );

        test_env.cleanup().await?;
        Ok(())
    }
}

#[cfg(test)]
mod event_delivery_performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_real_time_delivery_performance() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

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

        let tracker = PerformanceTracker::new();
        let start = tracker.start_operation("real_time_delivery".to_string()).await;

        // Publish events
        for i in 0..10000 {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4();
            event.metadata.insert("index".to_string(), i.to_string());
            test_env.mock_event_bus().publish(event).await?;
        }

        let publish_duration = start.await;
        let publish_rate = 10000.0 / publish_duration.as_secs_f64();

        assert!(
            publish_rate > 50000.0,
            "Event publish rate {:.2}/sec below 50,000 target",
            publish_rate
        );

        // Wait for delivery
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let delivered_count = test_env
            .mock_plugin_manager()
            .delivered_count("test-plugin")
            .await;
        let delivery_rate = delivered_count as f64 / publish_duration.as_secs_f64();

        assert!(
            delivery_rate > 20000.0,
            "Event delivery rate {:.2}/sec below 20,000 target",
            delivery_rate
        );

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_batched_delivery_performance() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Create batched subscription with short interval
        let mut subscription = TestFixtures::batched_subscription();
        if let SubscriptionType::Batched { ref mut interval_seconds, .. } = subscription.subscription_type {
            *interval_seconds = 1; // 1 second interval for testing
        }

        let _subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        let tracker = PerformanceTracker::new();
        let start = tracker.start_operation("batched_delivery".to_string()).await;

        // Publish events
        for i in 0..5000 {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4();
            test_env.mock_event_bus().publish(event).await?;
        }

        // Wait for batch processing
        tokio::time::sleep(Duration::from_millis(2000)).await;

        let duration = start.await;
        let throughput = 5000.0 / duration.as_secs_f64();

        assert!(
            throughput > 2000.0,
            "Batched delivery throughput {:.2}/sec below 2,000 target",
            throughput
        );

        test_env.cleanup().await?;
        Ok(())
    }
}

#[cfg(test)]
mod filter_performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_filter_compilation_performance() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        let tracker = PerformanceTracker::new();
        let start = tracker.start_operation("filter_compilation".to_string()).await;

        // Compile many filters
        for i in 0..10000 {
            let filter_expr = format!("event.type == 'test.event.{}' && event.priority >= 'Normal'", i);
            let _filter_id = filter_engine.compile_filter(&filter_expr).await?;
        }

        let duration = start.await;
        let avg_time = duration.as_micros() as f64 / 10000.0;

        assert!(
            avg_time < 100.0,
            "Average filter compilation time {:.2}μs exceeds 100μs target",
            avg_time
        );

        let stats = tracker.calculate_stats("filter_compilation").await.unwrap();
        assert!(stats.operations_per_second > 10000.0);

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_filter_evaluation_performance() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        // Compile test filter
        let filter_expr = "event.priority in ['Normal', 'High', 'Critical'] && event.source.id starts_with 'test-'";
        let filter_id = filter_engine.compile_filter(filter_expr).await?;

        let events = PerformanceFixtures::generate_performance_events(10000);

        let tracker = PerformanceTracker::new();
        let start = tracker.start_operation("filter_evaluation".to_string()).await;

        // Evaluate filter against many events
        for event in &events {
            let _matches = filter_engine.evaluate_filter(&filter_id, event).await?;
        }

        let duration = start.await;
        let avg_time = duration.as_nanos() as f64 / 10000.0;

        assert!(
            avg_time < 10000.0, // 10 microseconds
            "Average filter evaluation time {:.2}ns exceeds 10μs target",
            avg_time
        );

        filter_engine.stop().await?;
        Ok(())
    }
}

#[cfg(test)]
mod concurrency_performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_subscription_operations() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        let tracker = PerformanceTracker::new();
        let start = tracker.start_operation("concurrent_operations".to_string()).await;

        // Perform concurrent subscription operations
        let concurrency_tester = ConcurrencyTester::new(50, 20); // 50 threads, 20 ops each

        let results = concurrency_tester.run_concurrent_operations(|op_id| async {
            if op_id % 2 == 0 {
                // Create subscription
                let mut subscription = TestFixtures::basic_realtime_subscription();
                subscription.name = format!("Concurrent {}", op_id);
                test_env
                    .event_system()
                    .subscription_manager()
                    .create_subscription(subscription)
                    .await
            } else {
                // Perform other operation (e.g., get stats)
                Ok(test_env.event_system().get_system_stats().await)
            }
        }).await;

        let success_count = results.iter().filter(|r| r.is_ok()).count();
        let total_ops = 50 * 20;

        assert!(
            success_count as f64 / total_ops as f64 > 0.95,
            "Success rate {:.2}% below 95% target",
            (success_count as f64 / total_ops as f64) * 100.0
        );

        let duration = start.await;
        let ops_per_second = total_ops as f64 / duration.as_secs_f64();

        assert!(
            ops_per_second > 100.0,
            "Concurrent operations rate {:.2}/sec below 100 target",
            ops_per_second
        );

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_high_concurrency_event_delivery() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register many plugins
        for i in 0..100 {
            let plugin_info = TestFixtures::test_plugin_info(
                &format!("test-plugin-{}", i),
                PluginStatus::Connected,
            );
            test_env.mock_plugin_manager().register_plugin(plugin_info).await;
        }

        // Create subscriptions for all plugins
        let mut subscription_ids = Vec::new();
        for i in 0..100 {
            let mut subscription = TestFixtures::basic_realtime_subscription();
            subscription.plugin_id = format!("test-plugin-{}", i);
            let id = test_env
                .event_system()
                .subscription_manager()
                .create_subscription(subscription)
                .await?;
            subscription_ids.push(id);
        }

        let tracker = PerformanceTracker::new();
        let start = tracker.start_operation("high_concurrency_delivery".to_string()).await;

        // Publish events concurrently
        let concurrency_tester = ConcurrencyTester::new(100, 100); // 100 threads, 100 events each

        let _results = concurrency_tester.run_concurrent_operations(|op_id| async {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4();
            event.metadata.insert("thread".to_string(), (op_id / 100).to_string());
            event.metadata.insert("index".to_string(), (op_id % 100).to_string());
            test_env.mock_event_bus().publish(event).await
        }).await;

        // Wait for delivery
        tokio::time::sleep(Duration::from_millis(2000)).await;

        let duration = start.await;
        let total_events = 100 * 100; // 100 threads * 100 events
        let events_per_second = total_events as f64 / duration.as_secs_f64();

        assert!(
            events_per_second > 10000.0,
            "High concurrency delivery rate {:.2}/sec below 10,000 target",
            events_per_second
        );

        // Clean up subscriptions
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
mod memory_performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_usage_with_many_subscriptions() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Create many subscriptions
        let subscription_count = 10000;
        for i in 0..subscription_count {
            let mut subscription = TestFixtures::basic_realtime_subscription();
            subscription.name = format!("Memory Test {}", i);
            subscription.plugin_id = format!("plugin-{}", i % 1000);
            subscription.metadata.insert("test_data".to_string(), "x".repeat(100)); // Add some data

            let _subscription_id = test_env
                .event_system()
                .subscription_manager()
                .create_subscription(subscription)
                .await?;
        }

        // Test that system remains responsive
        let start = Instant::now();
        let stats = test_env.event_system().get_system_stats().await;
        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 100,
            "System stats took {:?}ms with 10k subscriptions",
            duration.as_millis()
        );

        assert_eq!(stats.manager_stats.active_subscriptions, subscription_count);

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_memory_efficiency_with_events() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Register plugin
        let plugin_info = TestFixtures::test_plugin_info("test-plugin", PluginStatus::Connected);
        test_env.mock_plugin_manager().register_plugin(plugin_info).await;

        // Create subscription
        let subscription = TestFixtures::persistent_subscription();
        let _subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Process many events to test memory usage
        let event_count = 50000;
        for i in 0..event_count {
            let mut event = TestFixtures::system_startup_event();
            event.id = uuid::Uuid::new_v4();
            event.metadata.insert("index".to_string(), i.to_string());
            event.metadata.insert("data".to_string(), "x".repeat(1000)); // Add large data
            test_env.mock_event_bus().publish(event).await?;
        }

        // System should still be responsive
        let start = Instant::now();
        let stats = test_env.event_system().get_system_stats().await;
        let duration = start.elapsed();

        assert!(
            duration.as_millis() < 200,
            "System stats took {:?}ms after processing 50k events",
            duration.as_millis()
        );

        test_env.cleanup().await?;
        Ok(())
    }
}

#[cfg(test)]
mod load_testing {
    use super::*;

    #[tokio::test]
    async fn test_system_load_sustained() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Create sustained load
        let duration = Duration::from_secs(10);
        let start = Instant::now();

        while start.elapsed() < duration {
            // Create subscriptions
            for i in 0..10 {
                let subscription = TestFixtures::basic_realtime_subscription();
                let _subscription_id = test_env
                    .event_system()
                    .subscription_manager()
                    .create_subscription(subscription)
                    .await?;
            }

            // Publish events
            for _ in 0..100 {
                let event = TestFixtures::system_startup_event();
                test_env.mock_event_bus().publish(event).await?;
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // System should still be healthy
        let health_result = test_env.event_system().health_check().await;
        assert!(!matches!(health_result.overall_status, HealthStatus::Unhealthy));

        test_env.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_stress_recovery() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Apply extreme stress
        for i in 0..1000 {
            let subscription = TestFixtures::basic_realtime_subscription();
            let _subscription_id = test_env
                .event_system()
                .subscription_manager()
                .create_subscription(subscription)
                .await?;

            for _ in 0..100 {
                let event = TestFixtures::system_startup_event();
                test_env.mock_event_bus().publish(event).await?;
            }
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(5000)).await;

        // System should recover and be healthy
        let health_result = test_env.event_system().health_check().await;
        assert!(!matches!(health_result.overall_status, HealthStatus::Unhealthy));

        test_env.cleanup().await?;
        Ok(())
    }
}