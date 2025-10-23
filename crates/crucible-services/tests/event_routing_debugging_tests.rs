//! Comprehensive unit tests for event routing with debugging integration
//!
//! Tests for event creation, routing decisions, handler management,
//! metrics collection, and debugging functionality.

use crucible_services::event_routing::*;
use crucible_services::logging::EventMetrics;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Mock event handler for testing
struct MockEventHandler {
    name: String,
    can_handle_types: Vec<EventType>,
    priority: EventPriority,
    should_fail: bool,
    handle_count: Arc<AtomicU64>,
    handle_duration: Duration,
}

impl MockEventHandler {
    fn new(name: &str, can_handle_types: Vec<EventType>) -> Self {
        Self {
            name: name.to_string(),
            can_handle_types,
            priority: EventPriority::Normal,
            should_fail: false,
            handle_count: Arc::new(AtomicU64::new(0)),
            handle_duration: Duration::from_millis(0),
        }
    }

    fn with_priority(mut self, priority: EventPriority) -> Self {
        self.priority = priority;
        self
    }

    fn with_failure(mut self, should_fail: bool) -> Self {
        self.should_fail = should_fail;
        self
    }

    fn with_duration(mut self, duration: Duration) -> Self {
        self.handle_duration = duration;
        self
    }

    fn get_handle_count(&self) -> u64 {
        self.handle_count.load(Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl EventHandler for MockEventHandler {
    fn handler_name(&self) -> &str {
        &self.name
    }

    async fn can_handle(&self, event: &Event) -> bool {
        // Simulate async work
        tokio::task::yield_now().await;
        self.can_handle_types.contains(&event.event_type)
    }

    async fn handle_event(&self, event: Event) -> Result<Event, ServiceError> {
        self.handle_count.fetch_add(1, Ordering::Relaxed);

        // Simulate processing time
        if self.handle_duration > Duration::ZERO {
            tokio::time::sleep(self.handle_duration).await;
        }

        if self.should_fail {
            Err(ServiceError::ExecutionError(format!("Handler {} failed", self.name)))
        } else {
            // Add handler metadata
            let mut modified_event = event;
            modified_event.metadata.insert(
                "handled_by".to_string(),
                self.name.clone(),
            );
            Ok(modified_event)
        }
    }

    fn handler_priority(&self) -> EventPriority {
        self.priority
    }
}

/// Test event creation and manipulation
#[cfg(test)]
mod event_creation_tests {
    use super::*;

    #[test]
    fn test_event_new_basic() {
        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({"test": "data"}),
        );

        assert!(!event.id.is_empty());
        assert_eq!(event.event_type, EventType::ScriptExecution);
        assert_eq!(event.source, "test_source");
        assert_eq!(event.priority, EventPriority::Normal);
        assert!(event.target.is_none());
        assert_eq!(event.metadata.len(), 0);
    }

    #[test]
    fn test_event_with_priority() {
        let event = Event::new(
            EventType::ToolExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        ).with_priority(EventPriority::High);

        assert_eq!(event.priority, EventPriority::High);
    }

    #[test]
    fn test_event_with_target() {
        let event = Event::new(
            EventType::System,
            "test_source".to_string(),
            serde_json::json!({}),
        ).with_target("test_target".to_string());

        assert_eq!(event.target, Some("test_target".to_string()));
    }

    #[test]
    fn test_event_with_metadata() {
        let event = Event::new(
            EventType::UserInteraction,
            "test_source".to_string(),
            serde_json::json!({}),
        ).with_metadata("key1".to_string(), "value1".to_string())
         .with_metadata("key2".to_string(), "value2".to_string());

        assert_eq!(event.metadata.get("key1"), Some(&"value1".to_string()));
        assert_eq!(event.metadata.get("key2"), Some(&"value2".to_string()));
        assert_eq!(event.metadata.len(), 2);
    }

    #[test]
    fn test_event_chaining() {
        let event = Event::new(
            EventType::Custom("test".to_string()),
            "test_source".to_string(),
            serde_json::json!({}),
        )
        .with_priority(EventPriority::Critical)
        .with_target("test_target".to_string())
        .with_metadata("key".to_string(), "value".to_string());

        assert_eq!(event.event_type, EventType::Custom("test".to_string()));
        assert_eq!(event.priority, EventPriority::Critical);
        assert_eq!(event.target, Some("test_target".to_string()));
        assert_eq!(event.metadata.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_event_age() {
        let event = Event::new(
            EventType::System,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        // Age should be very small
        let age = event.age();
        assert!(age.as_millis() < 100);

        // Test with older event by manipulating timestamp
        let mut old_event = event;
        old_event.created_at = chrono::Utc::now() - chrono::Duration::seconds(10);
        assert!(old_event.age().as_secs() >= 10);
    }

    #[test]
    fn test_event_serialization() {
        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({"key": "value"}),
        ).with_priority(EventPriority::High)
         .with_metadata("meta_key".to_string(), "meta_value".to_string());

        let serialized = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&serialized).unwrap();

        assert_eq!(event.id, deserialized.id);
        assert_eq!(event.event_type, deserialized.event_type);
        assert_eq!(event.priority, deserialized.priority);
        assert_eq!(event.source, deserialized.source);
        assert_eq!(event.metadata, deserialized.metadata);
    }

    #[test]
    fn test_event_types_display() {
        assert_eq!(EventType::ScriptExecution.to_string(), "script_execution");
        assert_eq!(EventType::ToolExecution.to_string(), "tool_execution");
        assert_eq!(EventType::System.to_string(), "system");
        assert_eq!(EventType::UserInteraction.to_string(), "user_interaction");
        assert_eq!(EventType::Error.to_string(), "error");
        assert_eq!(EventType::Custom("test".to_string()).to_string(), "custom_test");
    }

    #[test]
    fn test_event_priority_ordering() {
        assert!(EventPriority::Critical > EventPriority::High);
        assert!(EventPriority::High > EventPriority::Normal);
        assert!(EventPriority::Normal > EventPriority::Low);
    }
}

/// Test event router functionality
#[cfg(test)]
mod event_router_tests {
    use super::*;

    #[tokio::test]
    async fn test_event_router_creation() {
        let config = EventRouterConfig::default();
        let router = EventRouter::new(config);

        assert_eq!(router.get_active_events_count().await, 0);

        let metrics = router.get_metrics().await;
        assert_eq!(metrics.total_events, 0);
    }

    #[tokio::test]
    async fn test_event_router_handler_registration() {
        let router = EventRouter::new(EventRouterConfig::default());
        let handler = Arc::new(MockEventHandler::new(
            "test_handler",
            vec![EventType::ScriptExecution],
        ));

        router.register_handler(handler.clone()).await.unwrap();

        // Handler should be registered (we can't directly inspect handlers, but we can test routing)
        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        let result = router.route_event(event).await;
        assert!(result.is_ok());
        assert_eq!(handler.get_handle_count(), 1);
    }

    #[tokio::test]
    async fn test_event_router_multiple_handlers_priority() {
        let router = EventRouter::new(EventRouterConfig::default());

        let low_priority_handler = Arc::new(MockEventHandler::new(
            "low_priority",
            vec![EventType::ScriptExecution],
        ).with_priority(EventPriority::Low));

        let high_priority_handler = Arc::new(MockEventHandler::new(
            "high_priority",
            vec![EventType::ScriptExecution],
        ).with_priority(EventPriority::High));

        // Register in reverse order to test sorting
        router.register_handler(low_priority_handler.clone()).await.unwrap();
        router.register_handler(high_priority_handler.clone()).await.unwrap();

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        let result = router.route_event(event).await;
        assert!(result.is_ok());

        // Both handlers should be called (in our implementation, all suitable handlers are called)
        assert_eq!(low_priority_handler.get_handle_count(), 1);
        assert_eq!(high_priority_handler.get_handle_count(), 1);
    }

    #[tokio::test]
    async fn test_event_router_no_handlers() {
        let router = EventRouter::new(EventRouterConfig::default());

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        let result = router.route_event(event).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            ServiceError::ValidationError(msg) => {
                assert!(msg.contains("No handlers available"));
            }
            _ => panic!("Expected ValidationError"),
        }
    }

    #[tokio::test]
    async fn test_event_router_expired_event() {
        let config = EventRouterConfig {
            max_event_age: Duration::from_millis(1),
            ..Default::default()
        };
        let router = EventRouter::new(config);

        let handler = Arc::new(MockEventHandler::new(
            "test_handler",
            vec![EventType::ScriptExecution],
        ));

        router.register_handler(handler).await.unwrap();

        let mut event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        // Make event expired
        event.created_at = chrono::Utc::now() - chrono::Duration::milliseconds(10);

        let result = router.route_event(event).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            ServiceError::ValidationError(msg) => {
                assert!(msg.contains("Event too old"));
            }
            _ => panic!("Expected ValidationError"),
        }
    }

    #[tokio::test]
    async fn test_event_router_capacity_limit() {
        let config = EventRouterConfig {
            max_concurrent_events: 0, // No capacity
            ..Default::default()
        };
        let router = EventRouter::new(config);

        let handler = Arc::new(MockEventHandler::new(
            "test_handler",
            vec![EventType::ScriptExecution],
        ).with_duration(Duration::from_millis(100))); // Slow handler

        router.register_handler(handler).await.unwrap();

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        let result = router.route_event(event).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            ServiceError::ExecutionError(msg) => {
                assert!(msg.contains("System at capacity"));
            }
            _ => panic!("Expected ExecutionError"),
        }
    }

    #[tokio::test]
    async fn test_event_router_handler_failure() {
        let router = EventRouter::new(EventRouterConfig::default());

        let failing_handler = Arc::new(MockEventHandler::new(
            "failing_handler",
            vec![EventType::ScriptExecution],
        ).with_failure(true));

        let working_handler = Arc::new(MockEventHandler::new(
            "working_handler",
            vec![EventType::ScriptExecution],
        ).with_failure(false));

        router.register_handler(failing_handler).await.unwrap();
        router.register_handler(working_handler).await.unwrap();

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        let result = router.route_event(event).await;
        assert!(result.is_ok());

        let routing_result = result.unwrap();
        assert_eq!(routing_result.delivery_results.len(), 2);

        // One should fail, one should succeed
        let success_count = routing_result.delivery_results.iter()
            .filter(|r| r.success)
            .count();
        let failure_count = routing_result.delivery_results.iter()
            .filter(|r| !r.success)
            .count();

        assert_eq!(success_count, 1);
        assert_eq!(failure_count, 1);
    }

    #[tokio::test]
    async fn test_event_router_metrics_collection() {
        let router = EventRouter::new(EventRouterConfig::default());

        let handler = Arc::new(MockEventHandler::new(
            "test_handler",
            vec![EventType::ScriptExecution],
        ));

        router.register_handler(handler.clone()).await.unwrap();

        // Route several events
        for _ in 0..5 {
            let event = Event::new(
                EventType::ScriptExecution,
                "test_source".to_string(),
                serde_json::json!({}),
            );

            let _result = router.route_event(event).await.unwrap();
        }

        let metrics = router.get_metrics().await;
        assert_eq!(metrics.total_events, 5);
        assert_eq!(metrics.successful_events, 5);
        assert_eq!(metrics.failed_events, 0);
        assert!(metrics.total_duration_ms > 0);
        assert!(metrics.avg_duration_ms > 0.0);
        assert_eq!(handler.get_handle_count(), 5);
    }

    #[tokio::test]
    async fn test_event_router_routing_history() {
        let router = EventRouter::new(EventRouterConfig::default());

        let handler = Arc::new(MockEventHandler::new(
            "test_handler",
            vec![EventType::ScriptExecution],
        ));

        router.register_handler(handler).await.unwrap();

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        let _result = router.route_event(event).await.unwrap();

        let history = router.get_routing_history(Some(10)).await;
        assert_eq!(history.len(), 1);

        let decision = &history[0];
        assert!(!decision.event_id.is_empty());
        assert_eq!(decision.source, "test_source");
        assert_eq!(decision.targets.len(), 1);
        assert_eq!(decision.targets[0], "test_handler");
    }

    #[tokio::test]
    async fn test_event_router_reset_metrics() {
        let router = EventRouter::new(EventRouterConfig::default());

        let handler = Arc::new(MockEventHandler::new(
            "test_handler",
            vec![EventType::ScriptExecution],
        ));

        router.register_handler(handler).await.unwrap();

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        let _result = router.route_event(event).await.unwrap();

        // Verify metrics are recorded
        let metrics = router.get_metrics().await;
        assert_eq!(metrics.total_events, 1);

        // Reset metrics
        router.reset_metrics().await;

        // Verify metrics are reset
        let reset_metrics = router.get_metrics().await;
        assert_eq!(reset_metrics.total_events, 0);
        assert_eq!(reset_metrics.successful_events, 0);
        assert_eq!(reset_metrics.failed_events, 0);
    }
}

/// Test routing strategies and decisions
#[cfg(test)]
mod routing_strategy_tests {
    use super::*;

    #[tokio::test]
    async fn test_routing_strategy_direct() {
        let router = EventRouter::new(EventRouterConfig::default());

        let handler = Arc::new(MockEventHandler::new(
            "target_handler",
            vec![EventType::ScriptExecution],
        ));

        router.register_handler(handler).await.unwrap();

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        ).with_target("target_handler".to_string());

        let result = router.route_event(event).await.unwrap();

        assert_eq!(result.decision.strategy, RoutingStrategy::Direct);
        assert_eq!(result.decision.targets.len(), 1);
        assert_eq!(result.decision.targets[0], "target_handler");
    }

    #[tokio::test]
    async fn test_routing_strategy_type_based() {
        let config = EventRouterConfig {
            default_strategy: RoutingStrategy::TypeBased,
            ..Default::default()
        };
        let router = EventRouter::new(config);

        let handler = Arc::new(MockEventHandler::new(
            "test_handler",
            vec![EventType::ScriptExecution],
        ));

        router.register_handler(handler).await.unwrap();

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        let result = router.route_event(event).await.unwrap();

        assert_eq!(result.decision.strategy, RoutingStrategy::TypeBased);
        assert!(result.decision.reasoning.contains("script_execution"));
    }

    #[tokio::test]
    async fn test_routing_strategy_priority_based() {
        let config = EventRouterConfig {
            default_strategy: RoutingStrategy::PriorityBased,
            ..Default::default()
        };
        let router = EventRouter::new(config);

        let handler = Arc::new(MockEventHandler::new(
            "test_handler",
            vec![EventType::ScriptExecution],
        ));

        router.register_handler(handler).await.unwrap();

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        ).with_priority(EventPriority::High);

        let result = router.route_event(event).await.unwrap();

        assert_eq!(result.decision.strategy, RoutingStrategy::PriorityBased);
        assert!(result.decision.reasoning.contains("High"));
    }

    #[test]
    fn test_routing_strategy_display() {
        assert_eq!(RoutingStrategy::Direct.to_string(), "direct");
        assert_eq!(RoutingStrategy::Broadcast.to_string(), "broadcast");
        assert_eq!(RoutingStrategy::TypeBased.to_string(), "type_based");
        assert_eq!(RoutingStrategy::PriorityBased.to_string(), "priority_based");
        assert_eq!(RoutingStrategy::Custom("test".to_string()).to_string(), "custom_test");
    }

    #[test]
    fn test_routing_decision_serialization() {
        let decision = RoutingDecision {
            event_id: "test_id".to_string(),
            source: "test_source".to_string(),
            targets: vec!["handler1".to_string(), "handler2".to_string()],
            strategy: RoutingStrategy::TypeBased,
            decided_at: chrono::Utc::now(),
            reasoning: "Test reasoning".to_string(),
        };

        let serialized = serde_json::to_string(&decision).unwrap();
        let deserialized: RoutingDecision = serde_json::from_str(&serialized).unwrap();

        assert_eq!(decision.event_id, deserialized.event_id);
        assert_eq!(decision.source, deserialized.source);
        assert_eq!(decision.targets, deserialized.targets);
        assert_eq!(decision.strategy, deserialized.strategy);
        assert_eq!(decision.reasoning, deserialized.reasoning);
    }
}

/// Test configuration functionality
#[cfg(test)]
mod configuration_tests {
    use super::*;

    #[test]
    fn test_event_router_config_default() {
        let config = EventRouterConfig::default();

        assert_eq!(config.max_event_age, Duration::from_secs(300));
        assert_eq!(config.max_concurrent_events, 1000);
        assert_eq!(config.default_strategy, RoutingStrategy::TypeBased);
        assert!(!config.enable_detailed_tracing); // Default depends on env var
    }

    #[test]
    fn test_event_router_config_custom() {
        let config = EventRouterConfig {
            max_event_age: Duration::from_secs(600),
            max_concurrent_events: 2000,
            enable_detailed_tracing: true,
            default_strategy: RoutingStrategy::Broadcast,
        };

        assert_eq!(config.max_event_age, Duration::from_secs(600));
        assert_eq!(config.max_concurrent_events, 2000);
        assert!(config.enable_detailed_tracing);
        assert_eq!(config.default_strategy, RoutingStrategy::Broadcast);
    }

    #[test]
    fn test_handler_config_default() {
        let config = HandlerConfig::default();

        assert!(config.script_execution);
        assert!(config.tool_execution);
        assert!(config.system_events);
        assert!(config.user_interaction);
        assert_eq!(config.timeout_seconds, 30);
    }

    #[test]
    fn test_handler_config_serialization() {
        let config = HandlerConfig {
            script_execution: false,
            tool_execution: true,
            system_events: false,
            user_interaction: true,
            timeout_seconds: 60,
        };

        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: HandlerConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(config.script_execution, deserialized.script_execution);
        assert_eq!(config.tool_execution, deserialized.tool_execution);
        assert_eq!(config.system_events, deserialized.system_events);
        assert_eq!(config.user_interaction, deserialized.user_interaction);
        assert_eq!(config.timeout_seconds, deserialized.timeout_seconds);
    }
}

/// Test delivery results and routing results
#[cfg(test)]
mod result_tests {
    use super::*;

    #[test]
    fn test_delivery_result_creation() {
        let result = DeliveryResult {
            target: "test_target".to_string(),
            success: true,
            delivery_time_ms: 100,
            error: None,
        };

        assert_eq!(result.target, "test_target");
        assert!(result.success);
        assert_eq!(result.delivery_time_ms, 100);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_delivery_result_with_error() {
        let result = DeliveryResult {
            target: "test_target".to_string(),
            success: false,
            delivery_time_ms: 50,
            error: Some("Test error".to_string()),
        };

        assert_eq!(result.target, "test_target");
        assert!(!result.success);
        assert_eq!(result.delivery_time_ms, 50);
        assert_eq!(result.error, Some("Test error".to_string()));
    }

    #[test]
    fn test_routing_result_structure() {
        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        let decision = RoutingDecision {
            event_id: event.id.clone(),
            source: event.source.clone(),
            targets: vec!["handler1".to_string()],
            strategy: RoutingStrategy::Direct,
            decided_at: chrono::Utc::now(),
            reasoning: "Test".to_string(),
        };

        let delivery_results = vec![DeliveryResult {
            target: "handler1".to_string(),
            success: true,
            delivery_time_ms: 75,
            error: None,
        }];

        let result = RoutingResult {
            event: event.clone(),
            decision,
            delivery_results,
            routing_time_ms: 100,
        };

        assert_eq!(result.event.id, event.id);
        assert_eq!(result.decision.event_id, event.id);
        assert_eq!(result.delivery_results.len(), 1);
        assert_eq!(result.routing_time_ms, 100);
    }
}

/// Test performance characteristics
#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_high_throughput_routing() {
        let router = Arc::new(EventRouter::new(EventRouterConfig::default()));

        let handler = Arc::new(MockEventHandler::new(
            "fast_handler",
            vec![EventType::ScriptExecution],
        ));

        router.register_handler(handler).await.unwrap();

        let num_events = 1000;
        let start = Instant::now();

        let mut handles = vec![];
        for i in 0..num_events {
            let router_clone = router.clone();
            let handle = tokio::spawn(async move {
                let event = Event::new(
                    EventType::ScriptExecution,
                    format!("source_{}", i),
                    serde_json::json!({"index": i}),
                );

                router_clone.route_event(event).await
            });
            handles.push(handle);
        }

        let mut successful = 0;
        for handle in handles {
            match handle.await.unwrap() {
                Ok(_) => successful += 1,
                Err(_) => {}, // Some may fail due to capacity or timing
            }
        }

        let elapsed = start.elapsed();

        // Should complete reasonably quickly (< 5 seconds for 1000 events)
        assert!(elapsed.as_secs() < 5);
        assert!(successful > 900); // Most should succeed

        let metrics = router.get_metrics().await;
        assert!(metrics.total_events >= successful as u64);
    }

    #[tokio::test]
    async fn test_concurrent_handler_registration() {
        let router = Arc::new(EventRouter::new(EventRouterConfig::default()));

        let mut handles = vec![];
        for i in 0..10 {
            let router_clone = router.clone();
            let handle = tokio::spawn(async move {
                let handler = Arc::new(MockEventHandler::new(
                    &format!("handler_{}", i),
                    vec![EventType::ScriptExecution],
                ));

                router_clone.register_handler(handler).await
            });
            handles.push(handle);
        }

        // All registrations should succeed
        for handle in handles {
            assert!(handle.await.unwrap().is_ok());
        }
    }

    #[tokio::test]
    async fn test_memory_usage_stability() {
        let router = EventRouter::new(EventRouterConfig::default());

        let handler = Arc::new(MockEventHandler::new(
            "memory_test_handler",
            vec![EventType::ScriptExecution],
        ));

        router.register_handler(handler).await.unwrap();

        // Route many events to test memory stability
        for i in 0..5000 {
            let event = Event::new(
                EventType::ScriptExecution,
                format!("source_{}", i),
                serde_json::json!({
                    "data": "x".repeat(100), // Some payload
                    "index": i
                }),
            );

            let _result = router.route_event(event).await.unwrap();

            // Periodically check history size
            if i % 1000 == 0 {
                let history = router.get_routing_history(None).await;
                assert!(history.len() <= 1000); // Should be limited
            }
        }

        // Final check - history should be bounded
        let history = router.get_routing_history(None).await;
        assert!(history.len() <= 1000);
    }
}

/// Test edge cases and error conditions
#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[tokio::test]
    async fn test_event_with_large_payload() {
        let router = EventRouter::new(EventRouterConfig::default());

        let handler = Arc::new(MockEventHandler::new(
            "large_payload_handler",
            vec![EventType::ScriptExecution],
        ));

        router.register_handler(handler).await.unwrap();

        let large_payload = serde_json::json!({
            "data": "x".repeat(1_000_000) // 1MB payload
        });

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            large_payload,
        );

        let result = router.route_event(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_event_with_unicode_content() {
        let router = EventRouter::new(EventRouterConfig::default());

        let handler = Arc::new(MockEventHandler::new(
            "unicode_handler",
            vec![EventType::ScriptExecution],
        ));

        router.register_handler(handler).await.unwrap();

        let unicode_payload = serde_json::json!({
            "text": "Hello 世界 🌍",
            "emoji": "🚀🎉🔥",
            "chinese": "测试中文",
            "japanese": "テスト日本語",
            "arabic": "اختبار عربي",
            "russian": "Тест русский"
        });

        let event = Event::new(
            EventType::ScriptExecution,
            "unicode_source".to_string(),
            unicode_payload,
        ).with_metadata("unicode_key".to_string(), "测试值".to_string());

        let result = router.route_event(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handler_with_zero_duration() {
        let router = EventRouter::new(EventRouterConfig::default());

        let handler = Arc::new(MockEventHandler::new(
            "instant_handler",
            vec![EventType::ScriptExecution],
        ).with_duration(Duration::ZERO));

        router.register_handler(handler).await.unwrap();

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        let result = router.route_event(event).await;
        assert!(result.is_ok());

        // Routing time should be very small
        assert!(result.unwrap().routing_time_ms < 100);
    }

    #[tokio::test]
    async fn test_event_type_edge_cases() {
        let router = EventRouter::new(EventRouterConfig::default());

        let handler = Arc::new(MockEventHandler::new(
            "edge_case_handler",
            vec![
                EventType::Custom("".to_string()), // Empty custom type
                EventType::Custom("very_long_custom_type_name_that_tests_length_limits".to_string()),
            ],
        ));

        router.register_handler(handler).await.unwrap();

        // Test empty custom type
        let event1 = Event::new(
            EventType::Custom("".to_string()),
            "test_source".to_string(),
            serde_json::json!({}),
        );

        let result1 = router.route_event(event1).await;
        assert!(result1.is_ok());

        // Test very long custom type
        let event2 = Event::new(
            EventType::Custom("very_long_custom_type_name_that_tests_length_limits".to_string()),
            "test_source".to_string(),
            serde_json::json!({}),
        );

        let result2 = router.route_event(event2).await;
        assert!(result2.is_ok());
    }
}