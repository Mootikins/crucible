//! # Test Fixtures
//!
//! Common test data and message fixtures for testing IPC components.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::plugin_ipc::{
    message::{
        IpcMessage, MessageHeader, MessagePayload, MessageType, MessageFlags, MessagePriority,
        RequestPayload, ResponsePayload, EventPayload, HeartbeatPayload, ResourceUsage,
        HeartbeatStatus, ClientCapabilities, PluginCapabilities, StreamChunk, StreamEnd,
        ConfigurationUpdate, SecurityConfig, TransportConfig, MetricsConfig,
    },
    security::{AuthConfig, EncryptionConfig, AuthorizationConfig},
    transport::TransportConfig as TransportManagerConfig,
    config::IpcConfig,
};

/// Message fixtures for testing
pub struct MessageFixtures;

impl MessageFixtures {
    /// Create a basic heartbeat message
    pub fn heartbeat() -> IpcMessage {
        IpcMessage::new(
            MessageType::Heartbeat,
            MessagePayload::Heartbeat(HeartbeatPayload {
                status: HeartbeatStatus::Healthy,
                last_activity: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64,
                resource_usage: ResourceUsage {
                    memory_bytes: 1024 * 1024,
                    cpu_percentage: 25.5,
                    disk_bytes: 2048 * 1024,
                    network_bytes: 512 * 1024,
                    open_files: 15,
                    active_threads: 4,
                },
                metrics: HashMap::new(),
                status_data: HashMap::new(),
            }),
        )
    }

    /// Create a heartbeat message with custom resource usage
    pub fn heartbeat_with_resources(memory: u64, cpu: f64, disk: u64, network: u64) -> IpcMessage {
        IpcMessage::new(
            MessageType::Heartbeat,
            MessagePayload::Heartbeat(HeartbeatPayload {
                status: HeartbeatStatus::Healthy,
                last_activity: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64,
                resource_usage: ResourceUsage {
                    memory_bytes: memory,
                    cpu_percentage: cpu,
                    disk_bytes: disk,
                    network_bytes: network,
                    open_files: 10,
                    active_threads: 2,
                },
                metrics: HashMap::new(),
                status_data: HashMap::new(),
            }),
        )
    }

    /// Create a request message
    pub fn request(operation: &str, params: Value) -> IpcMessage {
        IpcMessage::new(
            MessageType::Request,
            MessagePayload::Request(RequestPayload {
                operation: operation.to_string(),
                parameters: params,
                timeout_ms: Some(30000),
                retry_policy: None,
                metadata: HashMap::new(),
                context: HashMap::new(),
            }),
        )
    }

    /// Create a request message with destination
    pub fn request_to(destination: &str, operation: &str, params: Value) -> IpcMessage {
        let mut message = Self::request(operation, params);
        message.header.destination = Some(destination.to_string());
        message.header.correlation_id = Some(Uuid::new_v4().to_string());
        message
    }

    /// Create a successful response message
    pub fn success_response(correlation_id: &str, data: Value) -> IpcMessage {
        IpcMessage::new(
            MessageType::Response,
            MessagePayload::Response(ResponsePayload {
                correlation_id: correlation_id.to_string(),
                success: true,
                data: Some(data),
                error: None,
                metadata: HashMap::new(),
                execution_time_ms: Some(150),
            }),
        )
    }

    /// Create an error response message
    pub fn error_response(correlation_id: &str, error_code: &str, error_message: &str) -> IpcMessage {
        IpcMessage::new(
            MessageType::Response,
            MessagePayload::Response(ResponsePayload {
                correlation_id: correlation_id.to_string(),
                success: false,
                data: None,
                error: Some(json!({
                    "code": error_code,
                    "message": error_message,
                    "details": {}
                })),
                metadata: HashMap::new(),
                execution_time_ms: Some(50),
            }),
        )
    }

    /// Create an event message
    pub fn event(event_type: &str, data: Value) -> IpcMessage {
        IpcMessage::new(
            MessageType::Event,
            MessagePayload::Event(EventPayload {
                event_type: event_type.to_string(),
                data,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                source: "test".to_string(),
                metadata: HashMap::new(),
            }),
        )
    }

    /// Create a stream chunk message
    pub fn stream_chunk(stream_id: &str, chunk_number: u32, data: Vec<u8>) -> IpcMessage {
        IpcMessage::new(
            MessageType::StreamChunk,
            MessagePayload::StreamChunk(StreamChunk {
                stream_id: stream_id.to_string(),
                chunk_number,
                data,
                is_final: false,
                metadata: HashMap::new(),
            }),
        )
    }

    /// Create a stream end message
    pub fn stream_end(stream_id: &str, success: bool, error: Option<Value>) -> IpcMessage {
        IpcMessage::new(
            MessageType::StreamEnd,
            MessagePayload::StreamEnd(StreamEnd {
                stream_id: stream_id.to_string(),
                success,
                error,
                metadata: HashMap::new(),
            }),
        )
    }

    /// Create a configuration update message
    pub fn config_update(config: Value) -> IpcMessage {
        IpcMessage::new(
            MessageType::ConfigurationUpdate,
            MessagePayload::ConfigurationUpdate(ConfigurationUpdate {
                config,
                version: 1,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                source: "test".to_string(),
                metadata: HashMap::new(),
            }),
        )
    }

    /// Create a message with custom header
    pub fn with_header(message_type: MessageType, payload: MessagePayload, mut header: MessageHeader) -> IpcMessage {
        header.message_type = message_type;
        IpcMessage { header, payload }
    }

    /// Create a large message for testing size limits
    pub fn large_message(size_bytes: usize) -> IpcMessage {
        let large_data = "x".repeat(size_bytes);
        Self::request("test_operation", json!({"large_data": large_data}))
    }

    /// Create a message with all optional fields populated
    pub fn full_message() -> IpcMessage {
        let mut header = MessageHeader {
            version: 1,
            message_type: MessageType::Request,
            flags: MessageFlags {
                compressed: true,
                encrypted: true,
                requires_ack: true,
                no_retry: false,
                high_priority: true,
                stream: false,
                final_message: true,
            },
            message_id: Uuid::new_v4().to_string(),
            session_id: Uuid::new_v4().to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            source: "test_source".to_string(),
            destination: Some("test_destination".to_string()),
            correlation_id: Some(Uuid::new_v4().to_string()),
            priority: MessagePriority::High,
            ttl: Some(300),
            metadata: {
                let mut map = HashMap::new();
                map.insert("test_key".to_string(), "test_value".to_string());
                map.insert("env".to_string(), "test".to_string());
                map
            },
        };

        let payload = MessagePayload::Request(RequestPayload {
            operation: "test_operation".to_string(),
            parameters: json!({
                "param1": "value1",
                "param2": 42,
                "param3": true,
                "param4": ["a", "b", "c"],
                "param5": {"nested": "value"}
            }),
            timeout_ms: Some(60000),
            retry_policy: Some(json!({
                "max_retries": 3,
                "backoff_strategy": "exponential",
                "initial_delay_ms": 100
            })),
            metadata: {
                let mut map = HashMap::new();
                map.insert("request_type".to_string(), "test".to_string());
                map
            },
            context: {
                let mut map = HashMap::new();
                map.insert("trace_id".to_string(), Uuid::new_v4().to_string());
                map.insert("user_id".to_string(), "test_user".to_string());
                map
            },
        });

        IpcMessage { header, payload }
    }
}

/// Capability fixtures for testing
pub struct CapabilityFixtures;

impl CapabilityFixtures {
    /// Create basic client capabilities
    pub fn basic_client() -> ClientCapabilities {
        ClientCapabilities {
            plugin_types: vec!["text".to_string(), "image".to_string()],
            operations: vec!["read".to_string(), "write".to_string()],
            data_formats: vec!["json".to_string(), "text".to_string()],
            max_concurrent_requests: 10,
            supports_streaming: false,
            supports_batching: true,
            supports_compression: true,
            supports_encryption: true,
        }
    }

    /// Create full client capabilities
    pub fn full_client() -> ClientCapabilities {
        ClientCapabilities {
            plugin_types: vec![
                "text".to_string(),
                "image".to_string(),
                "audio".to_string(),
                "video".to_string(),
                "document".to_string(),
            ],
            operations: vec![
                "read".to_string(),
                "write".to_string(),
                "delete".to_string(),
                "search".to_string(),
                "transform".to_string(),
                "analyze".to_string(),
            ],
            data_formats: vec![
                "json".to_string(),
                "text".to_string(),
                "binary".to_string(),
                "xml".to_string(),
                "yaml".to_string(),
            ],
            max_concurrent_requests: 100,
            supports_streaming: true,
            supports_batching: true,
            supports_compression: true,
            supports_encryption: true,
        }
    }

    /// Create minimal client capabilities
    pub fn minimal_client() -> ClientCapabilities {
        ClientCapabilities {
            plugin_types: vec!["text".to_string()],
            operations: vec!["read".to_string()],
            data_formats: vec!["json".to_string()],
            max_concurrent_requests: 1,
            supports_streaming: false,
            supports_batching: false,
            supports_compression: false,
            supports_encryption: false,
        }
    }

    /// Create basic plugin capabilities
    pub fn basic_plugin() -> PluginCapabilities {
        PluginCapabilities {
            plugin_type: "text".to_string(),
            version: "1.0.0".to_string(),
            operations: vec!["read".to_string(), "write".to_string()],
            data_formats: vec!["json".to_string(), "text".to_string()],
            max_concurrent_requests: 5,
            supports_streaming: false,
            supports_batching: true,
            supports_compression: true,
            supports_encryption: true,
            resource_requirements: HashMap::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Configuration fixtures for testing
pub struct ConfigFixtures;

impl ConfigFixtures {
    /// Create basic IPC configuration
    pub fn basic_ipc() -> IpcConfig {
        IpcConfig {
            protocol_version: 1,
            max_message_size: 1024 * 1024,
            connect_timeout_ms: 5000,
            request_timeout_ms: 30000,
            heartbeat_interval_ms: 10000,
            idle_timeout_ms: 60000,
            enable_compression: true,
            enable_encryption: true,
            max_retries: 3,
            retry_backoff_ms: 1000,
            connection_pool_size: 10,
            socket_path: "/tmp/crucible_test".to_string(),
            port_range: 9000..10000,
        }
    }

    /// Create fast IPC configuration for testing
    pub fn fast_ipc() -> IpcConfig {
        IpcConfig {
            connect_timeout_ms: 100,
            request_timeout_ms: 500,
            heartbeat_interval_ms: 50,
            idle_timeout_ms: 1000,
            ..Self::basic_ipc()
        }
    }

    /// Create IPC configuration with no security
    pub fn no_security_ipc() -> IpcConfig {
        IpcConfig {
            enable_compression: false,
            enable_encryption: false,
            ..Self::basic_ipc()
        }
    }

    /// Create basic authentication configuration
    pub fn basic_auth() -> AuthConfig {
        AuthConfig {
            jwt_secret: "test_secret_key_12345678901234567890".to_string(),
            token_expiry_ms: 3600000,
            refresh_token_expiry_ms: 86400000,
            issuer: "crucible_test".to_string(),
            audience: "crucible_plugins".to_string(),
            algorithm: "HS256".to_string(),
            require_https: false,
        }
    }

    /// Create basic encryption configuration
    pub fn basic_encryption() -> EncryptionConfig {
        EncryptionConfig {
            algorithm: "aes256gcm".to_string(),
            key_rotation_interval_ms: 86400000,
            key_derivation_iterations: 100000,
            salt: Some("test_salt_12345678901234567890".to_string()),
        }
    }

    /// Create basic authorization configuration
    pub fn basic_authorization() -> AuthorizationConfig {
        AuthorizationConfig {
            default_policy: "deny".to_string(),
            enable_rbac: true,
            enable_abac: false,
            policy_cache_ttl_ms: 300000,
            max_policies_per_entity: 100,
        }
    }

    /// Create basic transport configuration
    pub fn basic_transport() -> TransportManagerConfig {
        TransportManagerConfig {
            socket_path: "/tmp/crucible_test_transport".to_string(),
            tcp_port_range: 9000..10000,
            connection_timeout_ms: 5000,
            keepalive_interval_ms: 30000,
            max_connections: 100,
            enable_tcp_fallback: true,
            buffer_size: 8192,
            max_frame_size: 16 * 1024 * 1024,
        }
    }
}

/// Test data fixtures
pub struct DataFixtures;

impl DataFixtures {
    /// Create test JSON data
    pub fn json_data() -> Value {
        json!({
            "string_field": "test_value",
            "number_field": 42,
            "float_field": 3.14,
            "boolean_field": true,
            "null_field": null,
            "array_field": [1, 2, 3, "test"],
            "object_field": {
                "nested_string": "nested_value",
                "nested_number": 100
            }
        })
    }

    /// Create large test data
    pub fn large_json_data(size_kb: usize) -> Value {
        let mut data = json!({});
        for i in 0..(size_kb * 10) {
            data["data"][i] = json!(format!("test_data_{:06}", i));
        }
        data
    }

    /// Create binary test data
    pub fn binary_data(size_bytes: usize) -> Vec<u8> {
        (0..size_bytes).map(|i| (i % 256) as u8).collect()
    }

    /// Create string test data
    pub fn string_data(size_chars: usize) -> String {
        "test_string_".repeat(size_chars / 12 + 1)[..size_chars].to_string()
    }

    /// Create test metadata
    pub fn metadata() -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("test_id".to_string(), Uuid::new_v4().to_string());
        metadata.insert("test_env".to_string(), "unit_test".to_string());
        metadata.insert("test_version".to_string(), "1.0.0".to_string());
        metadata.insert("created_at".to_string(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis().to_string());
        metadata
    }

    /// Create test context data
    pub fn context() -> HashMap<String, String> {
        let mut context = HashMap::new();
        context.insert("trace_id".to_string(), Uuid::new_v4().to_string());
        context.insert("span_id".to_string(), Uuid::new_v4().to_string());
        context.insert("user_id".to_string(), "test_user".to_string());
        context.insert("session_id".to_string(), Uuid::new_v4().to_string());
        context
    }
}

/// Error fixtures for testing
pub struct ErrorFixtures;

impl ErrorFixtures {
    /// Create test error details
    pub fn error_details(code: &str, message: &str) -> Value {
        json!({
            "code": code,
            "message": message,
            "details": {
                "timestamp": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis(),
                "request_id": Uuid::new_v4().to_string(),
                "stack_trace": "test_stack_trace"
            }
        })
    }

    /// Create validation error
    pub fn validation_error(field: &str, reason: &str) -> Value {
        json!({
            "type": "validation_error",
            "field": field,
            "reason": reason,
            "value": null
        })
    }

    /// Create timeout error
    pub fn timeout_error(operation: &str, timeout_ms: u64) -> Value {
        json!({
            "type": "timeout_error",
            "operation": operation,
            "timeout_ms": timeout_ms,
            "actual_duration_ms": timeout_ms + 100
        })
    }

    /// Create resource error
    pub fn resource_error(resource_type: &str, operation: &str) -> Value {
        json!({
            "type": "resource_error",
            "resource_type": resource_type,
            "operation": operation,
            "reason": "resource_unavailable"
        })
    }
}