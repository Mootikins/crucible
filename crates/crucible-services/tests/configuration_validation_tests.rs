//! Comprehensive unit tests for configuration validation and error handling
//!
//! This test suite covers all aspects of the Phase 7.3 implementation:
//! - Configuration validation
//! - Error handling and recovery
//! - Configuration management
//! - Integration with existing services

use crucible_services::config::{
    EnhancedConfig, ServiceConfig, LoggingConfig, EventRoutingConfig,
    DatabaseConfig, SecurityConfig, PerformanceConfig, PluginConfig,
    ValidationEngine, ValidationRule, ValidationRuleType, ValidationContext,
    ValidationError, ValidationResult, ConfigManager, ConfigManagerBuilder,
    ConfigErrorHandler, RecoveryStrategy, ErrorHandlingResult,
};
use crucible_services::errors::ServiceResult;
use std::collections::HashMap;
use tokio_test;

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn test_validation_context_creation() {
        let context = ValidationContext::new("test_config.yaml")
            .with_section("logging")
            .with_field("level")
            .with_metadata("source", "file")
            .with_metadata("environment", "test");

        assert_eq!(context.source, "test_config.yaml");
        assert_eq!(context.section, Some("logging".to_string()));
        assert_eq!(context.field_path, vec!["level"]);
        assert_eq!(context.metadata.get("source"), Some(&"file".to_string()));
        assert_eq!(context.metadata.get("environment"), Some(&"test".to_string()));
        assert_eq!(context.full_field_path(), "level");
    }

    #[test]
    fn test_validation_error_types() {
        // Test missing field error
        let missing_error = ValidationError::MissingField {
            field: "required_field".to_string(),
            context: ValidationContext::new("test"),
        };
        assert_eq!(missing_error.severity(), crucible_services::config::ValidationSeverity::Error);
        assert!(!missing_error.is_recoverable());
        assert_eq!(missing_error.field(), Some("required_field"));

        // Test invalid value error with suggested fix
        let invalid_error = ValidationError::InvalidValue {
            field: "log_level".to_string(),
            value: "invalid_level".to_string(),
            reason: "Not a valid log level".to_string(),
            context: ValidationContext::new("test"),
            suggested_fix: Some("Use 'info', 'debug', 'warn', or 'error'".to_string()),
        };
        assert_eq!(invalid_error.severity(), crucible_services::config::ValidationSeverity::Error);
        assert!(invalid_error.is_recoverable());
        assert!(invalid_error.description().contains("suggested fix"));

        // Test constraint violation
        let constraint_error = ValidationError::ConstraintViolation {
            field: "max_connections".to_string(),
            constraint: "range".to_string(),
            details: "Value must be between 1 and 100".to_string(),
            context: ValidationContext::new("test"),
        };
        assert_eq!(constraint_error.severity(), crucible_services::config::ValidationSeverity::Warning);

        // Test multiple errors
        let multiple_errors = ValidationError::MultipleErrors {
            count: 3,
            errors: vec![
                missing_error.clone(),
                invalid_error.clone(),
                constraint_error.clone(),
            ],
        };
        assert_eq!(multiple_errors.severity(), crucible_services::config::ValidationSeverity::Error);
    }

    #[test]
    fn test_validation_result() {
        // Test successful result
        let success = ValidationResult::success();
        assert!(success.is_valid);
        assert!(!success.has_issues());
        assert!(success.errors.is_empty());
        assert!(success.warnings.is_empty());

        // Test result with errors
        let error = ValidationError::MissingField {
            field: "test_field".to_string(),
            context: ValidationContext::new("test"),
        };
        let error_result = ValidationResult::with_errors(vec![error]);
        assert!(!error_result.is_valid);
        assert!(error_result.has_issues());
        assert_eq!(error_result.errors.len(), 1);

        // Test result with warning
        let warning = ValidationError::InvalidValue {
            field: "optional_field".to_string(),
            value: "suboptimal".to_string(),
            reason: "Value is not optimal".to_string(),
            context: ValidationContext::new("test"),
            suggested_fix: Some("Use 'optimal' instead".to_string()),
        };
        let warning_result = ValidationResult::success().with_warning(warning);
        assert!(warning_result.is_valid); // Warnings don't make it invalid
        assert!(warning_result.has_issues());
        assert_eq!(warning_result.warnings.len(), 1);
    }

    #[test]
    fn test_validation_engine() {
        let mut engine = ValidationEngine::new();

        // Add required field rule
        engine.add_rule("name", ValidationRule {
            field: "name".to_string(),
            rule_type: ValidationRuleType::Required,
            parameters: HashMap::new(),
            error_message: "Name is required".to_string(),
            required: true,
        });

        // Add pattern rule
        engine.add_rule("name", ValidationRule {
            field: "name".to_string(),
            rule_type: ValidationRuleType::Pattern(r"^[a-zA-Z][a-zA-Z0-9_-]*$".to_string()),
            parameters: HashMap::new(),
            error_message: "Name must be alphanumeric".to_string(),
            required: true,
        });

        // Add enum rule
        engine.add_rule("environment", ValidationRule {
            field: "environment".to_string(),
            rule_type: ValidationRuleType::Enum(vec![
                "development".to_string(),
                "staging".to_string(),
                "production".to_string(),
            ]),
            parameters: HashMap::new(),
            error_message: "Invalid environment".to_string(),
            required: true,
        });

        // Test valid configuration
        let valid_config = serde_json::json!({
            "name": "test-service",
            "environment": "development"
        });

        let context = ValidationContext::new("test");
        let result = engine.validate_config(&valid_config, &context);
        assert!(result.is_valid);

        // Test invalid configuration
        let invalid_config = serde_json::json!({
            "name": "invalid name with spaces",
            "environment": "invalid_env"
        });

        let result = engine.validate_config(&invalid_config, &context);
        assert!(!result.is_valid);
        assert!(result.errors.len() >= 2); // Both name and environment should fail
    }

    #[test]
    fn test_validation_rule_types() {
        let mut engine = ValidationEngine::new();

        // Test range rule
        engine.add_rule("max_connections", ValidationRule {
            field: "max_connections".to_string(),
            rule_type: ValidationRuleType::Range { min: Some(1.0), max: Some(100.0) },
            parameters: HashMap::new(),
            error_message: "Must be between 1 and 100".to_string(),
            required: true,
        });

        // Test positive rule
        engine.add_rule("timeout", ValidationRule {
            field: "timeout".to_string(),
            rule_type: ValidationRuleType::Positive,
            parameters: HashMap::new(),
            error_message: "Must be positive".to_string(),
            required: true,
        });

        // Test non-empty rule
        engine.add_rule("description", ValidationRule {
            field: "description".to_string(),
            rule_type: ValidationRuleType::NonEmpty,
            parameters: HashMap::new(),
            error_message: "Cannot be empty".to_string(),
            required: true,
        });

        let config = serde_json::json!({
            "max_connections": 50,
            "timeout": 30,
            "description": "Test description"
        });

        let context = ValidationContext::new("test");
        let result = engine.validate_config(&config, &context);
        assert!(result.is_valid);

        // Test invalid values
        let invalid_config = serde_json::json!({
            "max_connections": 150, // Over max
            "timeout": -5, // Negative
            "description": "   " // Empty after trim
        });

        let result = engine.validate_config(&invalid_config, &context);
        assert!(!result.is_valid);
        assert_eq!(result.errors.len(), 3);
    }
}

#[cfg(test)]
mod config_validation_tests {
    use super::*;

    #[test]
    fn test_service_config_validation() {
        // Test valid configuration
        let valid_config = ServiceConfig {
            name: "crucible-service".to_string(),
            version: "1.0.0".to_string(),
            environment: "development".to_string(),
            description: Some("Test service".to_string()),
            tags: vec!["test".to_string()],
        };

        let result = valid_config.validate();
        assert!(result.is_valid);

        // Test invalid environment
        let mut invalid_config = valid_config.clone();
        invalid_config.environment = "invalid".to_string();
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Test empty name
        let mut invalid_config = valid_config.clone();
        invalid_config.name = "".to_string();
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Test invalid name format
        let mut invalid_config = valid_config.clone();
        invalid_config.name = "invalid name with spaces".to_string();
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_logging_config_validation() {
        // Test valid configuration
        let valid_config = LoggingConfig {
            level: "info".to_string(),
            format: "json".to_string(),
            file_enabled: true,
            file_path: Some("/var/log/crucible.log".into()),
            max_file_size: Some(10 * 1024 * 1024),
            max_files: Some(5),
            console_enabled: true,
            component_levels: HashMap::new(),
            structured: true,
            correlation_field: "trace_id".to_string(),
        };

        let result = valid_config.validate();
        assert!(result.is_valid);

        // Test invalid log level
        let mut invalid_config = valid_config.clone();
        invalid_config.level = "invalid".to_string();
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Test invalid log format
        let mut invalid_config = valid_config.clone();
        invalid_config.format = "invalid".to_string();
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Test file enabled but no path
        let mut invalid_config = valid_config.clone();
        invalid_config.file_path = None;
        let result = invalid_config.validate();
        assert!(result.is_valid); // Still valid, but with warning
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_event_routing_config_validation() {
        // Test valid configuration
        let valid_config = EventRoutingConfig {
            max_event_age_seconds: 300,
            max_concurrent_events: 1000,
            default_routing_strategy: "type_based".to_string(),
            enable_detailed_tracing: false,
            routing_history_limit: 1000,
            event_buffer_size: 10000,
            enable_persistence: false,
            storage_path: None,
        };

        let result = valid_config.validate();
        assert!(result.is_valid);

        // Test invalid max_event_age (too small)
        let mut invalid_config = valid_config.clone();
        invalid_config.max_event_age_seconds = 0;
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Test invalid max_event_age (too large)
        let mut invalid_config = valid_config.clone();
        invalid_config.max_event_age_seconds = 4000; // Over 1 hour
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Test persistence enabled but no storage path
        let mut invalid_config = valid_config.clone();
        invalid_config.enable_persistence = true;
        let result = invalid_config.validate();
        assert!(result.is_valid); // Still valid, but with warning
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_database_config_validation() {
        // Test valid configuration
        let valid_config = DatabaseConfig {
            url: "sqlite:crucible.db".to_string(),
            max_connections: 10,
            timeout_seconds: 30,
            enable_pooling: true,
            db_type: "sqlite".to_string(),
        };

        let result = valid_config.validate();
        assert!(result.is_valid);

        // Test empty URL
        let mut invalid_config = valid_config.clone();
        invalid_config.url = "".to_string();
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Test invalid max_connections
        let mut invalid_config = valid_config.clone();
        invalid_config.max_connections = 0;
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Test invalid db_type
        let mut invalid_config = valid_config.clone();
        invalid_config.db_type = "invalid".to_string();
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_security_config_validation() {
        // Test valid configuration
        let valid_config = SecurityConfig {
            encryption_enabled: false,
            encryption_key_path: None,
            authentication_enabled: false,
            jwt_secret: None,
            token_expiration_hours: 24,
            rate_limiting_enabled: false,
            rate_limit_rpm: 100,
        };

        let result = valid_config.validate();
        assert!(result.is_valid);

        // Test encryption enabled but no key path
        let mut invalid_config = valid_config.clone();
        invalid_config.encryption_enabled = true;
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Test authentication enabled but no JWT secret
        let mut invalid_config = valid_config.clone();
        invalid_config.authentication_enabled = true;
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Test short JWT secret (warning)
        let mut config_with_warning = valid_config.clone();
        config_with_warning.authentication_enabled = true;
        config_with_warning.jwt_secret = Some("short".to_string());
        let result = config_with_warning.validate();
        assert!(!result.is_valid); // Still error because JWT secret is required
    }

    #[test]
    fn test_performance_config_validation() {
        // Test valid configuration
        let valid_config = PerformanceConfig {
            max_memory_mb: 1024,
            enable_memory_profiling: false,
            cpu_threshold_percent: 80.0,
            enable_monitoring: false,
            metrics_interval_seconds: 60,
        };

        let result = valid_config.validate();
        assert!(result.is_valid);

        // Test invalid max_memory (too small)
        let mut invalid_config = valid_config.clone();
        invalid_config.max_memory_mb = 32; // Less than 64MB minimum
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Test invalid CPU threshold
        let mut invalid_config = valid_config.clone();
        invalid_config.cpu_threshold_percent = 150.0; // Over 100%
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_plugin_config_validation() {
        // Test valid configuration
        let valid_config = PluginConfig {
            enabled: true,
            search_paths: vec!["./plugins".into()],
            enabled_plugins: vec!["plugin1".to_string()],
            disabled_plugins: vec!["plugin2".to_string()],
            plugin_configs: HashMap::new(),
            enable_sandboxing: true,
            plugin_timeout_seconds: 30,
        };

        let result = valid_config.validate();
        assert!(result.is_valid);

        // Test plugin in both enabled and disabled lists
        let mut invalid_config = valid_config.clone();
        invalid_config.disabled_plugins.push("plugin1".to_string());
        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_enhanced_config_validation() {
        // Test valid configuration
        let mut valid_config = EnhancedConfig::default();
        valid_config.database = Some(DatabaseConfig::default());

        let result = valid_config.validate();
        assert!(result.is_valid);

        // Test with invalid sub-configurations
        let mut invalid_config = EnhancedConfig::default();
        invalid_config.service.environment = "invalid".to_string();
        invalid_config.logging.level = "invalid".to_string();
        invalid_config.event_routing.max_event_age_seconds = 0;

        let result = invalid_config.validate();
        assert!(!result.is_valid);
        assert!(result.errors.len() >= 3);
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[test]
    fn test_recovery_strategy_default_value() {
        let strategy = RecoveryStrategy::DefaultValue("default_value".to_string());
        let error = ValidationError::MissingField {
            field: "test_field".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = strategy.attempt_recovery(&error);
        assert!(result.is_ok());

        let recovery = result.unwrap();
        assert_eq!(recovery.recovered_value, "default_value");
        assert_eq!(recovery.strategy, "default_value");
        assert!(recovery.description.contains("default"));
    }

    #[test]
    fn test_recovery_strategy_environment_variable() {
        // Set up environment variable
        std::env::set_var("TEST_RECOVERY_VAR", "recovered_value");

        let strategy = RecoveryStrategy::EnvironmentVariable("TEST_RECOVERY_VAR".to_string());
        let error = ValidationError::MissingField {
            field: "test_field".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = strategy.attempt_recovery(&error);
        assert!(result.is_ok());

        let recovery = result.unwrap();
        assert_eq!(recovery.recovered_value, "recovered_value");
        assert_eq!(recovery.strategy, "environment_variable");

        // Clean up
        std::env::remove_var("TEST_RECOVERY_VAR");
    }

    #[test]
    fn test_recovery_strategy_skip_field() {
        let strategy = RecoveryStrategy::SkipField;
        let error = ValidationError::MissingField {
            field: "optional_field".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = strategy.attempt_recovery(&error);
        assert!(result.is_ok());

        let recovery = result.unwrap();
        assert_eq!(recovery.recovered_value, "[SKIPPED]");
        assert_eq!(recovery.strategy, "skip_field");
    }

    #[test]
    fn test_config_error_handler() {
        let mut handler = ConfigErrorHandler::new();

        // Test handling a recoverable error
        let recoverable_error = ValidationError::InvalidValue {
            field: "logging.level".to_string(),
            value: "invalid".to_string(),
            reason: "Invalid log level".to_string(),
            context: ValidationContext::new("test"),
            suggested_fix: Some("Use 'info', 'debug', 'warn', or 'error'".to_string()),
        };

        let result = handler.handle_validation_error(recoverable_error);
        match result {
            ErrorHandlingResult::Recovered(_) => {
                // Expected
            }
            _ => panic!("Expected recovered result"),
        }

        // Test handling a critical error
        let critical_error = ValidationError::ParseError {
            file_source: "config.yaml".to_string(),
            error: "Invalid YAML syntax".to_string(),
            line: Some(10),
            column: Some(5),
        };

        let result = handler.handle_validation_error(critical_error);
        match result {
            ErrorHandlingResult::Critical(_) => {
                // Expected
            }
            _ => panic!("Expected critical result"),
        }

        // Check error statistics
        let stats = handler.get_error_statistics();
        assert_eq!(stats.total_errors, 2);
        assert!(stats.critical_errors >= 1);
    }

    #[test]
    fn test_error_handling_result() {
        // Test successful result
        let success = ErrorHandlingResult::Success;
        assert!(success.is_success());
        assert!(!success.has_errors());
        assert!(success.into_service_result().is_ok());

        // Test warning result
        let warning_error = ValidationError::InvalidValue {
            field: "test".to_string(),
            value: "suboptimal".to_string(),
            reason: "Not optimal".to_string(),
            context: ValidationContext::new("test"),
            suggested_fix: None,
        };
        let warning = ErrorHandlingResult::Warning(warning_error);
        assert!(warning.is_success());
        assert!(!warning.has_errors());
        assert!(warning.into_service_result().is_ok());

        // Test error result
        let error = ValidationError::MissingField {
            field: "required".to_string(),
            context: ValidationContext::new("test"),
        };
        let error_result = ErrorHandlingResult::Failed(error);
        assert!(!error_result.is_success());
        assert!(error_result.has_errors());
        assert!(error_result.into_service_result().is_err());

        // Test recovered result
        let recovered = ErrorHandlingResult::Recovered(crucible_services::config::RecoveryResult {
            strategy: "default_value",
            original_value: None,
            recovered_value: "default".to_string(),
            description: "Applied default value".to_string(),
        });
        assert!(recovered.is_success());
        assert!(!recovered.has_errors());
        assert!(recovered.into_service_result().is_ok());
    }

    #[test]
    fn test_error_record() {
        let error = ValidationError::MissingField {
            field: "test_field".to_string(),
            context: ValidationContext::new("test_config.yaml")
                .with_section("logging")
                .with_field("level"),
        };

        let record = crucible_services::config::ErrorRecord::from_validation_error(&error);
        assert_eq!(record.field, Some("test_field".to_string()));
        assert_eq!(record.error_type, "validation_error");
        assert_eq!(record.severity, crucible_services::config::ValidationSeverity::Error);
        assert!(!record.recovery_attempted);
        assert!(!record.recovery_successful);

        // Test from service error
        let service_error = crucible_services::errors::ServiceError::ConfigurationError(
            "Test configuration error".to_string(),
        );
        let record = crucible_services::config::ErrorRecord::from_service_error(&service_error);
        assert_eq!(record.error_type, "service_error");
        assert_eq!(record.severity, crucible_services::config::ValidationSeverity::Error);
    }

    #[test]
    fn test_error_statistics() {
        let mut stats = crucible_services::config::ErrorStatistics::default();

        // Add some errors
        stats.total_errors = 10;
        stats.critical_errors = 2;
        stats.error_errors = 5;
        stats.warnings = 3;
        stats.field_error_counts.insert("logging.level".to_string(), 5);
        stats.field_error_counts.insert("database.url".to_string(), 3);

        // Test calculations
        assert_eq!(stats.error_rate_per_hour(1.0), 10.0);
        assert_eq!(stats.error_rate_per_hour(2.0), 5.0);
        assert!(!stats.is_high_error_rate(24.0));
        assert!(stats.is_high_error_rate(0.5));

        // Test most problematic fields
        let problematic = stats.most_problematic_fields(2);
        assert_eq!(problematic.len(), 2);
        assert_eq!(problematic[0].0, "logging.level");
        assert_eq!(problematic[0].1, 5);
        assert_eq!(problematic[1].0, "database.url");
        assert_eq!(problematic[1].1, 3);

        // Test summary
        let summary = stats.summary();
        assert!(summary.contains("10 total"));
        assert!(summary.contains("2 critical"));
        assert!(summary.contains("5 errors"));
        assert!(summary.contains("3 warnings"));
    }
}

#[tokio::test]
async fn test_config_manager_basic() {
    // Test basic configuration manager creation and operations
    let result = ConfigManager::new().await;
    assert!(result.is_ok());

    let manager = result.unwrap();

    // Test getting configuration
    let config = manager.get_config().await;
    assert!(config.validate().is_valid);

    // Test validation
    let validation_result = manager.validate_current_config().await;
    assert!(validation_result.is_valid);

    // Test health status
    let health = manager.health_status().await;
    assert!(health.is_healthy);
    assert_eq!(health.error_count, 0);

    // Test configuration summary
    let summary = manager.get_summary().await;
    assert!(!summary.is_empty());
    assert!(summary.contains("crucible-services"));
}

#[tokio::test]
async fn test_config_manager_builder() {
    // Test configuration manager builder
    let manager = ConfigManagerBuilder::new()
        .with_hot_reload(false)
        .build()
        .await;

    assert!(manager.is_ok());

    let manager = manager.unwrap();
    let config = manager.get_config().await;
    assert!(config.validate().is_valid);
}

#[tokio::test]
async fn test_config_export_import() {
    let manager = ConfigManager::new().await.unwrap();
    let config = manager.get_config().await;

    // Test export to JSON
    let json_export = manager.export_config(crucible_services::config::ConfigExportFormat::Json).await;
    assert!(json_export.is_ok());
    let json_str = json_export.unwrap();
    assert!(!json_str.is_empty());
    assert!(json_str.contains("crucible-services"));

    // Test export to YAML
    let yaml_export = manager.export_config(crucible_services::config::ConfigExportFormat::Yaml).await;
    assert!(yaml_export.is_ok());
    let yaml_str = yaml_export.unwrap();
    assert!(!yaml_str.is_empty());

    // Test import back
    let import_result = manager.import_config(&json_str, crucible_services::config::ConfigExportFormat::Json).await;
    assert!(import_result.is_ok());
}

#[tokio::test]
async fn test_config_update_validation() {
    let manager = ConfigManager::new().await.unwrap();

    // Create an invalid configuration
    let mut invalid_config = manager.get_config().await;
    invalid_config.service.environment = "invalid_env".to_string();
    invalid_config.logging.level = "invalid_level".to_string();

    // Try to update with invalid config - should fail
    let result = manager.update_config(invalid_config).await;
    assert!(result.is_err());

    // Get original config back
    let config = manager.get_config().await;
    assert!(config.validate().is_valid);
    assert_ne!(config.service.environment, "invalid_env");
}

#[tokio::test]
async fn test_config_patch() {
    let manager = ConfigManager::new().await.unwrap();

    // Create a patch to update log level
    let patch = crucible_services::config::ConfigPatch {
        operations: vec![
            crucible_services::config::ConfigOperation::Set {
                field: "logging.level".to_string(),
                value: serde_json::Value::String("debug".to_string()),
            },
            crucible_services::config::ConfigOperation::Set {
                field: "event_routing.max_concurrent_events".to_string(),
                value: serde_json::Value::Number(serde_json::Number::from(2000)),
            },
        ],
        metadata: crucible_services::config::PatchMetadata::default(),
    };

    // Apply patch
    let result = manager.apply_patch(&patch).await;
    assert!(result.is_ok());

    // Verify changes
    let config = manager.get_config().await;
    assert_eq!(config.logging.level, "debug");
    assert_eq!(config.event_routing.max_concurrent_events, 2000);
}

#[test]
fn test_error_handling_utils() {
    use crucible_services::config::error_handling::utils;

    // Test context creation
    let context = utils::create_error_context("test_config.yaml");
    assert_eq!(context.source, "test_config.yaml");
    assert!(context.metadata.contains_key("hostname"));
    assert!(context.metadata.contains_key("process_id"));
    assert!(context.metadata.contains_key("timestamp"));

    // Test error formatting
    let error = ValidationError::MissingField {
        field: "test_field".to_string(),
        context: context.clone(),
    };

    let formatted = utils::format_error_for_logging(&error, true);
    assert!(formatted.contains("ERROR"));
    assert!(formatted.contains("test_field"));
    assert!(formatted.contains("test_config.yaml"));

    // Test user-friendly message
    let user_msg = utils::create_user_friendly_message(&error);
    assert!(user_msg.contains("test_field"));
    assert!(user_msg.contains("missing"));

    // Test escalation check
    let critical_error = ValidationError::ParseError {
        file_source: "config.yaml".to_string(),
        error: "Invalid syntax".to_string(),
        line: None,
        column: None,
    };
    assert!(utils::should_escalate_error(&critical_error));

    let warning_error = ValidationError::InvalidValue {
        field: "optional".to_string(),
        value: "suboptimal".to_string(),
        reason: "Not optimal".to_string(),
        context: ValidationContext::new("test"),
        suggested_fix: None,
    };
    assert!(!utils::should_escalate_error(&warning_error));

    // Test error report generation
    let records = vec![
        crucible_services::config::ErrorRecord::from_validation_error(&error),
        crucible_services::config::ErrorRecord::from_validation_error(&critical_error),
    ];

    let report = utils::generate_error_report(&records);
    assert!(report.contains("Configuration Error Report"));
    assert!(report.contains("Summary"));
    assert!(report.contains("Recent Errors"));
}

// Integration test that combines multiple components
#[tokio::test]
async fn test_configuration_validation_integration() {
    // Test the complete configuration validation workflow

    // 1. Create configuration manager
    let manager = ConfigManager::new().await.unwrap();
    let initial_config = manager.get_config().await;

    // 2. Validate initial configuration
    let validation_result = initial_config.validate();
    assert!(validation_result.is_valid);

    // 3. Create error handler
    let mut error_handler = ConfigErrorHandler::new();

    // 4. Test handling validation result
    let handling_result = error_handler.handle_validation_result(validation_result);
    assert!(handling_result.is_success());

    // 5. Test configuration export/import cycle
    let json_export = manager.export_config(crucible_services::config::ConfigExportFormat::Json).await.unwrap();
    let import_result = manager.import_config(&json_export, crucible_services::config::ConfigExportFormat::Json).await;
    assert!(import_result.is_ok());

    // 6. Test configuration update with validation
    let mut updated_config = manager.get_config().await;
    updated_config.service.environment = "staging".to_string();
    updated_config.logging.level = "debug".to_string();

    let update_result = manager.update_config(updated_config).await;
    assert!(update_result.is_ok());

    // 7. Verify changes
    let final_config = manager.get_config().await;
    assert_eq!(final_config.service.environment, "staging");
    assert_eq!(final_config.logging.level, "debug");
    assert!(final_config.validate().is_valid);

    // 8. Test error statistics
    let stats = error_handler.get_error_statistics();
    assert_eq!(stats.total_errors, 0);

    // 9. Test health status
    let health = manager.health_status().await;
    assert!(health.is_healthy);
    assert_eq!(health.error_count, 0);
}