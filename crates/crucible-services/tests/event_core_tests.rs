//! Comprehensive unit tests for core event types and structures

use chrono::{DateTime, Utc};
use crucible_services::events::core::*;
use crucible_services::events::errors::{EventError, EventResult};
use serde_json;
use std::collections::HashMap;
use uuid::Uuid;

/// Test helper to create a basic event for testing
fn create_test_event() -> DaemonEvent {
    let event_type = EventType::Filesystem(FilesystemEventType::FileCreated {
        path: "/test/file.txt".to_string(),
    });
    let source = EventSource::service("test-service".to_string());
    let payload = EventPayload::json(serde_json::json!({"test": "data"}));

    DaemonEvent::new(event_type, source, payload)
}

/// Test helper to create test event metadata
fn create_test_metadata() -> EventMetadata {
    let mut metadata = EventMetadata::new();
    metadata.add_field("test_key".to_string(), "test_value".to_string());
    metadata.add_debug_info("debug_key".to_string(), "debug_value".to_string());
    metadata
}

#[cfg(test)]
mod daemon_event_tests {
    use super::*;

    #[test]
    fn test_daemon_event_creation() {
        let event = create_test_event();

        assert!(event.id.version() == 4); // UUID v4
        assert_eq!(event.priority, EventPriority::Normal);
        assert_eq!(event.retry_count, 0);
        assert_eq!(event.max_retries, 3);
        assert!(event.targets.is_empty());
        assert!(event.correlation_id.is_none());
        assert!(event.causation_id.is_none());
        assert!(event.scheduled_at.is_none());
        assert!(event.created_at <= Utc::now());
    }

    #[test]
    fn test_daemon_event_with_correlation() {
        let correlation_id = Uuid::new_v4();
        let event = DaemonEvent::with_correlation(
            EventType::System(SystemEventType::DaemonStarted {
                version: "1.0.0".to_string(),
            }),
            EventSource::system("daemon".to_string()),
            EventPayload::json(serde_json::json!({})),
            correlation_id,
        );

        assert_eq!(event.correlation_id, Some(correlation_id));
        assert!(event.causation_id.is_none());
    }

    #[test]
    fn test_daemon_event_as_response() {
        let causation_id = Uuid::new_v4();
        let event = DaemonEvent::as_response(
            EventType::Service(ServiceEventType::ResponseSent {
                from_service: "service-a".to_string(),
                to_service: "service-b".to_string(),
                response: serde_json::json!({"result": "ok"}),
            }),
            EventSource::service("service-a".to_string()),
            EventPayload::json(serde_json::json!({"response": "data"})),
            causation_id,
        );

        assert_eq!(event.causation_id, Some(causation_id));
        assert!(event.correlation_id.is_none());
    }

    #[test]
    fn test_daemon_event_builder_methods() {
        let event = create_test_event()
            .with_priority(EventPriority::Critical)
            .with_target(ServiceTarget::new("target-service".to_string()))
            .with_max_retries(5)
            .with_metadata("custom_key".to_string(), "custom_value".to_string());

        assert_eq!(event.priority, EventPriority::Critical);
        assert_eq!(event.targets.len(), 1);
        assert_eq!(event.targets[0].service_id, "target-service");
        assert_eq!(event.max_retries, 5);
        assert_eq!(
            event.metadata.get_field("custom_key"),
            Some(&"custom_value".to_string())
        );
    }

    #[test]
    fn test_daemon_event_scheduling() {
        let future_time = Utc::now() + chrono::Duration::hours(1);
        let event = create_test_event().with_schedule(future_time);

        assert_eq!(event.scheduled_at, Some(future_time));
        assert!(event.is_scheduled());
    }

    #[test]
    fn test_daemon_event_retry_logic() {
        let mut event = create_test_event();

        assert!(event.can_retry());
        assert_eq!(event.retry_count, 0);

        event.increment_retry();
        assert_eq!(event.retry_count, 1);
        assert!(event.can_retry());

        // Exceed max retries
        for _ in 0..event.max_retries {
            event.increment_retry();
        }

        assert!(!event.can_retry());
    }

    #[test]
    fn test_daemon_event_size_calculation() {
        let event = create_test_event();
        let size = event.size_bytes();

        assert!(size > 0);

        // Test with larger payload
        let large_payload = EventPayload::json(serde_json::json!({
            "data": "x".repeat(1000)
        }));
        let large_event = DaemonEvent::new(
            EventType::Custom("large-event".to_string()),
            EventSource::service("test".to_string()),
            large_payload,
        );

        assert!(large_event.size_bytes() > size);
    }

    #[test]
    fn test_daemon_event_validation() -> EventResult<()> {
        let event = create_test_event();

        // Valid event should pass
        assert!(event.validate().is_ok());

        // Test broadcast-allowed event without targets
        let system_event = DaemonEvent::new(
            EventType::System(SystemEventType::DaemonStarted {
                version: "1.0.0".to_string(),
            }),
            EventSource::system("daemon".to_string()),
            EventPayload::json(serde_json::json!({})),
        );
        assert!(system_event.validate().is_ok());

        // Test event requiring targets but none provided
        let service_event = DaemonEvent::new(
            EventType::Service(ServiceEventType::RequestReceived {
                from_service: "service-a".to_string(),
                to_service: "service-b".to_string(),
                request: serde_json::json!({}),
            }),
            EventSource::service("service-a".to_string()),
            EventPayload::json(serde_json::json!({})),
        );
        assert!(service_event.validate().is_err());

        Ok(())
    }

    #[test]
    fn test_daemon_event_serialization() {
        let event = create_test_event()
            .with_priority(EventPriority::High)
            .with_metadata("test".to_string(), "value".to_string());

        let serialized = serde_json::to_string(&event).unwrap();
        let deserialized: DaemonEvent = serde_json::from_str(&serialized).unwrap();

        assert_eq!(event.id, deserialized.id);
        assert_eq!(event.priority, deserialized.priority);
        assert_eq!(event.event_type, deserialized.event_type);
        assert_eq!(event.metadata.fields, deserialized.metadata.fields);
    }
}

#[cfg(test)]
mod event_type_tests {
    use super::*;

    #[test]
    fn test_event_type_category() {
        let tests = vec![
            (EventType::Filesystem(FilesystemEventType::FileCreated { path: "/test".to_string() }), EventCategory::Filesystem),
            (EventType::Database(DatabaseEventType::RecordCreated { table: "users".to_string(), id: "1".to_string() }), EventCategory::Database),
            (EventType::External(ExternalEventType::DataReceived { source: "api".to_string(), data: serde_json::json!({}) }), EventCategory::External),
            (EventType::Mcp(McpEventType::ToolCall { tool_name: "test".to_string(), parameters: serde_json::json!({}) }), EventCategory::Mcp),
            (EventType::Service(ServiceEventType::HealthCheck { service_id: "test".to_string(), status: "healthy".to_string() }), EventCategory::Service),
            (EventType::System(SystemEventType::DaemonStarted { version: "1.0.0".to_string() }), EventCategory::System),
            (EventType::Custom("custom-event".to_string()), EventCategory::Custom),
        ];

        for (event_type, expected_category) in tests {
            assert_eq!(event_type.category(), expected_category);
        }
    }

    #[test]
    fn test_broadcast_allowed_events() {
        let broadcast_allowed = vec![
            EventType::System(SystemEventType::DaemonStarted { version: "1.0.0".to_string() }),
            EventType::Service(ServiceEventType::HealthCheck { service_id: "test".to_string(), status: "healthy".to_string() }),
            EventType::Service(ServiceEventType::ServiceRegistered { service_id: "test".to_string(), service_type: "test".to_string() }),
            EventType::Service(ServiceEventType::ServiceUnregistered { service_id: "test".to_string() }),
        ];

        for event_type in broadcast_allowed {
            assert!(event_type.is_broadcast_allowed());
        }

        let not_broadcast_allowed = vec![
            EventType::Filesystem(FilesystemEventType::FileCreated { path: "/test".to_string() }),
            EventType::Database(DatabaseEventType::RecordCreated { table: "users".to_string(), id: "1".to_string() }),
            EventType::Service(ServiceEventType::RequestReceived { from_service: "a".to_string(), to_service: "b".to_string(), request: serde_json::json!({}) }),
        ];

        for event_type in not_broadcast_allowed {
            assert!(!event_type.is_broadcast_allowed());
        }
    }

    #[test]
    fn test_event_type_serialization() {
        let event_types = vec![
            EventType::Filesystem(FilesystemEventType::FileCreated { path: "/test/file.txt".to_string() }),
            EventType::Database(DatabaseEventType::RecordUpdated {
                table: "users".to_string(),
                id: "1".to_string(),
                changes: HashMap::from([
                    ("name".to_string(), serde_json::Value::String("John".to_string())),
                    ("email".to_string(), serde_json::Value::String("john@example.com".to_string())),
                ]),
            }),
            EventType::Mcp(McpEventType::ToolResponse {
                tool_name: "test_tool".to_string(),
                result: serde_json::json!({"output": "success"}),
            }),
        ];

        for event_type in event_types {
            let serialized = serde_json::to_string(&event_type).unwrap();
            let deserialized: EventType = serde_json::from_str(&serialized).unwrap();
            assert_eq!(event_type, deserialized);
        }
    }
}

#[cfg(test)]
mod event_priority_tests {
    use super::*;

    #[test]
    fn test_event_priority_ordering() {
        let priorities = vec![
            EventPriority::Critical,
            EventPriority::High,
            EventPriority::Normal,
            EventPriority::Low,
        ];

        assert!(EventPriority::Critical < EventPriority::High);
        assert!(EventPriority::High < EventPriority::Normal);
        assert!(EventPriority::Normal < EventPriority::Low);

        // Test that the array is sorted
        let mut sorted_priorities = priorities.clone();
        sorted_priorities.sort();
        assert_eq!(priorities, sorted_priorities);
    }

    #[test]
    fn test_event_priority_values() {
        assert_eq!(EventPriority::Critical.value(), 0);
        assert_eq!(EventPriority::High.value(), 1);
        assert_eq!(EventPriority::Normal.value(), 2);
        assert_eq!(EventPriority::Low.value(), 3);
    }
}

#[cfg(test)]
mod event_source_tests {
    use super::*;

    #[test]
    fn test_event_source_creation() {
        let service_source = EventSource::service("test-service".to_string());
        assert_eq!(service_source.id, "test-service");
        assert_eq!(service_source.source_type, SourceType::Service);

        let filesystem_source = EventSource::filesystem("watcher-1".to_string());
        assert_eq!(filesystem_source.id, "watcher-1");
        assert_eq!(filesystem_source.source_type, SourceType::Filesystem);

        let external_source = EventSource::external("api-gateway".to_string());
        assert_eq!(external_source.id, "api-gateway");
        assert_eq!(external_source.source_type, SourceType::External);
    }

    #[test]
    fn test_event_source_builder_methods() {
        let source = EventSource::new("test-source".to_string(), SourceType::Custom("custom".to_string()))
            .with_instance("instance-1".to_string())
            .with_metadata("meta_key".to_string(), "meta_value".to_string());

        assert_eq!(source.id, "test-source");
        assert_eq!(source.instance, Some("instance-1".to_string()));
        assert_eq!(
            source.metadata.get("meta_key"),
            Some(&"meta_value".to_string())
        );
    }

    #[test]
    fn test_event_source_serialization() {
        let source = EventSource::service("test-service".to_string())
            .with_instance("instance-1".to_string())
            .with_metadata("key".to_string(), "value".to_string());

        let serialized = serde_json::to_string(&source).unwrap();
        let deserialized: EventSource = serde_json::from_str(&serialized).unwrap();

        assert_eq!(source.id, deserialized.id);
        assert_eq!(source.source_type, deserialized.source_type);
        assert_eq!(source.instance, deserialized.instance);
        assert_eq!(source.metadata, deserialized.metadata);
    }
}

#[cfg(test)]
mod service_target_tests {
    use super::*;

    #[test]
    fn test_service_target_creation() {
        let target = ServiceTarget::new("test-service".to_string());
        assert_eq!(target.service_id, "test-service");
        assert_eq!(target.priority, 0);
        assert!(target.service_type.is_none());
        assert!(target.instance.is_none());
        assert!(target.filters.is_empty());
    }

    #[test]
    fn test_service_target_builder_methods() {
        let filter = EventFilter::new();
        let target = ServiceTarget::new("test-service".to_string())
            .with_type("service-type".to_string())
            .with_instance("instance-1".to_string())
            .with_priority(5)
            .with_filter(filter);

        assert_eq!(target.service_id, "test-service");
        assert_eq!(target.service_type, Some("service-type".to_string()));
        assert_eq!(target.instance, Some("instance-1".to_string()));
        assert_eq!(target.priority, 5);
        assert_eq!(target.filters.len(), 1);
    }

    #[test]
    fn test_service_target_serialization() {
        let target = ServiceTarget::new("test-service".to_string())
            .with_type("test-type".to_string())
            .with_priority(3);

        let serialized = serde_json::to_string(&target).unwrap();
        let deserialized: ServiceTarget = serde_json::from_str(&serialized).unwrap();

        assert_eq!(target.service_id, deserialized.service_id);
        assert_eq!(target.service_type, deserialized.service_type);
        assert_eq!(target.priority, deserialized.priority);
    }
}

#[cfg(test)]
mod event_payload_tests {
    use super::*;

    #[test]
    fn test_event_payload_json() {
        let data = serde_json::json!({
            "key": "value",
            "number": 42,
            "nested": {"inner": "data"}
        });
        let payload = EventPayload::json(data.clone());

        assert_eq!(payload.content_type, "application/json");
        assert_eq!(payload.encoding, "utf-8");
        assert_eq!(payload.as_json(), Some(data));
        assert!(payload.as_string().is_none());
        assert!(payload.size_bytes > 0);
    }

    #[test]
    fn test_event_payload_text() {
        let text = "Hello, world!".to_string();
        let payload = EventPayload::text(text.clone());

        assert_eq!(payload.content_type, "text/plain");
        assert_eq!(payload.encoding, "utf-8");
        assert_eq!(payload.as_string(), Some(text));
        assert!(payload.as_json().is_none());
        assert!(payload.size_bytes > 0);
    }

    #[test]
    fn test_event_payload_binary() {
        let data = b"binary data".to_vec();
        let payload = EventPayload::binary(data.clone(), "application/octet-stream".to_string());

        assert_eq!(payload.content_type, "application/octet-stream");
        assert_eq!(payload.encoding, "base64");
        assert!(payload.checksum.is_some());
        assert!(payload.verify_integrity());
    }

    #[test]
    fn test_event_payload_integrity_verification() {
        let data = b"test data for integrity".to_vec();
        let payload = EventPayload::binary(data.clone(), "application/octet-stream".to_string());

        // Valid payload should pass verification
        assert!(payload.verify_integrity());

        // Modify the data (simulating corruption)
        let mut corrupted_payload = payload;
        if let Some(encoded_data) = corrupted_payload.as_string() {
            let mut modified_data = encoded_data;
            modified_data.push_str("corruption");
            corrupted_payload.data = serde_json::Value::String(modified_data);
        }

        // Corrupted payload should fail verification
        assert!(!corrupted_payload.verify_integrity());
    }

    #[test]
    fn test_event_payload_serialization() {
        let payload = EventPayload::json(serde_json::json!({"test": "data"}));

        let serialized = serde_json::to_string(&payload).unwrap();
        let deserialized: EventPayload = serde_json::from_str(&serialized).unwrap();

        assert_eq!(payload.content_type, deserialized.content_type);
        assert_eq!(payload.encoding, deserialized.encoding);
        assert_eq!(payload.data, deserialized.data);
        assert_eq!(payload.size_bytes, deserialized.size_bytes);
    }
}

#[cfg(test)]
mod event_metadata_tests {
    use super::*;

    #[test]
    fn test_event_metadata_creation_and_fields() {
        let mut metadata = EventMetadata::new();

        assert!(metadata.fields.is_empty());
        assert_eq!(metadata.metrics.processing_attempts, 0);
        assert!(metadata.metrics.processing_started_at.is_none());

        metadata.add_field("key1".to_string(), "value1".to_string());
        metadata.add_field("key2".to_string(), "value2".to_string());

        assert_eq!(metadata.fields.len(), 2);
        assert_eq!(metadata.get_field("key1"), Some(&"value1".to_string()));
        assert_eq!(metadata.get_field("key2"), Some(&"value2".to_string()));
        assert_eq!(metadata.get_field("nonexistent"), None);
    }

    #[test]
    fn test_event_metrics_updates() {
        let mut metadata = EventMetadata::new();

        metadata.update_metrics(|metrics| {
            metrics.start_processing();
            metrics.add_success("service1".to_string());
            metrics.add_failure("service2".to_string());
        });

        assert!(metadata.metrics.processing_started_at.is_some());
        assert_eq!(metadata.metrics.processing_attempts, 1);
        assert!(metadata.metrics.processed_by.contains(&"service1".to_string()));
        assert!(metadata.metrics.failed_by.contains(&"service2".to_string()));

        metadata.update_metrics(|metrics| {
            metrics.complete_processing();
        });

        assert!(metadata.metrics.processing_duration_ms.is_some());
    }

    #[test]
    fn test_debug_info() {
        let mut metadata = EventMetadata::new();

        metadata.add_debug_info("debug_key".to_string(), "debug_value".to_string());

        assert_eq!(metadata.debug.info.get("debug_key"), Some(&"debug_value".to_string()));
        assert!(metadata.debug.stack_trace.is_none());
        assert!(metadata.debug.source_location.is_none());

        let debug_info = DebugInfo::new()
            .with_stack_trace("stack trace here".to_string())
            .with_source_location(SourceLocation {
                file: "test.rs".to_string(),
                line: 42,
                function: Some("test_function".to_string()),
            });

        assert_eq!(debug_info.stack_trace, Some("stack trace here".to_string()));
        assert!(debug_info.source_location.is_some());
    }

    #[test]
    fn test_event_metadata_serialization() {
        let mut metadata = EventMetadata::new();
        metadata.add_field("test".to_string(), "value".to_string());
        metadata.update_metrics(|m| m.start_processing());

        let serialized = serde_json::to_string(&metadata).unwrap();
        let deserialized: EventMetadata = serde_json::from_str(&serialized).unwrap();

        assert_eq!(metadata.fields, deserialized.fields);
        assert_eq!(metadata.metrics.processing_attempts, deserialized.metrics.processing_attempts);
    }
}

#[cfg(test)]
mod event_filter_tests {
    use super::*;

    fn create_test_event_with_filters() -> DaemonEvent {
        DaemonEvent::new(
            EventType::Filesystem(FilesystemEventType::FileCreated { path: "/test.txt".to_string() }),
            EventSource::service("test-service".to_string()),
            EventPayload::json(serde_json::json!({"content": "test data"})),
        )
        .with_priority(EventPriority::High)
    }

    #[test]
    fn test_event_filter_default() {
        let filter = EventFilter::default();
        let event = create_test_event_with_filters();

        // Default filter should match all events
        assert!(filter.matches(&event));
    }

    #[test]
    fn test_event_filter_by_event_types() {
        let filter = EventFilter {
            event_types: vec!["filesystem".to_string(), "database".to_string()],
            ..Default::default()
        };

        let fs_event = create_test_event_with_filters();
        assert!(filter.matches(&fs_event));

        let db_event = DaemonEvent::new(
            EventType::Database(DatabaseEventType::RecordCreated { table: "users".to_string(), id: "1".to_string() }),
            EventSource::service("test".to_string()),
            EventPayload::json(serde_json::json!({})),
        );
        assert!(filter.matches(&db_event));

        let system_event = DaemonEvent::new(
            EventType::System(SystemEventType::DaemonStarted { version: "1.0.0".to_string() }),
            EventSource::system("daemon".to_string()),
            EventPayload::json(serde_json::json!({})),
        );
        assert!(!filter.matches(&system_event));
    }

    #[test]
    fn test_event_filter_by_categories() {
        let filter = EventFilter {
            categories: vec![EventCategory::Filesystem, EventCategory::System],
            ..Default::default()
        };

        let fs_event = create_test_event_with_filters();
        assert!(filter.matches(&fs_event));

        let system_event = DaemonEvent::new(
            EventType::System(SystemEventType::DaemonStarted { version: "1.0.0".to_string() }),
            EventSource::system("daemon".to_string()),
            EventPayload::json(serde_json::json!({})),
        );
        assert!(filter.matches(&system_event));

        let db_event = DaemonEvent::new(
            EventType::Database(DatabaseEventType::RecordCreated { table: "users".to_string(), id: "1".to_string() }),
            EventSource::service("test".to_string()),
            EventPayload::json(serde_json::json!({})),
        );
        assert!(!filter.matches(&db_event));
    }

    #[test]
    fn test_event_filter_by_priorities() {
        let filter = EventFilter {
            priorities: vec![EventPriority::Critical, EventPriority::High],
            ..Default::default()
        };

        let high_priority_event = create_test_event_with_filters();
        assert!(filter.matches(&high_priority_event));

        let low_priority_event = DaemonEvent::new(
            EventType::System(SystemEventType::DaemonStarted { version: "1.0.0".to_string() }),
            EventSource::system("daemon".to_string()),
            EventPayload::json(serde_json::json!({})),
        )
        .with_priority(EventPriority::Low);
        assert!(!filter.matches(&low_priority_event));
    }

    #[test]
    fn test_event_filter_by_sources() {
        let filter = EventFilter {
            sources: vec!["test-service".to_string(), "another-service".to_string()],
            ..Default::default()
        };

        let matching_event = create_test_event_with_filters();
        assert!(filter.matches(&matching_event));

        let non_matching_event = DaemonEvent::new(
            EventType::System(SystemEventType::DaemonStarted { version: "1.0.0".to_string() }),
            EventSource::service("different-service".to_string()),
            EventPayload::json(serde_json::json!({})),
        );
        assert!(!filter.matches(&non_matching_event));
    }

    #[test]
    fn test_event_filter_by_payload_size() {
        let filter = EventFilter {
            max_payload_size: Some(100),
            ..Default::default()
        };

        let small_payload_event = DaemonEvent::new(
            EventType::System(SystemEventType::DaemonStarted { version: "1.0.0".to_string() }),
            EventSource::system("daemon".to_string()),
            EventPayload::json(serde_json::json!({"small": "data"})),
        );
        assert!(filter.matches(&small_payload_event));

        let large_payload_event = DaemonEvent::new(
            EventType::System(SystemEventType::DaemonStarted { version: "1.0.0".to_string() }),
            EventSource::system("daemon".to_string()),
            EventPayload::json(serde_json::json!({"large": "x".repeat(200)})),
        );
        assert!(!filter.matches(&large_payload_event));
    }

    #[test]
    fn test_event_filter_custom_expression() {
        let filter = EventFilter {
            expression: Some("test-service FilesystemCreated".to_string()),
            ..Default::default()
        };

        let matching_event = create_test_event_with_filters();
        assert!(filter.matches(&matching_event));

        let non_matching_event = DaemonEvent::new(
            EventType::Database(DatabaseEventType::RecordCreated { table: "users".to_string(), id: "1".to_string() }),
            EventSource::service("different-service".to_string()),
            EventPayload::json(serde_json::json!({})),
        );
        assert!(!filter.matches(&non_matching_event));
    }

    #[test]
    fn test_event_filter_combination() {
        let filter = EventFilter {
            event_types: vec!["filesystem".to_string()],
            priorities: vec![EventPriority::High, EventPriority::Critical],
            sources: vec!["test-service".to_string()],
            max_payload_size: Some(1000),
            ..Default::default()
        };

        let matching_event = create_test_event_with_filters();
        assert!(filter.matches(&matching_event));

        // Test non-matching combinations
        let wrong_type_event = DaemonEvent::new(
            EventType::Database(DatabaseEventType::RecordCreated { table: "users".to_string(), id: "1".to_string() }),
            EventSource::service("test-service".to_string()),
            EventPayload::json(serde_json::json!({})),
        )
        .with_priority(EventPriority::High);
        assert!(!filter.matches(&wrong_type_event));

        let wrong_priority_event = DaemonEvent::new(
            EventType::Filesystem(FilesystemEventType::FileCreated { path: "/test".to_string() }),
            EventSource::service("test-service".to_string()),
            EventPayload::json(serde_json::json!({})),
        )
        .with_priority(EventPriority::Low);
        assert!(!filter.matches(&wrong_priority_event));
    }

    #[test]
    fn test_event_filter_serialization() {
        let filter = EventFilter {
            event_types: vec!["filesystem".to_string(), "database".to_string()],
            categories: vec![EventCategory::Filesystem],
            priorities: vec![EventPriority::High],
            sources: vec!["test-service".to_string()],
            max_payload_size: Some(500),
            expression: Some("test expression".to_string()),
        };

        let serialized = serde_json::to_string(&filter).unwrap();
        let deserialized: EventFilter = serde_json::from_str(&serialized).unwrap();

        assert_eq!(filter.event_types, deserialized.event_types);
        assert_eq!(filter.categories, deserialized.categories);
        assert_eq!(filter.priorities, deserialized.priorities);
        assert_eq!(filter.sources, deserialized.sources);
        assert_eq!(filter.max_payload_size, deserialized.max_payload_size);
        assert_eq!(filter.expression, deserialized.expression);
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[test]
    fn test_event_error_creation() {
        let routing_error = EventError::routing_error("Service not found");
        assert!(matches!(routing_error, EventError::RoutingError(_)));

        let validation_error = EventError::validation_error("Invalid event data");
        assert!(matches!(validation_error, EventError::ValidationError(_)));

        let processing_error = EventError::processing_error("Failed to process event");
        assert!(matches!(processing_error, EventError::ProcessingError(_)));

        let delivery_error = EventError::delivery_error("service-1", "Connection timeout");
        assert!(matches!(delivery_error, EventError::DeliveryError { service_id: _, reason: _ }));
    }

    #[test]
    fn test_event_error_display() {
        let errors = vec![
            EventError::RoutingError("Service not found".to_string()),
            EventError::ValidationError("Invalid payload".to_string()),
            EventError::QueueFull { capacity: 1000 },
            EventError::Timeout { duration_ms: 5000 },
            EventError::EventTooLarge { size: 5000, max_size: 1000 },
        ];

        for error in errors {
            let display_string = format!("{}", error);
            assert!(!display_string.is_empty());
            assert!(display_string.len() > 5); // Ensure meaningful error messages
        }
    }

    #[test]
    fn test_event_error_serialization() {
        let error = EventError::ValidationError("test error".to_string());

        // Note: thiserror doesn't automatically make errors serializable
        // but we can test the error creation and matching
        assert!(matches!(error, EventError::ValidationError(msg) if msg == "test error"));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_complete_event_workflow() {
        // Create a complex event with all features
        let correlation_id = Uuid::new_v4();
        let causation_id = Uuid::new_v4();
        let future_time = Utc::now() + chrono::Duration::minutes(30);

        let event = DaemonEvent::with_correlation(
            EventType::Service(ServiceEventType::RequestReceived {
                from_service: "client-service".to_string(),
                to_service: "processor-service".to_string(),
                request: serde_json::json!({
                    "action": "process_data",
                    "data": "sample data",
                    "options": {"fast": true}
                }),
            }),
            EventSource::service("client-service".to_string())
                .with_instance("instance-1".to_string())
                .with_metadata("version".to_string(), "1.0.0".to_string()),
            EventPayload::json(serde_json::json!({
                "request_id": "req-123",
                "timestamp": Utc::now().to_rfc3339(),
                "payload_size": 256
            })),
            correlation_id,
        )
        .as_response(
            EventType::Service(ServiceEventType::ResponseSent {
                from_service: "processor-service".to_string(),
                to_service: "client-service".to_string(),
                response: serde_json::json!({
                    "status": "success",
                    "result": "Data processed successfully"
                }),
            }),
            EventSource::service("processor-service".to_string()),
            EventPayload::json(serde_json::json!({
                "response_id": "resp-456",
                "processing_time_ms": 150,
                "metadata": {"worker_id": "worker-3"}
            })),
            causation_id,
        )
        .with_priority(EventPriority::High)
        .with_target(ServiceTarget::new("client-service".to_string()).with_priority(1))
        .with_schedule(future_time)
        .with_max_retries(5)
        .with_metadata("workflow".to_string(), "data_processing".to_string())
        .with_metadata("environment".to_string(), "production".to_string());

        // Verify all properties
        assert_eq!(event.correlation_id, Some(correlation_id));
        assert_eq!(event.causation_id, Some(causation_id));
        assert_eq!(event.priority, EventPriority::High);
        assert_eq!(event.targets.len(), 1);
        assert_eq!(event.scheduled_at, Some(future_time));
        assert_eq!(event.max_retries, 5);
        assert_eq!(event.metadata.fields.len(), 3);
        assert!(event.is_scheduled());
        assert!(event.can_retry());

        // Test validation
        assert!(event.validate().is_ok());

        // Test serialization round-trip
        let serialized = serde_json::to_string(&event).unwrap();
        let deserialized: DaemonEvent = serde_json::from_str(&serialized).unwrap();

        assert_eq!(event.id, deserialized.id);
        assert_eq!(event.correlation_id, deserialized.correlation_id);
        assert_eq!(event.causation_id, deserialized.causation_id);
        assert_eq!(event.priority, deserialized.priority);
        assert_eq!(event.event_type, deserialized.event_type);
    }

    #[test]
    fn test_event_filtering_complex_scenario() {
        // Create multiple events of different types
        let events = vec![
            DaemonEvent::new(
                EventType::Filesystem(FilesystemEventType::FileCreated { path: "/file1.txt".to_string() }),
                EventSource::service("fs-watcher".to_string()),
                EventPayload::json(serde_json::json!({"size": 1024})),
            )
            .with_priority(EventPriority::Normal),

            DaemonEvent::new(
                EventType::Database(DatabaseEventType::RecordUpdated {
                    table: "users".to_string(),
                    id: "123".to_string(),
                    changes: HashMap::from([("status".to_string(), serde_json::Value::String("active".to_string()))]),
                }),
                EventSource::service("db-service".to_string()),
                EventPayload::json(serde_json::json!({"affected_rows": 1})),
            )
            .with_priority(EventPriority::High),

            DaemonEvent::new(
                EventType::Mcp(McpEventType::ToolCall {
                    tool_name: "search".to_string(),
                    parameters: serde_json::json!({"query": "test"}),
                }),
                EventSource::external("mcp-client".to_string()),
                EventPayload::json(serde_json::json!({"call_id": "call-123"})),
            )
            .with_priority(EventPriority::Critical),
        ];

        // Create filters for different scenarios
        let high_priority_filter = EventFilter {
            priorities: vec![EventPriority::High, EventPriority::Critical],
            ..Default::default()
        };

        let service_source_filter = EventFilter {
            sources: vec!["fs-watcher".to_string(), "db-service".to_string()],
            ..Default::default()
        };

        let database_filter = EventFilter {
            categories: vec![EventCategory::Database],
            ..Default::default()
        };

        // Test filtering
        let high_priority_events: Vec<_> = events.iter()
            .filter(|e| high_priority_filter.matches(e))
            .collect();
        assert_eq!(high_priority_events.len(), 2);

        let service_source_events: Vec<_> = events.iter()
            .filter(|e| service_source_filter.matches(e))
            .collect();
        assert_eq!(service_source_events.len(), 2);

        let database_events: Vec<_> = events.iter()
            .filter(|e| database_filter.matches(e))
            .collect();
        assert_eq!(database_events.len(), 1);
    }
}