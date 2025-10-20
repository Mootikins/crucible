use async_trait::async_trait;
use crate::errors::{ServiceError, ServiceResult};
use crate::types::{ServiceInfo, ServiceHealth, ServiceMetrics, ServiceDependency};
use std::collections::HashMap;
use uuid::Uuid;
use super::BaseService;

/// Trait for configuration management services
#[async_trait]
pub trait ConfigService: BaseService + Send + Sync {
    // === Configuration Storage ===

    /// Store configuration value
    async fn set_config(&self, key: &str, value: serde_json::Value, options: ConfigOptions) -> ServiceResult<()>;

    /// Get configuration value
    async fn get_config(&self, key: &str) -> ServiceResult<Option<serde_json::Value>>;

    /// Get configuration with default value
    async fn get_config_with_default(&self, key: &str, default: serde_json::Value) -> ServiceResult<serde_json::Value>;

    /// Delete configuration key
    async fn delete_config(&self, key: &str) -> ServiceResult<bool>;

    /// Check if configuration key exists
    async fn config_exists(&self, key: &str) -> ServiceResult<bool>;

    // === Bulk Operations ===

    /// Set multiple configuration values
    async fn set_configs(&self, configs: HashMap<String, serde_json::Value>, options: ConfigOptions) -> ServiceResult<()>;

    /// Get multiple configuration values
    async fn get_configs(&self, keys: Vec<String>) -> ServiceResult<HashMap<String, Option<serde_json::Value>>>;

    /// Get all configurations matching a pattern
    async fn get_configs_by_pattern(&self, pattern: &str) -> ServiceResult<HashMap<String, serde_json::Value>>;

    /// Delete multiple configuration keys
    async fn delete_configs(&self, keys: Vec<String>) -> ServiceResult<HashMap<String, bool>>;

    // === Configuration Namespaces ===

    /// List all namespaces
    async fn list_namespaces(&self) -> ServiceResult<Vec<String>>;

    /// Create namespace
    async fn create_namespace(&self, namespace: &str, options: NamespaceOptions) -> ServiceResult<()>;

    /// Delete namespace
    async fn delete_namespace(&self, namespace: &str, force: bool) -> ServiceResult<bool>;

    /// Get namespace information
    async fn get_namespace_info(&self, namespace: &str) -> ServiceResult<Option<NamespaceInfo>>;

    // === Configuration Schemas ===

    /// Register configuration schema
    async fn register_schema(&self, schema: ConfigSchema) -> ServiceResult<()>;

    /// Get configuration schema
    async fn get_schema(&self, schema_id: &str) -> ServiceResult<Option<ConfigSchema>>;

    /// Validate configuration against schema
    async fn validate_config(&self, key: &str, value: &serde_json::Value, schema_id: &str) -> ServiceResult<ValidationResult>;

    /// List available schemas
    async fn list_schemas(&self) -> ServiceResult<Vec<ConfigSchema>>;

    // === Configuration Watchers ===

    /// Watch for configuration changes
    async fn watch_config(&self, key: &str, options: WatchOptions) -> ServiceResult<Box<dyn ConfigWatcher>>;

    /// Watch namespace changes
    async fn watch_namespace(&self, namespace: &str, options: WatchOptions) -> ServiceResult<Box<dyn ConfigWatcher>>;

    // === Configuration Versioning ===

    /// Get configuration history
    async fn get_config_history(&self, key: &str, options: HistoryOptions) -> ServiceResult<Vec<ConfigVersion>>;

    /// Get specific configuration version
    async fn get_config_version(&self, key: &str, version: u64) -> ServiceResult<Option<ConfigVersion>>;

    /// Rollback configuration to previous version
    async fn rollback_config(&self, key: &str, version: u64) -> ServiceResult<bool>;

    // === Configuration Templates ===

    /// Create configuration template
    async fn create_template(&self, template: ConfigTemplate) -> ServiceResult<String>;

    /// Apply configuration template
    async fn apply_template(&self, template_id: &str, variables: HashMap<String, String>, options: TemplateOptions) -> ServiceResult<bool>;

    /// Get template information
    async fn get_template(&self, template_id: &str) -> ServiceResult<Option<ConfigTemplate>>;

    /// List available templates
    async fn list_templates(&self) -> ServiceResult<Vec<ConfigTemplate>>;

    // === Configuration Import/Export ===

    /// Export configuration
    async fn export_config(&self, options: ExportOptions) -> ServiceResult<ConfigExport>;

    /// Import configuration
    async fn import_config(&self, config_export: ConfigExport, options: ImportOptions) -> ServiceResult<ImportResult>;

    // === Configuration Encryption ===

    /// Encrypt configuration value
    async fn encrypt_config(&self, key: &str, value: serde_json::Value, encryption_key: &str) -> ServiceResult<()>;

    /// Decrypt configuration value
    async fn decrypt_config(&self, key: &str, encryption_key: &str) -> ServiceResult<Option<serde_json::Value>>;

    /// Rotate encryption keys
    async fn rotate_encryption_keys(&self, old_key: &str, new_key: &str, namespace: Option<&str>) -> ServiceResult<KeyRotationResult>;

    // === Configuration Access Control ===

    /// Set configuration permissions
    async fn set_permissions(&self, key: &str, permissions: ConfigPermissions) -> ServiceResult<()>;

    /// Get configuration permissions
    async fn get_permissions(&self, key: &str) -> ServiceResult<Option<ConfigPermissions>>;

    /// Check access permissions
    async fn check_access(&self, key: &str, user: &str, action: ConfigAction) -> ServiceResult<bool>;

    // === Configuration Caching ===

    /// Warm up configuration cache
    async fn warm_cache(&self, keys: Vec<String>) -> ServiceResult<CacheWarmupResult>;

    /// Invalidate cache entries
    async fn invalidate_cache(&self, keys: Vec<String>) -> ServiceResult<bool>;

    /// Get cache statistics
    async fn get_cache_stats(&self) -> ServiceResult<ConfigCacheStats>;

    // === Configuration Backups ===

    /// Create configuration backup
    async fn create_backup(&self, options: BackupOptions) -> ServiceResult<ConfigBackup>;

    /// Restore configuration from backup
    async fn restore_backup(&self, backup_id: &str, options: RestoreOptions) -> ServiceResult<RestoreResult>;

    /// List configuration backups
    async fn list_backups(&self) -> ServiceResult<Vec<ConfigBackup>>;

    // === Configuration Search ===

    /// Search configuration
    async fn search_config(&self, query: ConfigSearchQuery) -> ServiceResult<Vec<ConfigSearchResult>>;

    // === Configuration Metrics ===

    /// Get configuration metrics
    async fn get_config_metrics(&self) -> ServiceResult<ConfigMetrics>;

    /// Get usage statistics
    async fn get_usage_stats(&self, options: UsageStatsOptions) -> ServiceResult<ConfigUsageStats>;
}

/// Configuration options
#[derive(Debug, Clone)]
pub struct ConfigOptions {
    /// Configuration namespace
    pub namespace: Option<String>,
    /// Whether to encrypt the value
    pub encrypt: bool,
    /// TTL in seconds (None for permanent)
    pub ttl_seconds: Option<u64>,
    /// Configuration version
    pub version: Option<u64>,
    /// User who made the change
    pub user: Option<String>,
    /// Change description
    pub description: Option<String>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl Default for ConfigOptions {
    fn default() -> Self {
        Self {
            namespace: None,
            encrypt: false,
            ttl_seconds: None,
            version: None,
            user: None,
            description: None,
            metadata: HashMap::new(),
        }
    }
}

/// Namespace options
#[derive(Debug, Clone)]
pub struct NamespaceOptions {
    /// Namespace description
    pub description: Option<String>,
    /// Default encryption settings
    pub default_encrypt: bool,
    /// Namespace permissions
    pub permissions: Option<ConfigPermissions>,
    /// Retention policy
    pub retention_policy: Option<RetentionPolicy>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Namespace information
#[derive(Debug, Clone)]
pub struct NamespaceInfo {
    /// Namespace name
    pub name: String,
    /// Namespace description
    pub description: Option<String>,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last modified timestamp
    pub last_modified: chrono::DateTime<chrono::Utc>,
    /// Number of configuration entries
    pub entry_count: u32,
    /// Namespace size in bytes
    pub size_bytes: u64,
    /// Default encryption
    pub default_encrypt: bool,
    /// Permissions
    pub permissions: Option<ConfigPermissions>,
    /// Retention policy
    pub retention_policy: Option<RetentionPolicy>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Configuration schema
#[derive(Debug, Clone)]
pub struct ConfigSchema {
    /// Schema ID
    pub schema_id: String,
    /// Schema name
    pub name: String,
    /// Schema description
    pub description: Option<String>,
    /// JSON schema definition
    pub schema: serde_json::Value,
    /// Schema version
    pub version: String,
    /// Validation rules
    pub validation_rules: Vec<ValidationRule>,
    /// Schema namespace
    pub namespace: Option<String>,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last modified timestamp
    pub last_modified: chrono::DateTime<chrono::Utc>,
}

/// Validation rule
#[derive(Debug, Clone)]
pub struct ValidationRule {
    /// Rule name
    pub name: String,
    /// Rule type
    pub rule_type: ValidationRuleType,
    /// Rule parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Error message template
    pub error_message: String,
    /// Rule severity
    pub severity: ValidationSeverity,
}

/// Validation rule types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationRuleType {
    /// Range validation
    Range,
    /// Pattern validation
    Pattern,
    /// Length validation
    Length,
    /// Required field validation
    Required,
    /// Custom validation
    Custom,
}

/// Validation severity
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationSeverity {
    Error,
    Warning,
    Info,
}

/// Validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed
    pub valid: bool,
    /// Validation errors
    pub errors: Vec<ValidationError>,
    /// Validation warnings
    pub warnings: Vec<ValidationError>,
    /// Normalized value
    pub normalized_value: Option<serde_json::Value>,
}

/// Validation error
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Error message
    pub message: String,
    /// Error path (JSON pointer)
    pub path: String,
    /// Rule that failed
    pub rule: String,
    /// Error severity
    pub severity: ValidationSeverity,
    /// Invalid value
    pub value: serde_json::Value,
}

/// Watch options
#[derive(Debug, Clone)]
pub struct WatchOptions {
    /// Include current value in initial event
    pub include_current: bool,
    /// Filter changes by user
    pub filter_by_user: Option<String>,
    /// Minimum event version to start from
    pub from_version: Option<u64>,
    /// Watch timeout in seconds
    pub timeout_seconds: Option<u64>,
}

/// Trait for configuration watchers
#[async_trait]
pub trait ConfigWatcher: Send + Sync {
    /// Get next configuration change event
    async fn next_change(&mut self) -> ServiceResult<ConfigChangeEvent>;

    /// Check if watcher is still active
    fn is_active(&self) -> bool;

    /// Stop watching
    async fn stop(&mut self) -> ServiceResult<()>;
}

/// Configuration change event
#[derive(Debug, Clone)]
pub struct ConfigChangeEvent {
    /// Event type
    pub event_type: ChangeEventType,
    /// Configuration key
    pub key: String,
    /// Previous value (for updates)
    pub previous_value: Option<serde_json::Value>,
    /// New value (for creates and updates)
    pub new_value: Option<serde_json::Value>,
    /// Event version
    pub version: u64,
    /// Event timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// User who made the change
    pub user: Option<String>,
    /// Change description
    pub description: Option<String>,
}

/// Change event types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeEventType {
    Created,
    Updated,
    Deleted,
}

/// History options
#[derive(Debug, Clone)]
pub struct HistoryOptions {
    /// Maximum number of versions to return
    pub limit: Option<u32>,
    /// Start from specific version
    pub from_version: Option<u64>,
    /// End at specific version
    pub to_version: Option<u64>,
    /// Filter by user
    pub filter_by_user: Option<String>,
    /// Sort order
    pub sort_order: SortOrder,
}

/// Sort order
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

/// Configuration version
#[derive(Debug, Clone)]
pub struct ConfigVersion {
    /// Version number
    pub version: u64,
    /// Configuration value
    pub value: serde_json::Value,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// User who created this version
    pub user: Option<String>,
    /// Change description
    pub description: Option<String>,
    /// Version size in bytes
    pub size_bytes: u64,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Configuration template
#[derive(Debug, Clone)]
pub struct ConfigTemplate {
    /// Template ID
    pub template_id: String,
    /// Template name
    pub name: String,
    /// Template description
    pub description: Option<String>,
    /// Template content with placeholders
    pub content: HashMap<String, String>,
    /// Template variables definition
    pub variables: HashMap<String, TemplateVariable>,
    /// Template version
    pub version: String,
    /// Template namespace
    pub namespace: Option<String>,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last modified timestamp
    pub last_modified: chrono::DateTime<chrono::Utc>,
}

/// Template variable
#[derive(Debug, Clone)]
pub struct TemplateVariable {
    /// Variable name
    pub name: String,
    /// Variable description
    pub description: Option<String>,
    /// Variable type
    pub var_type: TemplateVariableType,
    /// Default value
    pub default_value: Option<String>,
    /// Whether variable is required
    pub required: bool,
    /// Validation pattern
    pub validation_pattern: Option<String>,
}

/// Template variable types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateVariableType {
    String,
    Number,
    Boolean,
    Json,
    Secret,
}

/// Template options
#[derive(Debug, Clone)]
pub struct TemplateOptions {
    /// Target namespace
    pub namespace: Option<String>,
    /// Whether to overwrite existing values
    pub overwrite: bool,
    /// Dry run (don't actually apply)
    pub dry_run: bool,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Export options
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Namespace to export (None for all)
    pub namespace: Option<String>,
    /// Export format
    pub format: ExportFormat,
    /// Include metadata
    pub include_metadata: bool,
    /// Include history
    pub include_history: bool,
    /// Filter keys by pattern
    pub key_pattern: Option<String>,
    /// Encryption
    pub encrypt: bool,
    /// Compression
    pub compress: bool,
}

/// Export formats
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Yaml,
    Toml,
    Env,
}

/// Configuration export
#[derive(Debug, Clone)]
pub struct ConfigExport {
    /// Export ID
    pub export_id: String,
    /// Export format
    pub format: ExportFormat,
    /// Exported data
    pub data: Vec<u8>,
    /// Export metadata
    pub metadata: ExportMetadata,
    /// Export timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Export size in bytes
    pub size_bytes: u64,
    /// Number of configuration entries
    pub entry_count: u32,
}

/// Export metadata
#[derive(Debug, Clone)]
pub struct ExportMetadata {
    /// Source namespace
    pub namespace: Option<String>,
    /// Export version
    pub version: String,
    /// Export tool/version
    pub exported_by: String,
    /// Checksum
    pub checksum: String,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Import options
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// Target namespace
    pub namespace: Option<String>,
    /// Whether to overwrite existing values
    pub overwrite: bool,
    /// Whether to create missing namespaces
    pub create_namespaces: bool,
    /// Whether to validate against schemas
    pub validate_schemas: bool,
    /// Whether to preserve timestamps
    pub preserve_timestamps: bool,
    /// Dry run (don't actually import)
    pub dry_run: bool,
}

/// Import result
#[derive(Debug, Clone)]
pub struct ImportResult {
    /// Whether import was successful
    pub success: bool,
    /// Number of entries processed
    pub entries_processed: u32,
    /// Number of entries imported
    pub entries_imported: u32,
    /// Number of entries skipped
    pub entries_skipped: u32,
    /// Number of entries failed
    pub entries_failed: u32,
    /// Import errors
    pub errors: Vec<ImportError>,
    /// Import warnings
    pub warnings: Vec<String>,
    /// Import duration in milliseconds
    pub duration_ms: u64,
}

/// Import error
#[derive(Debug, Clone)]
pub struct ImportError {
    /// Configuration key
    pub key: String,
    /// Error message
    pub error: String,
    /// Error type
    pub error_type: ImportErrorType,
}

/// Import error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportErrorType {
    /// Invalid format
    InvalidFormat,
    /// Validation failed
    ValidationFailed,
    /// Permission denied
    PermissionDenied,
    /// Key conflict
    KeyConflict,
    /// Namespace error
    NamespaceError,
    /// Encryption error
    EncryptionError,
}

/// Key rotation result
#[derive(Debug, Clone)]
pub struct KeyRotationResult {
    /// Number of keys rotated
    pub keys_rotated: u32,
    /// Number of keys failed
    pub keys_failed: u32,
    /// Rotation errors
    pub errors: Vec<String>,
    /// Rotation duration in milliseconds
    pub duration_ms: u64,
}

/// Configuration permissions
#[derive(Debug, Clone)]
pub struct ConfigPermissions {
    /// Owner user
    pub owner: String,
    /// Read permissions
    pub read: Vec<String>,
    /// Write permissions
    pub write: Vec<String>,
    /// Delete permissions
    pub delete: Vec<String>,
    /// Admin permissions
    pub admin: Vec<String>,
    /// Public read access
    pub public_read: bool,
    /// Permissions timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Configuration actions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigAction {
    Read,
    Write,
    Delete,
    Admin,
}

/// Retention policy
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// Retention duration in days
    pub retention_days: u32,
    /// Maximum number of versions to keep
    pub max_versions: Option<u32>,
    /// Whether to apply to all keys in namespace
    pub apply_to_all: bool,
}

/// Cache warmup result
#[derive(Debug, Clone)]
pub struct CacheWarmupResult {
    /// Number of keys warmed up
    pub keys_warmed_up: u32,
    /// Number of keys failed
    pub keys_failed: u32,
    /// Warmup duration in milliseconds
    pub duration_ms: u64,
    /// Warmup errors
    pub errors: Vec<String>,
}

/// Configuration cache statistics
#[derive(Debug, Clone)]
pub struct ConfigCacheStats {
    /// Cache size in bytes
    pub cache_size_bytes: u64,
    /// Number of cached items
    pub cached_items: u32,
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Hit rate
    pub hit_rate: f64,
    /// Evictions
    pub evictions: u64,
    /// Last updated timestamp
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// Backup options
#[derive(Debug, Clone)]
pub struct BackupOptions {
    /// Namespace to backup (None for all)
    pub namespace: Option<String>,
    /// Include history
    pub include_history: bool,
    /// Include metadata
    pub include_metadata: bool,
    /// Compression
    pub compress: bool,
    /// Encryption
    pub encrypt: bool,
    /// Backup description
    pub description: Option<String>,
}

/// Configuration backup
#[derive(Debug, Clone)]
pub struct ConfigBackup {
    /// Backup ID
    pub backup_id: String,
    /// Backup timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Backup size in bytes
    pub size_bytes: u64,
    /// Number of entries
    pub entry_count: u32,
    /// Backup description
    pub description: Option<String>,
    /// Backup status
    pub status: BackupStatus,
    /// Checksum
    pub checksum: Option<String>,
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

/// Restore options
#[derive(Debug, Clone)]
pub struct RestoreOptions {
    /// Target namespace
    pub namespace: Option<String>,
    /// Whether to overwrite existing values
    pub overwrite: bool,
    /// Whether to restore history
    pub restore_history: bool,
    /// Whether to restore metadata
    pub restore_metadata: bool,
    /// Dry run (don't actually restore)
    pub dry_run: bool,
}

/// Restore result
#[derive(Debug, Clone)]
pub struct RestoreResult {
    /// Whether restore was successful
    pub success: bool,
    /// Number of entries restored
    pub entries_restored: u32,
    /// Number of entries skipped
    pub entries_skipped: u32,
    /// Number of entries failed
    pub entries_failed: u32,
    /// Restore duration in milliseconds
    pub duration_ms: u64,
    /// Restore errors
    pub errors: Vec<String>,
    /// Restore warnings
    pub warnings: Vec<String>,
}

/// Configuration search query
#[derive(Debug, Clone)]
pub struct ConfigSearchQuery {
    /// Search query string
    pub query: String,
    /// Namespace to search (None for all)
    pub namespace: Option<String>,
    /// Search fields
    pub search_fields: Vec<SearchField>,
    /// Filter by key pattern
    pub key_pattern: Option<String>,
    /// Filter by value type
    pub value_type: Option<SearchValueType>,
    /// Filter by user
    pub filter_by_user: Option<String>,
    /// Filter by date range
    pub date_range: Option<DateRange>,
    /// Maximum results
    pub limit: Option<u32>,
    /// Sort results
    pub sort_by: Option<SearchSortBy>,
}

/// Search fields
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchField {
    Key,
    Value,
    Description,
    Metadata,
    All,
}

/// Search value types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchValueType {
    String,
    Number,
    Boolean,
    Object,
    Array,
    Null,
}

/// Date range
#[derive(Debug, Clone)]
pub struct DateRange {
    /// Start date
    pub start: chrono::DateTime<chrono::Utc>,
    /// End date
    pub end: chrono::DateTime<chrono::Utc>,
}

/// Search sort options
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchSortBy {
    Key,
    Value,
    CreatedAt,
    UpdatedAt,
    Relevance,
}

/// Configuration search result
#[derive(Debug, Clone)]
pub struct ConfigSearchResult {
    /// Configuration key
    pub key: String,
    /// Configuration value
    pub value: serde_json::Value,
    /// Search relevance score
    pub relevance_score: f32,
    /// Matched fields
    pub matched_fields: Vec<SearchField>,
    /// Match highlights
    pub highlights: HashMap<String, Vec<String>>,
    /// Configuration metadata
    pub metadata: HashMap<String, String>,
}

/// Configuration metrics
#[derive(Debug, Clone)]
pub struct ConfigMetrics {
    /// Total number of configuration entries
    pub total_entries: u64,
    /// Number of namespaces
    pub namespace_count: u32,
    /// Total storage size in bytes
    pub total_size_bytes: u64,
    /// Average entry size in bytes
    pub avg_entry_size_bytes: f64,
    /// Number of read operations
    pub read_ops: u64,
    /// Number of write operations
    pub write_ops: u64,
    /// Cache hit rate
    pub cache_hit_rate: f64,
    /// Number of active watchers
    pub active_watchers: u32,
    /// Metrics collection timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Usage statistics options
#[derive(Debug, Clone)]
pub struct UsageStatsOptions {
    /// Namespace to analyze (None for all)
    pub namespace: Option<String>,
    /// Time range
    pub time_range: DateRange,
    /// Group by field
    pub group_by: Option<UsageGroupBy>,
    /// Filter by user
    pub filter_by_user: Option<String>,
}

/// Usage group by options
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageGroupBy {
    User,
    Namespace,
    Key,
    Hour,
    Day,
    Week,
    Month,
}

/// Configuration usage statistics
#[derive(Debug, Clone)]
pub struct ConfigUsageStats {
    /// Statistics period
    pub period: DateRange,
    /// Total operations
    pub total_operations: u64,
    /// Read operations
    pub read_operations: u64,
    /// Write operations
    pub write_operations: u64,
    /// Delete operations
    pub delete_operations: u64,
    /// Unique users
    pub unique_users: u32,
    /// Most accessed keys
    pub top_keys: Vec<KeyUsageStats>,
    /// Most active users
    pub top_users: Vec<UserUsageStats>,
    /// Operations grouped by specified field
    pub grouped_stats: HashMap<String, GroupedUsageStats>,
}

/// Key usage statistics
#[derive(Debug, Clone)]
pub struct KeyUsageStats {
    /// Key name
    pub key: String,
    /// Access count
    pub access_count: u64,
    /// Last access timestamp
    pub last_access: chrono::DateTime<chrono::Utc>,
    /// Unique users who accessed this key
    pub unique_users: u32,
}

/// User usage statistics
#[derive(Debug, Clone)]
pub struct UserUsageStats {
    /// User identifier
    pub user: String,
    /// Operation count
    pub operation_count: u64,
    /// Last activity timestamp
    pub last_activity: chrono::DateTime<chrono::Utc>,
    /// Unique keys accessed
    pub unique_keys: u32,
}

/// Grouped usage statistics
#[derive(Debug, Clone)]
pub struct GroupedUsageStats {
    /// Group identifier
    pub group_id: String,
    /// Operation count
    pub operation_count: u64,
    /// Read operations
    pub read_operations: u64,
    /// Write operations
    pub write_operations: u64,
    /// Delete operations
    pub delete_operations: u64,
}