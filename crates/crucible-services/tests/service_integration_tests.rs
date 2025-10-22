//! # Comprehensive Service Integration Tests
//!
//! This test suite validates that our 4 services (ScriptEngine, InferenceEngine, DataStore, McpGateway)
//! actually communicate correctly through the centralized event router. It tests event flow,
//! service coordination, load balancing, circuit breakers, error handling, and performance.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, RwLock, Mutex};
use uuid::Uuid;
use chrono::Utc;
use serde_json::{json, Value};

use crucible_services::{
    script_engine::{ScriptEngineService, ScriptEngineConfig, ScriptExecutionRequest, ScriptExecutionResponse},
    inference_engine::{InferenceEngineService, InferenceEngineConfig},
    data_store::{CrucibleDataStore, DataStoreConfig, DatabaseBackend, DatabaseBackendConfig},
    mcp_gateway::{McpGateway, McpGatewayConfig},
    events::{
        core::{DaemonEvent, EventType, EventPriority, EventPayload, EventSource},
        routing::{EventRouter, ServiceRegistration, LoadBalancingStrategy, CircuitBreakerConfig},
        mock::MockEventRouter,
        integration::{EventIntegrationManager, LifecycleEventType},
        errors::EventError,
    },
    service_traits::*,
    service_types::*,
    types::*,
};

/// Test fixture containing all services and infrastructure
struct ServiceTestSuite {
    event_router: Arc<MockEventRouter>,
    services: TestServices,
    event_collector: Arc<Mutex<Vec<DaemonEvent>>>,
}

struct TestServices {
    script_engine: Arc<RwLock<ScriptEngineService>>,
    inference_engine: Arc<RwLock<InferenceEngineService>>,
    data_store: Arc<RwLock<CrucibleDataStore>>,
    mcp_gateway: Arc<RwLock<McpGateway>>,
}

/// Configuration for test scenarios
#[derive(Debug, Clone)]
struct TestConfig {
    enable_circuit_breaker: bool,
    enable_load_balancing: bool,
    event_timeout_ms: u64,
    max_retries: u32,
    performance_test_events: usize,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            enable_circuit_breaker: true,
            enable_load_balancing: true,
            event_timeout_ms: 5000,
            max_retries: 3,
            performance_test_events: 1000,
        }
    }
}

impl ServiceTestSuite {
    async fn new(config: TestConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Create mock event router with custom configuration
        let event_router = Arc::new(MockEventRouter::new());
        let event_collector = Arc::new(Mutex::new(Vec::new()));

        // Configure circuit breaker if enabled
        if config.enable_circuit_breaker {
            let circuit_breaker_config = CircuitBreakerConfig {
                failure_threshold: 5,
                success_threshold: 3,
                timeout: Duration::from_secs(30),
                max_retries: config.max_retries,
            };
            event_router.configure_circuit_breaker(circuit_breaker_config).await?;
        }

        // Configure load balancing if enabled
        if config.enable_load_balancing {
            event_router.set_load_balancing_strategy(LoadBalancingStrategy::RoundRobin).await?;
        }

        // Create ScriptEngine service
        let script_config = ScriptEngineConfig {
            max_concurrent_scripts: 10,
            script_timeout_seconds: 30,
            cache_enabled: true,
            security_sandbox_enabled: true,
            default_permissions: vec!["read".to_string(), "execute".to_string()],
        };
        let mut script_engine = ScriptEngineService::new(script_config, event_router.clone()).await?;
        script_engine.initialize_event_integration(event_router.clone()).await?;
        let script_engine = Arc::new(RwLock::new(script_engine));

        // Create InferenceEngine service
        let inference_config = InferenceEngineConfig {
            text_provider: crucible_llm::TextProviderConfig::mock(),
            embedding_provider: crucible_llm::EmbeddingConfig::mock(),
            default_models: crucible_services::inference_engine::DefaultModels {
                text_model: "mock-text-model".to_string(),
                embedding_model: "mock-embedding-model".to_string(),
                chat_model: "mock-chat-model".to_string(),
            },
            performance: crucible_services::inference_engine::PerformanceSettings {
                enable_batching: true,
                batch_size: 4,
                batch_timeout_ms: 1000,
                enable_deduplication: true,
                connection_pool_size: 10,
                request_timeout_ms: 30000,
            },
            cache: crucible_services::inference_engine::CacheSettings {
                enabled: true,
                ttl_seconds: 3600,
                max_size_bytes: 1024 * 1024 * 100, // 100MB
                eviction_policy: crucible_services::inference_engine::CacheEvictionPolicy::LRU,
            },
            limits: crucible_services::inference_engine::InferenceLimits {
                max_concurrent_requests: Some(10),
                max_request_tokens: Some(4096),
                max_response_tokens: Some(2048),
                request_timeout: Some(Duration::from_secs(30)),
                max_queue_size: Some(100),
            },
            monitoring: crucible_services::inference_engine::MonitoringSettings {
                enable_metrics: true,
                metrics_interval_seconds: 60,
                enable_profiling: false,
                export_metrics: false,
            },
        };
        let mut inference_engine = InferenceEngineService::new(inference_config).await?;
        inference_engine.initialize_event_integration(event_router.clone()).await?;
        let inference_engine = Arc::new(RwLock::new(inference_engine));

        // Create DataStore service
        let data_store_config = DataStoreConfig {
            backend: DatabaseBackend::Memory,
            database_config: DatabaseBackendConfig::Memory(crucible_services::data_store::MemoryConfig {
                max_documents: Some(10000),
                persist_to_disk: Some(false),
                persistence_path: None,
            }),
            connection_pool: crucible_services::data_store::ConnectionPoolConfig::default(),
            performance: crucible_services::data_store::PerformanceConfig::default(),
            events: crucible_services::data_store::EventConfig::default(),
        };
        let mut data_store = CrucibleDataStore::new(data_store_config).await?;
        data_store.initialize_event_integration(event_router.clone()).await?;
        let data_store = Arc::new(RwLock::new(data_store));

        // Create McpGateway service
        let mcp_config = McpGatewayConfig::default();
        let mut mcp_gateway = McpGateway::new(mcp_config, event_router.clone())?;
        mcp_gateway.initialize_event_integration().await?;
        let mcp_gateway = Arc::new(RwLock::new(mcp_gateway));

        Ok(Self {
            event_router,
            services: TestServices {
                script_engine,
                inference_engine,
                data_store,
                mcp_gateway,
            },
            event_collector,
        })
    }

    async fn start_all_services(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.services.script_engine.write().await.start().await?;
        self.services.inference_engine.write().await.start().await?;
        self.services.data_store.write().await.start().await?;
        self.services.mcp_gateway.write().await.start().await?;
        Ok(())
    }

    async fn stop_all_services(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.services.script_engine.write().await.stop().await?;
        self.services.inference_engine.write().await.stop().await?;
        self.services.data_store.write().await.stop().await?;
        self.services.mcp_gateway.write().await.stop().await?;
        Ok(())
    }

    async fn wait_for_event_count(&self, expected_count: usize, timeout_ms: u64) -> bool {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout {
            let events = self.event_collector.lock().await.len();
            if events >= expected_count {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }

    async fn clear_events(&self) {
        self.event_collector.lock().await.clear();
        self.event_router.clear_events().await;
    }
}

// -------------------------------------------------------------------------
// Basic Event Flow Tests
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_basic_service_lifecycle_events() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let suite = ServiceTestSuite::new(TestConfig::default()).await?;

    // Start services and collect lifecycle events
    suite.start_all_services().await?;

    // Wait for lifecycle events to be published
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify that all services published startup events
    let events = suite.event_router.get_published_events().await;
    assert!(events.len() >= 4, "Expected at least 4 lifecycle events");

    // Check for service start events
    let start_events: Vec<_> = events.iter()
        .filter(|e| matches!(&e.event_type, EventType::Service(crucible_services::events::core::ServiceEventType::ServiceStart)))
        .collect();

    assert!(start_events.len() >= 4, "Expected at least 4 service start events");

    // Stop services and collect shutdown events
    suite.stop_all_services().await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let events = suite.event_router.get_published_events().await;
    let stop_events: Vec<_> = events.iter()
        .filter(|e| matches!(&e.event_type, EventType::Service(crucible_services::events::core::ServiceEventType::ServiceStop)))
        .collect();

    assert!(stop_events.len() >= 4, "Expected at least 4 service stop events");

    Ok(())
}

#[tokio::test]
async fn test_script_engine_event_publishing() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
    suite.start_all_services().await?;

    // Execute a script and verify event publishing
    let script_request = ScriptExecutionRequest {
        script_id: "test_script".to_string(),
        script_content: "print('Hello, World!')".to_string(),
        language: "python".to_string(),
        parameters: HashMap::new(),
        permissions: vec!["execute".to_string()],
        timeout_ms: Some(5000),
        request_id: Uuid::new_v4().to_string(),
    };

    let response = suite.services.script_engine.write().await
        .execute_script(script_request).await?;

    assert!(response.success);
    assert!(response.result.is_some());

    // Wait for event to be published
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify script execution event was published
    let events = suite.event_router.get_published_events().await;
    let script_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Custom(event_type) if event_type.contains("script"))
        })
        .collect();

    assert!(!script_events.is_empty(), "Expected script execution events to be published");

    Ok(())
}

#[tokio::test]
async fn test_datastore_crud_events() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
    suite.start_all_services().await?;

    // Create a document
    let document = DocumentData {
        id: DocumentId("test_doc".to_string()),
        content: json!({"title": "Test Document", "content": "This is a test"}),
        metadata: DocumentMetadata {
            document_type: Some("test".to_string()),
            tags: vec!["test".to_string()],
            author: Some("test_user".to_string()),
            content_hash: None,
            size_bytes: 100,
            custom: HashMap::new(),
        },
        version: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let created_id = suite.services.data_store.read().await
        .create("test_db", document.clone()).await?;

    assert_eq!(created_id, DocumentId("test_doc".to_string()));

    // Wait for event to be published
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify document created event was published
    let events = suite.event_router.get_published_events().await;
    let create_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Database(crucible_services::events::core::DatabaseEventType::RecordCreated { .. }))
        })
        .collect();

    assert!(!create_events.is_empty(), "Expected document created events to be published");

    // Update the document
    let updated_document = DocumentData {
        content: json!({"title": "Updated Document", "content": "This has been updated"}),
        updated_at: Utc::now(),
        ..document
    };

    suite.services.data_store.read().await
        .update("test_db", "test_doc", updated_document).await?;

    // Wait for event to be published
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify document updated event was published
    let events = suite.event_router.get_published_events().await;
    let update_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Database(crucible_services::events::core::DatabaseEventType::RecordUpdated { .. }))
        })
        .collect();

    assert!(!update_events.is_empty(), "Expected document updated events to be published");

    // Delete the document
    suite.services.data_store.read().await
        .delete("test_db", "test_doc").await?;

    // Wait for event to be published
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify document deleted event was published
    let events = suite.event_router.get_published_events().await;
    let delete_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Database(crucible_services::events::core::DatabaseEventType::RecordDeleted { .. }))
        })
        .collect();

    assert!(!delete_events.is_empty(), "Expected document deleted events to be published");

    Ok(())
}

#[tokio::test]
async fn test_mcp_gateway_session_events() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
    suite.start_all_services().await?;

    // Create an MCP session
    let client_capabilities = crucible_services::types::McpCapabilities {
        tools: Some(crucible_services::types::ToolCapabilities {
            list_tools: Some(true),
            call_tool: Some(true),
            subscribe_to_tools: Some(false),
        }),
        resources: None,
        logging: None,
        sampling: None,
    };

    let session = suite.services.mcp_gateway.read().await
        .initialize_connection("test_client", client_capabilities).await?;

    assert_eq!(session.client_id, "test_client");

    // Wait for event to be published
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify session created event was published
    let events = suite.event_router.get_published_events().await;
    let session_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Mcp(_))
        })
        .collect();

    assert!(!session_events.is_empty(), "Expected MCP session events to be published");

    // Close the session
    suite.services.mcp_gateway.read().await
        .close_connection(&session.session_id).await?;

    // Wait for event to be published
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify session closed event was published
    let events = suite.event_router.get_published_events().await;
    let close_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Mcp(crucible_services::events::core::McpEventType::ContextUpdated { .. }))
        })
        .collect();

    assert!(!close_events.is_empty(), "Expected MCP session close events to be published");

    Ok(())
}

// -------------------------------------------------------------------------
// Cross-Service Communication Tests
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_cross_service_event_routing() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
    suite.start_all_services().await?;

    // Create a custom event that should be routed to multiple services
    let cross_service_event = DaemonEvent {
        id: Uuid::new_v4(),
        event_type: EventType::Custom("cross_service_test".to_string()),
        priority: EventPriority::Normal,
        source: EventSource::Service("test_coordinator".to_string()),
        targets: vec!["script-engine".to_string(), "datastore".to_string(), "inference-engine".to_string()],
        created_at: Utc::now(),
        scheduled_at: None,
        payload: EventPayload::json(json!({
            "test_type": "cross_service_communication",
            "timestamp": Utc::now().to_rfc3339(),
            "correlation_id": Uuid::new_v4().to_string(),
        })),
        metadata: HashMap::new(),
        correlation_id: Some(Uuid::new_v4().to_string()),
        causation_id: None,
        retry_count: 0,
        max_retries: 3,
    };

    // Publish the event
    suite.event_router.publish(Box::new(cross_service_event)).await?;

    // Wait for event processing
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify the event was routed to all target services
    let events = suite.event_router.get_published_events().await;
    let response_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Custom(event_type) if event_type.contains("response"))
        })
        .collect();

    // We expect at least some response events from the services
    assert!(response_events.len() >= 1, "Expected response events from target services");

    Ok(())
}

#[tokio::test]
async fn test_service_discovery_via_events() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
    suite.start_all_services().await?;

    // Wait for initial registration events
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Check that all services published registration events
    let events = suite.event_router.get_published_events().await;
    let registration_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Service(crucible_services::events::core::ServiceEventType::ServiceRegistered { .. }))
        })
        .collect();

    assert!(registration_events.len() >= 4, "Expected registration events from all services");

    // Verify that each service type is represented
    let service_types: std::collections::HashSet<_> = registration_events.iter()
        .map(|e| {
            if let EventType::Service(crucible_services::events::core::ServiceEventType::ServiceRegistered { service_type, .. }) = &e.event_type {
                service_type.clone()
            } else {
                "unknown".to_string()
            }
        })
        .collect();

    assert!(service_types.contains("script-engine"), "Missing script-engine registration");
    assert!(service_types.contains("inference-engine"), "Missing inference-engine registration");
    assert!(service_types.contains("datastore"), "Missing datastore registration");
    assert!(service_types.contains("mcp-gateway"), "Missing mcp-gateway registration");

    Ok(())
}

#[tokio::test]
async fn test_event_priority_handling() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
    suite.start_all_services().await?;

    // Create events with different priorities
    let priorities = vec![
        EventPriority::Low,
        EventPriority::Normal,
        EventPriority::High,
        EventPriority::Critical,
    ];

    let mut event_ids = Vec::new();

    for (i, priority) in priorities.into_iter().enumerate() {
        let event = DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom(format!("priority_test_{}", i)),
            priority,
            source: EventSource::Service("test_coordinator".to_string()),
            targets: vec!["script-engine".to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "priority": format!("{:?}", priority),
                "index": i,
            })),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 3,
        };

        event_ids.push(event.id);
        suite.event_router.publish(Box::new(event)).await?;
    }

    // Wait for event processing
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify that critical and high priority events were processed first
    let events = suite.event_router.get_published_events().await;
    let priority_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Custom(event_type) if event_type.starts_with("priority_test"))
        })
        .collect();

    assert_eq!(priority_events.len(), 4, "Expected all priority test events to be processed");

    // Verify processing order (critical events should be processed first)
    let mut sorted_events = priority_events.clone();
    sorted_events.sort_by(|a, b| {
        // Sort by priority (Critical > High > Normal > Low)
        match (&a.priority, &b.priority) {
            (EventPriority::Critical, EventPriority::Critical) => std::cmp::Ordering::Equal,
            (EventPriority::Critical, _) => std::cmp::Ordering::Less,
            (_, EventPriority::Critical) => std::cmp::Ordering::Greater,
            (EventPriority::High, EventPriority::High) => std::cmp::Ordering::Equal,
            (EventPriority::High, _) => std::cmp::Ordering::Less,
            (_, EventPriority::High) => std::cmp::Ordering::Greater,
            (EventPriority::Normal, EventPriority::Normal) => std::cmp::Ordering::Equal,
            (EventPriority::Normal, EventPriority::Low) => std::cmp::Ordering::Less,
            (EventPriority::Low, EventPriority::Normal) => std::cmp::Ordering::Greater,
            (EventPriority::Low, EventPriority::Low) => std::cmp::Ordering::Equal,
        }
    });

    // Check that critical events appear earlier in the processed list
    let critical_index = priority_events.iter().position(|e| e.priority == EventPriority::Critical);
    let low_index = priority_events.iter().position(|e| e.priority == EventPriority::Low);

    if let (Some(critical_pos), Some(low_pos)) = (critical_index, low_index) {
        assert!(critical_pos < low_pos, "Critical events should be processed before low priority events");
    }

    Ok(())
}

// -------------------------------------------------------------------------
// Load Balancing Tests
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_load_balancing_round_robin() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = TestConfig {
        enable_load_balancing: true,
        ..Default::default()
    };
    let suite = ServiceTestSuite::new(config).await?;
    suite.start_all_services().await?;

    // Configure round-robin load balancing
    suite.event_router.set_load_balancing_strategy(
        crucible_services::events::routing::LoadBalancingStrategy::RoundRobin
    ).await?;

    // Create multiple service instances of the same type
    let service_registrations = vec![
        ServiceRegistration {
            service_id: "script-engine-1".to_string(),
            service_type: "script-engine".to_string(),
            instance_id: Some("instance-1".to_string()),
            address: None,
            port: None,
            protocol: "http".to_string(),
            metadata: HashMap::new(),
            health_check_url: None,
            capabilities: vec![],
            version: "1.0.0".to_string(),
            registered_at: Utc::now(),
        },
        ServiceRegistration {
            service_id: "script-engine-2".to_string(),
            service_type: "script-engine".to_string(),
            instance_id: Some("instance-2".to_string()),
            address: None,
            port: None,
            protocol: "http".to_string(),
            metadata: HashMap::new(),
            health_check_url: None,
            capabilities: vec![],
            version: "1.0.0".to_string(),
            registered_at: Utc::now(),
        },
        ServiceRegistration {
            service_id: "script-engine-3".to_string(),
            service_type: "script-engine".to_string(),
            instance_id: Some("instance-3".to_string()),
            address: None,
            port: None,
            protocol: "http".to_string(),
            metadata: HashMap::new(),
            health_check_url: None,
            capabilities: vec![],
            version: "1.0.0".to_string(),
            registered_at: Utc::now(),
        },
    ];

    for registration in service_registrations {
        suite.event_router.register_service(registration).await?;
    }

    // Send multiple events to test load balancing
    let mut event_ids = Vec::new();
    for i in 0..9 {
        let event = DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom(format!("load_balance_test_{}", i)),
            priority: EventPriority::Normal,
            source: EventSource::Service("test_client".to_string()),
            targets: vec!["script-engine".to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "test_index": i,
                "load_balance": true,
            })),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 3,
        };

        event_ids.push(event.id);
        suite.event_router.publish(Box::new(event)).await?;
    }

    // Wait for event processing
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Verify that events were distributed across instances
    let events = suite.event_router.get_published_events().await;
    let processed_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Custom(event_type) if event_type.starts_with("load_balance_test"))
        })
        .collect();

    assert_eq!(processed_events.len(), 9, "Expected all load balance test events to be processed");

    // Check distribution (should be roughly equal across instances)
    let mut instance_counts = HashMap::new();
    for event in processed_events {
        let instance_id = event.targets.first().cloned().unwrap_or_default();
        *instance_counts.entry(instance_id).or_insert(0) += 1;
    }

    // With round-robin and 9 events across 3 instances, each should get 3 events
    assert_eq!(instance_counts.len(), 3, "Expected events to be distributed across 3 instances");
    for count in instance_counts.values() {
        assert_eq!(*count, 3, "Each instance should receive exactly 3 events");
    }

    Ok(())
}

// -------------------------------------------------------------------------
// Circuit Breaker Tests
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_circuit_breaker_failure_threshold() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = TestConfig {
        enable_circuit_breaker: true,
        ..Default::default()
    };
    let suite = ServiceTestSuite::new(config).await?;
    suite.start_all_services().await?;

    // Configure circuit breaker with low threshold for testing
    let circuit_breaker_config = crucible_services::events::routing::CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        timeout: Duration::from_millis(500),
        max_retries: 2,
    };
    suite.event_router.configure_circuit_breaker(circuit_breaker_config).await?;

    // Simulate service failures by sending events that will fail
    let mut failure_count = 0;
    for i in 0..5 {
        let event = DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom(format!("failure_test_{}", i)),
            priority: EventPriority::Normal,
            source: EventSource::Service("test_client".to_string()),
            targets: vec!["nonexistent_service".to_string()], // This will cause failures
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "test_index": i,
                "should_fail": true,
            })),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 2,
        };

        match suite.event_router.publish(Box::new(event)).await {
            Ok(_) => {}
            Err(_) => failure_count += 1,
        }

        // Small delay between failures
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Verify that circuit breaker opened after failure threshold
    let circuit_breaker_state = suite.event_router.get_circuit_breaker_state().await;
    assert!(circuit_breaker_state.is_open, "Circuit breaker should be open after failures");
    assert!(failure_count >= 3, "Expected at least 3 failures to trigger circuit breaker");

    // Try to send more events - they should be rejected immediately
    let additional_event = DaemonEvent {
        id: Uuid::new_v4(),
        event_type: EventType::Custom("circuit_breaker_test".to_string()),
        priority: EventPriority::Normal,
        source: EventSource::Service("test_client".to_string()),
        targets: vec!["script-engine".to_string()],
        created_at: Utc::now(),
        scheduled_at: None,
        payload: EventPayload::json(json!({
            "test": "circuit_breaker_open",
        })),
        metadata: HashMap::new(),
        correlation_id: Some(Uuid::new_v4().to_string()),
        causation_id: None,
        retry_count: 0,
        max_retries: 2,
    };

    let result = suite.event_router.publish(Box::new(additional_event)).await;
    assert!(result.is_err(), "Events should be rejected when circuit breaker is open");

    // Wait for circuit breaker timeout
    tokio::time::sleep(Duration::from_millis(600)).await;

    // Verify circuit breaker is half-open
    let circuit_breaker_state = suite.event_router.get_circuit_breaker_state().await;
    assert!(circuit_breaker_state.is_half_open, "Circuit breaker should be half-open after timeout");

    Ok(())
}

#[tokio::test]
async fn test_circuit_breaker_recovery() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = TestConfig {
        enable_circuit_breaker: true,
        ..Default::default()
    };
    let suite = ServiceTestSuite::new(config).await?;
    suite.start_all_services().await?;

    // Configure circuit breaker
    let circuit_breaker_config = crucible_services::events::routing::CircuitBreakerConfig {
        failure_threshold: 2,
        success_threshold: 2,
        timeout: Duration::from_millis(300),
        max_retries: 1,
    };
    suite.event_router.configure_circuit_breaker(circuit_breaker_config).await?;

    // Trigger circuit breaker to open
    for i in 0..3 {
        let event = DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom(format!("trigger_failure_{}", i)),
            priority: EventPriority::Normal,
            source: EventSource::Service("test_client".to_string()),
            targets: vec!["nonexistent_service".to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({"trigger": "failure"})),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 1,
        };

        let _ = suite.event_router.publish(Box::new(event)).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Verify circuit breaker is open
    let circuit_breaker_state = suite.event_router.get_circuit_breaker_state().await;
    assert!(circuit_breaker_state.is_open, "Circuit breaker should be open");

    // Wait for timeout to enter half-open state
    tokio::time::sleep(Duration::from_millis(350)).await;

    // Send successful events to close circuit breaker
    for i in 0..3 {
        let event = DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom(format!("recovery_test_{}", i)),
            priority: EventPriority::Normal,
            source: EventSource::Service("test_client".to_string()),
            targets: vec!["script-engine".to_string()], // This service exists
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({"recovery": "success"})),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 1,
        };

        let _ = suite.event_router.publish(Box::new(event)).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Verify circuit breaker is closed again
    tokio::time::sleep(Duration::from_millis(200)).await;
    let circuit_breaker_state = suite.event_router.get_circuit_breaker_state().await;
    assert!(!circuit_breaker_state.is_open, "Circuit breaker should be closed after successful recovery");
    assert!(!circuit_breaker_state.is_half_open, "Circuit breaker should not be half-open after recovery");

    Ok(())
}

// -------------------------------------------------------------------------
// Performance Tests
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_event_throughput_performance() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = TestConfig {
        performance_test_events: 1000,
        ..Default::default()
    };
    let suite = ServiceTestSuite::new(config).await?;
    suite.start_all_services().await?;

    let event_count = 1000;
    let start_time = std::time::Instant::now();

    // Send a large number of events
    let mut handles = Vec::new();
    for batch in 0..10 {
        let event_router = suite.event_router.clone();
        let handle = tokio::spawn(async move {
            for i in 0..100 {
                let event = DaemonEvent {
                    id: Uuid::new_v4(),
                    event_type: EventType::Custom(format!("performance_test_{}_{}", batch, i)),
                    priority: EventPriority::Normal,
                    source: EventSource::Service("performance_test_client".to_string()),
                    targets: vec!["script-engine".to_string()],
                    created_at: Utc::now(),
                    scheduled_at: None,
                    payload: EventPayload::json(json!({
                        "batch": batch,
                        "index": i,
                        "performance": true,
                    })),
                    metadata: HashMap::new(),
                    correlation_id: Some(Uuid::new_v4().to_string()),
                    causation_id: None,
                    retry_count: 0,
                    max_retries: 1,
                };

                let _ = event_router.publish(Box::new(event)).await;
            }
        });
        handles.push(handle);
    }

    // Wait for all batches to complete
    for handle in handles {
        handle.await?;
    }

    let publish_time = start_time.elapsed();

    // Wait for all events to be processed
    tokio::time::sleep(Duration::from_millis(2000)).await;

    let total_time = start_time.elapsed();

    // Verify that most events were processed
    let events = suite.event_router.get_published_events().await;
    let performance_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Custom(event_type) if event_type.starts_with("performance_test"))
        })
        .collect();

    let processed_count = performance_events.len();
    let processing_rate = processed_count as f64 / total_time.as_secs_f64();
    let publishing_rate = event_count as f64 / publish_time.as_secs_f64();

    println!("Performance Test Results:");
    println!("  Events sent: {}", event_count);
    println!("  Events processed: {}", processed_count);
    println!("  Total time: {:?}", total_time);
    println!("  Publishing time: {:?}", publish_time);
    println!("  Processing rate: {:.2} events/sec", processing_rate);
    println!("  Publishing rate: {:.2} events/sec", publishing_rate);

    // Performance assertions
    assert!(processed_count >= event_count * 90 / 100, "At least 90% of events should be processed");
    assert!(processing_rate > 100.0, "Processing rate should be at least 100 events/sec");
    assert!(publishing_rate > 500.0, "Publishing rate should be at least 500 events/sec");

    Ok(())
}

#[tokio::test]
async fn test_memory_usage_under_load() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
    suite.start_all_services().await?;

    // Get initial memory usage
    let initial_metrics = suite.services.script_engine.read().await.get_metrics().await?;
    let initial_memory = initial_metrics.memory_usage;

    // Send a moderate number of events
    for i in 0..500 {
        let event = DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom(format!("memory_test_{}", i)),
            priority: EventPriority::Normal,
            source: EventSource::Service("memory_test_client".to_string()),
            targets: vec!["script-engine".to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "index": i,
                "data": "x".repeat(1000), // 1KB per event
            })),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 1,
        };

        let _ = suite.event_router.publish(Box::new(event)).await;

        // Small delay to prevent overwhelming the system
        if i % 50 == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    // Wait for processing
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Get final memory usage
    let final_metrics = suite.services.script_engine.read().await.get_metrics().await?;
    let final_memory = final_metrics.memory_usage;

    println!("Memory Usage Test Results:");
    println!("  Initial memory: {} bytes", initial_memory);
    println!("  Final memory: {} bytes", final_memory);
    println!("  Memory increase: {} bytes", final_memory - initial_memory);
    println!("  Memory per event: {:.2} bytes", (final_memory - initial_memory) as f64 / 500.0);

    // Memory should not increase excessively (allow for reasonable overhead)
    let memory_increase = final_memory.saturating_sub(initial_memory);
    assert!(memory_increase < 50 * 1024 * 1024, "Memory increase should be less than 50MB"); // 50MB limit

    // Clean up by clearing events
    suite.clear_events().await;

    // Memory should decrease after cleanup
    tokio::time::sleep(Duration::from_millis(500)).await;
    let cleanup_metrics = suite.services.script_engine.read().await.get_metrics().await?;
    let cleanup_memory = cleanup_metrics.memory_usage;

    println!("  Memory after cleanup: {} bytes", cleanup_memory);
    assert!(cleanup_memory <= final_memory, "Memory should not increase after cleanup");

    Ok(())
}

// -------------------------------------------------------------------------
// Error Handling and Recovery Tests
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_service_error_handling() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
    suite.start_all_services().await?;

    // Send an event that will cause an error
    let error_event = DaemonEvent {
        id: Uuid::new_v4(),
        event_type: EventType::Custom("error_test".to_string()),
        priority: EventPriority::Normal,
        source: EventSource::Service("error_test_client".to_string()),
        targets: vec!["script-engine".to_string()],
        created_at: Utc::now(),
        scheduled_at: None,
        payload: EventPayload::json(json!({
            "action": "cause_error",
            "error_type": "test_error",
        })),
        metadata: HashMap::new(),
        correlation_id: Some(Uuid::new_v4().to_string()),
        causation_id: None,
        retry_count: 0,
        max_retries: 3,
    };

    // This should handle the error gracefully
    let result = suite.event_router.publish(Box::new(error_event)).await;
    // The result might be Ok (error handled internally) or Err (error propagated)
    // Both are acceptable as long as the system doesn't crash

    // Wait for error handling
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify the service is still running and responsive
    let health = suite.services.script_engine.read().await.health_check().await?;
    assert!(health.status == ServiceStatus::Healthy || health.status == ServiceStatus::Degraded);

    // Send a normal event to verify the service is still working
    let normal_event = DaemonEvent {
        id: Uuid::new_v4(),
        event_type: EventType::Custom("recovery_test".to_string()),
        priority: EventPriority::Normal,
        source: EventSource::Service("recovery_test_client".to_string()),
        targets: vec!["script-engine".to_string()],
        created_at: Utc::now(),
        scheduled_at: None,
        payload: EventPayload::json(json!({
            "action": "normal_operation",
        })),
        metadata: HashMap::new(),
        correlation_id: Some(Uuid::new_v4().to_string()),
        causation_id: None,
        retry_count: 0,
        max_retries: 3,
    };

    let result = suite.event_router.publish(Box::new(normal_event)).await;
    assert!(result.is_ok(), "Service should handle normal events after error recovery");

    Ok(())
}

#[tokio::test]
async fn test_service_restart_with_event_preservation() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
    suite.start_all_services().await?;

    // Send some events to establish baseline
    for i in 0..10 {
        let event = DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom(format!("restart_test_{}", i)),
            priority: EventPriority::Normal,
            source: EventSource::Service("restart_test_client".to_string()),
            targets: vec!["datastore".to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "index": i,
                "before_restart": true,
            })),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 3,
        };

        suite.event_router.publish(Box::new(event)).await?;
    }

    // Wait for processing
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Get event count before restart
    let events_before = suite.event_router.get_published_events().await.len();

    // Restart one service
    suite.services.data_store.write().await.restart().await?;

    // Wait for restart to complete
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Verify the service is running again
    let health = suite.services.data_store.read().await.health_check().await?;
    assert!(health.status == ServiceStatus::Healthy);

    // Send more events after restart
    for i in 0..10 {
        let event = DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom(format!("restart_test_after_{}", i)),
            priority: EventPriority::Normal,
            source: EventSource::Service("restart_test_client".to_string()),
            targets: vec!["datastore".to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "index": i,
                "after_restart": true,
            })),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 3,
        };

        suite.event_router.publish(Box::new(event)).await?;
    }

    // Wait for processing
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify events are still being processed
    let events_after = suite.event_router.get_published_events().await.len();
    assert!(events_after > events_before, "New events should be processed after service restart");

    Ok(())
}

// -------------------------------------------------------------------------
// End-to-End Workflow Tests
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_complete_workflow_event_chain() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
    suite.start_all_services().await?;

    // 1. User creates a document in DataStore
    let document = DocumentData {
        id: DocumentId("workflow_test_doc".to_string()),
        content: json!({
            "title": "Workflow Test Document",
            "content": "This document triggers a complete workflow",
            "metadata": {"workflow_id": "test_workflow_123"}
        }),
        metadata: DocumentMetadata {
            document_type: Some("workflow_test".to_string()),
            tags: vec!["test".to_string(), "workflow".to_string()],
            author: Some("test_user".to_string()),
            content_hash: None,
            size_bytes: 200,
            custom: HashMap::new(),
        },
        version: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let created_id = suite.services.data_store.read().await
        .create("workflow_test_db", document).await?;

    // 2. ScriptEngine processes the document (simulated by event)
    let script_event = DaemonEvent {
        id: Uuid::new_v4(),
        event_type: EventType::Custom("document_processing_required".to_string()),
        priority: EventPriority::Normal,
        source: EventSource::Service("datastore".to_string()),
        targets: vec!["script-engine".to_string()],
        created_at: Utc::now(),
        scheduled_at: None,
        payload: EventPayload::json(json!({
            "document_id": created_id,
            "processing_type": "extract_keywords",
        })),
        metadata: HashMap::new(),
        correlation_id: Some(Uuid::new_v4().to_string()),
        causation_id: None,
        retry_count: 0,
        max_retries: 3,
    };

    suite.event_router.publish(Box::new(script_event)).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 3. InferenceEngine generates embeddings for the document
    let inference_event = DaemonEvent {
        id: Uuid::new_v4(),
        event_type: EventType::Custom("embedding_generation_required".to_string()),
        priority: EventPriority::Normal,
        source: EventSource::Service("script-engine".to_string()),
        targets: vec!["inference-engine".to_string()],
        created_at: Utc::now(),
        scheduled_at: None,
        payload: EventPayload::json(json!({
            "document_id": created_id,
            "text": "This document triggers a complete workflow",
        })),
        metadata: HashMap::new(),
        correlation_id: Some(Uuid::new_v4().to_string()),
        causation_id: None,
        retry_count: 0,
        max_retries: 3,
    };

    suite.event_router.publish(Box::new(inference_event)).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 4. McpGateway makes the document available via MCP
    let mcp_event = DaemonEvent {
        id: Uuid::new_v4(),
        event_type: EventType::Mcp(crucible_services::events::core::McpEventType::ToolCall {
            tool_name: "register_document".to_string(),
            parameters: json!({
                "document_id": created_id,
                "access_level": "public",
            }),
        }),
        priority: EventPriority::Normal,
        source: EventSource::Service("inference-engine".to_string()),
        targets: vec!["mcp-gateway".to_string()],
        created_at: Utc::now(),
        scheduled_at: None,
        payload: EventPayload::json(json!({
            "action": "register_document_for_mcp",
            "document_id": created_id,
        })),
        metadata: HashMap::new(),
        correlation_id: Some(Uuid::new_v4().to_string()),
        causation_id: None,
        retry_count: 0,
        max_retries: 3,
    };

    suite.event_router.publish(Box::new(mcp_event)).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify the complete workflow was executed
    let events = suite.event_router.get_published_events().await;

    // Check for workflow events
    let workflow_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Custom(event_type)
                if event_type.contains("workflow") || event_type.contains("processing") || event_type.contains("embedding"))
        })
        .collect();

    assert!(workflow_events.len() >= 2, "Expected multiple workflow events to be published");

    // Check for database events
    let db_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Database(crucible_services::events::core::DatabaseEventType::RecordCreated { .. }))
        })
        .collect();

    assert!(!db_events.is_empty(), "Expected database events from document creation");

    // Check for MCP events
    let mcp_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Mcp(_))
        })
        .collect();

    assert!(!mcp_events.is_empty(), "Expected MCP events from document registration");

    // Verify correlation IDs are preserved throughout the workflow
    let correlation_ids: std::collections::HashSet<_> = events.iter()
        .filter_map(|e| e.correlation_id.as_ref())
        .collect();

    assert!(!correlation_ids.is_empty(), "Expected correlation IDs to be preserved");

    Ok(())
}

#[tokio::test]
async fn test_concurrent_service_coordination() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
    suite.start_all_services().await?;

    let task_count = 20;
    let correlation_id = Uuid::new_v4().to_string();

    // Create multiple concurrent tasks that involve different services
    let mut handles = Vec::new();

    for i in 0..task_count {
        let correlation_id = correlation_id.clone();
        let event_router = suite.event_router.clone();

        let handle = tokio::spawn(async move {
            let task_type = match i % 4 {
                0 => "datastore_task",
                1 => "script_engine_task",
                2 => "inference_engine_task",
                _ => "mcp_gateway_task",
            };

            let target_service = match task_type {
                "datastore_task" => "datastore",
                "script_engine_task" => "script-engine",
                "inference_engine_task" => "inference-engine",
                _ => "mcp-gateway",
            };

            let event = DaemonEvent {
                id: Uuid::new_v4(),
                event_type: EventType::Custom(format!("concurrent_{}_{}", task_type, i)),
                priority: EventPriority::Normal,
                source: EventSource::Service("concurrency_test_client".to_string()),
                targets: vec![target_service.to_string()],
                created_at: Utc::now(),
                scheduled_at: None,
                payload: EventPayload::json(json!({
                    "task_index": i,
                    "task_type": task_type,
                    "correlation_group": "concurrent_test",
                })),
                metadata: HashMap::new(),
                correlation_id: Some(correlation_id),
                causation_id: None,
                retry_count: 0,
                max_retries: 3,
            };

            event_router.publish(Box::new(event)).await
        });

        handles.push(handle);
    }

    // Wait for all concurrent tasks to complete
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await?);
    }

    // Check that most tasks succeeded
    let success_count = results.iter().filter(|r| r.is_ok()).count();
    assert!(success_count >= task_count * 90 / 100, "At least 90% of concurrent tasks should succeed");

    // Wait for event processing
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify events from all services were processed
    let events = suite.event_router.get_published_events().await;
    let concurrent_events: Vec<_> = events.iter()
        .filter(|e| {
            matches!(&e.event_type, EventType::Custom(event_type) if event_type.starts_with("concurrent_"))
        })
        .collect();

    assert!(concurrent_events.len() >= task_count * 90 / 100, "Most concurrent events should be processed");

    // Verify correlation ID grouping worked
    let correlation_groups: HashMap<String, usize> = concurrent_events.iter()
        .filter_map(|e| e.correlation_id.as_ref())
        .fold(HashMap::new(), |mut acc, id| {
            *acc.entry(id.clone()).or_insert(0) += 1;
            acc
        });

    assert!(correlation_groups.contains_key(&correlation_id), "Expected correlation group to exist");
    assert_eq!(correlation_groups[&correlation_id], task_count, "All events should have the same correlation ID");

    Ok(())
}