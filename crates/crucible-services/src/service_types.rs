//! # Simplified Service Type Definitions
//!
//! This module contains essential type definitions for service traits,
//! focusing on core functionality without over-engineering.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use chrono::{DateTime, Utc};

/// ============================================================================
/// ESSENTIAL SERVICE TYPES
/// ============================================================================

/// Basic resource usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// CPU usage percentage (0.0-100.0)
    pub cpu_percentage: f64,
    /// Timestamp of measurement
    pub measured_at: DateTime<Utc>,
}

/// Basic resource limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory in bytes
    pub max_memory_bytes: Option<u64>,
    /// Maximum CPU percentage
    pub max_cpu_percentage: Option<f64>,
    /// Timeout for operations
    pub operation_timeout: Option<Duration>,
}

/// ============================================================================
/// SCRIPT ENGINE TYPES
/// ============================================================================

/// Script compilation context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationContext {
    /// Script name/identifier
    pub script_name: String,
    /// Compilation options
    pub options: CompilationOptions,
    /// Security context
    pub security_context: SecurityContext,
}

/// Script compilation options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationOptions {
    /// Enable optimizations
    pub optimize: bool,
    /// Debug mode
    pub debug: bool,
    /// Strict mode
    pub strict: bool,
}

impl Default for CompilationOptions {
    fn default() -> Self {
        Self {
            optimize: true,
            debug: false,
            strict: true,
        }
    }
}

/// Security context for script execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityContext {
    /// Allowed permissions
    pub permissions: Vec<String>,
    /// Resource limits
    pub limits: ResourceLimits,
    /// Sandbox enabled
    pub sandbox_enabled: bool,
}

impl Default for SecurityContext {
    fn default() -> Self {
        Self {
            permissions: vec!["read".to_string(), "write".to_string()],
            limits: ResourceLimits {
                max_memory_bytes: Some(100 * 1024 * 1024), // 100MB
                max_cpu_percentage: Some(80.0),
                operation_timeout: Some(Duration::from_secs(30)),
            },
            sandbox_enabled: true,
        }
    }
}

/// Compiled script information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledScript {
    /// Script ID
    pub script_id: String,
    /// Script name
    pub script_name: String,
    /// Compilation timestamp
    pub compiled_at: DateTime<Utc>,
    /// Script hash for integrity
    pub script_hash: String,
    /// Security validation result
    pub security_validated: bool,
}

/// Script execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    /// Execution ID
    pub execution_id: String,
    /// Input parameters
    pub parameters: std::collections::HashMap<String, serde_json::Value>,
    /// Security context
    pub security_context: SecurityContext,
    /// Execution options
    pub options: ExecutionOptions,
}

/// Script execution options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionOptions {
    /// Stream output
    pub stream_output: bool,
    /// Timeout duration
    pub timeout: Option<Duration>,
    /// Capture metrics
    pub capture_metrics: bool,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            stream_output: false,
            timeout: Some(Duration::from_secs(30)),
            capture_metrics: true,
        }
    }
}

/// Script execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Execution ID
    pub execution_id: String,
    /// Success status
    pub success: bool,
    /// Result value
    pub result: Option<serde_json::Value>,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution duration
    pub duration_ms: u64,
    /// Memory used
    pub memory_used_bytes: u64,
    /// Output captured
    pub output: Option<String>,
}

/// Script tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptTool {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Tool parameters schema
    pub parameters: serde_json::Value,
    /// Script content
    pub script_content: String,
    /// Tool category
    pub category: Option<String>,
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
    /// Average execution time (ms)
    pub avg_execution_time_ms: f64,
    /// Total memory used (bytes)
    pub total_memory_used_bytes: u64,
    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
}

/// Script engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptEngineConfig {
    /// Maximum concurrent executions
    pub max_concurrent_executions: u32,
    /// Default security context
    pub default_security_context: SecurityContext,
    /// Cache compiled scripts
    pub enable_cache: bool,
    /// Maximum cache size
    pub max_cache_size: u32,
}

impl Default for ScriptEngineConfig {
    fn default() -> Self {
        Self {
            max_concurrent_executions: 10,
            default_security_context: SecurityContext::default(),
            enable_cache: true,
            max_cache_size: 100,
        }
    }
}

/// ============================================================================
/// VALIDATION TYPES
/// ============================================================================

/// Simplified validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Valid status
    pub valid: bool,
    /// Error message if invalid
    pub error: Option<String>,
    /// Warnings
    pub warnings: Vec<String>,
}

/// Security policy for script execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// Allowed operations
    pub allowed_operations: Vec<String>,
    /// Denied operations
    pub denied_operations: Vec<String>,
    /// Resource limits
    pub resource_limits: ResourceLimits,
    /// Sandbox requirements
    pub sandbox_requirements: Vec<String>,
}

/// Security validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityValidationResult {
    /// Security level
    pub security_level: SecurityLevel,
    /// Valid status
    pub valid: bool,
    /// Security issues found
    pub issues: Vec<SecurityIssue>,
    /// Recommendations
    pub recommendations: Vec<String>,
}

/// Security level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SecurityLevel {
    Safe,
    Restricted,
    Untrusted,
    Dangerous,
}

/// Security issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIssue {
    /// Issue type
    pub issue_type: String,
    /// Severity level
    pub severity: SecurityLevel,
    /// Description
    pub description: String,
    /// Location in code
    pub location: Option<String>,
}