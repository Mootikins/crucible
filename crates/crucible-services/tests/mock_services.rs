//! Mock Service Implementations for Testing
//!
//! This module provides mock implementations of services and event router components
//! specifically designed for testing scenarios.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{mpsc, RwLock, Mutex};
use uuid::Uuid;
use chrono::Utc;
use serde_json::{json, Value};

use crucible_services::{
    events::{
        core::{DaemonEvent, EventType, EventPriority, EventPayload, EventSource},
        routing::{EventRouter, ServiceRegistration, LoadBalancingStrategy, CircuitBreakerConfig, CircuitBreakerState},
        errors::{EventError, EventResult},
    },
    service_types::*,
    types::*,
};

/// Mock event router for testing
#[derive(Debug)]
pub struct MockEventRouter {
    events: Arc<Mutex<Vec<DaemonEvent>>>,
    services: Arc<RwLock<HashMap<String, ServiceRegistration>>>,
    circuit_breaker_state: Arc<RwLock<CircuitBreakerState>>,
    load_balancing_strategy: Arc<RwLock<LoadBalancingStrategy>>,
    failure_simulation: Arc<RwLock<FailureSimulation>>,
}

/// Configuration for simulating failures
#[derive(Debug, Clone)]
pub struct FailureSimulation {
    pub enabled: bool,
    pub failure_rate: f64, // 0.0 to 1.0
    pub failure_types: Vec<String>,
    pub affected_services: Vec<String>,
}

impl Default for FailureSimulation {
    fn default() -> Self {
        Self {
            enabled: false,
            failure_rate: 0.0,
            failure_types: vec![],
            affected_services: vec![],
        }
    }
}

impl MockEventRouter {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            services: Arc::new(RwLock::new(HashMap::new())),
            circuit_breaker_state: Arc::new(RwLock::new(CircuitBreakerState::default())),
            load_balancing_strategy: Arc::new(RwLock::new(LoadBalancingStrategy::RoundRobin)),
            failure_simulation: Arc::new(RwLock::new(FailureSimulation::default())),
        }
    }

    pub async fn get_published_events(&self) -> Vec<DaemonEvent> {
        self.events.lock().await.clone()
    }

    pub async fn clear_events(&self) {
        self.events.lock().await.clear();
    }

    pub async fn configure_circuit_breaker(&self, config: CircuitBreakerConfig) -> EventResult<()> {
        let mut state = self.circuit_breaker_state.write().await;
        state.config = Some(config);
        Ok(())
    }

    pub async fn get_circuit_breaker_state(&self) -> CircuitBreakerState {
        self.circuit_breaker_state.read().await.clone()
    }

    pub async fn set_load_balancing_strategy(&self, strategy: LoadBalancingStrategy) -> EventResult<()> {
        let mut current = self.load_balancing_strategy.write().await;
        *current = strategy;
        Ok(())
    }

    pub async fn configure_failure_simulation(&self, simulation: FailureSimulation) {
        let mut failure_sim = self.failure_simulation.write().await;
        *failure_sim = simulation;
    }

    async fn should_simulate_failure(&self, target: &str) -> bool {
        let failure_sim = self.failure_simulation.read().await;
        if !failure_sim.enabled {
            return false;
        }

        // Check if target service is affected
        if !failure_sim.affected_services.is_empty() && !failure_sim.affected_services.contains(&target.to_string()) {
            return false;
        }

        // Random failure based on failure rate
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < failure_sim.failure_rate
    }

    async fn route_to_service(&self, event: &DaemonEvent, service_id: &str) -> EventResult<()> {
        if self.should_simulate_failure(service_id).await {
            return Err(EventError::RoutingError(format!("Simulated failure for service: {}", service_id)));
        }

        // Store the event as "processed"
        let mut events = self.events.lock().await;
        events.push(event.clone());

        // Create a response event
        let response_event = DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom(format!("{}_response", event.event_type.to_string())),
            priority: event.priority,
            source: EventSource::Service(service_id.to_string()),
            targets: vec![],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "original_event_id": event.id,
                "processed_by": service_id,
                "timestamp": Utc::now().to_rfc3339(),
            })),
            metadata: HashMap::new(),
            correlation_id: event.correlation_id.clone(),
            causation_id: Some(event.id.to_string()),
            retry_count: 0,
            max_retries: 3,
        };

        events.push(response_event);
        Ok(())
    }
}

#[async_trait]
impl EventRouter for MockEventRouter {
    async fn publish(&self, event: Box<DaemonEvent>) -> EventResult<()> {
        // Check circuit breaker
        let circuit_state = self.circuit_breaker_state.read().await;
        if circuit_state.is_open {
            return Err(EventError::CircuitBreakerOpen("Circuit breaker is open".to_string()));
        }
        drop(circuit_state);

        // Route to target services
        for target in &event.targets {
            if let Err(e) = self.route_to_service(&event, target).await {
                // Update circuit breaker state on failure
                let mut state = self.circuit_breaker_state.write().await;
                state.failure_count += 1;

                if let Some(config) = &state.config {
                    if state.failure_count >= config.failure_threshold {
                        state.is_open = true;
                        state.last_failure_time = Some(Utc::now());
                    }
                }

                return Err(e);
            }
        }

        // Update circuit breaker on success
        let mut state = self.circuit_breaker_state.write().await;
        state.success_count += 1;

        if let Some(config) = &state.config {
            if state.is_open && state.success_count >= config.success_threshold {
                state.is_open = false;
                state.is_half_open = false;
                state.failure_count = 0;
                state.success_count = 0;
            }
        }

        Ok(())
    }

    async fn register_service(&self, registration: ServiceRegistration) -> EventResult<()> {
        let mut services = self.services.write().await;
        services.insert(registration.service_id.clone(), registration);
        Ok(())
    }

    async fn unregister_service(&self, service_id: &str) -> EventResult<()> {
        let mut services = self.services.write().await;
        services.remove(service_id);
        Ok(())
    }

    async fn get_service_registration(&self, service_id: &str) -> EventResult<Option<ServiceRegistration>> {
        let services = self.services.read().await;
        Ok(services.get(service_id).cloned())
    }

    async fn list_services(&self) -> EventResult<Vec<ServiceRegistration>> {
        let services = self.services.read().await;
        Ok(services.values().cloned().collect())
    }

    async fn get_service_health(&self, _service_id: &str) -> EventResult<ServiceHealth> {
        Ok(ServiceHealth {
            status: ServiceStatus::Healthy,
            message: Some("Mock service is healthy".to_string()),
            last_check: Utc::now(),
            response_time: Duration::from_millis(10),
            resource_usage: None,
            details: HashMap::new(),
        })
    }
}

/// Mock ScriptEngine service for testing
pub struct MockScriptEngine {
    execution_history: Arc<Mutex<Vec<ScriptExecutionRecord>>>,
    failure_simulation: Arc<RwLock<FailureSimulation>>,
}

#[derive(Debug, Clone)]
struct ScriptExecutionRecord {
    request_id: String,
    script_id: String,
    execution_time: Duration,
    success: bool,
    timestamp: chrono::DateTime<chrono::Utc>,
}

impl MockScriptEngine {
    pub fn new() -> Self {
        Self {
            execution_history: Arc::new(Mutex::new(Vec::new())),
            failure_simulation: Arc::new(RwLock::new(FailureSimulation::default())),
        }
    }

    pub async fn configure_failure_simulation(&self, simulation: FailureSimulation) {
        let mut failure_sim = self.failure_simulation.write().await;
        *failure_sim = simulation;
    }

    pub async fn get_execution_history(&self) -> Vec<ScriptExecutionRecord> {
        self.execution_history.lock().await.clone()
    }

    pub async fn clear_history(&self) {
        self.execution_history.lock().await.clear();
    }

    async fn should_simulate_failure(&self) -> bool {
        let failure_sim = self.failure_simulation.read().await;
        if !failure_sim.enabled {
            return false;
        }

        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < failure_sim.failure_rate
    }

    pub async fn execute_script(&self, request: ScriptExecutionRequest) -> EventResult<ScriptExecutionResponse> {
        let start_time = std::time::Instant::now();

        if self.should_simulate_failure().await {
            return Err(EventError::ExecutionError("Mock script execution failed".to_string()));
        }

        // Simulate script execution time
        tokio::time::sleep(Duration::from_millis(10)).await;

        let execution_time = start_time.elapsed();
        let success = true;

        let record = ScriptExecutionRecord {
            request_id: request.request_id.clone(),
            script_id: request.script_id.clone(),
            execution_time,
            success,
            timestamp: Utc::now(),
        };

        self.execution_history.lock().await.push(record);

        Ok(ScriptExecutionResponse {
            request_id: request.request_id,
            success,
            result: Some(json!({"output": "Mock script executed successfully"})),
            error: None,
            execution_time,
            timestamp: Utc::now(),
            logs: vec!["Mock execution log".to_string()],
        })
    }
}

/// Mock DataStore service for testing
pub struct MockDataStore {
    documents: Arc<RwLock<HashMap<String, DocumentData>>>,
    operation_history: Arc<Mutex<Vec<DataStoreOperation>>>,
    failure_simulation: Arc<RwLock<FailureSimulation>>,
}

#[derive(Debug, Clone)]
enum DataStoreOperation {
    Create { id: String, timestamp: chrono::DateTime<chrono::Utc> },
    Read { id: String, timestamp: chrono::DateTime<chrono::Utc> },
    Update { id: String, timestamp: chrono::DateTime<chrono::Utc> },
    Delete { id: String, timestamp: chrono::DateTime<chrono::Utc> },
}

impl MockDataStore {
    pub fn new() -> Self {
        Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
            operation_history: Arc::new(Mutex::new(Vec::new())),
            failure_simulation: Arc::new(RwLock::new(FailureSimulation::default())),
        }
    }

    pub async fn configure_failure_simulation(&self, simulation: FailureSimulation) {
        let mut failure_sim = self.failure_simulation.write().await;
        *failure_sim = simulation;
    }

    pub async fn get_operation_history(&self) -> Vec<DataStoreOperation> {
        self.operation_history.lock().await.clone()
    }

    pub async fn clear_history(&self) {
        self.operation_history.lock().await.clear();
    }

    async fn should_simulate_failure(&self) -> bool {
        let failure_sim = self.failure_simulation.read().await;
        if !failure_sim.enabled {
            return false;
        }

        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < failure_sim.failure_rate
    }

    pub async fn create_document(&self, database: &str, document: DocumentData) -> EventResult<DocumentId> {
        if self.should_simulate_failure().await {
            return Err(EventError::DatabaseError("Mock database operation failed".to_string()));
        }

        let mut documents = self.documents.write().await;
        let id = document.id.0.clone();
        documents.insert(id.clone(), document);

        let operation = DataStoreOperation::Create {
            id: id.clone(),
            timestamp: Utc::now(),
        };
        self.operation_history.lock().await.push(operation);

        Ok(DocumentId(id))
    }

    pub async fn read_document(&self, database: &str, id: &str) -> EventResult<Option<DocumentData>> {
        if self.should_simulate_failure().await {
            return Err(EventError::DatabaseError("Mock database read failed".to_string()));
        }

        let documents = self.documents.read().await;
        let operation = DataStoreOperation::Read {
            id: id.to_string(),
            timestamp: Utc::now(),
        };
        self.operation_history.lock().await.push(operation);

        Ok(documents.get(id).cloned())
    }

    pub async fn update_document(&self, database: &str, id: &str, document: DocumentData) -> EventResult<DocumentData> {
        if self.should_simulate_failure().await {
            return Err(EventError::DatabaseError("Mock database update failed".to_string()));
        }

        let mut documents = self.documents.write().await;
        documents.insert(id.to_string(), document.clone());

        let operation = DataStoreOperation::Update {
            id: id.to_string(),
            timestamp: Utc::now(),
        };
        self.operation_history.lock().await.push(operation);

        Ok(document)
    }

    pub async fn delete_document(&self, database: &str, id: &str) -> EventResult<bool> {
        if self.should_simulate_failure().await {
            return Err(EventError::DatabaseError("Mock database delete failed".to_string()));
        }

        let mut documents = self.documents.write().await;
        let deleted = documents.remove(id).is_some();

        let operation = DataStoreOperation::Delete {
            id: id.to_string(),
            timestamp: Utc::now(),
        };
        self.operation_history.lock().await.push(operation);

        Ok(deleted)
    }
}

/// Mock InferenceEngine service for testing
pub struct MockInferenceEngine {
    model_cache: Arc<RwLock<HashMap<String, ModelInfo>>>,
    inference_history: Arc<Mutex<Vec<InferenceRecord>>>,
    failure_simulation: Arc<RwLock<FailureSimulation>>,
}

#[derive(Debug, Clone)]
struct InferenceRecord {
    request_id: String,
    model: String,
    request_type: String,
    inference_time: Duration,
    success: bool,
    timestamp: chrono::DateTime<chrono::Utc>,
}

impl MockInferenceEngine {
    pub fn new() -> Self {
        Self {
            model_cache: Arc::new(RwLock::new(HashMap::new())),
            inference_history: Arc::new(Mutex::new(Vec::new())),
            failure_simulation: Arc::new(RwLock::new(FailureSimulation::default())),
        }
    }

    pub async fn configure_failure_simulation(&self, simulation: FailureSimulation) {
        let mut failure_sim = self.failure_simulation.write().await;
        *failure_sim = simulation;
    }

    pub async fn get_inference_history(&self) -> Vec<InferenceRecord> {
        self.inference_history.lock().await.clone()
    }

    pub async fn clear_history(&self) {
        self.inference_history.lock().await.clear();
    }

    pub async fn load_model(&self, model_info: ModelInfo) -> EventResult<()> {
        if self.should_simulate_failure().await {
            return Err(EventError::ModelError("Mock model loading failed".to_string()));
        }

        let mut models = self.model_cache.write().await;
        models.insert(model_info.model_id.clone(), model_info);
        Ok(())
    }

    async fn should_simulate_failure(&self) -> bool {
        let failure_sim = self.failure_simulation.read().await;
        if !failure_sim.enabled {
            return false;
        }

        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < failure_sim.failure_rate
    }

    pub async fn generate_completion(&self, request: CompletionRequest) -> EventResult<CompletionResponse> {
        let start_time = std::time::Instant::now();

        if self.should_simulate_failure().await {
            return Err(EventError::InferenceError("Mock inference failed".to_string()));
        }

        // Simulate inference time
        tokio::time::sleep(Duration::from_millis(50)).await;

        let inference_time = start_time.elapsed();

        let record = InferenceRecord {
            request_id: request.request_id.clone(),
            model: request.model.clone(),
            request_type: "completion".to_string(),
            inference_time,
            success: true,
            timestamp: Utc::now(),
        };

        self.inference_history.lock().await.push(record);

        Ok(CompletionResponse {
            completions: vec![Completion {
                text: "Mock completion response".to_string(),
                index: 0,
                logprobs: None,
                finish_reason: "stop".to_string(),
            }],
            model: request.model,
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
            request_id: request.request_id,
            timestamp: Utc::now(),
        })
    }

    pub async fn generate_embeddings(&self, request: EmbeddingRequest) -> EventResult<EmbeddingResponse> {
        let start_time = std::time::Instant::now();

        if self.should_simulate_failure().await {
            return Err(EventError::InferenceError("Mock embedding generation failed".to_string()));
        }

        // Simulate embedding generation time
        tokio::time::sleep(Duration::from_millis(30)).await;

        let inference_time = start_time.elapsed();

        let input_text = match request.input {
            EmbeddingInput::String(text) => text,
            EmbeddingInput::Array(texts) => texts.into_iter().next().unwrap_or_default(),
        };

        // Generate mock embedding (simple hash-based approach)
        let embedding: Vec<f32> = input_text
            .bytes()
            .enumerate()
            .map(|(i, b)| (b as f32 + i as f32) / 255.0)
            .take(1536) // Standard embedding size
            .collect();

        let record = InferenceRecord {
            request_id: request.request_id.clone(),
            model: request.model.clone(),
            request_type: "embedding".to_string(),
            inference_time,
            success: true,
            timestamp: Utc::now(),
        };

        self.inference_history.lock().await.push(record);

        Ok(EmbeddingResponse {
            data: vec![Embedding {
                index: 0,
                object: "embedding".to_string(),
                embedding,
            }],
            model: request.model,
            usage: TokenUsage {
                prompt_tokens: input_text.split_whitespace().count() as u32,
                completion_tokens: 0,
                total_tokens: input_text.split_whitespace().count() as u32,
            },
            request_id: request.request_id,
            timestamp: Utc::now(),
        })
    }
}

/// Mock McpGateway service for testing
pub struct MockMcpGateway {
    sessions: Arc<RwLock<HashMap<String, McpSession>>>,
    tools: Arc<RwLock<HashMap<String, ToolDefinition>>>,
    session_history: Arc<Mutex<Vec<SessionOperation>>>,
    failure_simulation: Arc<RwLock<FailureSimulation>>,
}

#[derive(Debug, Clone)]
enum SessionOperation {
    Created { session_id: String, client_id: String, timestamp: chrono::DateTime<chrono::Utc> },
    Closed { session_id: String, timestamp: chrono::DateTime<chrono::Utc> },
    ToolExecuted { session_id: String, tool_name: String, timestamp: chrono::DateTime<chrono::Utc> },
}

impl MockMcpGateway {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            tools: Arc::new(RwLock::new(HashMap::new())),
            session_history: Arc::new(Mutex::new(Vec::new())),
            failure_simulation: Arc::new(RwLock::new(FailureSimulation::default())),
        }
    }

    pub async fn configure_failure_simulation(&self, simulation: FailureSimulation) {
        let mut failure_sim = self.failure_simulation.write().await;
        *failure_sim = simulation;
    }

    pub async fn get_session_history(&self) -> Vec<SessionOperation> {
        self.session_history.lock().await.clone()
    }

    pub async fn clear_history(&self) {
        self.session_history.lock().await.clear();
    }

    async fn should_simulate_failure(&self) -> bool {
        let failure_sim = self.failure_simulation.read().await;
        if !failure_sim.enabled {
            return false;
        }

        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < failure_sim.failure_rate
    }

    pub async fn create_session(&self, client_id: &str, capabilities: McpCapabilities) -> EventResult<McpSession> {
        if self.should_simulate_failure().await {
            return Err(EventError::McpError("Mock session creation failed".to_string()));
        }

        let session_id = format!("mock_session_{}", Uuid::new_v4());
        let session = McpSession {
            session_id: session_id.clone(),
            client_id: client_id.to_string(),
            status: McpSessionStatus::Active,
            server_capabilities: McpCapabilities::default(),
            client_capabilities: capabilities,
            metadata: HashMap::new(),
            created_at: Utc::now(),
            last_activity: Utc::now(),
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id.clone(), session.clone());

        let operation = SessionOperation::Created {
            session_id: session_id.clone(),
            client_id: client_id.to_string(),
            timestamp: Utc::now(),
        };
        self.session_history.lock().await.push(operation);

        Ok(session)
    }

    pub async fn close_session(&self, session_id: &str) -> EventResult<()> {
        if self.should_simulate_failure().await {
            return Err(EventError::McpError("Mock session closure failed".to_string()));
        }

        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);

        let operation = SessionOperation::Closed {
            session_id: session_id.to_string(),
            timestamp: Utc::now(),
        };
        self.session_history.lock().await.push(operation);

        Ok(())
    }

    pub async fn execute_tool(&self, session_id: &str, tool_name: &str, arguments: HashMap<String, Value>) -> EventResult<McpToolResponse> {
        if self.should_simulate_failure().await {
            return Err(EventError::McpError("Mock tool execution failed".to_string()));
        }

        // Check if session exists
        let sessions = self.sessions.read().await;
        if !sessions.contains_key(session_id) {
            return Err(EventError::McpError("Session not found".to_string()));
        }
        drop(sessions);

        let operation = SessionOperation::ToolExecuted {
            session_id: session_id.to_string(),
            tool_name: tool_name.to_string(),
            timestamp: Utc::now(),
        };
        self.session_history.lock().await.push(operation);

        Ok(McpToolResponse {
            request_id: Uuid::new_v4().to_string(),
            result: Some(json!({
                "tool": tool_name,
                "arguments": arguments,
                "output": "Mock tool execution result"
            })),
            error: None,
            execution_time: Duration::from_millis(25),
            timestamp: Utc::now(),
        })
    }
}