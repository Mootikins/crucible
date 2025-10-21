//! # Crucible Services - Simplified
//!
//! This crate provides minimal service abstractions for the Crucible knowledge management system.
//! It focuses on essential traits and types without over-engineering.


/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Basic service error and result types
pub mod errors {
    use thiserror::Error;

    /// Service error type
    #[derive(Error, Debug)]
    pub enum ServiceError {
        #[error("Service not found: {0}")]
        ServiceNotFound(String),

        #[error("Tool not found: {0}")]
        ToolNotFound(String),

        #[error("Execution error: {0}")]
        ExecutionError(String),

        #[error("Configuration error: {0}")]
        ConfigurationError(String),

        #[error("Validation error: {0}")]
        ValidationError(String),

        #[error("IO error: {0}")]
        IoError(#[from] std::io::Error),

        #[error("Serialization error: {0}")]
        SerializationError(#[from] serde_json::Error),

        #[error("Other error: {0}")]
        Other(String),
    }

    impl ServiceError {
        pub fn execution_error(msg: impl Into<String>) -> Self {
            Self::ExecutionError(msg.into())
        }

        pub fn config_error(msg: impl Into<String>) -> Self {
            Self::ConfigurationError(msg.into())
        }

        pub fn validation_error(msg: impl Into<String>) -> Self {
            Self::ValidationError(msg.into())
        }
    }

    /// Service result type
    pub type ServiceResult<T> = Result<T, ServiceError>;
}

/// Essential service traits
pub mod traits {
    use super::{errors::ServiceResult, types::tool::*};
    use async_trait::async_trait;

    /// Basic tool service trait - simplified version
    #[async_trait]
    pub trait ToolService: Send + Sync {
        /// List all available tools
        async fn list_tools(&self) -> ServiceResult<Vec<ToolDefinition>>;

        /// Get tool definition by name
        async fn get_tool(&self, name: &str) -> ServiceResult<Option<ToolDefinition>>;

        /// Execute a tool
        async fn execute_tool(&self, request: ToolExecutionRequest) -> ServiceResult<ToolExecutionResult>;

        /// Validate a tool without executing it
        async fn validate_tool(&self, name: &str) -> ServiceResult<ValidationResult>;

        /// Get service health and status
        async fn service_health(&self) -> ServiceResult<super::types::ServiceHealth>;

        /// Get performance metrics
        async fn get_metrics(&self) -> ServiceResult<super::types::ServiceMetrics>;
    }
}

/// Essential types
pub mod types {
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    /// Basic service health information
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ServiceHealth {
        pub status: ServiceStatus,
        pub message: Option<String>,
        pub last_check: chrono::DateTime<chrono::Utc>,
        pub details: HashMap<String, String>,
    }

    /// Service status
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub enum ServiceStatus {
        Healthy,
        Degraded,
        Unhealthy,
    }

    /// Basic service metrics
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ServiceMetrics {
        pub total_requests: u64,
        pub successful_requests: u64,
        pub failed_requests: u64,
        pub average_response_time: std::time::Duration,
        pub uptime: std::time::Duration,
        pub memory_usage: u64,
        pub cpu_usage: f64,
    }

    /// Tool-specific types - minimal version of what's actually needed
    pub mod tool {
        use super::*;
        use serde::{Deserialize, Serialize};

        /// Tool definition - simplified
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct ToolDefinition {
            pub name: String,
            pub description: String,
            pub input_schema: serde_json::Value,
            pub category: Option<String>,
            pub version: Option<String>,
            pub author: Option<String>,
            pub tags: Vec<String>,
            pub enabled: bool,
            pub parameters: Vec<ToolParameter>,
        }

        /// Tool parameter definition
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct ToolParameter {
            pub name: String,
            pub param_type: String,
            pub description: Option<String>,
            pub required: bool,
            pub default_value: Option<serde_json::Value>,
        }

        /// Tool execution request
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct ToolExecutionRequest {
            pub tool_name: String,
            pub parameters: serde_json::Value,
            pub context: ToolExecutionContext,
            pub timeout_ms: Option<u64>,
        }

        /// Tool execution context
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct ToolExecutionContext {
            pub user_id: Option<String>,
            pub session_id: Option<String>,
            pub working_directory: Option<String>,
            pub environment: HashMap<String, String>,
            pub context: HashMap<String, String>,
        }

        /// Tool execution result
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct ToolExecutionResult {
            pub success: bool,
            pub result: Option<serde_json::Value>,
            pub error: Option<String>,
            pub execution_time: std::time::Duration,
            pub tool_name: String,
            pub context: ToolExecutionContext,
        }

        /// Tool validation result
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct ValidationResult {
            pub valid: bool,
            pub errors: Vec<String>,
            pub warnings: Vec<String>,
            pub tool_name: String,
        }

        impl Default for ToolExecutionContext {
            fn default() -> Self {
                Self {
                    user_id: None,
                    session_id: None,
                    working_directory: None,
                    environment: HashMap::new(),
                    context: HashMap::new(),
                }
            }
        }
    }
}

// Re-export main components for easier access
pub use errors::*;
pub use traits::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_service_error_creation() {
        let error = ServiceError::execution_error("test error");
        assert!(matches!(error, ServiceError::ExecutionError(_)));
    }

    #[test]
    fn test_tool_execution_context() {
        let context = tool::ToolExecutionContext::default();
        assert!(context.user_id.is_none());
        assert!(context.environment.is_empty());
    }
}