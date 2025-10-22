//! Integration tests for event routing flow through the system

use crucible_services::events::core::*;
use crucible_services::events::routing::*;
use crucible_services::events::errors::{EventError, EventResult};
use crucible_services::types::{ServiceHealth, ServiceStatus};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

/// Mock event handler for testing event delivery
struct MockEventHandler {
    service_id: String,
    events_received: Arc<RwLock<Vec<DaemonEvent>>>,
    should_fail: Arc<RwLock<bool>>,
    delay_ms: u64,
}

impl MockEventHandler {
    fn new(service_id: String, should_fail: bool, delay_ms: u64) -> Self {
        Self {
            service_id,
            events_received: Arc::new(RwLock::new(Vec::new())),
            should_fail: Arc::new(RwLock::new(should_fail)),
            delay_ms,
        }
    }

    async fn handle_event(&self, event: DaemonEvent) -> EventResult<()> {
        // Simulate processing delay
        if self.delay_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Check if should fail
        if *self.should_fail.read().await {
            return Err(EventError::delivery_error(
                self.service_id.clone(),
                "Mock handler configured to fail".to_string(),
            ));
        }

        // Store the event
        let mut events = self.events_received.write().await;
        events.push(event);

        Ok(())
    }

    async fn get_events(&self) -> Vec<DaemonEvent> {
        self.events_received.read().await.clone()
    }

    async fn set_should_fail(&self, should_fail: bool) {
        *self.should_fail.write().await = should_fail;
    }

    async fn clear_events(&self) {
        self.events_received.write().await.clear();
    }
}

/// Test utility to create a basic service registration
fn create_test_service_registration(
    service_id: &str,
    service_type: &str,
    supported_events: Vec<String>,
) -> ServiceRegistration {
    ServiceRegistration {
        service_id: service_id.to_string(),
        service_type: service_type.to_string(),
        instance_id: format!("{}-instance-1", service_id),
        endpoint: Some(format!("http://localhost:8080/{}", service_id)),
        supported_event_types: supported_events,
        priority: 0,
        weight: 1.0,
        max_concurrent_events: 10,
        filters: Vec::new(),
        metadata: HashMap::new(),
    }
}

/// Test utility to create a basic routing rule
fn create_test_routing_rule(
    rule_id: &str,
    filter: EventFilter,
    targets: Vec<ServiceTarget>,
) -> RoutingRule {
    RoutingRule {
        rule_id: rule_id.to_string(),
        name: format!("Test Rule {}", rule_id),
        description: "Test routing rule".to_string(),
        filter,
        targets,
        priority: 0,
        enabled: true,
        conditions: Vec::new(),
    }
}

/// Test utility to create a test event
fn create_test_event_with_target(
    event_type: EventType,
    source: EventSource,
    target_service: &str,
) -> DaemonEvent {
    DaemonEvent::new(
        event_type,
        source,
        EventPayload::json(serde_json::json!({"test": "data"})),
    )
    .with_target(ServiceTarget::new(target_service.to_string()))
}

#[cfg(test)]
mod basic_routing_tests {
    use super::*;

    #[tokio::test]
    async fn test_service_registration_and_deregistration() {
        let router = DefaultEventRouter::new();

        // Register a service
        let registration = create_test_service_registration(
            "test-service",
            "test",
            vec!["filesystem".to_string(), "database".to_string()],
        );

        router.register_service(registration).await.unwrap();

        // Verify service is registered by testing routing
        let event = create_test_event_with_target(
            EventType::Filesystem(FilesystemEventType::FileCreated {
                path: "/test/file.txt".to_string(),
            }),
            EventSource::service("test-client".to_string()),
            "test-service",
        );

        let targets = router.test_routing(&event).await.unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0], "test-service");

        // Unregister service
        router.unregister_service("test-service").await.unwrap();

        // Verify service is no longer available
        let targets = router.test_routing(&event).await.unwrap();
        assert_eq!(targets.len(), 0);
    }

    #[tokio::test]
    async fn test_service_health_updates() {
        let router = DefaultEventRouter::new();

        let registration = create_test_service_registration(
            "health-test-service",
            "test",
            vec!["system".to_string()],
        );

        router.register_service(registration).await.unwrap();

        // Test with healthy service
        let healthy_status = ServiceHealth {
            status: ServiceStatus::Healthy,
            message: Some("Service is healthy".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        };

        router.update_service_health("health-test-service", healthy_status).await.unwrap();

        let event = create_test_event_with_target(
            EventType::System(SystemEventType::DaemonStarted {
                version: "1.0.0".to_string(),
            }),
            EventSource::system("daemon".to_string()),
            "health-test-service",
        );

        let targets = router.test_routing(&event).await.unwrap();
        assert_eq!(targets.len(), 1);

        // Test with unhealthy service
        let unhealthy_status = ServiceHealth {
            status: ServiceStatus::Unhealthy,
            message: Some("Service is unhealthy".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        };

        router.update_service_health("health-test-service", unhealthy_status).await.unwrap();

        let targets = router.test_routing(&event).await.unwrap();
        assert_eq!(targets.len(), 0); // Unhealthy service should not be targeted
    }

    #[tokio::test]
    async fn test_basic_event_routing() {
        let router = DefaultEventRouter::new();

        // Register multiple services
        let fs_service = create_test_service_registration(
            "fs-processor",
            "filesystem",
            vec!["filesystem".to_string()],
        );

        let db_service = create_test_service_registration(
            "db-processor",
            "database",
            vec!["database".to_string()],
        );

        router.register_service(fs_service).await.unwrap();
        router.register_service(db_service).await.unwrap();

        // Test filesystem event routing
        let fs_event = create_test_event_with_target(
            EventType::Filesystem(FilesystemEventType::FileCreated {
                path: "/test/file.txt".to_string(),
            }),
            EventSource::filesystem("watcher-1".to_string()),
            "fs-processor",
        );

        let result = router.route_event(fs_event).await;
        assert!(result.is_ok());

        // Test database event routing
        let db_event = create_test_event_with_target(
            EventType::Database(DatabaseEventType::RecordCreated {
                table: "users".to_string(),
                id: "123".to_string(),
            }),
            EventSource::database("db-trigger".to_string()),
            "db-processor",
        );

        let result = router.route_event(db_event).await;
        assert!(result.is_ok());

        // Check routing statistics
        let stats = router.get_routing_stats().await.unwrap();
        assert_eq!(stats.total_events_routed, 2);
    }

    #[tokio::test]
    async fn test_broadcast_event_routing() {
        let router = DefaultEventRouter::new();

        // Register multiple services
        let services = vec![
            create_test_service_registration("service-1", "test", vec!["system".to_string()]),
            create_test_service_registration("service-2", "test", vec!["system".to_string()]),
            create_test_service_registration("service-3", "test", vec!["system".to_string()]),
        ];

        for service in services {
            router.register_service(service).await.unwrap();
        }

        // Create a system event (broadcast-allowed) without specific targets
        let system_event = DaemonEvent::new(
            EventType::System(SystemEventType::DaemonStarted {
                version: "1.0.0".to_string(),
            }),
            EventSource::system("daemon".to_string()),
            EventPayload::json(serde_json::json!({"startup": "complete"})),
        );

        let result = router.route_event(system_event).await;
        assert!(result.is_ok());

        // Check that all services were targeted
        let stats = router.get_routing_stats().await.unwrap();
        assert_eq!(stats.total_events_routed, 1);
        // Note: In a real implementation, we'd check that all services received the event
    }
}

#[cfg(test)]
mod routing_rule_tests {
    use super::*;

    #[tokio::test]
    async fn test_routing_rule_creation_and_application() {
        let router = DefaultEventRouter::new();

        // Register services
        let service1 = create_test_service_registration(
            "service-1",
            "test",
            vec!["filesystem".to_string()],
        );

        let service2 = create_test_service_registration(
            "service-2",
            "test",
            vec!["database".to_string()],
        );

        router.register_service(service1).await.unwrap();
        router.register_service(service2).await.unwrap();

        // Create routing rule for filesystem events
        let fs_filter = EventFilter {
            event_types: vec!["filesystem".to_string()],
            ..Default::default()
        };

        let fs_rule = create_test_routing_rule(
            "fs-rule",
            fs_filter,
            vec![ServiceTarget::new("service-1".to_string())],
        );

        router.add_routing_rule(fs_rule).await.unwrap();

        // Test filesystem event routing (should match rule)
        let fs_event = DaemonEvent::new(
            EventType::Filesystem(FilesystemEventType::FileModified {
                path: "/test/file.txt".to_string(),
            }),
            EventSource::filesystem("watcher".to_string()),
            EventPayload::json(serde_json::json!({"change": "modified"})),
        );

        let targets = router.test_routing(&fs_event).await.unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0], "service-1");

        // Test database event routing (should not match rule)
        let db_event = DaemonEvent::new(
            EventType::Database(DatabaseEventType::RecordUpdated {
                table: "users".to_string(),
                id: "123".to_string(),
                changes: HashMap::new(),
            }),
            EventSource::database("trigger".to_string()),
            EventPayload::json(serde_json::json!({"change": "update"})),
        );

        let targets = router.test_routing(&db_event).await.unwrap();
        assert_eq!(targets.len(), 0); // No matching rule or default routing
    }

    #[tokio::test]
    async fn test_routing_rule_priority_and_order() {
        let router = DefaultEventRouter::new();

        // Register services
        router.register_service(create_test_service_registration(
            "high-priority-service",
            "test",
            vec!["test".to_string()],
        )).await.unwrap();

        router.register_service(create_test_service_registration(
            "low-priority-service",
            "test",
            vec!["test".to_string()],
        )).await.unwrap();

        // Create high priority rule
        let high_priority_filter = EventFilter::new();
        let high_priority_rule = RoutingRule {
            rule_id: "high-priority".to_string(),
            name: "High Priority Rule".to_string(),
            description: "High priority routing rule".to_string(),
            filter: high_priority_filter,
            targets: vec![ServiceTarget::new("high-priority-service".to_string())],
            priority: 10, // Higher priority
            enabled: true,
            conditions: Vec::new(),
        };

        // Create low priority rule
        let low_priority_filter = EventFilter::new();
        let low_priority_rule = RoutingRule {
            rule_id: "low-priority".to_string(),
            name: "Low Priority Rule".to_string(),
            description: "Low priority routing rule".to_string(),
            filter: low_priority_filter,
            targets: vec![ServiceTarget::new("low-priority-service".to_string())],
            priority: 1, // Lower priority
            enabled: true,
            conditions: Vec::new(),
        };

        router.add_routing_rule(low_priority_rule).await.unwrap();
        router.add_routing_rule(high_priority_rule).await.unwrap();

        // Test event routing (should match high priority rule first)
        let test_event = DaemonEvent::new(
            EventType::Custom("test-event".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({"test": true})),
        );

        let targets = router.test_routing(&test_event).await.unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0], "high-priority-service");
    }

    #[tokio::test]
    async fn test_routing_rule_enable_disable() {
        let router = DefaultEventRouter::new();

        router.register_service(create_test_service_registration(
            "target-service",
            "test",
            vec!["test".to_string()],
        )).await.unwrap();

        // Create disabled rule
        let disabled_rule = RoutingRule {
            rule_id: "disabled-rule".to_string(),
            name: "Disabled Rule".to_string(),
            description: "A disabled routing rule".to_string(),
            filter: EventFilter::new(),
            targets: vec![ServiceTarget::new("target-service".to_string())],
            priority: 0,
            enabled: false, // Disabled
            conditions: Vec::new(),
        };

        router.add_routing_rule(disabled_rule).await.unwrap();

        // Test event routing (should not match disabled rule)
        let test_event = DaemonEvent::new(
            EventType::Custom("test-event".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({})),
        );

        let targets = router.test_routing(&test_event).await.unwrap();
        assert_eq!(targets.len(), 0);

        // Enable the rule by adding a new enabled version
        let enabled_rule = RoutingRule {
            rule_id: "enabled-rule".to_string(),
            name: "Enabled Rule".to_string(),
            description: "An enabled routing rule".to_string(),
            filter: EventFilter::new(),
            targets: vec![ServiceTarget::new("target-service".to_string())],
            priority: 0,
            enabled: true, // Enabled
            conditions: Vec::new(),
        };

        router.add_routing_rule(enabled_rule).await.unwrap();

        // Test event routing (should now match enabled rule)
        let targets = router.test_routing(&test_event).await.unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0], "target-service");
    }

    #[tokio::test]
    async fn test_routing_rule_removal() {
        let router = DefaultEventRouter::new();

        router.register_service(create_test_service_registration(
            "target-service",
            "test",
            vec!["test".to_string()],
        )).await.unwrap();

        // Add routing rule
        let rule = create_test_routing_rule(
            "removable-rule",
            EventFilter::new(),
            vec![ServiceTarget::new("target-service".to_string())],
        );

        router.add_routing_rule(rule).await.unwrap();

        // Verify rule works
        let test_event = DaemonEvent::new(
            EventType::Custom("test-event".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({})),
        );

        let targets = router.test_routing(&test_event).await.unwrap();
        assert_eq!(targets.len(), 1);

        // Remove the rule
        router.remove_routing_rule("removable-rule").await.unwrap();

        // Verify rule no longer works
        let targets = router.test_routing(&test_event).await.unwrap();
        assert_eq!(targets.len(), 0);
    }
}

#[cfg(test)]
mod load_balancing_tests {
    use super::*;

    #[tokio::test]
    async fn test_round_robin_load_balancing() {
        let config = RoutingConfig {
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
            ..Default::default()
        };

        let router = DefaultEventRouter::with_config(config);

        // Register multiple services with same service type
        let services = vec![
            create_test_service_registration("service-1", "test", vec!["test".to_string()]),
            create_test_service_registration("service-2", "test", vec!["test".to_string()]),
            create_test_service_registration("service-3", "test", vec!["test".to_string()]),
        ];

        for service in services {
            router.register_service(service).await.unwrap();
        }

        // Create routing rule that targets all services
        let filter = EventFilter::new();
        let targets = vec![
            ServiceTarget::new("service-1".to_string()),
            ServiceTarget::new("service-2".to_string()),
            ServiceTarget::new("service-3".to_string()),
        ];

        let rule = create_test_routing_rule("round-robin-rule", filter, targets);
        router.add_routing_rule(rule).await.unwrap();

        // Route multiple events and verify round-robin distribution
        let mut service_counts = HashMap::new();

        for i in 0..9 { // 9 events for 3 services (3 each expected)
            let event = DaemonEvent::new(
                EventType::Custom(format!("test-event-{}", i)),
                EventSource::service("test-client".to_string()),
                EventPayload::json(serde_json::json!({"event_id": i})),
            );

            let targets = router.test_routing(&event).await.unwrap();
            assert_eq!(targets.len(), 1);

            let service_id = &targets[0];
            *service_counts.entry(service_id.clone()).or_insert(0) += 1;
        }

        // Verify even distribution (each service should get 3 events)
        assert_eq!(service_counts.get("service-1"), Some(&3));
        assert_eq!(service_counts.get("service-2"), Some(&3));
        assert_eq!(service_counts.get("service-3"), Some(&3));
    }

    #[tokio::test]
    async fn test_least_connections_load_balancing() {
        let config = RoutingConfig {
            load_balancing_strategy: LoadBalancingStrategy::LeastConnections,
            max_concurrent_events: 5,
            ..Default::default()
        };

        let router = DefaultEventRouter::with_config(config);

        // Register services
        router.register_service(create_test_service_registration(
            "service-1",
            "test",
            vec!["test".to_string()],
        )).await.unwrap();

        router.register_service(create_test_service_registration(
            "service-2",
            "test",
            vec!["test".to_string()],
        )).await.unwrap();

        // Create routing rule targeting both services
        let filter = EventFilter::new();
        let targets = vec![
            ServiceTarget::new("service-1".to_string()),
            ServiceTarget::new("service-2".to_string()),
        ];

        let rule = create_test_routing_rule("least-conn-rule", filter, targets);
        router.add_routing_rule(rule).await.unwrap();

        // Route events and verify they go to the service with least connections
        // In this simple test, it should alternate between services
        for i in 0..4 {
            let event = DaemonEvent::new(
                EventType::Custom(format!("test-event-{}", i)),
                EventSource::service("test-client".to_string()),
                EventPayload::json(serde_json::json!({"event_id": i})),
            );

            let targets = router.test_routing(&event).await.unwrap();
            assert_eq!(targets.len(), 1);
            assert!(targets[0] == "service-1" || targets[0] == "service-2");
        }
    }

    #[tokio::test]
    async fn test_weighted_random_load_balancing() {
        let config = RoutingConfig {
            load_balancing_strategy: LoadBalancingStrategy::WeightedRandom,
            ..Default::default()
        };

        let router = DefaultEventRouter::with_config(config);

        // Register services with different weights
        let mut service1 = create_test_service_registration(
            "light-service",
            "test",
            vec!["test".to_string()],
        );
        service1.weight = 0.2; // 20% weight

        let mut service2 = create_test_service_registration(
            "heavy-service",
            "test",
            vec!["test".to_string()],
        );
        service2.weight = 0.8; // 80% weight

        router.register_service(service1).await.unwrap();
        router.register_service(service2).await.unwrap();

        // Create routing rule
        let filter = EventFilter::new();
        let targets = vec![
            ServiceTarget::new("light-service".to_string()),
            ServiceTarget::new("heavy-service".to_string()),
        ];

        let rule = create_test_routing_rule("weighted-rule", filter, targets);
        router.add_routing_rule(rule).await.unwrap();

        // Route many events to test distribution
        let mut service_counts = HashMap::new();

        for i in 0..100 {
            let event = DaemonEvent::new(
                EventType::Custom(format!("test-event-{}", i)),
                EventSource::service("test-client".to_string()),
                EventPayload::json(serde_json::json!({"event_id": i})),
            );

            let targets = router.test_routing(&event).await.unwrap();
            assert_eq!(targets.len(), 1);

            let service_id = &targets[0];
            *service_counts.entry(service_id.clone()).or_insert(0) += 1;
        }

        // Verify weighted distribution (heavy-service should get more events)
        let light_count = service_counts.get("light-service").unwrap_or(&0);
        let heavy_count = service_counts.get("heavy-service").unwrap_or(&0);

        assert!(*heavy_count > *light_count);
        assert!(*light_count + *heavy_count == 100);

        // With 80/20 weights, we expect roughly 80/20 distribution
        let light_ratio = *light_count as f64 / 100.0;
        let heavy_ratio = *heavy_count as f64 / 100.0;

        // Allow some variance due to randomness
        assert!((light_ratio - 0.2).abs() < 0.2); // Within 20% of expected
        assert!((heavy_ratio - 0.8).abs() < 0.2); // Within 20% of expected
    }

    #[tokio::test]
    async fn test_priority_based_load_balancing() {
        let config = RoutingConfig {
            load_balancing_strategy: LoadBalancingStrategy::PriorityBased,
            ..Default::default()
        };

        let router = DefaultEventRouter::with_config(config);

        // Register services with different priorities
        let mut low_priority_service = create_test_service_registration(
            "low-priority",
            "test",
            vec!["test".to_string()],
        );
        low_priority_service.priority = 10; // Lower priority (higher number)

        let mut high_priority_service = create_test_service_registration(
            "high-priority",
            "test",
            vec!["test".to_string()],
        );
        high_priority_service.priority = 1; // Higher priority (lower number)

        router.register_service(low_priority_service).await.unwrap();
        router.register_service(high_priority_service).await.unwrap();

        // Create routing rule
        let filter = EventFilter::new();
        let targets = vec![
            ServiceTarget::new("low-priority".to_string()),
            ServiceTarget::new("high-priority".to_string()),
        ];

        let rule = create_test_routing_rule("priority-rule", filter, targets);
        router.add_routing_rule(rule).await.unwrap();

        // Test high priority event (should go to high priority service)
        let high_priority_event = DaemonEvent::new(
            EventType::Custom("urgent-event".to_string()),
            EventSource::service("critical-client".to_string()),
            EventPayload::json(serde_json::json!({"urgent": true})),
        )
        .with_priority(EventPriority::Critical);

        let targets = router.test_routing(&high_priority_event).await.unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0], "high-priority");

        // Test normal priority event (could go to either)
        let normal_event = DaemonEvent::new(
            EventType::Custom("normal-event".to_string()),
            EventSource::service("normal-client".to_string()),
            EventPayload::json(serde_json::json!({"urgent": false})),
        )
        .with_priority(EventPriority::Normal);

        let targets = router.test_routing(&normal_event).await.unwrap();
        assert_eq!(targets.len(), 1);
        assert!(targets[0] == "high-priority" || targets[0] == "low-priority");
    }

    #[tokio::test]
    async fn test_health_based_load_balancing() {
        let config = RoutingConfig {
            load_balancing_strategy: LoadBalancingStrategy::HealthBased,
            ..Default::default()
        };

        let router = DefaultEventRouter::with_config(config);

        // Register services
        router.register_service(create_test_service_registration(
            "healthy-service",
            "test",
            vec!["test".to_string()],
        )).await.unwrap();

        router.register_service(create_test_service_registration(
            "degraded-service",
            "test",
            vec!["test".to_string()],
        )).await.unwrap();

        router.register_service(create_test_service_registration(
            "unhealthy-service",
            "test",
            vec!["test".to_string()],
        )).await.unwrap();

        // Set different health statuses
        router.update_service_health("healthy-service", ServiceHealth {
            status: ServiceStatus::Healthy,
            message: None,
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        router.update_service_health("degraded-service", ServiceHealth {
            status: ServiceStatus::Degraded,
            message: None,
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        router.update_service_health("unhealthy-service", ServiceHealth {
            status: ServiceStatus::Unhealthy,
            message: None,
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        // Create routing rule
        let filter = EventFilter::new();
        let targets = vec![
            ServiceTarget::new("healthy-service".to_string()),
            ServiceTarget::new("degraded-service".to_string()),
            ServiceTarget::new("unhealthy-service".to_string()),
        ];

        let rule = create_test_routing_rule("health-rule", filter, targets);
        router.add_routing_rule(rule).await.unwrap();

        // Test routing (should prefer healthy services)
        let test_event = DaemonEvent::new(
            EventType::Custom("test-event".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({})),
        );

        let targets = router.test_routing(&test_event).await.unwrap();

        // Should only route to healthy service
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0], "healthy-service");

        // Make healthy service unhealthy and test again
        router.update_service_health("healthy-service", ServiceHealth {
            status: ServiceStatus::Unhealthy,
            message: None,
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        let targets = router.test_routing(&test_event).await.unwrap();

        // Should now route to degraded service
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0], "degraded-service");
    }
}

#[cfg(test)]
mod circuit_breaker_tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_failure_detection() {
        let router = DefaultEventRouter::new();

        // Register a service
        let registration = create_test_service_registration(
            "failing-service",
            "test",
            vec!["test".to_string()],
        );

        router.register_service(registration).await.unwrap();

        // Create routing rule
        let filter = EventFilter::new();
        let targets = vec![ServiceTarget::new("failing-service".to_string())];
        let rule = create_test_routing_rule("circuit-test-rule", filter, targets);
        router.add_routing_rule(rule).await.unwrap();

        // Simulate multiple failures by setting service health to unhealthy
        for i in 0..10 {
            router.update_service_health("failing-service", ServiceHealth {
                status: ServiceStatus::Unhealthy,
                message: Some(format!("Failure #{}", i + 1)),
                last_check: Utc::now(),
                details: HashMap::new(),
            }).await.unwrap();

            let event = DaemonEvent::new(
                EventType::Custom(format!("test-event-{}", i)),
                EventSource::service("test-client".to_string()),
                EventPayload::json(serde_json::json!({"attempt": i})),
            );

            // Initially events might be routed, but eventually circuit breaker should open
            let result = router.route_event(event).await;

            // After some failures, routing should fail
            if i >= 5 {
                assert!(result.is_err());
            }
        }
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovery() {
        let router = DefaultEventRouter::new();

        let registration = create_test_service_registration(
            "recovery-service",
            "test",
            vec!["test".to_string()],
        );

        router.register_service(registration).await.unwrap();

        // Create routing rule
        let filter = EventFilter::new();
        let targets = vec![ServiceTarget::new("recovery-service".to_string())];
        let rule = create_test_routing_rule("recovery-test-rule", filter, targets);
        router.add_routing_rule(rule).await.unwrap();

        // Simulate service failure
        router.update_service_health("recovery-service", ServiceHealth {
            status: ServiceStatus::Unhealthy,
            message: Some("Service failure".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        // Try to route event (should fail)
        let event = DaemonEvent::new(
            EventType::Custom("test-event".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({})),
        );

        let result = router.route_event(event.clone()).await;
        assert!(result.is_err());

        // Simulate service recovery
        router.update_service_health("recovery-service", ServiceHealth {
            status: ServiceStatus::Healthy,
            message: Some("Service recovered".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        // Wait a bit for circuit breaker timeout
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Try to route event again (should succeed)
        let result = router.route_event(event).await;
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[tokio::test]
    async fn test_event_validation_errors() {
        let router = DefaultEventRouter::new();

        // Test oversized event
        let large_payload = EventPayload::json(serde_json::json!({
            "data": "x".repeat(11 * 1024 * 1024) // 11MB (exceeds 10MB limit)
        }));

        let oversized_event = DaemonEvent::new(
            EventType::Custom("oversized-event".to_string()),
            EventSource::service("test-client".to_string()),
            large_payload,
        );

        let result = router.route_event(oversized_event).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), EventError::EventTooLarge { .. }));
    }

    #[tokio::test]
    async fn test_service_not_found_errors() {
        let router = DefaultEventRouter::new();

        // Try to route event to non-existent service
        let event = create_test_event_with_target(
            EventType::Custom("test-event".to_string()),
            EventSource::service("test-client".to_string()),
            "non-existent-service",
        );

        let result = router.route_event(event).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), EventError::RoutingError(_)));
    }

    #[tokio::test]
    async fn test_duplicate_event_detection() {
        let config = RoutingConfig {
            enable_deduplication: true,
            deduplication_window_s: 60, // 1 minute
            ..Default::default()
        };

        let router = DefaultEventRouter::with_config(config);

        router.register_service(create_test_service_registration(
            "dedup-test-service",
            "test",
            vec!["test".to_string()],
        )).await.unwrap();

        // Create identical events
        let event = DaemonEvent::new(
            EventType::Custom("duplicate-test".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({"test": true})),
        );

        // Route first event (should succeed)
        let result1 = router.route_event(event.clone()).await;
        assert!(result1.is_ok());

        // Route identical event (should fail as duplicate)
        let result2 = router.route_event(event).await;
        assert!(result2.is_err());
        assert!(matches!(result2.unwrap_err(), EventError::ValidationError(_)));
    }

    #[tokio::test]
    async fn test_invalid_priority_errors() {
        let router = DefaultEventRouter::new();

        // Create event with invalid priority (this would require manual construction)
        let mut event = DaemonEvent::new(
            EventType::Custom("invalid-priority-event".to_string()),
            EventSource::service("test-client".to_string()),
            EventPayload::json(serde_json::json!({})),
        );

        // Manually set invalid priority (this is a hack for testing)
        // In real code, this shouldn't be possible due to the type system
        let event_json = serde_json::to_string(&event).unwrap();
        let mut event_value: serde_json::Value = serde_json::from_str(&event_json).unwrap();

        // Modify the priority to an invalid value
        if let Some(priority) = event_value.pointer_mut("/priority") {
            *priority = serde_json::Value::String("invalid".to_string());
        }

        let invalid_event: DaemonEvent = serde_json::from_value(event_value).unwrap();

        let result = router.route_event(invalid_event).await;
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod performance_and_stats_tests {
    use super::*;

    #[tokio::test]
    async fn test_routing_statistics_collection() {
        let router = DefaultEventRouter::new();

        router.register_service(create_test_service_registration(
            "stats-service",
            "test",
            vec!["test".to_string()],
        )).await.unwrap();

        // Route multiple events
        for i in 0..10 {
            let event = DaemonEvent::new(
                EventType::Custom(format!("stats-event-{}", i)),
                EventSource::service("test-client".to_string()),
                EventPayload::json(serde_json::json!({"event_id": i})),
            );

            router.route_event(event).await.unwrap();
        }

        // Check statistics
        let stats = router.get_routing_stats().await.unwrap();
        assert_eq!(stats.total_events_routed, 10);
        assert!(stats.service_stats.contains_key("stats-service"));

        let service_stats = &stats.service_stats["stats-service"];
        assert_eq!(service_stats.events_received, 10);
        assert!(service_stats.last_event_processed.is_some());
    }

    #[tokio::test]
    async fn test_high_volume_event_routing() {
        let router = DefaultEventRouter::new();

        // Register multiple services for load distribution
        for i in 0..5 {
            let service = create_test_service_registration(
                &format!("service-{}", i),
                "test",
                vec!["test".to_string()],
            );
            router.register_service(service).await.unwrap();
        }

        // Create routing rule for load balancing
        let filter = EventFilter::new();
        let targets: Vec<ServiceTarget> = (0..5)
            .map(|i| ServiceTarget::new(format!("service-{}", i)))
            .collect();

        let rule = create_test_routing_rule("load-test-rule", filter, targets);
        router.add_routing_rule(rule).await.unwrap();

        let start_time = std::time::Instant::now();
        let event_count = 1000;

        // Route many events concurrently
        let mut handles = Vec::new();
        for i in 0..event_count {
            let router_clone = router.clone();
            let handle = tokio::spawn(async move {
                let event = DaemonEvent::new(
                    EventType::Custom(format!("load-test-event-{}", i)),
                    EventSource::service("load-test-client".to_string()),
                    EventPayload::json(serde_json::json!({"event_id": i})),
                );

                router_clone.route_event(event).await
            });
            handles.push(handle);
        }

        // Wait for all events to be routed
        let mut successes = 0;
        let mut failures = 0;

        for handle in handles {
            match handle.await.unwrap() {
                Ok(_) => successes += 1,
                Err(_) => failures += 1,
            }
        }

        let duration = start_time.elapsed();

        // Check performance
        println!("Routed {} events in {:?} ({:.2} events/sec)",
                 event_count, duration, event_count as f64 / duration.as_secs_f64());

        assert_eq!(successes + failures, event_count);
        assert!(failures < event_count / 10); // Less than 10% failure rate

        // Check final statistics
        let stats = router.get_routing_stats().await.unwrap();
        assert_eq!(stats.total_events_routed, successes as u64);

        // Verify load distribution
        for i in 0..5 {
            let service_id = format!("service-{}", i);
            if let Some(service_stats) = stats.service_stats.get(&service_id) {
                println!("Service {} received {} events", service_id, service_stats.events_received);
                assert!(service_stats.events_received > 0); // Each service should get some events
            }
        }
    }

    #[tokio::test]
    async fn test_concurrent_event_processing() {
        let router = Arc::new(DefaultEventRouter::new());

        // Register service
        router.register_service(create_test_service_registration(
            "concurrent-service",
            "test",
            vec!["test".to_string()],
        )).await.unwrap();

        // Create multiple concurrent event streams
        let mut handles = Vec::new();

        for stream_id in 0..10 {
            let router_clone = router.clone();
            let handle = tokio::spawn(async move {
                for event_id in 0..100 {
                    let event = DaemonEvent::new(
                        EventType::Custom(format!("stream-{}-event-{}", stream_id, event_id)),
                        EventSource::service(format!("stream-{}-client", stream_id)),
                        EventPayload::json(serde_json::json!({
                            "stream_id": stream_id,
                            "event_id": event_id
                        })),
                    );

                    if let Err(e) = router_clone.route_event(event).await {
                        eprintln!("Failed to route event in stream {}: {}", stream_id, e);
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all streams to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Check final statistics
        let stats = router.get_routing_stats().await.unwrap();
        assert_eq!(stats.total_events_routed, 1000); // 10 streams * 100 events
    }
}