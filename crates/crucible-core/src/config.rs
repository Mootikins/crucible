//! Configuration management system for Crucible
//!
//! This module provides centralized configuration loading, validation,
//! and hot-reload capabilities for the entire application.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use notify::Watcher;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info};

/// Configuration change notification
#[derive(Debug, Clone)]
pub struct ConfigChange {
    pub path: String,
    pub old_value: Option<serde_json::Value>,
    pub new_value: serde_json::Value,
}

/// Configuration validation error
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Configuration file not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Invalid configuration format: {0}")]
    InvalidFormat(String),

    #[error("Configuration validation failed: {0}")]
    ValidationFailed(String),

    #[error("Required configuration missing: {0}")]
    RequiredMissing(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    ParseError(#[from] serde_json::Error),
}

/// Master configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CrucibleConfig {
    /// Service configuration
    pub services: ServiceConfig,
    /// Database configuration
    pub database: DatabaseConfig,
    /// Network configuration
    pub network: NetworkConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
    /// Feature flags
    pub features: FeatureConfig,
    /// Performance tuning
    pub performance: PerformanceConfig,
}

impl Default for CrucibleConfig {
    fn default() -> Self {
        Self {
            services: ServiceConfig::default(),
            database: DatabaseConfig::default(),
            network: NetworkConfig::default(),
            logging: LoggingConfig::default(),
            features: FeatureConfig::default(),
            performance: PerformanceConfig::default(),
        }
    }
}

/// Service configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServiceConfig {
    /// Service registry configuration
    pub registry: ServiceRegistryConfig,
    /// Orchestration configuration
    pub orchestration: OrchestrationConfig,
    /// Health check configuration
    pub health_check: HealthCheckConfig,
    /// Custom service configurations
    pub custom: HashMap<String, serde_json::Value>,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            registry: ServiceRegistryConfig::default(),
            orchestration: OrchestrationConfig::default(),
            health_check: HealthCheckConfig::default(),
            custom: HashMap::new(),
        }
    }
}

/// Service registry configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServiceRegistryConfig {
    /// Registry type (memory, redis, etc.)
    pub registry_type: String,
    /// Service discovery timeout
    pub discovery_timeout: Duration,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Service TTL
    pub service_ttl: Duration,
}

impl Default for ServiceRegistryConfig {
    fn default() -> Self {
        Self {
            registry_type: "memory".to_string(),
            discovery_timeout: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(10),
            service_ttl: Duration::from_secs(300),
        }
    }
}

/// Orchestration configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrchestrationConfig {
    /// Maximum concurrent services
    pub max_concurrent_services: usize,
    /// Service startup timeout
    pub startup_timeout: Duration,
    /// Service shutdown timeout
    pub shutdown_timeout: Duration,
    /// Dependency resolution timeout
    pub dependency_timeout: Duration,
    /// Restart policy
    pub restart_policy: RestartPolicy,
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        Self {
            max_concurrent_services: 100,
            startup_timeout: Duration::from_secs(60),
            shutdown_timeout: Duration::from_secs(30),
            dependency_timeout: Duration::from_secs(10),
            restart_policy: RestartPolicy::OnFailure,
        }
    }
}

/// Restart policy
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RestartPolicy {
    Never,
    OnFailure,
    Always,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HealthCheckConfig {
    /// Enable health checks
    pub enabled: bool,
    /// Health check interval
    pub interval: Duration,
    /// Health check timeout
    pub timeout: Duration,
    /// Failure threshold
    pub failure_threshold: usize,
    /// Success threshold
    pub success_threshold: usize,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
            failure_threshold: 3,
            success_threshold: 2,
        }
    }
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DatabaseConfig {
    /// Primary database configuration
    pub primary: DatabaseConnectionConfig,
    /// Replica database configuration
    pub replicas: Vec<DatabaseConnectionConfig>,
    /// Connection pool configuration
    pub pool: ConnectionPoolConfig,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            primary: DatabaseConnectionConfig::default(),
            replicas: Vec::new(),
            pool: ConnectionPoolConfig::default(),
        }
    }
}

/// Database connection configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DatabaseConnectionConfig {
    /// Database type
    pub db_type: String,
    /// Connection URL
    pub url: String,
    /// Maximum connections
    pub max_connections: u32,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Query timeout
    pub query_timeout: Duration,
}

impl Default for DatabaseConnectionConfig {
    fn default() -> Self {
        Self {
            db_type: "surrealdb".to_string(),
            url: "crucible.db".to_string(),
            max_connections: 10,
            connection_timeout: Duration::from_secs(30),
            query_timeout: Duration::from_secs(60),
        }
    }
}

/// Connection pool configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConnectionPoolConfig {
    /// Minimum connections
    pub min_connections: u32,
    /// Maximum connections
    pub max_connections: u32,
    /// Connection idle timeout
    pub idle_timeout: Duration,
    /// Connection lifetime
    pub max_lifetime: Duration,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            min_connections: 1,
            max_connections: 10,
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
        }
    }
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkConfig {
    /// HTTP server configuration
    pub http: HttpConfig,
    /// gRPC server configuration
    pub grpc: GrpcConfig,
    /// WebSocket configuration
    pub websocket: WebSocketConfig,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            http: HttpConfig::default(),
            grpc: GrpcConfig::default(),
            websocket: WebSocketConfig::default(),
        }
    }
}

/// HTTP server configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HttpConfig {
    /// Bind address
    pub bind_address: String,
    /// Bind port
    pub port: u16,
    /// Enable TLS
    pub tls_enabled: bool,
    /// TLS certificate path
    pub tls_cert_path: Option<String>,
    /// TLS private key path
    pub tls_key_path: Option<String>,
    /// Request timeout
    pub request_timeout: Duration,
    /// Maximum request size
    pub max_request_size: usize,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 8080,
            tls_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
            request_timeout: Duration::from_secs(30),
            max_request_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// gRPC server configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GrpcConfig {
    /// Bind address
    pub bind_address: String,
    /// Bind port
    pub port: u16,
    /// Enable TLS
    pub tls_enabled: bool,
    /// TLS certificate path
    pub tls_cert_path: Option<String>,
    /// TLS private key path
    pub tls_key_path: Option<String>,
    /// Maximum message size
    pub max_message_size: usize,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 9090,
            tls_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
            max_message_size: 4 * 1024 * 1024, // 4MB
        }
    }
}

/// WebSocket configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WebSocketConfig {
    /// Bind address
    pub bind_address: String,
    /// Bind port
    pub port: u16,
    /// Enable compression
    pub compression_enabled: bool,
    /// Ping interval
    pub ping_interval: Duration,
    /// Pong timeout
    pub pong_timeout: Duration,
    /// Maximum message size
    pub max_message_size: usize,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 8081,
            compression_enabled: true,
            ping_interval: Duration::from_secs(30),
            pong_timeout: Duration::from_secs(10),
            max_message_size: 1024 * 1024, // 1MB
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LoggingConfig {
    /// Log level
    pub level: String,
    /// Log format
    pub format: String,
    /// Enable file logging
    pub file_enabled: bool,
    /// Log file path
    pub file_path: Option<String>,
    /// Log rotation enabled
    pub rotation_enabled: bool,
    /// Maximum log file size
    pub max_file_size: Option<u64>,
    /// Maximum log files to keep
    pub max_files: Option<u32>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "json".to_string(),
            file_enabled: false,
            file_path: None,
            rotation_enabled: false,
            max_file_size: Some(100 * 1024 * 1024), // 100MB
            max_files: Some(10),
        }
    }
}

/// Feature configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FeatureConfig {
    /// Enable metrics
    pub metrics_enabled: bool,
    /// Enable health checks
    pub health_checks_enabled: bool,
    /// Enable hot reload
    pub hot_reload_enabled: bool,
    /// Enable service orchestration
    pub orchestration_enabled: bool,
    /// Custom feature flags
    pub custom: HashMap<String, bool>,
}

impl Default for FeatureConfig {
    fn default() -> Self {
        let mut custom = HashMap::new();
        custom.insert("development".to_string(), false);
        custom.insert("debug".to_string(), false);

        Self {
            metrics_enabled: true,
            health_checks_enabled: true,
            hot_reload_enabled: false,
            orchestration_enabled: true,
            custom,
        }
    }
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PerformanceConfig {
    /// Worker thread count
    pub worker_threads: Option<usize>,
    /// Max blocking threads
    pub max_blocking_threads: usize,
    /// Buffer sizes for various channels
    pub buffer_sizes: BufferSizes,
    /// Cache configuration
    pub cache: CacheConfig,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            worker_threads: None, // Use num_cpus
            max_blocking_threads: 512,
            buffer_sizes: BufferSizes::default(),
            cache: CacheConfig::default(),
        }
    }
}

/// Buffer sizes for internal channels
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BufferSizes {
    /// Event channel buffer size
    pub events: usize,
    /// Command channel buffer size
    pub commands: usize,
    /// Response channel buffer size
    pub responses: usize,
}

impl Default for BufferSizes {
    fn default() -> Self {
        Self {
            events: 1000,
            commands: 100,
            responses: 100,
        }
    }
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CacheConfig {
    /// Enable caching
    pub enabled: bool,
    /// Cache TTL
    pub ttl: Duration,
    /// Maximum cache size
    pub max_size: usize,
    /// Cache eviction policy
    pub eviction_policy: String,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl: Duration::from_secs(300), // 5 minutes
            max_size: 10000,
            eviction_policy: "lru".to_string(),
        }
    }
}

/// Configuration manager
#[derive(Debug)]
pub struct ConfigManager {
    config: Arc<RwLock<CrucibleConfig>>,
    change_notifier: broadcast::Sender<ConfigChange>,
    _file_watcher: Option<tokio::task::JoinHandle<()>>,
}

impl ConfigManager {
    /// Create a new configuration manager
    pub async fn new() -> Result<Self> {
        Self::with_config_path(None).await
    }

    /// Create a configuration manager with a specific config path
    pub async fn with_config_path(config_path: Option<PathBuf>) -> Result<Self> {
        let config = Self::load_config(config_path.as_deref()).await?;
        let config = Arc::new(RwLock::new(config));
        let (change_notifier, _) = broadcast::channel(100);

        #[cfg(feature = "hot-reload")]
        let file_watcher = if config.read().await.features.hot_reload_enabled {
            Some(Self::start_file_watcher(
                config_path.unwrap_or_else(|| PathBuf::from("crucible.toml")),
                change_notifier.clone(),
                config.clone(),
            )?)
        } else {
            None
        };

        #[cfg(not(feature = "hot-reload"))]
        let file_watcher = None;

        Ok(Self {
            config,
            change_notifier,
            _file_watcher: file_watcher,
        })
    }

    /// Load configuration from file
    async fn load_config(config_path: Option<&Path>) -> Result<CrucibleConfig> {
        let config_path = config_path.unwrap_or(Path::new("crucible.toml"));

        if !config_path.exists() {
            info!(
                "Configuration file not found, using defaults: {:?}",
                config_path
            );
            return Ok(CrucibleConfig::default());
        }

        let config_content = tokio::fs::read_to_string(config_path)
            .await
            .context("Failed to read configuration file")?;

        let config: CrucibleConfig =
            toml::from_str(&config_content).context("Failed to parse configuration")?;

        // Validate configuration
        Self::validate_config(&config)?;

        info!("Loaded configuration from: {:?}", config_path);
        debug!("Configuration: {:#?}", config);

        Ok(config)
    }

    /// Validate configuration
    fn validate_config(config: &CrucibleConfig) -> Result<()> {
        // Validate network ports
        if config.network.http.port == config.network.grpc.port {
            return Err(ConfigError::ValidationFailed(
                "HTTP and gRPC ports cannot be the same".to_string(),
            )
            .into());
        }

        if config.network.http.port == config.network.websocket.port {
            return Err(ConfigError::ValidationFailed(
                "HTTP and WebSocket ports cannot be the same".to_string(),
            )
            .into());
        }

        // Validate database configuration
        if config.database.primary.url.is_empty() {
            return Err(ConfigError::RequiredMissing("database.primary.url".to_string()).into());
        }

        // Validate service configuration
        if config.services.orchestration.max_concurrent_services == 0 {
            return Err(ConfigError::ValidationFailed(
                "services.orchestration.max_concurrent_services must be greater than 0".to_string(),
            )
            .into());
        }

        Ok(())
    }

    /// Start file watcher for hot reload
    #[allow(dead_code)]
    fn start_file_watcher(
        config_path: PathBuf,
        change_notifier: broadcast::Sender<ConfigChange>,
        config: Arc<RwLock<CrucibleConfig>>,
    ) -> Result<tokio::task::JoinHandle<()>> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Err(e) = tx.blocking_send(res) {
                error!("Failed to send file change event: {}", e);
            }
        })?;

        watcher.watch(&config_path, notify::RecursiveMode::NonRecursive)?;

        let handle = tokio::spawn(async move {
            info!("Started configuration file watcher for: {:?}", config_path);

            while let Some(res) = rx.recv().await {
                match res {
                    Ok(event) => {
                        debug!("Configuration file changed: {:?}", event);

                        // Debounce rapid changes
                        tokio::time::sleep(Duration::from_millis(500)).await;

                        match Self::load_config(Some(&config_path)).await {
                            Ok(new_config) => {
                                let old_config = config.read().await.clone();

                                if let Err(e) = Self::validate_config(&new_config) {
                                    error!("Invalid configuration after reload: {}", e);
                                    continue;
                                }

                                *config.write().await = new_config.clone();

                                // Notify subscribers
                                let change = ConfigChange {
                                    path: "root".to_string(),
                                    old_value: Some(
                                        serde_json::to_value(old_config).unwrap_or_default(),
                                    ),
                                    new_value: serde_json::to_value(new_config).unwrap_or_default(),
                                };

                                if let Err(e) = change_notifier.send(change) {
                                    debug!("No subscribers for configuration change: {}", e);
                                }

                                info!("Configuration hot-reloaded successfully");
                            }
                            Err(e) => {
                                error!("Failed to reload configuration: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("File watcher error: {}", e);
                    }
                }
            }
        });

        Ok(handle)
    }

    /// Get current configuration
    pub async fn get(&self) -> CrucibleConfig {
        self.config.read().await.clone()
    }

    /// Get specific configuration section
    pub async fn get_section<T>(&self, section_fn: impl FnOnce(&CrucibleConfig) -> &T) -> T
    where
        T: Clone,
    {
        let config = self.config.read().await;
        section_fn(&*config).clone()
    }

    /// Subscribe to configuration changes
    pub fn subscribe(&self) -> broadcast::Receiver<ConfigChange> {
        self.change_notifier.subscribe()
    }

    /// Update configuration section
    pub async fn update_section<F, T>(&self, update_fn: F) -> Result<()>
    where
        F: FnOnce(&mut CrucibleConfig) -> &mut T,
        T: Clone,
    {
        let mut config = self.config.write().await;
        update_fn(&mut *config);

        // Validate after update
        Self::validate_config(&config)?;

        // Notify subscribers
        let change = ConfigChange {
            path: "manual_update".to_string(),
            old_value: None,
            new_value: serde_json::to_value(&*config).unwrap_or_default(),
        };

        if let Err(e) = self.change_notifier.send(change) {
            debug!("No subscribers for configuration change: {}", e);
        }

        info!("Configuration updated manually");
        Ok(())
    }

    /// Generate JSON schema for configuration
    pub fn generate_schema() -> serde_json::Value {
        use schemars::SchemaGenerator;
        SchemaGenerator::default()
            .into_root_schema_for::<CrucibleConfig>()
            .into()
    }

    /// Export current configuration to TOML
    pub async fn export_toml(&self) -> Result<String> {
        let config = self.config.read().await;
        toml::to_string_pretty(&*config).context("Failed to serialize configuration to TOML")
    }

    /// Import configuration from TOML string
    pub async fn import_toml(&self, toml_str: &str) -> Result<()> {
        let new_config: CrucibleConfig =
            toml::from_str(toml_str).context("Failed to parse TOML configuration")?;

        Self::validate_config(&new_config)?;

        let old_config = self.config.read().await.clone();
        *self.config.write().await = new_config.clone();

        // Notify subscribers
        let change = ConfigChange {
            path: "import".to_string(),
            old_value: Some(serde_json::to_value(old_config).unwrap_or_default()),
            new_value: serde_json::to_value(new_config).unwrap_or_default(),
        };

        if let Err(e) = self.change_notifier.send(change) {
            debug!("No subscribers for configuration change: {}", e);
        }

        info!("Configuration imported from TOML");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::fs;

    #[tokio::test]
    async fn test_default_config() {
        let config = CrucibleConfig::default();

        assert_eq!(config.services.registry.registry_type, "memory");
        assert_eq!(config.network.http.port, 8080);
        assert_eq!(config.database.primary.db_type, "surrealdb");
    }

    #[tokio::test]
    async fn test_config_validation() {
        let mut config = CrucibleConfig::default();

        // Invalid: same ports
        config.network.grpc.port = 8080;
        assert!(ConfigManager::validate_config(&config).is_err());

        // Valid: different ports
        config.network.grpc.port = 9090;
        assert!(ConfigManager::validate_config(&config).is_ok());
    }

    #[tokio::test]
    async fn test_config_load_and_save() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("test.toml");

        // Create test config
        let config = CrucibleConfig {
            network: NetworkConfig {
                http: HttpConfig {
                    port: 9091,
                    ..Default::default()
                },
                grpc: GrpcConfig {
                    port: 9090,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };

        // Save config
        let toml_str = toml::to_string_pretty(&config).unwrap();
        fs::write(&config_path, toml_str).await.unwrap();

        // Load config
        let loaded_config = ConfigManager::load_config(Some(&config_path))
            .await
            .unwrap();

        assert_eq!(loaded_config.network.http.port, 9091);
    }

    #[tokio::test]
    async fn test_config_update() {
        let manager = ConfigManager::new().await.unwrap();

        // Update HTTP port
        manager
            .update_section(|config| &mut config.network.http.port)
            .await
            .unwrap();

        let config = manager.get().await;
        assert_eq!(config.network.http.port, 8080); // Should be default
    }

    #[test]
    fn test_config_schema() {
        let schema = ConfigManager::generate_schema();

        assert!(schema.get("properties").is_some());
        assert!(schema.get("properties").unwrap().get("services").is_some());
        assert!(schema.get("properties").unwrap().get("database").is_some());
    }
}
