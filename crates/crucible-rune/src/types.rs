//! Shared types for the Crucible Rune system
//!
//! This module contains common types used across the Rune system.

use chrono::{DateTime, Utc};
use crucible_services::types::tool::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// System information for the Rune system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// Library version
    pub version: String,
    /// Rune version
    pub rune_version: &'static str,
    /// Supported file extensions
    pub supported_extensions: Vec<String>,
    /// Default tool directories
    pub default_directories: Vec<String>,
}

/// Service configuration for RuneService
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuneServiceConfig {
    /// Service name
    pub service_name: String,
    /// Service version
    pub version: String,
    /// Tool discovery configuration
    pub discovery: DiscoveryServiceConfig,
    /// Hot reload configuration
    pub hot_reload: HotReloadConfig,
    /// Execution configuration
    pub execution: ExecutionConfig,
    /// Cache configuration
    pub cache: CacheConfig,
    /// Security configuration
    pub security: SecurityConfig,
}

impl Default for RuneServiceConfig {
    fn default() -> Self {
        Self {
            service_name: "crucible-rune".to_string(),
            version: "1.0.0".to_string(),
            discovery: DiscoveryServiceConfig::default(),
            hot_reload: HotReloadConfig::default(),
            execution: ExecutionConfig::default(),
            cache: CacheConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

/// Discovery service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryServiceConfig {
    /// Directories to scan for tools
    pub tool_directories: Vec<String>,
    /// Discovery patterns
    pub patterns: DiscoveryPatterns,
    /// Whether to enable recursive discovery
    pub recursive: bool,
    /// Discovery interval in seconds
    pub discovery_interval_seconds: u64,
    /// Maximum file size to process
    pub max_file_size_bytes: usize,
}

impl Default for DiscoveryServiceConfig {
    fn default() -> Self {
        Self {
            tool_directories: vec![
                "./tools".to_string(),
                "./rune-tools".to_string(),
                "./scripts".to_string(),
            ],
            patterns: DiscoveryPatterns::default(),
            recursive: true,
            discovery_interval_seconds: 30,
            max_file_size_bytes: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// Hot reload configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotReloadConfig {
    /// Whether hot reload is enabled
    pub enabled: bool,
    /// Debounce interval in milliseconds
    pub debounce_ms: u64,
    /// File patterns to watch
    pub watch_patterns: Vec<String>,
    /// Patterns to ignore
    pub ignore_patterns: Vec<String>,
}

impl Default for HotReloadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            debounce_ms: 500,
            watch_patterns: vec!["*.rn".to_string(), "*.rune".to_string()],
            ignore_patterns: vec![
                "*.tmp".to_string(),
                "*.bak".to_string(),
                ".*".to_string(),
            ],
        }
    }
}

/// Execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Default timeout in milliseconds
    pub default_timeout_ms: u64,
    /// Maximum timeout in milliseconds
    pub max_timeout_ms: u64,
    /// Maximum memory usage per execution in bytes
    pub max_memory_bytes: u64,
    /// Whether to capture stdout/stderr
    pub capture_output: bool,
    /// Default environment variables
    pub default_environment: HashMap<String, String>,
    /// Sandbox configuration
    pub sandbox: SandboxConfig,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: 30000, // 30 seconds
            max_timeout_ms: 300000,    // 5 minutes
            max_memory_bytes: 100 * 1024 * 1024, // 100MB
            capture_output: true,
            default_environment: HashMap::new(),
            sandbox: SandboxConfig::default(),
        }
    }
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Maximum number of cached tools
    pub max_cached_tools: usize,
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
    /// Whether to enable compilation cache
    pub enable_compilation_cache: bool,
    /// Maximum size of compilation cache in bytes
    pub max_compilation_cache_bytes: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_cached_tools: 1000,
            cache_ttl_seconds: 3600, // 1 hour
            enable_compilation_cache: true,
            max_compilation_cache_bytes: 100 * 1024 * 1024, // 100MB
        }
    }
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Whether to enable sandbox
    pub enable_sandbox: bool,
    /// Allowed modules
    pub allowed_modules: Vec<String>,
    /// Blocked modules
    pub blocked_modules: Vec<String>,
    /// Maximum recursion depth
    pub max_recursion_depth: usize,
    /// Network access policy
    pub network_policy: NetworkPolicy,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_sandbox: false,
            allowed_modules: vec![
                "math".to_string(),
                "json".to_string(),
                "io".to_string(),
                "http".to_string(),
            ],
            blocked_modules: vec![
                "fs".to_string(),
                "net".to_string(),
                "process".to_string(),
            ],
            max_recursion_depth: 100,
            network_policy: NetworkPolicy::default(),
        }
    }
}

/// Sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Whether sandbox is enabled
    pub enabled: bool,
    /// Working directory restriction
    pub working_directory_restricted: bool,
    /// Allowed working directories
    pub allowed_working_directories: Vec<String>,
    /// Resource limits
    pub resource_limits: ResourceLimits,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            working_directory_restricted: false,
            allowed_working_directories: vec![],
            resource_limits: ResourceLimits::default(),
        }
    }
}

/// Resource limits for sandbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum CPU time in seconds
    pub max_cpu_time_seconds: u64,
    /// Maximum memory in bytes
    pub max_memory_bytes: u64,
    /// Maximum file size in bytes
    pub max_file_size_bytes: u64,
    /// Maximum number of file descriptors
    pub max_file_descriptors: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu_time_seconds: 30,
            max_memory_bytes: 100 * 1024 * 1024, // 100MB
            max_file_size_bytes: 10 * 1024 * 1024, // 10MB
            max_file_descriptors: 10,
        }
    }
}

/// Network access policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// Whether network access is allowed
    pub allow_network: bool,
    /// Allowed domains
    pub allowed_domains: Vec<String>,
    /// Blocked domains
    pub blocked_domains: Vec<String>,
    /// Allowed ports
    pub allowed_ports: Vec<u16>,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            allow_network: false,
            allowed_domains: vec![],
            blocked_domains: vec![],
            allowed_ports: vec![],
        }
    }
}

/// Discovery patterns configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryPatterns {
    /// Enable direct tools
    pub direct_tools: bool,
    /// Enable module tools
    pub module_tools: bool,
    /// Enable semantic naming
    pub semantic_naming: bool,
    /// Enable topic-module-function pattern
    pub topic_module_function: bool,
    /// Custom patterns
    pub custom_patterns: HashMap<String, CustomPattern>,
}

impl Default for DiscoveryPatterns {
    fn default() -> Self {
        Self {
            direct_tools: true,
            module_tools: true,
            semantic_naming: false,
            topic_module_function: false,
            custom_patterns: HashMap::new(),
        }
    }
}

/// Custom discovery pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomPattern {
    /// Pattern name
    pub name: String,
    /// Regex pattern
    pub regex: String,
    /// Extraction groups
    pub groups: Vec<String>,
    /// Name template
    pub name_template: String,
}

/// Tool loading result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolLoadingResult {
    /// Loading status
    pub status: LoadingStatus,
    /// Tool information
    pub tool: Option<ToolDefinition>,
    /// Loading duration in milliseconds
    pub duration_ms: u64,
    /// Error message (if any)
    pub error: Option<String>,
    /// Warnings
    pub warnings: Vec<String>,
}

/// Loading status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadingStatus {
    /// Tool loaded successfully
    Success,
    /// Tool loaded with warnings
    Warning,
    /// Tool loading failed
    Error,
    /// Tool was skipped
    Skipped,
}

/// Validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether validation passed
    pub valid: bool,
    /// Validation errors
    pub errors: Vec<String>,
    /// Validation warnings
    pub warnings: Vec<String>,
    /// Validation metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Hot reload event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotReloadEvent {
    /// Event ID
    pub id: String,
    /// Event type
    pub event_type: HotReloadEventType,
    /// File path
    pub file_path: String,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Event data
    pub data: HashMap<String, serde_json::Value>,
}

/// Hot reload event types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HotReloadEventType {
    /// File was created
    Created,
    /// File was modified
    Modified,
    /// File was deleted
    Deleted,
    /// File was renamed
    Renamed { from: String, to: String },
    /// Error occurred
    Error,
}

/// Async function information from AST analysis
#[derive(Debug, Clone)]
pub struct AsyncFunctionInfo {
    /// Function name
    pub name: String,
    /// Function parameters
    pub parameters: Vec<ParameterInfo>,
    /// Return type
    pub return_type: Option<String>,
    /// Documentation comments
    pub doc_comments: Vec<String>,
    /// Source location
    pub location: SourceLocation,
    /// Whether function is public
    pub is_public: bool,
    /// Function attributes
    pub attributes: Vec<String>,
}

/// Parameter information
#[derive(Debug, Clone)]
pub struct ParameterInfo {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub type_name: String,
    /// Whether parameter is optional
    pub is_optional: bool,
    /// Default value (if any)
    pub default_value: Option<String>,
}

/// Source location
#[derive(Debug, Clone)]
pub struct SourceLocation {
    /// Line number (1-based)
    pub line: usize,
    /// Column number (1-based)
    pub column: usize,
    /// Byte offset
    pub offset: usize,
}

/// Discovered module information
#[derive(Debug, Clone)]
pub struct DiscoveredModule {
    /// Module name
    pub name: String,
    /// Module path
    pub path: Vec<String>,
    /// Functions in this module
    pub functions: Vec<AsyncFunctionInfo>,
    /// Module documentation
    pub documentation: Option<String>,
    /// Source location
    pub location: SourceLocation,
}

/// Type constraint information
#[derive(Debug, Clone)]
pub struct TypeConstraint {
    /// Constraint type
    pub constraint_type: ConstraintType,
    /// Constraint value
    pub value: String,
    /// Constraint description
    pub description: Option<String>,
}

/// Constraint types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintType {
    /// Type equality
    Equals,
    /// Subtype relationship
    Subtype,
    /// Implements trait
    Implements,
    /// Custom constraint
    Custom(String),
}

/// Rune type information
#[derive(Debug, Clone)]
pub struct RuneType {
    /// Type name
    pub name: String,
    /// Type kind
    pub kind: TypeKind,
    /// Type parameters
    pub parameters: Vec<RuneType>,
    /// Type constraints
    pub constraints: Vec<TypeConstraint>,
}

/// Type kinds
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeKind {
    /// Primitive type
    Primitive,
    /// Struct type
    Struct,
    /// Enum type
    Enum,
    /// Function type
    Function,
    /// Tuple type
    Tuple,
    /// Array type
    Array,
    /// Map type
    Map,
    /// Optional type
    Optional,
    /// Custom type
    Custom(String),
}

/// Validation rule
#[derive(Debug, Clone)]
pub struct ValidationRule {
    /// Rule name
    pub name: String,
    /// Rule description
    pub description: String,
    /// Rule function (would be a function pointer in real implementation)
    pub validator: String, // Placeholder for actual validator
    /// Rule severity
    pub severity: ValidationSeverity,
}

/// Validation severity
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationSeverity {
    /// Error - must be fixed
    Error,
    /// Warning - should be fixed
    Warning,
    /// Info - informational only
    Info,
}

/// Analyzer configuration
#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    /// Whether to enable type inference
    pub enable_type_inference: bool,
    /// Whether to enable validation
    pub enable_validation: bool,
    /// Maximum analysis depth
    pub max_depth: usize,
    /// Custom validation rules
    pub validation_rules: Vec<ValidationRule>,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            enable_type_inference: true,
            enable_validation: true,
            max_depth: 10,
            validation_rules: vec![],
        }
    }
}

/// Error recovery strategy
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// Retry the operation
    Retry,
    /// Skip the problematic item
    Skip,
    /// Use fallback value
    Fallback,
    /// Abort the operation
    Abort,
}

/// Recovery attempt information
#[derive(Debug, Clone)]
pub struct RecoveryAttempt {
    /// Attempt number
    pub attempt_number: u32,
    /// Strategy used
    pub strategy: RecoveryStrategy,
    /// Timestamp of attempt
    pub timestamp: DateTime<Utc>,
    /// Whether attempt was successful
    pub success: bool,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Error statistics
#[derive(Debug, Clone)]
pub struct ErrorStats {
    /// Total errors
    pub total_errors: u64,
    /// Errors by type
    pub errors_by_type: HashMap<String, u64>,
    /// Recovery attempts
    pub recovery_attempts: u64,
    /// Successful recoveries
    pub successful_recoveries: u64,
    /// Last error timestamp
    pub last_error: Option<DateTime<Utc>>,
}

/// Service health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealth {
    /// Overall health status
    pub status: ServiceHealthStatus,
    /// Last health check
    pub last_check: DateTime<Utc>,
    /// Health checks performed
    pub checks: HashMap<String, HealthCheckResult>,
    /// Overall health score (0-100)
    pub health_score: u8,
}

/// Service health status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceHealthStatus {
    /// Service is healthy
    Healthy,
    /// Service is degraded but functional
    Degraded,
    /// Service is unhealthy
    Unhealthy,
    /// Service health is unknown
    Unknown,
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Check name
    pub name: String,
    /// Whether check passed
    pub passed: bool,
    /// Check duration in milliseconds
    pub duration_ms: u64,
    /// Check message
    pub message: String,
    /// Additional details
    pub details: HashMap<String, serde_json::Value>,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Total executions
    pub total_executions: u64,
    /// Successful executions
    pub successful_executions: u64,
    /// Failed executions
    pub failed_executions: u64,
    /// Average execution time in milliseconds
    pub avg_execution_time_ms: f64,
    /// Minimum execution time in milliseconds
    pub min_execution_time_ms: u64,
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: u64,
    /// 95th percentile execution time
    pub p95_execution_time_ms: u64,
    /// Throughput (executions per second)
    pub throughput_rps: f64,
    /// Memory usage in bytes
    pub memory_usage_bytes: u64,
    /// CPU usage percentage
    pub cpu_usage_percent: f64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            avg_execution_time_ms: 0.0,
            min_execution_time_ms: u64::MAX,
            max_execution_time_ms: 0,
            p95_execution_time_ms: 0,
            throughput_rps: 0.0,
            memory_usage_bytes: 0,
            cpu_usage_percent: 0.0,
        }
    }
}