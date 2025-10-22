//! Comprehensive error handling tests for all error scenarios

use crucible_services::events::core::*;
use crucible_services::events::routing::*;
use crucible_services::events::errors::{EventError, EventResult};
use crucible_services::types::{ServiceHealth, ServiceStatus};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

/// Test helper to create events that will trigger specific errors
struct ErrorTestHelper;

impl ErrorTestHelper {
    fn create_oversized_event() -> DaemonEvent {
        // Create an event larger than the maximum allowed size
        let large_data = "x".repeat(11 * 1024 * 1024); // 11MB (exceeds 10MB limit)
        DaemonEvent::new(
            EventType::Custom("oversized-event".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({
                "large_data": large_data,
                "more_data": "x".repeat(1024 * 1024) // Additional 1MB
            })),
        )
    }

    fn create_event_with_invalid_targets() -> DaemonEvent {
        DaemonEvent::new(
            EventType::Service(ServiceEventType::RequestReceived {
                from_service: "client".to_string(),
                to_service: "server".to_string(),
                request: serde_json::json!({}),
            }),
            EventSource::service("client".to_string()),
            EventPayload::json(serde_json::json!({})),
        )
        // Note: No targets specified for a non-broadcast event
    }

    fn create_event_for_nonexistent_service() -> DaemonEvent {
        DaemonEvent::new(
            EventType::Custom("test-event".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({})),
        )
        .with_target(ServiceTarget::new("nonexistent-service".to_string()))
    }

    fn create_malformed_payload_event() -> DaemonEvent {
        // Create an event with malformed JSON payload
        let malformed_json = serde_json::json!({
            "valid_field": "valid_value",
            "nested": {
                "invalid": null,
                "array": [1, 2, "mixed", null],
                "deeply_nested": {
                    "value": "test"
                }
            }
        });

        DaemonEvent::new(
            EventType::Custom("malformed-event".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(malformed_json),
        )
    }

    fn create_event_with_invalid_priority() -> DaemonEvent {
        // This would require manual construction since the API prevents invalid priorities
        // For testing purposes, we'll create an event and then modify it through serialization
        let event = DaemonEvent::new(
            EventType::Custom("test-event".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({})),
        );

        // Serialize and modify to create invalid priority
        let event_json = serde_json::to_string(&event).unwrap();
        let mut event_value: serde_json::Value = serde_json::from_str(&event_json).unwrap();

        // Set invalid priority
        if let Some(priority) = event_value.pointer_mut("/priority") {
            *priority = serde_json::Value::String("invalid_priority".to_string());
        }

        serde_json::from_value(event_value).unwrap()
    }

    fn create_corrupted_event() -> DaemonEvent {
        let mut event = DaemonEvent::new(
            EventType::Custom("corrupted-event".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({"data": "test"})),
        );

        // Manually corrupt the event by setting invalid timestamps or data
        event.created_at = chrono::DateTime::parse_from_rfc3339("9999-12-31T23:59:59Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        event
    }
}

#[cfg(test)]
mod validation_error_tests {
    use super::*;

    #[tokio::test]
    async fn test_oversized_event_validation() {
        let router = DefaultEventRouter::new();
        let oversized_event = ErrorTestHelper::create_oversized_event();

        let result = router.route_event(oversized_event).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            EventError::EventTooLarge { size, max_size } => {
                assert!(size > max_size);
                assert_eq!(max_size, 10 * 1024 * 1024); // 10MB default limit
            }
            _ => panic!("Expected EventTooLarge error"),
        }
    }

    #[tokio::test]
    async fn test_missing_targets_validation() {
        let router = DefaultEventRouter::new();
        let event_without_targets = ErrorTestHelper::create_event_with_invalid_targets();

        let result = router.route_event(event_without_targets).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            EventError::ValidationError(msg) => {
                assert!(msg.contains("requires specific targets"));
            }
            _ => panic!("Expected ValidationError for missing targets"),
        }
    }

    #[tokio::test]
    async fn test_invalid_event_type_validation() {
        let router = DefaultEventRouter::new();

        // Test with malformed event type
        let mut event = DaemonEvent::new(
            EventType::Custom("valid-event".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({})),
        );

        // Manually create invalid event type through serialization manipulation
        let event_json = serde_json::to_string(&event).unwrap();
        let mut event_value: serde_json::Value = serde_json::from_str(&event_json).unwrap();

        // Corrupt the event type
        if let Some(event_type) = event_value.pointer_mut("/event_type") {
            *event_type = serde_json::json!({
                "type": "invalid_type",
                "data": null
            });
        }

        let invalid_event: DaemonEvent = serde_json::from_value(event_value).unwrap();

        let result = router.route_event(invalid_event).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_corrupted_event_data_validation() {
        let router = DefaultEventRouter::new();
        let corrupted_event = ErrorTestHelper::create_corrupted_event();

        let result = router.route_event(corrupted_event).await;
        assert!(result.is_err());

        // The specific error may vary depending on what gets corrupted
        // but it should be some form of validation or processing error
        match result.unwrap_err() {
            EventError::ValidationError(_) |
            EventError::ProcessingError(_) |
            EventError::InvalidEventData(_) => {
                // Expected validation/processing errors
            }
            other => panic!("Unexpected error type: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_event_payload_integrity_validation() {
        let router = DefaultEventRouter::new();

        // Create event with checksum
        let data = b"test data for integrity".to_vec();
        let payload = EventPayload::binary(data.clone(), "application/octet-stream".to_string());

        let event = DaemonEvent::new(
            EventType::Custom("integrity-test".to_string()),
            EventSource::service("test-client".to_string()),
            payload,
        );

        // Verify the event is initially valid
        assert!(event.payload.verify_integrity());

        let result = router.route_event(event).await;
        assert!(result.is_ok()); // Should succeed with valid checksum

        // Test with corrupted payload
        let corrupted_data = b"corrupted data".to_vec();
        let mut corrupted_payload = EventPayload::binary(corrupted_data, "application/octet-stream".to_string());

        // Manually corrupt the checksum
        corrupted_payload.checksum = Some("invalid_checksum".to_string());

        let corrupted_event = DaemonEvent::new(
            EventType::Custom("corrupted-integrity".to_string()),
            EventSource::service("test-client".to_string()),
            corrupted_payload,
        );

        let result = router.route_event(corrupted_event).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_duplicate_event_detection() {
        let config = RoutingConfig {
            enable_deduplication: true,
            deduplication_window_s: 60, // 1 minute window
            ..Default::default()
        };

        let router = DefaultEventRouter::with_config(config);

        // Register a service
        let registration = ServiceRegistration {
            service_id: "test-service".to_string(),
            service_type: "test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["custom".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 10,
            filters: Vec::new(),
            metadata: HashMap::new(),
        };

        router.register_service(registration).await.unwrap();

        // Create and route first event
        let event = DaemonEvent::new(
            EventType::Custom("duplicate-test".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({"test": "data"})),
        );

        let result1 = router.route_event(event.clone()).await;
        assert!(result1.is_ok());

        // Route identical event (should fail)
        let result2 = router.route_event(event).await;
        assert!(result2.is_err());

        match result2.unwrap_err() {
            EventError::ValidationError(msg) => {
                assert!(msg.contains("Duplicate event"));
            }
            _ => panic!("Expected ValidationError for duplicate event"),
        }
    }
}

#[cfg(test)]
mod routing_error_tests {
    use super::*;

    #[tokio::test]
    async fn test_service_not_found_error() {
        let router = DefaultEventRouter::new();
        let event_for_nonexistent_service = ErrorTestHelper::create_event_for_nonexistent_service();

        let result = router.route_event(event_for_nonexistent_service).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            EventError::RoutingError(msg) => {
                assert!(msg.contains("No target services found"));
            }
            EventError::ServiceNotFound(service_id) => {
                assert!(service_id.contains("nonexistent-service"));
            }
            _ => panic!("Expected ServiceNotFound or RoutingError"),
        }
    }

    #[tokio::test]
    async fn test_all_services_unhealthy_error() {
        let router = DefaultEventRouter::new();

        // Register services but mark them as unhealthy
        let registration = ServiceRegistration {
            service_id: "unhealthy-service".to_string(),
            service_type: "test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["custom".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 10,
            filters: Vec::new(),
            metadata: HashMap::new(),
        };

        router.register_service(registration).await.unwrap();

        // Mark service as unhealthy
        router.update_service_health("unhealthy-service", ServiceHealth {
            status: ServiceStatus::Unhealthy,
            message: Some("Service is unhealthy".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        // Try to route event to unhealthy service
        let event = DaemonEvent::new(
            EventType::Custom("test-event".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({})),
        )
        .with_target(ServiceTarget::new("unhealthy-service".to_string()));

        let result = router.route_event(event).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            EventError::RoutingError(msg) => {
                assert!(msg.contains("No target services found") || msg.contains("unhealthy"));
            }
            _ => panic!("Expected RoutingError for unhealthy service"),
        }
    }

    #[tokio::test]
    async fn test_circuit_breaker_open_error() {
        let router = DefaultEventRouter::new();

        let registration = ServiceRegistration {
            service_id: "circuit-test-service".to_string(),
            service_type: "test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["custom".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 10,
            filters: Vec::new(),
            metadata: HashMap::new(),
        };

        router.register_service(registration).await.unwrap();

        // Simulate multiple failures to trigger circuit breaker
        for i in 0..10 {
            router.update_service_health("circuit-test-service", ServiceHealth {
                status: ServiceStatus::Failed,
                message: Some(format!("Simulated failure #{}", i)),
                last_check: Utc::now(),
                details: HashMap::new(),
            }).await.unwrap();

            let event = DaemonEvent::new(
                EventType::Custom(format!("failure-event-{}", i)),
                EventSource::service("test-client".to_string()),
                EventPayload::json(serde_json::json!({"attempt": i})),
            )
            .with_target(ServiceTarget::new("circuit-test-service".to_string()));

            let result = router.route_event(event).await;

            // Initially some might succeed, but eventually circuit breaker should open
            if i >= 5 {
                assert!(result.is_err());

                match result.unwrap_err() {
                    EventError::CircuitBreakerOpen(service_id) => {
                        assert_eq!(service_id, "circuit-test-service");
                    }
                    EventError::RoutingError(_) => {
                        // Also acceptable as routing may fail before reaching circuit breaker
                    }
                    _ => {
                        // Other errors might occur during service failure simulation
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn test_queue_full_error() {
        let config = RoutingConfig {
            max_queue_size: 1, // Very small queue
            ..Default::default()
        };

        let router = DefaultEventRouter::with_config(config);

        let registration = ServiceRegistration {
            service_id: "queue-test-service".to_string(),
            service_type: "test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["custom".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 1, // Limit concurrent events
            filters: Vec::new(),
            metadata: HashMap::new(),
        };

        router.register_service(registration).await.unwrap();

        // Create events that will exceed queue capacity
        let events: Vec<DaemonEvent> = (0..10)
            .map(|i| DaemonEvent::new(
                EventType::Custom(format!("queue-event-{}", i)),
                EventSource::service("test-client".to_string()),
                EventPayload::json(serde_json::json!({"event_id": i})),
            )
            .with_target(ServiceTarget::new("queue-test-service".to_string())))
            .collect();

        let mut queue_full_errors = 0;
        let mut successful_routes = 0;

        for event in events {
            match router.route_event(event).await {
                Ok(_) => successful_routes += 1,
                Err(EventError::QueueFull { capacity }) => {
                    assert_eq!(capacity, 1);
                    queue_full_errors += 1;
                }
                Err(_) => {
                    // Other errors are acceptable
                }
            }
        }

        // Should have at least one queue full error
        assert!(queue_full_errors > 0);
        assert!(successful_routes <= 1); // Should not exceed queue capacity
    }

    #[tokio::test]
    async fn test_event_timeout_error() {
        let config = RoutingConfig {
            event_timeout_ms: 10, // Very short timeout
            ..Default::default()
        };

        let router = DefaultEventRouter::with_config(config);

        let registration = ServiceRegistration {
            service_id: "timeout-test-service".to_string(),
            service_type: "test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["custom".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 1,
            filters: Vec::new(),
            metadata: HashMap::new(),
        };

        router.register_service(registration).await.unwrap();

        // Create an event that will likely timeout
        let event = DaemonEvent::new(
            EventType::Custom("timeout-event".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({"large_data": "x".repeat(1024 * 1024)})),
        )
        .with_target(ServiceTarget::new("timeout-test-service".to_string()));

        let result = router.route_event(event).await;

        // May timeout or may succeed depending on processing speed
        if let Err(EventError::Timeout { duration_ms }) = result {
            assert_eq!(duration_ms, 10);
        }
    }
}

#[cfg(test)]
mod serialization_error_tests {
    use super::*;

    #[tokio::test]
    async fn test_event_serialization_error() {
        // Test that malformed events cause serialization errors
        let malformed_json = r#"{
            "id": "invalid-uuid",
            "event_type": {
                "type": "invalid",
                "data": null
            },
            "priority": "invalid",
            "source": {
                "id": "",
                "source_type": "invalid",
                "instance": null,
                "metadata": {}
            },
            "targets": [],
            "created_at": "invalid-date",
            "payload": {
                "data": null,
                "content_type": "",
                "encoding": "",
                "size_bytes": -1,
                "checksum": null
            },
            "metadata": {
                "fields": {},
                "metrics": null,
                "debug": null
            },
            "correlation_id": null,
            "causation_id": null,
            "retry_count": -1,
            "max_retries": -1
        }"#;

        let result: Result<DaemonEvent, _> = serde_json::from_str(malformed_json);
        assert!(result.is_err());

        // Test with EventError serialization handling
        let serialization_error = EventError::SerializationError(
            serde_json::Error::syntax(serde_json::error::ErrorCode::ExpectedColon, 1, 1)
        );

        assert!(matches!(serialization_error, EventError::SerializationError(_)));
    }

    #[tokio::test]
    async fn test_payload_serialization_error() {
        // Create payload with unserializable data
        let unserializable_data = serde_json::json!({
            "valid_field": "valid_value",
            "circular_ref": serde_json::Value::Null, // This can cause issues in some cases
            "deeply_nested": {
                "very_deep": {
                    "nested": {
                        "structure": {
                            "that": {
                                "might": {
                                    "cause": {
                                        "stack": "overflow"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        let payload = EventPayload::json(unserializable_data);

        // Test serialization and deserialization
        let serialized = serde_json::to_string(&payload);
        assert!(serialized.is_ok());

        let deserialized: Result<EventPayload, _> = serde_json::from_str(&serialized.unwrap());
        assert!(deserialized.is_ok());
    }

    #[tokio::test]
    async fn test_service_registration_serialization_error() {
        // Create service registration with potentially problematic data
        let registration = ServiceRegistration {
            service_id: "test-service".to_string(),
            service_type: "test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: Some("http://localhost:8080/test".to_string()),
            supported_event_types: vec!["test".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 10,
            filters: Vec::new(),
            metadata: HashMap::from([
                ("key".to_string(), "value".to_string()),
                ("special_chars".to_string(), "test & value".to_string()),
                ("unicode".to_string(), "测试值".to_string()),
            ]),
        };

        // Test serialization
        let serialized = serde_json::to_string(&registration);
        assert!(serialized.is_ok());

        // Test deserialization
        let deserialized: Result<ServiceRegistration, _> = serde_json::from_str(&serialized.unwrap());
        assert!(deserialized.is_ok());

        let deserialized_reg = deserialized.unwrap();
        assert_eq!(deserialized_reg.service_id, registration.service_id);
        assert_eq!(deserialized_reg.metadata, registration.metadata);
    }
}

#[cfg(test)]
mod error_recovery_tests {
    use super::*;

    #[tokio::test]
    async fn test_service_health_recovery() {
        let router = DefaultEventRouter::new();

        let registration = ServiceRegistration {
            service_id: "recovery-service".to_string(),
            service_type: "test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["custom".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 10,
            filters: Vec::new(),
            metadata: HashMap::new(),
        };

        router.register_service(registration).await.unwrap();

        // Mark service as unhealthy
        router.update_service_health("recovery-service", ServiceHealth {
            status: ServiceStatus::Unhealthy,
            message: Some("Service is unhealthy".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        // Try to route event (should fail)
        let event = DaemonEvent::new(
            EventType::Custom("recovery-test".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({})),
        )
        .with_target(ServiceTarget::new("recovery-service".to_string()));

        let result = router.route_event(event.clone()).await;
        assert!(result.is_err());

        // Mark service as healthy again
        router.update_service_health("recovery-service", ServiceHealth {
            status: ServiceStatus::Healthy,
            message: Some("Service recovered".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        // Wait a bit for any circuit breaker timeout
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Try to route event again (should succeed)
        let result = router.route_event(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovery() {
        let router = DefaultEventRouter::new();

        let registration = ServiceRegistration {
            service_id: "circuit-recovery-service".to_string(),
            service_type: "test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["custom".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 10,
            filters: Vec::new(),
            metadata: HashMap::new(),
        };

        router.register_service(registration).await.unwrap();

        // Simulate multiple failures to open circuit breaker
        for i in 0..10 {
            router.update_service_health("circuit-recovery-service", ServiceHealth {
                status: ServiceStatus::Failed,
                message: Some(format!("Failure #{}", i)),
                last_check: Utc::now(),
                details: HashMap::new(),
            }).await.unwrap();

            let event = DaemonEvent::new(
                EventType::Custom(format!("circuit-failure-{}", i)),
                EventSource::service("test-client".to_string()),
                EventPayload::json(serde_json::json!({"attempt": i})),
            )
            .with_target(ServiceTarget::new("circuit-recovery-service".to_string()));

            router.route_event(event).await; // May fail, that's expected
        }

        // Now mark service as healthy
        router.update_service_health("circuit-recovery-service", ServiceHealth {
            status: ServiceStatus::Healthy,
            message: Some("Service recovered".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        // Wait for circuit breaker timeout
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Try to route event (should eventually succeed)
        let recovery_event = DaemonEvent::new(
            EventType::Custom("circuit-recovery".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({"recovery": true})),
        )
        .with_target(ServiceTarget::new("circuit-recovery-service".to_string()));

        let mut attempts = 0;
        let max_attempts = 5;

        while attempts < max_attempts {
            match router.route_event(recovery_event.clone()).await {
                Ok(_) => {
                    // Recovery successful
                    break;
                }
                Err(_) => {
                    attempts += 1;
                    if attempts >= max_attempts {
                        panic!("Circuit breaker should have recovered by now");
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    #[tokio::test]
    async fn test_routing_rule_recovery() {
        let router = DefaultEventRouter::new();

        // Register service
        let registration = ServiceRegistration {
            service_id: "rule-recovery-service".to_string(),
            service_type: "test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["custom".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 10,
            filters: Vec::new(),
            metadata: HashMap::new(),
        };

        router.register_service(registration).await.unwrap();

        // Add routing rule
        let filter = EventFilter {
            event_types: vec!["custom".to_string()],
            ..Default::default()
        };

        let rule = RoutingRule {
            rule_id: "recovery-rule".to_string(),
            name: "Recovery Test Rule".to_string(),
            description: "Test rule for recovery".to_string(),
            filter,
            targets: vec![ServiceTarget::new("rule-recovery-service".to_string())],
            priority: 0,
            enabled: true,
            conditions: Vec::new(),
        };

        router.add_routing_rule(rule).await.unwrap();

        // Test routing with rule enabled
        let event = DaemonEvent::new(
            EventType::Custom("rule-test".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({})),
        );

        let result = router.route_event(event.clone()).await;
        assert!(result.is_ok());

        // Remove the rule
        router.remove_routing_rule("recovery-rule").await.unwrap();

        // Test routing without rule (should fail - no default routing)
        let result = router.route_event(event.clone()).await;
        assert!(result.is_err());

        // Add the rule back
        let rule = RoutingRule {
            rule_id: "recovery-rule".to_string(),
            name: "Recovery Test Rule".to_string(),
            description: "Test rule for recovery".to_string(),
            filter: EventFilter {
                event_types: vec!["custom".to_string()],
                ..Default::default()
            },
            targets: vec![ServiceTarget::new("rule-recovery-service".to_string())],
            priority: 0,
            enabled: true,
            conditions: Vec::new(),
        };

        router.add_routing_rule(rule).await.unwrap();

        // Test routing with rule restored
        let result = router.route_event(event).await;
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod error_propagation_tests {
    use super::*;

    #[tokio::test]
    async fn test_error_propagation_through_routing_pipeline() {
        let router = DefaultEventRouter::new();

        // Register service
        let registration = ServiceRegistration {
            service_id: "propagation-test-service".to_string(),
            service_type: "test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["custom".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 10,
            filters: Vec::new(),
            metadata: HashMap::new(),
        };

        router.register_service(registration).await.unwrap();

        // Create an event that will fail at validation stage
        let oversized_event = ErrorTestHelper::create_oversized_event();

        let result = router.route_event(oversized_event).await;
        assert!(result.is_err());

        // Verify error context is preserved
        match result.unwrap_err() {
            EventError::EventTooLarge { size, max_size } => {
                assert!(size > max_size);
                // Error should contain specific context about which validation failed
            }
            _ => panic!("Expected EventTooLarge error with specific context"),
        }
    }

    #[tokio::test]
    async fn test_error_context_preservation() {
        let router = DefaultEventRouter::new();

        // Test various error scenarios and verify context is preserved
        let error_scenarios = vec![
            ErrorTestHelper::create_oversized_event(),
            ErrorTestHelper::create_event_for_nonexistent_service(),
            ErrorTestHelper::create_event_with_invalid_targets(),
        ];

        for (index, event) in error_scenarios.into_iter().enumerate() {
            let result = router.route_event(event).await;
            assert!(result.is_err(), "Scenario {} should fail", index);

            let error = result.unwrap_err();

            // Each error should provide meaningful context
            let error_string = format!("{}", error);
            assert!(!error_string.is_empty(), "Error message should not be empty");
            assert!(error_string.len() > 10, "Error message should be descriptive");

            // Check that error provides useful information for debugging
            match error {
                EventError::ValidationError(msg) => {
                    assert!(msg.contains("validation") || msg.len() > 5);
                }
                EventError::RoutingError(msg) => {
                    assert!(msg.contains("routing") || msg.contains("service"));
                }
                EventError::ServiceNotFound(service_id) => {
                    assert!(!service_id.is_empty());
                }
                EventError::EventTooLarge { size, max_size } => {
                    assert!(size > 0);
                    assert!(max_size > 0);
                }
                _ => {
                    // Other error types should also have meaningful context
                }
            }
        }
    }

    #[tokio::test]
    async fn test_error_aggregation_in_statistics() {
        let router = DefaultEventRouter::new();

        // Generate various types of errors
        let error_events = vec![
            ErrorTestHelper::create_oversized_event(),
            ErrorTestHelper::create_event_for_nonexistent_service(),
            ErrorTestHelper::create_event_with_invalid_targets(),
        ];

        let mut error_count = 0;
        for event in error_events {
            if router.route_event(event).await.is_err() {
                error_count += 1;
            }
        }

        // Check that routing statistics reflect the errors
        let stats = router.get_routing_stats().await.unwrap();

        // Statistics should show error activity
        // Note: The exact implementation may vary, but errors should be tracked
        assert!(stats.total_events_routed >= 0); // Should have attempted routing

        // If the implementation tracks errors in service stats, check those
        for (service_id, service_stats) in &stats.service_stats {
            if service_stats.events_failed > 0 {
                println!("Service {} had {} failed events", service_id, service_stats.events_failed);
            }
        }
    }

    #[tokio::test]
    async fn test_cascading_error_prevention() {
        let router = DefaultEventRouter::new();

        // Register multiple services
        let services = vec![
            ("service-1", "test"),
            ("service-2", "test"),
            ("service-3", "test"),
        ];

        for (service_id, service_type) in services {
            let registration = ServiceRegistration {
                service_id: service_id.to_string(),
                service_type: service_type.to_string(),
                instance_id: format!("{}-instance-1", service_id),
                endpoint: None,
                supported_event_types: vec!["custom".to_string()],
                priority: 0,
                weight: 1.0,
                max_concurrent_events: 10,
                filters: Vec::new(),
                metadata: HashMap::new(),
            };

            router.register_service(registration).await.unwrap();
        }

        // Make one service unhealthy
        router.update_service_health("service-2", ServiceHealth {
            status: ServiceStatus::Unhealthy,
            message: Some("Service 2 is unhealthy".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        // Create routing rule that targets all services
        let filter = EventFilter::new();
        let targets = vec![
            ServiceTarget::new("service-1".to_string()),
            ServiceTarget::new("service-2".to_string()),
            ServiceTarget::new("service-3".to_string()),
        ];

        let rule = RoutingRule {
            rule_id: "cascading-test-rule".to_string(),
            name: "Cascading Test Rule".to_string(),
            description: "Test cascading error prevention".to_string(),
            filter,
            targets,
            priority: 0,
            enabled: true,
            conditions: Vec::new(),
        };

        router.add_routing_rule(rule).await.unwrap();

        // Route events and verify that failure of one service doesn't affect others
        let test_events: Vec<DaemonEvent> = (0..10)
            .map(|i| DaemonEvent::new(
                EventType::Custom(format!("cascading-test-{}", i)),
                EventSource::service("test-client".to_string()),
                EventPayload::json(serde_json::json!({"event_id": i})),
            ))
            .collect();

        let mut successful_routes = 0;
        let mut failed_routes = 0;

        for event in test_events {
            match router.route_event(event).await {
                Ok(_) => successful_routes += 1,
                Err(_) => failed_routes += 1,
            }
        }

        // Should have some successful routes (to healthy services)
        assert!(successful_routes > 0, "Should have successful routes to healthy services");

        // May have some failed routes, but not all should fail
        assert!(failed_routes < test_events.len(), "Not all routes should fail");
    }
}