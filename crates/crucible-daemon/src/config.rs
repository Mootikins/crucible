//! Daemon configuration management
//!
//! Handles configuration loading, validation, and runtime updates for the data layer daemon.

use anyhow::Result;
use crucible_config::ConfigError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Daemon configuration for data layer coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Filesystem watching configuration
    pub filesystem: FilesystemConfig,
    /// Database synchronization configuration
    pub database: DaemonDatabaseConfig,
    /// Performance configuration
    pub performance: PerformanceConfig,
    /// Health monitoring configuration
    pub health: HealthConfig,
    /// Service integration configuration
    pub services: ServicesConfig,
}

/// Filesystem watching configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemConfig {
    /// Paths to watch
    pub watch_paths: Vec<WatchPath>,
    /// Watching backend to use
    pub backend: WatchBackend,
    /// Debouncing configuration
    pub debounce: DebounceConfig,
    /// Event filtering
    pub filters: Vec<FilterRule>,
    /// File parsing configuration
    pub parsing: ParsingConfig,
}

/// Path to watch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchPath {
    /// Path to watch
    pub path: PathBuf,
    /// Whether to watch recursively
    pub recursive: bool,
    /// Watch mode
    pub mode: WatchMode,
    /// Path-specific filters (overrides global filters)
    pub filters: Option<Vec<FilterRule>>,
    /// Path-specific events to watch
    pub events: Option<Vec<WatchFilesystemEventType>>,
}

/// Filesystem watch backends
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WatchBackend {
    /// Use OS-specific file notifications (recommended)
    Notify,
    /// Use polling (fallback)
    Polling,
    /// Low-frequency watching for editor integration
    Editor,
}

/// Watch mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WatchMode {
    /// Watch all events
    All,
    /// Watch only file modifications
    Modifications,
    /// Watch only file creation/deletion
    CreateDelete,
    /// Custom event set
    Custom(Vec<WatchFilesystemEventType>),
}

/// Filesystem event types to watch
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WatchFilesystemEventType {
    Created,
    Modified,
    Deleted,
    Renamed,
    PermissionChanged,
    MetadataChanged,
}

/// Debouncing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebounceConfig {
    /// Debounce delay in milliseconds
    pub delay_ms: u64,
    /// Maximum events to batch
    pub max_batch_size: usize,
    /// Whether to deduplicate events
    pub deduplicate: bool,
    /// Time window for deduplication in milliseconds
    pub dedup_window_ms: Option<u64>,
}

/// Event filtering rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterRule {
    /// Rule name
    pub name: String,
    /// Whether to include or exclude matches
    pub action: FilterAction,
    /// Pattern to match (glob pattern)
    pub pattern: String,
    /// File size filter
    pub size_filter: Option<SizeFilter>,
    /// MIME type filter
    pub mime_filter: Option<Vec<String>>,
}

/// Filter action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FilterAction {
    Include,
    Exclude,
}

/// Size filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeFilter {
    /// Minimum file size in bytes
    pub min_size: Option<u64>,
    /// Maximum file size in bytes
    pub max_size: Option<u64>,
}

/// File parsing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsingConfig {
    /// File type parsers to enable
    pub parsers: HashMap<String, ParserConfig>,
    /// Metadata extraction settings
    pub metadata: MetadataExtractionConfig,
    /// Content analysis settings
    pub content_analysis: ContentAnalysisConfig,
}

/// Parser configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserConfig {
    /// Parser name
    pub name: String,
    /// File extensions this parser handles
    pub extensions: Vec<String>,
    /// Whether to enable this parser
    pub enabled: bool,
    /// Parser-specific options
    pub options: HashMap<String, serde_json::Value>,
}

/// Metadata extraction configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataExtractionConfig {
    /// Whether to extract file metadata
    pub enabled: bool,
    /// Metadata fields to extract
    pub fields: Vec<String>,
    /// Whether to calculate file checksums
    pub calculate_checksums: bool,
    /// Checksum algorithm to use
    pub checksum_algorithm: ChecksumAlgorithm,
}

/// Checksum algorithms
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChecksumAlgorithm {
    MD5,
    SHA1,
    SHA256,
    SHA512,
}

/// Content analysis configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentAnalysisConfig {
    /// Whether to analyze file content
    pub enabled: bool,
    /// Analysis types to perform
    pub analysis_types: Vec<AnalysisType>,
    /// Maximum file size to analyze (in bytes)
    pub max_file_size: Option<u64>,
    /// Analysis timeout in seconds
    pub timeout_seconds: Option<u64>,
}

/// Content analysis types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AnalysisType {
    /// Detect MIME type
    MimeTypeDetection,
    /// Extract text content
    TextExtraction,
    /// Language detection
    LanguageDetection,
    /// Keyword extraction
    KeywordExtraction,
    /// Named entity recognition
    NamedEntityRecognition,
    /// Topic modeling
    TopicModeling,
}

/// Daemon-specific database synchronization configuration
///
/// This is distinct from crucible-config::DatabaseConfig which is used for
/// simple profile-based configuration. This config adds daemon-specific
/// features like sync strategies, transactions, indexing, and backup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonDatabaseConfig {
    /// Database connection settings
    pub connection: DatabaseConnectionConfig,
    /// Sync strategies
    pub sync_strategies: Vec<SyncStrategy>,
    /// Transaction settings
    pub transactions: TransactionConfig,
    /// Indexing configuration
    pub indexing: IndexingConfig,
    /// Backup configuration
    pub backup: BackupConfig,
}

/// Database connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConnectionConfig {
    /// Database type
    pub database_type: DatabaseType,
    /// Connection string
    pub connection_string: String,
    /// Connection pool settings
    pub pool: ConnectionPoolConfig,
    /// TLS configuration
    pub tls: Option<TlsConfig>,
}

/// Database types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DatabaseType {
    SurrealDB,
    PostgreSQL,
    MySQL,
    SQLite,
    MongoDB,
}

/// Connection pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPoolConfig {
    /// Maximum number of connections
    pub max_connections: u32,
    /// Minimum number of connections
    pub min_connections: u32,
    /// Connection timeout in seconds
    pub timeout_seconds: u64,
    /// Idle timeout in seconds
    pub idle_timeout_seconds: u64,
    /// Maximum lifetime of connections in seconds
    pub max_lifetime_seconds: Option<u64>,
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Whether to enable TLS
    pub enabled: bool,
    /// Path to CA certificate
    pub ca_cert_path: Option<PathBuf>,
    /// Path to client certificate
    pub client_cert_path: Option<PathBuf>,
    /// Path to client private key
    pub client_key_path: Option<PathBuf>,
    /// Whether to verify certificates
    pub verify_certificates: bool,
}

/// Synchronization strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStrategy {
    /// Strategy name
    pub name: String,
    /// Source type
    pub source: SyncSource,
    /// Target type
    pub target: SyncTarget,
    /// Sync mode
    pub mode: SyncMode,
    /// Conflict resolution strategy
    pub conflict_resolution: ConflictResolution,
    /// Strategy-specific options
    pub options: HashMap<String, serde_json::Value>,
}

/// Sync source types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncSource {
    /// Filesystem
    Filesystem,
    /// Database
    Database,
    /// Remote API
    RemoteApi,
    /// Custom source
    Custom(String),
}

/// Sync target types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncTarget {
    /// Database table
    DatabaseTable(String),
    /// Database collection
    DatabaseCollection(String),
    /// Search index
    SearchIndex(String),
    /// Cache
    Cache(String),
    /// Custom target
    Custom(String),
}

/// Sync modes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncMode {
    /// One-way sync (source to target)
    OneWay,
    /// Two-way sync (bidirectional)
    TwoWay,
    /// Mirror (target matches source exactly)
    Mirror,
    /// Incremental (only sync changes)
    Incremental,
}

/// Conflict resolution strategies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConflictResolution {
    /// Source wins
    SourceWins,
    /// Target wins
    TargetWins,
    /// Manual resolution
    Manual,
    /// Timestamp-based resolution
    Timestamp,
    /// Custom resolution strategy
    Custom(String),
}

/// Transaction configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionConfig {
    /// Default transaction timeout in seconds
    pub default_timeout_seconds: u64,
    /// Maximum transaction size (number of operations)
    pub max_transaction_size: Option<u32>,
    /// Whether to use transactions for bulk operations
    pub use_transactions: bool,
    /// Batch size for bulk operations
    pub batch_size: u32,
}

/// Indexing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingConfig {
    /// Whether to automatically create indexes
    pub auto_create: bool,
    /// Index definitions
    pub indexes: Vec<IndexDefinition>,
    /// Index update strategy
    pub update_strategy: IndexUpdateStrategy,
}

/// Index definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDefinition {
    /// Index name
    pub name: String,
    /// Table/collection to index
    pub table: String,
    /// Fields to index
    pub fields: Vec<String>,
    /// Index type
    pub index_type: IndexType,
    /// Index options
    pub options: HashMap<String, serde_json::Value>,
}

/// Index types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IndexType {
    /// B-tree index
    BTree,
    /// Hash index
    Hash,
    /// Full-text search index
    FullText,
    /// Geospatial index
    Geospatial,
    /// Vector index for similarity search
    Vector,
}

/// Index update strategies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IndexUpdateStrategy {
    /// Update indexes immediately
    Immediate,
    /// Batch index updates
    Batch,
    /// Update indexes in background
    Background,
}

/// Backup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    /// Whether to enable automatic backups
    pub enabled: bool,
    /// Backup schedule (cron expression)
    pub schedule: Option<String>,
    /// Backup retention policy
    pub retention: BackupRetentionPolicy,
    /// Backup storage configuration
    pub storage: BackupStorageConfig,
}

/// Backup retention policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRetentionPolicy {
    /// Number of daily backups to keep
    pub daily_backups: u32,
    /// Number of weekly backups to keep
    pub weekly_backups: u32,
    /// Number of monthly backups to keep
    pub monthly_backups: u32,
    /// Maximum age of backups in days
    pub max_age_days: Option<u32>,
}

/// Backup storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStorageConfig {
    /// Storage type
    pub storage_type: BackupStorageType,
    /// Storage path
    pub path: PathBuf,
    /// Storage-specific options
    pub options: HashMap<String, serde_json::Value>,
}

/// Backup storage types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BackupStorageType {
    /// Local filesystem
    Local,
    /// S3-compatible storage
    S3,
    /// Azure Blob Storage
    AzureBlob,
    /// Google Cloud Storage
    GCS,
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Worker pool configuration
    pub workers: WorkerPoolConfig,
    /// Cache configuration
    pub cache: CacheConfig,
    /// Resource limits
    pub limits: ResourceLimits,
}

/// Worker pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerPoolConfig {
    /// Number of worker threads
    pub num_workers: Option<usize>,
    /// Maximum task queue size
    pub max_queue_size: usize,
    /// Worker thread affinity
    pub thread_affinity: Option<Vec<usize>>,
}

// CacheConfig and CacheType are now imported from crucible-config (canonical)
pub use crucible_config::{CacheConfig, CacheType};

/// Resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory usage in bytes
    pub max_memory_bytes: Option<u64>,
    /// Maximum CPU usage percentage
    pub max_cpu_percent: Option<f64>,
    /// Maximum file descriptors
    pub max_file_descriptors: Option<u32>,
    /// Maximum open files
    pub max_open_files: Option<u32>,
}

/// Health monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    /// Health check configuration
    pub checks: Vec<HealthCheck>,
    /// Metrics collection configuration
    pub metrics: MetricsConfig,
    /// Alert configuration
    pub alerts: AlertConfig,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Check name
    pub name: String,
    /// Check type
    pub check_type: HealthCheckType,
    /// Check interval in seconds
    pub interval_seconds: u64,
    /// Check timeout in seconds
    pub timeout_seconds: u64,
    /// Failure threshold before alerting
    pub failure_threshold: u32,
    /// Check-specific options
    pub options: HashMap<String, serde_json::Value>,
}

/// Health check types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthCheckType {
    /// Database connectivity check
    Database,
    /// Filesystem accessibility check
    Filesystem,
    /// Memory usage check
    Memory,
    /// CPU usage check
    Cpu,
    /// Disk space check
    DiskSpace,
    /// Network connectivity check
    Network,
    /// Custom health check
    Custom(String),
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Whether to enable metrics collection
    pub enabled: bool,
    /// Metrics collection interval in seconds
    pub interval_seconds: u64,
    /// Metrics to collect
    pub metrics: Vec<String>,
    /// Metrics exporter configuration
    pub exporter: MetricsExporterConfig,
}

/// Metrics exporter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsExporterConfig {
    /// Exporter type
    pub exporter_type: MetricsExporterType,
    /// Exporter endpoint/destination
    pub endpoint: Option<String>,
    /// Exporter-specific options
    pub options: HashMap<String, serde_json::Value>,
}

/// Metrics exporter types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MetricsExporterType {
    /// Prometheus exporter
    Prometheus,
    /// StatsD exporter
    StatsD,
    /// OpenTelemetry exporter
    OpenTelemetry,
    /// Console exporter
    Console,
    /// Disabled
    Disabled,
}

/// Alert configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Whether to enable alerts
    pub enabled: bool,
    /// Alert channels
    pub channels: Vec<AlertChannel>,
    /// Alert rules
    pub rules: Vec<AlertRule>,
}

/// Alert channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertChannel {
    /// Channel name
    pub name: String,
    /// Channel type
    pub channel_type: AlertChannelType,
    /// Channel configuration
    pub config: HashMap<String, serde_json::Value>,
}

/// Alert channel types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlertChannelType {
    /// Email alerts
    Email,
    /// Slack alerts
    Slack,
    /// Webhook alerts
    Webhook,
    /// SMS alerts
    Sms,
    /// Custom alert channel
    Custom(String),
}

/// Alert rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// Rule name
    pub name: String,
    /// Rule condition
    pub condition: String,
    /// Rule severity
    pub severity: AlertSeverity,
    /// Whether the rule is enabled
    pub enabled: bool,
    /// Cooldown period in seconds
    pub cooldown_seconds: u64,
}

/// Alert severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Service integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesConfig {
    /// Core controller connection
    pub core_controller: CoreControllerConfig,
    /// External service integrations
    pub external: HashMap<String, ExternalServiceConfig>,
}

/// Core controller configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreControllerConfig {
    /// Controller endpoint
    pub endpoint: String,
    /// Authentication configuration
    pub auth: AuthConfig,
    /// Connection settings
    pub connection: ServiceConnectionConfig,
    /// Event subscription configuration
    pub events: EventSubscriptionConfig,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Authentication type
    pub auth_type: AuthType,
    /// Authentication credentials
    pub credentials: AuthCredentials,
}

/// Authentication types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuthType {
    /// No authentication
    None,
    /// API key authentication
    ApiKey,
    /// Bearer token authentication
    BearerToken,
    /// Basic authentication
    Basic,
    /// OAuth2 authentication
    OAuth2,
}

/// Authentication credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthCredentials {
    /// API key
    pub api_key: Option<String>,
    /// Bearer token
    pub bearer_token: Option<String>,
    /// Username
    pub username: Option<String>,
    /// Password
    pub password: Option<String>,
    /// OAuth2 configuration
    pub oauth2: Option<OAuth2Config>,
}

/// OAuth2 configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Config {
    /// Client ID
    pub client_id: String,
    /// Client secret
    pub client_secret: String,
    /// Token URL
    pub token_url: String,
    /// Scopes
    pub scopes: Vec<String>,
}

/// Service connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConnectionConfig {
    /// Connection timeout in seconds
    pub timeout_seconds: u64,
    /// Keep-alive interval in seconds
    pub keep_alive_seconds: Option<u64>,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Retry delay in milliseconds
    pub retry_delay_ms: u64,
}

/// Event subscription configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSubscriptionConfig {
    /// Events to subscribe to
    pub events: Vec<String>,
    /// Subscription mode
    pub mode: SubscriptionMode,
    /// Event filter
    pub filter: Option<String>,
}

/// Subscription modes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SubscriptionMode {
    /// Subscribe to all events
    All,
    /// Subscribe to specific events
    Selective,
    /// Subscribe with pattern matching
    Pattern,
}

/// External service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalServiceConfig {
    /// Service name
    pub name: String,
    /// Service endpoint
    pub endpoint: String,
    /// Service type
    pub service_type: String,
    /// Authentication configuration
    pub auth: Option<AuthConfig>,
    /// Connection configuration
    pub connection: ServiceConnectionConfig,
    /// Service-specific options
    pub options: HashMap<String, serde_json::Value>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            filesystem: FilesystemConfig::default(),
            database: DaemonDatabaseConfig::default(),
            performance: PerformanceConfig::default(),
            health: HealthConfig::default(),
            services: ServicesConfig::default(),
        }
    }
}

impl Default for FilesystemConfig {
    fn default() -> Self {
        Self {
            watch_paths: vec![],
            backend: WatchBackend::Notify,
            debounce: DebounceConfig {
                delay_ms: 100,
                max_batch_size: 100,
                deduplicate: true,
                dedup_window_ms: Some(500),
            },
            filters: vec![],
            parsing: ParsingConfig::default(),
        }
    }
}

impl Default for ParsingConfig {
    fn default() -> Self {
        Self {
            parsers: HashMap::new(),
            metadata: MetadataExtractionConfig::default(),
            content_analysis: ContentAnalysisConfig::default(),
        }
    }
}

impl Default for MetadataExtractionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            fields: vec![
                "size".to_string(),
                "modified".to_string(),
                "created".to_string(),
            ],
            calculate_checksums: false,
            checksum_algorithm: ChecksumAlgorithm::SHA256,
        }
    }
}

impl Default for ContentAnalysisConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            analysis_types: vec![AnalysisType::MimeTypeDetection],
            max_file_size: Some(10 * 1024 * 1024), // 10MB
            timeout_seconds: Some(30),
        }
    }
}

impl Default for DaemonDatabaseConfig {
    fn default() -> Self {
        Self {
            connection: DatabaseConnectionConfig::default(),
            sync_strategies: vec![],
            transactions: TransactionConfig::default(),
            indexing: IndexingConfig::default(),
            backup: BackupConfig::default(),
        }
    }
}

impl Default for DatabaseConnectionConfig {
    fn default() -> Self {
        Self {
            database_type: DatabaseType::SurrealDB,
            connection_string: "memory".to_string(),
            pool: ConnectionPoolConfig::default(),
            tls: None,
        }
    }
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 1,
            timeout_seconds: 30,
            idle_timeout_seconds: 300,
            max_lifetime_seconds: Some(3600),
        }
    }
}

impl Default for TransactionConfig {
    fn default() -> Self {
        Self {
            default_timeout_seconds: 30,
            max_transaction_size: Some(1000),
            use_transactions: true,
            batch_size: 100,
        }
    }
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            auto_create: false,
            indexes: vec![],
            update_strategy: IndexUpdateStrategy::Immediate,
        }
    }
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            schedule: None,
            retention: BackupRetentionPolicy::default(),
            storage: BackupStorageConfig::default(),
        }
    }
}

impl Default for BackupRetentionPolicy {
    fn default() -> Self {
        Self {
            daily_backups: 7,
            weekly_backups: 4,
            monthly_backups: 12,
            max_age_days: Some(365),
        }
    }
}

impl Default for BackupStorageConfig {
    fn default() -> Self {
        Self {
            storage_type: BackupStorageType::Local,
            path: PathBuf::from("./backups"),
            options: HashMap::new(),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            workers: WorkerPoolConfig::default(),
            cache: CacheConfig::default(),
            limits: ResourceLimits::default(),
        }
    }
}

impl Default for WorkerPoolConfig {
    fn default() -> Self {
        Self {
            num_workers: None, // Use CPU count
            max_queue_size: 10000,
            thread_affinity: None,
        }
    }
}

// CacheConfig Default is provided by canonical implementation in crucible-config

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: Some(1024 * 1024 * 1024), // 1GB
            max_cpu_percent: Some(80.0),
            max_file_descriptors: Some(10000),
            max_open_files: Some(1000),
        }
    }
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            checks: vec![],
            metrics: MetricsConfig::default(),
            alerts: AlertConfig::default(),
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 60,
            metrics: vec!["memory_usage".to_string(), "cpu_usage".to_string()],
            exporter: MetricsExporterConfig::default(),
        }
    }
}

impl Default for MetricsExporterConfig {
    fn default() -> Self {
        Self {
            exporter_type: MetricsExporterType::Console,
            endpoint: None,
            options: HashMap::new(),
        }
    }
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            channels: vec![],
            rules: vec![],
        }
    }
}

impl Default for ServicesConfig {
    fn default() -> Self {
        Self {
            core_controller: CoreControllerConfig::default(),
            external: HashMap::new(),
        }
    }
}

impl Default for CoreControllerConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:8080".to_string(),
            auth: AuthConfig::default(),
            connection: ServiceConnectionConfig::default(),
            events: EventSubscriptionConfig::default(),
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            auth_type: AuthType::None,
            credentials: AuthCredentials::default(),
        }
    }
}

impl Default for AuthCredentials {
    fn default() -> Self {
        Self {
            api_key: None,
            bearer_token: None,
            username: None,
            password: None,
            oauth2: None,
        }
    }
}

impl Default for ServiceConnectionConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 30,
            keep_alive_seconds: Some(60),
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}

impl Default for EventSubscriptionConfig {
    fn default() -> Self {
        Self {
            events: vec!["*".to_string()],
            mode: SubscriptionMode::All,
            filter: None,
        }
    }
}

impl DaemonConfig {
    /// Load configuration from a file (simplified)
    pub async fn load_from_file(_path: &PathBuf) -> Result<Self, ConfigError> {
        // Simplified loading - for now just return default config
        // In a real implementation, this would load and parse the file
        Ok(Self::default())
    }

    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, ConfigError> {
        let mut config = Self::default();

        // Read OBSIDIAN_VAULT_PATH for secure configuration
        if let Ok(vault_path) = std::env::var("OBSIDIAN_VAULT_PATH") {
            let vault_path = std::path::PathBuf::from(vault_path);

            // Set up filesystem watching for the vault
            config.filesystem.watch_paths.push(WatchPath {
                path: vault_path.clone(),
                recursive: true,
                mode: WatchMode::All,
                filters: None,
                events: None,
            });

            // Set database connection to use vault's .crucible directory
            // Use the same database path, namespace, and database as the CLI for consistency
            config.database.connection.connection_string = format!(
                "file://{}/.crucible/embeddings.db/crucible/vault",
                vault_path.display()
            );
        } else {
            // SECURITY: Require OBSIDIAN_VAULT_PATH to be set
            return Err(ConfigError::InvalidValue {
                field: "OBSIDIAN_VAULT_PATH".to_string(),
                value: "missing".to_string(),
            });
        }

        // Additional environment variables can be read here
        if let Ok(timeout) = std::env::var("CRUCIBLE_DAEMON_TIMEOUT") {
            if let Ok(timeout_secs) = timeout.parse::<u64>() {
                config
                    .health
                    .checks
                    .iter_mut()
                    .for_each(|check| check.timeout_seconds = timeout_secs);
            }
        }

        Ok(config)
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate filesystem config
        if self.filesystem.watch_paths.is_empty() {
            return Err(ConfigError::InvalidValue {
                field: "watch_paths".to_string(),
                value: "empty".to_string(),
            });
        }

        // Validate database connection
        if self.database.connection.connection_string.is_empty() {
            return Err(ConfigError::InvalidValue {
                field: "connection_string".to_string(),
                value: "empty".to_string(),
            });
        }

        // Validate performance limits
        if let Some(max_memory) = self.performance.limits.max_memory_bytes {
            if max_memory == 0 {
                return Err(ConfigError::InvalidValue {
                    field: "max_memory_bytes".to_string(),
                    value: "0".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Get configuration as a typed value
    pub fn get<T>(&self, _key: &str) -> Option<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        // This would need implementation for dynamic config access
        None
    }

    /// Set a configuration value
    pub fn set<T>(&mut self, _key: &str, _value: T) -> Result<(), ConfigError>
    where
        T: Serialize,
    {
        // This would need implementation for dynamic config setting
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DaemonConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation() {
        let mut config = DaemonConfig::default();

        // Test empty watch paths validation
        config.filesystem.watch_paths.clear();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_watch_path_serialization() {
        let watch_path = WatchPath {
            path: PathBuf::from("/test"),
            recursive: true,
            mode: WatchMode::All,
            filters: None,
            events: None,
        };

        let serialized = serde_json::to_string(&watch_path).unwrap();
        let deserialized: WatchPath = serde_json::from_str(&serialized).unwrap();

        assert_eq!(watch_path.path, deserialized.path);
        assert_eq!(watch_path.recursive, deserialized.recursive);
        assert_eq!(watch_path.mode, deserialized.mode);
    }
}
