use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Service identifier and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Unique service identifier
    pub id: Uuid,
    /// Human-readable service name
    pub name: String,
    /// Service type/category
    pub service_type: ServiceType,
    /// Service version
    pub version: String,
    /// Service description
    pub description: Option<String>,
    /// Service status
    pub status: ServiceStatus,
    /// Service capabilities
    pub capabilities: Vec<String>,
    /// Service configuration schema
    pub config_schema: Option<serde_json::Value>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Types of services in the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ServiceType {
    /// Tool execution service
    Tool,
    /// Database storage service
    Database,
    /// LLM/AI service
    LLM,
    /// Configuration management service
    Config,
    /// Authentication/authorization service
    Auth,
    /// File system service
    FileSystem,
    /// Network service
    Network,
    /// Custom service type
    Custom(String),
}

impl ServiceType {
    /// Get the string representation of the service type
    pub fn as_str(&self) -> &str {
        match self {
            ServiceType::Tool => "tool",
            ServiceType::Database => "database",
            ServiceType::LLM => "llm",
            ServiceType::Config => "config",
            ServiceType::Auth => "auth",
            ServiceType::FileSystem => "filesystem",
            ServiceType::Network => "network",
            ServiceType::Custom(name) => name,
        }
    }

    /// Create service type from string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "tool" => ServiceType::Tool,
            "database" => ServiceType::Database,
            "llm" => ServiceType::LLM,
            "config" => ServiceType::Config,
            "auth" => ServiceType::Auth,
            "filesystem" => ServiceType::FileSystem,
            "network" => ServiceType::Network,
            _ => ServiceType::Custom(s.to_string()),
        }
    }
}

/// Service status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServiceStatus {
    /// Service is healthy and available
    Healthy,
    /// Service is starting up
    Starting,
    /// Service is shutting down
    Stopping,
    /// Service is unhealthy but may recover
    Degraded,
    /// Service is permanently unavailable
    Failed,
    /// Service is under maintenance
    Maintenance,
}

impl ServiceStatus {
    /// Check if service is available for requests
    pub fn is_available(&self) -> bool {
        matches!(self, ServiceStatus::Healthy | ServiceStatus::Degraded)
    }

    /// Check if service is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, ServiceStatus::Failed)
    }
}

/// Generic service request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRequest {
    /// Unique request identifier
    pub request_id: Uuid,
    /// Target service type
    pub service_type: ServiceType,
    /// Target service instance (optional)
    pub service_instance: Option<Uuid>,
    /// Request method/operation
    pub method: String,
    /// Request payload
    pub payload: serde_json::Value,
    /// Request metadata
    pub metadata: RequestMetadata,
    /// Timeout in milliseconds
    pub timeout_ms: Option<u64>,
}

/// Request metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMetadata {
    /// Request timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// User identifier (if applicable)
    pub user_id: Option<String>,
    /// Session identifier
    pub session_id: Option<String>,
    /// Authentication token
    pub auth_token: Option<String>,
    /// Client identifier
    pub client_id: Option<String>,
    /// Request priority
    pub priority: RequestPriority,
    /// Retry attempt number
    pub retry_count: u32,
    /// Trace context for distributed tracing
    pub trace_context: Option<HashMap<String, String>>,
}

/// Request priority levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RequestPriority {
    /// Low priority (background tasks)
    Low = 0,
    /// Normal priority (default)
    Normal = 1,
    /// High priority (interactive requests)
    High = 2,
    /// Critical priority (system operations)
    Critical = 3,
}

impl Default for RequestPriority {
    fn default() -> Self {
        RequestPriority::Normal
    }
}

/// Generic service response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceResponse {
    /// Corresponding request identifier
    pub request_id: Uuid,
    /// Response status
    pub status: ResponseStatus,
    /// Response payload
    pub payload: serde_json::Value,
    /// Response metadata
    pub metadata: ResponseMetadata,
}

/// Response status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResponseStatus {
    /// Request completed successfully
    Success,
    /// Request failed with an error
    Error,
    /// Request was partially processed
    Partial,
}

/// Response metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMetadata {
    /// Response timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Processing duration in milliseconds
    pub duration_ms: u64,
    /// Service that handled the request
    pub service_id: Uuid,
    /// Additional response metadata
    pub metadata: HashMap<String, String>,
}

/// Service health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealth {
    /// Service identifier
    pub service_id: Uuid,
    /// Health status
    pub status: ServiceStatus,
    /// Last health check timestamp
    pub last_check: chrono::DateTime<chrono::Utc>,
    /// Health check metrics
    pub metrics: HashMap<String, f64>,
    /// Health check message
    pub message: Option<String>,
    /// Service uptime in seconds
    pub uptime_seconds: Option<u64>,
}

/// Service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Service identifier
    pub service_id: Uuid,
    /// Configuration values
    pub config: HashMap<String, serde_json::Value>,
    /// Environment-specific overrides
    pub environment_overrides: HashMap<String, HashMap<String, serde_json::Value>>,
    /// Configuration version
    pub version: u32,
    /// Last modified timestamp
    pub last_modified: chrono::DateTime<chrono::Utc>,
}

/// Service dependency information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDependency {
    /// Dependent service identifier
    pub service_id: Uuid,
    /// Required service type
    pub required_service_type: ServiceType,
    /// Required service instance (if specific)
    pub required_service_id: Option<Uuid>,
    /// Dependency type
    pub dependency_type: DependencyType,
    /// Whether this dependency is required for service to function
    pub required: bool,
}

/// Types of service dependencies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DependencyType {
    /// Hard dependency - required for operation
    Hard,
    /// Soft dependency - optional but enhances functionality
    Soft,
    /// Event-driven dependency - reacts to events
    Event,
}

/// Service metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMetrics {
    /// Service identifier
    pub service_id: Uuid,
    /// Metrics collection timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Request count
    pub request_count: u64,
    /// Success count
    pub success_count: u64,
    /// Error count
    pub error_count: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Current throughput (requests per second)
    pub throughput_rps: f64,
    /// Memory usage in bytes
    pub memory_usage_bytes: Option<u64>,
    /// CPU usage percentage
    pub cpu_usage_percent: Option<f64>,
    /// Custom metrics
    pub custom_metrics: HashMap<String, f64>,
}

impl ServiceMetrics {
    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        if self.request_count == 0 {
            0.0
        } else {
            self.success_count as f64 / self.request_count as f64
        }
    }

    /// Calculate error rate
    pub fn error_rate(&self) -> f64 {
        if self.request_count == 0 {
            0.0
        } else {
            self.error_count as f64 / self.request_count as f64
        }
    }
}

/// Service registration request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRegistration {
    /// Service information
    pub service_info: ServiceInfo,
    /// Service configuration
    pub config: Option<ServiceConfig>,
    /// Service dependencies
    pub dependencies: Vec<ServiceDependency>,
    /// Health check configuration
    pub health_check: Option<HealthCheckConfig>,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Health check interval in seconds
    pub interval_seconds: u64,
    /// Health check timeout in seconds
    pub timeout_seconds: u64,
    /// Number of consecutive failures before marking as unhealthy
    pub failure_threshold: u32,
    /// Number of consecutive successes before marking as healthy
    pub success_threshold: u32,
    /// Custom health check endpoint
    pub endpoint: Option<String>,
}

/// Tool-specific types
pub mod tool {
    use super::*;
    use serde::{Deserialize, Serialize};

    /// Tool definition
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToolDefinition {
        /// Tool name
        pub name: String,
        /// Tool description
        pub description: String,
        /// Tool parameters schema
        pub parameters: serde_json::Value,
        /// Tool return type schema
        pub returns: Option<serde_json::Value>,
        /// Tool category
        pub category: Option<String>,
        /// Tool tags
        pub tags: Vec<String>,
        /// Tool metadata
        pub metadata: HashMap<String, String>,
    }

    /// Tool execution request
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToolExecutionRequest {
        /// Tool name
        pub tool_name: String,
        /// Tool parameters
        pub parameters: serde_json::Value,
        /// Execution context
        pub context: ToolExecutionContext,
    }

    /// Tool execution context
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToolExecutionContext {
        /// User identifier
        pub user_id: Option<String>,
        /// Session identifier
        pub session_id: Option<String>,
        /// Working directory
        pub working_directory: Option<String>,
        /// Environment variables
        pub environment: HashMap<String, String>,
        /// Additional context
        pub context: HashMap<String, String>,
    }

    /// Tool execution result
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToolExecutionResult {
        /// Tool name
        pub tool_name: String,
        /// Execution success
        pub success: bool,
        /// Result data
        pub result: Option<serde_json::Value>,
        /// Error message (if failed)
        pub error: Option<String>,
        /// Execution duration in milliseconds
        pub duration_ms: u64,
        /// Additional metadata
        pub metadata: HashMap<String, String>,
    }
}

/// LLM-specific types
pub mod llm {
    use super::*;
    use serde::{Deserialize, Serialize};

    /// LLM model information
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ModelInfo {
        /// Model name
        pub name: String,
        /// Model provider
        pub provider: String,
        /// Model version
        pub version: Option<String>,
        /// Model capabilities
        pub capabilities: Vec<ModelCapability>,
        /// Context window size
        pub context_window: Option<usize>,
        /// Maximum output tokens
        pub max_output_tokens: Option<usize>,
        /// Model pricing information
        pub pricing: Option<ModelPricing>,
    }

    /// Model capabilities
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub enum ModelCapability {
        TextGeneration,
        ChatCompletion,
        Embeddings,
        FunctionCalling,
        CodeGeneration,
        ImageGeneration,
        AudioProcessing,
        Vision,
    }

    /// Model pricing information
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ModelPricing {
        /// Input token price per 1K tokens
        pub input_price_per_1k: f64,
        /// Output token price per 1K tokens
        pub output_price_per_1k: f64,
        /// Currency
        pub currency: String,
    }

    /// LLM completion request
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CompletionRequest {
        /// Model to use
        pub model: String,
        /// Prompt or messages
        pub input: CompletionInput,
        /// Generation parameters
        pub parameters: GenerationParameters,
        /// Request options
        pub options: CompletionOptions,
    }

    /// Completion input types
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "type")]
    pub enum CompletionInput {
        /// Single prompt
        Prompt { text: String },
        /// Chat messages
        Messages { messages: Vec<ChatMessage> },
    }

    /// Chat message
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ChatMessage {
        /// Message role
        pub role: MessageRole,
        /// Message content
        pub content: String,
        /// Tool calls (if any)
        pub tool_calls: Option<Vec<ToolCall>>,
        /// Tool call ID (if this is a tool response)
        pub tool_call_id: Option<String>,
    }

    /// Message roles
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub enum MessageRole {
        System,
        User,
        Assistant,
        Tool,
    }

    /// Tool call
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToolCall {
        /// Tool call ID
        pub id: String,
        /// Tool name
        pub name: String,
        /// Tool arguments
        pub arguments: serde_json::Value,
    }

    /// Generation parameters
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GenerationParameters {
        /// Maximum tokens to generate
        pub max_tokens: Option<u32>,
        /// Temperature (0.0 to 2.0)
        pub temperature: Option<f32>,
        /// Top-p sampling (0.0 to 1.0)
        pub top_p: Option<f32>,
        /// Top-k sampling
        pub top_k: Option<u32>,
        /// Presence penalty (-2.0 to 2.0)
        pub presence_penalty: Option<f32>,
        /// Frequency penalty (-2.0 to 2.0)
        pub frequency_penalty: Option<f32>,
        /// Stop sequences
        pub stop: Option<Vec<String>>,
        /// Log probabilities
        pub logprobs: Option<bool>,
        /// Number of log probabilities to return
        pub top_logprobs: Option<u32>,
    }

    /// Completion options
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CompletionOptions {
        /// Stream response
        pub stream: bool,
        /// Include usage information
        pub include_usage: bool,
        /// Seed for reproducible results
        pub seed: Option<u32>,
    }

    /// LLM completion response
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CompletionResponse {
        /// Generated text
        pub content: String,
        /// Model used
        pub model: String,
        /// Token usage information
        pub usage: TokenUsage,
        /// Finish reason
        pub finish_reason: FinishReason,
        /// Response metadata
        pub metadata: HashMap<String, String>,
    }

    /// Token usage information
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct TokenUsage {
        /// Input tokens used
        pub prompt_tokens: u32,
        /// Output tokens used
        pub completion_tokens: u32,
        /// Total tokens used
        pub total_tokens: u32,
    }

    /// Completion finish reasons
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub enum FinishReason {
        Stop,
        Length,
        ToolCalls,
        ContentFilter,
        FunctionCall,
    }

    /// Embedding request
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EmbeddingRequest {
        /// Model to use
        pub model: String,
        /// Text to embed
        pub input: Vec<String>,
        /// Request options
        pub options: EmbeddingOptions,
    }

    /// Embedding options
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EmbeddingOptions {
        /// Embedding dimensions
        pub dimensions: Option<u32>,
        /// User identifier
        pub user: Option<String>,
    }

    /// Embedding response
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EmbeddingResponse {
        /// Model used
        pub model: String,
        /// Embedding data
        pub data: Vec<EmbeddingData>,
        /// Token usage information
        pub usage: TokenUsage,
    }

    /// Single embedding data
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EmbeddingData {
        /// Embedding vector
        pub embedding: Vec<f32>,
        /// Index in the input array
        pub index: usize,
    }
}