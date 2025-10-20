use async_trait::async_trait;
use crate::errors::{ServiceError, ServiceResult};
use crate::types::{ServiceInfo, ServiceHealth, ServiceMetrics, ServiceDependency};
use crate::types::tool::*;
use std::collections::HashMap;
use uuid::Uuid;
use super::BaseService;

/// Trait for tool execution and management services
#[async_trait]
pub trait ToolService: BaseService + Send + Sync {
    /// Register a new tool
    async fn register_tool(&self, tool: ToolDefinition) -> ServiceResult<String>;

    /// Unregister a tool
    async fn unregister_tool(&self, tool_name: &str) -> ServiceResult<bool>;

    /// Get tool definition
    async fn get_tool(&self, tool_name: &str) -> ServiceResult<Option<ToolDefinition>>;

    /// List all available tools
    async fn list_tools(&self) -> ServiceResult<Vec<ToolDefinition>>;

    /// List tools by category
    async fn list_tools_by_category(&self, category: &str) -> ServiceResult<Vec<ToolDefinition>>;

    /// Find tools by tag
    async fn find_tools_by_tag(&self, tag: &str) -> ServiceResult<Vec<ToolDefinition>>;

    /// Search tools by name or description
    async fn search_tools(&self, query: &str) -> ServiceResult<Vec<ToolDefinition>>;

    /// Execute a tool
    async fn execute_tool(&self, request: ToolExecutionRequest) -> ServiceResult<ToolExecutionResult>;

    /// Execute a tool asynchronously
    async fn execute_tool_async(&self, request: ToolExecutionRequest) -> ServiceResult<String>;

    /// Get execution result for async tool execution
    async fn get_execution_result(&self, execution_id: &str) -> ServiceResult<Option<ToolExecutionResult>>;

    /// Cancel an ongoing tool execution
    async fn cancel_execution(&self, execution_id: &str) -> ServiceResult<bool>;

    /// List active executions
    async fn list_active_executions(&self) -> ServiceResult<Vec<ActiveExecution>>;

    /// Get tool execution history
    async fn get_execution_history(&self, tool_name: &str, limit: Option<u32>) -> ServiceResult<Vec<ToolExecutionResult>>;

    /// Validate tool parameters against schema
    async fn validate_tool_parameters(&self, tool_name: &str, parameters: &serde_json::Value) -> ServiceResult<ValidationResult>;

    /// Get tool usage statistics
    async fn get_tool_stats(&self, tool_name: &str) -> ServiceResult<ToolUsageStats>;

    /// Get overall tool service statistics
    async fn get_service_stats(&self) -> ServiceResult<ToolServiceStats>;

    /// Update tool definition
    async fn update_tool(&self, tool_name: &str, tool: ToolDefinition) -> ServiceResult<bool>;

    /// Enable/disable a tool
    async fn set_tool_enabled(&self, tool_name: &str, enabled: bool) -> ServiceResult<bool>;

    /// Check if a tool is enabled
    async fn is_tool_enabled(&self, tool_name: &str) -> ServiceResult<bool>;

    /// Get tool permissions
    async fn get_tool_permissions(&self, tool_name: &str) -> ServiceResult<Vec<String>>;

    /// Set tool permissions
    async fn set_tool_permissions(&self, tool_name: &str, permissions: Vec<String>) -> ServiceResult<()>;

    /// Check if user has permission to execute tool
    async fn check_tool_permission(&self, tool_name: &str, user_id: &str) -> ServiceResult<bool>;

    /// Get tool dependencies
    async fn get_tool_dependencies(&self, tool_name: &str) -> ServiceResult<Vec<ToolDependency>>;

    /// Install tool dependencies
    async fn install_tool_dependencies(&self, tool_name: &str) -> ServiceResult<()>;

    /// Verify tool installation
    async fn verify_tool(&self, tool_name: &str) -> ServiceResult<ToolVerificationResult>;
}

/// Active tool execution information
#[derive(Debug, Clone)]
pub struct ActiveExecution {
    /// Execution ID
    pub execution_id: String,
    /// Tool name
    pub tool_name: String,
    /// Execution context
    pub context: ToolExecutionContext,
    /// Start timestamp
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Current status
    pub status: ExecutionStatus,
    /// Progress percentage (0-100)
    pub progress: Option<f32>,
    /// Estimated remaining time in seconds
    pub estimated_remaining_seconds: Option<u64>,
    /// Current output/progress information
    pub current_output: Option<String>,
}

/// Tool execution status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    /// Execution is queued
    Queued,
    /// Execution is running
    Running,
    /// Execution completed successfully
    Completed,
    /// Execution failed
    Failed,
    /// Execution was cancelled
    Cancelled,
    /// Execution timed out
    TimedOut,
}

/// Tool parameter validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed
    pub valid: bool,
    /// Validation errors (if any)
    pub errors: Vec<ValidationError>,
    /// Normalized parameters (if valid)
    pub normalized_parameters: Option<serde_json::Value>,
}

/// Validation error details
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Error path (JSON pointer)
    pub path: String,
    /// Error message
    pub message: String,
    /// Error code
    pub code: String,
    /// Invalid value
    pub value: serde_json::Value,
}

/// Tool usage statistics
#[derive(Debug, Clone)]
pub struct ToolUsageStats {
    /// Tool name
    pub tool_name: String,
    /// Total executions
    pub total_executions: u64,
    /// Successful executions
    pub successful_executions: u64,
    /// Failed executions
    pub failed_executions: u64,
    /// Average execution time in milliseconds
    pub avg_execution_time_ms: f64,
    /// Last execution timestamp
    pub last_execution: Option<chrono::DateTime<chrono::Utc>>,
    /// Most frequent users
    pub top_users: Vec<UserUsageStats>,
    /// Usage by time period
    pub usage_by_period: HashMap<String, u64>,
}

/// User-specific usage statistics
#[derive(Debug, Clone)]
pub struct UserUsageStats {
    /// User identifier
    pub user_id: String,
    /// Execution count
    pub execution_count: u64,
    /// Last execution timestamp
    pub last_execution: chrono::DateTime<chrono::Utc>,
}

/// Tool service statistics
#[derive(Debug, Clone)]
pub struct ToolServiceStats {
    /// Total registered tools
    pub total_tools: usize,
    /// Enabled tools
    pub enabled_tools: usize,
    /// Total executions
    pub total_executions: u64,
    /// Active executions
    pub active_executions: u64,
    /// Average execution time across all tools
    pub avg_execution_time_ms: f64,
    /// Most used tools
    pub top_tools: Vec<String>,
    /// Service uptime in seconds
    pub uptime_seconds: u64,
}

/// Tool dependency information
#[derive(Debug, Clone)]
pub struct ToolDependency {
    /// Dependency name
    pub name: String,
    /// Dependency version requirement
    pub version_requirement: Option<String>,
    /// Dependency type
    pub dependency_type: ToolDependencyType,
    /// Whether this dependency is required
    pub required: bool,
    /// Installation status
    pub status: DependencyStatus,
}

/// Types of tool dependencies
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolDependencyType {
    /// System package
    SystemPackage,
    /// Python package
    PythonPackage,
    /// Node.js package
    NodePackage,
    /// Rust crate
    RustCrate,
    /// Docker image
    DockerImage,
    /// Executable binary
    Binary,
    /// Custom dependency
    Custom(String),
}

/// Dependency installation status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyStatus {
    /// Dependency is installed and available
    Installed,
    /// Dependency is missing
    Missing,
    /// Dependency version doesn't match requirements
    VersionMismatch,
    /// Dependency installation failed
    Failed,
    /// Installation in progress
    Installing,
}

/// Tool verification result
#[derive(Debug, Clone)]
pub struct ToolVerificationResult {
    /// Whether tool verification passed
    pub verified: bool,
    /// Verification timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Verification errors (if any)
    pub errors: Vec<VerificationError>,
    /// Tool health information
    pub health: ToolHealth,
    /// Performance metrics
    pub performance: Option<ToolPerformanceMetrics>,
}

/// Verification error details
#[derive(Debug, Clone)]
pub struct VerificationError {
    /// Error type
    pub error_type: VerificationErrorType,
    /// Error message
    pub message: String,
    /// Error context
    pub context: HashMap<String, String>,
}

/// Types of verification errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationErrorType {
    /// Tool binary not found
    BinaryNotFound,
    /// Tool binary not executable
    BinaryNotExecutable,
    /// Tool version mismatch
    VersionMismatch,
    /// Tool failed health check
    HealthCheckFailed,
    /// Tool dependencies missing
    DependenciesMissing,
    /// Tool configuration invalid
    InvalidConfiguration,
    /// Tool permissions insufficient
    InsufficientPermissions,
    /// Tool timeout
    Timeout,
}

/// Tool health information
#[derive(Debug, Clone)]
pub struct ToolHealth {
    /// Overall health status
    pub status: ToolHealthStatus,
    /// Last health check timestamp
    pub last_check: chrono::DateTime<chrono::Utc>,
    /// Response time in milliseconds
    pub response_time_ms: Option<u64>,
    /// Health check messages
    pub messages: Vec<String>,
}

/// Tool health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolHealthStatus {
    /// Tool is healthy
    Healthy,
    /// Tool is degraded but functional
    Degraded,
    /// Tool is unhealthy
    Unhealthy,
    /// Tool health unknown
    Unknown,
}

/// Tool performance metrics
#[derive(Debug, Clone)]
pub struct ToolPerformanceMetrics {
    /// Average execution time in milliseconds
    pub avg_execution_time_ms: f64,
    /// Minimum execution time in milliseconds
    pub min_execution_time_ms: u64,
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: u64,
    /// 95th percentile execution time in milliseconds
    pub p95_execution_time_ms: u64,
    /// Throughput (executions per second)
    pub throughput_rps: f64,
    /// Memory usage in bytes
    pub memory_usage_bytes: Option<u64>,
    /// CPU usage percentage
    pub cpu_usage_percent: Option<f64>,
}

/// Trait for tool discovery and registration
#[async_trait]
pub trait ToolRegistry: Send + Sync {
    /// Discover available tools
    async fn discover_tools(&self) -> ServiceResult<Vec<ToolDefinition>>;

    /// Register multiple tools
    async fn register_tools(&self, tools: Vec<ToolDefinition>) -> ServiceResult<Vec<String>>;

    /// Unregister multiple tools
    async fn unregister_tools(&self, tool_names: Vec<String>) -> ServiceResult<Vec<bool>>;

    /// Sync tools with external registry
    async fn sync_tools(&self, source: &str) -> ServiceResult<SyncResult>;

    /// Validate tool registry integrity
    async fn validate_registry(&self) -> ServiceResult<RegistryValidationResult>;
}

/// Tool synchronization result
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// Source of synchronization
    pub source: String,
    /// Timestamp of synchronization
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Tools added
    pub tools_added: Vec<String>,
    /// Tools updated
    pub tools_updated: Vec<String>,
    /// Tools removed
    pub tools_removed: Vec<String>,
    /// Synchronization errors
    pub errors: Vec<String>,
    /// Whether synchronization was successful
    pub success: bool,
}

/// Registry validation result
#[derive(Debug, Clone)]
pub struct RegistryValidationResult {
    /// Whether registry is valid
    pub valid: bool,
    /// Total tools in registry
    pub total_tools: usize,
    /// Valid tools
    pub valid_tools: usize,
    /// Invalid tools
    pub invalid_tools: usize,
    /// Validation errors
    pub errors: Vec<RegistryValidationError>,
    /// Validation timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Registry validation error
#[derive(Debug, Clone)]
pub struct RegistryValidationError {
    /// Tool name
    pub tool_name: String,
    /// Error type
    pub error_type: RegistryErrorType,
    /// Error message
    pub message: String,
}

/// Types of registry validation errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryErrorType {
    /// Duplicate tool registration
    DuplicateRegistration,
    /// Invalid tool definition
    InvalidDefinition,
    /// Missing required fields
    MissingRequiredFields,
    /// Invalid schema
    InvalidSchema,
    /// Permission conflict
    PermissionConflict,
    /// Dependency conflict
    DependencyConflict,
}