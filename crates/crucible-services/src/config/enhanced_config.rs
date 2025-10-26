//! Enhanced configuration structures with comprehensive validation
//!
//! This module provides enhanced configuration structures that integrate
//! with the validation framework for robust configuration management.

use super::validation::{
    ConfigValidator, ValidationContext, ValidationEngine, ValidationError, ValidationResult,
    ValidationRule, ValidationRuleType,
};
use crate::errors::ServiceResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Enhanced configuration with built-in validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedConfig {
    /// Service configuration
    pub service: ServiceConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
    /// Event routing configuration
    pub event_routing: EventRoutingConfig,
    /// Database configuration
    pub database: Option<DatabaseConfig>,
    /// Security configuration
    pub security: SecurityConfig,
    /// Performance configuration
    pub performance: PerformanceConfig,
    /// Plugin configuration
    pub plugins: PluginConfig,
    /// Environment-specific overrides
    #[serde(flatten)]
    pub environment_overrides: HashMap<String, serde_json::Value>,
}

impl Default for EnhancedConfig {
    fn default() -> Self {
        Self {
            service: ServiceConfig::default(),
            logging: LoggingConfig::default(),
            event_routing: EventRoutingConfig::default(),
            database: None,
            security: SecurityConfig::default(),
            performance: PerformanceConfig::default(),
            plugins: PluginConfig::default(),
            environment_overrides: HashMap::new(),
        }
    }
}

impl EnhancedConfig {
    /// Load configuration from multiple sources
    pub async fn load_from_sources() -> ServiceResult<Self> {
        info!("Loading enhanced configuration from multiple sources");

        let mut config = Self::default();

        // Load from environment variables
        config = config.apply_environment_overrides();

        // Load from config file if exists
        if let Some(file_path) = std::env::var("CRUCIBLE_CONFIG_FILE").ok().or_else(|| {
            // Try default locations
            [
                "./crucible.yaml",
                "./config/crucible.yaml",
                "~/.crucible/config.yaml",
            ]
            .iter()
            .find(|path| std::path::Path::new(path).exists())
            .map(|p| p.to_string())
        }) {
            config = config.load_from_file(&file_path).await?;
        }

        // Validate the final configuration
        config.validate().into_service_result()?;

        info!("Enhanced configuration loaded and validated successfully");
        Ok(config)
    }

    /// Apply environment variable overrides
    fn apply_environment_overrides(mut self) -> Self {
        debug!("Applying environment variable overrides");

        // Service configuration overrides
        if let Ok(name) = std::env::var("CRUCIBLE_SERVICE_NAME") {
            self.service.name = name;
        }
        if let Ok(version) = std::env::var("CRUCIBLE_SERVICE_VERSION") {
            self.service.version = version;
        }
        if let Ok(environment) = std::env::var("CRUCIBLE_ENVIRONMENT") {
            self.service.environment = environment;
        }

        // Logging configuration overrides
        if let Ok(level) = std::env::var("CRUCIBLE_LOG_LEVEL") {
            if let Ok(parsed_level) = level.parse::<tracing::Level>() {
                self.logging.level = parsed_level.to_string();
            }
        }
        if let Ok(format) = std::env::var("CRUCIBLE_LOG_FORMAT") {
            self.logging.format = format;
        }
        if let Ok(file_enabled) = std::env::var("CRUCIBLE_LOG_FILE") {
            self.logging.file_enabled = file_enabled.parse().unwrap_or(self.logging.file_enabled);
        }

        // Event routing overrides
        if let Ok(max_age) = std::env::var("CRUCIBLE_MAX_EVENT_AGE") {
            if let Ok(seconds) = max_age.parse() {
                self.event_routing.max_event_age_seconds = seconds;
            }
        }
        if let Ok(max_concurrent) = std::env::var("CRUCIBLE_MAX_CONCURRENT_EVENTS") {
            if let Ok(count) = max_concurrent.parse() {
                self.event_routing.max_concurrent_events = count;
            }
        }

        // Database configuration overrides
        if let Ok(db_url) = std::env::var("CRUCIBLE_DATABASE_URL") {
            self.database = Some(DatabaseConfig {
                url: db_url,
                ..DatabaseConfig::default()
            });
        }

        self
    }

    /// Load configuration from file
    async fn load_from_file(mut self, file_path: &str) -> ServiceResult<Self> {
        debug!(file_path = %file_path, "Loading configuration from file");

        let content = tokio::fs::read_to_string(file_path)
            .await
            .map_err(|e| crate::errors::ServiceError::IoError(e))?;

        // Try different formats based on file extension
        let extension = std::path::Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("yaml");

        match extension {
            "yaml" | "yml" => {
                let file_config: EnhancedConfig = serde_yaml::from_str(&content).map_err(|e| {
                    crate::errors::ServiceError::ConfigurationError(format!(
                        "YAML parsing error: {}",
                        e
                    ))
                })?;
                self.merge_with(file_config);
            }
            "json" => {
                let file_config: EnhancedConfig = serde_json::from_str(&content).map_err(|e| {
                    crate::errors::ServiceError::ConfigurationError(format!(
                        "JSON parsing error: {}",
                        e
                    ))
                })?;
                self.merge_with(file_config);
            }
            "toml" => {
                let file_config: EnhancedConfig = toml::from_str(&content).map_err(|e| {
                    crate::errors::ServiceError::ConfigurationError(format!(
                        "TOML parsing error: {}",
                        e
                    ))
                })?;
                self.merge_with(file_config);
            }
            _ => {
                warn!(extension = %extension, "Unsupported configuration file format, using defaults");
            }
        }

        Ok(self)
    }

    /// Merge with another configuration (file config takes precedence)
    fn merge_with(&mut self, other: EnhancedConfig) {
        // Simple merge logic - in a real implementation, this would be more sophisticated
        if other.service.name != ServiceConfig::default().name {
            self.service = other.service;
        }
        if other.logging.level != LoggingConfig::default().level {
            self.logging = other.logging;
        }
        if other.event_routing.max_event_age_seconds
            != EventRoutingConfig::default().max_event_age_seconds
        {
            self.event_routing = other.event_routing;
        }
        if other.database.is_some() {
            self.database = other.database;
        }
        if other.security.encryption_enabled != SecurityConfig::default().encryption_enabled {
            self.security = other.security;
        }
        if other.performance.max_memory_mb != PerformanceConfig::default().max_memory_mb {
            self.performance = other.performance;
        }
        if other.plugins.enabled_plugins != PluginConfig::default().enabled_plugins {
            self.plugins = other.plugins;
        }
    }

    /// Get configuration summary
    pub fn get_summary(&self) -> String {
        format!(
            "EnhancedConfig: service={}, env={}, log_level={}, max_events={}, db={}, plugins={}",
            self.service.name,
            self.service.environment,
            self.logging.level,
            self.event_routing.max_concurrent_events,
            self.database
                .as_ref()
                .map(|_d| "configured")
                .unwrap_or("none"),
            self.plugins.enabled_plugins.len()
        )
    }
}

impl ConfigValidator for EnhancedConfig {
    fn validate(&self) -> ValidationResult {
        let context = ValidationContext::new("enhanced_config");
        self.validate_with_context(context)
    }

    fn validate_with_context(&self, mut context: ValidationContext) -> ValidationResult {
        let mut result = ValidationResult::success();

        // Validate service configuration
        context = context.with_section("service");
        let service_result = self.service.validate_with_context(context.clone());
        if !service_result.is_valid {
            result.is_valid = false;
            result.errors.extend(service_result.errors);
        }
        result.warnings.extend(service_result.warnings);
        result.info.extend(service_result.info);

        // Validate logging configuration
        context = ValidationContext::new("enhanced_config").with_section("logging");
        let logging_result = self.logging.validate_with_context(context.clone());
        if !logging_result.is_valid {
            result.is_valid = false;
            result.errors.extend(logging_result.errors);
        }
        result.warnings.extend(logging_result.warnings);
        result.info.extend(logging_result.info);

        // Validate event routing configuration
        context = ValidationContext::new("enhanced_config").with_section("event_routing");
        let routing_result = self.event_routing.validate_with_context(context.clone());
        if !routing_result.is_valid {
            result.is_valid = false;
            result.errors.extend(routing_result.errors);
        }
        result.warnings.extend(routing_result.warnings);
        result.info.extend(routing_result.info);

        // Validate database configuration if present
        if let Some(database) = &self.database {
            context = ValidationContext::new("enhanced_config").with_section("database");
            let db_result = database.validate_with_context(context.clone());
            if !db_result.is_valid {
                result.is_valid = false;
                result.errors.extend(db_result.errors);
            }
            result.warnings.extend(db_result.warnings);
            result.info.extend(db_result.info);
        }

        // Validate security configuration
        context = ValidationContext::new("enhanced_config").with_section("security");
        let security_result = self.security.validate_with_context(context.clone());
        if !security_result.is_valid {
            result.is_valid = false;
            result.errors.extend(security_result.errors);
        }
        result.warnings.extend(security_result.warnings);
        result.info.extend(security_result.info);

        // Validate performance configuration
        context = ValidationContext::new("enhanced_config").with_section("performance");
        let perf_result = self.performance.validate_with_context(context.clone());
        if !perf_result.is_valid {
            result.is_valid = false;
            result.errors.extend(perf_result.errors);
        }
        result.warnings.extend(perf_result.warnings);
        result.info.extend(perf_result.info);

        // Validate plugin configuration
        context = ValidationContext::new("enhanced_config").with_section("plugins");
        let plugin_result = self.plugins.validate_with_context(context.clone());
        if !plugin_result.is_valid {
            result.is_valid = false;
            result.errors.extend(plugin_result.errors);
        }
        result.warnings.extend(plugin_result.warnings);
        result.info.extend(plugin_result.info);

        result
    }

    fn validation_rules(&self) -> Vec<ValidationRule> {
        let mut rules = vec![];
        rules.extend(self.service.validation_rules());
        rules.extend(self.logging.validation_rules());
        rules.extend(self.event_routing.validation_rules());
        if let Some(database) = &self.database {
            rules.extend(database.validation_rules());
        }
        rules.extend(self.security.validation_rules());
        rules.extend(self.performance.validation_rules());
        rules.extend(self.plugins.validation_rules());
        rules
    }
}

/// Service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Service name
    pub name: String,
    /// Service version
    pub version: String,
    /// Environment (development, staging, production)
    pub environment: String,
    /// Service description
    pub description: Option<String>,
    /// Service tags
    pub tags: Vec<String>,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            name: "crucible-services".to_string(),
            version: "0.1.0".to_string(),
            environment: "development".to_string(),
            description: None,
            tags: vec![],
        }
    }
}

impl ConfigValidator for ServiceConfig {
    fn validate(&self) -> ValidationResult {
        let context = ValidationContext::new("service_config");
        self.validate_with_context(context)
    }

    fn validate_with_context(&self, context: ValidationContext) -> ValidationResult {
        let mut result = ValidationResult::success();
        let mut engine = ValidationEngine::new();

        // Add validation rules
        engine.add_rule(
            "name",
            ValidationRule {
                field: "name".to_string(),
                rule_type: ValidationRuleType::NonEmpty,
                parameters: HashMap::new(),
                error_message: "Service name cannot be empty".to_string(),
                required: true,
            },
        );

        engine.add_rule("name", ValidationRule {
            field: "name".to_string(),
            rule_type: ValidationRuleType::Pattern(r"^[a-zA-Z][a-zA-Z0-9_-]*$".to_string()),
            parameters: HashMap::new(),
            error_message: "Service name must contain only alphanumeric characters, hyphens, and underscores".to_string(),
            required: true,
        });

        engine.add_rule(
            "environment",
            ValidationRule {
                field: "environment".to_string(),
                rule_type: ValidationRuleType::Enum(vec![
                    "development".to_string(),
                    "staging".to_string(),
                    "production".to_string(),
                ]),
                parameters: HashMap::new(),
                error_message: "Environment must be one of: development, staging, production"
                    .to_string(),
                required: true,
            },
        );

        // Convert to JSON for validation
        let config_json = serde_json::to_value(self).unwrap_or_default();
        let validation_result = engine.validate_config(&config_json, &context);

        if !validation_result.is_valid {
            result.is_valid = false;
            result.errors.extend(validation_result.errors);
        }
        result.warnings.extend(validation_result.warnings);
        result.info.extend(validation_result.info);

        result
    }

    fn validation_rules(&self) -> Vec<ValidationRule> {
        vec![
            ValidationRule {
                field: "name".to_string(),
                rule_type: ValidationRuleType::NonEmpty,
                parameters: HashMap::new(),
                error_message: "Service name is required".to_string(),
                required: true,
            },
            ValidationRule {
                field: "environment".to_string(),
                rule_type: ValidationRuleType::Enum(vec![
                    "development".to_string(),
                    "staging".to_string(),
                    "production".to_string(),
                ]),
                parameters: HashMap::new(),
                error_message: "Invalid environment".to_string(),
                required: true,
            },
        ]
    }
}

/// Enhanced logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Log format (json, text)
    #[serde(default = "default_log_format")]
    pub format: String,
    /// Enable file logging
    #[serde(default)]
    pub file_enabled: bool,
    /// Log file path
    pub file_path: Option<PathBuf>,
    /// Maximum log file size in bytes
    pub max_file_size: Option<u64>,
    /// Number of log files to retain
    pub max_files: Option<u32>,
    /// Enable console logging
    #[serde(default = "default_true")]
    pub console_enabled: bool,
    /// Component-specific log levels
    pub component_levels: HashMap<String, String>,
    /// Enable structured logging
    #[serde(default)]
    pub structured: bool,
    /// Log correlation ID field
    #[serde(default = "default_correlation_field")]
    pub correlation_field: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "json".to_string()
}

fn default_true() -> bool {
    true
}

fn default_correlation_field() -> String {
    "trace_id".to_string()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
            file_enabled: false,
            file_path: None,
            max_file_size: Some(10 * 1024 * 1024), // 10MB
            max_files: Some(5),
            console_enabled: true,
            component_levels: HashMap::new(),
            structured: true,
            correlation_field: default_correlation_field(),
        }
    }
}

impl ConfigValidator for LoggingConfig {
    fn validate(&self) -> ValidationResult {
        let context = ValidationContext::new("logging_config");
        self.validate_with_context(context)
    }

    fn validate_with_context(&self, context: ValidationContext) -> ValidationResult {
        let mut result = ValidationResult::success();
        let mut engine = ValidationEngine::new();

        // Add validation rules
        engine.add_rule(
            "level",
            ValidationRule {
                field: "level".to_string(),
                rule_type: ValidationRuleType::Enum(vec![
                    "trace".to_string(),
                    "debug".to_string(),
                    "info".to_string(),
                    "warn".to_string(),
                    "error".to_string(),
                ]),
                parameters: HashMap::new(),
                error_message: "Log level must be one of: trace, debug, info, warn, error"
                    .to_string(),
                required: true,
            },
        );

        engine.add_rule(
            "format",
            ValidationRule {
                field: "format".to_string(),
                rule_type: ValidationRuleType::Enum(vec![
                    "json".to_string(),
                    "text".to_string(),
                    "compact".to_string(),
                ]),
                parameters: HashMap::new(),
                error_message: "Log format must be one of: json, text, compact".to_string(),
                required: true,
            },
        );

        if let Some(_file_size) = self.max_file_size {
            engine.add_rule(
                "max_file_size",
                ValidationRule {
                    field: "max_file_size".to_string(),
                    rule_type: ValidationRuleType::Positive,
                    parameters: HashMap::new(),
                    error_message: "Max file size must be positive".to_string(),
                    required: false,
                },
            );
        }

        if let Some(_max_files) = self.max_files {
            engine.add_rule(
                "max_files",
                ValidationRule {
                    field: "max_files".to_string(),
                    rule_type: ValidationRuleType::Range {
                        min: Some(1.0),
                        max: Some(100.0),
                    },
                    parameters: HashMap::new(),
                    error_message: "Max files must be between 1 and 100".to_string(),
                    required: false,
                },
            );
        }

        // Convert to JSON for validation
        let config_json = serde_json::to_value(self).unwrap_or_default();
        let validation_result = engine.validate_config(&config_json, &context);

        if !validation_result.is_valid {
            result.is_valid = false;
            result.errors.extend(validation_result.errors);
        }
        result.warnings.extend(validation_result.warnings);
        result.info.extend(validation_result.info);

        // Add custom validation
        if self.file_enabled && self.file_path.is_none() {
            result = result.with_warning(ValidationError::InvalidValue {
                field: "file_enabled".to_string(),
                value: "true".to_string(),
                reason: "File logging is enabled but no file path is specified".to_string(),
                context: context.clone(),
                suggested_fix: Some("Set file_path or disable file logging".to_string()),
            });
        }

        result
    }

    fn validation_rules(&self) -> Vec<ValidationRule> {
        vec![
            ValidationRule {
                field: "level".to_string(),
                rule_type: ValidationRuleType::Enum(vec![
                    "trace".to_string(),
                    "debug".to_string(),
                    "info".to_string(),
                    "warn".to_string(),
                    "error".to_string(),
                ]),
                parameters: HashMap::new(),
                error_message: "Invalid log level".to_string(),
                required: true,
            },
            ValidationRule {
                field: "format".to_string(),
                rule_type: ValidationRuleType::Enum(vec!["json".to_string(), "text".to_string()]),
                parameters: HashMap::new(),
                error_message: "Invalid log format".to_string(),
                required: true,
            },
        ]
    }
}

/// Event routing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRoutingConfig {
    /// Maximum event age in seconds
    #[serde(default = "default_max_event_age")]
    pub max_event_age_seconds: u64,
    /// Maximum concurrent events
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_events: usize,
    /// Default routing strategy
    #[serde(default = "default_routing_strategy")]
    pub default_routing_strategy: String,
    /// Enable detailed event tracing
    #[serde(default)]
    pub enable_detailed_tracing: bool,
    /// Routing history size limit
    #[serde(default = "default_history_limit")]
    pub routing_history_limit: usize,
    /// Event buffer size
    #[serde(default = "default_buffer_size")]
    pub event_buffer_size: usize,
    /// Enable event persistence
    #[serde(default)]
    pub enable_persistence: bool,
    /// Event storage path (if persistence enabled)
    pub storage_path: Option<PathBuf>,
}

fn default_max_event_age() -> u64 {
    300 // 5 minutes
}

fn default_max_concurrent() -> usize {
    1000
}

fn default_routing_strategy() -> String {
    "type_based".to_string()
}

fn default_history_limit() -> usize {
    1000
}

fn default_buffer_size() -> usize {
    10000
}

impl Default for EventRoutingConfig {
    fn default() -> Self {
        Self {
            max_event_age_seconds: default_max_event_age(),
            max_concurrent_events: default_max_concurrent(),
            default_routing_strategy: default_routing_strategy(),
            enable_detailed_tracing: false,
            routing_history_limit: default_history_limit(),
            event_buffer_size: default_buffer_size(),
            enable_persistence: false,
            storage_path: None,
        }
    }
}

impl ConfigValidator for EventRoutingConfig {
    fn validate(&self) -> ValidationResult {
        let context = ValidationContext::new("event_routing_config");
        self.validate_with_context(context)
    }

    fn validate_with_context(&self, context: ValidationContext) -> ValidationResult {
        let mut result = ValidationResult::success();
        let mut engine = ValidationEngine::new();

        // Add validation rules
        engine.add_rule(
            "max_event_age_seconds",
            ValidationRule {
                field: "max_event_age_seconds".to_string(),
                rule_type: ValidationRuleType::Range {
                    min: Some(1.0),
                    max: Some(3600.0),
                },
                parameters: HashMap::new(),
                error_message: "Max event age must be between 1 second and 1 hour".to_string(),
                required: true,
            },
        );

        engine.add_rule(
            "max_concurrent_events",
            ValidationRule {
                field: "max_concurrent_events".to_string(),
                rule_type: ValidationRuleType::Range {
                    min: Some(1.0),
                    max: Some(100000.0),
                },
                parameters: HashMap::new(),
                error_message: "Max concurrent events must be between 1 and 100,000".to_string(),
                required: true,
            },
        );

        engine.add_rule(
            "event_buffer_size",
            ValidationRule {
                field: "event_buffer_size".to_string(),
                rule_type: ValidationRuleType::Range {
                    min: Some(100.0),
                    max: Some(1000000.0),
                },
                parameters: HashMap::new(),
                error_message: "Event buffer size must be between 100 and 1,000,000".to_string(),
                required: true,
            },
        );

        // Convert to JSON for validation
        let config_json = serde_json::to_value(self).unwrap_or_default();
        let validation_result = engine.validate_config(&config_json, &context);

        if !validation_result.is_valid {
            result.is_valid = false;
            result.errors.extend(validation_result.errors);
        }
        result.warnings.extend(validation_result.warnings);
        result.info.extend(validation_result.info);

        // Add custom validation
        if self.enable_persistence && self.storage_path.is_none() {
            result = result.with_warning(ValidationError::InvalidValue {
                field: "enable_persistence".to_string(),
                value: "true".to_string(),
                reason: "Event persistence is enabled but no storage path is specified".to_string(),
                context: context.clone(),
                suggested_fix: Some("Set storage_path or disable persistence".to_string()),
            });
        }

        result
    }

    fn validation_rules(&self) -> Vec<ValidationRule> {
        vec![
            ValidationRule {
                field: "max_event_age_seconds".to_string(),
                rule_type: ValidationRuleType::Positive,
                parameters: HashMap::new(),
                error_message: "Max event age must be positive".to_string(),
                required: true,
            },
            ValidationRule {
                field: "max_concurrent_events".to_string(),
                rule_type: ValidationRuleType::Positive,
                parameters: HashMap::new(),
                error_message: "Max concurrent events must be positive".to_string(),
                required: true,
            },
        ]
    }
}

// DatabaseConfig is now imported from crucible-config (canonical)
// We extend it with a type alias and additional validation
pub use crucible_config::DatabaseConfig as BaseDatabaseConfig;

/// Service-specific database configuration with enhanced validation
///
/// This extends the base DatabaseConfig from crucible-config with
/// service-specific features like pooling control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database connection URL
    pub url: String,
    /// Maximum number of connections
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// Connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    /// Enable connection pooling
    #[serde(default = "default_true")]
    pub enable_pooling: bool,
    /// Database type
    #[serde(default = "default_db_type")]
    pub db_type: String,
}

fn default_max_connections() -> u32 {
    10
}

fn default_timeout() -> u64 {
    30
}

fn default_db_type() -> String {
    "sqlite".to_string()
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite:crucible.db".to_string(),
            max_connections: default_max_connections(),
            timeout_seconds: default_timeout(),
            enable_pooling: true,
            db_type: default_db_type(),
        }
    }
}

impl From<BaseDatabaseConfig> for DatabaseConfig {
    fn from(base: BaseDatabaseConfig) -> Self {
        Self {
            url: base.url,
            max_connections: base.max_connections.unwrap_or(10),
            timeout_seconds: base.timeout_seconds.unwrap_or(30),
            enable_pooling: true,
            db_type: match base.db_type {
                crucible_config::DatabaseType::Sqlite => "sqlite".to_string(),
                crucible_config::DatabaseType::Postgres => "postgres".to_string(),
                crucible_config::DatabaseType::Mysql => "mysql".to_string(),
                crucible_config::DatabaseType::Surrealdb => "surrealdb".to_string(),
                crucible_config::DatabaseType::Custom(s) => s,
            },
        }
    }
}

impl ConfigValidator for DatabaseConfig {
    fn validate(&self) -> ValidationResult {
        let context = ValidationContext::new("database_config");
        self.validate_with_context(context)
    }

    fn validate_with_context(&self, context: ValidationContext) -> ValidationResult {
        let mut result = ValidationResult::success();
        let mut engine = ValidationEngine::new();

        // Add validation rules
        engine.add_rule(
            "url",
            ValidationRule {
                field: "url".to_string(),
                rule_type: ValidationRuleType::NonEmpty,
                parameters: HashMap::new(),
                error_message: "Database URL cannot be empty".to_string(),
                required: true,
            },
        );

        engine.add_rule(
            "max_connections",
            ValidationRule {
                field: "max_connections".to_string(),
                rule_type: ValidationRuleType::Range {
                    min: Some(1.0),
                    max: Some(1000.0),
                },
                parameters: HashMap::new(),
                error_message: "Max connections must be between 1 and 1000".to_string(),
                required: true,
            },
        );

        engine.add_rule(
            "db_type",
            ValidationRule {
                field: "db_type".to_string(),
                rule_type: ValidationRuleType::Enum(vec![
                    "sqlite".to_string(),
                    "postgres".to_string(),
                    "mysql".to_string(),
                ]),
                parameters: HashMap::new(),
                error_message: "Database type must be one of: sqlite, postgres, mysql".to_string(),
                required: true,
            },
        );

        // Convert to JSON for validation
        let config_json = serde_json::to_value(self).unwrap_or_default();
        let validation_result = engine.validate_config(&config_json, &context);

        if !validation_result.is_valid {
            result.is_valid = false;
            result.errors.extend(validation_result.errors);
        }
        result.warnings.extend(validation_result.warnings);
        result.info.extend(validation_result.info);

        result
    }

    fn validation_rules(&self) -> Vec<ValidationRule> {
        vec![
            ValidationRule {
                field: "url".to_string(),
                rule_type: ValidationRuleType::NonEmpty,
                parameters: HashMap::new(),
                error_message: "Database URL is required".to_string(),
                required: true,
            },
            ValidationRule {
                field: "max_connections".to_string(),
                rule_type: ValidationRuleType::Positive,
                parameters: HashMap::new(),
                error_message: "Max connections must be positive".to_string(),
                required: true,
            },
        ]
    }
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable encryption
    #[serde(default)]
    pub encryption_enabled: bool,
    /// Encryption key path
    pub encryption_key_path: Option<PathBuf>,
    /// Enable authentication
    #[serde(default)]
    pub authentication_enabled: bool,
    /// JWT secret
    pub jwt_secret: Option<String>,
    /// Token expiration in hours
    #[serde(default = "default_token_expiration")]
    pub token_expiration_hours: u64,
    /// Enable rate limiting
    #[serde(default)]
    pub rate_limiting_enabled: bool,
    /// Rate limit requests per minute
    #[serde(default = "default_rate_limit")]
    pub rate_limit_rpm: u32,
}

fn default_token_expiration() -> u64 {
    24 // 24 hours
}

fn default_rate_limit() -> u32 {
    100 // 100 requests per minute
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            encryption_enabled: false,
            encryption_key_path: None,
            authentication_enabled: false,
            jwt_secret: None,
            token_expiration_hours: default_token_expiration(),
            rate_limiting_enabled: false,
            rate_limit_rpm: default_rate_limit(),
        }
    }
}

impl ConfigValidator for SecurityConfig {
    fn validate(&self) -> ValidationResult {
        let context = ValidationContext::new("security_config");
        self.validate_with_context(context)
    }

    fn validate_with_context(&self, context: ValidationContext) -> ValidationResult {
        let mut result = ValidationResult::success();

        // Custom validation rules
        if self.encryption_enabled && self.encryption_key_path.is_none() {
            result = result.with_error(ValidationError::MissingField {
                field: "encryption_key_path".to_string(),
                context: context.clone(),
            });
        }

        if self.authentication_enabled && self.jwt_secret.is_none() {
            result = result.with_error(ValidationError::MissingField {
                field: "jwt_secret".to_string(),
                context: context.clone(),
            });
        }

        if let Some(jwt_secret) = &self.jwt_secret {
            if jwt_secret.len() < 32 {
                result = result.with_warning(ValidationError::InvalidValue {
                    field: "jwt_secret".to_string(),
                    value: "[REDACTED]".to_string(),
                    reason: "JWT secret should be at least 32 characters for security".to_string(),
                    context: context.clone(),
                    suggested_fix: Some("Use a longer, more secure secret".to_string()),
                });
            }
        }

        result
    }

    fn validation_rules(&self) -> Vec<ValidationRule> {
        vec![ValidationRule {
            field: "token_expiration_hours".to_string(),
            rule_type: ValidationRuleType::Positive,
            parameters: HashMap::new(),
            error_message: "Token expiration must be positive".to_string(),
            required: true,
        }]
    }
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Maximum memory usage in MB
    #[serde(default = "default_max_memory")]
    pub max_memory_mb: u64,
    /// Enable memory profiling
    #[serde(default)]
    pub enable_memory_profiling: bool,
    /// CPU usage threshold (percentage)
    #[serde(default = "default_cpu_threshold")]
    pub cpu_threshold_percent: f64,
    /// Enable performance monitoring
    #[serde(default)]
    pub enable_monitoring: bool,
    /// Metrics collection interval in seconds
    #[serde(default = "default_metrics_interval")]
    pub metrics_interval_seconds: u64,
}

fn default_max_memory() -> u64 {
    1024 // 1GB
}

fn default_cpu_threshold() -> f64 {
    80.0 // 80%
}

fn default_metrics_interval() -> u64 {
    60 // 1 minute
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            max_memory_mb: default_max_memory(),
            enable_memory_profiling: false,
            cpu_threshold_percent: default_cpu_threshold(),
            enable_monitoring: false,
            metrics_interval_seconds: default_metrics_interval(),
        }
    }
}

impl ConfigValidator for PerformanceConfig {
    fn validate(&self) -> ValidationResult {
        let context = ValidationContext::new("performance_config");
        self.validate_with_context(context)
    }

    fn validate_with_context(&self, context: ValidationContext) -> ValidationResult {
        let mut result = ValidationResult::success();
        let mut engine = ValidationEngine::new();

        // Add validation rules
        engine.add_rule(
            "max_memory_mb",
            ValidationRule {
                field: "max_memory_mb".to_string(),
                rule_type: ValidationRuleType::Range {
                    min: Some(64.0),
                    max: Some(32768.0),
                },
                parameters: HashMap::new(),
                error_message: "Max memory must be between 64MB and 32GB".to_string(),
                required: true,
            },
        );

        engine.add_rule(
            "cpu_threshold_percent",
            ValidationRule {
                field: "cpu_threshold_percent".to_string(),
                rule_type: ValidationRuleType::Range {
                    min: Some(1.0),
                    max: Some(100.0),
                },
                parameters: HashMap::new(),
                error_message: "CPU threshold must be between 1% and 100%".to_string(),
                required: true,
            },
        );

        // Convert to JSON for validation
        let config_json = serde_json::to_value(self).unwrap_or_default();
        let validation_result = engine.validate_config(&config_json, &context);

        if !validation_result.is_valid {
            result.is_valid = false;
            result.errors.extend(validation_result.errors);
        }
        result.warnings.extend(validation_result.warnings);
        result.info.extend(validation_result.info);

        result
    }

    fn validation_rules(&self) -> Vec<ValidationRule> {
        vec![
            ValidationRule {
                field: "max_memory_mb".to_string(),
                rule_type: ValidationRuleType::Positive,
                parameters: HashMap::new(),
                error_message: "Max memory must be positive".to_string(),
                required: true,
            },
            ValidationRule {
                field: "cpu_threshold_percent".to_string(),
                rule_type: ValidationRuleType::Range {
                    min: Some(1.0),
                    max: Some(100.0),
                },
                parameters: HashMap::new(),
                error_message: "CPU threshold must be between 1% and 100%".to_string(),
                required: true,
            },
        ]
    }
}

/// Plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Enable plugin system
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Plugin search paths
    #[serde(default)]
    pub search_paths: Vec<PathBuf>,
    /// Enabled plugins list
    #[serde(default)]
    pub enabled_plugins: Vec<String>,
    /// Disabled plugins list
    #[serde(default)]
    pub disabled_plugins: Vec<String>,
    /// Plugin configuration values
    #[serde(default)]
    pub plugin_configs: HashMap<String, serde_json::Value>,
    /// Enable plugin sandboxing
    #[serde(default = "default_true")]
    pub enable_sandboxing: bool,
    /// Plugin timeout in seconds
    #[serde(default = "default_plugin_timeout")]
    pub plugin_timeout_seconds: u64,
}

fn default_plugin_timeout() -> u64 {
    30
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            search_paths: vec![PathBuf::from("./plugins")],
            enabled_plugins: vec![],
            disabled_plugins: vec![],
            plugin_configs: HashMap::new(),
            enable_sandboxing: true,
            plugin_timeout_seconds: default_plugin_timeout(),
        }
    }
}

impl ConfigValidator for PluginConfig {
    fn validate(&self) -> ValidationResult {
        let context = ValidationContext::new("plugin_config");
        self.validate_with_context(context)
    }

    fn validate_with_context(&self, context: ValidationContext) -> ValidationResult {
        let mut result = ValidationResult::success();

        // Custom validation rules
        for plugin_name in &self.enabled_plugins {
            if self.disabled_plugins.contains(plugin_name) {
                result = result.with_error(ValidationError::InvalidValue {
                    field: "enabled_plugins".to_string(),
                    value: plugin_name.clone(),
                    reason: "Plugin is both enabled and disabled".to_string(),
                    context: context.clone(),
                    suggested_fix: Some(format!(
                        "Remove {} from either enabled or disabled plugins list",
                        plugin_name
                    )),
                });
            }
        }

        // Validate search paths exist
        for path in &self.search_paths {
            if !path.exists() {
                result = result.with_warning(ValidationError::InvalidValue {
                    field: "search_paths".to_string(),
                    value: path.display().to_string(),
                    reason: "Plugin search path does not exist".to_string(),
                    context: context.clone(),
                    suggested_fix: Some(format!(
                        "Create directory {} or remove from search paths",
                        path.display()
                    )),
                });
            }
        }

        result
    }

    fn validation_rules(&self) -> Vec<ValidationRule> {
        vec![ValidationRule {
            field: "plugin_timeout_seconds".to_string(),
            rule_type: ValidationRuleType::Positive,
            parameters: HashMap::new(),
            error_message: "Plugin timeout must be positive".to_string(),
            required: true,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enhanced_config_default() {
        let config = EnhancedConfig::default();
        assert!(config.validate().is_valid);
    }

    #[test]
    fn test_service_config_validation() {
        let mut config = ServiceConfig::default();
        assert!(config.validate().is_valid);

        // Test invalid environment
        config.environment = "invalid".to_string();
        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_logging_config_validation() {
        let mut config = LoggingConfig::default();
        assert!(config.validate().is_valid);

        // Test invalid log level
        config.level = "invalid".to_string();
        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_database_config_validation() {
        let config = DatabaseConfig::default();
        assert!(config.validate().is_valid);
    }
}
