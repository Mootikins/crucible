//! Configuration management for plugin event subscription system

use crate::plugin_events::{
    delivery_system::DeliveryConfig,
    event_bridge::BridgeConfig,
    subscription_api::{ApiConfig, AuthConfig, RateLimitConfig, WebSocketConfig},
    subscription_manager::{AuditConfig, ManagerConfig, PersistenceConfig},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Complete configuration for the plugin event subscription system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionSystemConfig {
    /// Manager configuration
    pub manager: ManagerConfig,

    /// Delivery system configuration
    pub delivery: DeliveryConfig,

    /// Event bridge configuration
    pub bridge: BridgeConfig,

    /// API server configuration
    pub api: ApiConfig,

    /// Filter engine configuration
    pub filter_engine: FilterEngineConfig,

    /// Security configuration
    pub security: SecurityConfig,

    /// Monitoring configuration
    pub monitoring: MonitoringConfig,

    /// Logging configuration
    pub logging: LoggingConfig,

    /// System-wide settings
    pub system: SystemConfig,
}

impl Default for SubscriptionSystemConfig {
    fn default() -> Self {
        Self {
            manager: ManagerConfig::default(),
            delivery: DeliveryConfig::default(),
            bridge: BridgeConfig::default(),
            api: ApiConfig::default(),
            filter_engine: FilterEngineConfig::default(),
            security: SecurityConfig::default(),
            monitoring: MonitoringConfig::default(),
            logging: LoggingConfig::default(),
            system: SystemConfig::default(),
        }
    }
}

/// Filter engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterEngineConfig {
    /// Maximum filter complexity
    pub max_complexity: u8,

    /// Enable filter compilation cache
    pub enable_cache: bool,

    /// Maximum cache size
    pub max_cache_size: usize,

    /// Enable parallel execution
    pub enable_parallel: bool,

    /// Filter execution timeout in milliseconds
    pub execution_timeout_ms: u64,

    /// Enable statistics collection
    pub enable_stats: bool,

    /// Maximum regex complexity
    pub max_regex_complexity: u8,

    /// Custom filter functions
    pub custom_functions: HashMap<String, CustomFunctionConfig>,
}

impl Default for FilterEngineConfig {
    fn default() -> Self {
        Self {
            max_complexity: 10,
            enable_cache: true,
            max_cache_size: 1000,
            enable_parallel: true,
            execution_timeout_ms: 1000,
            enable_stats: true,
            max_regex_complexity: 5,
            custom_functions: HashMap::new(),
        }
    }
}

/// Custom filter function configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomFunctionConfig {
    /// Function name
    pub name: String,

    /// Function implementation type
    pub implementation_type: FunctionImplementationType,

    /// Function script or module path
    pub path: String,

    /// Function parameters
    pub parameters: Vec<FunctionParameter>,

    /// Function enabled flag
    pub enabled: bool,

    /// Function metadata
    pub metadata: HashMap<String, String>,
}

/// Function implementation type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FunctionImplementationType {
    /// JavaScript function
    JavaScript,

    /// Rust function (native)
    Rust,

    /// WebAssembly function
    Wasm,

    /// External process
    External,

    /// SQL function
    Sql,
}

/// Function parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionParameter {
    /// Parameter name
    pub name: String,

    /// Parameter type
    pub param_type: String,

    /// Required flag
    pub required: bool,

    /// Default value
    pub default_value: Option<serde_json::Value>,

    /// Description
    pub description: Option<String>,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable security features
    pub enabled: bool,

    /// Authentication configuration
    pub authentication: AuthConfig,

    /// Authorization configuration
    pub authorization: AuthorizationConfig,

    /// Encryption configuration
    pub encryption: EncryptionConfig,

    /// Audit configuration
    pub audit: AuditConfig,

    /// Rate limiting configuration
    pub rate_limiting: RateLimitConfig,

    /// Security policies
    pub policies: HashMap<String, SecurityPolicyConfig>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            authentication: AuthConfig::default(),
            authorization: AuthorizationConfig::default(),
            encryption: EncryptionConfig::default(),
            audit: AuditConfig::default(),
            rate_limiting: RateLimitConfig::default(),
            policies: HashMap::new(),
        }
    }
}

/// Authorization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationConfig {
    /// Enable authorization
    pub enabled: bool,

    /// Default permissions for new subscriptions
    pub default_permissions: Vec<PermissionConfig>,

    /// Access control lists
    pub access_control_lists: HashMap<String, AccessControlListConfig>,

    /// Permission inheritance rules
    pub inheritance_rules: Vec<InheritanceRule>,
}

impl Default for AuthorizationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_permissions: vec![
                PermissionConfig {
                    scope: "plugin".to_string(),
                    event_types: vec![],
                    categories: vec![],
                    sources: vec!["self".to_string()],
                    max_priority: None,
                    conditions: vec![],
                }
            ],
            access_control_lists: HashMap::new(),
            inheritance_rules: vec![],
        }
    }
}

/// Permission configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionConfig {
    /// Permission scope
    pub scope: String,

    /// Allowed event types
    pub event_types: Vec<String>,

    /// Allowed event categories
    pub categories: Vec<String>,

    /// Allowed sources
    pub sources: Vec<String>,

    /// Maximum priority level
    pub max_priority: Option<u8>,

    /// Permission conditions
    pub conditions: Vec<String>,
}

/// Access control list configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlListConfig {
    /// ACL name
    pub name: String,

    /// ACL description
    pub description: Option<String>,

    /// ACL entries
    pub entries: Vec<AclEntryConfig>,

    /// Default action
    pub default_action: String,

    /// ACL priority
    pub priority: u8,
}

/// ACL entry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclEntryConfig {
    /// Principal (plugin/user ID)
    pub principal: String,

    /// Event type pattern
    pub event_pattern: String,

    /// Permission (allow/deny)
    pub permission: String,

    /// Conditions
    pub conditions: Vec<String>,

    /// Entry priority
    pub priority: u8,
}

/// Permission inheritance rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InheritanceRule {
    /// Rule name
    pub name: String,

    /// Source scope
    pub from_scope: String,

    /// Target scope
    pub to_scope: String,

    /// Inheritance conditions
    pub conditions: Vec<String>,

    /// Rule enabled flag
    pub enabled: bool,
}

/// Encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    /// Enable encryption
    pub enabled: bool,

    /// Default encryption algorithm
    pub default_algorithm: String,

    /// Encryption keys configuration
    pub keys: EncryptionKeysConfig,

    /// Key rotation configuration
    pub key_rotation: KeyRotationConfig,

    /// Encryption at rest settings
    pub at_rest: AtRestEncryptionConfig,
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_algorithm: "aes256-gcm".to_string(),
            keys: EncryptionKeysConfig::default(),
            key_rotation: KeyRotationConfig::default(),
            at_rest: AtRestEncryptionConfig::default(),
        }
    }
}

/// Encryption keys configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionKeysConfig {
    /// Key management service
    pub key_management_service: String,

    /// Key store path
    pub key_store_path: Option<PathBuf>,

    /// Master key ID
    pub master_key_id: Option<String>,

    /// Key derivation settings
    pub key_derivation: KeyDerivationConfig,
}

impl Default for EncryptionKeysConfig {
    fn default() -> Self {
        Self {
            key_management_service: "local".to_string(),
            key_store_path: None,
            master_key_id: None,
            key_derivation: KeyDerivationConfig::default(),
        }
    }
}

/// Key derivation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyDerivationConfig {
    /// Key derivation function
    pub function: String,

    /// Salt
    pub salt: Option<String>,

    /// Iterations
    pub iterations: u32,

    /// Key length in bytes
    pub key_length: u32,
}

impl Default for KeyDerivationConfig {
    fn default() -> Self {
        Self {
            function: "pbkdf2".to_string(),
            salt: None,
            iterations: 100000,
            key_length: 32,
        }
    }
}

/// Key rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationConfig {
    /// Enable automatic key rotation
    pub enabled: bool,

    /// Rotation interval in days
    pub rotation_interval_days: u32,

    /// Key retention period in days
    pub retention_period_days: u32,

    /// Rotation notification settings
    pub notification: RotationNotificationConfig,
}

impl Default for KeyRotationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rotation_interval_days: 90,
            retention_period_days: 180,
            notification: RotationNotificationConfig::default(),
        }
    }
}

/// Rotation notification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationNotificationConfig {
    /// Enable notifications
    pub enabled: bool,

    /// Notification channels
    pub channels: Vec<String>,

    /// Notification recipients
    pub recipients: Vec<String>,

    /// Advance warning in days
    pub advance_warning_days: u32,
}

impl Default for RotationNotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            channels: vec!["email".to_string(), "webhook".to_string()],
            recipients: vec![],
            advance_warning_days: 7,
        }
    }
}

/// Encryption at rest configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtRestEncryptionConfig {
    /// Enable encryption at rest
    pub enabled: bool,

    /// Database encryption
    pub database: DatabaseEncryptionConfig,

    /// File encryption
    pub files: FileEncryptionConfig,

    /// Log encryption
    pub logs: LogEncryptionConfig,
}

impl Default for AtRestEncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            database: DatabaseEncryptionConfig::default(),
            files: FileEncryptionConfig::default(),
            logs: LogEncryptionConfig::default(),
        }
    }
}

/// Database encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseEncryptionConfig {
    /// Enable database encryption
    pub enabled: bool,

    /// Encrypted columns
    pub encrypted_columns: Vec<String>,

    /// Encryption key ID
    pub key_id: Option<String>,
}

impl Default for DatabaseEncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            encrypted_columns: vec![],
            key_id: None,
        }
    }
}

/// File encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEncryptionConfig {
    /// Enable file encryption
    pub enabled: bool,

    /// Encrypted file patterns
    pub encrypted_patterns: Vec<String>,

    /// Encryption key ID
    pub key_id: Option<String>,
}

impl Default for FileEncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            encrypted_patterns: vec!["*.enc".to_string()],
            key_id: None,
        }
    }
}

/// Log encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEncryptionConfig {
    /// Enable log encryption
    pub enabled: bool,

    /// Encrypt sensitive data
    pub encrypt_sensitive_data: bool,

    /// Sensitive patterns
    pub sensitive_patterns: Vec<String>,
}

impl Default for LogEncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            encrypt_sensitive_data: true,
            sensitive_patterns: vec![
                "password".to_string(),
                "token".to_string(),
                "secret".to_string(),
                "key".to_string(),
            ],
        }
    }
}

/// Security policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicyConfig {
    /// Policy name
    pub name: String,

    /// Policy description
    pub description: Option<String>,

    /// Policy rules
    pub rules: Vec<SecurityRuleConfig>,

    /// Policy priority
    pub priority: u8,

    /// Policy enabled flag
    pub enabled: bool,
}

/// Security rule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRuleConfig {
    /// Rule name
    pub name: String,

    /// Rule condition
    pub condition: String,

    /// Rule action
    pub action: String,

    /// Rule parameters
    pub parameters: HashMap<String, String>,

    /// Rule enabled flag
    pub enabled: bool,
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable monitoring
    pub enabled: bool,

    /// Metrics configuration
    pub metrics: MetricsConfig,

    /// Health checks configuration
    pub health_checks: HealthChecksConfig,

    /// Alerting configuration
    pub alerting: AlertingConfig,

    /// Tracing configuration
    pub tracing: TracingConfig,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            metrics: MetricsConfig::default(),
            health_checks: HealthChecksConfig::default(),
            alerting: AlertingConfig::default(),
            tracing: TracingConfig::default(),
        }
    }
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,

    /// Metrics collection interval in seconds
    pub collection_interval_seconds: u64,

    /// Metrics retention period in hours
    pub retention_hours: u32,

    /// Exporter configuration
    pub exporter: MetricsExporterConfig,

    /// Custom metrics
    pub custom_metrics: Vec<CustomMetricConfig>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval_seconds: 60,
            retention_hours: 24,
            exporter: MetricsExporterConfig::default(),
            custom_metrics: vec![],
        }
    }
}

/// Metrics exporter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsExporterConfig {
    /// Exporter type
    pub exporter_type: String,

    /// Exporter endpoint
    pub endpoint: Option<String>,

    /// Exporter configuration
    pub config: HashMap<String, serde_json::Value>,
}

impl Default for MetricsExporterConfig {
    fn default() -> Self {
        Self {
            exporter_type: "prometheus".to_string(),
            endpoint: Some("/metrics".to_string()),
            config: HashMap::new(),
        }
    }
}

/// Custom metric configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomMetricConfig {
    /// Metric name
    pub name: String,

    /// Metric type
    pub metric_type: String,

    /// Metric description
    pub description: Option<String>,

    /// Metric labels
    pub labels: HashMap<String, String>,

    /// Collection function
    pub collection_function: String,
}

/// Health checks configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthChecksConfig {
    /// Enable health checks
    pub enabled: bool,

    /// Health check interval in seconds
    pub interval_seconds: u64,

    /// Health check timeout in seconds
    pub timeout_seconds: u64,

    /// Health check endpoints
    pub endpoints: Vec<HealthCheckEndpointConfig>,

    /// Unhealthy threshold
    pub unhealthy_threshold: u32,
}

impl Default for HealthChecksConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 30,
            timeout_seconds: 10,
            endpoints: vec![
                HealthCheckEndpointConfig {
                    name: "api".to_string(),
                    url: "http://localhost:8080/health".to_string(),
                    method: "GET".to_string(),
                    headers: HashMap::new(),
                    expected_status: 200,
                    timeout_seconds: 5,
                }
            ],
            unhealthy_threshold: 3,
        }
    }
}

/// Health check endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckEndpointConfig {
    /// Endpoint name
    pub name: String,

    /// Endpoint URL
    pub url: String,

    /// HTTP method
    pub method: String,

    /// Request headers
    pub headers: HashMap<String, String>,

    /// Expected HTTP status
    pub expected_status: u16,

    /// Timeout in seconds
    pub timeout_seconds: u64,
}

/// Alerting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertingConfig {
    /// Enable alerting
    pub enabled: bool,

    /// Alert rules
    pub rules: Vec<AlertRuleConfig>,

    /// Notification channels
    pub channels: HashMap<String, NotificationChannelConfig>,

    /// Alert grouping configuration
    pub grouping: AlertGroupingConfig,
}

impl Default for AlertingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rules: vec![],
            channels: HashMap::new(),
            grouping: AlertGroupingConfig::default(),
        }
    }
}

/// Alert rule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRuleConfig {
    /// Rule name
    pub name: String,

    /// Rule condition
    pub condition: String,

    /// Rule severity
    pub severity: String,

    /// Rule description
    pub description: Option<String>,

    /// Evaluation interval in seconds
    pub evaluation_interval_seconds: u64,

    /// For duration in seconds
    pub for_duration_seconds: u64,

    /// Notification channels
    pub channels: Vec<String>,

    /// Rule enabled flag
    pub enabled: bool,
}

/// Notification channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannelConfig {
    /// Channel name
    pub name: String,

    /// Channel type
    pub channel_type: String,

    /// Channel configuration
    pub config: HashMap<String, serde_json::Value>,

    /// Channel enabled flag
    pub enabled: bool,
}

/// Alert grouping configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertGroupingConfig {
    /// Enable alert grouping
    pub enabled: bool,

    /// Group by fields
    pub group_by: Vec<String>,

    /// Group wait time in seconds
    pub group_wait_seconds: u64,

    /// Group interval in seconds
    pub group_interval_seconds: u64,

    /// Repeat interval in seconds
    pub repeat_interval_seconds: u64,
}

impl Default for AlertGroupingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            group_by: vec!["plugin_id".to_string(), "subscription_id".to_string()],
            group_wait_seconds: 10,
            group_interval_seconds: 300,
            repeat_interval_seconds: 3600,
        }
    }
}

/// Tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// Enable tracing
    pub enabled: bool,

    /// Tracing service name
    pub service_name: String,

    /// Sampling configuration
    pub sampling: SamplingConfig,

    /// Exporter configuration
    pub exporter: TracingExporterConfig,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            service_name: "crucible-plugin-events".to_string(),
            sampling: SamplingConfig::default(),
            exporter: TracingExporterConfig::default(),
        }
    }
}

/// Sampling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingConfig {
    /// Sampling type
    pub sampling_type: String,

    /// Sampling rate (0.0 to 1.0)
    pub rate: f64,
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            sampling_type: "probabilistic".to_string(),
            rate: 0.1,
        }
    }
}

/// Tracing exporter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingExporterConfig {
    /// Exporter type
    pub exporter_type: String,

    /// Exporter endpoint
    pub endpoint: Option<String>,

    /// Exporter configuration
    pub config: HashMap<String, serde_json::Value>,
}

impl Default for TracingExporterConfig {
    fn default() -> Self {
        Self {
            exporter_type: "jaeger".to_string(),
            endpoint: Some("http://localhost:14268/api/traces".to_string()),
            config: HashMap::new(),
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Logging level
    pub level: String,

    /// Log format
    pub format: String,

    /// Log output configuration
    pub output: LogOutputConfig,

    /// Log rotation configuration
    pub rotation: LogRotationConfig,

    /// Log filtering configuration
    pub filtering: LogFilteringConfig,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "json".to_string(),
            output: LogOutputConfig::default(),
            rotation: LogRotationConfig::default(),
            filtering: LogFilteringConfig::default(),
        }
    }
}

/// Log output configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogOutputConfig {
    /// Output targets
    pub targets: Vec<LogTargetConfig>,

    /// Console output settings
    pub console: ConsoleLogConfig,

    /// File output settings
    pub file: FileLogConfig,
}

impl Default for LogOutputConfig {
    fn default() -> Self {
        Self {
            targets: vec![LogTargetConfig {
                name: "default".to_string(),
                target_type: "console".to_string(),
                level: "info".to_string(),
                format: "json".to_string(),
                config: HashMap::new(),
            }],
            console: ConsoleLogConfig::default(),
            file: FileLogConfig::default(),
        }
    }
}

/// Log target configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogTargetConfig {
    /// Target name
    pub name: String,

    /// Target type
    pub target_type: String,

    /// Log level
    pub level: String,

    /// Log format
    pub format: String,

    /// Target configuration
    pub config: HashMap<String, serde_json::Value>,
}

/// Console log configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleLogConfig {
    /// Enable console output
    pub enabled: bool,

    /// Use colors
    pub use_colors: bool,

    /// Output to stderr
    pub stderr: bool,
}

impl Default for ConsoleLogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            use_colors: true,
            stderr: false,
        }
    }
}

/// File log configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLogConfig {
    /// Enable file output
    pub enabled: bool,

    /// Log file path
    pub file_path: Option<PathBuf>,

    /// File permissions
    pub file_permissions: Option<String>,
}

impl Default for FileLogConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            file_path: Some(PathBuf::from("logs/crucible-plugin-events.log")),
            file_permissions: Some("644".to_string()),
        }
    }
}

/// Log rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRotationConfig {
    /// Enable log rotation
    pub enabled: bool,

    /// Maximum file size in megabytes
    pub max_file_size_mb: u64,

    /// Maximum number of files
    pub max_files: u32,

    /// Rotation schedule
    pub schedule: String,
}

impl Default for LogRotationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_file_size_mb: 100,
            max_files: 10,
            schedule: "daily".to_string(),
        }
    }
}

/// Log filtering configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogFilteringConfig {
    /// Enable log filtering
    pub enabled: bool,

    /// Filter rules
    pub filters: Vec<LogFilterConfig>,

    /// Exclude patterns
    pub exclude_patterns: Vec<String>,
}

impl Default for LogFilteringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            filters: vec![],
            exclude_patterns: vec![
                "hyper::client".to_string(),
                "tokio".to_string(),
                "h2".to_string(),
            ],
        }
    }
}

/// Log filter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogFilterConfig {
    /// Filter name
    pub name: String,

    /// Target pattern
    pub target: String,

    /// Filter condition
    pub condition: String,

    /// Action (include/exclude)
    pub action: String,
}

/// System configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// System name
    pub name: String,

    /// Environment
    pub environment: String,

    /// Data directory
    pub data_dir: PathBuf,

    /// Temporary directory
    pub temp_dir: PathBuf,

    /// Thread pool configuration
    pub thread_pool: ThreadPoolConfig,

    /// Resource limits
    pub resource_limits: ResourceLimitsConfig,

    /// Performance tuning
    pub performance: PerformanceConfig,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            name: "crucible-plugin-events".to_string(),
            environment: "development".to_string(),
            data_dir: PathBuf::from("./data"),
            temp_dir: PathBuf::from("./tmp"),
            thread_pool: ThreadPoolConfig::default(),
            resource_limits: ResourceLimitsConfig::default(),
            performance: PerformanceConfig::default(),
        }
    }
}

/// Thread pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadPoolConfig {
    /// Event processing threads
    pub event_processing_threads: usize,

    /// Delivery threads
    pub delivery_threads: usize,

    /// I/O threads
    pub io_threads: usize,

    /// Metrics threads
    pub metrics_threads: usize,
}

impl Default for ThreadPoolConfig {
    fn default() -> Self {
        let cpu_count = num_cpus::get();
        Self {
            event_processing_threads: (cpu_count / 2).max(2),
            delivery_threads: (cpu_count / 4).max(1),
            io_threads: 4,
            metrics_threads: 2,
        }
    }
}

/// Resource limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimitsConfig {
    /// Maximum memory usage in megabytes
    pub max_memory_mb: u64,

    /// Maximum file descriptors
    pub max_file_descriptors: u32,

    /// Maximum concurrent connections
    pub max_connections: u32,

    /// Maximum queue depth
    pub max_queue_depth: usize,
}

impl Default for ResourceLimitsConfig {
    fn default() -> Self {
        Self {
            max_memory_mb: 1024,
            max_file_descriptors: 10000,
            max_connections: 1000,
            max_queue_depth: 10000,
        }
    }
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Enable performance optimizations
    pub enable_optimizations: bool,

    /// Buffer sizes
    pub buffer_sizes: BufferSizesConfig,

    /// Cache configuration
    pub cache: CacheConfig,

    /// Batching configuration
    pub batching: BatchingConfig,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            enable_optimizations: true,
            buffer_sizes: BufferSizesConfig::default(),
            cache: CacheConfig::default(),
            batching: BatchingConfig::default(),
        }
    }
}

/// Buffer sizes configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferSizesConfig {
    /// Event buffer size
    pub event_buffer_size: usize,

    /// Delivery buffer size
    pub delivery_buffer_size: usize,

    /// Metrics buffer size
    pub metrics_buffer_size: usize,
}

impl Default for BufferSizesConfig {
    fn default() -> Self {
        Self {
            event_buffer_size: 10000,
            delivery_buffer_size: 5000,
            metrics_buffer_size: 1000,
        }
    }
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Enable caching
    pub enabled: bool,

    /// Cache TTL in seconds
    pub ttl_seconds: u64,

    /// Maximum cache size
    pub max_size: usize,

    /// Cache eviction policy
    pub eviction_policy: String,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_seconds: 300,
            max_size: 1000,
            eviction_policy: "lru".to_string(),
        }
    }
}

/// Batching configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchingConfig {
    /// Enable batching
    pub enabled: bool,

    /// Default batch size
    pub default_batch_size: usize,

    /// Maximum batch size
    pub max_batch_size: usize,

    /// Batch timeout in milliseconds
    pub batch_timeout_ms: u64,
}

impl Default for BatchingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_batch_size: 100,
            max_batch_size: 1000,
            batch_timeout_ms: 1000,
        }
    }
}

impl SubscriptionSystemConfig {
    /// Load configuration from file
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> SubscriptionResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| SubscriptionError::ConfigurationError(
                format!("Failed to read config file: {}", e)
            ))?;

        let config: SubscriptionSystemConfig = toml::from_str(&content)
            .map_err(|e| SubscriptionError::ConfigurationError(
                format!("Failed to parse config file: {}", e)
            ))?;

        info!("Loaded configuration from file");
        Ok(config)
    }

    /// Save configuration to file
    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> SubscriptionResult<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| SubscriptionError::ConfigurationError(
                format!("Failed to serialize config: {}", e)
            ))?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| SubscriptionError::ConfigurationError(
                    format!("Failed to create config directory: {}", e)
                ))?;
        }

        std::fs::write(path, content)
            .map_err(|e| SubscriptionError::ConfigurationError(
                format!("Failed to write config file: {}", e)
            ))?;

        info!("Saved configuration to file");
        Ok(())
    }

    /// Load configuration from environment variables
    pub fn from_env() -> SubscriptionSystemConfig {
        let mut config = SubscriptionSystemConfig::default();

        // Override with environment variables
        if let Ok(val) = std::env::var("CRUCIBLE_PLUGIN_EVENTS_LOG_LEVEL") {
            config.logging.level = val;
        }

        if let Ok(val) = std::env::var("CRUCIBLE_PLUGIN_EVENTS_API_PORT") {
            if let Ok(port) = val.parse() {
                config.api.port = port;
            }
        }

        if let Ok(val) = std::env::var("CRUCIBLE_PLUGIN_EVENTS_DATA_DIR") {
            config.system.data_dir = PathBuf::from(val);
        }

        if let Ok(val) = std::env::var("CRUCIBLE_PLUGIN_EVENTS_ENABLE_SECURITY") {
            config.security.enabled = val.parse().unwrap_or(config.security.enabled);
        }

        if let Ok(val) = std::env::var("CRUCIBLE_PLUGIN_EVENTS_MAX_MEMORY_MB") {
            if let Ok(max_memory) = val.parse() {
                config.system.resource_limits.max_memory_mb = max_memory;
            }
        }

        debug!("Loaded configuration from environment variables");
        config
    }

    /// Validate configuration
    pub fn validate(&self) -> SubscriptionResult<()> {
        // Validate system configuration
        if self.system.data_dir.as_os_str().is_empty() {
            return Err(SubscriptionError::ConfigurationError(
                "Data directory cannot be empty".to_string()
            ));
        }

        // Validate API configuration
        if self.api.port == 0 {
            return Err(SubscriptionError::ConfigurationError(
                "API port cannot be 0".to_string()
            ));
        }

        // Validate security configuration
        if self.security.enabled {
            if self.security.authentication.enabled && self.security.authentication.methods.is_empty() {
                warn!("Security authentication is enabled but no methods are configured");
            }
        }

        // Validate resource limits
        if self.system.resource_limits.max_memory_mb < 64 {
            warn!("Maximum memory limit is very low ({} MB)", self.system.resource_limits.max_memory_mb);
        }

        // Validate thread pool configuration
        if self.system.thread_pool.event_processing_threads == 0 {
            return Err(SubscriptionError::ConfigurationError(
                "Event processing threads cannot be 0".to_string()
            ));
        }

        info!("Configuration validation completed");
        Ok(())
    }

    /// Get effective configuration (with defaults applied)
    pub fn effective(&self) -> Self {
        let mut effective = self.clone();

        // Apply defaults where needed
        if effective.system.data_dir.as_os_str().is_empty() {
            effective.system.data_dir = PathBuf::from("./data");
        }

        if effective.api.port == 0 {
            effective.api.port = 8080;
        }

        effective
    }

    /// Merge with another configuration
    pub fn merge(&mut self, other: SubscriptionSystemConfig) {
        // System configuration
        if other.system.name != "crucible-plugin-events" {
            self.system.name = other.system.name;
        }
        if other.system.environment != "development" {
            self.system.environment = other.system.environment;
        }
        if !other.system.data_dir.as_os_str().is_empty() {
            self.system.data_dir = other.system.data_dir;
        }

        // API configuration
        if other.api.port != 8080 {
            self.api.port = other.api.port;
        }
        if other.api.bind_address != "127.0.0.1" {
            self.api.bind_address = other.api.bind_address;
        }

        // Security configuration
        if other.security.enabled != self.security.enabled {
            self.security.enabled = other.security.enabled;
        }

        // Logging configuration
        if other.logging.level != "info" {
            self.logging.level = other.logging.level;
        }

        // Note: This is a simplified merge implementation
        // In a production system, you'd want more comprehensive merge logic
        info!("Configuration merged successfully");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_configuration() {
        let config = SubscriptionSystemConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.api.port, 8080);
        assert_eq!(config.logging.level, "info");
        assert!(config.security.enabled);
    }

    #[test]
    fn test_configuration_serialization() {
        let config = SubscriptionSystemConfig::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: SubscriptionSystemConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(config.api.port, deserialized.api.port);
    }

    #[test]
    fn test_configuration_from_env() {
        std::env::set_var("CRUCIBLE_PLUGIN_EVENTS_API_PORT", "9090");
        std::env::set_var("CRUCIBLE_PLUGIN_EVENTS_LOG_LEVEL", "debug");

        let config = SubscriptionSystemConfig::from_env();
        assert_eq!(config.api.port, 9090);
        assert_eq!(config.logging.level, "debug");

        std::env::remove_var("CRUCIBLE_PLUGIN_EVENTS_API_PORT");
        std::env::remove_var("CRUCIBLE_PLUGIN_EVENTS_LOG_LEVEL");
    }

    #[test]
    fn test_configuration_file_io() {
        let config = SubscriptionSystemConfig::default();
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");

        // Save configuration
        assert!(config.save_to_file(&config_path).is_ok());
        assert!(config_path.exists());

        // Load configuration
        let loaded_config = SubscriptionSystemConfig::from_file(&config_path).unwrap();
        assert_eq!(config.api.port, loaded_config.api.port);
    }

    #[test]
    fn test_configuration_validation() {
        let mut config = SubscriptionSystemConfig::default();

        // Valid configuration should pass
        assert!(config.validate().is_ok());

        // Invalid port should fail
        config.api.port = 0;
        assert!(config.validate().is_err());

        // Empty data directory should fail
        config.api.port = 8080;
        config.system.data_dir = PathBuf::new();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_configuration_merge() {
        let mut base_config = SubscriptionSystemConfig::default();
        let mut override_config = SubscriptionSystemConfig::default();
        override_config.api.port = 9090;
        override_config.logging.level = "debug".to_string();

        base_config.merge(override_config);
        assert_eq!(base_config.api.port, 9090);
        assert_eq!(base_config.logging.level, "debug");
    }
}