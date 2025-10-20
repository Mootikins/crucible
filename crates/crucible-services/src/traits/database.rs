use async_trait::async_trait;
use crate::errors::{ServiceError, ServiceResult};
use crate::types::{ServiceInfo, ServiceHealth, ServiceMetrics, ServiceDependency};
use crate::types::tool::ToolExecutionContext;
use crucible_core::database::*;
use std::collections::HashMap;
use uuid::Uuid;
use super::BaseService;

/// Trait for database services that handle storage operations
#[async_trait]
pub trait DatabaseService: BaseService + Send + Sync {
    // === Connection Management ===

    /// Get database connection status
    async fn connection_status(&self) -> ServiceResult<ConnectionStatus>;

    /// Create a new database
    async fn create_database(&self, name: &str) -> ServiceResult<DatabaseInfo>;

    /// Drop a database
    async fn drop_database(&self, name: &str) -> ServiceResult<bool>;

    /// List all databases
    async fn list_databases(&self) -> ServiceResult<Vec<DatabaseInfo>>;

    /// Get database information
    async fn get_database_info(&self, name: &str) -> ServiceResult<Option<DatabaseInfo>>;

    // === Schema Management ===

    /// Create a table/collection
    async fn create_table(&self, table_schema: TableSchema) -> ServiceResult<String>;

    /// Drop a table/collection
    async fn drop_table(&self, table_name: &str) -> ServiceResult<bool>;

    /// Get table schema
    async fn get_table_schema(&self, table_name: &str) -> ServiceResult<Option<TableSchema>>;

    /// List all tables
    async fn list_tables(&self) -> ServiceResult<Vec<TableSchema>>;

    /// Alter table schema
    async fn alter_table(&self, table_name: &str, changes: Vec<SchemaChange>) -> ServiceResult<bool>;

    /// Create index
    async fn create_index(&self, index_def: IndexDefinition) -> ServiceResult<String>;

    /// Drop index
    async fn drop_index(&self, index_name: &str) -> ServiceResult<bool>;

    /// List indexes
    async fn list_indexes(&self, table_name: &str) -> ServiceResult<Vec<IndexDefinition>>;

    // === Relational Operations ===

    /// Execute SELECT query
    async fn select(&self, query: SelectQuery) -> ServiceResult<QueryResult>;

    /// Execute INSERT operation
    async fn insert(&self, table_name: &str, records: Vec<Record>) -> ServiceResult<BatchResult>;

    /// Execute UPDATE operation
    async fn update(&self, query: UpdateClause) -> ServiceResult<BatchResult>;

    /// Execute DELETE operation
    async fn delete(&self, table_name: &str, filter: Option<FilterClause>) -> ServiceResult<BatchResult>;

    /// Execute JOIN query
    async fn join(&self, query: JoinQuery) -> ServiceResult<QueryResult>;

    /// Execute aggregation query
    async fn aggregate(&self, query: AggregateQuery) -> ServiceResult<QueryResult>;

    // === Document Operations ===

    /// Insert document
    async fn insert_document(&self, database: &str, collection: &str, document: Document) -> ServiceResult<DocumentId>;

    /// Find documents
    async fn find_documents(&self, query: DocumentQuery) -> ServiceResult<Vec<Document>>;

    /// Find one document
    async fn find_one_document(&self, query: DocumentQuery) -> ServiceResult<Option<Document>>;

    /// Update documents
    async fn update_documents(&self, database: &str, collection: &str, filter: DocumentFilter, updates: DocumentUpdates) -> ServiceResult<BatchResult>;

    /// Delete documents
    async fn delete_documents(&self, database: &str, collection: &str, filter: DocumentFilter) -> ServiceResult<BatchResult>;

    /// Count documents
    async fn count_documents(&self, database: &str, collection: &str, filter: Option<DocumentFilter>) -> ServiceResult<u64>;

    /// Search documents (full-text)
    async fn search_documents(&self, database: &str, collection: &str, query: &str, options: SearchOptions) -> ServiceResult<Vec<SearchResult>>;

    /// Execute aggregation pipeline
    async fn aggregate_documents(&self, database: &str, collection: &str, pipeline: AggregationPipeline) -> ServiceResult<Vec<AggregationResult>>;

    // === Graph Operations ===

    /// Create node
    async fn create_node(&self, node: Node) -> ServiceResult<NodeId>;

    /// Get node by ID
    async fn get_node(&self, node_id: NodeId) -> ServiceResult<Option<Node>>;

    /// Update node
    async fn update_node(&self, node_id: NodeId, properties: NodeProperties) -> ServiceResult<bool>;

    /// Delete node
    async fn delete_node(&self, node_id: NodeId) -> ServiceResult<bool>;

    /// Create edge
    async fn create_edge(&self, edge: Edge) -> ServiceResult<EdgeId>;

    /// Get edge by ID
    async fn get_edge(&self, edge_id: EdgeId) -> ServiceResult<Option<Edge>>;

    /// Update edge
    async fn update_edge(&self, edge_id: EdgeId, properties: EdgeProperties) -> ServiceResult<bool>;

    /// Delete edge
    async fn delete_edge(&self, edge_id: EdgeId) -> ServiceResult<bool>;

    /// Traverse graph
    async fn traverse_graph(&self, pattern: TraversalPattern) -> ServiceResult<TraversalResult>;

    /// Find shortest path
    async fn find_shortest_path(&self, from: NodeId, to: NodeId) -> ServiceResult<Option<Path>>;

    /// Get graph analytics
    async fn get_graph_analytics(&self, subgraph: Option<Subgraph>) -> ServiceResult<GraphAnalysis>;

    // === Transaction Management ===

    /// Begin transaction
    async fn begin_transaction(&self) -> ServiceResult<TransactionId>;

    /// Commit transaction
    async fn commit_transaction(&self, transaction_id: TransactionId) -> ServiceResult<bool>;

    /// Rollback transaction
    async fn rollback_transaction(&self, transaction_id: TransactionId) -> ServiceResult<bool>;

    /// Get transaction status
    async fn get_transaction_status(&self, transaction_id: TransactionId) -> ServiceResult<TransactionStatus>;

    // === Backup and Restore ===

    /// Create backup
    async fn create_backup(&self, options: BackupOptions) -> ServiceResult<BackupResult>;

    /// Restore from backup
    async fn restore_backup(&self, backup_id: &str, options: RestoreOptions) -> ServiceResult<RestoreResult>;

    /// List backups
    async fn list_backups(&self) -> ServiceResult<Vec<BackupInfo>>;

    /// Delete backup
    async fn delete_backup(&self, backup_id: &str) -> ServiceResult<bool>;

    // === Performance and Monitoring ===

    /// Get database statistics
    async fn get_database_stats(&self, database_name: &str) -> ServiceResult<DatabaseStats>;

    /// Get table statistics
    async fn get_table_stats(&self, table_name: &str) -> ServiceResult<TableStats>;

    /// Analyze query performance
    async fn explain_query(&self, query: QueryPlan) -> ServiceResult<QueryExplanation>;

    /// Get slow queries
    async fn get_slow_queries(&self, options: SlowQueryOptions) -> ServiceResult<Vec<SlowQuery>>;

    /// Optimize database
    async fn optimize_database(&self, database_name: &str, options: OptimizationOptions) -> ServiceResult<OptimizationResult>;

    // === Security and Access Control ===

    /// Create user
    async fn create_user(&self, user: DatabaseUser) -> ServiceResult<String>;

    /// Delete user
    async fn delete_user(&self, username: &str) -> ServiceResult<bool>;

    /// Grant permissions
    async fn grant_permissions(&self, username: &str, permissions: DatabasePermissions) -> ServiceResult<bool>;

    /// Revoke permissions
    async fn revoke_permissions(&self, username: &str, permissions: DatabasePermissions) -> ServiceResult<bool>;

    /// Check user permissions
    async fn check_permissions(&self, username: &str, resource: &str, action: &str) -> ServiceResult<bool>;

    /// List users
    async fn list_users(&self) -> ServiceResult<Vec<DatabaseUser>>;

    // === Advanced Features ===

    /// Execute raw SQL/NoSQL query
    async fn execute_raw_query(&self, query: &str, parameters: Option<Vec<serde_json::Value>>) -> ServiceResult<RawQueryResult>;

    /// Create materialized view
    async fn create_materialized_view(&self, view_def: MaterializedViewDefinition) -> ServiceResult<String>;

    /// Refresh materialized view
    async fn refresh_materialized_view(&self, view_name: &str) -> ServiceResult<bool>;

    /// Create trigger
    async fn create_trigger(&self, trigger_def: TriggerDefinition) -> ServiceResult<String>;

    /// Drop trigger
    async fn drop_trigger(&self, trigger_name: &str) -> ServiceResult<bool>;

    /// List triggers
    async fn list_triggers(&self, table_name: &str) -> ServiceResult<Vec<TriggerDefinition>>;

    /// Enable/disable change data capture
    async fn enable_cdc(&self, database: &str, tables: Vec<String>, options: CdcOptions) -> ServiceResult<CdcStreamInfo>;

    /// Get CDC changes
    async fn get_cdc_changes(&self, stream_id: &str, from_sequence: Option<u64>, limit: Option<u32>) -> ServiceResult<Vec<CdcChangeEvent>>;
}

/// Database connection status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Connecting,
    Error(String),
}

/// Database information
#[derive(Debug, Clone)]
pub struct DatabaseInfo {
    /// Database name
    pub name: String,
    /// Database type
    pub database_type: DatabaseType,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Size in bytes
    pub size_bytes: Option<u64>,
    /// Number of tables/collections
    pub table_count: Option<u32>,
    /// Database status
    pub status: DatabaseStatus,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Database types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatabaseType {
    Relational,
    Document,
    Graph,
    Key,
    TimeSeries,
    Search,
    Hybrid,
}

/// Database status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatabaseStatus {
    Online,
    Offline,
    Maintenance,
    Readonly,
    Error(String),
}

/// Schema change operation
#[derive(Debug, Clone)]
pub enum SchemaChange {
    AddColumn(ColumnDefinition),
    DropColumn(String),
    ModifyColumn(String, ColumnDefinition),
    RenameColumn(String, String),
    AddForeignKey(ForeignKey),
    DropForeignKey(String),
}

/// Query plan for analysis
#[derive(Debug, Clone)]
pub struct QueryPlan {
    /// Query string
    pub query: String,
    /// Query parameters
    pub parameters: Option<Vec<serde_json::Value>>,
    /// Query type
    pub query_type: QueryType,
}

/// Query types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryType {
    Select,
    Insert,
    Update,
    Delete,
    Join,
    Aggregate,
    Explain,
}

/// Query execution explanation
#[derive(Debug, Clone)]
pub struct QueryExplanation {
    /// Query plan details
    pub plan: QueryPlanDetails,
    /// Estimated cost
    pub estimated_cost: f64,
    /// Estimated rows
    pub estimated_rows: Option<u64>,
    /// Execution time in milliseconds
    pub execution_time_ms: Option<u64>,
    /// Index usage information
    pub index_usage: Vec<IndexUsage>,
}

/// Query plan details
#[derive(Debug, Clone)]
pub struct QueryPlanDetails {
    /// Plan operation type
    pub operation: String,
    /// Operation cost
    pub cost: f64,
    /// Expected rows
    pub rows: Option<u64>,
    /// Sub-plans
    pub subplans: Vec<QueryPlanDetails>,
    /// Additional details
    pub details: HashMap<String, serde_json::Value>,
}

/// Index usage information
#[derive(Debug, Clone)]
pub struct IndexUsage {
    /// Index name
    pub index_name: String,
    /// Table name
    pub table_name: String,
    /// Whether index was used
    pub used: bool,
    /// Usage reason
    pub reason: Option<String>,
}

/// Slow query information
#[derive(Debug, Clone)]
pub struct SlowQuery {
    /// Query string
    pub query: String,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Execution timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Database name
    pub database: String,
    /// User who executed the query
    pub user: String,
    /// Query parameters (if any)
    pub parameters: Option<Vec<serde_json::Value>>,
    /// Additional metrics
    pub metrics: SlowQueryMetrics,
}

/// Slow query metrics
#[derive(Debug, Clone)]
pub struct SlowQueryMetrics {
    /// Rows examined
    pub rows_examined: u64,
    /// Rows returned
    pub rows_returned: u64,
    /// Temporary tables created
    pub temp_tables: u32,
    /// Files sorted
    pub files_sorted: u32,
    /// Index usage
    pub index_used: Option<String>,
}

/// Slow query options
#[derive(Debug, Clone)]
pub struct SlowQueryOptions {
    /// Minimum execution time in milliseconds
    pub min_execution_time_ms: u64,
    /// Time range start
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Time range end
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Maximum number of results
    pub limit: Option<u32>,
    /// Filter by database
    pub database_filter: Option<String>,
    /// Filter by user
    pub user_filter: Option<String>,
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    /// Database name
    pub database_name: String,
    /// Total size in bytes
    pub total_size_bytes: u64,
    /// Number of tables
    pub table_count: u32,
    /// Number of indexes
    pub index_count: u32,
    /// Number of records
    pub record_count: u64,
    /// Connections count
    pub connection_count: u32,
    /// Queries per second
    pub queries_per_second: f64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Cache hit rate
    pub cache_hit_rate: f64,
    /// Collection timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Table statistics
#[derive(Debug, Clone)]
pub struct TableStats {
    /// Table name
    pub table_name: String,
    /// Number of rows
    pub row_count: u64,
    /// Table size in bytes
    pub size_bytes: u64,
    /// Index size in bytes
    pub index_size_bytes: u64,
    /// Average row size in bytes
    pub avg_row_size_bytes: f64,
    /// Number of indexes
    pub index_count: u32,
    /// Last update timestamp
    pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
    /// Auto-increment value (if applicable)
    pub auto_increment_value: Option<u64>,
}

/// Optimization options
#[derive(Debug, Clone)]
pub struct OptimizationOptions {
    /// Whether to rebuild indexes
    pub rebuild_indexes: bool,
    /// Whether to update statistics
    pub update_statistics: bool,
    /// Whether to vacuum the database
    pub vacuum: bool,
    /// Target table (if optimizing specific table)
    pub target_table: Option<String>,
    /// Optimization level
    pub optimization_level: OptimizationLevel,
}

/// Optimization levels
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptimizationLevel {
    Light,
    Medium,
    Aggressive,
}

/// Optimization result
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    /// Whether optimization was successful
    pub success: bool,
    /// Optimization duration in milliseconds
    pub duration_ms: u64,
    /// Space freed in bytes
    pub space_freed_bytes: Option<u64>,
    /// Performance improvement percentage
    pub performance_improvement_percent: Option<f32>,
    /// Operations performed
    pub operations_performed: Vec<String>,
    /// Warnings or errors
    pub messages: Vec<String>,
}

/// Backup options
#[derive(Debug, Clone)]
pub struct BackupOptions {
    /// Backup type
    pub backup_type: BackupType,
    /// Databases to include (empty for all)
    pub databases: Vec<String>,
    /// Whether to compress backup
    pub compress: bool,
    /// Backup encryption key (optional)
    pub encryption_key: Option<String>,
    /// Include indexes
    pub include_indexes: bool,
    /// Include triggers
    pub include_triggers: bool,
    /// Maximum backup size in bytes (optional)
    pub max_size_bytes: Option<u64>,
}

/// Backup types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupType {
    Full,
    Incremental,
    Differential,
}

/// Backup result
#[derive(Debug, Clone)]
pub struct BackupResult {
    /// Backup ID
    pub backup_id: String,
    /// Backup filename/path
    pub backup_path: String,
    /// Backup size in bytes
    pub size_bytes: u64,
    /// Backup duration in milliseconds
    pub duration_ms: u64,
    /// Number of records backed up
    pub records_count: u64,
    /// Whether backup was successful
    pub success: bool,
    /// Backup checksum
    pub checksum: Option<String>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Backup information
#[derive(Debug, Clone)]
pub struct BackupInfo {
    /// Backup ID
    pub backup_id: String,
    /// Backup type
    pub backup_type: BackupType,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Backup size in bytes
    pub size_bytes: u64,
    /// Databases included
    pub databases: Vec<String>,
    /// Backup status
    pub status: BackupStatus,
    /// Checksum
    pub checksum: Option<String>,
    /// Retention policy
    pub retention_policy: Option<RetentionPolicy>,
}

/// Backup status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupStatus {
    Creating,
    Completed,
    Failed,
    Corrupted,
    Expired,
}

/// Retention policy
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// Retention duration in days
    pub retention_days: u32,
    /// Maximum number of backups to keep
    pub max_backups: Option<u32>,
}

/// Restore options
#[derive(Debug, Clone)]
pub struct RestoreOptions {
    /// Target database names (optional)
    pub target_databases: Option<HashMap<String, String>>,
    /// Whether to drop existing databases
    pub drop_existing: bool,
    /// Whether to restore indexes
    pub restore_indexes: bool,
    /// Whether to restore triggers
    pub restore_triggers: bool,
    /// Point-in-time restore timestamp (optional)
    pub point_in_time: Option<chrono::DateTime<chrono::Utc>>,
}

/// Restore result
#[derive(Debug, Clone)]
pub struct RestoreResult {
    /// Whether restore was successful
    pub success: bool,
    /// Restore duration in milliseconds
    pub duration_ms: u64,
    /// Number of records restored
    pub records_restored: u64,
    /// Databases restored
    pub databases_restored: Vec<String>,
    /// Warnings or errors
    pub messages: Vec<String>,
}

/// Database user
#[derive(Debug, Clone)]
pub struct DatabaseUser {
    /// Username
    pub username: String,
    /// User roles
    pub roles: Vec<String>,
    /// User status
    pub status: UserStatus,
    /// Created timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last login timestamp
    pub last_login: Option<chrono::DateTime<chrono::Utc>>,
    /// Password policy
    pub password_policy: Option<PasswordPolicy>,
}

/// User status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserStatus {
    Active,
    Inactive,
    Locked,
    Expired,
}

/// Password policy
#[derive(Debug, Clone)]
pub struct PasswordPolicy {
    /// Minimum password length
    pub min_length: u32,
    /// Require uppercase letters
    pub require_uppercase: bool,
    /// Require lowercase letters
    pub require_lowercase: bool,
    /// Require numbers
    pub require_numbers: bool,
    /// Require special characters
    pub require_special: bool,
    /// Password expiration in days
    pub expiration_days: Option<u32>,
}

/// Database permissions
#[derive(Debug, Clone)]
pub struct DatabasePermissions {
    /// Database name
    pub database: String,
    /// Table permissions
    pub table_permissions: HashMap<String, Vec<TablePermission>>,
    /// Global permissions
    pub global_permissions: Vec<GlobalPermission>,
}

/// Table permissions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TablePermission {
    Select,
    Insert,
    Update,
    Delete,
    Create,
    Drop,
    Alter,
    Index,
    References,
}

/// Global permissions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalPermission {
    CreateDatabase,
    DropDatabase,
    CreateUser,
    DropUser,
    Grant,
    Revoke,
    Backup,
    Restore,
}

/// Raw query result
#[derive(Debug, Clone)]
pub struct RawQueryResult {
    /// Whether query was successful
    pub success: bool,
    /// Number of affected rows
    pub affected_rows: u64,
    /// Query results (if SELECT)
    pub results: Option<Vec<HashMap<String, serde_json::Value>>>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Warning messages
    pub warnings: Vec<String>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Materialized view definition
#[derive(Debug, Clone)]
pub struct MaterializedViewDefinition {
    /// View name
    pub name: String,
    /// View query
    pub query: String,
    /// Refresh schedule (cron expression)
    pub refresh_schedule: Option<String>,
    /// Whether to refresh immediately
    pub refresh_immediately: bool,
    /// Additional options
    pub options: HashMap<String, serde_json::Value>,
}

/// Trigger definition
#[derive(Debug, Clone)]
pub struct TriggerDefinition {
    /// Trigger name
    pub name: String,
    /// Table name
    pub table_name: String,
    /// Trigger event
    pub event: TriggerEvent,
    /// Trigger timing
    pub timing: TriggerTiming,
    /// Trigger function/procedure
    pub trigger_function: String,
    /// Whether trigger is enabled
    pub enabled: bool,
    /// Additional options
    pub options: HashMap<String, serde_json::Value>,
}

/// Trigger events
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerEvent {
    Insert,
    Update,
    Delete,
}

/// Trigger timing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerTiming {
    Before,
    After,
    InsteadOf,
}

/// Change Data Capture options
#[derive(Debug, Clone)]
pub struct CdcOptions {
    /// Capture mode
    pub capture_mode: CdcCaptureMode,
    /// Include old values
    pub include_old_values: bool,
    /// Include new values
    pub include_new_values: bool,
    /// Filter changes
    pub filter: Option<CdcFilter>,
    /// Retention period in days
    pub retention_days: u32,
}

/// CDC capture modes
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdcCaptureMode {
    AllChanges,
    BeforeImages,
    AfterImages,
}

/// CDC filter
#[derive(Debug, Clone)]
pub struct CdcFilter {
    /// Tables to include
    pub include_tables: Vec<String>,
    /// Tables to exclude
    pub exclude_tables: Vec<String>,
    /// Operations to include
    pub include_operations: Vec<CdcOperation>,
    /// Custom filter expression
    pub custom_filter: Option<String>,
}

/// CDC operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdcOperation {
    Insert,
    Update,
    Delete,
}

/// CDC stream information
#[derive(Debug, Clone)]
pub struct CdcStreamInfo {
    /// Stream ID
    pub stream_id: String,
    /// Stream status
    pub status: CdcStreamStatus,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last processed sequence
    pub last_sequence: Option<u64>,
    /// Configuration
    pub config: CdcOptions,
}

/// CDC stream status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdcStreamStatus {
    Active,
    Paused,
    Stopped,
    Error(String),
}

/// CDC change event
#[derive(Debug, Clone)]
pub struct CdcChangeEvent {
    /// Event sequence number
    pub sequence: u64,
    /// Event timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Operation type
    pub operation: CdcOperation,
    /// Database name
    pub database: String,
    /// Table name
    pub table: String,
    /// Primary key
    pub primary_key: serde_json::Value,
    /// Old values (before image)
    pub old_values: Option<HashMap<String, serde_json::Value>>,
    /// New values (after image)
    pub new_values: Option<HashMap<String, serde_json::Value>>,
    /// Changed columns
    pub changed_columns: Option<Vec<String>>,
}

/// Transaction status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    Active,
    Committed,
    RolledBack,
    Prepared,
    Failed,
}