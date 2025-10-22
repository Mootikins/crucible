//! Test Utilities and Helper Functions
//!
//! This module provides common utilities and helper functions for testing
//! the service integration and event handling functionality.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, RwLock, Mutex};
use uuid::Uuid;
use chrono::Utc;
use serde_json::{json, Value};

use crucible_services::{
    events::{
        core::{DaemonEvent, EventType, EventPriority, EventPayload, EventSource},
        routing::{ServiceRegistration, LoadBalancingStrategy, CircuitBreakerConfig},
    },
    service_types::*,
    types::*,
};

use super::mock_services::{MockEventRouter, MockScriptEngine, MockDataStore, MockInferenceEngine, MockMcpGateway, FailureSimulation};

/// Test configuration builder
#[derive(Debug, Clone)]
pub struct TestConfigBuilder {
    enable_circuit_breaker: bool,
    enable_load_balancing: bool,
    circuit_breaker_config: Option<CircuitBreakerConfig>,
    load_balancing_strategy: LoadBalancingStrategy,
    failure_simulation: Option<FailureSimulation>,
    event_timeout_ms: u64,
    performance_test_events: usize,
}

impl Default for TestConfigBuilder {
    fn default() -> Self {
        Self {
            enable_circuit_breaker: false,
            enable_load_balancing: false,
            circuit_breaker_config: None,
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
            failure_simulation: None,
            event_timeout_ms: 5000,
            performance_test_events: 100,
        }
    }
}

impl TestConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_circuit_breaker(mut self, enabled: bool) -> Self {
        self.enable_circuit_breaker = enabled;
        if enabled && self.circuit_breaker_config.is_none() {
            self.circuit_breaker_config = Some(CircuitBreakerConfig::default());
        }
        self
    }

    pub fn with_circuit_breaker_config(mut self, config: CircuitBreakerConfig) -> Self {
        self.circuit_breaker_config = Some(config);
        self.enable_circuit_breaker = true;
        self
    }

    pub fn with_load_balancing(mut self, enabled: bool) -> Self {
        self.enable_load_balancing = enabled;
        self
    }

    pub fn with_load_balancing_strategy(mut self, strategy: LoadBalancingStrategy) -> Self {
        self.load_balancing_strategy = strategy;
        self.enable_load_balancing = true;
        self
    }

    pub fn with_failure_simulation(mut self, simulation: FailureSimulation) -> Self {
        self.failure_simulation = Some(simulation);
        self
    }

    pub fn with_event_timeout(mut self, timeout_ms: u64) -> Self {
        self.event_timeout_ms = timeout_ms;
        self
    }

    pub fn with_performance_test_events(mut self, count: usize) -> Self {
        self.performance_test_events = count;
        self
    }

    pub fn build(self) -> TestConfig {
        TestConfig {
            enable_circuit_breaker: self.enable_circuit_breaker,
            enable_load_balancing: self.enable_load_balancing,
            circuit_breaker_config: self.circuit_breaker_config,
            load_balancing_strategy: self.load_balancing_strategy,
            failure_simulation: self.failure_simulation,
            event_timeout_ms: self.event_timeout_ms,
            performance_test_events: self.performance_test_events,
        }
    }
}

/// Final test configuration
#[derive(Debug, Clone)]
pub struct TestConfig {
    pub enable_circuit_breaker: bool,
    pub enable_load_balancing: bool,
    pub circuit_breaker_config: Option<CircuitBreakerConfig>,
    pub load_balancing_strategy: LoadBalancingStrategy,
    pub failure_simulation: Option<FailureSimulation>,
    pub event_timeout_ms: u64,
    pub performance_test_events: usize,
}

/// Test fixture containing all mock services
pub struct MockServiceTestSuite {
    pub event_router: Arc<MockEventRouter>,
    pub script_engine: Arc<MockScriptEngine>,
    pub data_store: Arc<MockDataStore>,
    pub inference_engine: Arc<MockInferenceEngine>,
    pub mcp_gateway: Arc<MockMcpGateway>,
}

impl MockServiceTestSuite {
    pub async fn new(config: TestConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let event_router = Arc::new(MockEventRouter::new());

        // Configure circuit breaker if enabled
        if let Some(circuit_config) = config.circuit_breaker_config {
            event_router.configure_circuit_breaker(circuit_config).await?;
        }

        // Configure load balancing if enabled
        if config.enable_load_balancing {
            event_router.set_load_balancing_strategy(config.load_balancing_strategy).await?;
        }

        // Create mock services
        let script_engine = Arc::new(MockScriptEngine::new());
        let data_store = Arc::new(MockDataStore::new());
        let inference_engine = Arc::new(MockInferenceEngine::new());
        let mcp_gateway = Arc::new(MockMcpGateway::new());

        // Configure failure simulation if specified
        if let Some(failure_sim) = config.failure_simulation {
            event_router.configure_failure_simulation(failure_sim.clone()).await;
            script_engine.configure_failure_simulation(failure_sim.clone()).await;
            data_store.configure_failure_simulation(failure_sim.clone()).await;
            inference_engine.configure_failure_simulation(failure_sim.clone()).await;
            mcp_gateway.configure_failure_simulation(failure_sim).await;
        }

        // Register mock services
        self::register_mock_services(&event_router).await?;

        Ok(Self {
            event_router,
            script_engine,
            data_store,
            inference_engine,
            mcp_gateway,
        })
    }

    async fn register_mock_services(event_router: &MockEventRouter) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let services = vec![
            ServiceRegistration {
                service_id: "script-engine".to_string(),
                service_type: "script-engine".to_string(),
                instance_id: Some("mock-instance-1".to_string()),
                address: None,
                port: None,
                protocol: "mock".to_string(),
                metadata: HashMap::new(),
                health_check_url: None,
                capabilities: vec!["script_execution".to_string()],
                version: "1.0.0-mock".to_string(),
                registered_at: Utc::now(),
            },
            ServiceRegistration {
                service_id: "datastore".to_string(),
                service_type: "datastore".to_string(),
                instance_id: Some("mock-instance-1".to_string()),
                address: None,
                port: None,
                protocol: "mock".to_string(),
                metadata: HashMap::new(),
                health_check_url: None,
                capabilities: vec!["document_storage".to_string()],
                version: "1.0.0-mock".to_string(),
                registered_at: Utc::now(),
            },
            ServiceRegistration {
                service_id: "inference-engine".to_string(),
                service_type: "inference-engine".to_string(),
                instance_id: Some("mock-instance-1".to_string()),
                address: None,
                port: None,
                protocol: "mock".to_string(),
                metadata: HashMap::new(),
                health_check_url: None,
                capabilities: vec!["text_generation".to_string(), "embeddings".to_string()],
                version: "1.0.0-mock".to_string(),
                registered_at: Utc::now(),
            },
            ServiceRegistration {
                service_id: "mcp-gateway".to_string(),
                service_type: "mcp-gateway".to_string(),
                instance_id: Some("mock-instance-1".to_string()),
                address: None,
                port: None,
                protocol: "mock".to_string(),
                metadata: HashMap::new(),
                health_check_url: None,
                capabilities: vec!["mcp_protocol".to_string()],
                version: "1.0.0-mock".to_string(),
                registered_at: Utc::now(),
            },
        ];

        for service in services {
            event_router.register_service(service).await?;
        }

        Ok(())
    }

    pub async fn clear_all_history(&self) {
        self.event_router.clear_events().await;
        self.script_engine.clear_history().await;
        self.data_store.clear_history().await;
        self.inference_engine.clear_history().await;
        self.mcp_gateway.clear_history().await;
    }

    pub async fn wait_for_event_count(&self, expected_count: usize, timeout_ms: u64) -> bool {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout {
            let events = self.event_router.get_published_events().await.len();
            if events >= expected_count {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }
}

/// Event factory for creating test events
pub struct EventFactory;

impl EventFactory {
    pub fn create_script_execution_event(script_id: &str, script_content: &str) -> DaemonEvent {
        DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom("script_execution_request".to_string()),
            priority: EventPriority::Normal,
            source: EventSource::Service("test_client".to_string()),
            targets: vec!["script-engine".to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "script_id": script_id,
                "script_content": script_content,
                "language": "python",
                "parameters": {},
                "timeout_ms": 5000,
            })),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 3,
        }
    }

    pub fn create_document_creation_event(database: &str, document: &DocumentData) -> DaemonEvent {
        DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom("document_creation_request".to_string()),
            priority: EventPriority::Normal,
            source: EventSource::Service("test_client".to_string()),
            targets: vec!["datastore".to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "database": database,
                "document": document,
                "operation": "create",
            })),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 3,
        }
    }

    pub fn create_inference_event(model: &str, prompt: &str) -> DaemonEvent {
        DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom("inference_request".to_string()),
            priority: EventPriority::Normal,
            source: EventSource::Service("test_client".to_string()),
            targets: vec!["inference-engine".to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "model": model,
                "prompt": prompt,
                "request_type": "completion",
                "max_tokens": 100,
                "temperature": 0.7,
            })),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 3,
        }
    }

    pub fn create_embedding_event(model: &str, text: &str) -> DaemonEvent {
        DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom("embedding_request".to_string()),
            priority: EventPriority::Normal,
            source: EventSource::Service("test_client".to_string()),
            targets: vec!["inference-engine".to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "model": model,
                "input": text,
                "request_type": "embedding",
            })),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 3,
        }
    }

    pub fn create_mcp_session_event(client_id: &str) -> DaemonEvent {
        DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Mcp(crucible_services::events::core::McpEventType::ContextUpdated {
                context_id: "session".to_string(),
                changes: HashMap::from([
                    ("action".to_string(), serde_json::Value::String("create".to_string())),
                    ("client_id".to_string(), serde_json::Value::String(client_id.to_string())),
                ]),
            }),
            priority: EventPriority::Normal,
            source: EventSource::Service("test_client".to_string()),
            targets: vec!["mcp-gateway".to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "client_id": client_id,
                "action": "create_session",
                "capabilities": {
                    "tools": {"list_tools": true, "call_tool": true},
                    "resources": None,
                    "logging": None,
                    "sampling": None,
                },
            })),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 3,
        }
    }

    pub fn create_cross_service_event(event_type: &str, targets: Vec<String>) -> DaemonEvent {
        DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom(event_type.to_string()),
            priority: EventPriority::Normal,
            source: EventSource::Service("test_coordinator".to_string()),
            targets,
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "test_type": "cross_service_communication",
                "timestamp": Utc::now().to_rfc3339(),
            })),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 3,
        }
    }

    pub fn create_performance_test_event(batch: usize, index: usize) -> DaemonEvent {
        DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom(format!("performance_test_{}_{}", batch, index)),
            priority: EventPriority::Normal,
            source: EventSource::Service("performance_test_client".to_string()),
            targets: vec!["script-engine".to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "batch": batch,
                "index": index,
                "performance": true,
                "timestamp": Utc::now().to_rfc3339(),
            })),
            metadata: HashMap::new(),
            correlation_id: Some(format!("perf_batch_{}", batch)),
            causation_id: None,
            retry_count: 0,
            max_retries: 1,
        }
    }
}

/// Test data factory for creating common test objects
pub struct TestDataFactory;

impl TestDataFactory {
    pub fn create_test_document(id: &str, title: &str, content: &str) -> DocumentData {
        DocumentData {
            id: DocumentId(id.to_string()),
            content: json!({
                "title": title,
                "content": content,
                "created_by": "test_suite",
                "tags": ["test", "mock"]
            }),
            metadata: DocumentMetadata {
                document_type: Some("test_document".to_string()),
                tags: vec!["test".to_string(), "mock".to_string()],
                author: Some("test_user".to_string()),
                content_hash: None,
                size_bytes: content.len() as u64,
                custom: HashMap::from([
                    ("test_id".to_string(), id.to_string()),
                    ("test_suite".to_string(), "crucible_services".to_string()),
                ]),
            },
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub fn create_test_script_request(script_id: &str, script_content: &str) -> ScriptExecutionRequest {
        ScriptExecutionRequest {
            script_id: script_id.to_string(),
            script_content: script_content.to_string(),
            language: "python".to_string(),
            parameters: HashMap::new(),
            permissions: vec!["execute".to_string(), "read".to_string()],
            timeout_ms: Some(5000),
            request_id: Uuid::new_v4().to_string(),
        }
    }

    pub fn create_test_completion_request(model: &str, prompt: &str) -> CompletionRequest {
        CompletionRequest {
            model: model.to_string(),
            prompt: prompt.to_string(),
            max_tokens: Some(100),
            temperature: Some(0.7),
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            stop: None,
            n: Some(1),
            echo: None,
            logit_bias: None,
            user: None,
        }
    }

    pub fn create_test_embedding_request(model: &str, text: &str) -> EmbeddingRequest {
        EmbeddingRequest {
            model: model.to_string(),
            input: EmbeddingInput::String(text.to_string()),
            request_id: Uuid::new_v4().to_string(),
        }
    }

    pub fn create_test_mcp_capabilities() -> McpCapabilities {
        McpCapabilities {
            tools: Some(ToolCapabilities {
                list_tools: Some(true),
                call_tool: Some(true),
                subscribe_to_tools: Some(false),
            }),
            resources: None,
            logging: Some(LoggingCapabilities {
                set_log_level: Some(false),
                get_log_messages: Some(false),
            }),
            sampling: Some(SamplingCapabilities {
                create_message: Some(false),
            }),
        }
    }

    pub fn create_test_tool_definition(name: &str, description: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Input for the tool"
                    }
                },
                "required": ["input"]
            }),
            category: Some("test".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("test_suite".to_string()),
            tags: vec!["test".to_string()],
            enabled: true,
            parameters: vec![],
        }
    }
}

/// Performance measurement utilities
pub struct PerformanceTracker {
    measurements: Arc<Mutex<Vec<PerformanceMeasurement>>>,
}

#[derive(Debug, Clone)]
pub struct PerformanceMeasurement {
    pub name: String,
    pub duration: Duration,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: HashMap<String, String>,
}

impl PerformanceTracker {
    pub fn new() -> Self {
        Self {
            measurements: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn measure<F, Fut>(&self, name: &str, operation: F) -> Result<Fut::Output, Box<dyn std::error::Error + Send + Sync>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future,
    {
        let start_time = std::time::Instant::now();
        let result = operation().await;
        let duration = start_time.elapsed();

        let measurement = PerformanceMeasurement {
            name: name.to_string(),
            duration,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        };

        self.measurements.lock().await.push(measurement);
        Ok(result)
    }

    pub async fn get_measurements(&self) -> Vec<PerformanceMeasurement> {
        self.measurements.lock().await.clone()
    }

    pub async fn clear_measurements(&self) {
        self.measurements.lock().await.clear();
    }

    pub async fn get_statistics(&self) -> PerformanceStatistics {
        let measurements = self.measurements.lock().await;
        if measurements.is_empty() {
            return PerformanceStatistics::default();
        }

        let durations: Vec<Duration> = measurements.iter().map(|m| m.duration).collect();
        let total_duration: Duration = durations.iter().sum();
        let average_duration = total_duration / durations.len() as u32;
        let min_duration = *durations.iter().min().unwrap();
        let max_duration = *durations.iter().max().unwrap();

        PerformanceStatistics {
            count: measurements.len(),
            total_duration,
            average_duration,
            min_duration,
            max_duration,
        }
    }
}

#[derive(Debug, Default)]
pub struct PerformanceStatistics {
    pub count: usize,
    pub total_duration: Duration,
    pub average_duration: Duration,
    pub min_duration: Duration,
    pub max_duration: Duration,
}

/// Assertion utilities for test validation
pub struct TestAssertions;

impl TestAssertions {
    pub fn assert_event_received(events: &[DaemonEvent], expected_event_type: &str) -> bool {
        events.iter().any(|e| {
            matches!(&e.event_type, EventType::Custom(event_type) if event_type.contains(expected_event_type))
        })
    }

    pub fn assert_events_with_correlation(events: &[DaemonEvent], correlation_id: &str) -> Vec<&DaemonEvent> {
        events.iter()
            .filter(|e| e.correlation_id.as_ref().map_or(false, |id| id == correlation_id))
            .collect()
    }

    pub fn assert_service_responded(events: &[DaemonEvent], service_id: &str) -> bool {
        events.iter().any(|e| e.source.to_string().contains(service_id))
    }

    pub fn assert_event_order(events: &[DaemonEvent], expected_order: &[&str]) -> bool {
        if events.len() < expected_order.len() {
            return false;
        }

        for (i, expected_type) in expected_order.iter().enumerate() {
            match &events[i].event_type {
                EventType::Custom(event_type) if event_type.contains(expected_type) => continue,
                EventType::Mcp(_) if expected_type.contains("mcp") => continue,
                EventType::Database(_) if expected_type.contains("database") => continue,
                EventType::Service(_) if expected_type.contains("service") => continue,
                _ => return false,
            }
        }

        true
    }

    pub fn count_events_by_type(events: &[DaemonEvent], event_type_pattern: &str) -> usize {
        events.iter().filter(|e| {
            match &e.event_type {
                EventType::Custom(event_type) => event_type.contains(event_type_pattern),
                EventType::Mcp(_) => event_type_pattern.contains("mcp"),
                EventType::Database(_) => event_type_pattern.contains("database"),
                EventType::Service(_) => event_type_pattern.contains("service"),
                _ => false,
            }
        }).count()
    }
}