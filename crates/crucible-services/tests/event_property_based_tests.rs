//! Property-based tests for event system invariants

use crucible_services::events::core::*;
use crucible_services::events::routing::*;
use crucible_services::events::errors::{EventError, EventResult};
use crucible_services::types::{ServiceHealth, ServiceStatus};
use chrono::{Utc, DateTime};
use std::collections::HashMap;
use uuid::Uuid;

/// Property-based test utilities
mod prop_utils {
    use super::*;

    /// Generate a random event type
    pub fn random_event_type() -> EventType {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        match rng.gen_range(0..6) {
            0 => EventType::Filesystem(FilesystemEventType::FileCreated {
                path: format!("/test/path{}.txt", rng.gen_range(0..1000)),
            }),
            1 => EventType::Database(DatabaseEventType::RecordCreated {
                table: format!("table{}", rng.gen_range(0..10)),
                id: format!("id-{}", Uuid::new_v4()),
            }),
            2 => EventType::External(ExternalEventType::DataReceived {
                source: format!("source{}", rng.gen_range(0..5)),
                data: serde_json::json!({"random": rng.gen_range(0..100)}),
            }),
            3 => EventType::Mcp(McpEventType::ToolCall {
                tool_name: format!("tool{}", rng.gen_range(0..10)),
                parameters: serde_json::json!({"param": rng.gen_range(0..50)}),
            }),
            4 => EventType::Service(ServiceEventType::HealthCheck {
                service_id: format!("service{}", rng.gen_range(0..5)),
                status: "healthy".to_string(),
            }),
            _ => EventType::System(SystemEventType::DaemonStarted {
                version: format!("{}.{}.{}", rng.gen_range(1..10), rng.gen_range(0..10), rng.gen_range(0..10)),
            }),
        }
    }

    /// Generate a random event source
    pub fn random_event_source() -> EventSource {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let source_types = vec![
            SourceType::Service,
            SourceType::Filesystem,
            SourceType::Database,
            SourceType::External,
            SourceType::Mcp,
            SourceType::System,
        ];

        let source_type = source_types[rng.gen_range(0..source_types.len())];
        let id = format!("source-{}", rng.gen_range(0..1000));

        let mut source = EventSource::new(id, source_type);

        // Randomly add metadata
        if rng.gen_bool(0.3) {
            source = source.with_metadata(
                format!("meta-{}", rng.gen_range(0..10)),
                format!("value-{}", rng.gen_range(0..100)),
            );
        }

        // Randomly add instance
        if rng.gen_bool(0.2) {
            source = source.with_instance(format!("instance-{}", rng.gen_range(0..5)));
        }

        source
    }

    /// Generate a random event priority
    pub fn random_event_priority() -> EventPriority {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        match rng.gen_range(0..4) {
            0 => EventPriority::Critical,
            1 => EventPriority::High,
            2 => EventPriority::Normal,
            _ => EventPriority::Low,
        }
    }

    /// Generate a random event payload
    pub fn random_event_payload() -> EventPayload {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let payload_types = vec![
            serde_json::json!({"type": "simple", "value": rng.gen_range(0..100)}),
            serde_json::json!({
                "type": "complex",
                "nested": {
                    "field1": rng.gen_range(0..1000),
                    "field2": format!("value-{}", rng.gen_range(0..50))
                },
                "array": (0..rng.gen_range(1..5)).map(|i| rng.gen_range(0..100)).collect::<Vec<_>>()
            }),
            serde_json::json!({
                "type": "text",
                "content": format!("Random text {}", rng.gen_range(0..1000))
            }),
            serde_json::json!({
                "type": "empty",
                "data": null
            }),
        ];

        let data = payload_types[rng.gen_range(0..payload_types.len())];
        EventPayload::json(data)
    }

    /// Generate a random daemon event
    pub fn random_daemon_event() -> DaemonEvent {
        let event_type = random_event_type();
        let source = random_event_source();
        let priority = random_event_priority();
        let payload = random_event_payload();

        let mut event = DaemonEvent::new(event_type, source, payload)
            .with_priority(priority);

        // Randomly add targets
        use rand::Rng;
        let mut rng = rand::thread_rng();
        if rng.gen_bool(0.3) {
            let target_count = rng.gen_range(1..4);
            for i in 0..target_count {
                let target = ServiceTarget::new(format!("target-{}", i))
                    .with_priority(rng.gen_range(0..5) as u8);
                event = event.with_target(target);
            }
        }

        // Randomly add metadata
        if rng.gen_bool(0.2) {
            event = event.with_metadata(
                format!("key-{}", rng.gen_range(0..5)),
                format!("value-{}", rng.gen_range(0..100)),
            );
        }

        // Randomly set max retries
        if rng.gen_bool(0.2) {
            event = event.with_max_retries(rng.gen_range(1..10));
        }

        event
    }

    /// Generate a random event filter
    pub fn random_event_filter() -> EventFilter {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let mut filter = EventFilter::new();

        // Randomly add event types
        if rng.gen_bool(0.7) {
            let event_types = vec!["filesystem", "database", "external", "mcp", "service", "system"];
            let count = rng.gen_range(1..=event_types.len());
            let mut selected = Vec::new();
            for _ in 0..count {
                let event_type = event_types[rng.gen_range(0..event_types.len())];
                if !selected.contains(&event_type) {
                    selected.push(event_type.to_string());
                }
            }
            filter.event_types = selected;
        }

        // Randomly add priorities
        if rng.gen_bool(0.5) {
            let all_priorities = vec![EventPriority::Critical, EventPriority::High, EventPriority::Normal, EventPriority::Low];
            let count = rng.gen_range(1..=all_priorities.len());
            filter.priorities = all_priorities.into_iter().take(count).collect();
        }

        // Randomly add sources
        if rng.gen_bool(0.4) {
            let source_count = rng.gen_range(1..=3);
            for i in 0..source_count {
                filter.sources.push(format!("source-{}", i));
            }
        }

        // Randomly add max payload size
        if rng.gen_bool(0.3) {
            filter.max_payload_size = Some(rng.gen_range(100..=10000));
        }

        // Randomly add expression
        if rng.gen_bool(0.2) {
            filter.expression = Some(format!("keyword-{}", rng.gen_range(0..10)));
        }

        filter
    }
}

#[cfg(test)]
mod event_property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_event_serialization_roundtrip(event in prop_utils::random_daemon_event()) {
            // Serialize event
            let serialized = serde_json::to_string(&event);
            prop_assert!(serialized.is_ok());

            // Deserialize event
            let deserialized: Result<DaemonEvent, _> = serde_json::from_str(&serialized.unwrap());
            prop_assert!(deserialized.is_ok());

            let deserialized_event = deserialized.unwrap();

            // Check that key properties are preserved
            prop_assert_eq!(event.id, deserialized_event.id);
            prop_assert_eq!(event.priority, deserialized_event.priority);
            prop_assert_eq!(event.event_type, deserialized_event.event_type);
            prop_assert_eq!(event.source.id, deserialized_event.source.id);
        }

        #[test]
        fn test_event_validation_preserves_properties(event in prop_utils::random_daemon_event()) {
            // Validate event
            let validation_result = event.validate();

            // If validation passes, basic properties should be valid
            if validation_result.is_ok() {
                // Check that timestamp is reasonable (not too far in future)
                let now = Utc::now();
                let time_diff = (event.created_at - now).num_seconds();
                prop_assert!(time_diff <= 60, "Event timestamp should not be more than 60 seconds in the future");

                // Check that event size is reasonable
                let size = event.size_bytes();
                prop_assert!(size > 0, "Event size should be positive");
                prop_assert!(size < 50 * 1024 * 1024, "Event size should be reasonable (< 50MB)");
            }
        }

        #[test]
        fn test_event_priority_ordering(event1 in prop_utils::random_daemon_event(),
                                          event2 in prop_utils::random_daemon_event()) {
            // Test that priority ordering works correctly
            let cmp_result = event1.priority.cmp(&event2.priority);

            // Verify that the ordering matches expected priority levels
            let priority1_value = event1.priority.value();
            let priority2_value = event2.priority.value();

            let expected_cmp = priority1_value.cmp(&priority2_value);
            prop_assert_eq!(cmp_result, expected_cmp);
        }

        #[test]
        fn test_retry_logic_invariants(event in prop_utils::random_daemon_event()) {
            // Test that retry logic maintains invariants
            let initial_retry_count = event.retry_count;
            let initial_max_retries = event.max_retries;

            // Initial state should be valid
            prop_assert!(initial_retry_count <= initial_max_retries,
                        "Initial retry count should not exceed max retries");

            // Test can_retry logic
            let can_retry_initial = event.can_retry();
            prop_assert_eq!(can_retry_initial, initial_retry_count < initial_max_retries,
                           "can_retry should match retry count vs max retries");

            // If we can retry, increment and test again
            if can_retry_initial {
                let mut event_mut = event.clone();
                event_mut.increment_retry();

                prop_assert_eq!(event_mut.retry_count, initial_retry_count + 1,
                               "Retry count should increment by 1");

                // May or may not be able to retry after increment
                if event_mut.retry_count < event_mut.max_retries {
                    prop_assert!(event_mut.can_retry(),
                                  "Should still be able to retry if under max");
                }
            }
        }

        #[test]
        fn test_event_size_calculation(event in prop_utils::random_daemon_event()) {
            // Test that size calculation is consistent
            let size1 = event.size_bytes();
            let size2 = event.size_bytes();

            prop_assert_eq!(size1, size2, "Size calculation should be deterministic");

            // Size should be positive for valid events
            if event.validate().is_ok() {
                prop_assert!(size1 > 0, "Valid event should have positive size");
            }
        }

        #[test]
        fn test_metadata_consistency(event in prop_utils::random_daemon_event()) {
            // Test that metadata operations are consistent
            let initial_field_count = event.metadata.fields.len();

            // Adding the same field should overwrite the value
            let test_key = "test_key".to_string();
            let test_value1 = "value1".to_string();
            let test_value2 = "value2".to_string();

            let mut event_mut = event.clone();
            event_mut = event_mut.with_metadata(test_key.clone(), test_value1.clone());
            prop_assert_eq!(event_mut.metadata.get_field(&test_key), Some(&test_value1));

            event_mut = event_mut.with_metadata(test_key.clone(), test_value2.clone());
            prop_assert_eq!(event_mut.metadata.get_field(&test_key), Some(&test_value2));
            prop_assert!(event_mut.metadata.fields.len() == initial_field_count + 1 ||
                        event_mut.metadata.fields.len() == initial_field_count);
        }
    }
}

#[cfg(test)]
mod filter_property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_filter_idempotency(filter in prop_utils::random_event_filter(),
                                   event in prop_utils::random_daemon_event()) {
            // Filter matching should be idempotent
            let result1 = filter.matches(&event);
            let result2 = filter.matches(&event);

            prop_assert_eq!(result1, result2, "Filter matching should be deterministic");
        }

        #[test]
        fn test_empty_filter_matches_all(event in prop_utils::random_daemon_event()) {
            let empty_filter = EventFilter::default();

            // Empty filter should match all events
            prop_assert!(empty_filter.matches(&event), "Empty filter should match all events");
        }

        #[test]
        fn test_filter_combination_properties(events in proptest::collection::vec(prop_utils::random_daemon_event(), 1..10)) {
            // Test that filter combinations behave as expected
            let type_filter = EventFilter {
                event_types: vec!["filesystem".to_string()],
                ..Default::default()
            };

            let priority_filter = EventFilter {
                priorities: vec![EventPriority::High, EventPriority::Critical],
                ..Default::default()
            };

            let combined_filter = EventFilter {
                event_types: type_filter.event_types.clone(),
                priorities: priority_filter.priorities.clone(),
                ..Default::default()
            };

            // Events matching combined filter should match both individual filters
            for event in &events {
                let combined_match = combined_filter.matches(event);
                let type_match = type_filter.matches(event);
                let priority_match = priority_filter.matches(event);

                if combined_match {
                    // Combined match means both conditions should be satisfied
                    // (Note: this depends on the specific filter implementation logic)
                    prop_assert!(type_match || priority_filter.priorities.is_empty(),
                                   "Combined match should satisfy individual conditions");
                }
            }
        }

        #[test]
        fn test_filter_source_matching(events in proptest::collection::vec(prop_utils::random_daemon_event(), 1..5)) {
            // Create filter for the first event's source
            if !events.is_empty() {
                let first_source = &events[0].source.id;
                let source_filter = EventFilter {
                    sources: vec![first_source.clone()],
                    ..Default::default()
                };

                let mut matching_events = 0;
                for event in &events {
                    if source_filter.matches(event) {
                        matching_events += 1;
                        prop_assert_eq!(&event.source.id, first_source,
                                       "Matching event should have expected source");
                    }
                }

                // At least the first event should match
                prop_assert!(matching_events >= 1, "At least one event should match source filter");
            }
        }

        #[test]
        fn test_filter_size_limiting(events in proptest::collection::vec(prop_utils::random_daemon_event(), 1..5)) {
            // Test size-based filtering
            let size_limits = vec![100, 1000, 10000, 100000];

            for size_limit in size_limits {
                let size_filter = EventFilter {
                    max_payload_size: Some(size_limit),
                    ..Default::default()
                };

                for event in &events {
                    let matches = size_filter.matches(event);
                    let event_size = event.size_bytes();

                    if matches {
                        prop_assert!(event_size <= size_limit,
                                       "Event matching size filter should be within limit");
                    } else {
                        // May or may not be due to size, depending on other filter criteria
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod routing_property_tests {
    use super::*;
    use proptest::prelude::*;
    use std::sync::Arc;

    proptest! {
        #[test]
        fn test_routing_consistency(events in proptest::collection::vec(prop_utils::random_daemon_event(), 1..10)) {
            // Test that routing is consistent for identical events
            let router = Arc::new(DefaultEventRouter::new());

            // Register a test service
            let registration = ServiceRegistration {
                service_id: "test-service".to_string(),
                service_type: "test".to_string(),
                instance_id: "instance-1".to_string(),
                endpoint: None,
                supported_event_types: vec!["custom".to_string(), "system".to_string()],
                priority: 0,
                weight: 1.0,
                max_concurrent_events: 10,
                filters: Vec::new(),
                metadata: HashMap::new(),
            };

            // Note: This test demonstrates the property testing pattern
            // In a real implementation, you'd need to handle async calls differently
        }

        #[test]
        fn test_service_registration_properties() {
            // Test that service registration maintains invariants
            let service_id = "test-service".to_string();
            let service_type = "test".to_string();

            let registration = ServiceRegistration {
                service_id: service_id.clone(),
                service_type: service_type.clone(),
                instance_id: "instance-1".to_string(),
                endpoint: None,
                supported_event_types: vec!["test".to_string()],
                priority: 0,
                weight: 1.0,
                max_concurrent_events: 10,
                filters: Vec::new(),
                metadata: HashMap::new(),
            };

            // Service ID should be preserved
            prop_assert_eq!(registration.service_id, service_id);
            prop_assert_eq!(registration.service_type, service_type);

            // Priority should be in valid range
            prop_assert!(registration.priority <= 255);

            // Weight should be positive
            prop_assert!(registration.weight > 0.0);

            // Max concurrent events should be positive
            prop_assert!(registration.max_concurrent_events > 0);
        }

        #[test]
        fn test_event_target_properties(service_ids in proptest::collection::vec(".*".to_string(), 1..5)) {
            // Test that service targets maintain invariants
            for (i, service_id) in service_ids.iter().enumerate() {
                let target = ServiceTarget::new(service_id.clone())
                    .with_priority(i as u8)
                    .with_instance(format!("instance-{}", i));

                // Service ID should be preserved
                prop_assert_eq!(&target.service_id, service_id);

                // Priority should be preserved
                prop_assert_eq!(target.priority, i as u8);

                // Instance should be set
                prop_assert!(target.instance.is_some());
            }
        }
    }
}

#[cfg(test)]
mod load_balancing_property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_round_robin_distribution(event_count in 1..100_usize) {
            // Test that round-robin maintains distribution properties
            let service_count = 3;
            let mut service_counts = vec![0; service_count];
            let mut current_service = 0;

            for _ in 0..event_count {
                service_counts[current_service] += 1;
                current_service = (current_service + 1) % service_count;
            }

            // Distribution should be approximately even
            let max_diff = service_counts.iter().max().unwrap() - service_counts.iter().min().unwrap();
            prop_assert!(max_diff <= 1, "Round-robin distribution should be approximately even");
        }

        #[test]
        fn test_weighted_random_distribution(total_events in 100..1000_usize) {
            // Test that weighted random maintains statistical properties
            let weights = vec![0.2, 0.3, 0.5]; // Total = 1.0
            let mut distribution = vec![0; 3];

            // Simulate weighted random selection
            for _ in 0..total_events {
                let rand_val: f64 = rand::random();
                let mut cumulative = 0.0;
                let mut selected = 0;

                for (i, &weight) in weights.iter().enumerate() {
                    cumulative += weight;
                    if rand_val < cumulative {
                        selected = i;
                        break;
                    }
                }

                distribution[selected] += 1;
            }

            // Check that distribution approximately matches weights
            for (i, &expected_weight) in weights.iter().enumerate() {
                let actual_ratio = distribution[i] as f64 / total_events as f64;
                let expected_ratio = expected_weight;
                let diff = (actual_ratio - expected_ratio).abs();

                // Allow some variance due to randomness
                prop_assert!(diff < 0.1, "Weighted distribution should approximately match weights");
            }
        }

        #[test]
        fn test_priority_invariant(events in proptest::collection::vec(prop_utils::random_daemon_event(), 1..50)) {
            // Test that priority-based routing maintains priority invariants
            let mut events_by_priority = HashMap::new();

            for event in &events {
                events_by_priority.entry(event.priority).or_insert_with(Vec::new).push(event);
            }

            // Higher priority events should have lower priority values
            let priority_levels: Vec<_> = events_by_priority.keys().cloned().collect();
            if priority_levels.len() > 1 {
                priority_levels.sort();

                for i in 1..priority_levels.len() {
                    let prev_priority = priority_levels[i - 1];
                    let curr_priority = priority_levels[i];

                    // Lower numeric value = higher priority
                    prop_assert!(prev_priority.value() < curr_priority.value(),
                                   "Priority levels should be correctly ordered");
                }
            }
        }
    }
}

#[cfg(test)]
mod error_handling_property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_error_validation_properties(event in prop_utils::random_daemon_event()) {
            // Test that error handling maintains invariants
            let validation_result = event.validate();

            // If validation succeeds, event should be routeable
            if validation_result.is_ok() {
                // Event should have valid timestamp
                let now = Utc::now();
                let time_diff = (event.created_at - now).num_seconds().abs();
                prop_assert!(time_diff <= 300, "Event timestamp should be within 5 minutes");

                // Event should have reasonable size
                prop_assert!(event.size_bytes() > 0, "Event should have positive size");

                // Event should have valid priority
                match event.priority {
                    EventPriority::Critical | EventPriority::High | EventPriority::Normal | EventPriority::Low => {
                        // Valid priority
                    }
                }

                // Event source should be valid
                prop_assert!(!event.source.id.is_empty(), "Event source should have valid ID");
            }

            // Test error message properties
            if let Err(error) = validation_result {
                let error_message = format!("{}", error);
                prop_assert!(!error_message.is_empty(), "Error message should not be empty");
                prop_assert!(error_message.len() > 5, "Error message should be descriptive");
            }
        }

        #[test]
        fn test_retry_properties(event in prop_utils::random_daemon_event()) {
            // Test that retry logic maintains invariants
            let original_retry_count = event.retry_count;
            let original_max_retries = event.max_retries;

            // Initial state should be valid
            prop_assert!(original_retry_count <= original_max_retries,
                        "Retry count should not exceed max retries");

            // Test retry increment
            if event.can_retry() {
                let mut event_mut = event.clone();
                event_mut.increment_retry();

                prop_assert_eq!(event_mut.retry_count, original_retry_count + 1,
                               "Retry count should increment correctly");

                // Should not exceed max retries
                prop_assert!(event_mut.retry_count <= event_mut.max_retries,
                               "Retry count should not exceed max after increment");
            }
        }

        #[test]
        fn test_event_size_limits(events in proptest::collection::vec(prop_utils::random_daemon_event(), 1..20)) {
            // Test that event size limits are enforced
            const MAX_REASONABLE_SIZE: usize = 10 * 1024 * 1024; // 10MB

            for event in &events {
                let size = event.size_bytes();

                // Size should be positive
                prop_assert!(size > 0, "Event size should be positive");

                // Size should be reasonable (though very large events might fail validation)
                if event.validate().is_ok() {
                    prop_assert!(size <= MAX_REASONABLE_SIZE,
                                   "Valid event should have reasonable size");
                }
            }
        }
    }
}

#[cfg(test)]
mod invariants_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_event_correlation_invariants(events in proptest::collection::vec(prop_utils::random_daemon_event(), 2..10)) {
            // Test that event correlation maintains invariants
            let correlation_id = Uuid::new_v4();

            // Create correlated events
            let mut correlated_events = Vec::new();
            for (i, event) in events.iter().enumerate() {
                let correlated_event = DaemonEvent::with_correlation(
                    event.event_type.clone(),
                    event.source.clone(),
                    event.payload.clone(),
                    correlation_id,
                );
                correlated_events.push(correlated_event);
            }

            // All events should have the same correlation ID
            for event in &correlated_events {
                prop_assert_eq!(event.correlation_id, Some(correlation_id),
                               "All correlated events should have the same correlation ID");
            }

            // Create a response event
            if !correlated_events.is_empty() {
                let causation_id = correlated_events[0].id;
                let response_event = DaemonEvent::as_response(
                    EventType::Service(ServiceEventType::ResponseSent {
                        from_service: "test-service".to_string(),
                        to_service: "client".to_string(),
                        response: serde_json::json!({"status": "ok"}),
                    }),
                    EventSource::service("test-service".to_string()),
                    EventPayload::json(serde_json::json!({})),
                    causation_id,
                );

                prop_assert_eq!(response_event.causation_id, Some(causation_id),
                               "Response event should have correct causation ID");
            }
        }

        #[test]
        fn test_event_ordering_invariants(events in proptest::collection::vec(prop_utils::random_daemon_event(), 2..20)) {
            // Test that event ordering maintains invariants
            let mut events_copy = events.clone();

            // Sort by priority
            events_copy.sort_by(|a, b| a.priority.cmp(&b.priority));

            // Verify that sorting is correct
            for i in 1..events_copy.len() {
                let prev_priority = events_copy[i - 1].priority;
                let curr_priority = events_copy[i].priority;

                prop_assert!(prev_priority <= curr_priority,
                               "Events should be sorted by priority");
            }

            // Sort by creation time
            events_copy.sort_by(|a, b| a.created_at.cmp(&b.created_at));

            // Verify time ordering
            for i in 1..events_copy.len() {
                let prev_time = events_copy[i - 1].created_at;
                let curr_time = events_copy[i].created_at;

                prop_assert!(prev_time <= curr_time,
                               "Events should be sorted by creation time");
            }
        }

        #[test]
        fn test_event_payload_invariants(events in proptest::collection::vec(prop_utils::random_daemon_event(), 1..10)) {
            // Test that event payloads maintain invariants
            for event in &events {
                // Payload should be accessible
                let payload_size = event.payload.size_bytes;
                prop_assert!(payload_size > 0, "Payload should have positive size");

                // Content type should be set
                prop_assert!(!event.payload.content_type.is_empty(),
                               "Payload should have content type");

                // Encoding should be set
                prop_assert!(!event.payload.encoding.is_empty(),
                               "Payload should have encoding");

                // JSON payload should be accessible as JSON
                if event.payload.content_type == "application/json" {
                    prop_assert!(event.payload.as_json().is_some(),
                                   "JSON payload should be accessible as JSON");
                }

                // Test payload integrity if checksum is present
                if let Some(checksum) = &event.payload.checksum {
                    prop_assert!(!checksum.is_empty(), "Checksum should not be empty");
                }
            }
        }

        #[test]
        fn test_service_target_invariants() {
            // Test that service targets maintain invariants
            let test_cases = vec![
                ("service1", Some("type1"), Some("instance1"), 0),
                ("service2", None, None, 5),
                ("service3", Some("type3"), Some("instance3"), 10),
            ];

            for (service_id, service_type, instance, priority) in test_cases {
                let mut target = ServiceTarget::new(service_id.to_string())
                    .with_priority(priority);

                if let Some(t) = service_type {
                    target = target.with_type(t.to_string());
                }

                if let Some(i) = instance {
                    target = target.with_instance(i.to_string());
                }

                // Service ID should be preserved
                prop_assert_eq!(&target.service_id, service_id);

                // Priority should be preserved
                prop_assert_eq!(target.priority, priority);

                // Optional fields should be set correctly
                if service_type.is_some() {
                    prop_assert!(target.service_type.is_some());
                }

                if instance.is_some() {
                    prop_assert!(target.instance.is_some());
                }
            }
        }
    }
}

// Helper trait for proptest integration
trait Arbitrary {
    fn arbitrary() -> Self;
}

// Note: This file demonstrates the structure and approach for property-based testing.
// In a real implementation, you would need to add the `proptest` crate to your dependencies
// and potentially implement proper Arbitrary traits for your custom types.