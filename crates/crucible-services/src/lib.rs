//! # Crucible Services - Simplified
//!
//! This crate provides minimal service abstractions for the Crucible knowledge management system.
//! It focuses on essential traits and types without over-engineering.

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// Logging and debugging framework
pub mod logging;

// Event routing system
pub mod event_routing;

// Configuration management
pub mod config;

// Debugging utilities
pub mod debugging;

// Type definitions
pub mod types;

// Script engine service
pub mod script_engine;

/// Simplified service trait definitions
pub mod service_traits;

/// Essential service type definitions
pub mod service_types;

/// Re-export commonly used types
pub use service_types::ScriptEngineConfig;

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

/// Basic tool service trait for backward compatibility
pub mod traits {
    use super::errors::ServiceResult;
    use crate::types::{ToolDefinition, ToolExecutionRequest, ToolExecutionResult};
    use async_trait::async_trait;

    /// Basic tool service trait - simplified version
    #[async_trait]
    pub trait ToolService: Send + Sync {
        /// List all available tools
        async fn list_tools(&self) -> ServiceResult<Vec<ToolDefinition>>;

        /// Get tool definition by name
        async fn get_tool(&self, name: &str) -> ServiceResult<Option<ToolDefinition>>;

        /// Execute a tool
        async fn execute_tool(
            &self,
            request: ToolExecutionRequest,
        ) -> ServiceResult<ToolExecutionResult>;

        /// Get service health and status
        async fn service_health(&self) -> ServiceResult<super::types::ServiceHealth>;
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
        fn connection_status(
            &self,
        ) -> impl std::future::Future<Output = ServiceResult<ConnectionStatus>> + Send;
        fn create_database(
            &self,
            name: &str,
        ) -> impl std::future::Future<Output = ServiceResult<DatabaseInfo>> + Send;
        fn list_databases(
            &self,
        ) -> impl std::future::Future<Output = ServiceResult<Vec<DatabaseInfo>>> + Send;
        fn get_database(
            &self,
            name: &str,
        ) -> impl std::future::Future<Output = ServiceResult<Option<DatabaseInfo>>> + Send;
        fn drop_database(
            &self,
            name: &str,
        ) -> impl std::future::Future<Output = ServiceResult<bool>> + Send;
        fn apply_schema_changes(
            &self,
            database: &str,
            changes: Vec<SchemaChange>,
        ) -> impl std::future::Future<Output = ServiceResult<bool>> + Send;
        fn create_transaction(
            &self,
            database: &str,
        ) -> impl std::future::Future<Output = ServiceResult<TransactionStatus>> + Send;
    }
}
