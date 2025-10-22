//! Tool-related type definitions
//!
//! This module contains type definitions for tools, tool execution,
//! and related functionality used across the crucible services.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Definition of a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// JSON schema for tool input
    pub input_schema: Value,
    /// Tool category (optional)
    pub category: Option<String>,
    /// Tool version (optional)
    pub version: Option<String>,
    /// Tool author (optional)
    pub author: Option<String>,
    /// Tool tags
    pub tags: Vec<String>,
    /// Whether the tool is enabled
    pub enabled: bool,
    /// Tool parameters
    pub parameters: Vec<ToolParameter>,
}

/// Tool parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameter {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub param_type: String,
    /// Parameter description
    pub description: String,
    /// Whether parameter is required
    pub required: bool,
    /// Default value (optional)
    pub default_value: Option<Value>,
}

/// Context for tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionContext {
    /// Execution ID
    pub execution_id: String,
    /// Context reference for tracking
    pub context_ref: Option<ContextRef>,
    /// Execution timeout
    pub timeout: Option<Duration>,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// User context
    pub user_context: Option<Value>,
    /// Service context
    pub service_context: Option<Value>,
    /// Timestamp when execution started
    pub started_at: DateTime<Utc>,
}

impl Default for ToolExecutionContext {
    fn default() -> Self {
        Self {
            execution_id: Uuid::new_v4().to_string(),
            context_ref: Some(ContextRef::new()),
            timeout: Some(Duration::from_secs(30)),
            environment: HashMap::new(),
            user_context: None,
            service_context: None,
            started_at: Utc::now(),
        }
    }
}

/// Context reference for tracking tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRef {
    /// Unique context ID
    pub id: String,
    /// Context metadata
    pub metadata: HashMap<String, Value>,
    /// Parent context ID (for nested calls)
    pub parent_id: Option<String>,
    /// Context creation timestamp
    pub created_at: DateTime<Utc>,
}

impl ContextRef {
    /// Create a new context reference
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            metadata: HashMap::new(),
            parent_id: None,
            created_at: Utc::now(),
        }
    }

    /// Create a new context reference with metadata
    pub fn with_metadata(metadata: HashMap<String, Value>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            metadata,
            parent_id: None,
            created_at: Utc::now(),
        }
    }

    /// Create a child context
    pub fn child(&self) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            metadata: HashMap::new(),
            parent_id: Some(self.id.clone()),
            created_at: Utc::now(),
        }
    }

    /// Add metadata to context
    pub fn add_metadata(&mut self, key: String, value: Value) {
        self.metadata.insert(key, value);
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&Value> {
        self.metadata.get(key)
    }
}

/// Request for tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionRequest {
    /// Tool name to execute
    pub tool_name: String,
    /// Tool input parameters
    pub parameters: Value,
    /// Execution context
    pub context: ToolExecutionContext,
    /// Request ID
    pub request_id: String,
}

impl ToolExecutionRequest {
    /// Create a new tool execution request
    pub fn new(
        tool_name: String,
        parameters: Value,
        context: ToolExecutionContext,
    ) -> Self {
        Self {
            tool_name,
            parameters,
            context,
            request_id: Uuid::new_v4().to_string(),
        }
    }
}

/// Result of tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    /// Whether execution was successful
    pub success: bool,
    /// Execution result data
    pub result: Option<Value>,
    /// Error message (if any)
    pub error: Option<String>,
    /// Execution time
    pub execution_time: Duration,
    /// Tool name
    pub tool_name: String,
    /// Context reference
    pub context_ref: Option<ContextRef>,
    /// Additional metadata
    pub metadata: HashMap<String, Value>,
}

impl ToolExecutionResult {
    /// Create a successful result
    pub fn success(
        result: Value,
        execution_time: Duration,
        tool_name: String,
        context_ref: Option<ContextRef>,
    ) -> Self {
        Self {
            success: true,
            result: Some(result),
            error: None,
            execution_time,
            tool_name,
            context_ref,
            metadata: HashMap::new(),
        }
    }

    /// Create an error result
    pub fn error(
        error: String,
        execution_time: Duration,
        tool_name: String,
        context_ref: Option<ContextRef>,
    ) -> Self {
        Self {
            success: false,
            result: None,
            error: Some(error),
            execution_time,
            tool_name,
            context_ref,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to result
    pub fn add_metadata(&mut self, key: String, value: Value) {
        self.metadata.insert(key, value);
    }
}

/// Tool validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether validation passed
    pub valid: bool,
    /// Validation errors
    pub errors: Vec<String>,
    /// Validation warnings
    pub warnings: Vec<String>,
    /// Additional validation metadata
    pub metadata: HashMap<String, Value>,
}

impl ValidationResult {
    /// Create a successful validation result
    pub fn valid() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Create a failed validation result
    pub fn invalid(errors: Vec<String>) -> Self {
        Self {
            valid: false,
            errors,
            warnings: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Add a warning to the validation result
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    /// Add metadata to the validation result
    pub fn add_metadata(&mut self, key: String, value: Value) {
        self.metadata.insert(key, value);
    }
}

/// Tool execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionStats {
    /// Total number of executions
    pub total_executions: u64,
    /// Number of successful executions
    pub successful_executions: u64,
    /// Number of failed executions
    pub failed_executions: u64,
    /// Average execution time in milliseconds
    pub avg_execution_time_ms: f64,
    /// Minimum execution time in milliseconds
    pub min_execution_time_ms: u64,
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: u64,
    /// Last execution timestamp
    pub last_execution: Option<DateTime<Utc>>,
    /// Tool name
    pub tool_name: String,
}

impl ToolExecutionStats {
    /// Create new execution stats
    pub fn new(tool_name: String) -> Self {
        Self {
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            avg_execution_time_ms: 0.0,
            min_execution_time_ms: u64::MAX,
            max_execution_time_ms: 0,
            last_execution: None,
            tool_name,
        }
    }

    /// Record an execution
    pub fn record_execution(&mut self, execution_time_ms: u64, success: bool) {
        self.total_executions += 1;
        if success {
            self.successful_executions += 1;
        } else {
            self.failed_executions += 1;
        }

        // Update min/max times
        self.min_execution_time_ms = self.min_execution_time_ms.min(execution_time_ms);
        self.max_execution_time_ms = self.max_execution_time_ms.max(execution_time_ms);

        // Update average
        self.avg_execution_time_ms = ((self.avg_execution_time_ms * (self.total_executions - 1) as f64)
            + execution_time_ms as f64) / self.total_executions as f64;

        self.last_execution = Some(Utc::now());
    }
}

/// Tool status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolStatus {
    /// Tool is available and ready
    Available,
    /// Tool is currently executing
    Executing,
    /// Tool is disabled
    Disabled,
    /// Tool has an error
    Error(String),
    /// Tool is loading
    Loading,
}

/// Tool category
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolCategory {
    /// System utilities
    System,
    /// File operations
    File,
    /// Database operations
    Database,
    /// Network operations
    Network,
    /// Vault operations
    Vault,
    /// Search operations
    Search,
    /// AI/ML operations
    AI,
    /// General purpose
    General,
}

impl std::fmt::Display for ToolCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolCategory::System => write!(f, "System"),
            ToolCategory::File => write!(f, "File"),
            ToolCategory::Database => write!(f, "Database"),
            ToolCategory::Network => write!(f, "Network"),
            ToolCategory::Vault => write!(f, "Vault"),
            ToolCategory::Search => write!(f, "Search"),
            ToolCategory::AI => write!(f, "AI"),
            ToolCategory::General => write!(f, "General"),
        }
    }
}

impl std::str::FromStr for ToolCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "system" => Ok(ToolCategory::System),
            "file" => Ok(ToolCategory::File),
            "database" => Ok(ToolCategory::Database),
            "network" => Ok(ToolCategory::Network),
            "vault" => Ok(ToolCategory::Vault),
            "search" => Ok(ToolCategory::Search),
            "ai" => Ok(ToolCategory::AI),
            "general" => Ok(ToolCategory::General),
            _ => Err(format!("Unknown tool category: {}", s)),
        }
    }
}