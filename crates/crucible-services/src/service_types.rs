//! # Service Type Definitions
//!
//! This module contains comprehensive type definitions for all service traits,
//! including configurations, requests, responses, and supporting types.

use super::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use chrono::{DateTime, Utc};

/// ============================================================================
/// COMMON SERVICE TYPES
/// ============================================================================

/// Enhanced performance metrics for services
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Request processing times in milliseconds
    pub request_times: Vec<f64>,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// CPU usage percentage
    pub cpu_usage: f64,
    /// Active connections count
    pub active_connections: u32,
    /// Queue sizes
    pub queue_sizes: HashMap<String, u32>,
    /// Custom metrics
    pub custom_metrics: HashMap<String, f64>,
    /// Timestamp of metrics collection
    pub timestamp: DateTime<Utc>,
}

/// Resource usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// CPU usage percentage (0.0-100.0)
    pub cpu_percentage: f64,
    /// Disk usage in bytes
    pub disk_bytes: u64,
    /// Network usage in bytes (incoming + outgoing)
    pub network_bytes: u64,
    /// Number of open file descriptors
    pub open_files: u32,
    /// Number of active threads
    pub active_threads: u32,
    /// Timestamp of measurement
    pub measured_at: DateTime<Utc>,
}

/// Resource limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory in bytes
    pub max_memory_bytes: Option<u64>,
    /// Maximum CPU percentage
    pub max_cpu_percentage: Option<f64>,
    /// Maximum disk space in bytes
    pub max_disk_bytes: Option<u64>,
    /// Maximum concurrent operations
    pub max_concurrent_operations: Option<u32>,
    /// Maximum queue size
    pub max_queue_size: Option<u32>,
    /// Timeout for operations
    pub operation_timeout: Option<Duration>,
}

/// System health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    /// Overall system status
    pub status: ServiceStatus,
    /// Individual service health
    pub services: HashMap<String, ServiceHealth>,
    /// System-wide resource usage
    pub resource_usage: ResourceUsage,
    /// System uptime
    pub uptime: Duration,
    /// Last health check timestamp
    pub last_check: DateTime<Utc>,
}

/// Service information for registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Service name
    pub name: String,
    /// Service version
    pub version: String,
    /// Service status
    pub status: ServiceStatus,
    /// Service type
    pub service_type: String,
    /// Health check endpoint
    pub health_endpoint: Option<String>,
    /// Metrics endpoint
    pub metrics_endpoint: Option<String>,
    /// Configuration
    pub configuration: Option<serde_json::Value>,
    /// Start time
    pub started_at: Option<DateTime<Utc>>,
}

/// ============================================================================
/// MCP GATEWAY TYPES
/// ============================================================================

/// MCP session information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSession {
    /// Unique session identifier
    pub session_id: String,
    /// Client identifier
    pub client_id: String,
    /// Session status
    pub status: McpSessionStatus,
    /// Server capabilities
    pub server_capabilities: McpCapabilities,
    /// Client capabilities
    pub client_capabilities: McpCapabilities,
    /// Session metadata
    pub metadata: HashMap<String, String>,
    /// Created at timestamp
    pub created_at: DateTime<Utc>,
    /// Last activity timestamp
    pub last_activity: DateTime<Utc>,
}

/// MCP session status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum McpSessionStatus {
    /// Session is being established
    Connecting,
    /// Session is active and ready
    Active,
    /// Session is being closed
    Closing,
    /// Session is closed
    Closed,
    /// Session encountered an error
    Error(String),
}

/// MCP capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCapabilities {
    /// Tool calling capabilities
    pub tools: Option<ToolCapabilities>,
    /// Resource capabilities
    pub resources: Option<ResourceCapabilities>,
    /// Logging capabilities
    pub logging: Option<LoggingCapabilities>,
    /// Sampling capabilities
    pub sampling: Option<SamplingCapabilities>,
}

/// Tool capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCapabilities {
    /// List tool capability
    pub list_tools: Option<bool>,
    /// Call tool capability
    pub call_tool: Option<bool>,
    /// Subscribe to tool capability
    pub subscribe_to_tools: Option<bool>,
}

/// Resource capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCapabilities {
    /// Subscribe to resource capability
    pub subscribe_to_resources: Option<bool>,
    /// Read resource capability
    pub read_resource: Option<bool>,
    /// List resources capability
    pub list_resources: Option<bool>,
}

/// Logging capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingCapabilities {
    /// Set log level capability
    pub set_log_level: Option<bool>,
    /// Get log messages capability
    pub get_log_messages: Option<bool>,
}

/// Sampling capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingCapabilities {
    /// Create message capability
    pub create_message: Option<bool>,
}

/// MCP notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpNotification {
    /// Notification method
    pub method: String,
    /// Notification parameters
    pub params: serde_json::Value,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// MCP request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    /// Request ID
    pub id: Option<String>,
    /// Request method
    pub method: String,
    /// Request parameters
    pub params: serde_json::Value,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// MCP response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    /// Request ID
    pub id: String,
    /// Response result (if successful)
    pub result: Option<serde_json::Value>,
    /// Response error (if failed)
    pub error: Option<McpError>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// MCP error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Error data
    pub data: Option<serde_json::Value>,
}

/// MCP tool request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolRequest {
    /// Tool name
    pub tool_name: String,
    /// Tool arguments
    pub arguments: HashMap<String, serde_json::Value>,
    /// Session ID
    pub session_id: String,
    /// Request ID
    pub request_id: String,
    /// Timeout in milliseconds
    pub timeout_ms: Option<u64>,
}

/// MCP tool response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResponse {
    /// Request ID
    pub request_id: String,
    /// Tool result
    pub result: Option<serde_json::Value>,
    /// Tool error
    pub error: Option<String>,
    /// Execution time
    pub execution_time: Duration,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionStatus {
    /// Execution is pending
    Pending,
    /// Execution is running
    Running,
    /// Execution completed successfully
    Completed,
    /// Execution failed
    Failed(String),
    /// Execution was cancelled
    Cancelled,
    /// Execution timed out
    Timeout,
}

/// Active execution information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveExecution {
    /// Execution ID
    pub execution_id: String,
    /// Tool name
    pub tool_name: String,
    /// Session ID
    pub session_id: String,
    /// Execution status
    pub status: ExecutionStatus,
    /// Started at timestamp
    pub started_at: DateTime<Utc>,
    /// Progress percentage (0.0-100.0)
    pub progress: Option<f32>,
}

/// MCP resource usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceUsage {
    /// Active sessions count
    pub active_sessions: u32,
    /// Active executions count
    pub active_executions: u32,
    /// Registered tools count
    pub registered_tools: u32,
    /// Memory usage for MCP operations
    pub memory_usage: u64,
    /// Network usage
    pub network_usage: u64,
}

/// MCP protocol settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpProtocolSettings {
    /// Maximum concurrent sessions
    pub max_sessions: Option<u32>,
    /// Session timeout in seconds
    pub session_timeout_seconds: Option<u64>,
    /// Maximum request size in bytes
    pub max_request_size: Option<u64>,
    /// Enable compression
    pub enable_compression: Option<bool>,
    /// Enable transport encryption
    pub enable_encryption: Option<bool>,
}

/// ============================================================================
/// INFERENCE ENGINE TYPES
/// ============================================================================

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Model name or identifier
    pub name: String,
    /// Model provider (openai, anthropic, local, etc.)
    pub provider: String,
    /// Model version
    pub version: Option<String>,
    /// Model parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// API key or credentials
    pub api_key: Option<String>,
    /// Base URL for API
    pub base_url: Option<String>,
    /// Maximum tokens for generation
    pub max_tokens: Option<u32>,
    /// Temperature for generation
    pub temperature: Option<f32>,
    /// Top p for nucleus sampling
    pub top_p: Option<f32>,
}

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model unique identifier
    pub model_id: String,
    /// Model name
    pub name: String,
    /// Model provider
    pub provider: String,
    /// Model version
    pub version: Option<String>,
    /// Model type
    pub model_type: ModelType,
    /// Model capabilities
    pub capabilities: Vec<ModelCapability>,
    /// Context window size
    pub context_window: u32,
    /// Model size in parameters
    pub parameter_count: Option<u64>,
    /// Model loaded status
    pub loaded: bool,
    /// Memory usage in bytes
    pub memory_usage: Option<u64>,
    /// Load timestamp
    pub loaded_at: Option<DateTime<Utc>>,
}

/// Model type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelType {
    /// Text generation model
    TextGeneration,
    /// Embedding model
    Embedding,
    /// Multimodal model
    Multimodal,
    /// Code generation model
    CodeGeneration,
    /// Chat model
    Chat,
}

/// Model capability
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelCapability {
    /// Text completion
    TextCompletion,
    /// Chat completion
    ChatCompletion,
    /// Embedding generation
    Embedding,
    /// Function calling
    FunctionCalling,
    /// Code generation
    CodeGeneration,
    /// Reasoning
    Reasoning,
    /// Tool use
    ToolUse,
    /// Vision/image processing
    Vision,
    /// Audio processing
    Audio,
}

/// Completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Model to use
    pub model: String,
    /// Prompt text
    pub prompt: String,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Temperature for generation
    pub temperature: Option<f32>,
    /// Top p for nucleus sampling
    pub top_p: Option<f32>,
    /// Frequency penalty
    pub frequency_penalty: Option<f32>,
    /// Presence penalty
    pub presence_penalty: Option<f32>,
    /// Stop sequences
    pub stop: Option<Vec<String>>,
    /// Number of completions to generate
    pub n: Option<u32>,
    /// Echo the prompt in the response
    pub echo: Option<bool>,
}

/// Completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Generated completions
    pub completions: Vec<Completion>,
    /// Model used
    pub model: String,
    /// Usage information
    pub usage: TokenUsage,
    /// Request ID
    pub request_id: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Individual completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Completion {
    /// Generated text
    pub text: String,
    /// Completion index
    pub index: u32,
    /// Log probabilities
    pub logprobs: Option<LogProbs>,
    /// Finish reason
    pub finish_reason: Option<String>,
}

/// Completion chunk for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionChunk {
    /// Partial text
    pub text: String,
    /// Chunk index
    pub index: u32,
    /// Finish reason if complete
    pub finish_reason: Option<String>,
}

/// Log probabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogProbs {
    /// Token probabilities
    pub tokens: Vec<String>,
    /// Log probabilities
    pub token_logprobs: Vec<f32>,
    /// Top log probabilities
    pub top_logprobs: Vec<HashMap<String, f32>>,
    /// Byte offsets
    pub bytes_offset: Vec<u32>,
}

/// Chat completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    /// Model to use
    pub model: String,
    /// Conversation messages
    pub messages: Vec<ChatMessage>,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Temperature for generation
    pub temperature: Option<f32>,
    /// Top p for nucleus sampling
    pub top_p: Option<f32>,
    /// Function calling configuration
    pub functions: Option<Vec<FunctionDefinition>>,
    /// Function call behavior
    pub function_call: Option<FunctionCallBehavior>,
    /// System prompt
    pub system: Option<String>,
    /// Stop sequences
    pub stop: Option<Vec<String>>,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Message role
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// Function call (if any)
    pub function_call: Option<FunctionCall>,
    /// Tool calls (if any)
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Message name (for function results)
    pub name: Option<String>,
}

/// Message role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Function,
    Tool,
}

/// Function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Function name
    pub name: String,
    /// Function description
    pub description: String,
    /// Function parameters schema
    pub parameters: Option<serde_json::Value>,
}

/// Function call behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FunctionCallBehavior {
    /// Auto mode (model decides)
    Auto,
    /// Force function call
    Force(String),
    /// No function call
    None,
}

/// Function call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Function name
    pub name: String,
    /// Function arguments (JSON string)
    pub arguments: String,
}

/// Tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool call ID
    pub id: String,
    /// Tool call type
    pub r#type: String,
    /// Function call
    pub function: FunctionCall,
}

/// Chat completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    /// Chat message choices
    pub choices: Vec<ChatChoice>,
    /// Model used
    pub model: String,
    /// Usage information
    pub usage: TokenUsage,
    /// Request ID
    pub request_id: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Finish reasons
    pub finish_reason: Vec<String>,
}

/// Chat choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChoice {
    /// Message index
    pub index: u32,
    /// Chat message
    pub message: ChatMessage,
    /// Finish reason
    pub finish_reason: Option<String>,
}

/// Chat completion chunk for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    /// Choice index
    pub index: u32,
    /// Delta message
    pub delta: ChatMessageDelta,
    /// Finish reason
    pub finish_reason: Option<String>,
}

/// Chat message delta for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageDelta {
    /// Message role (may be omitted)
    pub role: Option<MessageRole>,
    /// Message content delta
    pub content: Option<String>,
    /// Function call delta
    pub function_call: Option<FunctionCall>,
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Prompt tokens used
    pub prompt_tokens: u32,
    /// Completion tokens used
    pub completion_tokens: u32,
    /// Total tokens used
    pub total_tokens: u32,
}

/// Embedding request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    /// Model to use
    pub model: String,
    /// Input text(s)
    pub input: EmbeddingInput,
    /// Encoding format
    pub encoding_format: Option<EncodingFormat>,
    /// Dimensions of embedding
    pub dimensions: Option<u32>,
}

/// Embedding input (single text or array)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    /// Single string
    String(String),
    /// Array of strings
    Array(Vec<String>),
}

/// Encoding format for embeddings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EncodingFormat {
    /// Float format
    Float,
    /// Base64 format
    Base64,
}

/// Embedding response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    /// Embedding data
    pub data: Vec<Embedding>,
    /// Model used
    pub model: String,
    /// Usage information
    pub usage: TokenUsage,
    /// Request ID
    pub request_id: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Single embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    /// Embedding index
    pub index: u32,
    /// Embedding object
    pub object: String,
    /// Embedding vector
    pub embedding: Vec<f32>,
}

/// Batch embedding request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchEmbeddingRequest {
    /// Model to use
    pub model: String,
    /// Input texts
    pub inputs: Vec<String>,
    /// Encoding format
    pub encoding_format: Option<EncodingFormat>,
    /// Dimensions of embedding
    pub dimensions: Option<u32>,
    /// Batch size
    pub batch_size: Option<u32>,
}

/// Batch embedding response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchEmbeddingResponse {
    /// Embedding data
    pub data: Vec<Embedding>,
    /// Model used
    pub model: String,
    /// Usage information
    pub usage: TokenUsage,
    /// Request ID
    pub request_id: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Reasoning request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningRequest {
    /// Model to use
    pub model: String,
    /// Task description
    pub task: String,
    /// Context information
    pub context: Option<String>,
    /// Examples for few-shot learning
    pub examples: Option<Vec<ReasoningExample>>,
    /// Maximum reasoning steps
    pub max_steps: Option<u32>,
}

/// Reasoning example
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningExample {
    /// Input text
    pub input: String,
    /// Reasoning steps
    pub reasoning: Vec<String>,
    /// Output text
    pub output: String,
}

/// Reasoning response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningResponse {
    /// Reasoning result
    pub result: String,
    /// Reasoning steps
    pub steps: Vec<ReasoningStep>,
    /// Confidence score
    pub confidence: f32,
    /// Model used
    pub model: String,
    /// Request ID
    pub request_id: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Reasoning step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    /// Step number
    pub step: u32,
    /// Step description
    pub description: String,
    /// Step result
    pub result: Option<String>,
    /// Confidence in this step
    pub confidence: f32,
}

/// Tool use request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseRequest {
    /// Model to use
    pub model: String,
    /// Task description
    pub task: String,
    /// Available tools
    pub tools: Vec<FunctionDefinition>,
    /// Conversation context
    pub messages: Option<Vec<ChatMessage>>,
    /// Maximum tool calls
    pub max_tool_calls: Option<u32>,
}

/// Tool use response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseResponse {
    /// Generated text
    pub text: String,
    /// Tool calls to make
    pub tool_calls: Vec<ToolCall>,
    /// Model used
    pub model: String,
    /// Request ID
    pub request_id: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Semantic search request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchRequest {
    /// Model to use
    pub model: String,
    /// Query text
    pub query: String,
    /// Documents to search through
    pub documents: Vec<String>,
    /// Top k results
    pub top_k: Option<u32>,
}

/// Semantic search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchResponse {
    /// Search results
    pub results: Vec<SemanticSearchResult>,
    /// Model used
    pub model: String,
    /// Request ID
    pub request_id: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Semantic search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchResult {
    /// Document index
    pub index: u32,
    /// Document text
    pub document: String,
    /// Similarity score
    pub similarity_score: f32,
}

/// Fine-tuning request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuningRequest {
    /// Base model to fine-tune
    pub base_model: String,
    /// Training data
    pub training_data: Vec<FineTuningExample>,
    /// Validation data
    pub validation_data: Option<Vec<FineTuningExample>>,
    /// Fine-tuning parameters
    pub parameters: FineTuningParameters,
    /// Number of training epochs
    pub epochs: Option<u32>,
    /// Batch size
    pub batch_size: Option<u32>,
    /// Learning rate
    pub learning_rate: Option<f32>,
}

/// Fine-tuning example
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuningExample {
    /// Input text
    pub input: String,
    /// Expected output text
    pub output: String,
    /// Example weight (optional)
    pub weight: Option<f32>,
}

/// Fine-tuning parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuningParameters {
    /// Early stopping patience
    pub early_stopping_patience: Option<u32>,
    /// Warmup steps
    pub warmup_steps: Option<u32>,
    /// Weight decay
    pub weight_decay: Option<f32>,
    /// Dropout rate
    pub dropout: Option<f32>,
}

/// Fine-tuning job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuningJob {
    /// Job ID
    pub job_id: String,
    /// Base model
    pub base_model: String,
    /// Job status
    pub status: FineTuningJobStatus,
    /// Started at timestamp
    pub started_at: DateTime<Utc>,
    /// Estimated completion
    pub estimated_completion: Option<DateTime<Utc>>,
}

/// Fine-tuning job status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FineTuningJobStatus {
    Queued,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

/// Fine-tuning status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuningStatus {
    /// Job ID
    pub job_id: String,
    /// Current status
    pub status: FineTuningJobStatus,
    /// Progress percentage
    pub progress: f32,
    /// Current epoch
    pub current_epoch: Option<u32>,
    /// Training loss
    pub training_loss: Option<f32>,
    /// Validation loss
    pub validation_loss: Option<f32>,
    /// Estimated time remaining
    pub estimated_time_remaining: Option<Duration>,
    /// Started at timestamp
    pub started_at: DateTime<Utc>,
    /// Updated at timestamp
    pub updated_at: DateTime<Utc>,
}

/// Model optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelOptimization {
    /// Optimization type
    pub optimization_type: OptimizationType,
    /// Optimization parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Target metric
    pub target_metric: Option<String>,
}

/// Optimization type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OptimizationType {
    /// Quantization optimization
    Quantization,
    /// Pruning optimization
    Pruning,
    /// Distillation optimization
    Distillation,
    /// Knowledge distillation
    KnowledgeDistillation,
    /// LoRA (Low-Rank Adaptation)
    LoRA,
    /// Custom optimization
    Custom(String),
}

/// Model resource usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResourceUsage {
    /// Model ID
    pub model_id: String,
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// GPU memory usage in bytes
    pub gpu_memory_bytes: Option<u64>,
    /// Number of active requests
    pub active_requests: u32,
    /// Average response time
    pub average_response_time: Duration,
    /// Requests per minute
    pub requests_per_minute: f32,
}

/// Inference limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceLimits {
    /// Maximum concurrent requests
    pub max_concurrent_requests: Option<u32>,
    /// Maximum request size in tokens
    pub max_request_tokens: Option<u32>,
    /// Maximum response size in tokens
    pub max_response_tokens: Option<u32>,
    /// Request timeout
    pub request_timeout: Option<Duration>,
    /// Queue size limit
    pub max_queue_size: Option<u32>,
}

/// Inference statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceStatistics {
    /// Total requests
    pub total_requests: u64,
    /// Successful requests
    pub successful_requests: u64,
    /// Failed requests
    pub failed_requests: u64,
    /// Average response time
    pub average_response_time: Duration,
    /// Requests per minute
    pub requests_per_minute: f32,
    /// Token usage statistics
    pub token_usage: TokenUsage,
    /// Error rates by type
    pub error_rates: HashMap<String, f32>,
    /// Model-specific statistics
    pub model_stats: HashMap<String, ModelStatistics>,
}

/// Model-specific statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStatistics {
    /// Model name
    pub model: String,
    /// Request count
    pub request_count: u64,
    /// Success rate
    pub success_rate: f32,
    /// Average response time
    pub average_response_time: Duration,
    /// Token usage
    pub token_usage: TokenUsage,
}

/// ============================================================================
/// SCRIPT ENGINE TYPES
/// ============================================================================

/// Compilation context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationContext {
    /// Compilation target
    pub target: CompilationTarget,
    /// Optimization level
    pub optimization_level: OptimizationLevel,
    /// Include paths
    pub include_paths: Vec<String>,
    /// Preprocessor definitions
    pub definitions: HashMap<String, String>,
    /// Debug information
    pub debug_info: bool,
}

/// Compilation target
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompilationTarget {
    /// Standard execution
    Standard,
    /// Tool execution
    Tool,
    /// Library compilation
    Library,
    /// Debug mode
    Debug,
}

/// Optimization level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OptimizationLevel {
    /// No optimization
    None,
    /// Basic optimization
    Basic,
    /// Aggressive optimization
    Aggressive,
}

/// Compiled script
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledScript {
    /// Unique script identifier
    pub script_id: String,
    /// Original source code
    pub source: String,
    /// Compiled bytecode
    pub bytecode: Vec<u8>,
    /// Compilation metadata
    pub metadata: CompilationMetadata,
    /// Security validation results
    pub security_validation: SecurityValidationResult,
    /// Compilation timestamp
    pub compiled_at: DateTime<Utc>,
}

/// Compilation metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationMetadata {
    /// Script language/version
    pub language: String,
    /// Version
    pub version: String,
    /// Compilation warnings
    pub warnings: Vec<CompilationWarning>,
    /// Compilation duration
    pub compilation_time: Duration,
    /// Size of compiled code
    pub compiled_size: u32,
    /// Dependencies
    pub dependencies: Vec<String>,
    /// Exports
    pub exports: Vec<String>,
}

/// Compilation warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationWarning {
    /// Warning message
    pub message: String,
    /// Warning level
    pub level: WarningLevel,
    /// Source location
    pub location: SourceLocation,
}

/// Warning level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WarningLevel {
    Info,
    Warning,
    Error,
}

/// Source location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLocation {
    /// File name
    pub file: Option<String>,
    /// Line number
    pub line: Option<u32>,
    /// Column number
    pub column: Option<u32>,
}

/// Compilation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationError {
    /// Error message
    pub message: String,
    /// Error code
    pub code: String,
    /// Source location
    pub location: SourceLocation,
    /// Error severity
    pub severity: ErrorSeverity,
}

/// Error severity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ErrorSeverity {
    Error,
    Fatal,
    Warning,
    Info,
}

/// Compilation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationResult {
    /// Success status
    pub success: bool,
    /// Compiled script (if successful)
    pub script: Option<CompiledScript>,
    /// Errors encountered
    pub errors: Vec<CompilationError>,
    /// Warnings encountered
    pub warnings: Vec<CompilationWarning>,
    /// Compilation duration
    pub duration: Duration,
}

/// Execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    /// Execution ID
    pub execution_id: String,
    /// Script ID
    pub script_id: String,
    /// Input arguments
    pub arguments: HashMap<String, serde_json::Value>,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// Working directory
    pub working_directory: Option<String>,
    /// Security context
    pub security_context: SecurityContext,
    /// Execution timeout
    pub timeout: Option<Duration>,
    /// Available tools
    pub available_tools: Vec<String>,
    /// User context
    pub user_context: Option<UserContext>,
}

/// Security context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityContext {
    /// Security level
    pub level: SecurityLevel,
    /// Allowed operations
    pub allowed_operations: Vec<String>,
    /// Blocked operations
    pub blocked_operations: Vec<String>,
    /// Resource limits
    pub resource_limits: ResourceLimits,
    /// Sandbox configuration
    pub sandbox_config: SandboxConfig,
}

/// Security level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SecurityLevel {
    /// No security restrictions
    None,
    /// Basic security
    Basic,
    /// Strict security
    Strict,
    /// Maximum security
    Maximum,
}

/// Sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Enable sandbox
    pub enabled: bool,
    /// Sandbox type
    pub sandbox_type: SandboxType,
    /// Isolated filesystem
    pub isolated_filesystem: bool,
    /// Network access
    pub network_access: bool,
    /// System calls allowed
    pub allowed_syscalls: Vec<String>,
}

/// Sandbox type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SandboxType {
    /// Process isolation
    Process,
    /// Container isolation
    Container,
    /// Virtual machine isolation
    VirtualMachine,
    /// Language-level isolation
    Language,
}

/// User context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    /// User ID
    pub user_id: String,
    /// User permissions
    pub permissions: Vec<String>,
    /// User groups
    pub groups: Vec<String>,
    /// Session ID
    pub session_id: Option<String>,
}

/// Execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Execution ID
    pub execution_id: String,
    /// Success status
    pub success: bool,
    /// Return value
    pub return_value: Option<serde_json::Value>,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Execution duration
    pub execution_time: Duration,
    /// Memory usage
    pub memory_usage: u64,
    /// Execution statistics
    pub statistics: ExecutionStatistics,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStatistics {
    /// Instructions executed
    pub instructions_executed: u64,
    /// Function calls made
    pub function_calls: u32,
    /// System calls made
    pub system_calls: u32,
    /// Memory allocated
    pub memory_allocated: u64,
    /// Memory deallocated
    pub memory_deallocated: u64,
    /// Peak memory usage
    pub peak_memory: u64,
}

/// Execution chunk for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionChunk {
    /// Execution ID
    pub execution_id: String,
    /// Chunk type
    pub chunk_type: ExecutionChunkType,
    /// Chunk data
    pub data: serde_json::Value,
    /// Chunk sequence number
    pub sequence: u64,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Execution chunk type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionChunkType {
    /// Standard output
    Stdout,
    /// Standard error
    Stderr,
    /// Progress update
    Progress,
    /// Result
    Result,
    /// Error
    Error,
    /// Status update
    Status,
}

/// Script tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptTool {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Function signature
    pub signature: String,
    /// Parameters
    pub parameters: Vec<ToolParameter>,
    /// Return type
    pub return_type: String,
    /// Script ID that implements this tool
    pub script_id: String,
    /// Function name in script
    pub function_name: String,
    /// Tool metadata
    pub metadata: HashMap<String, String>,
    /// Tool version
    pub version: Option<String>,
    /// Tool author
    pub author: Option<String>,
}

/// Tool parameter (redefined from existing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameter {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub param_type: String,
    /// Parameter description
    pub description: Option<String>,
    /// Required flag
    pub required: bool,
    /// Default value
    pub default_value: Option<serde_json::Value>,
    /// Validation rules
    pub validation: Option<ValidationRules>,
}

/// Validation rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRules {
    /// Minimum value (for numbers)
    pub min: Option<f64>,
    /// Maximum value (for numbers)
    pub max: Option<f64>,
    /// Minimum length (for strings/arrays)
    pub min_length: Option<u32>,
    /// Maximum length (for strings/arrays)
    pub max_length: Option<u32>,
    /// Pattern (for strings)
    pub pattern: Option<String>,
    /// Allowed values
    pub enum_values: Option<Vec<serde_json::Value>>,
}

/// Script information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptInfo {
    /// Script ID
    pub script_id: String,
    /// Script name
    pub name: String,
    /// Script description
    pub description: Option<String>,
    /// Script author
    pub author: Option<String>,
    /// Script version
    pub version: Option<String>,
    /// Script language
    pub language: String,
    /// Created at timestamp
    pub created_at: DateTime<Utc>,
    /// Updated at timestamp
    pub updated_at: DateTime<Utc>,
    /// Script size in bytes
    pub size_bytes: u64,
    /// Execution count
    pub execution_count: u64,
    /// Last executed
    pub last_executed: Option<DateTime<Utc>>,
    /// Security level
    pub security_level: SecurityLevel,
    /// Tags
    pub tags: Vec<String>,
}

/// Security policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// Policy name
    pub name: String,
    /// Policy version
    pub version: String,
    /// Default security level
    pub default_level: SecurityLevel,
    /// Allowed operations
    pub allowed_operations: HashMap<SecurityLevel, Vec<String>>,
    /// Blocked operations
    pub blocked_operations: HashMap<SecurityLevel, Vec<String>>,
    /// Resource limits by level
    pub resource_limits: HashMap<SecurityLevel, ResourceLimits>,
    /// Sandbox configuration by level
    pub sandbox_configs: HashMap<SecurityLevel, SandboxConfig>,
}

/// Security validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityValidationResult {
    /// Validation passed
    pub passed: bool,
    /// Security level assigned
    pub security_level: SecurityLevel,
    /// Security issues found
    pub issues: Vec<SecurityIssue>,
    /// Recommendations
    pub recommendations: Vec<String>,
    /// Validation timestamp
    pub validated_at: DateTime<Utc>,
}

/// Security issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIssue {
    /// Issue type
    pub issue_type: SecurityIssueType,
    /// Issue severity
    pub severity: SecuritySeverity,
    /// Issue description
    pub description: String,
    /// Source location
    pub location: SourceLocation,
    /// Recommendation
    pub recommendation: Option<String>,
}

/// Security issue type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecurityIssueType {
    /// File system access
    FileSystemAccess,
    /// Network access
    NetworkAccess,
    /// System call access
    SystemCallAccess,
    /// Code injection risk
    CodeInjection,
    /// Information disclosure
    InformationDisclosure,
    /// Resource exhaustion
    ResourceExhaustion,
    /// Privilege escalation
    PrivilegeEscalation,
    /// Denial of service
    DenialOfService,
}

/// Security severity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecuritySeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Cache enabled
    pub enabled: bool,
    /// Cache size limit in bytes
    pub size_limit: Option<u64>,
    /// Time to live for cached items
    pub ttl: Option<Duration>,
    /// Cache eviction policy
    pub eviction_policy: CacheEvictionPolicy,
    /// Compression enabled
    pub compression: Option<bool>,
}

/// Cache eviction policy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CacheEvictionPolicy {
    /// Least recently used
    LRU,
    /// Least frequently used
    LFU,
    /// First in, first out
    FIFO,
    /// Random eviction
    Random,
}

/// Script execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptExecutionStats {
    /// Total executions
    pub total_executions: u64,
    /// Successful executions
    pub successful_executions: u64,
    /// Failed executions
    pub failed_executions: u64,
    /// Average execution time
    pub average_execution_time: Duration,
    /// Total memory used
    pub total_memory_used: u64,
    /// Executions by script
    pub executions_by_script: HashMap<String, u64>,
    /// Error rates by script
    pub error_rates_by_script: HashMap<String, f32>,
    /// Popular scripts
    pub popular_scripts: Vec<(String, u64)>,
}

/// ============================================================================
/// DATA STORE TYPES
/// ============================================================================

/// Database schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSchema {
    /// Schema name
    pub name: String,
    /// Schema version
    pub version: String,
    /// Field definitions
    pub fields: Vec<FieldDefinition>,
    /// Indexes
    pub indexes: Vec<IndexDefinition>,
    /// Constraints
    pub constraints: Vec<Constraint>,
    /// Schema metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Field definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    /// Field name
    pub name: String,
    /// Field type
    pub field_type: FieldType,
    /// Required flag
    pub required: bool,
    /// Default value
    pub default_value: Option<serde_json::Value>,
    /// Validation rules
    pub validation: Option<ValidationRules>,
    /// Field description
    pub description: Option<String>,
}

/// Field type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FieldType {
    /// String type
    String,
    /// Integer type
    Integer,
    /// Float type
    Float,
    /// Boolean type
    Boolean,
    /// Array type
    Array(Box<FieldType>),
    /// Object type
    Object(HashMap<String, FieldType>),
    /// Date/time type
    DateTime,
    /// Binary data type
    Binary,
    /// Text type (long string)
    Text,
    /// UUID type
    Uuid,
    /// JSON type
    Json,
}

/// Constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    /// Constraint name
    pub name: String,
    /// Constraint type
    pub constraint_type: ConstraintType,
    /// Constraint fields
    pub fields: Vec<String>,
    /// Constraint parameters
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Constraint type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConstraintType {
    /// Unique constraint
    Unique,
    /// Primary key constraint
    PrimaryKey,
    /// Foreign key constraint
    ForeignKey,
    /// Check constraint
    Check,
    /// Not null constraint
    NotNull,
}

/// Document data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentData {
    /// Document ID
    pub id: DocumentId,
    /// Document content
    pub content: serde_json::Value,
    /// Document metadata
    pub metadata: DocumentMetadata,
    /// Document version
    pub version: u32,
    /// Created at timestamp
    pub created_at: DateTime<Utc>,
    /// Updated at timestamp
    pub updated_at: DateTime<Utc>,
}

/// Document ID
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentId(pub String);

/// Document metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Document type
    pub document_type: Option<String>,
    /// Tags
    pub tags: Vec<String>,
    /// Author
    pub author: Option<String>,
    /// Content hash
    pub content_hash: Option<String>,
    /// Size in bytes
    pub size_bytes: u64,
    /// Custom metadata
    pub custom: HashMap<String, serde_json::Value>,
}

/// Query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    /// Query type
    pub query_type: QueryType,
    /// Query filter
    pub filter: Option<QueryFilter>,
    /// Projection (fields to return)
    pub projection: Option<Vec<String>>,
    /// Sort order
    pub sort: Option<Vec<SortOrder>>,
    /// Limit
    pub limit: Option<u32>,
    /// Offset
    pub offset: Option<u32>,
    /// Aggregation pipeline
    pub aggregation: Option<AggregationPipeline>,
}

/// Query type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QueryType {
    /// Find documents
    Find,
    /// Count documents
    Count,
    /// Distinct values
    Distinct,
    /// Aggregation
    Aggregate,
}

/// Query filter
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QueryFilter {
    /// Simple field filter
    Field {
        /// Field name
        field: String,
        /// Operator
        operator: FilterOperator,
        /// Value
        value: serde_json::Value,
    },
    /// Compound filter
    Compound {
        /// Logical operator
        operator: LogicalOperator,
        /// Sub-filters
        filters: Vec<QueryFilter>,
    },
    /// Raw JSON filter
    Raw(serde_json::Value),
}

/// Filter operator
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FilterOperator {
    /// Equals
    Eq,
    /// Not equals
    Ne,
    /// Greater than
    Gt,
    /// Greater than or equal
    Gte,
    /// Less than
    Lt,
    /// Less than or equal
    Lte,
    /// In list
    In,
    /// Not in list
    Nin,
    /// Regex match
    Regex,
    /// Exists
    Exists,
    /// Array contains
    Contains,
    /// Array size
    Size,
}

/// Logical operator
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LogicalOperator {
    /// Logical AND
    And,
    /// Logical OR
    Or,
    /// Logical NOT
    Not,
}

/// Sort order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortOrder {
    /// Field name
    pub field: String,
    /// Sort direction
    pub direction: SortDirection,
}

/// Sort direction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SortDirection {
    /// Ascending
    Ascending,
    /// Descending
    Descending,
}

/// Query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Resulting documents
    pub documents: Vec<DocumentData>,
    /// Total count (if available)
    pub total_count: Option<u64>,
    /// Query execution time
    pub execution_time: Duration,
    /// Query metadata
    pub metadata: QueryMetadata,
}

/// Query metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMetadata {
    /// Query plan (if available)
    pub query_plan: Option<serde_json::Value>,
    /// Index used
    pub index_used: Option<String>,
    /// Documents scanned
    pub documents_scanned: u64,
    /// Results returned
    pub results_returned: u64,
}

/// Aggregation pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationPipeline {
    /// Pipeline stages
    pub stages: Vec<AggregationStage>,
}

/// Aggregation stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationStage {
    /// Stage type
    pub stage_type: String,
    /// Stage specification
    pub specification: serde_json::Value,
}

/// Aggregation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationResult {
    /// Result documents
    pub results: Vec<serde_json::Value>,
    /// Execution time
    pub execution_time: Duration,
    /// Aggregation metadata
    pub metadata: AggregationMetadata,
}

/// Aggregation metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationMetadata {
    /// Number of stages processed
    pub stages_processed: u32,
    /// Documents processed per stage
    pub documents_per_stage: Vec<u64>,
    /// Memory usage
    pub memory_usage: u64,
}

/// Search query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Search text
    pub query: String,
    /// Fields to search
    pub fields: Option<Vec<String>>,
    /// Search type
    pub search_type: SearchType,
    /// Fuzzy search threshold
    pub fuzzy_threshold: Option<f32>,
    /// Boost fields
    pub boost_fields: Option<HashMap<String, f32>>,
    /// Filters to apply
    pub filters: Option<Vec<QueryFilter>>,
}

/// Search type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SearchType {
    /// Full-text search
    FullText,
    /// Fuzzy search
    Fuzzy,
    /// Phrase search
    Phrase,
    /// Prefix search
    Prefix,
    /// Wildcard search
    Wildcard,
}

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Matching documents
    pub documents: Vec<SearchResultDocument>,
    /// Total matches
    pub total_matches: u64,
    /// Search execution time
    pub execution_time: Duration,
    /// Search metadata
    pub metadata: SearchMetadata,
}

/// Search result document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultDocument {
    /// Document data
    pub document: DocumentData,
    /// Search score
    pub score: f32,
    /// Highlight snippets
    pub highlights: Vec<String>,
}

/// Search metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMetadata {
    /// Query used
    pub query: String,
    /// Search type
    pub search_type: SearchType,
    /// Fields searched
    pub fields_searched: Vec<String>,
    /// Documents scanned
    pub documents_scanned: u64,
}

/// Vector search options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchOptions {
    /// Number of results to return
    pub top_k: u32,
    /// Distance metric
    pub distance_metric: DistanceMetric,
    /// Include vector data in results
    pub include_vectors: bool,
    /// Filter to apply
    pub filter: Option<QueryFilter>,
    /// Index to use (if multiple)
    pub index_name: Option<String>,
}

/// Distance metric
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DistanceMetric {
    /// Cosine similarity
    Cosine,
    /// Euclidean distance
    Euclidean,
    /// Manhattan distance
    Manhattan,
    /// Dot product
    DotProduct,
}

/// Vector search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    /// Matching documents with distances
    pub results: Vec<VectorSearchDocument>,
    /// Search execution time
    pub execution_time: Duration,
    /// Search metadata
    pub metadata: VectorSearchMetadata,
}

/// Vector search document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchDocument {
    /// Document data
    pub document: DocumentData,
    /// Distance/score
    pub distance: f32,
    /// Document vector (if included)
    pub vector: Option<Vec<f32>>,
}

/// Vector search metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchMetadata {
    /// Vector dimension
    pub vector_dimension: u32,
    /// Distance metric used
    pub distance_metric: DistanceMetric,
    /// Documents scanned
    pub documents_scanned: u64,
    /// Index used
    pub index_used: Option<String>,
}

/// Bulk insert result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkInsertResult {
    /// Number of successfully inserted documents
    pub inserted_count: u32,
    /// Number of failed insertions
    pub failed_count: u32,
    /// Inserted document IDs
    pub inserted_ids: Vec<DocumentId>,
    /// Errors encountered
    pub errors: Vec<BulkOperationError>,
    /// Execution time
    pub execution_time: Duration,
}

/// Update operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOperation {
    /// Document ID
    pub id: DocumentId,
    /// Update operations
    pub updates: Vec<DocumentUpdate>,
}

/// Document update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentUpdate {
    /// Update operation type
    pub operation: UpdateOperationType,
    /// Field path
    pub field: String,
    /// Value to set
    pub value: Option<serde_json::Value>,
}

/// Update operation type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdateOperationType {
    /// Set field value
    Set,
    /// Unset field
    Unset,
    /// Increment numeric field
    Increment,
    /// Push to array
    Push,
    /// Pull from array
    Pull,
    /// Add to set
    AddToSet,
}

/// Bulk update result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkUpdateResult {
    /// Number of successfully updated documents
    pub updated_count: u32,
    /// Number of failed updates
    pub failed_count: u32,
    /// Updated document IDs
    pub updated_ids: Vec<DocumentId>,
    /// Errors encountered
    pub errors: Vec<BulkOperationError>,
    /// Execution time
    pub execution_time: Duration,
}

/// Bulk delete result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkDeleteResult {
    /// Number of successfully deleted documents
    pub deleted_count: u32,
    /// Number of failed deletions
    pub failed_count: u32,
    /// Deleted document IDs
    pub deleted_ids: Vec<DocumentId>,
    /// Errors encountered
    pub errors: Vec<BulkOperationError>,
    /// Execution time
    pub execution_time: Duration,
}

/// Bulk operation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkOperationError {
    /// Document index
    pub index: u32,
    /// Document ID
    pub document_id: DocumentId,
    /// Error message
    pub error: String,
    /// Error code
    pub error_code: String,
}

/// Transaction ID
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransactionId(pub String);

/// Index definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDefinition {
    /// Index name
    pub name: String,
    /// Indexed fields
    pub fields: Vec<IndexField>,
    /// Index type
    pub index_type: IndexType,
    /// Index options
    pub options: IndexOptions,
    /// Unique flag
    pub unique: bool,
    /// Sparse flag
    pub sparse: bool,
}

/// Index field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexField {
    /// Field name
    pub field: String,
    /// Sort direction
    pub direction: SortDirection,
    /// Field type (for text indexes)
    pub field_type: Option<FieldType>,
}

/// Index type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum IndexType {
    /// B-tree index
    BTree,
    /// Hash index
    Hash,
    /// Full-text search index
    FullText,
    /// Vector index
    Vector,
    /// Geospatial index
    Geospatial,
    /// Compound index
    Compound,
}

/// Index options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexOptions {
    /// Index language (for text indexes)
    pub language: Option<String>,
    /// Vector dimension (for vector indexes)
    pub vector_dimension: Option<u32>,
    /// Distance metric (for vector indexes)
    pub distance_metric: Option<DistanceMetric>,
    /// Custom options
    pub custom: HashMap<String, serde_json::Value>,
}

/// Index info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexInfo {
    /// Index name
    pub name: String,
    /// Index fields
    pub fields: Vec<IndexField>,
    /// Index type
    pub index_type: IndexType,
    /// Index size in bytes
    pub size_bytes: u64,
    /// Number of indexed documents
    pub document_count: u64,
    /// Unique flag
    pub unique: bool,
    /// Sparse flag
    pub sparse: bool,
    /// Created at timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

/// Index statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    /// Index name
    pub name: String,
    /// Index size
    pub size_bytes: u64,
    /// Number of entries
    pub entry_count: u64,
    /// Index usage statistics
    pub usage_stats: IndexUsageStats,
    /// Last accessed timestamp
    pub last_accessed: Option<DateTime<Utc>>,
}

/// Index usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexUsageStats {
    /// Number of times used
    pub usage_count: u64,
    /// Average query time
    pub average_query_time: Duration,
    /// Selectivity ratio
    pub selectivity: f32,
}

/// Backup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    /// Backup name
    pub name: Option<String>,
    /// Include collections
    pub include_collections: Option<Vec<String>>,
    /// Exclude collections
    pub exclude_collections: Option<Vec<String>>,
    /// Compression enabled
    pub compression: bool,
    /// Encryption enabled
    pub encryption: bool,
    /// Retention period
    pub retention_period: Option<Duration>,
}

/// Backup info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    /// Backup ID
    pub backup_id: String,
    /// Backup name
    pub name: String,
    /// Backup size in bytes
    pub size_bytes: u64,
    /// Number of documents
    pub document_count: u64,
    /// Created at timestamp
    pub created_at: DateTime<Utc>,
    /// Completed at timestamp
    pub completed_at: Option<DateTime<Utc>>,
    /// Backup status
    pub status: BackupStatus,
    /// Included collections
    pub collections: Vec<String>,
    /// Backup metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Backup status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackupStatus {
    /// Backup is in progress
    InProgress,
    /// Backup completed successfully
    Completed,
    /// Backup failed
    Failed(String),
    /// Backup was cancelled
    Cancelled,
}

/// Restore configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreConfig {
    /// Target database name
    pub target_database: Option<String>,
    /// Include collections
    pub include_collections: Option<Vec<String>>,
    /// Exclude collections
    pub exclude_collections: Option<Vec<String>>,
    /// Drop existing collections
    pub drop_existing: bool,
    /// Preserve indexes
    pub preserve_indexes: bool,
}

/// Restore result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    /// Backup ID
    pub backup_id: String,
    /// Number of restored documents
    pub restored_documents: u64,
    /// Number of restored collections
    pub restored_collections: u32,
    /// Restore duration
    pub duration: Duration,
    /// Restore status
    pub status: RestoreStatus,
    /// Errors encountered
    pub errors: Vec<String>,
}

/// Restore status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RestoreStatus {
    /// Restore completed successfully
    Completed,
    /// Restore failed
    Failed(String),
    /// Restore was cancelled
    Cancelled,
    /// Restore completed with warnings
    CompletedWithWarnings(Vec<String>),
}

/// Replication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    /// Replication mode
    pub mode: ReplicationMode,
    /// Primary node(s)
    pub primary_nodes: Vec<String>,
    /// Secondary nodes
    pub secondary_nodes: Vec<String>,
    /// Replication lag threshold
    pub lag_threshold: Option<Duration>,
    /// Automatic failover
    pub automatic_failover: bool,
    /// Consistency level
    pub consistency_level: ConsistencyLevel,
}

/// Replication mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReplicationMode {
    /// Primary-secondary replication
    PrimarySecondary,
    /// Multi-primary replication
    MultiPrimary,
    /// Chain replication
    Chain,
}

/// Consistency level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConsistencyLevel {
    /// Strong consistency
    Strong,
    /// Eventual consistency
    Eventual,
    /// Quorum consistency
    Quorum,
}

/// Replication status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationStatus {
    /// Replication mode
    pub mode: ReplicationMode,
    /// Node status
    pub nodes: Vec<NodeStatus>,
    /// Current primary
    pub current_primary: Option<String>,
    /// Replication lag
    pub replication_lag: Duration,
    /// Last sync timestamp
    pub last_sync: DateTime<Utc>,
    /// Overall status
    pub status: ReplicationOverallStatus,
}

/// Node status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    /// Node ID
    pub node_id: String,
    /// Node role
    pub role: NodeRole,
    /// Node status
    pub status: NodeStatusType,
    /// Last heartbeat
    pub last_heartbeat: DateTime<Utc>,
    /// Replication lag
    pub replication_lag: Option<Duration>,
    /// Node metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Node role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeRole {
    /// Primary node
    Primary,
    /// Secondary node
    Secondary,
    /// Arbiter node
    Arbiter,
}

/// Node status type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeStatusType {
    /// Node is online
    Online,
    /// Node is offline
    Offline,
    /// Node is syncing
    Syncing,
    /// Node has errors
    Error(String),
}

/// Replication overall status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReplicationOverallStatus {
    /// Replication is healthy
    Healthy,
    /// Replication has issues
    Degraded,
    /// Replication is down
    Down,
    /// Replication has errors
    Error(String),
}

/// Sync configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Remote endpoint
    pub remote_endpoint: String,
    /// Authentication credentials
    pub credentials: Option<SyncCredentials>,
    /// Sync direction
    pub direction: SyncDirection,
    /// Conflict resolution strategy
    pub conflict_resolution: ConflictResolution,
    /// Sync interval
    pub interval: Option<Duration>,
    /// Batch size
    pub batch_size: Option<u32>,
}

/// Sync credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCredentials {
    /// Authentication type
    pub auth_type: AuthType,
    /// Username
    pub username: Option<String>,
    /// Password or token
    pub token: Option<String>,
    /// API key
    pub api_key: Option<String>,
    /// Custom credentials
    pub custom: HashMap<String, String>,
}

/// Auth type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthType {
    /// Basic authentication
    Basic,
    /// Bearer token
    Bearer,
    /// API key
    ApiKey,
    /// Custom authentication
    Custom(String),
}

/// Sync direction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyncDirection {
    /// Upload to remote
    Upload,
    /// Download from remote
    Download,
    /// Bidirectional sync
    Bidirectional,
}

/// Conflict resolution strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Local wins
    LocalWins,
    /// Remote wins
    RemoteWins,
    /// Manual resolution
    Manual,
    /// Timestamp-based resolution
    Timestamp,
    /// Merge changes
    Merge,
}

/// Sync result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    /// Sync operation ID
    pub sync_id: String,
    /// Number of documents uploaded
    pub uploaded_count: u32,
    /// Number of documents downloaded
    pub downloaded_count: u32,
    /// Number of conflicts
    pub conflict_count: u32,
    /// Number of errors
    pub error_count: u32,
    /// Sync duration
    pub duration: Duration,
    /// Sync status
    pub status: SyncStatus,
    /// Last sync timestamp
    pub last_sync: DateTime<Utc>,
}

/// Sync status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyncStatus {
    /// Sync completed successfully
    Completed,
    /// Sync failed
    Failed(String),
    /// Sync was cancelled
    Cancelled,
    /// Sync completed with conflicts
    CompletedWithConflicts,
    /// Sync is in progress
    InProgress,
}

/// Schema info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaInfo {
    /// Schema name
    pub name: String,
    /// Schema version
    pub version: String,
    /// Created at timestamp
    pub created_at: DateTime<Utc>,
    /// Updated at timestamp
    pub updated_at: DateTime<Utc>,
    /// Schema status
    pub status: SchemaStatus,
    /// Number of documents using this schema
    pub document_count: u64,
}

/// Schema status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SchemaStatus {
    /// Schema is active
    Active,
    /// Schema is deprecated
    Deprecated,
    /// Schema is in draft
    Draft,
    /// Schema has errors
    Error(String),
}

/// Validation result (redefined for data store)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Validation passed
    pub valid: bool,
    /// Validation errors
    pub errors: Vec<ValidationError>,
    /// Validation warnings
    pub warnings: Vec<ValidationWarning>,
    /// Validation metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// Field path
    pub field: String,
    /// Error message
    pub message: String,
    /// Error code
    pub code: String,
    /// Error severity
    pub severity: ErrorSeverity,
}

/// Validation warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    /// Field path
    pub field: String,
    /// Warning message
    pub message: String,
    /// Warning code
    pub code: String,
}