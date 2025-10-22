//! # IPC Configuration
//!
//! Comprehensive configuration management for the plugin IPC system with support for
//! different environments, validation, and hot reloading.

use crate::plugin_ipc::{
    error::{IpcError, IpcResult},
    security::{AuthConfig, EncryptionConfig, AuthorizationConfig},
    transport::ConnectionPoolConfig,
    metrics::MetricsConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Complete IPC system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcConfig {
    /// Transport configuration
    pub transport: TransportConfig,
    /// Security configuration
    pub security: SecurityConfig,
    /// Performance configuration
    pub performance: PerformanceConfig,
    /// Monitoring configuration
    pub monitoring: MonitoringConfig,
    /// Plugin configuration
    pub plugins: PluginConfig,
    /// Environment configuration
    pub environment: EnvironmentConfig,
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            transport: TransportConfig::default(),
            security: SecurityConfig::default(),
            performance: PerformanceConfig::default(),
            monitoring: MonitoringConfig::default(),
            plugins: PluginConfig::default(),
            environment: EnvironmentConfig::default(),
        }
    }
}

impl IpcConfig {
    /// Create configuration for a specific environment
    pub fn for_environment(env: Environment) -> Self {
        match env {
            Environment::Development => Self::development(),
            Environment::Testing => Self::testing(),
            Environment::Staging => Self::staging(),
            Environment::Production => Self::production(),
        }
    }

    /// Development configuration
    pub fn development() -> Self {
        Self {
            transport: TransportConfig {
                default_type: crate::plugin_ipc::transport::TransportType::UnixDomainSocket,
                socket_path: PathBuf::from("/tmp/crucible-dev-plugins"),
                tcp_port_range: 9000..9100,
                connection_pool: ConnectionPoolConfig {
                    max_total_connections: 10,
                    max_connections_per_endpoint: 2,
                    connect_timeout_ms: 2000,
                    idle_timeout: Duration::from_secs(60),
                    health_check_interval: Duration::from_secs(10),
                    enable_connection_multiplexing: false,
                    enable_compression: false,
                    enable_encryption: false,
                },
                enable_tls: false,
                tls_config: None,
            },
            security: SecurityConfig {
                auth: AuthConfig {
                    token_type: crate::plugin_ipc::security::TokenType::ApiKey,
                    session_timeout_s: 1800, // 30 minutes
                    max_sessions_per_user: 5,
                    token_expiry_s: 3600, // 1 hour
                    refresh_enabled: false,
                    issuer: "crucible-dev".to_string(),
                    audience: "crucible-plugins-dev".to_string(),
                },
                encryption: EncryptionConfig {
                    algorithm: crate::plugin_ipc::security::EncryptionAlgorithm::Aes256Gcm,
                    key_rotation_interval_s: 1800, // 30 minutes
                    key_derivation: crate::plugin_ipc::security::KeyDerivation::HkdfSha256,
                    compression_enabled: false,
                    integrity_check: true,
                },
                authorization: AuthorizationConfig {
                    allowed_plugins: vec![], // All allowed in dev
                    blocked_plugins: vec![],
                    default_permissions: vec![],
                    rbac_enabled: false,
                    abac_enabled: false,
                    policy_engine: "allow-all".to_string(),
                },
                rate_limiting: crate::plugin_ipc::security::RateLimitConfig {
                    enabled: false,
                    requests_per_minute: 1000,
                    requests_per_hour: 10000,
                    burst_size: 100,
                    penalty_duration_s: 60,
                },
            },
            performance: PerformanceConfig {
                enable_compression: false,
                compression_level: 3,
                enable_multiplexing: false,
                max_concurrent_requests: 10,
                request_timeout_ms: 10000,
                enable_caching: true,
                cache_size: 100,
                cache_ttl: Duration::from_secs(300),
                enable_batching: true,
                batch_size: 10,
                batch_timeout_ms: 100,
            },
            monitoring: MonitoringConfig {
                metrics: MetricsConfig {
                    enable_history: true,
                    history_retention: 100,
                    collection_interval: Duration::from_secs(5),
                    export_enabled: true,
                    export_format: crate::plugin_ipc::metrics::ExportFormat::Json,
                    alerting_enabled: false,
                    alert_thresholds: crate::plugin_ipc::metrics::AlertThresholds {
                        error_rate_threshold: 0.1, // 10%
                        response_time_threshold: Duration::from_millis(5000),
                        cpu_threshold: 90.0,
                        memory_threshold: 95.0,
                        connection_threshold: 50,
                    },
                },
                tracing: TracingConfig {
                    enabled: true,
                    level: "debug".to_string(),
                    sampling_rate: 1.0,
                    export_jaeger: false,
                    export_prometheus: false,
                    log_format: LogFormat::Pretty,
                },
                health_check: HealthCheckConfig {
                    enabled: true,
                    interval: Duration::from_secs(10),
                    timeout: Duration::from_secs(5),
                    failure_threshold: 3,
                    success_threshold: 2,
                },
            },
            plugins: PluginConfig {
                auto_discovery: true,
                plugin_directories: vec![
                    PathBuf::from("./plugins"),
                    PathBuf::from("./dev-plugins"),
                ],
                max_plugins: 50,
                plugin_timeout: Duration::from_secs(30),
                enable_hot_reload: true,
                resource_limits: crate::plugin_ipc::security::ResourceLimits {
                    max_memory_mb: 512,
                    max_cpu_cores: 1.0,
                    max_disk_mb: 1024,
                    max_network_bandwidth_mbps: 10,
                    max_file_descriptors: 100,
                    max_processes: 10,
                },
                sandbox: crate::plugin_ipc::security::SandboxConfig {
                    enabled: false, // Disabled in dev
                    sandbox_type: crate::plugin_ipc::security::SandboxType::Process,
                    isolated_filesystem: false,
                    network_access: true,
                    allowed_syscalls: vec![],
                    blocked_syscalls: vec![],
                    environment_variables: HashMap::new(),
                },
            },
            environment: EnvironmentConfig {
                name: Environment::Development,
                debug_enabled: true,
                log_level: "debug".to_string(),
                profile_enabled: true,
                crash_reporting: false,
                telemetry_enabled: false,
            },
        }
    }

    /// Testing configuration
    pub fn testing() -> Self {
        let mut config = Self::development();
        config.transport.socket_path = PathBuf::from("/tmp/crucible-test-plugins");
        config.transport.tcp_port_range = 9100..9200;
        config.monitoring.tracing.level = "trace".to_string();
        config.plugins.plugin_directories = vec![PathBuf::from("./test-plugins")];
        config.environment.name = Environment::Testing;
        config
    }

    /// Staging configuration
    pub fn staging() -> Self {
        Self {
            transport: TransportConfig {
                default_type: crate::plugin_ipc::transport::TransportType::UnixDomainSocket,
                socket_path: PathBuf::from("/tmp/crucible-staging-plugins"),
                tcp_port_range: 9200..9300,
                connection_pool: ConnectionPoolConfig {
                    max_total_connections: 50,
                    max_connections_per_endpoint: 5,
                    connect_timeout_ms: 5000,
                    idle_timeout: Duration::from_secs(300),
                    health_check_interval: Duration::from_secs(30),
                    enable_connection_multiplexing: true,
                    enable_compression: true,
                    enable_encryption: true,
                },
                enable_tls: false,
                tls_config: None,
            },
            security: SecurityConfig {
                auth: AuthConfig {
                    token_type: crate::plugin_ipc::security::TokenType::Jwt,
                    session_timeout_s: 3600, // 1 hour
                    max_sessions_per_user: 20,
                    token_expiry_s: 7200, // 2 hours
                    refresh_enabled: true,
                    issuer: "crucible-staging".to_string(),
                    audience: "crucible-plugins-staging".to_string(),
                },
                encryption: EncryptionConfig {
                    algorithm: crate::plugin_ipc::security::EncryptionAlgorithm::Aes256Gcm,
                    key_rotation_interval_s: 3600, // 1 hour
                    key_derivation: crate::plugin_ipc::security::KeyDerivation::HkdfSha256,
                    compression_enabled: true,
                    integrity_check: true,
                },
                authorization: AuthorizationConfig {
                    allowed_plugins: vec![], // Configured per deployment
                    blocked_plugins: vec![],
                    default_permissions: vec![],
                    rbac_enabled: true,
                    abac_enabled: false,
                    policy_engine: "staging-policy".to_string(),
                },
                rate_limiting: crate::plugin_ipc::security::RateLimitConfig {
                    enabled: true,
                    requests_per_minute: 500,
                    requests_per_hour: 5000,
                    burst_size: 50,
                    penalty_duration_s: 300,
                },
            },
            performance: PerformanceConfig {
                enable_compression: true,
                compression_level: 6,
                enable_multiplexing: true,
                max_concurrent_requests: 100,
                request_timeout_ms: 30000,
                enable_caching: true,
                cache_size: 1000,
                cache_ttl: Duration::from_secs(600),
                enable_batching: true,
                batch_size: 25,
                batch_timeout_ms: 50,
            },
            monitoring: MonitoringConfig {
                metrics: MetricsConfig {
                    enable_history: true,
                    history_retention: 1000,
                    collection_interval: Duration::from_secs(15),
                    export_enabled: true,
                    export_format: crate::plugin_ipc::metrics::ExportFormat::Prometheus,
                    alerting_enabled: true,
                    alert_thresholds: crate::plugin_ipc::metrics::AlertThresholds::default(),
                },
                tracing: TracingConfig {
                    enabled: true,
                    level: "info".to_string(),
                    sampling_rate: 0.1,
                    export_jaeger: true,
                    export_prometheus: true,
                    log_format: LogFormat::Json,
                },
                health_check: HealthCheckConfig {
                    enabled: true,
                    interval: Duration::from_secs(30),
                    timeout: Duration::from_secs(10),
                    failure_threshold: 3,
                    success_threshold: 2,
                },
            },
            plugins: PluginConfig {
                auto_discovery: true,
                plugin_directories: vec![
                    PathBuf::from("/opt/crucible/staging/plugins"),
                ],
                max_plugins: 200,
                plugin_timeout: Duration::from_secs(60),
                enable_hot_reload: true,
                resource_limits: crate::plugin_ipc::security::ResourceLimits {
                    max_memory_mb: 2048,
                    max_cpu_cores: 2.0,
                    max_disk_mb: 5120,
                    max_network_bandwidth_mbps: 100,
                    max_file_descriptors: 500,
                    max_processes: 50,
                },
                sandbox: crate::plugin_ipc::security::SandboxConfig {
                    enabled: true,
                    sandbox_type: crate::plugin_ipc::security::SandboxType::Process,
                    isolated_filesystem: true,
                    network_access: false,
                    allowed_syscalls: vec![],
                    blocked_syscalls: vec![
                        "ptrace".to_string(),
                        "mount".to_string(),
                        "umount".to_string(),
                    ],
                    environment_variables: HashMap::new(),
                },
            },
            environment: EnvironmentConfig {
                name: Environment::Staging,
                debug_enabled: false,
                log_level: "info".to_string(),
                profile_enabled: false,
                crash_reporting: true,
                telemetry_enabled: true,
            },
        }
    }

    /// Production configuration
    pub fn production() -> Self {
        Self {
            transport: TransportConfig {
                default_type: crate::plugin_ipc::transport::TransportType::UnixDomainSocket,
                socket_path: PathBuf::from("/var/run/crucible/plugins"),
                tcp_port_range: 9300..9400,
                connection_pool: ConnectionPoolConfig {
                    max_total_connections: 200,
                    max_connections_per_endpoint: 20,
                    connect_timeout_ms: 3000,
                    idle_timeout: Duration::from_secs(600),
                    health_check_interval: Duration::from_secs(60),
                    enable_connection_multiplexing: true,
                    enable_compression: true,
                    enable_encryption: true,
                },
                enable_tls: false, // Unix sockets don't need TLS
                tls_config: None,
            },
            security: SecurityConfig {
                auth: AuthConfig {
                    token_type: crate::plugin_ipc::security::TokenType::Jwt,
                    session_timeout_s: 7200, // 2 hours
                    max_sessions_per_user: 100,
                    token_expiry_s: 14400, // 4 hours
                    refresh_enabled: true,
                    issuer: "crucible-production".to_string(),
                    audience: "crucible-plugins".to_string(),
                },
                encryption: EncryptionConfig {
                    algorithm: crate::plugin_ipc::security::EncryptionAlgorithm::Aes256Gcm,
                    key_rotation_interval_s: 7200, // 2 hours
                    key_derivation: crate::plugin_ipc::security::KeyDerivation::HkdfSha256,
                    compression_enabled: true,
                    integrity_check: true,
                },
                authorization: AuthorizationConfig {
                    allowed_plugins: vec![], // Explicitly configured
                    blocked_plugins: vec![],
                    default_permissions: vec![],
                    rbac_enabled: true,
                    abac_enabled: true,
                    policy_engine: "production-policy".to_string(),
                },
                rate_limiting: crate::plugin_ipc::security::RateLimitConfig {
                    enabled: true,
                    requests_per_minute: 1000,
                    requests_per_hour: 10000,
                    burst_size: 100,
                    penalty_duration_s: 600,
                },
            },
            performance: PerformanceConfig {
                enable_compression: true,
                compression_level: 9,
                enable_multiplexing: true,
                max_concurrent_requests: 500,
                request_timeout_ms: 60000,
                enable_caching: true,
                cache_size: 10000,
                cache_ttl: Duration::from_secs(1800),
                enable_batching: true,
                batch_size: 50,
                batch_timeout_ms: 25,
            },
            monitoring: MonitoringConfig {
                metrics: MetricsConfig {
                    enable_history: true,
                    history_retention: 10000,
                    collection_interval: Duration::from_secs(30),
                    export_enabled: true,
                    export_format: crate::plugin_ipc::metrics::ExportFormat::Prometheus,
                    alerting_enabled: true,
                    alert_thresholds: crate::plugin_ipc::metrics::AlertThresholds::default(),
                },
                tracing: TracingConfig {
                    enabled: true,
                    level: "warn".to_string(),
                    sampling_rate: 0.01,
                    export_jaeger: true,
                    export_prometheus: true,
                    log_format: LogFormat::Json,
                },
                health_check: HealthCheckConfig {
                    enabled: true,
                    interval: Duration::from_secs(30),
                    timeout: Duration::from_secs(10),
                    failure_threshold: 5,
                    success_threshold: 3,
                },
            },
            plugins: PluginConfig {
                auto_discovery: false, // Explicit registration in production
                plugin_directories: vec![
                    PathBuf::from("/opt/crucible/plugins"),
                ],
                max_plugins: 1000,
                plugin_timeout: Duration::from_secs(120),
                enable_hot_reload: false,
                resource_limits: crate::plugin_ipc::security::ResourceLimits {
                    max_memory_mb: 4096,
                    max_cpu_cores: 4.0,
                    max_disk_mb: 10240,
                    max_network_bandwidth_mbps: 1000,
                    max_file_descriptors: 1000,
                    max_processes: 100,
                },
                sandbox: crate::plugin_ipc::security::SandboxConfig {
                    enabled: true,
                    sandbox_type: crate::plugin_ipc::security::SandboxType::Container,
                    isolated_filesystem: true,
                    network_access: false,
                    allowed_syscalls: vec![],
                    blocked_syscalls: vec![
                        "ptrace".to_string(),
                        "mount".to_string(),
                        "umount".to_string(),
                        "clone".to_string(),
                        "fork".to_string(),
                        "execve".to_string(),
                    ],
                    environment_variables: HashMap::new(),
                },
            },
            environment: EnvironmentConfig {
                name: Environment::Production,
                debug_enabled: false,
                log_level: "warn".to_string(),
                profile_enabled: false,
                crash_reporting: true,
                telemetry_enabled: true,
            },
        }
    }

    /// Validate the configuration
    pub fn validate(&self) -> IpcResult<()> {
        // Validate transport configuration
        if self.transport.connection_pool.max_total_connections == 0 {
            return Err(IpcError::Configuration {
                message: "max_total_connections must be greater than 0".to_string(),
                code: crate::plugin_ipc::error::ConfigErrorCode::ValidationFailed,
                config_key: Some("transport.connection_pool.max_total_connections".to_string()),
            });
        }

        if self.transport.connection_pool.connect_timeout_ms == 0 {
            return Err(IpcError::Configuration {
                message: "connect_timeout_ms must be greater than 0".to_string(),
                code: crate::plugin_ipc::error::ConfigErrorCode::ValidationFailed,
                config_key: Some("transport.connection_pool.connect_timeout_ms".to_string()),
            });
        }

        // Validate security configuration
        if self.security.auth.session_timeout_s == 0 {
            return Err(IpcError::Configuration {
                message: "session_timeout_s must be greater than 0".to_string(),
                code: crate::plugin_ipc::error::ConfigErrorCode::ValidationFailed,
                config_key: Some("security.auth.session_timeout_s".to_string()),
            });
        }

        // Validate performance configuration
        if self.performance.max_concurrent_requests == 0 {
            return Err(IpcError::Configuration {
                message: "max_concurrent_requests must be greater than 0".to_string(),
                code: crate::plugin_ipc::error::ConfigErrorCode::ValidationFailed,
                config_key: Some("performance.max_concurrent_requests".to_string()),
            });
        }

        if self.performance.request_timeout_ms == 0 {
            return Err(IpcError::Configuration {
                message: "request_timeout_ms must be greater than 0".to_string(),
                code: crate::plugin_ipc::error::ConfigErrorCode::ValidationFailed,
                config_key: Some("performance.request_timeout_ms".to_string()),
            });
        }

        // Validate plugin configuration
        if self.plugins.max_plugins == 0 {
            return Err(IpcError::Configuration {
                message: "max_plugins must be greater than 0".to_string(),
                code: crate::plugin_ipc::error::ConfigErrorCode::ValidationFailed,
                config_key: Some("plugins.max_plugins".to_string()),
            });
        }

        Ok(())
    }

    /// Load configuration from file
    pub async fn from_file(path: &PathBuf) -> IpcResult<Self> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| IpcError::Configuration {
                message: format!("Failed to read config file: {}", e),
                code: crate::plugin_ipc::error::ConfigErrorCode::NotFound,
                config_key: Some("file_path".to_string()),
            })?;

        let config: Self = serde_yaml::from_str(&content)
            .map_err(|e| IpcError::Configuration {
                message: format!("Failed to parse config file: {}", e),
                code: crate::plugin_ipc::error::ConfigErrorCode::ParseError,
                config_key: Some("file_parse".to_string()),
            })?;

        config.validate()?;
        Ok(config)
    }

    /// Save configuration to file
    pub async fn to_file(&self, path: &PathBuf) -> IpcResult<()> {
        self.validate()?;

        let content = serde_yaml::to_string(self)
            .map_err(|e| IpcError::Configuration {
                message: format!("Failed to serialize config: {}", e),
                code: crate::plugin_ipc::error::ConfigErrorCode::ParseError,
                config_key: Some("serialize".to_string()),
            })?;

        tokio::fs::write(path, content)
            .await
            .map_err(|e| IpcError::Configuration {
                message: format!("Failed to write config file: {}", e),
                code: crate::plugin_ipc::error::ConfigErrorCode::AccessDenied,
                config_key: Some("file_write".to_string()),
            })?;

        Ok(())
    }

    /// Merge with another configuration
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            transport: other.transport.clone(),
            security: other.security.clone(),
            performance: other.performance.clone(),
            monitoring: other.monitoring.clone(),
            plugins: other.plugins.clone(),
            environment: other.environment.clone(),
        }
    }
}

/// Transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub default_type: crate::plugin_ipc::transport::TransportType,
    pub socket_path: PathBuf,
    pub tcp_port_range: std::ops::Range<u16>,
    pub connection_pool: ConnectionPoolConfig,
    pub enable_tls: bool,
    pub tls_config: Option<TlsConfig>,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            default_type: crate::plugin_ipc::transport::TransportType::UnixDomainSocket,
            socket_path: PathBuf::from("/tmp/crucible-plugins"),
            tcp_port_range: 9000..10000,
            connection_pool: ConnectionPoolConfig::default(),
            enable_tls: false,
            tls_config: None,
        }
    }
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    pub ca_path: Option<PathBuf>,
    pub verify_clients: bool,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub auth: AuthConfig,
    pub encryption: EncryptionConfig,
    pub authorization: AuthorizationConfig,
    pub rate_limiting: crate::plugin_ipc::security::RateLimitConfig,
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub enable_compression: bool,
    pub compression_level: u32,
    pub enable_multiplexing: bool,
    pub max_concurrent_requests: u32,
    pub request_timeout_ms: u64,
    pub enable_caching: bool,
    pub cache_size: usize,
    pub cache_ttl: Duration,
    pub enable_batching: bool,
    pub batch_size: usize,
    pub batch_timeout_ms: u64,
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub metrics: MetricsConfig,
    pub tracing: TracingConfig,
    pub health_check: HealthCheckConfig,
}

/// Tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    pub enabled: bool,
    pub level: String,
    pub sampling_rate: f64,
    pub export_jaeger: bool,
    pub export_prometheus: bool,
    pub log_format: LogFormat,
}

/// Log format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    Pretty,
    Json,
    Compact,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    pub enabled: bool,
    pub interval: Duration,
    pub timeout: Duration,
    pub failure_threshold: u32,
    pub success_threshold: u32,
}

/// Plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub auto_discovery: bool,
    pub plugin_directories: Vec<PathBuf>,
    pub max_plugins: u32,
    pub plugin_timeout: Duration,
    pub enable_hot_reload: bool,
    pub resource_limits: crate::plugin_ipc::security::ResourceLimits,
    pub sandbox: crate::plugin_ipc::security::SandboxConfig,
}

/// Environment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    pub name: Environment,
    pub debug_enabled: bool,
    pub log_level: String,
    pub profile_enabled: bool,
    pub crash_reporting: bool,
    pub telemetry_enabled: bool,
}

/// Environment type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Environment {
    Development,
    Testing,
    Staging,
    Production,
}

/// Configuration loader with hot reload support
pub struct ConfigLoader {
    config_path: PathBuf,
    config: Arc<tokio::sync::RwLock<IpcConfig>>,
    watch_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ConfigLoader {
    /// Create a new configuration loader
    pub async fn new(config_path: PathBuf) -> IpcResult<Self> {
        let config = IpcConfig::from_file(&config_path).await?;
        Ok(Self {
            config_path,
            config: Arc::new(tokio::sync::RwLock::new(config)),
            watch_handle: None,
        })
    }

    /// Get the current configuration
    pub async fn get(&self) -> IpcConfig {
        self.config.read().await.clone()
    }

    /// Enable hot reloading
    pub async fn enable_hot_reload(&mut self) -> IpcResult<()> {
        let config_path = self.config_path.clone();
        let config = self.config.clone();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            let mut last_modified = get_file_modified_time(&config_path).await;

            loop {
                interval.tick().await;

                if let Ok(current_modified) = get_file_modified_time(&config_path).await {
                    if current_modified > last_modified {
                        info!("Configuration file changed, reloading...");
                        match IpcConfig::from_file(&config_path).await {
                            Ok(new_config) => {
                                *config.write().await = new_config;
                                info!("Configuration reloaded successfully");
                            }
                            Err(e) => {
                                error!("Failed to reload configuration: {}", e);
                            }
                        }
                        last_modified = current_modified;
                    }
                }
            }
        });

        self.watch_handle = Some(handle);
        Ok(())
    }

    /// Stop hot reloading
    pub async fn stop_hot_reload(&mut self) {
        if let Some(handle) = self.watch_handle.take() {
            handle.abort();
        }
    }
}

async fn get_file_modified_time(path: &PathBuf) -> IpcResult<std::time::SystemTime> {
    tokio::fs::metadata(path)
        .await
        .map_err(|_| IpcError::Configuration {
            message: "Failed to get file metadata".to_string(),
            code: crate::plugin_ipc::error::ConfigErrorCode::NotFound,
            config_key: None,
        })?
        .modified()
        .map_err(|_| IpcError::Configuration {
            message: "Failed to get file modification time".to_string(),
            code: crate::plugin_ipc::error::ConfigErrorCode::AccessDenied,
            config_key: None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = IpcConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.transport.default_type, crate::plugin_ipc::transport::TransportType::UnixDomainSocket);
    }

    #[test]
    fn test_development_config() {
        let config = IpcConfig::development();
        assert!(config.validate().is_ok());
        assert_eq!(config.environment.name, Environment::Development);
        assert!(config.environment.debug_enabled);
        assert!(!config.plugins.sandbox.enabled);
    }

    #[test]
    fn test_production_config() {
        let config = IpcConfig::production();
        assert!(config.validate().is_ok());
        assert_eq!(config.environment.name, Environment::Production);
        assert!(!config.environment.debug_enabled);
        assert!(config.plugins.sandbox.enabled);
        assert_eq!(config.plugins.sandbox.sandbox_type, crate::plugin_ipc::security::SandboxType::Container);
    }

    #[test]
    fn test_config_validation() {
        let mut config = IpcConfig::default();

        // Test invalid max_total_connections
        config.transport.connection_pool.max_total_connections = 0;
        assert!(config.validate().is_err());

        // Fix it
        config.transport.connection_pool.max_total_connections = 10;
        assert!(config.validate().is_ok());

        // Test invalid session_timeout
        config.security.auth.session_timeout_s = 0;
        assert!(config.validate().is_err());

        // Fix it
        config.security.auth.session_timeout_s = 3600;
        assert!(config.validate().is_ok());
    }

    #[tokio::test]
    async fn test_config_serialization() {
        let config = IpcConfig::development();
        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: IpcConfig = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(config.environment.name, parsed.environment.name);
        assert_eq!(config.transport.socket_path, parsed.transport.socket_path);
    }

    #[tokio::test]
    async fn test_config_file_operations() {
        let config = IpcConfig::testing();
        let temp_path = std::env::temp_dir().join("test_config.yaml");

        // Write config to file
        config.to_file(&temp_path).await.unwrap();

        // Read config from file
        let loaded = IpcConfig::from_file(&temp_path).await.unwrap();

        assert_eq!(config.environment.name, loaded.environment.name);

        // Clean up
        let _ = tokio::fs::remove_file(&temp_path).await;
    }

    #[test]
    fn test_environment_config() {
        let envs = [
            Environment::Development,
            Environment::Testing,
            Environment::Staging,
            Environment::Production,
        ];

        for env in envs {
            let config = IpcConfig::for_environment(env);
            assert_eq!(config.environment.name, env);
            assert!(config.validate().is_ok());
        }
    }

    #[test]
    fn test_tls_config() {
        let tls_config = TlsConfig {
            cert_path: PathBuf::from("/path/to/cert.pem"),
            key_path: PathBuf::from("/path/to/key.pem"),
            ca_path: Some(PathBuf::from("/path/to/ca.pem")),
            verify_clients: true,
        };

        let serialized = serde_json::to_value(&tls_config).unwrap();
        assert_eq!(serialized["verify_clients"], true);
        assert!(serialized["ca_path"].is_object());
    }

    #[test]
    fn test_log_format() {
        let formats = vec![
            LogFormat::Pretty,
            LogFormat::Json,
            LogFormat::Compact,
        ];

        for format in formats {
            let serialized = serde_json::to_string(&format).unwrap();
            let deserialized: LogFormat = serde_json::from_str(&serialized).unwrap();
            assert_eq!(format, deserialized);
        }
    }
}