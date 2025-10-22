//! Subscription registry tests
//!
//! Tests for the subscription registry including subscription management,
 indexing, lookup, status tracking, and concurrent operations.

use super::*;
use crate::plugin_events::*;
use crate::plugin_events::tests::common::*;
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
mod registry_lifecycle_tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_creation() {
        let registry = SubscriptionRegistry::new();

        // Registry should start empty
        let stats = registry.get_registry_stats().await;
        assert_eq!(stats.total_subscriptions, 0);
        assert_eq!(stats.active_subscriptions, 0);
        assert_eq!(stats.paused_subscriptions, 0);
    }

    #[tokio::test]
    async fn test_registry_startup_shutdown() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();

        // Start registry
        registry.start().await?;

        // Verify it's running
        let stats = registry.get_registry_stats().await;
        assert!(stats.is_running);

        // Stop registry
        registry.stop().await?;

        // Verify it's stopped
        let stats = registry.get_registry_stats().await;
        assert!(!stats.is_running);

        Ok(())
    }

    #[tokio::test]
    async fn test_registry_restart() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();

        // Start, create subscription, stop, restart
        registry.start().await?;

        let subscription = TestFixtures::basic_realtime_subscription();
        let subscription_id = registry.create_subscription(subscription).await?;

        assert!(registry.get_subscription(&subscription_id).await.is_some());

        registry.stop().await?;
        registry.start().await?;

        // Subscription should still exist after restart
        assert!(registry.get_subscription(&subscription_id).await.is_some());

        Ok(())
    }
}

#[cfg(test)]
mod subscription_management_tests {
    use super::*;

    #[tokio::test]
    async fn test_create_subscription() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        let subscription = TestFixtures::basic_realtime_subscription();
        let subscription_id = registry.create_subscription(subscription).await?;

        // Verify subscription was created
        let retrieved = registry.get_subscription(&subscription_id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, subscription_id);

        // Verify stats
        let stats = registry.get_registry_stats().await;
        assert_eq!(stats.total_subscriptions, 1);
        assert_eq!(stats.active_subscriptions, 1);

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_create_subscription_with_validation() {
        let registry = SubscriptionRegistry::new();

        // Test invalid subscription (empty plugin ID)
        let mut invalid_subscription = TestFixtures::basic_realtime_subscription();
        invalid_subscription.plugin_id = "".to_string();

        let result = registry.create_subscription(invalid_subscription).await;
        assert!(result.is_err());

        // Test valid subscription
        let valid_subscription = TestFixtures::basic_realtime_subscription();
        let result = registry.create_subscription(valid_subscription).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_subscription() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        let subscription = TestFixtures::basic_realtime_subscription();
        let subscription_id = registry.create_subscription(subscription).await?;

        // Verify subscription exists
        assert!(registry.get_subscription(&subscription_id).await.is_some());

        // Delete subscription
        let result = registry.delete_subscription(&subscription_id).await;
        assert!(result.is_ok());

        // Verify subscription is gone
        assert!(registry.get_subscription(&subscription_id).await.is_none());

        // Verify stats
        let stats = registry.get_registry_stats().await;
        assert_eq!(stats.total_subscriptions, 0);

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_delete_nonexistent_subscription() {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        let fake_id = SubscriptionId::new();
        let result = registry.delete_subscription(&fake_id).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SubscriptionError::SubscriptionNotFound(_)));

        registry.stop().await;
    }

    #[tokio::test]
    async fn test_update_subscription() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        let subscription = TestFixtures::basic_realtime_subscription();
        let subscription_id = registry.create_subscription(subscription).await?;

        // Update subscription
        let mut updated_subscription = registry.get_subscription(&subscription_id).await.unwrap();
        updated_subscription.name = "Updated Subscription".to_string();
        updated_subscription.update_status(SubscriptionStatus::Paused);

        let result = registry.update_subscription(updated_subscription.clone()).await?;
        assert!(result);

        // Verify update
        let retrieved = registry.get_subscription(&subscription_id).await.unwrap();
        assert_eq!(retrieved.name, "Updated Subscription");
        assert_eq!(retrieved.status, SubscriptionStatus::Paused);

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_update_nonexistent_subscription() {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        let fake_subscription = TestFixtures::basic_realtime_subscription();
        let result = registry.update_subscription(fake_subscription).await;

        assert!(!result);

        registry.stop().await;
    }
}

#[cfg(test)]
mod subscription_lookup_tests {
    use super::*;

    #[tokio::test]
    async fn test_get_subscription_by_id() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        let subscription = TestFixtures::basic_realtime_subscription();
        let subscription_id = registry.create_subscription(subscription).await?;

        // Get by ID
        let retrieved = registry.get_subscription(&subscription_id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, subscription_id);

        // Get non-existent subscription
        let fake_id = SubscriptionId::new();
        let retrieved = registry.get_subscription(&fake_id).await;
        assert!(retrieved.is_none());

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_get_subscriptions_by_plugin() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        // Create subscriptions for multiple plugins
        let sub1 = TestFixtures::basic_realtime_subscription();
        let sub2 = TestFixtures::batched_subscription();
        let sub3 = TestFixtures::basic_realtime_subscription();

        let _id1 = registry.create_subscription(sub1).await?;
        let _id2 = registry.create_subscription(sub2).await?;
        let _id3 = registry.create_subscription(sub3).await?;

        // Get subscriptions for plugin-1
        let plugin1_subs = registry.get_subscriptions_by_plugin("test-plugin-1").await;
        assert_eq!(plugin1_subs.len(), 2);

        // Get subscriptions for plugin-2
        let plugin2_subs = registry.get_subscriptions_by_plugin("test-plugin-2").await;
        assert_eq!(plugin2_subs.len(), 1);

        // Get subscriptions for non-existent plugin
        let no_subs = registry.get_subscriptions_by_plugin("non-existent").await;
        assert_eq!(no_subs.len(), 0);

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_get_subscriptions_by_status() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        // Create subscriptions with different statuses
        let mut sub1 = TestFixtures::basic_realtime_subscription();
        sub1.update_status(SubscriptionStatus::Paused);

        let mut sub2 = TestFixtures::batched_subscription();
        sub2.update_status(SubscriptionStatus::Suspended {
            reason: "Test suspension".to_string(),
            suspended_at: chrono::Utc::now(),
            retry_after: None,
        });

        let sub3 = TestFixtures::persistent_subscription();

        let _id1 = registry.create_subscription(sub1).await?;
        let _id2 = registry.create_subscription(sub2).await?;
        let _id3 = registry.create_subscription(sub3).await?;

        // Get active subscriptions
        let active_subs = registry.get_subscriptions_by_status(SubscriptionStatus::Active).await;
        assert_eq!(active_subs.len(), 1);

        // Get paused subscriptions
        let paused_subs = registry.get_subscriptions_by_status(SubscriptionStatus::Paused).await;
        assert_eq!(paused_subs.len(), 1);

        // Get suspended subscriptions
        let suspended_subs = registry.get_subscriptions_by_status(
            SubscriptionStatus::Suspended {
                reason: "".to_string(),
                suspended_at: chrono::Utc::now(),
                retry_after: None,
            }
        ).await;
        assert_eq!(suspended_subs.len(), 1);

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_get_subscriptions_by_type() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        // Create subscriptions with different types
        let sub1 = TestFixtures::basic_realtime_subscription();
        let sub2 = TestFixtures::batched_subscription();
        let sub3 = TestFixtures::persistent_subscription();

        let _id1 = registry.create_subscription(sub1).await?;
        let _id2 = registry.create_subscription(sub2).await?;
        let _id3 = registry.create_subscription(sub3).await?;

        // Get real-time subscriptions
        let realtime_subs = registry.get_subscriptions_by_type(&SubscriptionType::Realtime).await;
        assert_eq!(realtime_subs.len(), 1);

        // Get batched subscriptions
        let batched_subs = registry.get_subscriptions_by_type(&SubscriptionType::Batched {
            interval_seconds: 5,
            max_batch_size: 100,
        }).await;
        assert_eq!(batched_subs.len(), 1);

        // Get persistent subscriptions
        let persistent_subs = registry.get_subscriptions_by_type(&SubscriptionType::Persistent {
            max_stored_events: 10000,
            ttl: Duration::from_secs(86400),
        }).await;
        assert_eq!(persistent_subs.len(), 1);

        registry.stop().await?;
        Ok(())
    }
}

#[cfg(test)]
mod subscription_filtering_tests {
    use super::*;

    #[tokio::test]
    async fn test_find_matching_subscriptions() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        // Create subscriptions with different filters
        let sub1 = TestFixtures::basic_realtime_subscription();
        let sub2 = TestFixtures::batched_subscription();
        let sub3 = TestFixtures::conditional_subscription();

        let _id1 = registry.create_subscription(sub1).await?;
        let _id2 = registry.create_subscription(sub2).await?;
        let _id3 = registry.create_subscription(sub3).await?;

        // Test with system startup event
        let event = TestFixtures::system_startup_event();
        let matching_subs = registry.find_matching_subscriptions(&event).await;

        // Should match subscriptions that allow system events
        assert!(matching_subs.len() >= 1);

        // Test with file created event
        let event = TestFixtures::file_created_event();
        let matching_subs = registry.find_matching_subscriptions(&event).await;

        // Should match subscriptions that allow filesystem events
        assert!(matching_subs.len() >= 1);

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_subscription_event_matching() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        // Create subscription with specific permissions
        let subscription = generators::test_subscription(
            "test-plugin",
            "Test Subscription",
            SubscriptionType::Realtime,
        );

        let subscription_id = registry.create_subscription(subscription).await?;

        // Test with different events
        let system_event = TestFixtures::system_startup_event();
        let service_event = TestFixtures::service_started_event();
        let security_event = TestFixtures::security_alert_event();

        // Check if subscription matches events
        let subscription = registry.get_subscription(&subscription_id).await.unwrap();

        let matches_system = subscription.matches_event(&system_event);
        let matches_service = subscription.matches_event(&service_event);
        let matches_security = subscription.matches_event(&security_event);

        // Basic subscription should match all events
        assert!(matches_system);
        assert!(matches_service);
        assert!(matches_security);

        registry.stop().await?;
        Ok(())
    }
}

#[cfg(test)]
mod registry_statistics_tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_stats_initial() {
        let registry = SubscriptionRegistry::new();
        let stats = registry.get_registry_stats().await;

        assert_eq!(stats.total_subscriptions, 0);
        assert_eq!(stats.active_subscriptions, 0);
        assert_eq!(stats.paused_subscriptions, 0);
        assert_eq!(stats.suspended_subscriptions, 0);
        assert_eq!(stats.terminated_subscriptions, 0);
        assert!(!stats.is_running);
    }

    #[tokio::test]
    async fn test_registry_stats_with_subscriptions() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        // Create subscriptions with different statuses
        let mut sub1 = TestFixtures::basic_realtime_subscription();
        sub1.update_status(SubscriptionStatus::Paused);

        let mut sub2 = TestFixtures::batched_subscription();
        sub2.update_status(SubscriptionStatus::Suspended {
            reason: "Test".to_string(),
            suspended_at: chrono::Utc::now(),
            retry_after: None,
        });

        let mut sub3 = TestFixtures::persistent_subscription();
        sub3.update_status(SubscriptionStatus::Terminated {
            reason: "Test".to_string(),
            terminated_at: chrono::Utc::now(),
        });

        let sub4 = TestFixtures::conditional_subscription();

        let _id1 = registry.create_subscription(sub1).await?;
        let _id2 = registry.create_subscription(sub2).await?;
        let _id3 = registry.create_subscription(sub3).await?;
        let _id4 = registry.create_subscription(sub4).await?;

        let stats = registry.get_registry_stats().await;
        assert_eq!(stats.total_subscriptions, 4);
        assert_eq!(stats.active_subscriptions, 1);
        assert_eq!(stats.paused_subscriptions, 1);
        assert_eq!(stats.suspended_subscriptions, 1);
        assert_eq!(stats.terminated_subscriptions, 1);
        assert!(stats.is_running);

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_registry_performance_metrics() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        // Create many subscriptions
        for i in 0..100 {
            let mut subscription = TestFixtures::basic_realtime_subscription();
            subscription.name = format!("Performance Test Subscription {}", i);
            let _id = registry.create_subscription(subscription).await?;
        }

        let stats = registry.get_registry_stats().await;
        assert_eq!(stats.total_subscriptions, 100);
        assert_eq!(stats.active_subscriptions, 100);

        // Test performance metrics are being tracked
        let performance = &stats.performance;
        assert!(performance.subscriptions_created >= 100);
        assert!(performance.subscriptions_deleted >= 0);
        assert!(performance.avg_subscription_creation_time_ms >= 0.0);

        registry.stop().await?;
        Ok(())
    }
}

#[cfg(test)]
mod registry_concurrency_tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_subscription_creation() -> TestResult<()> {
        let registry = Arc::new(SubscriptionRegistry::new());
        registry.start().await?;

        let mut handles = Vec::new();

        // Create subscriptions concurrently
        for i in 0..50 {
            let registry = Arc::clone(&registry);
            let handle = tokio::spawn(async move {
                let mut subscription = TestFixtures::basic_realtime_subscription();
                subscription.name = format!("Concurrent Subscription {}", i);
                subscription.plugin_id = format!("plugin-{}", i % 5);

                registry.create_subscription(subscription).await
            });
            handles.push(handle);
        }

        // Wait for all creations to complete
        let mut results = Vec::new();
        for handle in handles {
            let result = handle.await?;
            results.push(result);
        }

        // Verify all creations succeeded
        assert_eq!(results.len(), 50);
        for result in results {
            assert!(result.is_ok());
        }

        // Verify all subscriptions were created
        let stats = registry.get_registry_stats().await;
        assert_eq!(stats.total_subscriptions, 50);

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_subscription_deletion() -> TestResult<()> {
        let registry = Arc::new(SubscriptionRegistry::new());
        registry.start().await?;

        // Create subscriptions first
        let mut subscription_ids = Vec::new();
        for i in 0..20 {
            let subscription = TestFixtures::basic_realtime_subscription();
            let id = registry.create_subscription(subscription).await?;
            subscription_ids.push(id);
        }

        // Delete subscriptions concurrently
        let mut handles = Vec::new();
        for subscription_id in subscription_ids {
            let registry = Arc::clone(&registry);
            let handle = tokio::spawn(async move {
                registry.delete_subscription(&subscription_id).await
            });
            handles.push(handle);
        }

        // Wait for all deletions to complete
        let mut results = Vec::new();
        for handle in handles {
            let result = handle.await?;
            results.push(result);
        }

        // Verify all deletions succeeded
        assert_eq!(results.len(), 20);
        for result in results {
            assert!(result.is_ok());
        }

        // Verify all subscriptions were deleted
        let stats = registry.get_registry_stats().await;
        assert_eq!(stats.total_subscriptions, 0);

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_read_write_operations() -> TestResult<()> {
        let registry = Arc::new(SubscriptionRegistry::new());
        registry.start().await?;

        // Create initial subscriptions
        let subscription_ids: Vec<_> = (0..10).map(|i| async {
            let subscription = TestFixtures::basic_realtime_subscription();
            registry.create_subscription(subscription).await.unwrap()
        }).collect::<futures::future::JoinAll<_>>().await;

        let mut handles = Vec::new();

        // Spawn concurrent read operations
        for _ in 0..20 {
            let registry = Arc::clone(&registry);
            let handle = tokio::spawn(async move {
                let stats = registry.get_registry_stats().await;
                assert!(stats.total_subscriptions >= 0);
                stats.total_subscriptions
            });
            handles.push(handle);
        }

        // Spawn concurrent write operations
        for i in 0..5 {
            let registry = Arc::clone(&registry);
            let handle = tokio::spawn(async move {
                let subscription = TestFixtures::batched_subscription();
                registry.create_subscription(subscription).await
            });
            handles.push(handle);
        }

        // Wait for all operations to complete
        for handle in handles {
            let _ = handle.await?;
        }

        // Verify consistency
        let final_stats = registry.get_registry_stats().await;
        assert_eq!(final_stats.total_subscriptions, 15); // 10 initial + 5 new

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_subscription_lookup() -> TestResult<()> {
        let registry = Arc::new(SubscriptionRegistry::new());
        registry.start().await?;

        // Create test subscriptions
        let subscription = TestFixtures::basic_realtime_subscription();
        let subscription_id = registry.create_subscription(subscription).await?;

        let mut handles = Vec::new();

        // Spawn concurrent lookup operations
        for _ in 0..100 {
            let registry = Arc::clone(&registry);
            let subscription_id = subscription_id.clone();
            let handle = tokio::spawn(async move {
                registry.get_subscription(&subscription_id).await
            });
            handles.push(handle);
        }

        // Wait for all lookups to complete
        let mut results = Vec::new();
        for handle in handles {
            let result = handle.await?;
            results.push(result);
        }

        // Verify all lookups succeeded
        assert_eq!(results.len(), 100);
        for result in results {
            assert!(result.is_some());
            assert_eq!(result.unwrap().id, subscription_id);
        }

        registry.stop().await?;
        Ok(())
    }
}

#[cfg(test)]
mod registry_error_handling_tests {
    use super::*;

    #[tokio::test]
    async fn test_duplicate_subscription_handling() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        // Create a subscription
        let subscription = TestFixtures::basic_realtime_subscription();
        let subscription_id = registry.create_subscription(subscription.clone()).await?;

        // Try to create the same subscription again (should fail or handle gracefully)
        let result = registry.create_subscription(subscription).await;

        // Registry should either reject duplicate or handle it gracefully
        match result {
            Ok(_) => {
                // If it succeeded, there should be 2 subscriptions
                let stats = registry.get_registry_stats().await;
                assert_eq!(stats.total_subscriptions, 2);
            }
            Err(_) => {
                // If it failed, there should still be 1 subscription
                let stats = registry.get_registry_stats().await;
                assert_eq!(stats.total_subscriptions, 1);
            }
        }

        // Original subscription should still exist
        assert!(registry.get_subscription(&subscription_id).await.is_some());

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_invalid_subscription_id_handling() {
        let registry = SubscriptionRegistry::new();
        registry.start().await;

        // Try operations with invalid ID
        let invalid_id = SubscriptionId::new();

        let get_result = registry.get_subscription(&invalid_id).await;
        assert!(get_result.is_none());

        let delete_result = registry.delete_subscription(&invalid_id).await;
        assert!(delete_result.is_err());

        let mut fake_subscription = TestFixtures::basic_realtime_subscription();
        fake_subscription.id = invalid_id.clone();
        let update_result = registry.update_subscription(fake_subscription).await;
        assert!(!update_result);

        registry.stop().await;
    }

    #[tokio::test]
    async fn test_registry_operations_when_stopped() {
        let registry = SubscriptionRegistry::new();
        // Don't start the registry

        let subscription = TestFixtures::basic_realtime_subscription();

        // Operations should fail when registry is not started
        let create_result = registry.create_subscription(subscription).await;
        assert!(create_result.is_err());

        let fake_id = SubscriptionId::new();
        let get_result = registry.get_subscription(&fake_id).await;
        assert!(get_result.is_none());

        let delete_result = registry.delete_subscription(&fake_id).await;
        assert!(delete_result.is_err());
    }

    #[tokio::test]
    async fn test_subscription_validation_on_create() {
        let registry = SubscriptionRegistry::new();
        registry.start().await;

        // Test creating subscription with invalid data
        let mut invalid_subscription = TestFixtures::basic_realtime_subscription();
        invalid_subscription.plugin_id = "".to_string(); // Empty plugin ID

        let result = registry.create_subscription(invalid_subscription).await;
        assert!(result.is_err());

        // Test creating subscription with invalid auth context
        let mut invalid_subscription = TestFixtures::basic_realtime_subscription();
        invalid_subscription.auth_context.permissions.clear(); // No permissions

        let result = registry.create_subscription(invalid_subscription).await;
        assert!(result.is_err());

        registry.stop().await;
    }
}

#[cfg(test)]
mod registry_performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_subscription_creation_performance() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        let start = Instant::now();

        // Create many subscriptions
        for i in 0..1000 {
            let mut subscription = TestFixtures::basic_realtime_subscription();
            subscription.name = format!("Perf Test {}", i);
            subscription.plugin_id = format!("plugin-{}", i % 100);

            registry.create_subscription(subscription).await?;
        }

        let duration = start.elapsed();
        let avg_time_per_subscription = duration.as_millis() as f64 / 1000.0;

        assert!(
            avg_time_per_subscription < 10.0,
            "Average creation time {:.2}ms exceeds 10ms target",
            avg_time_per_subscription
        );

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_subscription_lookup_performance() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        // Create subscriptions
        let mut subscription_ids = Vec::new();
        for i in 0..1000 {
            let subscription = TestFixtures::basic_realtime_subscription();
            let id = registry.create_subscription(subscription).await?;
            subscription_ids.push(id);
        }

        let start = Instant::now();

        // Perform many lookups
        for subscription_id in &subscription_ids {
            let _result = registry.get_subscription(subscription_id).await;
        }

        let duration = start.elapsed();
        let avg_time_per_lookup = duration.as_micros() as f64 / 1000.0;

        assert!(
            avg_time_per_lookup < 100.0,
            "Average lookup time {:.2}μs exceeds 100μs target",
            avg_time_per_lookup
        );

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_matching_subscription_performance() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        // Create many subscriptions
        for _ in 0..1000 {
            let subscription = TestFixtures::basic_realtime_subscription();
            let _id = registry.create_subscription(subscription).await?;
        }

        let test_events = TestFixtures::test_events();

        let start = Instant::now();

        // Find matching subscriptions for many events
        for _ in 0..1000 {
            let event = &test_events[rand::random::<usize>() % test_events.len()];
            let _matches = registry.find_matching_subscriptions(event).await;
        }

        let duration = start.elapsed();
        let avg_time_per_match = duration.as_micros() as f64 / 1000.0;

        assert!(
            avg_time_per_match < 500.0,
            "Average match time {:.2}μs exceeds 500μs target",
            avg_time_per_match
        );

        registry.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_memory_usage_with_many_subscriptions() -> TestResult<()> {
        let registry = SubscriptionRegistry::new();
        registry.start().await?;

        // Create a large number of subscriptions
        for i in 0..10000 {
            let mut subscription = TestFixtures::basic_realtime_subscription();
            subscription.name = format!("Large Scale Test {}", i);
            subscription.plugin_id = format!("plugin-{}", i % 1000);

            registry.create_subscription(subscription).await?;
        }

        // Check that registry can handle the load
        let stats = registry.get_registry_stats().await;
        assert_eq!(stats.total_subscriptions, 10000);

        // Test lookup performance still acceptable
        let start = Instant::now();
        let plugin_subs = registry.get_subscriptions_by_plugin("plugin-0").await;
        let lookup_time = start.elapsed();

        assert_eq!(plugin_subs.len(), 10); // Should have 10 subscriptions for plugin-0
        assert!(
            lookup_time.as_millis() < 100,
            "Lookup took {:?}ms with 10k subscriptions",
            lookup_time.as_millis()
        );

        registry.stop().await?;
        Ok(())
    }
}