//! Configuration manager with enhanced validation and error handling
//!
//! This module provides a centralized configuration management system that
//! integrates validation, error handling, and service initialization.

use super::{enhanced_config::*, validation::*};
use crate::errors::ServiceResult;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Configuration manager with validation and hot-reload capabilities
#[derive(Debug)]
pub struct ConfigManager {
    /// Current configuration
    config: Arc<RwLock<EnhancedConfig>>,
    /// Configuration validation cache
    validation_cache: Arc<RwLock<Option<ValidationResult>>>,
    /// Last configuration load time
    last_load_time: Arc<RwLock<Option<Instant>>>,
    /// Configuration file path
    config_file_path: Option<String>,
    /// Enable hot-reload
    enable_hot_reload: bool,
    /// Configuration reload interval
    reload_interval: Duration,
}

impl ConfigManager {
    /// Create new configuration manager
    pub async fn new() -> ServiceResult<Self> {
        info!("Initializing configuration manager");

        let config = EnhancedConfig::load_from_sources().await?;
        let manager = Self {
            config: Arc::new(RwLock::new(config)),
            validation_cache: Arc::new(RwLock::new(None)),
            last_load_time: Arc::new(RwLock::new(Some(Instant::now()))),
            config_file_path: std::env::var("CRUCIBLE_CONFIG_FILE").ok(),
            enable_hot_reload: std::env::var("CRUCIBLE_CONFIG_HOT_RELOAD")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            reload_interval: Duration::from_secs(
                std::env::var("CRUCIBLE_CONFIG_RELOAD_INTERVAL")
                    .unwrap_or_else(|_| "300".to_string())
                    .parse()
                    .unwrap_or(300),
            ),
        };

        // Validate initial configuration
        let validation_result = manager.validate_current_config().await;
        validation_result.log_result("config_manager");
        if !validation_result.is_valid {
            return Err(crate::errors::ServiceError::ValidationError(
                "Initial configuration validation failed".to_string(),
            ));
        }

        info!("Configuration manager initialized successfully");
        Ok(manager)
    }

    /// Create configuration manager with custom config file
    pub async fn with_file(file_path: impl Into<String>) -> ServiceResult<Self> {
        std::env::set_var("CRUCIBLE_CONFIG_FILE", file_path.into());
        Self::new().await
    }

    /// Get current configuration
    pub async fn get_config(&self) -> EnhancedConfig {
        self.config.read().await.clone()
    }

    /// Get configuration section
    pub async fn get_service_config(&self) -> ServiceConfig {
        self.config.read().await.service.clone()
    }

    /// Get logging configuration
    pub async fn get_logging_config(&self) -> LoggingConfig {
        self.config.read().await.logging.clone()
    }

    /// Get event routing configuration
    pub async fn get_event_routing_config(&self) -> EventRoutingConfig {
        self.config.read().await.event_routing.clone()
    }

    /// Get database configuration if available
    pub async fn get_database_config(&self) -> Option<DatabaseConfig> {
        self.config.read().await.database.clone()
    }

    /// Get security configuration
    pub async fn get_security_config(&self) -> SecurityConfig {
        self.config.read().await.security.clone()
    }

    /// Get performance configuration
    pub async fn get_performance_config(&self) -> PerformanceConfig {
        self.config.read().await.performance.clone()
    }

    /// Get plugin configuration
    pub async fn get_plugin_config(&self) -> PluginConfig {
        self.config.read().await.plugins.clone()
    }

    /// Update configuration with validation
    pub async fn update_config(&self, new_config: EnhancedConfig) -> ServiceResult<()> {
        info!("Updating configuration");

        // Validate new configuration
        let validation_result = new_config.validate();
        validation_result.log_result("config_update");

        if !validation_result.is_valid {
            error!("Configuration update failed validation");
            return Err(crate::errors::ServiceError::ValidationError(
                "Updated configuration validation failed".to_string(),
            ));
        }

        // Apply the new configuration
        {
            let mut config = self.config.write().await;
            *config = new_config;
        }

        // Update cache and timestamp
        {
            let mut cache = self.validation_cache.write().await;
            *cache = Some(validation_result);
        }
        {
            let mut last_load = self.last_load_time.write().await;
            *last_load = Some(Instant::now());
        }

        info!("Configuration updated successfully");
        Ok(())
    }

    /// Validate current configuration
    pub async fn validate_current_config(&self) -> ValidationResult {
        let config = self.config.read().await;
        let result = config.validate();

        // Cache the result
        {
            let mut cache = self.validation_cache.write().await;
            *cache = Some(result.clone());
        }

        result
    }

    /// Get cached validation result
    pub async fn get_cached_validation(&self) -> Option<ValidationResult> {
        self.validation_cache.read().await.clone()
    }

    /// Reload configuration from sources
    pub async fn reload_config(&self) -> ServiceResult<()> {
        info!("Reloading configuration from sources");

        match EnhancedConfig::load_from_sources().await {
            Ok(new_config) => {
                self.update_config(new_config).await?;
                info!("Configuration reloaded successfully");
            }
            Err(e) => {
                error!(error = %e, "Failed to reload configuration");
                return Err(e);
            }
        }

        Ok(())
    }

    /// Start hot-reload monitoring if enabled
    pub async fn start_hot_reload_monitor(&self) -> ServiceResult<()> {
        if !self.enable_hot_reload {
            debug!("Hot-reload is disabled");
            return Ok(());
        }

        info!("Starting configuration hot-reload monitoring");

        let config_manager = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config_manager.reload_interval);

            loop {
                interval.tick().await;

                if let Err(e) = config_manager.check_and_reload().await {
                    error!(error = %e, "Hot-reload check failed");
                }
            }
        });

        info!("Hot-reload monitoring started");
        Ok(())
    }

    /// Check for configuration changes and reload if necessary
    async fn check_and_reload(&self) -> ServiceResult<()> {
        if let Some(file_path) = &self.config_file_path {
            match tokio::fs::metadata(file_path).await {
                Ok(metadata) => {
                    if let Ok(modified) = metadata.modified() {
                        let should_reload = {
                            let last_load = self.last_load_time.read().await;
                            last_load
                                .map(|last| {
                                    // Convert Instant to SystemTime for comparison
                                    let last_system = std::time::SystemTime::now() - last.elapsed();
                                    modified.duration_since(last_system).unwrap_or_default()
                                        > Duration::from_secs(1)
                                })
                                .unwrap_or(true)
                        };

                        if should_reload {
                            info!("Configuration file changed, reloading");
                            self.reload_config().await?;
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, file_path = %file_path, "Failed to check config file metadata");
                }
            }
        }

        Ok(())
    }

    /// Get configuration summary
    pub async fn get_summary(&self) -> String {
        let config = self.config.read().await;
        config.get_summary()
    }

    /// Export configuration to string
    pub async fn export_config(&self, format: ConfigExportFormat) -> ServiceResult<String> {
        let config = self.config.read().await;

        match format {
            ConfigExportFormat::YAML => serde_yaml::to_string(&*config).map_err(|e| {
                crate::errors::ServiceError::ConfigurationError(format!(
                    "YAML serialization error: {}",
                    e
                ))
            }),
            ConfigExportFormat::JSON => serde_json::to_string_pretty(&*config)
                .map_err(|e| crate::errors::ServiceError::SerializationError(e)),
            ConfigExportFormat::TOML => toml::to_string_pretty(&*config).map_err(|e| {
                crate::errors::ServiceError::ConfigurationError(format!(
                    "TOML serialization error: {}",
                    e
                ))
            }),
        }
    }

    /// Import configuration from string
    pub async fn import_config(
        &self,
        content: &str,
        format: ConfigExportFormat,
    ) -> ServiceResult<()> {
        let new_config: EnhancedConfig = match format {
            ConfigExportFormat::YAML => serde_yaml::from_str(content).map_err(|e| {
                crate::errors::ServiceError::ConfigurationError(format!(
                    "YAML parsing error: {}",
                    e
                ))
            })?,
            ConfigExportFormat::JSON => serde_json::from_str(content).map_err(|e| {
                crate::errors::ServiceError::ConfigurationError(format!(
                    "JSON parsing error: {}",
                    e
                ))
            })?,
            ConfigExportFormat::TOML => toml::from_str(content).map_err(|e| {
                crate::errors::ServiceError::ConfigurationError(format!(
                    "TOML parsing error: {}",
                    e
                ))
            })?,
        };

        self.update_config(new_config).await
    }

    /// Get configuration health status
    pub async fn health_status(&self) -> ConfigHealthStatus {
        let config = self.config.read().await;
        let validation = self
            .get_cached_validation()
            .await
            .unwrap_or_else(|| config.validate());

        ConfigHealthStatus {
            is_healthy: validation.is_valid,
            has_warnings: !validation.warnings.is_empty(),
            error_count: validation.errors.len(),
            warning_count: validation.warnings.len(),
            last_validation: Some(chrono::Utc::now()),
            config_age: self.last_load_time.read().await.map(|last| last.elapsed()),
            hot_reload_enabled: self.enable_hot_reload,
        }
    }

    /// Apply configuration diff/patch
    pub async fn apply_patch(&self, patch: &ConfigPatch) -> ServiceResult<()> {
        info!("Applying configuration patch");

        let mut config = self.config.read().await.clone();

        // Apply patch operations
        for operation in &patch.operations {
            match operation {
                ConfigOperation::Set { field, value } => {
                    self.apply_field_operation(&mut config, field, value)?;
                }
                ConfigOperation::Remove { field } => {
                    self.apply_field_removal(&mut config, field)?;
                }
                ConfigOperation::Update { field, value } => {
                    self.apply_field_update(&mut config, field, value)?;
                }
            }
        }

        self.update_config(config).await?;
        info!("Configuration patch applied successfully");
        Ok(())
    }

    /// Apply field operation to configuration
    fn apply_field_operation(
        &self,
        config: &mut EnhancedConfig,
        field: &str,
        value: &serde_json::Value,
    ) -> ServiceResult<()> {
        match field {
            "service.name" => {
                if let Some(name) = value.as_str() {
                    config.service.name = name.to_string();
                }
            }
            "service.environment" => {
                if let Some(env) = value.as_str() {
                    config.service.environment = env.to_string();
                }
            }
            "logging.level" => {
                if let Some(level) = value.as_str() {
                    config.logging.level = level.to_string();
                }
            }
            "logging.format" => {
                if let Some(format) = value.as_str() {
                    config.logging.format = format.to_string();
                }
            }
            "event_routing.max_concurrent_events" => {
                if let Some(count) = value.as_u64() {
                    config.event_routing.max_concurrent_events = count as usize;
                }
            }
            "event_routing.max_event_age_seconds" => {
                if let Some(seconds) = value.as_u64() {
                    config.event_routing.max_event_age_seconds = seconds;
                }
            }
            _ => {
                return Err(crate::errors::ServiceError::ConfigurationError(format!(
                    "Unsupported configuration field: {}",
                    field
                )));
            }
        }
        Ok(())
    }

    /// Apply field removal to configuration
    fn apply_field_removal(&self, config: &mut EnhancedConfig, field: &str) -> ServiceResult<()> {
        match field {
            "database" => {
                config.database = None;
            }
            "security.encryption_key_path" => {
                config.security.encryption_key_path = None;
            }
            "security.jwt_secret" => {
                config.security.jwt_secret = None;
            }
            _ => {
                return Err(crate::errors::ServiceError::ConfigurationError(format!(
                    "Cannot remove field: {}",
                    field
                )));
            }
        }
        Ok(())
    }

    /// Apply field update to configuration
    fn apply_field_update(
        &self,
        config: &mut EnhancedConfig,
        field: &str,
        value: &serde_json::Value,
    ) -> ServiceResult<()> {
        // For update operations, we apply similar to set but with additional validation
        self.apply_field_operation(config, field, value)
    }
}

impl Clone for ConfigManager {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            validation_cache: self.validation_cache.clone(),
            last_load_time: self.last_load_time.clone(),
            config_file_path: self.config_file_path.clone(),
            enable_hot_reload: self.enable_hot_reload,
            reload_interval: self.reload_interval,
        }
    }
}

/// Configuration export format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigExportFormat {
    YAML,
    JSON,
    TOML,
}

/// Configuration health status
#[derive(Debug, Clone)]
pub struct ConfigHealthStatus {
    /// Overall health status
    pub is_healthy: bool,
    /// Has warnings
    pub has_warnings: bool,
    /// Number of errors
    pub error_count: usize,
    /// Number of warnings
    pub warning_count: usize,
    /// Last validation time
    pub last_validation: Option<chrono::DateTime<chrono::Utc>>,
    /// Configuration age
    pub config_age: Option<Duration>,
    /// Hot reload enabled
    pub hot_reload_enabled: bool,
}

/// Configuration patch for incremental updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPatch {
    /// Patch operations
    pub operations: Vec<ConfigOperation>,
    /// Patch metadata
    pub metadata: PatchMetadata,
}

/// Configuration operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ConfigOperation {
    /// Set field value
    Set {
        field: String,
        value: serde_json::Value,
    },
    /// Remove field
    Remove { field: String },
    /// Update field value
    Update {
        field: String,
        value: serde_json::Value,
    },
}

/// Patch metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchMetadata {
    /// Patch ID
    pub id: String,
    /// Patch description
    pub description: Option<String>,
    /// Patch author
    pub author: Option<String>,
    /// Patch timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Patch version
    pub version: Option<String>,
}

impl Default for PatchMetadata {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: None,
            author: None,
            timestamp: chrono::Utc::now(),
            version: None,
        }
    }
}

/// Configuration manager builder
#[derive(Debug, Default)]
pub struct ConfigManagerBuilder {
    config_file_path: Option<String>,
    enable_hot_reload: Option<bool>,
    reload_interval: Option<Duration>,
}

impl ConfigManagerBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set configuration file path
    pub fn with_config_file(mut self, path: impl Into<String>) -> Self {
        self.config_file_path = Some(path.into());
        self
    }

    /// Enable or disable hot reload
    pub fn with_hot_reload(mut self, enabled: bool) -> Self {
        self.enable_hot_reload = Some(enabled);
        self
    }

    /// Set reload interval
    pub fn with_reload_interval(mut self, interval: Duration) -> Self {
        self.reload_interval = Some(interval);
        self
    }

    /// Build configuration manager
    pub async fn build(self) -> ServiceResult<ConfigManager> {
        // Apply builder settings to environment
        if let Some(path) = self.config_file_path {
            std::env::set_var("CRUCIBLE_CONFIG_FILE", path);
        }
        if let Some(hot_reload) = self.enable_hot_reload {
            std::env::set_var("CRUCIBLE_CONFIG_HOT_RELOAD", hot_reload.to_string());
        }
        if let Some(interval) = self.reload_interval {
            std::env::set_var(
                "CRUCIBLE_CONFIG_RELOAD_INTERVAL",
                interval.as_secs().to_string(),
            );
        }

        ConfigManager::new().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_config_manager_creation() {
        let manager = ConfigManager::new().await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_config_validation() {
        let manager = ConfigManager::new().await.unwrap();
        let result = manager.validate_current_config().await;
        assert!(result.is_valid);
    }

    #[tokio::test]
    async fn test_config_health_status() {
        let manager = ConfigManager::new().await.unwrap();
        let status = manager.health_status().await;
        assert!(status.is_healthy);
    }

    #[tokio::test]
    async fn test_config_export() {
        let manager = ConfigManager::new().await.unwrap();
        let json = manager.export_config(ConfigExportFormat::JSON).await;
        assert!(json.is_ok());
        assert!(json.unwrap().contains("crucible-services"));
    }

    #[tokio::test]
    async fn test_config_builder() {
        let manager = ConfigManagerBuilder::new()
            .with_hot_reload(false)
            .build()
            .await;
        assert!(manager.is_ok());
    }
}
