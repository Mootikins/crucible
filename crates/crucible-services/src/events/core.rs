//! Core event types and structures for daemon coordination

use super::errors::{EventError, EventResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Core event that flows through the daemon coordination system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonEvent {
    /// Unique identifier for this event
    pub id: Uuid,

    /// Type of the event
    pub event_type: EventType,

    /// Event priority for ordering and processing
    pub priority: EventPriority,

    /// Source of the event
    pub source: EventSource,

    /// Target service(s) for routing (empty = broadcast)
    pub targets: Vec<ServiceTarget>,

    /// Timestamp when the event was created
    pub created_at: DateTime<Utc>,

    /// Optional timestamp when the event should be processed
    pub scheduled_at: Option<DateTime<Utc>>,

    /// Event payload data
    pub payload: EventPayload,

    /// Event metadata for debugging and monitoring
    pub metadata: EventMetadata,

    /// Correlation ID for tracking related events
    pub correlation_id: Option<Uuid>,

    /// Causation ID - the event that caused this event
    pub causation_id: Option<Uuid>,

    /// Number of times this event has been retried
    pub retry_count: u32,

    /// Maximum retry attempts allowed
    pub max_retries: u32,
}

impl DaemonEvent {
    /// Create a new event with minimal required fields
    pub fn new(event_type: EventType, source: EventSource, payload: EventPayload) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            priority: EventPriority::Normal,
            source,
            targets: Vec::new(),
            created_at: Utc::now(),
            scheduled_at: None,
            payload,
            metadata: EventMetadata::new(),
            correlation_id: None,
            causation_id: None,
            retry_count: 0,
            max_retries: 3,
        }
    }

    /// Create an event with correlation ID for tracking
    pub fn with_correlation(
        event_type: EventType,
        source: EventSource,
        payload: EventPayload,
        correlation_id: Uuid,
    ) -> Self {
        let mut event = Self::new(event_type, source, payload);
        event.correlation_id = Some(correlation_id);
        event
    }

    /// Create an event as a response to another event
    pub fn as_response(
        event_type: EventType,
        source: EventSource,
        payload: EventPayload,
        causation_id: Uuid,
    ) -> Self {
        let mut event = Self::new(event_type, source, payload);
        event.causation_id = Some(causation_id);
        event
    }

    /// Set event priority
    pub fn with_priority(mut self, priority: EventPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Add target service
    pub fn with_target(mut self, target: ServiceTarget) -> Self {
        self.targets.push(target);
        self
    }

    /// Add multiple target services
    pub fn with_targets(mut self, targets: Vec<ServiceTarget>) -> Self {
        self.targets = targets;
        self
    }

    /// Set scheduled processing time
    pub fn with_schedule(mut self, scheduled_at: DateTime<Utc>) -> Self {
        self.scheduled_at = Some(scheduled_at);
        self
    }

    /// Add metadata field
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.add_field(key, value);
        self
    }

    /// Set max retry attempts
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Check if event should be retried
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// Increment retry count
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }

    /// Check if event is scheduled for future processing
    pub fn is_scheduled(&self) -> bool {
        self.scheduled_at.map_or(false, |scheduled| scheduled > Utc::now())
    }

    /// Get event size in bytes (approximate)
    pub fn size_bytes(&self) -> usize {
        serde_json::to_string(self)
            .map(|s| s.len())
            .unwrap_or(0)
    }

    /// Validate event structure
    pub fn validate(&self) -> EventResult<()> {
        // Validate targets
        if self.targets.is_empty() && !self.event_type.is_broadcast_allowed() {
            return Err(EventError::ValidationError(
                "Event requires specific targets but none provided".to_string(),
            ));
        }

        // Validate payload size (configurable limit)
        const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10MB
        if self.size_bytes() > MAX_PAYLOAD_SIZE {
            return Err(EventError::EventTooLarge {
                size: self.size_bytes(),
                max_size: MAX_PAYLOAD_SIZE,
            });
        }

        // Validate priority
        if !matches!(self.priority, EventPriority::Critical | EventPriority::High | EventPriority::Normal | EventPriority::Low) {
            return Err(EventError::InvalidPriority(format!("{:?}", self.priority)));
        }

        Ok(())
    }
}

/// Event type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum EventType {
    /// File system events
    Filesystem(FilesystemEventType),

    /// Database events
    Database(DatabaseEventType),

    /// External data events
    External(ExternalEventType),

    /// MCP (Model Context Protocol) events
    Mcp(McpEventType),

    /// Service coordination events
    Service(ServiceEventType),

    /// System events
    System(SystemEventType),

    /// Custom event types
    Custom(String),
}

impl EventType {
    /// Check if event type supports broadcast
    pub fn is_broadcast_allowed(&self) -> bool {
        matches!(
            self,
            EventType::System(_) |
            EventType::Service(ServiceEventType::HealthCheck { .. }) |
            EventType::Service(ServiceEventType::ServiceRegistered { .. }) |
            EventType::Service(ServiceEventType::ServiceUnregistered { .. })
        )
    }

    /// Get event category for routing
    pub fn category(&self) -> EventCategory {
        match self {
            EventType::Filesystem(_) => EventCategory::Filesystem,
            EventType::Database(_) => EventCategory::Database,
            EventType::External(_) => EventCategory::External,
            EventType::Mcp(_) => EventCategory::Mcp,
            EventType::Service(_) => EventCategory::Service,
            EventType::System(_) => EventCategory::System,
            EventType::Custom(_) => EventCategory::Custom,
        }
    }
}

/// Event category for routing and filtering
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EventCategory {
    Filesystem,
    Database,
    External,
    Mcp,
    Service,
    System,
    Custom,
}

/// File system event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FilesystemEventType {
    FileCreated { path: String },
    FileModified { path: String },
    FileDeleted { path: String },
    FileMoved { from: String, to: String },
    DirectoryCreated { path: String },
    DirectoryDeleted { path: String },
    BatchChange { changes: Vec<FilesystemChange> },
}

/// File system change representation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilesystemChange {
    pub path: String,
    pub change_type: String,
    pub timestamp: DateTime<Utc>,
}

/// Database event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DatabaseEventType {
    RecordCreated { table: String, id: String },
    RecordUpdated { table: String, id: String, changes: HashMap<String, serde_json::Value> },
    RecordDeleted { table: String, id: String },
    TableCreated { name: String },
    TableDropped { name: String },
    SchemaChanged { table: String, changes: Vec<SchemaChange> },
    TransactionStarted { id: String },
    TransactionCommitted { id: String },
    TransactionRolledBack { id: String },
}

/// Schema change representation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaChange {
    pub column: String,
    pub change_type: String,
    pub old_type: Option<String>,
    pub new_type: Option<String>,
}

/// External data event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExternalEventType {
    DataReceived { source: String, data: serde_json::Value },
    WebhookTriggered { url: String, payload: serde_json::Value },
    ApiCallCompleted { endpoint: String, status: u16, response: serde_json::Value },
    StreamDataReceived { stream: String, data: Vec<u8> },
    NotificationReceived { channel: String, message: String },
}

/// MCP (Model Context Protocol) event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum McpEventType {
    ToolCall { tool_name: String, parameters: serde_json::Value },
    ToolResponse { tool_name: String, result: serde_json::Value },
    ToolError { tool_name: String, error: String },
    ResourceRequested { resource_type: String, parameters: serde_json::Value },
    ResourceProvided { resource_type: String, data: serde_json::Value },
    ContextUpdated { context_id: String, changes: HashMap<String, serde_json::Value> },
}

/// Service coordination event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceEventType {
    ServiceRegistered { service_id: String, service_type: String },
    ServiceUnregistered { service_id: String },
    HealthCheck { service_id: String, status: String },
    ServiceStatusChanged { service_id: String, old_status: String, new_status: String },
    ConfigurationChanged { service_id: String, changes: HashMap<String, serde_json::Value> },
    RequestReceived { from_service: String, to_service: String, request: serde_json::Value },
    ResponseSent { from_service: String, to_service: String, response: serde_json::Value },
    ServiceRestarted { service_id: String },
}

/// System event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SystemEventType {
    DaemonStarted { version: String },
    DaemonStopped { reason: Option<String> },
    ConfigurationReloaded { config_hash: String },
    EmergencyShutdown { reason: String },
    MaintenanceStarted { reason: String },
    MaintenanceCompleted { reason: String },
    MetricsCollected { metrics: HashMap<String, f64> },
    LogRotated { log_file: String },
    BackupCompleted { backup_path: String, size_bytes: u64 },
}

/// Event priority levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventPriority {
    Critical = 0,
    High = 1,
    Normal = 2,
    Low = 3,
}

impl EventPriority {
    /// Get priority as numeric value for sorting
    pub fn value(&self) -> u8 {
        match self {
            EventPriority::Critical => 0,
            EventPriority::High => 1,
            EventPriority::Normal => 2,
            EventPriority::Low => 3,
        }
    }
}

/// Event source identification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventSource {
    /// Source identifier (service name, system component, etc.)
    pub id: String,

    /// Source type
    pub source_type: SourceType,

    /// Source instance (useful for multiple instances of same service)
    pub instance: Option<String>,

    /// Source metadata
    pub metadata: HashMap<String, String>,
}

impl EventSource {
    /// Create a new event source
    pub fn new(id: String, source_type: SourceType) -> Self {
        Self {
            id,
            source_type,
            instance: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a service source
    pub fn service(service_id: String) -> Self {
        Self::new(service_id, SourceType::Service)
    }

    /// Create a filesystem source
    pub fn filesystem(watch_id: String) -> Self {
        Self::new(watch_id, SourceType::Filesystem)
    }

    /// Create an external source
    pub fn external(source_id: String) -> Self {
        Self::new(source_id, SourceType::External)
    }

    /// Add source metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Set source instance
    pub fn with_instance(mut self, instance: String) -> Self {
        self.instance = Some(instance);
        self
    }
}

/// Source type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SourceType {
    /// Service-generated event
    Service,

    /// File system watcher
    Filesystem,

    /// Database trigger
    Database,

    /// External system/webhook
    External,

    /// MCP protocol
    Mcp,

    /// System component
    System,

    /// Manual/administrative
    Manual,

    /// Custom source type
    Custom(String),
}

/// Service target for routing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceTarget {
    /// Target service identifier
    pub service_id: String,

    /// Target service type
    pub service_type: Option<String>,

    /// Target instance (for multi-instance services)
    pub instance: Option<String>,

    /// Target priority (for load balancing)
    pub priority: u8,

    /// Target filters
    pub filters: Vec<EventFilter>,
}

impl ServiceTarget {
    /// Create a new service target
    pub fn new(service_id: String) -> Self {
        Self {
            service_id,
            service_type: None,
            instance: None,
            priority: 0,
            filters: Vec::new(),
        }
    }

    /// Create target with service type
    pub fn with_type(mut self, service_type: String) -> Self {
        self.service_type = Some(service_type);
        self
    }

    /// Create target with specific instance
    pub fn with_instance(mut self, instance: String) -> Self {
        self.instance = Some(instance);
        self
    }

    /// Create target with priority
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// Add event filter
    pub fn with_filter(mut self, filter: EventFilter) -> Self {
        self.filters.push(filter);
        self
    }
}

/// Event payload
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventPayload {
    /// Payload data
    pub data: serde_json::Value,

    /// Payload content type
    pub content_type: String,

    /// Payload encoding
    pub encoding: String,

    /// Payload size in bytes
    pub size_bytes: usize,

    /// Payload checksum for integrity verification
    pub checksum: Option<String>,
}

impl EventPayload {
    /// Create a new JSON payload
    pub fn json(data: serde_json::Value) -> Self {
        let json_str = serde_json::to_string(&data).unwrap_or_default();
        Self {
            data,
            content_type: "application/json".to_string(),
            encoding: "utf-8".to_string(),
            size_bytes: json_str.len(),
            checksum: None,
        }
    }

    /// Create a new text payload
    pub fn text(text: String) -> Self {
        let size = text.len();
        Self {
            data: serde_json::Value::String(text),
            content_type: "text/plain".to_string(),
            encoding: "utf-8".to_string(),
            size_bytes: size,
            checksum: None,
        }
    }

    /// Create a new binary payload
    pub fn binary(data: Vec<u8>, content_type: String) -> Self {
        let size = data.len();
        let checksum = Some(format!("{:x}", md5::compute(&data)));
        Self {
            data: serde_json::Value::String(base64::encode(&data)),
            content_type,
            encoding: "base64".to_string(),
            size_bytes: size,
            checksum,
        }
    }

    /// Get payload as string
    pub fn as_string(&self) -> Option<String> {
        match &self.data {
            serde_json::Value::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// Get payload as JSON value
    pub fn as_json(&self) -> Option<serde_json::Value> {
        if self.content_type == "application/json" {
            Some(self.data.clone())
        } else {
            None
        }
    }

    /// Verify payload integrity
    pub fn verify_integrity(&self) -> bool {
        if let Some(expected_checksum) = &self.checksum {
            let computed_checksum = match self.encoding.as_str() {
                "base64" => {
                    if let Some(encoded) = self.as_string() {
                        if let Ok(decoded) = base64::decode(&encoded) {
                            Some(format!("{:x}", md5::compute(&decoded)))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                "utf-8" => {
                    if let Some(text) = self.as_string() {
                        Some(format!("{:x}", md5::compute(text.as_bytes())))
                    } else {
                        None
                    }
                }
                _ => None,
            };

            computed_checksum.as_ref() == Some(expected_checksum)
        } else {
            true // No checksum to verify
        }
    }
}

/// Event metadata for debugging and monitoring
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventMetadata {
    /// Custom metadata fields
    pub fields: HashMap<String, String>,

    /// Processing metrics
    pub metrics: EventMetrics,

    /// Debug information
    pub debug: DebugInfo,
}

impl EventMetadata {
    /// Create new metadata
    pub fn new() -> Self {
        Self {
            fields: HashMap::new(),
            metrics: EventMetrics::new(),
            debug: DebugInfo::new(),
        }
    }

    /// Add metadata field
    pub fn add_field(&mut self, key: String, value: String) {
        self.fields.insert(key, value);
    }

    /// Get metadata field
    pub fn get_field(&self, key: &str) -> Option<&String> {
        self.fields.get(key)
    }

    /// Update processing metrics
    pub fn update_metrics<F>(&mut self, updater: F)
    where
        F: FnOnce(&mut EventMetrics),
    {
        updater(&mut self.metrics);
    }

    /// Add debug information
    pub fn add_debug_info(&mut self, key: String, value: String) {
        self.debug.add_info(key, value);
    }
}

/// Event processing metrics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventMetrics {
    /// Processing start time
    pub processing_started_at: Option<DateTime<Utc>>,

    /// Processing duration
    pub processing_duration_ms: Option<u64>,

    /// Queue wait time
    pub queue_wait_ms: Option<u64>,

    /// Number of processing attempts
    pub processing_attempts: u32,

    /// Services that processed this event
    pub processed_by: Vec<String>,

    /// Services that failed to process this event
    pub failed_by: Vec<String>,
}

impl EventMetrics {
    /// Create new metrics
    pub fn new() -> Self {
        Self {
            processing_started_at: None,
            processing_duration_ms: None,
            queue_wait_ms: None,
            processing_attempts: 0,
            processed_by: Vec::new(),
            failed_by: Vec::new(),
        }
    }

    /// Mark processing start
    pub fn start_processing(&mut self) {
        self.processing_started_at = Some(Utc::now());
        self.processing_attempts += 1;
    }

    /// Mark processing completion
    pub fn complete_processing(&mut self) {
        if let Some(started_at) = self.processing_started_at {
            self.processing_duration_ms = Some(
                (Utc::now() - started_at).num_milliseconds() as u64
            );
        }
    }

    /// Record service success
    pub fn add_success(&mut self, service_id: String) {
        if !self.processed_by.contains(&service_id) {
            self.processed_by.push(service_id);
        }
    }

    /// Record service failure
    pub fn add_failure(&mut self, service_id: String) {
        if !self.failed_by.contains(&service_id) {
            self.failed_by.push(service_id);
        }
    }
}

/// Debug information for events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DebugInfo {
    /// Debug information fields
    pub info: HashMap<String, String>,

    /// Stack trace if available
    pub stack_trace: Option<String>,

    /// Source code location if available
    pub source_location: Option<SourceLocation>,
}

impl DebugInfo {
    /// Create new debug info
    pub fn new() -> Self {
        Self {
            info: HashMap::new(),
            stack_trace: None,
            source_location: None,
        }
    }

    /// Add debug information
    pub fn add_info(&mut self, key: String, value: String) {
        self.info.insert(key, value);
    }

    /// Set stack trace
    pub fn with_stack_trace(mut self, trace: String) -> Self {
        self.stack_trace = Some(trace);
        self
    }

    /// Set source location
    pub fn with_source_location(mut self, location: SourceLocation) -> Self {
        self.source_location = Some(location);
        self
    }
}

/// Source code location information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub function: Option<String>,
}

/// Event filter for routing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventFilter {
    /// Filter by event type
    pub event_types: Vec<String>,

    /// Filter by event category
    pub categories: Vec<EventCategory>,

    /// Filter by priority
    pub priorities: Vec<EventPriority>,

    /// Filter by source
    pub sources: Vec<String>,

    /// Custom filter expression
    pub expression: Option<String>,

    /// Maximum payload size
    pub max_payload_size: Option<usize>,
}

impl EventFilter {
    /// Create new filter
    pub fn new() -> Self {
        Self {
            event_types: Vec::new(),
            categories: Vec::new(),
            priorities: Vec::new(),
            sources: Vec::new(),
            expression: None,
            max_payload_size: None,
        }
    }

    /// Check if event matches this filter
    pub fn matches(&self, event: &DaemonEvent) -> bool {
        // Check event types
        if !self.event_types.is_empty() {
            let event_type_str = match &event.event_type {
                EventType::Filesystem(_) => "filesystem",
                EventType::Database(_) => "database",
                EventType::External(_) => "external",
                EventType::Mcp(_) => "mcp",
                EventType::Service(_) => "service",
                EventType::System(_) => "system",
                EventType::Custom(name) => name,
            };
            if !self.event_types.contains(&event_type_str.to_string()) {
                return false;
            }
        }

        // Check categories
        if !self.categories.is_empty() && !self.categories.contains(&event.event_type.category()) {
            return false;
        }

        // Check priorities
        if !self.priorities.is_empty() && !self.priorities.contains(&event.priority) {
            return false;
        }

        // Check sources
        if !self.sources.is_empty() && !self.sources.contains(&event.source.id) {
            return false;
        }

        // Check payload size
        if let Some(max_size) = self.max_payload_size {
            if event.payload.size_bytes > max_size {
                return false;
            }
        }

        // Custom expression evaluation (simplified)
        if let Some(expr) = &self.expression {
            // In a real implementation, this would use a proper expression evaluator
            // For now, just check if the expression contains known keywords
            let event_str = format!("{:?}", event);
            if !expr.split_whitespace().all(|word| event_str.contains(word)) {
                return false;
            }
        }

        true
    }
}

impl Default for EventFilter {
    fn default() -> Self {
        Self::new()
    }
}

