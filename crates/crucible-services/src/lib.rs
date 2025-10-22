//! # Crucible Services - Simplified
//!
//! This crate provides minimal service abstractions for the Crucible knowledge management system.
//! It focuses on essential traits and types without over-engineering.


/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// Event system for daemon coordination
pub mod events;

/// Comprehensive service trait definitions
pub mod service_traits;

/// Service type definitions
pub mod service_types;

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

/// Essential service traits (maintaining compatibility)
pub mod traits {
    use super::{errors::ServiceResult, types::*};
    use async_trait::async_trait;
    use crucible_llm::text_generation::{ToolDefinition, ToolExecutionRequest, ToolExecutionResult};
    use super::types::tool::ValidationResult;

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

/// Database service traits (minimal compatibility layer)
pub mod database {
    use super::errors::ServiceResult;
    use serde::{Deserialize, Serialize};

    /// Database connection status
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub enum ConnectionStatus {
        Connected,
        Disconnected,
        Connecting,
        Error(String),
    }

    /// Database information
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct DatabaseInfo {
        pub name: String,
        pub status: ConnectionStatus,
        pub size_bytes: Option<u64>,
        pub table_count: Option<u32>,
        pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    }

    /// Schema change information
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SchemaChange {
        pub table_name: String,
        pub change_type: ChangeType,
        pub sql: String,
    }

    /// Schema change type
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub enum ChangeType {
        Create,
        Drop,
        Alter,
    }

    /// Transaction status
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub enum TransactionStatus {
        Active,
        Committed,
        RolledBack,
    }

    /// Minimal database service trait
    pub trait DatabaseService: Send + Sync {
        fn connection_status(&self) -> impl std::future::Future<Output = ServiceResult<ConnectionStatus>> + Send;
        fn create_database(&self, name: &str) -> impl std::future::Future<Output = ServiceResult<DatabaseInfo>> + Send;
        fn list_databases(&self) -> impl std::future::Future<Output = ServiceResult<Vec<DatabaseInfo>>> + Send;
        fn get_database(&self, name: &str) -> impl std::future::Future<Output = ServiceResult<Option<DatabaseInfo>>> + Send;
        fn drop_database(&self, name: &str) -> impl std::future::Future<Output = ServiceResult<bool>> + Send;
        fn apply_schema_changes(&self, database: &str, changes: Vec<SchemaChange>) -> impl std::future::Future<Output = ServiceResult<bool>> + Send;
        fn create_transaction(&self, database: &str) -> impl std::future::Future<Output = ServiceResult<TransactionStatus>> + Send;
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

    }

/// MCP Gateway service implementation
// pub mod mcp_gateway;

/// Data Store service implementation
// pub mod data_store;

/// Script Engine service implementation
pub mod script_engine;

/// Plugin Manager service implementation
pub mod plugin_manager;

/// Inference Engine service implementation
pub mod inference_engine;

/// Plugin Event Subscription System
pub mod plugin_events;

// Services unit tests
#[cfg(test)]
pub mod services;

// Memory testing framework
#[cfg(feature = "memory-testing")]
pub mod memory_testing;

#[cfg(feature = "memory-testing")]
pub use memory_testing::*;

// Re-export main components for easier access
pub use errors::*;
pub use traits::*;
pub use types::*;
pub use service_traits::*;
pub use service_types::*;
pub use events::*;
// pub use mcp_gateway::*;
// pub use data_store::*;
// pub use script_engine::*;
pub use inference_engine::*;
pub use plugin_events::*;

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