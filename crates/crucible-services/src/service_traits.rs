//! # Comprehensive Service Trait Definitions
//!
//! This module provides complete trait definitions for the core services in the simplified
//! Crucible architecture. Each service trait follows async/await patterns with comprehensive
//! error handling, lifecycle management, and event integration.

use super::{errors::ServiceResult, types::*, service_types::*, database::*};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use crucible_llm::text_generation::ToolDefinition;

/// ============================================================================
/// COMMON SERVICE TRAITS
/// ============================================================================

/// Base trait that all services must implement for lifecycle management
#[async_trait]
pub trait ServiceLifecycle: Send + Sync {
    /// Start the service with given configuration
    async fn start(&mut self) -> ServiceResult<()>;

    /// Stop the service gracefully
    async fn stop(&mut self) -> ServiceResult<()>;

    /// Restart the service
    async fn restart(&mut self) -> ServiceResult<()> {
        self.stop().await?;
        self.start().await
    }

    /// Check if the service is currently running
    fn is_running(&self) -> bool;

    /// Get the service name
    fn service_name(&self) -> &str;

    /// Get the service version
    fn service_version(&self) -> &str;
}

/// Trait for health check capabilities
#[async_trait]
pub trait HealthCheck: Send + Sync {
    /// Perform a comprehensive health check
    async fn health_check(&self) -> ServiceResult<ServiceHealth>;

    /// Perform a quick liveness check (is the service responding?)
    async fn liveness_check(&self) -> ServiceResult<bool> {
        Ok(self.health_check().await?.status == ServiceStatus::Healthy)
    }

    /// Perform a readiness check (is the service ready to handle requests?)
    async fn readiness_check(&self) -> ServiceResult<bool> {
        self.liveness_check().await
    }
}

/// Trait for configuration management
#[async_trait]
pub trait Configurable: Send + Sync {
    /// Configuration type for this service
    type Config: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de>;

    /// Get current configuration
    async fn get_config(&self) -> ServiceResult<Self::Config>;

    /// Update configuration
    async fn update_config(&mut self, config: Self::Config) -> ServiceResult<()>;

    /// Validate configuration
    async fn validate_config(&self, config: &Self::Config) -> ServiceResult<()>;

    /// Reload configuration from source
    async fn reload_config(&mut self) -> ServiceResult<()>;
}

/// Trait for metrics and monitoring
#[async_trait]
pub trait Observable: Send + Sync {
    /// Get current service metrics
    async fn get_metrics(&self) -> ServiceResult<ServiceMetrics>;

    /// Reset metrics (for testing or maintenance)
    async fn reset_metrics(&mut self) -> ServiceResult<()>;

    /// Get detailed performance metrics
    async fn get_performance_metrics(&self) -> ServiceResult<PerformanceMetrics>;
}

/// Trait for event handling and subscription
#[async_trait]
pub trait EventDriven: Send + Sync {
    /// Event type this service can handle
    type Event: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de>;

    /// Subscribe to events
    async fn subscribe(&mut self, event_type: &str) -> ServiceResult<mpsc::UnboundedReceiver<Self::Event>>;

    /// Unsubscribe from events
    async fn unsubscribe(&mut self, event_type: &str) -> ServiceResult<()>;

    /// Publish an event
    async fn publish(&self, event: Self::Event) -> ServiceResult<()>;

    /// Handle incoming events
    async fn handle_event(&mut self, event: Self::Event) -> ServiceResult<()>;
}

/// Trait for resource management
#[async_trait]
pub trait ResourceManager: Send + Sync {
    /// Get current resource usage
    async fn get_resource_usage(&self) -> ServiceResult<ResourceUsage>;

    /// Set resource limits
    async fn set_limits(&mut self, limits: ResourceLimits) -> ServiceResult<()>;

    /// Get current limits
    async fn get_limits(&self) -> ServiceResult<ResourceLimits>;

    /// Cleanup resources
    async fn cleanup_resources(&mut self) -> ServiceResult<()>;
}

/// ============================================================================
/// MCP GATEWAY SERVICE
/// ============================================================================

/// MCP Gateway service for handling MCP protocol operations and tool management
#[async_trait]
pub trait McpGateway: ServiceLifecycle + HealthCheck + Configurable + Observable + EventDriven + ResourceManager {
    /// MCP gateway specific configuration
    type Config: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de>;

    /// MCP event types
    type Event: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de>;

    // -------------------------------------------------------------------------
    // MCP Protocol Operations
    // -------------------------------------------------------------------------

    /// Initialize MCP connection with a client
    async fn initialize_connection(&self, client_id: &str, capabilities: McpCapabilities) -> ServiceResult<McpSession>;

    /// Close MCP connection
    async fn close_connection(&self, session_id: &str) -> ServiceResult<()>;

    /// List active MCP connections
    async fn list_connections(&self) -> ServiceResult<Vec<McpSession>>;

    /// Send notification to client
    async fn send_notification(&self, session_id: &str, notification: McpNotification) -> ServiceResult<()>;

    /// Handle incoming request from client
    async fn handle_request(&self, session_id: &str, request: McpRequest) -> ServiceResult<McpResponse>;

    // -------------------------------------------------------------------------
    // Tool Management
    // -------------------------------------------------------------------------

    /// Register a new tool with the MCP gateway
    async fn register_tool(&mut self, tool: ToolDefinition) -> ServiceResult<()>;

    /// Unregister a tool
    async fn unregister_tool(&mut self, tool_name: &str) -> ServiceResult<()>;

    /// List all registered tools
    async fn list_tools(&self) -> ServiceResult<Vec<ToolDefinition>>;

    /// Get tool definition by name
    async fn get_tool(&self, name: &str) -> ServiceResult<Option<ToolDefinition>>;

    /// Update tool definition
    async fn update_tool(&mut self, tool: ToolDefinition) -> ServiceResult<()>;

    // -------------------------------------------------------------------------
    // Tool Execution
    // -------------------------------------------------------------------------

    /// Execute a tool via MCP protocol
    async fn execute_tool(&self, request: McpToolRequest) -> ServiceResult<McpToolResponse>;

    /// Cancel an ongoing tool execution
    async fn cancel_execution(&self, execution_id: &str) -> ServiceResult<()>;

    /// Get execution status
    async fn get_execution_status(&self, execution_id: &str) -> ServiceResult<ExecutionStatus>;

    /// List active executions
    async fn list_active_executions(&self) -> ServiceResult<Vec<ActiveExecution>>;

    // -------------------------------------------------------------------------
    // Protocol Capabilities
    // -------------------------------------------------------------------------

    /// Get supported MCP capabilities
    async fn get_capabilities(&self) -> ServiceResult<McpCapabilities>;

    /// Set server capabilities
    async fn set_capabilities(&mut self, capabilities: McpCapabilities) -> ServiceResult<()>;

    /// Negotiate capabilities with client
    async fn negotiate_capabilities(&self, client_capabilities: McpCapabilities) -> ServiceResult<McpCapabilities>;

    // -------------------------------------------------------------------------
    // Resource Management
    // -------------------------------------------------------------------------

    /// Get MCP-specific resource usage
    async fn get_mcp_resources(&self) -> ServiceResult<McpResourceUsage>;

    /// Configure MCP protocol settings
    async fn configure_protocol(&mut self, settings: McpProtocolSettings) -> ServiceResult<()>;
}

/// ============================================================================
/// INFERENCE ENGINE SERVICE
/// ============================================================================

/// Inference Engine service for AI/LLM operations
#[async_trait]
pub trait InferenceEngine: ServiceLifecycle + HealthCheck + Configurable + Observable + EventDriven + ResourceManager {
    /// Inference engine specific configuration
    type Config: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de>;

    /// Inference engine event types
    type Event: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de>;

    // -------------------------------------------------------------------------
    // Model Management
    // -------------------------------------------------------------------------

    /// Load a model
    async fn load_model(&mut self, model_config: ModelConfig) -> ServiceResult<ModelInfo>;

    /// Unload a model
    async fn unload_model(&mut self, model_id: &str) -> ServiceResult<()>;

    /// List loaded models
    async fn list_models(&self) -> ServiceResult<Vec<ModelInfo>>;

    /// Get model information
    async fn get_model(&self, model_id: &str) -> ServiceResult<Option<ModelInfo>>;

    /// Switch active model
    async fn switch_model(&mut self, model_id: &str) -> ServiceResult<()>;

    // -------------------------------------------------------------------------
    // Text Generation
    // -------------------------------------------------------------------------

    /// Generate text completion
    async fn generate_completion(&self, request: CompletionRequest) -> ServiceResult<CompletionResponse>;

    /// Generate streaming text completion
    async fn generate_completion_stream(&self, request: CompletionRequest) -> ServiceResult<mpsc::UnboundedReceiver<CompletionChunk>>;

    /// Generate chat completion
    async fn generate_chat_completion(&self, request: ChatCompletionRequest) -> ServiceResult<ChatCompletionResponse>;

    /// Generate streaming chat completion
    async fn generate_chat_completion_stream(&self, request: ChatCompletionRequest) -> ServiceResult<mpsc::UnboundedReceiver<ChatCompletionChunk>>;

    // -------------------------------------------------------------------------
    // Embedding Generation
    // -------------------------------------------------------------------------

    /// Generate text embeddings
    async fn generate_embeddings(&self, request: EmbeddingRequest) -> ServiceResult<EmbeddingResponse>;

    /// Generate embeddings for multiple texts
    async fn generate_batch_embeddings(&self, request: BatchEmbeddingRequest) -> ServiceResult<BatchEmbeddingResponse>;

    // -------------------------------------------------------------------------
    // Advanced Inference
    // -------------------------------------------------------------------------

    /// Perform reasoning task
    async fn perform_reasoning(&self, request: ReasoningRequest) -> ServiceResult<ReasoningResponse>;

    /// Perform tool use inference
    async fn perform_tool_use(&self, request: ToolUseRequest) -> ServiceResult<ToolUseResponse>;

    /// Perform semantic search
    async fn semantic_search(&self, request: SemanticSearchRequest) -> ServiceResult<SemanticSearchResponse>;

    // -------------------------------------------------------------------------
    // Model Optimization
    // -------------------------------------------------------------------------

    /// Fine-tune a model
    async fn fine_tune_model(&mut self, request: FineTuningRequest) -> ServiceResult<FineTuningJob>;

    /// Get fine-tuning job status
    async fn get_fine_tuning_status(&self, job_id: &str) -> ServiceResult<FineTuningStatus>;

    /// Optimize model for inference
    async fn optimize_model(&mut self, model_id: &str, optimization: ModelOptimization) -> ServiceResult<ModelInfo>;

    // -------------------------------------------------------------------------
    // Resource Management
    // -------------------------------------------------------------------------

    /// Get model resource usage
    async fn get_model_resources(&self, model_id: &str) -> ServiceResult<ModelResourceUsage>;

    /// Set inference limits
    async fn set_inference_limits(&mut self, limits: InferenceLimits) -> ServiceResult<()>;

    /// Get inference statistics
    async fn get_inference_stats(&self) -> ServiceResult<InferenceStatistics>;
}

/// ============================================================================
/// SCRIPT ENGINE SERVICE
/// ============================================================================

/// Script Engine service for Rune script execution
#[async_trait]
pub trait ScriptEngine: ServiceLifecycle + HealthCheck + Configurable + Observable + EventDriven + ResourceManager {
    /// Script engine specific configuration
    type Config: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de>;

    /// Script engine event types
    type Event: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de>;

    // -------------------------------------------------------------------------
    // Script Compilation
    // -------------------------------------------------------------------------

    /// Compile a script from source code
    async fn compile_script(&mut self, source: &str, context: CompilationContext) -> ServiceResult<CompiledScript>;

    /// Compile script from file
    async fn compile_script_file(&mut self, file_path: &str, context: CompilationContext) -> ServiceResult<CompiledScript>;

    /// Get compilation errors for a script
    async fn get_compilation_errors(&self, script_id: &str) -> ServiceResult<Vec<CompilationError>>;

    /// Revalidate a compiled script
    async fn revalidate_script(&self, script_id: &str) -> ServiceResult<crate::service_types::ValidationResult>;

    // -------------------------------------------------------------------------
    // Script Execution
    // -------------------------------------------------------------------------

    /// Execute a compiled script
    async fn execute_script(&self, script_id: &str, context: ExecutionContext) -> ServiceResult<ExecutionResult>;

    /// Execute script directly from source
    async fn execute_script_source(&self, source: &str, context: ExecutionContext) -> ServiceResult<ExecutionResult>;

    /// Execute script with streaming output
    async fn execute_script_stream(&self, script_id: &str, context: ExecutionContext) -> ServiceResult<mpsc::UnboundedReceiver<ExecutionChunk>>;

    /// Cancel script execution
    async fn cancel_execution(&self, execution_id: &str) -> ServiceResult<()>;

    // -------------------------------------------------------------------------
    // Tool Integration
    // -------------------------------------------------------------------------

    /// Register a tool with the script engine
    async fn register_tool(&mut self, tool: ScriptTool) -> ServiceResult<()>;

    /// Unregister a tool
    async fn unregister_tool(&mut self, tool_name: &str) -> ServiceResult<()>;

    /// List available tools
    async fn list_script_tools(&self) -> ServiceResult<Vec<ScriptTool>>;

    /// Get tool definition
    async fn get_script_tool(&self, name: &str) -> ServiceResult<Option<ScriptTool>>;

    // -------------------------------------------------------------------------
    // Script Management
    // -------------------------------------------------------------------------

    /// List compiled scripts
    async fn list_scripts(&self) -> ServiceResult<Vec<ScriptInfo>>;

    /// Get script information
    async fn get_script_info(&self, script_id: &str) -> ServiceResult<Option<ScriptInfo>>;

    /// Delete compiled script
    async fn delete_script(&mut self, script_id: &str) -> ServiceResult<()>;

    /// Update script context
    async fn update_script_context(&mut self, script_id: &str, context: ExecutionContext) -> ServiceResult<()>;

    // -------------------------------------------------------------------------
    // Security and Sandboxing
    // -------------------------------------------------------------------------

    /// Set security policy for script execution
    async fn set_security_policy(&mut self, policy: SecurityPolicy) -> ServiceResult<()>;

    /// Get current security policy
    async fn get_security_policy(&self) -> ServiceResult<SecurityPolicy>;

    /// Validate script security
    async fn validate_script_security(&self, script_id: &str) -> ServiceResult<SecurityValidationResult>;

    // -------------------------------------------------------------------------
    // Performance Optimization
    // -------------------------------------------------------------------------

    /// Precompile script for better performance
    async fn precompile_script(&mut self, script_id: &str) -> ServiceResult<CompilationResult>;

    /// Cache compiled script
    async fn cache_script(&mut self, script_id: &str, cache_config: CacheConfig) -> ServiceResult<()>;

    /// Clear script cache
    async fn clear_cache(&mut self) -> ServiceResult<()>;

    /// Get execution statistics
    async fn get_execution_stats(&self) -> ServiceResult<ScriptExecutionStats>;
}

/// ============================================================================
/// DATA STORE SERVICE
/// ============================================================================

/// Data Store service for database operations and persistence
#[async_trait]
pub trait DataStore: ServiceLifecycle + HealthCheck + Configurable + Observable + EventDriven + ResourceManager {
    /// Data store specific configuration
    type Config: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de>;

    /// Data store event types
    type Event: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de>;

    // -------------------------------------------------------------------------
    // Database Operations
    // -------------------------------------------------------------------------

    /// Create a new database/collection
    async fn create_database(&mut self, name: &str, schema: Option<DatabaseSchema>) -> ServiceResult<DatabaseInfo>;

    /// Drop a database/collection
    async fn drop_database(&mut self, name: &str) -> ServiceResult<()>;

    /// List all databases
    async fn list_databases(&self) -> ServiceResult<Vec<DatabaseInfo>>;

    /// Get database information
    async fn get_database(&self, name: &str) -> ServiceResult<Option<DatabaseInfo>>;

    /// Get connection status
    async fn get_connection_status(&self) -> ServiceResult<ConnectionStatus>;

    // -------------------------------------------------------------------------
    // CRUD Operations
    // -------------------------------------------------------------------------

    /// Create a new document/record
    async fn create(&self, database: &str, data: DocumentData) -> ServiceResult<DocumentId>;

    /// Read a document/record by ID
    async fn read(&self, database: &str, id: &str) -> ServiceResult<Option<DocumentData>>;

    /// Update a document/record
    async fn update(&self, database: &str, id: &str, data: DocumentData) -> ServiceResult<DocumentData>;

    /// Delete a document/record
    async fn delete(&self, database: &str, id: &str) -> ServiceResult<bool>;

    /// Upsert (create or update) a document
    async fn upsert(&self, database: &str, id: &str, data: DocumentData) -> ServiceResult<DocumentData>;

    // -------------------------------------------------------------------------
    // Query Operations
    // -------------------------------------------------------------------------

    /// Execute a query
    async fn query(&self, database: &str, query: Query) -> ServiceResult<QueryResult>;

    /// Execute a query with streaming results
    async fn query_stream(&self, database: &str, query: Query) -> ServiceResult<mpsc::UnboundedReceiver<DocumentData>>;

    /// Execute an aggregation query
    async fn aggregate(&self, database: &str, pipeline: AggregationPipeline) -> ServiceResult<AggregationResult>;

    /// Perform full-text search
    async fn search(&self, database: &str, search_query: SearchQuery) -> ServiceResult<SearchResult>;

    /// Perform vector similarity search
    async fn vector_search(&self, database: &str, vector: Vec<f32>, options: VectorSearchOptions) -> ServiceResult<VectorSearchResult>;

    // -------------------------------------------------------------------------
    // Batch Operations
    // -------------------------------------------------------------------------

    /// Bulk insert documents
    async fn bulk_insert(&self, database: &str, documents: Vec<DocumentData>) -> ServiceResult<BulkInsertResult>;

    /// Bulk update documents
    async fn bulk_update(&self, database: &str, updates: Vec<UpdateOperation>) -> ServiceResult<BulkUpdateResult>;

    /// Bulk delete documents
    async fn bulk_delete(&self, database: &str, ids: Vec<DocumentId>) -> ServiceResult<BulkDeleteResult>;

    // -------------------------------------------------------------------------
    // Transaction Support
    // -------------------------------------------------------------------------

    /// Begin a transaction
    async fn begin_transaction(&self) -> ServiceResult<TransactionId>;

    /// Commit a transaction
    async fn commit_transaction(&self, transaction_id: &str) -> ServiceResult<()>;

    /// Rollback a transaction
    async fn rollback_transaction(&self, transaction_id: &str) -> ServiceResult<()>;

    /// Execute operation within transaction
    async fn execute_in_transaction<F, R>(&self, transaction_id: &str, operation: F) -> ServiceResult<R>
    where
        F: FnOnce() -> ServiceResult<R> + Send + Sync,
        R: Send + Sync;

    // -------------------------------------------------------------------------
    // Index Management
    // -------------------------------------------------------------------------

    /// Create an index
    async fn create_index(&mut self, database: &str, index: IndexDefinition) -> ServiceResult<IndexInfo>;

    /// Drop an index
    async fn drop_index(&mut self, database: &str, index_name: &str) -> ServiceResult<()>;

    /// List indexes
    async fn list_indexes(&self, database: &str) -> ServiceResult<Vec<IndexInfo>>;

    /// Get index statistics
    async fn get_index_stats(&self, database: &str, index_name: &str) -> ServiceResult<IndexStats>;

    // -------------------------------------------------------------------------
    // Backup and Restore
    // -------------------------------------------------------------------------

    /// Create a backup
    async fn create_backup(&self, database: &str, backup_config: BackupConfig) -> ServiceResult<BackupInfo>;

    /// Restore from backup
    async fn restore_backup(&mut self, backup_id: &str, restore_config: RestoreConfig) -> ServiceResult<RestoreResult>;

    /// List backups
    async fn list_backups(&self) -> ServiceResult<Vec<BackupInfo>>;

    /// Delete backup
    async fn delete_backup(&mut self, backup_id: &str) -> ServiceResult<()>;

    // -------------------------------------------------------------------------
    // Replication and Sync
    // -------------------------------------------------------------------------

    /// Configure replication
    async fn configure_replication(&mut self, config: ReplicationConfig) -> ServiceResult<()>;

    /// Get replication status
    async fn get_replication_status(&self) -> ServiceResult<ReplicationStatus>;

    /// Sync with remote database
    async fn sync_database(&mut self, database: &str, sync_config: SyncConfig) -> ServiceResult<SyncResult>;

    // -------------------------------------------------------------------------
    // Schema Management
    // -------------------------------------------------------------------------

    /// Create schema
    async fn create_schema(&mut self, database: &str, schema: DatabaseSchema) -> ServiceResult<SchemaInfo>;

    /// Update schema
    async fn update_schema(&mut self, database: &str, schema: DatabaseSchema) -> ServiceResult<SchemaInfo>;

    /// Get schema
    async fn get_schema(&self, database: &str) -> ServiceResult<Option<DatabaseSchema>>;

    /// Validate document against schema
    async fn validate_document(&self, database: &str, document: &DocumentData) -> ServiceResult<crate::service_types::ValidationResult>;
}

/// ============================================================================
/// SERVICE REGISTRY AND MANAGEMENT
/// ============================================================================

/// Service registry for managing multiple services
#[async_trait]
pub trait ServiceRegistry: Send + Sync {
    /// Register a service
    async fn register_service(&mut self, service: Arc<dyn ServiceLifecycle>) -> ServiceResult<()>;

    /// Unregister a service
    async fn unregister_service(&mut self, service_name: &str) -> ServiceResult<()>;

    /// Get a service by name
    async fn get_service(&self, service_name: &str) -> ServiceResult<Option<Arc<dyn ServiceLifecycle>>>;

    /// List all registered services
    async fn list_services(&self) -> ServiceResult<Vec<ServiceInfo>>;

    /// Start all services
    async fn start_all(&mut self) -> ServiceResult<()>;

    /// Stop all services
    async fn stop_all(&mut self) -> ServiceResult<()>;

    /// Get overall system health
    async fn system_health(&self) -> ServiceResult<SystemHealth>;

    /// Route events between services
    async fn route_event(&self, event_type: &str, payload: serde_json::Value) -> ServiceResult<()>;
}