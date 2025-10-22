//! # IPC Message Types
//!
//! Comprehensive message type definitions for the plugin IPC protocol with strong
//! typing, validation, and serialization support.

use crate::plugin_ipc::{error::IpcError, IpcResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Message header containing metadata and routing information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageHeader {
    /// Protocol version
    pub version: u8,
    /// Message type
    pub message_type: MessageType,
    /// Message flags
    pub flags: MessageFlags,
    /// Unique message identifier
    pub message_id: String,
    /// Session identifier
    pub session_id: String,
    /// Message timestamp (Unix nanoseconds)
    pub timestamp: u64,
    /// Source identifier
    pub source: String,
    /// Destination identifier (optional for broadcasts)
    pub destination: Option<String>,
    /// Correlation ID for request/response matching
    pub correlation_id: Option<String>,
    /// Request priority
    pub priority: MessagePriority,
    /// Message TTL in seconds
    pub ttl: Option<u32>,
    /// Message metadata
    pub metadata: HashMap<String, String>,
}

impl Default for MessageHeader {
    fn default() -> Self {
        Self {
            version: crate::plugin_ipc::PROTOCOL_VERSION,
            message_type: MessageType::Unknown,
            flags: MessageFlags::empty(),
            message_id: Uuid::new_v4().to_string(),
            session_id: Uuid::new_v4().to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            source: "unknown".to_string(),
            destination: None,
            correlation_id: None,
            priority: MessagePriority::Normal,
            ttl: None,
            metadata: HashMap::new(),
        }
    }
}

/// Complete IPC message with header and payload
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IpcMessage {
    /// Message header
    pub header: MessageHeader,
    /// Message payload
    pub payload: MessagePayload,
}

impl IpcMessage {
    /// Create a new message with the given type and payload
    pub fn new(message_type: MessageType, payload: MessagePayload) -> Self {
        let mut header = MessageHeader::default();
        header.message_type = message_type;

        Self { header, payload }
    }

    /// Create a request message
    pub fn request(destination: String, payload: RequestPayload) -> Self {
        let mut header = MessageHeader::default();
        header.message_type = MessageType::Request;
        header.destination = Some(destination);
        header.correlation_id = Some(Uuid::new_v4().to_string());

        Self {
            header,
            payload: MessagePayload::Request(payload),
        }
    }

    /// Create a response message
    pub fn response(correlation_id: String, payload: ResponsePayload) -> Self {
        let mut header = MessageHeader::default();
        header.message_type = MessageType::Response;
        header.correlation_id = Some(correlation_id);

        Self {
            header,
            payload: MessagePayload::Response(payload),
        }
    }

    /// Create an event message
    pub fn event(event_type: String, payload: EventPayload) -> Self {
        let mut header = MessageHeader::default();
        header.message_type = MessageType::Event;
        header.metadata.insert("event_type".to_string(), event_type);

        Self {
            header,
            payload: MessagePayload::Event(payload),
        }
    }

    /// Create an error message
    pub fn error(error: IpcError, correlation_id: Option<String>) -> Self {
        let mut header = MessageHeader::default();
        header.message_type = MessageType::Error;
        header.correlation_id = correlation_id;
        header.priority = MessagePriority::High;

        Self {
            header,
            payload: MessagePayload::Error(error.to_error_response()),
        }
    }

    /// Validate the message structure and content
    pub fn validate(&self) -> IpcResult<()> {
        // Validate header
        if self.header.version != crate::plugin_ipc::PROTOCOL_VERSION {
            return Err(IpcError::Protocol {
                message: format!(
                    "Version mismatch: expected {}, got {}",
                    crate::plugin_ipc::PROTOCOL_VERSION,
                    self.header.version
                ),
                code: crate::plugin_ipc::error::ProtocolErrorCode::VersionMismatch,
                source: None,
            });
        }

        if self.header.message_id.is_empty() {
            return Err(IpcError::Validation {
                message: "Message ID cannot be empty".to_string(),
                field: Some("message_id".to_string()),
                value: None,
                constraint: None,
            });
        }

        if self.header.session_id.is_empty() {
            return Err(IpcError::Validation {
                message: "Session ID cannot be empty".to_string(),
                field: Some("session_id".to_string()),
                value: None,
                constraint: None,
            });
        }

        // Validate payload
        self.payload.validate()?;

        // Validate message size
        let serialized = serde_json::to_vec(self)
            .map_err(|e| IpcError::Protocol {
                message: format!("Serialization failed: {}", e),
                code: crate::plugin_ipc::error::ProtocolErrorCode::SerializationFailed,
                source: None,
            })?;

        if serialized.len() > crate::plugin_ipc::MAX_MESSAGE_SIZE {
            return Err(IpcError::Message {
                message: format!(
                    "Message too large: {} bytes (max: {})",
                    serialized.len(),
                    crate::plugin_ipc::MAX_MESSAGE_SIZE
                ),
                code: crate::plugin_ipc::error::MessageErrorCode::MessageTooLarge,
                message_id: Some(self.header.message_id.clone()),
            });
        }

        Ok(())
    }

    /// Check if the message has expired
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.header.ttl {
            let age = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() - (self.header.timestamp / 1_000_000_000);
            age > ttl as u64
        } else {
            false
        }
    }

    /// Get message size in bytes
    pub fn size(&self) -> IpcResult<usize> {
        serde_json::to_vec(self)
            .map(|v| v.len())
            .map_err(|e| IpcError::Protocol {
                message: format!("Failed to calculate message size: {}", e),
                code: crate::plugin_ipc::error::ProtocolErrorCode::SerializationFailed,
                source: None,
            })
    }
}

/// Message type enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageType {
    /// Handshake message for connection establishment
    Handshake,
    /// Heartbeat for health checking
    Heartbeat,
    /// Request message
    Request,
    /// Response message
    Response,
    /// Event/notification message
    Event,
    /// Error message
    Error,
    /// Plugin registration
    PluginRegister,
    /// Plugin unregistration
    PluginUnregister,
    /// Capability query
    CapabilityQuery,
    /// Resource request
    ResourceRequest,
    /// Health check
    HealthCheck,
    /// Metrics report
    MetricsReport,
    /// Shutdown notification
    Shutdown,
    /// Unknown message type
    Unknown,
}

impl Default for MessageType {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Message flags for special handling
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct MessageFlags {
    /// Message is compressed
    pub compressed: bool,
    /// Message is encrypted
    pub encrypted: bool,
    /// Message requires acknowledgment
    pub ack_required: bool,
    /// Message is a duplicate/retry
    pub is_retry: bool,
    /// Message is part of a batch
    pub is_batch: bool,
    /// Message is streaming
    pub is_stream: bool,
    /// Message is high priority
    pub is_critical: bool,
}

impl MessageFlags {
    /// Create empty flags
    pub fn empty() -> Self {
        Self {
            compressed: false,
            encrypted: false,
            ack_required: false,
            is_retry: false,
            is_batch: false,
            is_stream: false,
            is_critical: false,
        }
    }

    /// Check if any flags are set
    pub fn is_empty(&self) -> bool {
        !(self.compressed
            || self.encrypted
            || self.ack_required
            || self.is_retry
            || self.is_batch
            || self.is_stream
            || self.is_critical)
    }
}

impl Default for MessageFlags {
    fn default() -> Self {
        Self::empty()
    }
}

/// Message priority levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    /// Low priority (background tasks)
    Low = 0,
    /// Normal priority (default)
    Normal = 1,
    /// High priority (important messages)
    High = 2,
    /// Critical priority (system messages)
    Critical = 3,
}

impl Default for MessagePriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Message payload variants
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessagePayload {
    /// Handshake payload
    Handshake(HandshakePayload),
    /// Heartbeat payload
    Heartbeat(HeartbeatPayload),
    /// Request payload
    Request(RequestPayload),
    /// Response payload
    Response(ResponsePayload),
    /// Event payload
    Event(EventPayload),
    /// Error payload
    Error(crate::plugin_ipc::error::ErrorResponse),
    /// Plugin registration payload
    PluginRegister(PluginRegisterPayload),
    /// Plugin unregistration payload
    PluginUnregister(PluginUnregisterPayload),
    /// Capability query payload
    CapabilityQuery(CapabilityQueryPayload),
    /// Resource request payload
    ResourceRequest(ResourceRequestPayload),
    /// Health check payload
    HealthCheck(HealthCheckPayload),
    /// Metrics report payload
    MetricsReport(MetricsReportPayload),
    /// Shutdown payload
    Shutdown(ShutdownPayload),
    /// Unknown payload
    Unknown(serde_json::Value),
}

impl MessagePayload {
    /// Validate the payload content
    pub fn validate(&self) -> IpcResult<()> {
        match self {
            MessagePayload::Handshake(payload) => payload.validate(),
            MessagePayload::Heartbeat(payload) => payload.validate(),
            MessagePayload::Request(payload) => payload.validate(),
            MessagePayload::Response(payload) => payload.validate(),
            MessagePayload::Event(payload) => payload.validate(),
            MessagePayload::Error(_) => Ok(()), // Error payloads are always valid
            MessagePayload::PluginRegister(payload) => payload.validate(),
            MessagePayload::PluginUnregister(payload) => payload.validate(),
            MessagePayload::CapabilityQuery(payload) => payload.validate(),
            MessagePayload::ResourceRequest(payload) => payload.validate(),
            MessagePayload::HealthCheck(payload) => payload.validate(),
            MessagePayload::MetricsReport(payload) => payload.validate(),
            MessagePayload::Shutdown(payload) => payload.validate(),
            MessagePayload::Unknown(_) => Ok(()), // Unknown payloads are not validated
        }
    }
}

/// Handshake payload for connection establishment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HandshakePayload {
    /// Protocol version
    pub protocol_version: u8,
    /// Client/plugin ID
    pub client_id: String,
    /// Authentication token
    pub auth_token: String,
    /// Supported message types
    pub supported_types: Vec<MessageType>,
    /// Supported compression algorithms
    pub compression_algos: Vec<String>,
    /// Supported encryption algorithms
    pub encryption_algos: Vec<String>,
    /// Maximum message size
    pub max_message_size: usize,
    /// Client capabilities
    pub capabilities: ClientCapabilities,
    /// Connection metadata
    pub metadata: HashMap<String, String>,
}

impl HandshakePayload {
    pub fn validate(&self) -> IpcResult<()> {
        if self.client_id.is_empty() {
            return Err(IpcError::Validation {
                message: "Client ID cannot be empty".to_string(),
                field: Some("client_id".to_string()),
                value: None,
                constraint: None,
            });
        }

        if self.auth_token.is_empty() {
            return Err(IpcError::Validation {
                message: "Auth token cannot be empty".to_string(),
                field: Some("auth_token".to_string()),
                value: None,
                constraint: None,
            });
        }

        if self.max_message_size == 0 {
            return Err(IpcError::Validation {
                message: "Max message size must be greater than 0".to_string(),
                field: Some("max_message_size".to_string()),
                value: Some(self.max_message_size.to_string()),
                constraint: Some("> 0".to_string()),
            });
        }

        Ok(())
    }
}

/// Heartbeat payload for health checking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeartbeatPayload {
    /// Current status
    pub status: HeartbeatStatus,
    /// Last activity timestamp
    pub last_activity: u64,
    /// Resource usage information
    pub resource_usage: ResourceUsage,
    /// Performance metrics
    pub metrics: HashMap<String, f64>,
    /// Custom status data
    pub status_data: HashMap<String, String>,
}

impl HeartbeatPayload {
    pub fn validate(&self) -> IpcResult<()> {
        // Heartbeat payloads are generally valid if they have a status
        Ok(())
    }
}

/// Heartbeat status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HeartbeatStatus {
    /// Plugin is healthy and ready
    Healthy,
    /// Plugin is degraded but functional
    Degraded,
    /// Plugin is unhealthy
    Unhealthy,
    /// Plugin is starting up
    Starting,
    /// Plugin is shutting down
    ShuttingDown,
    /// Plugin has crashed
    Crashed,
}

/// Resource usage information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceUsage {
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// CPU usage percentage (0.0-100.0)
    pub cpu_percentage: f64,
    /// Disk usage in bytes
    pub disk_bytes: u64,
    /// Network usage in bytes
    pub network_bytes: u64,
    /// Number of open file descriptors
    pub open_files: u32,
    /// Number of active threads
    pub active_threads: u32,
}

/// Request payload for plugin operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequestPayload {
    /// Operation to perform
    pub operation: String,
    /// Operation parameters
    pub parameters: serde_json::Value,
    /// Execution context
    pub context: ExecutionContext,
    /// Timeout in milliseconds
    pub timeout_ms: Option<u64>,
    /// Request metadata
    pub metadata: HashMap<String, String>,
}

impl RequestPayload {
    pub fn validate(&self) -> IpcResult<()> {
        if self.operation.is_empty() {
            return Err(IpcError::Validation {
                message: "Operation cannot be empty".to_string(),
                field: Some("operation".to_string()),
                value: None,
                constraint: None,
            });
        }

        // Validate timeout if provided
        if let Some(timeout) = self.timeout_ms {
            if timeout == 0 {
                return Err(IpcError::Validation {
                    message: "Timeout must be greater than 0".to_string(),
                    field: Some("timeout_ms".to_string()),
                    value: Some(timeout.to_string()),
                    constraint: Some("> 0".to_string()),
                });
            }
        }

        Ok(())
    }
}

/// Execution context for requests
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionContext {
    /// User ID
    pub user_id: Option<String>,
    /// Session ID
    pub session_id: Option<String>,
    /// Request ID
    pub request_id: String,
    /// Working directory
    pub working_directory: Option<String>,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// Security context
    pub security_context: SecurityContext,
    /// Time limits
    pub time_limits: TimeLimits,
    /// Resource limits
    pub resource_limits: ResourceLimits,
}

/// Security context information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecurityContext {
    /// Security level
    pub security_level: String,
    /// Allowed operations
    pub allowed_operations: Vec<String>,
    /// Blocked operations
    pub blocked_operations: Vec<String>,
    /// Sandbox configuration
    pub sandbox_config: serde_json::Value,
}

/// Time limits for execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeLimits {
    /// Maximum CPU time in seconds
    pub max_cpu_time: Option<u32>,
    /// Maximum wall time in seconds
    pub max_wall_time: Option<u32>,
    /// Maximum real time in seconds
    pub max_real_time: Option<u32>,
}

/// Resource limits
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceLimits {
    /// Maximum memory in bytes
    pub max_memory: Option<u64>,
    /// Maximum disk space in bytes
    pub max_disk: Option<u64>,
    /// Maximum number of processes
    pub max_processes: Option<u32>,
    /// Maximum number of files
    pub max_files: Option<u32>,
}

/// Response payload for request results
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponsePayload {
    /// Request success status
    pub success: bool,
    /// Response data
    pub data: Option<serde_json::Value>,
    /// Error information (if unsuccessful)
    pub error: Option<ErrorInfo>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Resource usage during execution
    pub resource_usage: ResourceUsage,
    /// Response metadata
    pub metadata: HashMap<String, String>,
}

impl ResponsePayload {
    pub fn validate(&self) -> IpcResult<()> {
        if self.success && self.error.is_some() {
            return Err(IpcError::Validation {
                message: "Successful response cannot contain error information".to_string(),
                field: None,
                value: None,
                constraint: None,
            });
        }

        if !self.success && self.error.is_none() {
            return Err(IpcError::Validation {
                message: "Unsuccessful response must contain error information".to_string(),
                field: None,
                value: None,
                constraint: None,
            });
        }

        Ok(())
    }
}

/// Error information for responses
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorInfo {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Error details
    pub details: Option<serde_json::Value>,
    /// Stack trace (if available)
    pub stack_trace: Option<String>,
}

/// Event payload for notifications
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventPayload {
    /// Event type
    pub event_type: String,
    /// Event data
    pub data: serde_json::Value,
    /// Event source
    pub source: String,
    /// Event timestamp
    pub event_timestamp: u64,
    /// Event severity
    pub severity: EventSeverity,
    /// Event metadata
    pub metadata: HashMap<String, String>,
}

impl EventPayload {
    pub fn validate(&self) -> IpcResult<()> {
        if self.event_type.is_empty() {
            return Err(IpcError::Validation {
                message: "Event type cannot be empty".to_string(),
                field: Some("event_type".to_string()),
                value: None,
                constraint: None,
            });
        }

        if self.source.is_empty() {
            return Err(IpcError::Validation {
                message: "Event source cannot be empty".to_string(),
                field: Some("source".to_string()),
                value: None,
                constraint: None,
            });
        }

        Ok(())
    }
}

/// Event severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventSeverity {
    /// Debug information
    Debug = 0,
    /// Informational messages
    Info = 1,
    /// Warning messages
    Warning = 2,
    /// Error messages
    Error = 3,
    /// Critical errors
    Critical = 4,
}

/// Client capabilities advertised during handshake
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientCapabilities {
    /// Supported plugin types
    pub plugin_types: Vec<String>,
    /// Supported operations
    pub operations: Vec<String>,
    /// Supported data formats
    pub data_formats: Vec<String>,
    /// Maximum concurrent requests
    pub max_concurrent_requests: u32,
    /// Streaming support
    pub supports_streaming: bool,
    /// Batch processing support
    pub supports_batching: bool,
    /// Compression support
    pub supports_compression: bool,
    /// Encryption support
    pub supports_encryption: bool,
}

/// Plugin registration payload
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginRegisterPayload {
    /// Plugin ID
    pub plugin_id: String,
    /// Plugin name
    pub plugin_name: String,
    /// Plugin version
    pub plugin_version: String,
    /// Plugin description
    pub description: String,
    /// Plugin capabilities
    pub capabilities: ClientCapabilities,
    /// Plugin metadata
    pub metadata: HashMap<String, String>,
}

impl PluginRegisterPayload {
    pub fn validate(&self) -> IpcResult<()> {
        if self.plugin_id.is_empty() {
            return Err(IpcError::Validation {
                message: "Plugin ID cannot be empty".to_string(),
                field: Some("plugin_id".to_string()),
                value: None,
                constraint: None,
            });
        }

        if self.plugin_name.is_empty() {
            return Err(IpcError::Validation {
                message: "Plugin name cannot be empty".to_string(),
                field: Some("plugin_name".to_string()),
                value: None,
                constraint: None,
            });
        }

        Ok(())
    }
}

/// Plugin unregistration payload
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginUnregisterPayload {
    /// Plugin ID to unregister
    pub plugin_id: String,
    /// Reason for unregistration
    pub reason: Option<String>,
    /// Graceful shutdown flag
    pub graceful: bool,
}

impl PluginUnregisterPayload {
    pub fn validate(&self) -> IpcResult<()> {
        if self.plugin_id.is_empty() {
            return Err(IpcError::Validation {
                message: "Plugin ID cannot be empty".to_string(),
                field: Some("plugin_id".to_string()),
                value: None,
                constraint: None,
            });
        }

        Ok(())
    }
}

/// Capability query payload
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityQueryPayload {
    /// Query type
    pub query_type: CapabilityQueryType,
    /// Query parameters
    pub parameters: HashMap<String, String>,
    /// Specific capabilities to query
    pub capabilities: Option<Vec<String>>,
}

impl CapabilityQueryPayload {
    pub fn validate(&self) -> IpcResult<()> {
        // Capability queries are generally valid
        Ok(())
    }
}

/// Capability query types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CapabilityQueryType {
    /// Query all capabilities
    All,
    /// Query specific capabilities
    Specific,
    /// Query plugin information
    PluginInfo,
    /// Query system capabilities
    System,
}

/// Resource request payload
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceRequestPayload {
    /// Resource type
    pub resource_type: ResourceType,
    /// Resource amount
    pub amount: u64,
    /// Request priority
    pub priority: MessagePriority,
    /// Request duration in seconds
    pub duration: Option<u32>,
    /// Request metadata
    pub metadata: HashMap<String, String>,
}

impl ResourceRequestPayload {
    pub fn validate(&self) -> IpcResult<()> {
        if self.amount == 0 {
            return Err(IpcError::Validation {
                message: "Resource amount must be greater than 0".to_string(),
                field: Some("amount".to_string()),
                value: Some(self.amount.to_string()),
                constraint: Some("> 0".to_string()),
            });
        }

        Ok(())
    }
}

/// Resource types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResourceType {
    /// Memory resource
    Memory,
    /// CPU resource
    Cpu,
    /// Disk space
    Disk,
    /// Network bandwidth
    Network,
    /// File descriptors
    FileDescriptors,
    /// Custom resource
    Custom(String),
}

/// Health check payload
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthCheckPayload {
    /// Check type
    pub check_type: HealthCheckType,
    /// Check parameters
    pub parameters: HashMap<String, String>,
}

impl HealthCheckPayload {
    pub fn validate(&self) -> IpcResult<()> {
        // Health check payloads are generally valid
        Ok(())
    }
}

/// Health check types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthCheckType {
    /// Basic liveness check
    Liveness,
    /// Readiness check
    Readiness,
    /// Comprehensive health check
    Full,
    /// Custom check
    Custom(String),
}

/// Metrics report payload
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsReportPayload {
    /// Metrics collection
    pub metrics: MetricsCollection,
    /// Report timestamp
    pub timestamp: u64,
    /// Report interval in seconds
    pub interval: u32,
}

impl MetricsReportPayload {
    pub fn validate(&self) -> IpcResult<()> {
        if self.interval == 0 {
            return Err(IpcError::Validation {
                message: "Report interval must be greater than 0".to_string(),
                field: Some("interval".to_string()),
                value: Some(self.interval.to_string()),
                constraint: Some("> 0".to_string()),
            });
        }

        Ok(())
    }
}

/// Metrics collection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsCollection {
    /// Performance metrics
    pub performance: HashMap<String, f64>,
    /// Resource metrics
    pub resources: HashMap<String, u64>,
    /// Custom metrics
    pub custom: HashMap<String, serde_json::Value>,
}

/// Shutdown payload
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShutdownPayload {
    /// Shutdown reason
    pub reason: ShutdownReason,
    /// Graceful shutdown flag
    pub graceful: bool,
    /// Timeout in seconds
    pub timeout: Option<u32>,
    /// Shutdown message
    pub message: Option<String>,
}

impl ShutdownPayload {
    pub fn validate(&self) -> IpcResult<()> {
        if let Some(timeout) = self.timeout {
            if timeout == 0 {
                return Err(IpcError::Validation {
                    message: "Shutdown timeout must be greater than 0".to_string(),
                    field: Some("timeout".to_string()),
                    value: Some(timeout.to_string()),
                    constraint: Some("> 0".to_string()),
                });
            }
        }

        Ok(())
    }
}

/// Shutdown reasons
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ShutdownReason {
    /// Normal shutdown
    Normal,
    /// Maintenance shutdown
    Maintenance,
    /// Error shutdown
    Error,
    /// Timeout shutdown
    Timeout,
    /// Resource exhaustion
    ResourceExhaustion,
    /// Security violation
    SecurityViolation,
    /// Custom reason
    Custom(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let message = IpcMessage::new(
            MessageType::Handshake,
            MessagePayload::Handshake(HandshakePayload {
                protocol_version: 1,
                client_id: "test_client".to_string(),
                auth_token: "test_token".to_string(),
                supported_types: vec![MessageType::Request, MessageType::Response],
                compression_algos: vec!["lz4".to_string()],
                encryption_algos: vec!["aes256".to_string()],
                max_message_size: 1024 * 1024,
                capabilities: ClientCapabilities {
                    plugin_types: vec!["test".to_string()],
                    operations: vec!["test_op".to_string()],
                    data_formats: vec!["json".to_string()],
                    max_concurrent_requests: 10,
                    supports_streaming: false,
                    supports_batching: true,
                    supports_compression: true,
                    supports_encryption: true,
                },
                metadata: HashMap::new(),
            }),
        );

        assert_eq!(message.header.message_type, MessageType::Handshake);
        assert!(message.validate().is_ok());
    }

    #[test]
    fn test_request_response_pair() {
        let request = IpcMessage::request(
            "test_plugin".to_string(),
            RequestPayload {
                operation: "test_operation".to_string(),
                parameters: serde_json::json!({"param": "value"}),
                context: ExecutionContext {
                    user_id: Some("user123".to_string()),
                    session_id: Some("session456".to_string()),
                    request_id: "req789".to_string(),
                    working_directory: None,
                    environment: HashMap::new(),
                    security_context: SecurityContext {
                        security_level: "basic".to_string(),
                        allowed_operations: vec!["test_operation".to_string()],
                        blocked_operations: vec![],
                        sandbox_config: serde_json::json!({}),
                    },
                    time_limits: TimeLimits {
                        max_cpu_time: Some(30),
                        max_wall_time: Some(60),
                        max_real_time: Some(90),
                    },
                    resource_limits: ResourceLimits {
                        max_memory: Some(1024 * 1024 * 1024), // 1GB
                        max_disk: None,
                        max_processes: Some(10),
                        max_files: Some(100),
                    },
                },
                timeout_ms: Some(5000),
                metadata: HashMap::new(),
            },
        );

        assert!(request.validate().is_ok());
        assert_eq!(request.header.destination, Some("test_plugin".to_string()));
        assert!(request.header.correlation_id.is_some());

        let response = IpcMessage::response(
            request.header.correlation_id.unwrap(),
            ResponsePayload {
                success: true,
                data: Some(serde_json::json!({"result": "success"})),
                error: None,
                execution_time_ms: 100,
                resource_usage: ResourceUsage {
                    memory_bytes: 10 * 1024 * 1024, // 10MB
                    cpu_percentage: 5.0,
                    disk_bytes: 0,
                    network_bytes: 1024,
                    open_files: 5,
                    active_threads: 2,
                },
                metadata: HashMap::new(),
            },
        );

        assert!(response.validate().is_ok());
        assert_eq!(response.header.correlation_id, request.header.correlation_id);
    }

    #[test]
    fn test_message_validation() {
        // Test invalid message (empty message ID)
        let mut invalid_message = IpcMessage::new(MessageType::Heartbeat, MessagePayload::Heartbeat(HeartbeatPayload {
            status: HeartbeatStatus::Healthy,
            last_activity: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64,
            resource_usage: ResourceUsage {
                memory_bytes: 0,
                cpu_percentage: 0.0,
                disk_bytes: 0,
                network_bytes: 0,
                open_files: 0,
                active_threads: 0,
            },
            metrics: HashMap::new(),
            status_data: HashMap::new(),
        }));

        invalid_message.header.message_id = "".to_string();
        assert!(invalid_message.validate().is_err());

        // Test version mismatch
        invalid_message.header.message_id = "valid_id".to_string();
        invalid_message.header.version = 255;
        assert!(invalid_message.validate().is_err());
    }

    #[test]
    fn test_message_flags() {
        let mut flags = MessageFlags::empty();
        assert!(flags.is_empty());

        flags.compressed = true;
        flags.encrypted = true;
        assert!(!flags.is_empty());

        let serialized = serde_json::to_value(&flags).unwrap();
        assert!(serialized.get("compressed").unwrap().as_bool().unwrap());
        assert!(serialized.get("encrypted").unwrap().as_bool().unwrap());
        assert!(!serialized.get("ack_required").unwrap().as_bool().unwrap());
    }

    #[test]
    fn test_message_priority() {
        assert!(MessagePriority::Critical > MessagePriority::High);
        assert!(MessagePriority::High > MessagePriority::Normal);
        assert!(MessagePriority::Normal > MessagePriority::Low);
    }

    #[test]
    fn test_error_message() {
        let error = IpcError::Validation {
            message: "Test validation error".to_string(),
            field: Some("test_field".to_string()),
            value: Some("invalid_value".to_string()),
            constraint: Some("must be valid".to_string()),
        };

        let error_message = IpcMessage::error(error.clone(), Some("correlation_123".to_string()));
        assert_eq!(error_message.header.message_type, MessageType::Error);
        assert_eq!(error_message.header.correlation_id, Some("correlation_123".to_string()));
        assert_eq!(error_message.header.priority, MessagePriority::High);

        if let MessagePayload::Error(error_response) = error_message.payload {
            assert_eq!(error_response.message, error.to_string());
            assert!(!error_response.retryable);
        } else {
            panic!("Expected error payload");
        }
    }
}