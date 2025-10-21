//! Examples of using the event system for daemon coordination

use super::core::*;
use super::routing::*;
use super::service_events::*;
use super::errors::EventResult;
use crate::types::{ServiceHealth, ServiceStatus};
use chrono::Utc;
use std::collections::HashMap;

/// Example: Basic event creation and routing
pub async fn basic_event_routing_example() -> EventResult<()> {
    // Create the event router
    let router = DefaultEventRouter::new();

    // Register services
    register_example_services(&router).await?;

    // Create a file system event
    let file_event = DaemonEvent::new(
        EventType::Filesystem(FilesystemEventType::FileCreated {
            path: "/vault/new-document.md".to_string(),
        }),
        EventSource::filesystem("watcher-1".to_string()),
        EventPayload::json(serde_json::json!({
            "size": 2048,
            "type": "text/markdown",
            "created_by": "user-123"
        })),
    )
    .with_priority(EventPriority::Normal)
    .with_correlation(Uuid::new_v4());

    // Route the event
    router.route_event(file_event).await?;

    println!("File event routed successfully");

    Ok(())
}

/// Example: Service-specific events
pub async fn service_specific_events_example() -> EventResult<()> {
    let router = DefaultEventRouter::new();
    register_example_services(&router).await?;

    // Create an MCP tool call event
    let mcp_event = McpGatewayEvent::tool_call_completed(
        "mcp-server-1".to_string(),
        "search_files".to_string(),
        "call-456".to_string(),
        ToolCallResult::Success {
            result: serde_json::json!({
                "files": [
                    {"path": "/doc1.md", "score": 0.95},
                    {"path": "/doc2.md", "score": 0.87}
                ]
            }),
        },
        1200, // duration_ms
    );

    let daemon_event = ServiceEventBuilder::new(EventSource::service("mcp-gateway".to_string()))
        .with_correlation(Uuid::new_v4())
        .mcp_gateway_event(mcp_event);

    router.route_event(daemon_event).await?;

    println!("MCP event routed successfully");

    Ok(())
}

/// Example: Inference engine events
pub async fn inference_engine_example() -> EventResult<()> {
    let router = DefaultEventRouter::new();
    register_example_services(&router).await?;

    // Create an inference completion event
    let inference_event = InferenceEngineEvent::inference_completed(
        "req-789".to_string(),
        "gpt-4".to_string(),
        InferenceResult {
            output: "Here's the analysis of your document...".to_string(),
            output_tokens: 256,
            confidence: Some(0.92),
            finish_reason: "stop".to_string(),
            metadata: HashMap::new(),
            alternatives: None,
        },
        2500, // duration_ms
        TokenUsage {
            prompt_tokens: 128,
            completion_tokens: 256,
            total_tokens: 384,
        },
    );

    let daemon_event = ServiceEventBuilder::new(EventSource::service("inference-engine".to_string()))
        .with_correlation(Uuid::new_v4())
        .inference_engine_event(inference_event);

    router.route_event(daemon_event).await?;

    println!("Inference event routed successfully");

    Ok(())
}

/// Example: Script execution events
pub async fn script_engine_example() -> EventResult<()> {
    let router = DefaultEventRouter::new();
    register_example_services(&router).await?;

    // Create a script execution completed event
    let script_event = ScriptEngineEvent::script_execution_completed(
        "exec-123".to_string(),
        "process-document".to_string(),
        ScriptResult {
            output: serde_json::json!({
                "processed": true,
                "word_count": 1500,
                "sections": 5
            }),
            return_code: 0,
            stdout: Some("Document processed successfully".to_string()),
            stderr: None,
            artifacts: vec![],
            metadata: HashMap::new(),
        },
        800, // duration_ms
        ResourceUsage {
            cpu_time_ms: 750,
            memory_peak_mb: 128,
            disk_read_bytes: 2048,
            disk_write_bytes: 1024,
            network_bytes_sent: 0,
            network_bytes_received: 0,
        },
    );

    let daemon_event = ServiceEventBuilder::new(EventSource::service("script-engine".to_string()))
        .with_correlation(Uuid::new_v4())
        .script_engine_event(script_event);

    router.route_event(daemon_event).await?;

    println!("Script execution event routed successfully");

    Ok(())
}

/// Example: Custom routing rules
pub async fn custom_routing_rules_example() -> EventResult<()> {
    let router = DefaultEventRouter::with_config(RoutingConfig {
        load_balancing_strategy: LoadBalancingStrategy::HealthBased,
        enable_deduplication: true,
        ..Default::default()
    });

    register_example_services(&router).await?;

    // Create a routing rule for urgent file processing
    let urgent_files_rule = RoutingRule {
        rule_id: "urgent-files".to_string(),
        name: "Urgent File Processing".to_string(),
        description: "Route high-priority file events to inference engine".to_string(),
        filter: EventFilter {
            event_types: vec!["filesystem".to_string()],
            priorities: vec![EventPriority::Critical, EventPriority::High],
            ..Default::default()
        },
        targets: vec![
            ServiceTarget::new("inference-engine".to_string())
                .with_priority(1)
                .with_filter(EventFilter {
                    event_types: vec!["filesystem".to_string()],
                    ..Default::default()
                })
        ],
        priority: 10,
        enabled: true,
        conditions: vec![],
    };

    router.add_routing_rule(urgent_files_rule).await?;

    // Create a high-priority file event
    let urgent_file_event = DaemonEvent::new(
        EventType::Filesystem(FilesystemEventType::FileCreated {
            path: "/vault/urgent-request.md".to_string(),
        }),
        EventSource::filesystem("watcher-urgent".to_string()),
        EventPayload::json(serde_json::json!({
            "priority": "urgent",
            "user": "admin",
            "deadline": "2024-01-01T00:00:00Z"
        })),
    )
    .with_priority(EventPriority::High)
    .with_correlation(Uuid::new_v4());

    router.route_event(urgent_file_event).await?;

    println!("Urgent file event routed with custom rules");

    Ok(())
}

/// Example: Error handling and retries
pub async fn error_handling_example() -> EventResult<()> {
    let router = DefaultEventRouter::with_config(RoutingConfig {
        default_max_retries: 5,
        circuit_breaker_threshold: 3,
        circuit_breaker_timeout_ms: 5000,
        ..Default::default()
    });

    register_example_services(&router).await?;

    // Simulate a service failure
    router.update_service_health(
        "datastore",
        ServiceHealth {
            status: ServiceStatus::Unhealthy,
            message: Some("Database connection failed".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        },
    ).await?;

    // Create an event that would go to the unhealthy service
    let db_event = DaemonEvent::new(
        EventType::Database(DatabaseEventType::RecordCreated {
            table: "documents".to_string(),
            id: "doc-123".to_string(),
        }),
        EventSource::service("api-gateway".to_string()),
        EventPayload::json(serde_json::json!({
            "title": "New Document",
            "content": "Document content here"
        })),
    )
    .with_priority(EventPriority::Normal)
    .with_max_retries(5);

    match router.route_event(db_event).await {
        Ok(_) => println!("Event routed successfully"),
        Err(e) => println!("Event routing failed: {}", e),
    }

    // Restore service health
    router.update_service_health(
        "datastore",
        ServiceHealth {
            status: ServiceStatus::Healthy,
            message: Some("Service restored".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        },
    ).await?;

    println!("Service health restored");

    Ok(())
}

/// Example: Load balancing demonstration
pub async fn load_balancing_example() -> EventResult<()> {
    // Create multiple instances of the same service
    let router = DefaultEventRouter::with_config(RoutingConfig {
        load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
        ..Default::default()
    });

    // Register multiple service instances
    for i in 1..=3 {
        let registration = ServiceRegistration {
            service_id: format!("inference-engine-{}", i),
            service_type: "inference-engine".to_string(),
            instance_id: format!("instance-{}", i),
            endpoint: None,
            supported_event_types: vec!["mcp".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 10,
            filters: vec![],
            metadata: HashMap::new(),
        };

        router.register_service(registration).await?;
    }

    // Create multiple events to see load balancing in action
    for i in 1..=10 {
        let mcp_event = McpGatewayEvent::tool_call_completed(
            format!("server-{}", i),
            "analyze_text".to_string(),
            format!("call-{}", i),
            ToolCallResult::Success {
                result: serde_json::json!({"analysis": "completed"}),
            },
            1000,
        );

        let daemon_event = ServiceEventBuilder::new(EventSource::service("mcp-gateway".to_string()))
            .mcp_gateway_event(mcp_event);

        router.route_event(daemon_event).await?;
        println!("Routed event {}", i);
    }

    // Get routing statistics
    let stats = router.get_routing_stats().await?;
    println!("Total events routed: {}", stats.total_events_routed);

    for (service_id, service_stats) in stats.service_stats {
        println!("Service {} processed {} events", service_id, service_stats.events_processed);
    }

    Ok(())
}

/// Example: Event deduplication
pub async fn deduplication_example() -> EventResult<()> {
    let router = DefaultEventRouter::with_config(RoutingConfig {
        enable_deduplication: true,
        deduplication_window_s: 30,
        ..Default::default()
    });

    register_example_services(&router).await?;

    // Create the same event twice
    let event_data = DaemonEvent::new(
        EventType::Filesystem(FilesystemEventType::FileModified {
            path: "/vault/test.md".to_string(),
        }),
        EventSource::filesystem("watcher-1".to_string()),
        EventPayload::json(serde_json::json!({"checksum": "abc123"})),
    );

    let event1 = event_data.clone().with_correlation(Uuid::new_v4());
    let event2 = event_data.with_correlation(Uuid::new_v4());

    // Route first event (should succeed)
    match router.route_event(event1).await {
        Ok(_) => println!("First event routed successfully"),
        Err(e) => println!("First event failed: {}", e),
    }

    // Route second identical event (should be rejected)
    match router.route_event(event2).await {
        Ok(_) => println!("Second event routed successfully"),
        Err(e) => println!("Second event rejected as expected: {}", e),
    }

    Ok(())
}

/// Helper function to register example services
async fn register_example_services(router: &DefaultEventRouter) -> EventResult<()> {
    let services = vec![
        ServiceRegistration {
            service_id: "mcp-gateway".to_string(),
            service_type: "mcp-gateway".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["filesystem".to_string(), "mcp".to_string()],
            priority: 1,
            weight: 1.0,
            max_concurrent_events: 50,
            filters: vec![],
            metadata: HashMap::new(),
        },
        ServiceRegistration {
            service_id: "inference-engine".to_string(),
            service_type: "inference-engine".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["mcp".to_string(), "service".to_string()],
            priority: 2,
            weight: 2.0,
            max_concurrent_events: 20,
            filters: vec![],
            metadata: HashMap::new(),
        },
        ServiceRegistration {
            service_id: "script-engine".to_string(),
            service_type: "script-engine".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["filesystem".to_string(), "external".to_string()],
            priority: 1,
            weight: 1.5,
            max_concurrent_events: 30,
            filters: vec![],
            metadata: HashMap::new(),
        },
        ServiceRegistration {
            service_id: "datastore".to_string(),
            service_type: "datastore".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["database".to_string(), "filesystem".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 100,
            filters: vec![],
            metadata: HashMap::new(),
        },
    ];

    for service in services {
        router.register_service(service).await?;
    }

    Ok(())
}

/// Example utility functions for common event patterns
pub mod patterns {
    use super::*;

    /// Create a chain of related events
    pub async fn event_chain_example() -> EventResult<()> {
        let router = DefaultEventRouter::new();
        register_example_services(&router).await?;

        // Start with correlation ID
        let correlation_id = Uuid::new_v4();

        // Event 1: File created
        let file_event = DaemonEvent::with_correlation(
            EventType::Filesystem(FilesystemEventType::FileCreated {
                path: "/vault/new-doc.md".to_string(),
            }),
            EventSource::filesystem("watcher-1".to_string()),
            EventPayload::json(serde_json::json!({"size": 1024})),
            correlation_id,
        );

        let file_id = file_event.id;
        router.route_event(file_event).await?;

        // Event 2: Script processing (caused by file event)
        let script_event = DaemonEvent::as_response(
            EventType::Service(ServiceEventType::RequestReceived {
                from_service: "daemon".to_string(),
                to_service: "script-engine".to_string(),
                request: serde_json::json!({"script": "process-document"}),
            }),
            EventSource::service("script-engine".to_string()),
            EventPayload::json(serde_json::json!({"status": "started"})),
            file_id,
        );

        let script_id = script_event.id;
        router.route_event(script_event).await?;

        // Event 3: Inference analysis (caused by script event)
        let inference_event = DaemonEvent::as_response(
            EventType::Service(ServiceEventType::RequestReceived {
                from_service: "script-engine".to_string(),
                to_service: "inference-engine".to_string(),
                request: serde_json::json!({"task": "analyze-content"}),
            }),
            EventSource::service("inference-engine".to_string()),
            EventPayload::json(serde_json::json!({"analysis": "in-progress"})),
            script_id,
        );

        router.route_event(inference_event).await?;

        println!("Event chain created with correlation ID: {}", correlation_id);

        Ok(())
    }

    /// Create a batch of events for bulk processing
    pub async fn batch_events_example() -> EventResult<()> {
        let router = DefaultEventRouter::new();
        register_example_services(&router).await?;

        let correlation_id = Uuid::new_v4();

        // Create multiple file events in batch
        let file_paths = vec![
            "/vault/doc1.md",
            "/vault/doc2.md",
            "/vault/doc3.md",
            "/vault/doc4.md",
            "/vault/doc5.md",
        ];

        for (i, path) in file_paths.iter().enumerate() {
            let event = DaemonEvent::with_correlation(
                EventType::Filesystem(FilesystemEventType::FileCreated {
                    path: path.to_string(),
                }),
                EventSource::filesystem("batch-watcher".to_string()),
                EventPayload::json(serde_json::json!({
                    "batch_id": "batch-001",
                    "file_index": i,
                    "total_files": file_paths.len()
                })),
                correlation_id,
            );

            router.route_event(event).await?;
        }

        println!("Batch of {} events routed", file_paths.len());

        Ok(())
    }

    /// Create priority-based events
    pub async fn priority_events_example() -> EventResult<()> {
        let router = DefaultEventRouter::new();
        register_example_services(&router).await?;

        // Critical: Security violation
        let critical_event = DaemonEvent::new(
            EventType::Service(ServiceEventType::RequestReceived {
                from_service: "security-monitor".to_string(),
                to_service: "daemon".to_string(),
                request: serde_json::json!({"violation": "unauthorized_access"}),
            }),
            EventSource::service("security-monitor".to_string()),
            EventPayload::json(serde_json::json!({
                "severity": "critical",
                "action_required": true
            })),
        )
        .with_priority(EventPriority::Critical);

        router.route_event(critical_event).await?;

        // High: Service failure
        let high_event = DaemonEvent::new(
            EventType::Service(ServiceEventType::ServiceStatusChanged {
                service_id: "database".to_string(),
                old_status: "healthy".to_string(),
                new_status: "degraded".to_string(),
            }),
            EventSource::service("database".to_string()),
            EventPayload::json(serde_json::json!({
                "error_rate": 0.15,
                "response_time_ms": 5000
            })),
        )
        .with_priority(EventPriority::High);

        router.route_event(high_event).await?;

        // Normal: Regular file change
        let normal_event = DaemonEvent::new(
            EventType::Filesystem(FilesystemEventType::FileModified {
                path: "/vault/regular-doc.md".to_string(),
            }),
            EventSource::filesystem("watcher-1".to_string()),
            EventPayload::json(serde_json::json!({"size": 2048})),
        )
        .with_priority(EventPriority::Normal);

        router.route_event(normal_event).await?;

        // Low: Metrics collection
        let low_event = DaemonEvent::new(
            EventType::System(SystemEventType::MetricsCollected {
                metrics: HashMap::from([
                    ("cpu_usage".to_string(), 0.45),
                    ("memory_usage".to_string(), 0.67),
                    ("disk_usage".to_string(), 0.23),
                ]),
            }),
            EventSource::system("metrics-collector".to_string()),
            EventPayload::json(serde_json::json!({"interval_seconds": 60})),
        )
        .with_priority(EventPriority::Low);

        router.route_event(low_event).await?;

        println!("Events with different priorities routed");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_event_routing() {
        basic_event_routing_example().await.unwrap();
    }

    #[tokio::test]
    async fn test_service_specific_events() {
        service_specific_events_example().await.unwrap();
    }

    #[tokio::test]
    async fn test_custom_routing_rules() {
        custom_routing_rules_example().await.unwrap();
    }

    #[tokio::test]
    async fn test_error_handling() {
        error_handling_example().await.unwrap();
    }

    #[tokio::test]
    async fn test_event_patterns() {
        patterns::event_chain_example().await.unwrap();
        patterns::batch_events_example().await.unwrap();
        patterns::priority_events_example().await.unwrap();
    }
}