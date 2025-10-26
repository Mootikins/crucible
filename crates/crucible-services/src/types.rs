//! # Essential Type Definitions
//!
//! This module contains core type definitions for the simplified Crucible services.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// ============================================================================
/// BASIC SERVICE TYPES
/// ============================================================================

/// Service health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Service health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealth {
    /// Health status
    pub status: ServiceStatus,
    /// Health message
    pub message: Option<String>,
    /// Last health check timestamp
    pub last_check: DateTime<Utc>,
}

/// Service metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMetrics {
    /// Request count
    pub request_count: u64,
    /// Error count
    pub error_count: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
}

impl Default for ServiceMetrics {
    fn default() -> Self {
        Self {
            request_count: 0,
            error_count: 0,
            avg_response_time_ms: 0.0,
            last_updated: Utc::now(),
        }
    }
}

/// ============================================================================
/// TOOL TYPES
/// ============================================================================

/// Tool execution request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionRequest {
    /// Tool name
    pub tool_name: String,
    /// Tool parameters
    pub parameters: std::collections::HashMap<String, serde_json::Value>,
    /// Request ID
    pub request_id: String,
}

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    /// Request ID
    pub request_id: String,
    /// Success status
    pub success: bool,
    /// Result data
    pub result: Option<serde_json::Value>,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
}

/// Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Tool parameters schema
    pub parameters: serde_json::Value,
}

/// ============================================================================
/// EXECUTION TYPES
/// ============================================================================

/// Execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Execution chunk for streaming results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionChunk {
    /// Chunk ID
    pub chunk_id: String,
    /// Execution ID
    pub execution_id: String,
    /// Chunk data
    pub data: String,
    /// Is this the final chunk
    pub is_final: bool,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Compilation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationError {
    /// Error message
    pub message: String,
    /// Line number if available
    pub line: Option<u32>,
    /// Column number if available
    pub column: Option<u32>,
    /// Error type
    pub error_type: String,
}

/// Compilation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationResult {
    /// Success status
    pub success: bool,
    /// Compiled script ID if successful
    pub script_id: Option<String>,
    /// Compilation errors if any
    pub errors: Vec<CompilationError>,
    /// Compilation duration in milliseconds
    pub duration_ms: u64,
}

// CacheConfig removed - unused dead code. Use crucible_config::CacheConfig if needed.

/// Script information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptInfo {
    /// Script ID
    pub script_id: String,
    /// Script name
    pub script_name: String,
    /// Script description
    pub description: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last modified timestamp
    pub modified_at: DateTime<Utc>,
    /// Script size in bytes
    pub size_bytes: u64,
    /// Security validated
    pub security_validated: bool,
}
