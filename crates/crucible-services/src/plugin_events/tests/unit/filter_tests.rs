//! Filter engine tests
//!
//! Tests for event filtering and matching logic, filter compilation and optimization,
//! pattern matching, and filter performance under load.

use super::*;
use crate::plugin_events::*;
use crate::plugin_events::tests::common::*;
use crate::events::{EventFilter, EventType};
use std::time::Duration;

#[cfg(test)]
mod filter_compilation_tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_filter_compilation() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        // Test simple pattern filter
        let filter_expression = "event.type == 'system.startup'";
        let filter_id = filter_engine.compile_filter(filter_expression).await?;

        assert!(!filter_id.is_empty());

        let compiled_filter = filter_engine.get_compiled_filter(&filter_id).await;
        assert!(compiled_filter.is_some());

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_complex_filter_compilation() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        let complex_filter = r#"
            (event.type == 'system.startup' || event.type == 'service.started')
            && event.priority in ['High', 'Critical']
            && event.source.id starts_with 'daemon-'
            && event.metadata.component != 'test'
        "#;

        let filter_id = filter_engine.compile_filter(complex_filter).await?;
        assert!(!filter_id.is_empty());

        let compiled_filter = filter_engine.get_compiled_filter(&filter_id).await;
        assert!(compiled_filter.is_some());

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_invalid_filter_compilation() {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await;

        // Test syntactically invalid filter
        let invalid_filter = "event.type == 'system.startup' &&"; // Incomplete expression
        let result = filter_engine.compile_filter(invalid_filter).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SubscriptionError::FilteringError(_)));

        filter_engine.stop().await;
    }

    #[tokio::test]
    async fn test_filter_caching() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        let filter_expression = "event.type == 'system.startup'";

        // Compile filter twice
        let filter_id1 = filter_engine.compile_filter(filter_expression).await?;
        let filter_id2 = filter_engine.compile_filter(filter_expression).await?;

        // Should return the same filter ID (cached)
        assert_eq!(filter_id1, filter_id2);

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_filter_cache_eviction() -> TestResult<()> {
        let mut config = SubscriptionSystemConfig::default();
        config.filtering.cache_size = 2; // Small cache for testing
        config.filtering.compilation_cache_ttl_seconds = 1; // Short TTL

        let filter_engine = FilterEngine::with_config(config.filtering.clone());
        filter_engine.start().await?;

        // Fill cache beyond capacity
        let filter1 = filter_engine.compile_filter("event.type == 'system.startup'").await?;
        let filter2 = filter_engine.compile_filter("event.type == 'service.started'").await?;
        let filter3 = filter_engine.compile_filter("event.type == 'file.created'").await?;

        // All should compile successfully, but some may be evicted from cache
        assert!(!filter1.is_empty());
        assert!(!filter2.is_empty());
        assert!(!filter3.is_empty());

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Compile again - should trigger cache refresh
        let filter1_new = filter_engine.compile_filter("event.type == 'system.startup'").await?;

        filter_engine.stop().await?;
        Ok(())
    }
}

#[cfg(test)]
mod filter_evaluation_tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_event_matching() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        // Compile filter for system startup events
        let filter_id = filter_engine.compile_filter("event.type == 'system.startup'").await?;

        // Test with matching event
        let startup_event = TestFixtures::system_startup_event();
        let matches = filter_engine.evaluate_filter(&filter_id, &startup_event).await?;
        assert!(matches);

        // Test with non-matching event
        let service_event = TestFixtures::service_started_event();
        let matches = filter_engine.evaluate_filter(&filter_id, &service_event).await?;
        assert!(!matches);

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_complex_event_matching() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        let filter_expression = r#"
            event.priority == 'Critical'
            && event.source.id == 'security-monitor'
            && event.metadata.alert == 'true'
        "#;

        let filter_id = filter_engine.compile_filter(filter_expression).await?;

        // Test with matching security alert
        let security_event = TestFixtures::security_alert_event();
        let matches = filter_engine.evaluate_filter(&filter_id, &security_event).await?;
        assert!(matches);

        // Test with non-matching event (different priority)
        let mut service_event = TestFixtures::service_started_event();
        service_event.priority = crate::events::EventPriority::Normal;
        let matches = filter_engine.evaluate_filter(&filter_id, &service_event).await?;
        assert!(!matches);

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_wildcard_matching() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        // Test wildcard for event types
        let filter_id = filter_engine.compile_filter("event.type starts_with 'system.'").await?;

        let system_event = TestFixtures::system_startup_event();
        let matches = filter_engine.evaluate_filter(&filter_id, &system_event).await?;
        assert!(matches);

        let service_event = TestFixtures::service_started_event();
        let matches = filter_engine.evaluate_filter(&filter_id, &service_event).await?;
        assert!(!matches);

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_regex_matching() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        // Test regex pattern for source IDs
        let filter_id = filter_engine.compile_filter("event.source.id matches r'^service-.*'").await?;

        let service_event = TestFixtures::service_started_event();
        let matches = filter_engine.evaluate_filter(&filter_id, &service_event).await?;
        assert!(matches);

        let system_event = TestFixtures::system_startup_event();
        let matches = filter_engine.evaluate_filter(&filter_id, &system_event).await?;
        assert!(!matches);

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_numeric_comparisons() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        // Test numeric comparison on duration
        let filter_id = filter_engine.compile_filter("event.metadata.duration_ms > 100").await?;

        // Create event with duration metadata
        let mut db_event = TestFixtures::database_query_event();
        db_event.metadata.insert("duration_ms".to_string(), "150".to_string());

        let matches = filter_engine.evaluate_filter(&filter_id, &db_event).await?;
        assert!(matches);

        // Test with smaller duration
        db_event.metadata.insert("duration_ms".to_string(), "50".to_string());
        let matches = filter_engine.evaluate_filter(&filter_id, &db_event).await?;
        assert!(!matches);

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_boolean_logic() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        // Test OR logic
        let or_filter = filter_engine.compile_filter(
            "event.type == 'system.startup' || event.type == 'service.started'"
        ).await?;

        let startup_event = TestFixtures::system_startup_event();
        let matches = filter_engine.evaluate_filter(&or_filter, &startup_event).await?;
        assert!(matches);

        let service_event = TestFixtures::service_started_event();
        let matches = filter_engine.evaluate_filter(&or_filter, &service_event).await?;
        assert!(matches);

        let file_event = TestFixtures::file_created_event();
        let matches = filter_engine.evaluate_filter(&or_filter, &file_event).await?;
        assert!(!matches);

        // Test AND logic
        let and_filter = filter_engine.compile_filter(
            "event.type == 'service.started' && event.priority == 'Normal'"
        ).await?;

        let matches = filter_engine.evaluate_filter(&and_filter, &service_event).await?;
        assert!(matches);

        // Test NOT logic
        let not_filter = filter_engine.compile_filter("event.type != 'system.startup'").await?;

        let matches = filter_engine.evaluate_filter(&not_filter, &service_event).await?;
        assert!(matches);

        let matches = filter_engine.evaluate_filter(&not_filter, &startup_event).await?;
        assert!(!matches);

        filter_engine.stop().await?;
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

        let start = Instant::now();

        // Compile many filters
        for i in 0..1000 {
            let filter_expr = format!("event.type == 'test.event.{}'", i);
            let _filter_id = filter_engine.compile_filter(&filter_expr).await?;
        }

        let duration = start.elapsed();
        let avg_time = duration.as_millis() as f64 / 1000.0;

        assert!(
            avg_time < 1.0,
            "Average filter compilation time {:.2}ms exceeds 1ms target",
            avg_time
        );

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_filter_evaluation_performance() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        // Compile a moderately complex filter
        let filter_expr = r#"
            event.priority in ['Normal', 'High', 'Critical']
            && event.source.id starts_with 'test-'
            && event.metadata.category == 'performance'
        "#;

        let filter_id = filter_engine.compile_filter(filter_expr).await?;

        // Create test events
        let events = PerformanceFixtures::generate_performance_events(1000);

        let start = Instant::now();

        // Evaluate filter against many events
        for event in &events {
            let _matches = filter_engine.evaluate_filter(&filter_id, event).await?;
        }

        let duration = start.elapsed();
        let avg_time = duration.as_micros() as f64 / 1000.0;

        assert!(
            avg_time < 100.0,
            "Average filter evaluation time {:.2}μs exceeds 100μs target",
            avg_time
        );

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_filter_operations() -> TestResult<()> {
        let filter_engine = Arc::new(FilterEngine::new());
        filter_engine.start().await?;

        let mut handles = Vec::new();

        // Spawn concurrent filter compilation
        for i in 0..50 {
            let filter_engine = Arc::clone(&filter_engine);
            let handle = tokio::spawn(async move {
                let filter_expr = format!("event.type == 'concurrent.test.{}'", i);
                filter_engine.compile_filter(&filter_expr).await
            });
            handles.push(handle);
        }

        // Spawn concurrent filter evaluation
        for i in 0..50 {
            let filter_engine = Arc::clone(&filter_engine);
            let handle = tokio::spawn(async move {
                // Use a pre-compiled filter for evaluation
                let filter_id = "test-filter".to_string();
                let event = TestFixtures::system_startup_event();

                // This might fail if filter doesn't exist, but we're testing concurrency
                let result = filter_engine.evaluate_filter(&filter_id, &event).await;
                result.is_ok()
            });
            handles.push(handle);
        }

        // Wait for all operations to complete
        for handle in handles {
            let _ = handle.await?;
        }

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_memory_usage_with_many_filters() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        // Compile many unique filters
        let mut filter_ids = Vec::new();
        for i in 0..10000 {
            let filter_expr = format!("event.type == 'memory.test.{}' && event.id == '{}'", i, i);
            let filter_id = filter_engine.compile_filter(&filter_expr).await?;
            filter_ids.push(filter_id);
        }

        // Verify all filters were compiled
        assert_eq!(filter_ids.len(), 10000);

        // Test that we can still evaluate filters efficiently
        let test_event = TestFixtures::system_startup_event();
        let start = Instant::now();

        for filter_id in &filter_ids[0..1000] { // Test first 1000
            let _result = filter_engine.evaluate_filter(filter_id, &test_event).await;
        }

        let duration = start.elapsed();
        assert!(
            duration.as_millis() < 1000,
            "Evaluating 1000 filters took {:?}ms",
            duration.as_millis()
        );

        filter_engine.stop().await?;
        Ok(())
    }
}

#[cfg(test)]
mod filter_optimization_tests {
    use super::*;

    #[tokio::test]
    async fn test_filter_simplification() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        // Test filter that can be simplified
        let complex_filter = "event.type == 'system.startup' && true && event.type != 'other'";
        let filter_id = filter_engine.compile_filter(complex_filter).await?;

        // Should optimize to simple event.type == 'system.startup'
        let test_event = TestFixtures::system_startup_event();
        let matches = filter_engine.evaluate_filter(&filter_id, &test_event).await?;
        assert!(matches);

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_filter_index_optimization() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        // Compile filters that can benefit from indexing
        let filter1 = filter_engine.compile_filter("event.type == 'system.startup'").await?;
        let filter2 = filter_engine.compile_filter("event.type == 'service.started'").await?;
        let filter3 = filter_engine.compile_filter("event.source.id == 'daemon'").await?;

        // Test that filters with same field are optimized together
        let system_event = TestFixtures::system_startup_event();

        let start = Instant::now();
        let _result1 = filter_engine.evaluate_filter(&filter1, &system_event).await?;
        let _result2 = filter_engine.evaluate_filter(&filter2, &system_event).await?;
        let _result3 = filter_engine.evaluate_filter(&filter3, &system_event).await?;
        let duration = start.elapsed();

        // Should be fast due to field indexing
        assert!(
            duration.as_micros() < 1000,
            "Indexed filter evaluation took {:?}μs",
            duration.as_micros()
        );

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_cache_hit_performance() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        let filter_expr = "event.type == 'cache.test'";
        let filter_id = filter_engine.compile_filter(filter_expr).await?;

        let test_event = TestFixtures::custom_test_event();

        // First evaluation (cache miss)
        let start = Instant::now();
        let _result1 = filter_engine.evaluate_filter(&filter_id, &test_event).await?;
        let first_time = start.elapsed();

        // Second evaluation (cache hit)
        let start = Instant::now();
        let _result2 = filter_engine.evaluate_filter(&filter_id, &test_event).await?;
        let second_time = start.elapsed();

        // Cache hit should be faster
        assert!(
            second_time < first_time || second_time.as_micros() < 100,
            "Cache hit ({:?}) should be faster than cache miss ({:?})",
            second_time,
            first_time
        );

        filter_engine.stop().await?;
        Ok(())
    }
}

#[cfg(test)]
mod filter_error_handling_tests {
    use super::*;

    #[tokio::test]
    async fn test_invalid_filter_expressions() {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await;

        let invalid_filters = vec![
            "event.type == 'unclosed_string",
            "event.type == \"mismatched quotes'",
            "event.type in [1, 2, 3", // Incomplete array
            "event.field &&& event.other", // Invalid operator
            "event.type == ()", // Empty parentheses
            "event.type == function()", // Invalid function call
        ];

        for invalid_filter in invalid_filters {
            let result = filter_engine.compile_filter(invalid_filter).await;
            assert!(result.is_err(), "Filter should have failed: {}", invalid_filter);
        }

        filter_engine.stop().await;
    }

    #[tokio::test]
    async fn test_evaluation_with_missing_fields() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        // Filter references a field that might not exist
        let filter_id = filter_engine.compile_filter("event.metadata.nonexistent == 'value'").await?;

        let test_event = TestFixtures::system_startup_event();

        // Should handle missing field gracefully
        let result = filter_engine.evaluate_filter(&filter_id, &test_event).await;
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should return false for missing fields

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_type_conversion_errors() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        // Filter tries to compare string field as number
        let filter_id = filter_engine.compile_filter("event.type > 100").await?;

        let test_event = TestFixtures::system_startup_event();

        // Should handle type conversion gracefully
        let result = filter_engine.evaluate_filter(&filter_id, &test_event).await;
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should return false for type mismatch

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_filter_engine_lifecycle() {
        let filter_engine = FilterEngine::new();

        // Operations should fail when not started
        let result = filter_engine.compile_filter("event.type == 'test'").await;
        assert!(result.is_err());

        // Start the engine
        filter_engine.start().await;

        // Operations should work when started
        let result = filter_engine.compile_filter("event.type == 'test'").await;
        assert!(result.is_ok());

        // Stop the engine
        filter_engine.stop().await;

        // Operations should fail when stopped
        let result = filter_engine.compile_filter("event.type == 'test'").await;
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod filter_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_filter_with_subscription_system() -> TestResult<()> {
        let mut test_env = TestEnvironment::new();
        test_env.setup().await?;

        // Create subscription with complex filter
        let mut subscription = TestFixtures::basic_realtime_subscription();
        subscription.filters = vec![
            EventFilter::Pattern("event.priority == 'Critical'".to_string()),
            EventFilter::Pattern("event.source.id starts_with 'security-'".to_string()),
        ];

        let subscription_id = test_env
            .event_system()
            .subscription_manager()
            .create_subscription(subscription)
            .await?;

        // Test matching event
        let security_event = TestFixtures::security_alert_event();
        test_env.mock_event_bus().publish(security_event.clone()).await?;

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Test non-matching event
        let file_event = TestFixtures::file_created_event();
        test_env.mock_event_bus().publish(file_event.clone()).await?;

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(50)).await;

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
    async fn test_multiple_filters_combination() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        // Create multiple filters
        let filter1 = filter_engine.compile_filter("event.type == 'system.startup'").await?;
        let filter2 = filter_engine.compile_filter("event.priority == 'Critical'").await?;
        let filter3 = filter_engine.compile_filter("event.source.id == 'security-monitor'").await?;

        let security_event = TestFixtures::security_alert_event();

        // Test individual filters
        let result1 = filter_engine.evaluate_filter(&filter1, &security_event).await?;
        let result2 = filter_engine.evaluate_filter(&filter2, &security_event).await?;
        let result3 = filter_engine.evaluate_filter(&filter3, &security_event).await?;

        // Security event should match filters 2 and 3, but not 1
        assert!(!result1);
        assert!(result2);
        assert!(result3);

        filter_engine.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_filter_with_various_event_types() -> TestResult<()> {
        let filter_engine = FilterEngine::new();
        filter_engine.start().await?;

        let events = TestFixtures::test_events();
        let filter_id = filter_engine.compile_filter("event.priority >= 'High'").await?;

        let mut high_priority_count = 0;
        for event in &events {
            let matches = filter_engine.evaluate_filter(&filter_id, event).await?;
            if matches {
                high_priority_count += 1;
            }
        }

        // Should match high and critical priority events
        assert!(high_priority_count > 0);
        assert!(high_priority_count < events.len());

        filter_engine.stop().await?;
        Ok(())
    }
}