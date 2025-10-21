//! Service-specific event types for each service in the daemon coordination system

use super::core::{DaemonEvent, EventSource, EventPayload};
use super::errors::{EventError, EventResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Service-specific events for McpGateway
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum McpGatewayEvent {
    /// MCP server connected
    ServerConnected {
        server_id: String,
        server_name: String,
        capabilities: Vec<String>,
        connection_info: ConnectionInfo,
    },

    /// MCP server disconnected
    ServerDisconnected {
        server_id: String,
        reason: String,
        was_clean: bool,
    },

    /// Tool call received from MCP server
    ToolCallReceived {
        server_id: String,
        tool_name: String,
        parameters: serde_json::Value,
        call_id: String,
    },

    /// Tool call completed
    ToolCallCompleted {
        server_id: String,
        tool_name: String,
        call_id: String,
        result: ToolCallResult,
        duration_ms: u64,
    },

    /// Resource requested from MCP server
    ResourceRequested {
        server_id: String,
        resource_type: String,
        resource_id: String,
        parameters: serde_json::Value,
    },

    /// Resource provided by MCP server
    ResourceProvided {
        server_id: String,
        resource_type: String,
        resource_id: String,
        data: serde_json::Value,
        metadata: HashMap<String, String>,
    },

    /// MCP protocol error
    ProtocolError {
        server_id: String,
        error_code: String,
        error_message: String,
        recoverable: bool,
    },

    /// Server health status changed
    ServerHealthChanged {
        server_id: String,
        old_status: String,
        new_status: String,
        last_ping: DateTime<Utc>,
    },

    /// Configuration updated
    ConfigurationUpdated {
        server_id: String,
        changes: HashMap<String, serde_json::Value>,
    },

    /// Load balancing decision
    LoadBalanced {
        server_id: String,
        load_score: f64,
        selected: bool,
    },
}

/// Connection information for MCP servers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionInfo {
    pub connection_type: String,
    pub endpoint: String,
    pub established_at: DateTime<Utc>,
    pub latency_ms: Option<u64>,
    pub protocol_version: String,
}

/// Tool call result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ToolCallResult {
    Success { result: serde_json::Value },
    Error { error: String, code: Option<String> },
    Timeout { duration_ms: u64 },
    Cancelled { reason: String },
}

impl McpGatewayEvent {
    /// Create a server connected event
    pub fn server_connected(
        server_id: String,
        server_name: String,
        capabilities: Vec<String>,
        connection_info: ConnectionInfo,
    ) -> Self {
        Self::ServerConnected {
            server_id,
            server_name,
            capabilities,
            connection_info,
        }
    }

    /// Create a tool call completed event
    pub fn tool_call_completed(
        server_id: String,
        tool_name: String,
        call_id: String,
        result: ToolCallResult,
        duration_ms: u64,
    ) -> Self {
        Self::ToolCallCompleted {
            server_id,
            tool_name,
            call_id,
            result,
            duration_ms,
        }
    }

    /// Convert to DaemonEvent
    pub fn to_daemon_event(self, source: EventSource) -> DaemonEvent {
        let payload = EventPayload::json(serde_json::to_value(&self).unwrap());
        let event_type = super::core::EventType::Service(super::core::ServiceEventType::RequestReceived {
            from_service: source.id.clone(),
            to_service: "mcp-gateway".to_string(),
            request: serde_json::to_value(&self).unwrap(),
        });

        DaemonEvent::new(event_type, source, payload)
    }
}

/// Service-specific events for InferenceEngine
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InferenceEngineEvent {
    /// Inference request received
    InferenceRequested {
        request_id: String,
        model_id: String,
        input: InferenceInput,
        parameters: InferenceParameters,
        priority: String,
    },

    /// Inference started
    InferenceStarted {
        request_id: String,
        model_id: String,
        queue_position: u32,
        estimated_duration_ms: u64,
    },

    /// Inference completed
    InferenceCompleted {
        request_id: String,
        model_id: String,
        result: InferenceResult,
        duration_ms: u64,
        tokens_used: TokenUsage,
    },

    /// Inference failed
    InferenceFailed {
        request_id: String,
        model_id: String,
        error: String,
        error_code: Option<String>,
        duration_ms: u64,
    },

    /// Model loaded
    ModelLoaded {
        model_id: String,
        model_type: String,
        memory_usage_mb: u64,
        load_time_ms: u64,
    },

    /// Model unloaded
    ModelUnloaded {
        model_id: String,
        reason: String,
        memory_freed_mb: u64,
    },

    /// Queue status updated
    QueueStatusUpdated {
        model_id: String,
        queue_length: u32,
        average_wait_ms: u64,
        processing_rate: f64,
    },

    /// Resource usage updated
    ResourceUsageUpdated {
        model_id: String,
        cpu_usage_percent: f64,
        memory_usage_mb: u64,
        gpu_usage_percent: Option<f64>,
        gpu_memory_mb: Option<u64>,
    },

    /// Batch inference completed
    BatchInferenceCompleted {
        batch_id: String,
        request_ids: Vec<String>,
        model_id: String,
        results: Vec<InferenceResult>,
        total_duration_ms: u64,
        efficiency_gain: f64,
    },

    /// Model health check
    ModelHealthCheck {
        model_id: String,
        status: String,
        response_time_ms: u64,
        last_inference: Option<DateTime<Utc>>,
    },
}

/// Inference input data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InferenceInput {
    pub input_type: String,
    pub data: serde_json::Value,
    pub context: Option<serde_json::Value>,
    pub metadata: HashMap<String, String>,
}

/// Inference parameters
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InferenceParameters {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub repetition_penalty: Option<f32>,
    pub stream: Option<bool>,
    pub custom_parameters: HashMap<String, serde_json::Value>,
}

/// Inference result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InferenceResult {
    pub output: String,
    pub output_tokens: u32,
    pub confidence: Option<f64>,
    pub finish_reason: String,
    pub metadata: HashMap<String, serde_json::Value>,
    pub alternatives: Option<Vec<String>>,
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl InferenceEngineEvent {
    /// Create an inference completed event
    pub fn inference_completed(
        request_id: String,
        model_id: String,
        result: InferenceResult,
        duration_ms: u64,
        tokens_used: TokenUsage,
    ) -> Self {
        Self::InferenceCompleted {
            request_id,
            model_id,
            result,
            duration_ms,
            tokens_used,
        }
    }

    /// Convert to DaemonEvent
    pub fn to_daemon_event(self, source: EventSource) -> DaemonEvent {
        let payload = EventPayload::json(serde_json::to_value(&self).unwrap());
        let event_type = super::core::EventType::Service(super::core::ServiceEventType::RequestReceived {
            from_service: source.id.clone(),
            to_service: "inference-engine".to_string(),
            request: serde_json::to_value(&self).unwrap(),
        });

        DaemonEvent::new(event_type, source, payload)
    }
}

/// Service-specific events for ScriptEngine
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScriptEngineEvent {
    /// Script execution requested
    ScriptExecutionRequested {
        execution_id: String,
        script_id: String,
        script_type: String,
        parameters: serde_json::Value,
        context: ExecutionContext,
    },

    /// Script execution started
    ScriptExecutionStarted {
        execution_id: String,
        script_id: String,
        runtime: String,
        sandbox_id: String,
    },

    /// Script execution completed
    ScriptExecutionCompleted {
        execution_id: String,
        script_id: String,
        result: ScriptResult,
        duration_ms: u64,
        resource_usage: ResourceUsage,
    },

    /// Script execution failed
    ScriptExecutionFailed {
        execution_id: String,
        script_id: String,
        error: ScriptError,
        duration_ms: u64,
    },

    /// Script loaded
    ScriptLoaded {
        script_id: String,
        script_path: String,
        script_type: String,
        size_bytes: u64,
        hash: String,
    },

    /// Script unloaded
    ScriptUnloaded {
        script_id: String,
        reason: String,
    },

    /// Runtime started
    RuntimeStarted {
        runtime_type: String,
        runtime_id: String,
        version: String,
        capabilities: Vec<String>,
    },

    /// Runtime stopped
    RuntimeStopped {
        runtime_id: String,
        reason: String,
        executions_completed: u32,
    },

    /// Sandbox created
    SandboxCreated {
        sandbox_id: String,
        runtime_type: String,
        isolation_level: String,
        resources: SandboxResources,
    },

    /// Sandbox destroyed
    SandboxDestroyed {
        sandbox_id: String,
        reason: String,
        total_executions: u32,
    },

    /// Security violation detected
    SecurityViolation {
        execution_id: String,
        script_id: String,
        violation_type: String,
        details: String,
        action_taken: String,
    },

    /// Script compilation completed
    ScriptCompiled {
        script_id: String,
        success: bool,
        compilation_time_ms: u64,
        warnings: Vec<String>,
        errors: Vec<String>,
    },
}

/// Execution context for scripts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionContext {
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub working_directory: Option<String>,
    pub environment: HashMap<String, String>,
    pub permissions: Vec<String>,
    pub timeout_ms: Option<u64>,
    pub memory_limit_mb: Option<u64>,
}

/// Script execution result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptResult {
    pub output: serde_json::Value,
    pub return_code: i32,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub artifacts: Vec<ScriptArtifact>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Script execution error
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptError {
    pub error_type: String,
    pub message: String,
    pub stack_trace: Option<String>,
    pub line_number: Option<u32>,
    pub column: Option<u32>,
    pub recoverable: bool,
}

/// Resource usage for script execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceUsage {
    pub cpu_time_ms: u64,
    pub memory_peak_mb: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub network_bytes_sent: u64,
    pub network_bytes_received: u64,
}

/// Script artifact
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptArtifact {
    pub artifact_type: String,
    pub name: String,
    pub path: Option<String>,
    pub data: Option<serde_json::Value>,
    pub size_bytes: Option<u64>,
    pub hash: Option<String>,
}

/// Sandbox resources
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxResources {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_mb: u64,
    pub network_access: bool,
    pub allowed_paths: Vec<String>,
}

impl ScriptEngineEvent {
    /// Create a script execution completed event
    pub fn script_execution_completed(
        execution_id: String,
        script_id: String,
        result: ScriptResult,
        duration_ms: u64,
        resource_usage: ResourceUsage,
    ) -> Self {
        Self::ScriptExecutionCompleted {
            execution_id,
            script_id,
            result,
            duration_ms,
            resource_usage,
        }
    }

    /// Convert to DaemonEvent
    pub fn to_daemon_event(self, source: EventSource) -> DaemonEvent {
        let payload = EventPayload::json(serde_json::to_value(&self).unwrap());
        let event_type = super::core::EventType::Service(super::core::ServiceEventType::RequestReceived {
            from_service: source.id.clone(),
            to_service: "script-engine".to_string(),
            request: serde_json::to_value(&self).unwrap(),
        });

        DaemonEvent::new(event_type, source, payload)
    }
}

/// Service-specific events for DataStore
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DataStoreEvent {
    /// Data query requested
    DataQueryRequested {
        query_id: String,
        query_type: String,
        query: String,
        parameters: serde_json::Value,
        requester: String,
    },

    /// Data query completed
    DataQueryCompleted {
        query_id: String,
        query_type: String,
        result: QueryResult,
        duration_ms: u64,
        rows_affected: u64,
    },

    /// Data query failed
    DataQueryFailed {
        query_id: String,
        query_type: String,
        error: String,
        error_code: Option<String>,
        duration_ms: u64,
    },

    /// Data inserted
    DataInserted {
        table: String,
        record_id: String,
        data: serde_json::Value,
        timestamp: DateTime<Utc>,
    },

    /// Data updated
    DataUpdated {
        table: String,
        record_id: String,
        old_data: serde_json::Value,
        new_data: serde_json::Value,
        changes: Vec<DataChange>,
        timestamp: DateTime<Utc>,
    },

    /// Data deleted
    DataDeleted {
        table: String,
        record_id: String,
        deleted_data: serde_json::Value,
        timestamp: DateTime<Utc>,
    },

    /// Index created
    IndexCreated {
        table: String,
        index_name: String,
        columns: Vec<String>,
        index_type: String,
    },

    /// Index dropped
    IndexDropped {
        table: String,
        index_name: String,
        reason: String,
    },

    /// Schema changed
    SchemaChanged {
        table: String,
        change_type: String,
        changes: Vec<SchemaChangeDetail>,
        timestamp: DateTime<Utc>,
    },

    /// Backup completed
    BackupCompleted {
        backup_id: String,
        backup_type: String,
        size_bytes: u64,
        duration_ms: u64,
        tables: Vec<String>,
    },

    /// Restore completed
    RestoreCompleted {
        backup_id: String,
        restore_point: DateTime<Utc>,
        tables_restored: Vec<String>,
        duration_ms: u64,
    },

    /// Connection pool status updated
    ConnectionPoolUpdated {
        pool_type: String,
        active_connections: u32,
        idle_connections: u32,
        max_connections: u32,
    },

    /// Performance metrics updated
    PerformanceMetricsUpdated {
        table: String,
        operation_type: String,
        avg_duration_ms: f64,
        operations_per_second: f64,
        error_rate: f64,
    },
}

/// Query result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryResult {
    pub success: bool,
    pub data: serde_json::Value,
    pub rows_returned: u64,
    pub columns: Vec<ColumnInfo>,
    pub execution_plan: Option<serde_json::Value>,
    pub warnings: Vec<String>,
}

/// Column information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<serde_json::Value>,
}

/// Data change information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataChange {
    pub column: String,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
    pub change_type: String,
}

/// Schema change detail
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaChangeDetail {
    pub operation: String,
    pub object_type: String,
    pub object_name: String,
    pub details: HashMap<String, serde_json::Value>,
}

impl DataStoreEvent {
    /// Create a data query completed event
    pub fn data_query_completed(
        query_id: String,
        query_type: String,
        result: QueryResult,
        duration_ms: u64,
        rows_affected: u64,
    ) -> Self {
        Self::DataQueryCompleted {
            query_id,
            query_type,
            result,
            duration_ms,
            rows_affected,
        }
    }

    /// Convert to DaemonEvent
    pub fn to_daemon_event(self, source: EventSource) -> DaemonEvent {
        let payload = EventPayload::json(serde_json::to_value(&self).unwrap());
        let event_type = super::core::EventType::Service(super::core::ServiceEventType::RequestReceived {
            from_service: source.id.clone(),
            to_service: "datastore".to_string(),
            request: serde_json::to_value(&self).unwrap(),
        });

        DaemonEvent::new(event_type, source, payload)
    }
}

/// Service event builder utility
pub struct ServiceEventBuilder {
    source: EventSource,
    correlation_id: Option<Uuid>,
}

impl ServiceEventBuilder {
    /// Create a new event builder
    pub fn new(source: EventSource) -> Self {
        Self {
            source,
            correlation_id: None,
        }
    }

    /// Set correlation ID
    pub fn with_correlation(mut self, correlation_id: Uuid) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Build MCP Gateway event
    pub fn mcp_gateway_event(self, event: McpGatewayEvent) -> DaemonEvent {
        let mut daemon_event = event.to_daemon_event(self.source);
        if let Some(correlation_id) = self.correlation_id {
            daemon_event.correlation_id = Some(correlation_id);
        }
        daemon_event
    }

    /// Build Inference Engine event
    pub fn inference_engine_event(self, event: InferenceEngineEvent) -> DaemonEvent {
        let mut daemon_event = event.to_daemon_event(self.source);
        if let Some(correlation_id) = self.correlation_id {
            daemon_event.correlation_id = Some(correlation_id);
        }
        daemon_event
    }

    /// Build Script Engine event
    pub fn script_engine_event(self, event: ScriptEngineEvent) -> DaemonEvent {
        let mut daemon_event = event.to_daemon_event(self.source);
        if let Some(correlation_id) = self.correlation_id {
            daemon_event.correlation_id = Some(correlation_id);
        }
        daemon_event
    }

    /// Build DataStore event
    pub fn datastore_event(self, event: DataStoreEvent) -> DaemonEvent {
        let mut daemon_event = event.to_daemon_event(self.source);
        if let Some(correlation_id) = self.correlation_id {
            daemon_event.correlation_id = Some(correlation_id);
        }
        daemon_event
    }
}

/// Utility functions for creating common service events
pub mod utils {
    use super::*;

    /// Create a health check event for any service
    pub fn health_check_event(service_id: String, status: String, details: HashMap<String, String>) -> DaemonEvent {
        let source = EventSource::service(service_id.clone());
        use crate::events::{EventType, ServiceEventType};
        let event_type = EventType::Service(ServiceEventType::HealthCheck {
            service_id: service_id.clone(),
            status,
        });
        let payload = EventPayload::json(serde_json::to_value(details).unwrap());

        DaemonEvent::new(event_type, source, payload)
    }

    /// Create a service registered event
    pub fn service_registered_event(
        service_id: String,
        service_type: String,
        capabilities: Vec<String>,
    ) -> DaemonEvent {
        let source = EventSource::service(service_id.clone());
        use crate::events::{EventType, ServiceEventType};
        let event_type = EventType::Service(ServiceEventType::ServiceRegistered {
            service_id: service_id.clone(),
            service_type,
        });

        let mut details = HashMap::new();
        details.insert("capabilities".to_string(), serde_json::to_string(&capabilities).unwrap());
        let payload = EventPayload::json(serde_json::to_value(details).unwrap());

        DaemonEvent::new(event_type, source, payload)
    }

    /// Create an error event for any service
    pub fn service_error_event(
        service_id: String,
        error: String,
        context: HashMap<String, String>,
    ) -> DaemonEvent {
        let source = EventSource::service(service_id.clone());
        use crate::events::{EventType, ServiceEventType};
        let event_type = EventType::Service(ServiceEventType::RequestReceived {
            from_service: service_id.clone(),
            to_service: "daemon".to_string(),
            request: serde_json::json!({
                "error": error,
                "context": context
            }),
        });

        let payload = EventPayload::json(serde_json::json!({
            "error": error,
            "context": context,
            "service_id": service_id
        }));

        use crate::events::EventPriority;
        DaemonEvent::new(event_type, source, payload)
            .with_priority(EventPriority::High)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_gateway_event_serialization() {
        let event = McpGatewayEvent::server_connected(
            "server-1".to_string(),
            "Test Server".to_string(),
            vec!["tool1".to_string(), "tool2".to_string()],
            ConnectionInfo {
                connection_type: "websocket".to_string(),
                endpoint: "ws://localhost:8080".to_string(),
                established_at: Utc::now(),
                latency_ms: Some(50),
                protocol_version: "1.0".to_string(),
            },
        );

        let serialized = serde_json::to_string(&event).unwrap();
        let deserialized: McpGatewayEvent = serde_json::from_str(&serialized).unwrap();

        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_inference_engine_event() {
        let event = InferenceEngineEvent::inference_completed(
            "req-123".to_string(),
            "gpt-4".to_string(),
            InferenceResult {
                output: "Hello, world!".to_string(),
                output_tokens: 10,
                confidence: Some(0.95),
                finish_reason: "stop".to_string(),
                metadata: HashMap::new(),
                alternatives: None,
            },
            1500,
            TokenUsage {
                prompt_tokens: 5,
                completion_tokens: 10,
                total_tokens: 15,
            },
        );

        let daemon_event = event.to_daemon_event(EventSource::service("inference-engine".to_string()));
        assert_eq!(daemon_event.source.id, "inference-engine");
    }

    #[test]
    fn test_service_event_builder() {
        let builder = ServiceEventBuilder::new(EventSource::service("test-service".to_string()))
            .with_correlation(Uuid::new_v4());

        let event = McpGatewayEvent::tool_call_completed(
            "server-1".to_string(),
            "test_tool".to_string(),
            "call-123".to_string(),
            ToolCallResult::Success {
                result: serde_json::json!({"result": "success"}),
            },
            500,
        );

        let daemon_event = builder.mcp_gateway_event(event);
        assert!(daemon_event.correlation_id.is_some());
        assert_eq!(daemon_event.source.id, "test-service");
    }

    #[test]
    fn test_utility_functions() {
        let health_event = utils::health_check_event(
            "test-service".to_string(),
            "healthy".to_string(),
            HashMap::new(),
        );

        assert_eq!(health_event.source.id, "test-service");
        match health_event.event_type {
            super::core::EventType::Service(super::core::ServiceEventType::HealthCheck { service_id, status }) => {
                assert_eq!(service_id, "test-service");
                assert_eq!(status, "healthy");
            }
            _ => panic!("Expected health check event"),
        }
    }
}