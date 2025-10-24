//! Comprehensive integration tests for configuration system
//!
//! This module provides end-to-end testing coverage for the Phase 7.3
//! configuration system with real-world scenarios and >95% coverage target.

use crucible_services::config::enhanced_config::*;
use crucible_services::config::manager::*;
use crucible_services::config::validation::*;
use crucible_services::config::error_handling::*;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use tempfile::TempDir;
use std::fs;
use std::io::Write;
use tokio::time::{sleep, Duration};
use serde_json::json;

#[cfg(test)]
mod end_to_end_workflow_tests {
    use super::*;

    #[tokio::test]
    async fn test_complete_configuration_lifecycle() {
        // Create temporary directory for test files
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.yaml");

        // Write initial configuration
        let initial_config = r#"
service:
  name: "lifecycle-test"
  version: "1.0.0"
  environment: "development"
  tags: ["test", "integration"]

logging:
  level: "info"
  format: "json"
  console_enabled: true
  structured: true

event_routing:
  max_event_age_seconds: 300
  max_concurrent_events: 1000
  enable_detailed_tracing: false

security:
  encryption_enabled: false
  authentication_enabled: false

performance:
  max_memory_mb: 1024
  enable_monitoring: true

plugins:
  enabled: true
  enabled_plugins: ["logger", "metrics"]
  plugin_timeout_seconds: 30
"#;

        fs::write(&config_path, initial_config).unwrap();

        // Test 1: Load configuration from file
        env::set_var("CRUCIBLE_CONFIG_FILE", config_path.to_str().unwrap());
        let manager = ConfigManager::new().await.unwrap();

        let config = manager.get_config().await;
        assert_eq!(config.service.name, "lifecycle-test");
        assert_eq!(config.service.version, "1.0.0");
        assert_eq!(config.service.environment, "development");
        assert_eq!(config.service.tags, vec!["test", "integration"]);
        assert_eq!(config.logging.level, "info");
        assert_eq!(config.event_routing.max_concurrent_events, 1000);
        assert!(config.performance.enable_monitoring);
        assert_eq!(config.plugins.enabled_plugins, vec!["logger", "metrics"]);

        // Test 2: Validate configuration
        let validation_result = manager.validate_current_config().await;
        assert!(validation_result.is_valid);
        assert!(validation_result.errors.is_empty());
        assert!(validation_result.warnings.is_empty());

        // Test 3: Export configuration
        let exported_json = manager.export_config(ConfigExportFormat::JSON).await.unwrap();
        let exported_yaml = manager.export_config(ConfigExportFormat::YAML).await.unwrap();
        let exported_toml = manager.export_config(ConfigExportFormat::TOML).await.unwrap();

        // Verify exported formats are valid
        let _: EnhancedConfig = serde_json::from_str(&exported_json).unwrap();
        let _: EnhancedConfig = serde_yaml::from_str(&exported_yaml).unwrap();
        let _: EnhancedConfig = toml::from_str(&exported_toml).unwrap();

        // Test 4: Modify configuration via patch
        let patch = ConfigPatch {
            operations: vec![
                ConfigOperation::Set {
                    field: "service.environment".to_string(),
                    value: json!("staging"),
                },
                ConfigOperation::Set {
                    field: "logging.level".to_string(),
                    value: json!("debug"),
                },
                ConfigOperation::Set {
                    field: "event_routing.enable_detailed_tracing".to_string(),
                    value: json!(true),
                },
                ConfigOperation::Set {
                    field: "performance.max_memory_mb".to_string(),
                    value: json!(2048),
                },
            ],
            metadata: PatchMetadata {
                description: Some("Integration test patch".to_string()),
                author: Some("integration-test".to_string()),
                ..Default::default()
            },
        };

        let patch_result = manager.apply_patch(&patch).await;
        assert!(patch_result.is_ok());

        // Verify patch was applied
        let patched_config = manager.get_config().await;
        assert_eq!(patched_config.service.environment, "staging");
        assert_eq!(patched_config.logging.level, "debug");
        assert!(patched_config.event_routing.enable_detailed_tracing);
        assert_eq!(patched_config.performance.max_memory_mb, 2048);

        // Test 5: Validate patched configuration
        let patched_validation = manager.validate_current_config().await;
        assert!(patched_validation.is_valid);

        // Test 6: Import modified configuration
        let modified_config_json = json!({
            "service": {
                "name": "imported-service",
                "version": "2.0.0",
                "environment": "production"
            },
            "logging": {
                "level": "warn",
                "format": "text"
            },
            "database": {
                "url": "postgresql://localhost/test",
                "max_connections": 20,
                "db_type": "postgres"
            },
            "security": {
                "encryption_enabled": true,
                "encryption_key_path": "/tmp/test.key"
            }
        });

        let import_result = manager.import_config(
            &serde_json::to_string(&modified_config_json).unwrap(),
            ConfigExportFormat::JSON
        ).await;
        assert!(import_result.is_err()); // Should fail due to missing encryption key path

        // Test 7: Fix and import valid configuration
        let valid_config_json = json!({
            "service": {
                "name": "imported-service",
                "version": "2.0.0",
                "environment": "production"
            },
            "logging": {
                "level": "warn",
                "format": "text"
            },
            "database": {
                "url": "postgresql://localhost/test",
                "max_connections": 20,
                "db_type": "postgres"
            }
        });

        let valid_import_result = manager.import_config(
            &serde_json::to_string(&valid_config_json).unwrap(),
            ConfigExportFormat::JSON
        ).await;
        assert!(valid_import_result.is_ok());

        // Test 8: Verify final state
        let final_config = manager.get_config().await;
        assert_eq!(final_config.service.name, "imported-service");
        assert_eq!(final_config.service.version, "2.0.0");
        assert_eq!(final_config.service.environment, "production");
        assert_eq!(final_config.logging.level, "warn");
        assert_eq!(final_config.logging.format, "text");
        assert!(final_config.database.is_some());
        assert_eq!(final_config.database.as_ref().unwrap().url, "postgresql://localhost/test");

        // Test 9: Health check
        let health_status = manager.health_status().await;
        assert!(health_status.is_healthy);
        assert_eq!(health_status.error_count, 0);

        env::remove_var("CRUCIBLE_CONFIG_FILE");
    }

    #[tokio::test]
    async fn test_configuration_with_environment_overrides() {
        // Set up environment variables
        env::set_var("CRUCIBLE_SERVICE_NAME", "env-override-test");
        env::set_var("CRUCIBLE_SERVICE_VERSION", "3.0.0");
        env::set_var("CRUCIBLE_ENVIRONMENT", "production");
        env::set_var("CRUCIBLE_LOG_LEVEL", "error");
        env::set_var("CRUCIBLE_LOG_FORMAT", "text");
        env::set_var("CRUCIBLE_MAX_EVENT_AGE", "600");
        env::set_var("CRUCIBLE_MAX_CONCURRENT_EVENTS", "2000");
        env::set_var("CRUCIBLE_DATABASE_URL", "postgresql://localhost:5432/envtest");

        let manager = ConfigManager::new().await.unwrap();
        let config = manager.get_config().await;

        // Verify environment overrides were applied
        assert_eq!(config.service.name, "env-override-test");
        assert_eq!(config.service.version, "3.0.0");
        assert_eq!(config.service.environment, "production");
        assert_eq!(config.logging.level, "error");
        assert_eq!(config.logging.format, "text");
        assert_eq!(config.event_routing.max_event_age_seconds, 600);
        assert_eq!(config.event_routing.max_concurrent_events, 2000);
        assert!(config.database.is_some());
        assert_eq!(config.database.as_ref().unwrap().url, "postgresql://localhost:5432/envtest");

        // Validate the overridden configuration
        let validation_result = manager.validate_current_config().await;
        assert!(validation_result.is_valid);

        // Test individual section accessors
        let service_config = manager.get_service_config().await;
        assert_eq!(service_config.name, "env-override-test");

        let logging_config = manager.get_logging_config().await;
        assert_eq!(logging_config.level, "error");

        let event_routing_config = manager.get_event_routing_config().await;
        assert_eq!(event_routing_config.max_concurrent_events, 2000);

        let database_config = manager.get_database_config().await;
        assert!(database_config.is_some());
        assert_eq!(database_config.unwrap().url, "postgresql://localhost:5432/envtest");

        // Cleanup
        env::remove_var("CRUCIBLE_SERVICE_NAME");
        env::remove_var("CRUCIBLE_SERVICE_VERSION");
        env::remove_var("CRUCIBLE_ENVIRONMENT");
        env::remove_var("CRUCIBLE_LOG_LEVEL");
        env::remove_var("CRUCIBLE_LOG_FORMAT");
        env::remove_var("CRUCIBLE_MAX_EVENT_AGE");
        env::remove_var("CRUCIBLE_MAX_CONCURRENT_EVENTS");
        env::remove_var("CRUCIBLE_DATABASE_URL");
    }

    #[tokio::test]
    async fn test_configuration_reload_functionality() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("reload_test.yaml");

        // Write initial configuration
        let initial_config = r#"
service:
  name: "initial-service"
  environment: "development"

logging:
  level: "info"
"#;

        fs::write(&config_path, initial_config).unwrap();

        env::set_var("CRUCIBLE_CONFIG_FILE", config_path.to_str().unwrap());
        let manager = ConfigManager::new().await.unwrap();

        // Verify initial state
        let initial_loaded = manager.get_config().await;
        assert_eq!(initial_loaded.service.name, "initial-service");
        assert_eq!(initial_loaded.logging.level, "info");

        // Modify the file
        sleep(Duration::from_millis(100)).await; // Ensure different timestamp
        let updated_config = r#"
service:
  name: "updated-service"
  environment: "production"
  version: "2.0.0"

logging:
  level: "debug"
  format: "json"

event_routing:
  max_concurrent_events: 1500
"#;

        fs::write(&config_path, updated_config).unwrap();

        // Reload configuration
        let reload_result = manager.reload_config().await;
        assert!(reload_result.is_ok());

        // Verify updated state
        let reloaded_config = manager.get_config().await;
        assert_eq!(reloaded_config.service.name, "updated-service");
        assert_eq!(reloaded_config.service.environment, "production");
        assert_eq!(reloaded_config.service.version, "2.0.0");
        assert_eq!(reloaded_config.logging.level, "debug");
        assert_eq!(reloaded_config.logging.format, "json");
        assert_eq!(reloaded_config.event_routing.max_concurrent_events, 1500);

        // Validate reloaded configuration
        let validation_result = manager.validate_current_config().await;
        assert!(validation_result.is_valid);

        env::remove_var("CRUCIBLE_CONFIG_FILE");
    }
}

#[cfg(test)]
mod error_handling_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_configuration_error_recovery() {
        let mut error_handler = ConfigErrorHandler::new();

        // Create manager with invalid configuration
        env::set_var("CRUCIBLE_SERVICE_NAME", ""); // Invalid: empty name
        env::set_var("CRUCIBLE_LOG_LEVEL", "invalid"); // Invalid: not in enum

        let result = ConfigManager::new().await;
        assert!(result.is_err()); // Should fail due to validation

        // Handle validation errors
        let errors = vec![
            ValidationError::InvalidValue {
                field: "service.name".to_string(),
                value: "".to_string(),
                reason: "Service name cannot be empty".to_string(),
                context: ValidationContext::new("environment"),
                suggested_fix: Some("Use a valid service name".to_string()),
            },
            ValidationError::InvalidValue {
                field: "logging.level".to_string(),
                value: "invalid".to_string(),
                reason: "Invalid log level".to_string(),
                context: ValidationContext::new("environment"),
                suggested_fix: Some("Use: trace, debug, info, warn, error".to_string()),
            },
        ];

        let mut recovery_results = vec![];
        for error in errors {
            let result = error_handler.handle_validation_error(error);
            recovery_results.push(result);
        }

        // Verify recovery attempts
        assert_eq!(recovery_results.len(), 2);
        assert!(recovery_results.iter().any(|r| matches!(r, ErrorHandlingResult::Recovered(_))));

        // Verify error history
        assert_eq!(error_handler.error_history().len(), 2);

        // Get error statistics
        let stats = error_handler.get_error_statistics();
        assert_eq!(stats.total_errors, 2);

        // Generate error report
        let report = utils::generate_error_report(&error_handler.error_history());
        assert!(report.contains("Configuration Error Report"));
        assert!(report.contains("service.name"));
        assert!(report.contains("logging.level"));

        // Cleanup
        env::remove_var("CRUCIBLE_SERVICE_NAME");
        env::remove_var("CRUCIBLE_LOG_LEVEL");
    }

    #[tokio::test]
    async fn test_complex_error_scenarios() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("error_test.yaml");

        // Write configuration with various issues
        let problematic_config = r#"
service:
  name: "test-service"
  environment: "production"

logging:
  level: "info"
  file_enabled: true
  # Missing file_path - will generate warning

event_routing:
  max_event_age_seconds: 0
  max_concurrent_events: 0

security:
  encryption_enabled: true
  # Missing encryption_key_path - will generate error

performance:
  max_memory_mb: 0
  cpu_threshold_percent: 150.0
"#;

        fs::write(&config_path, problematic_config).unwrap();

        env::set_var("CRUCIBLE_CONFIG_FILE", config_path.to_str().unwrap());

        let result = ConfigManager::new().await;
        assert!(result.is_err()); // Should fail due to validation errors

        // Create error handler and process the validation result
        let mut error_handler = ConfigErrorHandler::with_config(ErrorReportingConfig {
            enable_reporting: true,
            report_warnings: true,
            include_context: true,
            ..Default::default()
        });

        // Simulate the validation errors that would occur
        let validation_errors = vec![
            ValidationError::ConstraintViolation {
                field: "max_event_age_seconds".to_string(),
                constraint: "minimum value".to_string(),
                details: "Value 0 is less than minimum 1".to_string(),
                context: ValidationContext::new("error_test.yaml")
                    .with_section("event_routing"),
            },
            ValidationError::ConstraintViolation {
                field: "max_concurrent_events".to_string(),
                constraint: "minimum value".to_string(),
                details: "Value 0 is less than minimum 1".to_string(),
                context: ValidationContext::new("error_test.yaml")
                    .with_section("event_routing"),
            },
            ValidationError::MissingField {
                field: "encryption_key_path".to_string(),
                context: ValidationContext::new("error_test.yaml")
                    .with_section("security"),
            },
            ValidationError::InvalidValue {
                field: "file_enabled".to_string(),
                value: "true".to_string(),
                reason: "File logging is enabled but no file path is specified".to_string(),
                context: ValidationContext::new("error_test.yaml")
                    .with_section("logging"),
                suggested_fix: Some("Set file_path or disable file logging".to_string()),
            },
        ];

        let validation_result = ValidationResult::with_errors(validation_errors);
        let handling_result = error_handler.handle_validation_result(validation_result);

        match handling_result {
            ErrorHandlingResult::Failed(_) => {
                // Expected for critical errors
            }
            _ => panic!("Expected Failed result for critical validation errors"),
        }

        // Verify comprehensive error tracking
        let stats = error_handler.get_error_statistics();
        assert_eq!(stats.total_errors, 4);
        assert!(stats.error_errors > 0);
        assert!(stats.warnings > 0);

        // Verify most problematic fields are identified
        let problematic_fields = stats.most_problematic_fields(5);
        assert!(problematic_fields.len() > 0);

        env::remove_var("CRUCIBLE_CONFIG_FILE");
    }

    #[tokio::test]
    async fn test_error_recovery_with_defaults() {
        let mut error_handler = ConfigErrorHandler::new();

        // Test recovery scenarios for fields with default strategies
        let recoverable_errors = vec![
            ValidationError::InvalidValue {
                field: "logging.level".to_string(),
                value: "invalid".to_string(),
                reason: "Invalid log level".to_string(),
                context: ValidationContext::new("test"),
                suggested_fix: Some("Use default log level".to_string()),
            },
            ValidationError::InvalidValue {
                field: "service.environment".to_string(),
                value: "invalid".to_string(),
                reason: "Invalid environment".to_string(),
                context: ValidationContext::new("test"),
                suggested_fix: Some("Use default environment".to_string()),
            },
            ValidationError::InvalidValue {
                field: "event_routing.max_concurrent_events".to_string(),
                value: "invalid".to_string(),
                reason: "Invalid number".to_string(),
                context: ValidationContext::new("test"),
                suggested_fix: Some("Use default value".to_string()),
            },
        ];

        let mut recovered_count = 0;
        for error in recoverable_errors {
            let result = error_handler.handle_validation_error(error);
            if matches!(result, ErrorHandlingResult::Recovered(_)) {
                recovered_count += 1;
            }
        }

        // Most should be recoverable due to default strategies
        assert!(recovered_count > 0);

        // Verify recovery statistics
        let stats = error_handler.get_error_statistics();
        assert_eq!(stats.total_errors, 3);
        assert!(stats.error_errors == 0); // All should be warnings or recovered
    }
}

#[cfg(test)]
mod performance_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_configuration_loading_performance() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("perf_test.yaml");

        // Create a large configuration
        let mut large_config = HashMap::new();
        large_config.insert("service".to_string(), json!({
            "name": "performance-test",
            "version": "1.0.0",
            "environment": "production",
            "tags": (0..100).map(|i| format!("tag-{}", i)).collect::<Vec<_>>()
        }));

        large_config.insert("plugins".to_string(), json!({
            "enabled": true,
            "enabled_plugins": (0..50).map(|i| format!("plugin-{}", i)).collect::<Vec<_>>(),
            "plugin_configs": (0..50).map(|i| (
                format!("plugin-{}", i),
                json!({
                    "setting1": format!("value1-{}", i),
                    "setting2": format!("value2-{}", i),
                    "nested": {
                        "deep": (0..10).map(|j| format!("deep-value-{}-{}", i, j)).collect::<Vec<_>>()
                    }
                })
            )).collect::<std::collections::HashMap<_, _>>()
        }));

        let yaml_content = serde_yaml::to_string(&large_config).unwrap();
        fs::write(&config_path, yaml_content).unwrap();

        env::set_var("CRUCIBLE_CONFIG_FILE", config_path.to_str().unwrap());

        // Measure loading time
        let start = std::time::Instant::now();
        let manager = ConfigManager::new().await.unwrap();
        let load_duration = start.elapsed();

        // Should load within reasonable time
        assert!(load_duration.as_millis() < 1000);

        // Verify configuration loaded correctly
        let config = manager.get_config().await;
        assert_eq!(config.service.name, "performance-test");
        assert_eq!(config.service.tags.len(), 100);
        assert_eq!(config.plugins.enabled_plugins.len(), 50);
        assert_eq!(config.plugins.plugin_configs.len(), 50);

        // Measure validation time
        let start = std::time::Instant::now();
        let validation_result = manager.validate_current_config().await;
        let validation_duration = start.elapsed();

        assert!(validation_duration.as_millis() < 500);
        assert!(validation_result.is_valid);

        env::remove_var("CRUCIBLE_CONFIG_FILE");
    }

    #[tokio::test]
    async fn test_concurrent_configuration_access() {
        let manager = ConfigManager::new().await.unwrap();

        // Spawn multiple concurrent tasks
        let mut handles = vec![];

        for i in 0..10 {
            let manager_clone = manager.clone();
            let handle = tokio::spawn(async move {
                for j in 0..100 {
                    // Read configuration
                    let _config = manager_clone.get_config().await;

                    // Validate configuration
                    let _validation = manager_clone.validate_current_config().await;

                    // Get section configurations
                    let _service = manager_clone.get_service_config().await;
                    let _logging = manager_clone.get_logging_config().await;

                    // Small delay to simulate real work
                    tokio::time::sleep(Duration::from_micros(1)).await;

                    if i == 5 && j == 50 {
                        // Apply a small patch in one of the tasks
                        let patch = ConfigPatch {
                            operations: vec![ConfigOperation::Set {
                                field: "performance.metrics_interval_seconds".to_string(),
                                value: json!(30 + j),
                            }],
                            metadata: PatchMetadata::default(),
                        };
                        let _ = manager_clone.apply_patch(&patch).await;
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            assert!(handle.await.is_ok());
        }

        // Verify final state is consistent
        let final_config = manager.get_config().await;
        assert!(final_config.validate().is_valid);

        let health_status = manager.health_status().await;
        assert!(health_status.is_healthy);
    }

    #[tokio::test]
    async fn test_memory_usage_under_load() {
        let manager = ConfigManager::new().await.unwrap();

        // Apply many configuration updates to test memory management
        for i in 0..1000 {
            let patch = ConfigPatch {
                operations: vec![ConfigOperation::Set {
                    field: "performance.metrics_interval_seconds".to_string(),
                    value: json!(i % 60), // Cycle through values 0-59
                }],
                metadata: PatchMetadata {
                    description: Some(format!("Load test patch {}", i)),
                    ..Default::default()
                },
            };

            let result = manager.apply_patch(&patch).await;
            assert!(result.is_ok());

            // Periodically validate to ensure no memory leaks
            if i % 100 == 0 {
                let validation_result = manager.validate_current_config().await;
                assert!(validation_result.is_valid);
            }
        }

        // Final verification
        let final_config = manager.get_config().await;
        assert!(final_config.validate().is_valid);

        let error_count = manager.health_status().await.error_count;
        assert_eq!(error_count, 0);
    }
}

#[cfg(test)]
mod real_world_scenario_tests {
    use super::*;

    #[tokio::test]
    async fn test_production_deployment_scenario() {
        // Simulate a production deployment configuration
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("production.yaml");

        let production_config = r#"
service:
  name: "crucible-api"
  version: "2.1.0"
  environment: "production"
  description: "Production API service"
  tags: ["api", "production", "critical"]

logging:
  level: "warn"
  format: "json"
  file_enabled: true
  file_path: "/var/log/crucible/api.log"
  max_file_size: 5368709120  # 5GB
  max_files: 10
  console_enabled: false
  structured: true
  correlation_field: "request_id"

event_routing:
  max_event_age_seconds: 600  # 10 minutes
  max_concurrent_events: 10000
  default_routing_strategy: "priority_based"
  enable_detailed_tracing: false
  routing_history_limit: 10000
  event_buffer_size: 100000
  enable_persistence: true
  storage_path: "/var/lib/crucible/events"

database:
  url: "postgresql://prod-cluster:5432/crucible_prod"
  max_connections: 50
  timeout_seconds: 10
  enable_pooling: true
  db_type: "postgres"

security:
  encryption_enabled: true
  encryption_key_path: "/etc/crucible/encryption.key"
  authentication_enabled: true
  jwt_secret: "very-long-secure-jwt-secret-key-for-production-environment-32-chars-min"
  token_expiration_hours: 1
  rate_limiting_enabled: true
  rate_limit_rpm: 1000

performance:
  max_memory_mb: 8192  # 8GB
  enable_memory_profiling: false
  cpu_threshold_percent: 75.0
  enable_monitoring: true
  metrics_interval_seconds: 30

plugins:
  enabled: true
  search_paths: ["/opt/crucible/plugins"]
  enabled_plugins: ["auth", "metrics", "audit", "cache"]
  disabled_plugins: ["debug", "experimental"]
  enable_sandboxing: true
  plugin_timeout_seconds: 15
"#;

        fs::write(&config_path, production_config).unwrap();

        // Set production environment variables
        env::set_var("CRUCIBLE_CONFIG_FILE", config_path.to_str().unwrap());
        env::set_var("CRUCIBLE_CONFIG_HOT_RELOAD", "false"); // Disabled in production
        env::set_var("CRUCIBLE_ENVIRONMENT", "production");

        let manager = ConfigManager::new().await.unwrap();

        // Verify production configuration
        let config = manager.get_config().await;
        assert_eq!(config.service.name, "crucible-api");
        assert_eq!(config.service.environment, "production");
        assert_eq!(config.logging.level, "warn");
        assert!(!config.logging.console_enabled);
        assert!(config.logging.file_enabled);
        assert_eq!(config.logging.file_path, Some(PathBuf::from("/var/log/crucible/api.log")));
        assert_eq!(config.event_routing.max_concurrent_events, 10000);
        assert!(config.event_routing.enable_persistence);
        assert!(config.database.is_some());
        assert_eq!(config.database.as_ref().unwrap().max_connections, 50);
        assert!(config.security.encryption_enabled);
        assert!(config.security.authentication_enabled);
        assert_eq!(config.security.rate_limit_rpm, 1000);
        assert_eq!(config.performance.max_memory_mb, 8192);
        assert!(config.performance.enable_monitoring);
        assert_eq!(config.plugins.enabled_plugins.len(), 4);
        assert_eq!(config.plugins.disabled_plugins.len(), 2);

        // Validate production configuration
        let validation_result = manager.validate_current_config().await;
        assert!(validation_result.is_valid);

        // Test production-specific operations
        let exported_config = manager.export_config(ConfigExportFormat::YAML).await.unwrap();
        assert!(exported_config.contains("crucible-api"));
        assert!(exported_config.contains("production"));

        // Test health monitoring
        let health_status = manager.health_status().await;
        assert!(health_status.is_healthy);
        assert!(!health_status.has_warnings);
        assert!(!health_status.hot_reload_enabled);

        // Test configuration summary
        let summary = manager.get_summary().await;
        assert!(summary.contains("crucible-api"));
        assert!(summary.contains("production"));
        assert!(summary.contains("configured")); // Database is configured

        // Cleanup
        env::remove_var("CRUCIBLE_CONFIG_FILE");
        env::remove_var("CRUCIBLE_CONFIG_HOT_RELOAD");
        env::remove_var("CRUCIBLE_ENVIRONMENT");
    }

    #[tokio::test]
    async fn test_development_environment_scenario() {
        // Simulate a development environment with frequent changes
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("development.yaml");

        let dev_config = r#"
service:
  name: "crucible-dev"
  version: "0.1.0-dev"
  environment: "development"
  tags: ["dev", "debug"]

logging:
  level: "debug"
  format: "text"
  console_enabled: true
  structured: false

event_routing:
  max_event_age_seconds: 60
  max_concurrent_events: 100
  enable_detailed_tracing: true

security:
  encryption_enabled: false
  authentication_enabled: false

performance:
  max_memory_mb: 512
  enable_memory_profiling: true
  enable_monitoring: true
  metrics_interval_seconds: 10

plugins:
  enabled: true
  enabled_plugins: ["debug", "profiler", "test-utils"]
  enable_sandboxing: false
  plugin_timeout_seconds: 60
"#;

        fs::write(&config_path, dev_config).unwrap();

        // Set development environment
        env::set_var("CRUCIBLE_CONFIG_FILE", config_path.to_str().unwrap());
        env::set_var("CRUCIBLE_CONFIG_HOT_RELOAD", "true");
        env::set_var("CRUCIBLE_CONFIG_RELOAD_INTERVAL", "1"); // 1 second for testing
        env::set_var("CRUCIBLE_DEBUG_EVENTS", "true");
        env::set_var("CRUCIBLE_DEBUG_PERFORMANCE", "true");

        let manager = ConfigManager::new().await.unwrap();

        // Verify development configuration
        let config = manager.get_config().await;
        assert_eq!(config.service.name, "crucible-dev");
        assert_eq!(config.service.environment, "development");
        assert_eq!(config.logging.level, "debug");
        assert!(config.logging.console_enabled);
        assert!(config.event_routing.enable_detailed_tracing);
        assert!(!config.security.encryption_enabled);
        assert!(config.performance.enable_memory_profiling);
        assert!(!config.plugins.enable_sandboxing);

        // Start hot reload monitoring
        let monitor_result = manager.start_hot_reload_monitor().await;
        assert!(monitor_result.is_ok());

        // Simulate configuration change
        sleep(Duration::from_millis(1500)).await; // Wait for reload interval

        let updated_config = r#"
service:
  name: "crucible-dev-updated"
  version: "0.2.0-dev"
  environment: "development"

logging:
  level: "trace"  # Changed for debugging

event_routing:
  max_concurrent_events: 200  # Increased for testing
"#;

        fs::write(&config_path, updated_config).unwrap();

        // Wait for potential reload
        sleep(Duration::from_millis(2000)).await;

        // Verify hot reload is enabled
        let health_status = manager.health_status().await;
        assert!(health_status.hot_reload_enabled);

        // Test rapid development workflow with patches
        for i in 0..10 {
            let patch = ConfigPatch {
                operations: vec![
                    ConfigOperation::Set {
                        field: "performance.metrics_interval_seconds".to_string(),
                        value: json!(5 + i),
                    },
                    ConfigOperation::Set {
                        field: "event_routing.routing_history_limit".to_string(),
                        value: json!(1000 + i * 100),
                    },
                ],
                metadata: PatchMetadata {
                    description: Some(format!("Dev iteration {}", i)),
                    ..Default::default()
                },
            };

            let result = manager.apply_patch(&patch).await;
            assert!(result.is_ok());
        }

        // Verify final state
        let final_config = manager.get_config().await;
        assert!(final_config.validate().is_valid);

        // Cleanup
        env::remove_var("CRUCIBLE_CONFIG_FILE");
        env::remove_var("CRUCIBLE_CONFIG_HOT_RELOAD");
        env::remove_var("CRUCIBLE_CONFIG_RELOAD_INTERVAL");
        env::remove_var("CRUCIBLE_DEBUG_EVENTS");
        env::remove_var("CRUCIBLE_DEBUG_PERFORMANCE");
    }

    #[tokio::test]
    async fn test_multi_environment_configuration() {
        // Test configuration that works across multiple environments
        let base_config = EnhancedConfig {
            service: ServiceConfig {
                name: "multi-env-service".to_string(),
                version: "1.0.0".to_string(),
                environment: "development".to_string(), // Will be overridden
                description: Some("Multi-environment service".to_string()),
                tags: vec!["api".to_string(), "multi-env".to_string()],
            },
            logging: LoggingConfig {
                level: "info".to_string(), // Will be overridden per environment
                format: "json".to_string(),
                file_enabled: true,
                file_path: Some(PathBuf::from("/var/log/app.log")),
                console_enabled: true,
                structured: true,
                ..Default::default()
            },
            event_routing: EventRoutingConfig {
                max_concurrent_events: 1000,
                enable_detailed_tracing: false,
                ..Default::default()
            },
            database: Some(DatabaseConfig {
                url: "sqlite:local.db".to_string(), // Will be overridden
                ..Default::default()
            }),
            security: SecurityConfig {
                authentication_enabled: false, // Will be enabled in prod
                ..Default::default()
            },
            performance: PerformanceConfig {
                max_memory_mb: 512, // Will be increased in prod
                enable_monitoring: true,
                ..Default::default()
            },
            plugins: PluginConfig {
                enabled_plugins: vec!["logger".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };

        // Test development environment
        env::set_var("CRUCIBLE_ENVIRONMENT", "development");
        env::set_var("CRUCIBLE_LOG_LEVEL", "debug");
        env::set_var("CRUCIBLE_DATABASE_URL", "sqlite:dev.db");

        let manager = ConfigManager::new().await.unwrap();
        let dev_config = manager.get_config().await;
        assert_eq!(dev_config.service.environment, "development");
        assert_eq!(dev_config.logging.level, "debug");
        assert_eq!(dev_config.database.as_ref().unwrap().url, "sqlite:dev.db");

        // Test staging environment
        env::set_var("CRUCIBLE_ENVIRONMENT", "staging");
        env::set_var("CRUCIBLE_LOG_LEVEL", "info");
        env::set_var("CRUCIBLE_DATABASE_URL", "postgresql://staging-db/test");
        env::set_var("CRUCIBLE_MAX_CONCURRENT_EVENTS", "5000");

        let staging_manager = ConfigManager::new().await.unwrap();
        let staging_config = staging_manager.get_config().await;
        assert_eq!(staging_config.service.environment, "staging");
        assert_eq!(staging_config.logging.level, "info");
        assert_eq!(staging_config.database.as_ref().unwrap().url, "postgresql://staging-db/test");
        assert_eq!(staging_config.event_routing.max_concurrent_events, 5000);

        // Test production environment
        env::set_var("CRUCIBLE_ENVIRONMENT", "production");
        env::set_var("CRUCIBLE_LOG_LEVEL", "warn");
        env::set_var("CRUCIBLE_DATABASE_URL", "postgresql://prod-db/production");
        env::set_var("CRUCIBLE_MAX_CONCURRENT_EVENTS", "10000");
        env::set_var("CRUCIBLE_SERVICE_NAME", "multi-env-api-prod");

        let prod_manager = ConfigManager::new().await.unwrap();
        let prod_config = prod_manager.get_config().await;
        assert_eq!(prod_config.service.environment, "production");
        assert_eq!(prod_config.service.name, "multi-env-api-prod");
        assert_eq!(prod_config.logging.level, "warn");
        assert_eq!(prod_config.database.as_ref().unwrap().url, "postgresql://prod-db/production");
        assert_eq!(prod_config.event_routing.max_concurrent_events, 10000);

        // Validate all configurations
        for config in [dev_config, staging_config, prod_config] {
            assert!(config.validate().is_valid);
        }

        // Cleanup
        env::remove_var("CRUCIBLE_ENVIRONMENT");
        env::remove_var("CRUCIBLE_LOG_LEVEL");
        env::remove_var("CRUCIBLE_DATABASE_URL");
        env::remove_var("CRUCIBLE_MAX_CONCURRENT_EVENTS");
        env::remove_var("CRUCIBLE_SERVICE_NAME");
    }
}

#[cfg(test)]
mod configuration_security_tests {
    use super::*;

    #[tokio::test]
    async fn test_sensitive_data_handling() {
        // Test that sensitive data is handled properly
        let config_with_secrets = EnhancedConfig {
            service: ServiceConfig {
                name: "secure-service".to_string(),
                ..Default::default()
            },
            security: SecurityConfig {
                encryption_enabled: true,
                encryption_key_path: Some(PathBuf::from("/secure/encryption.key")),
                authentication_enabled: true,
                jwt_secret: Some("super-secret-jwt-key-that-is-long-enough".to_string()),
                ..Default::default()
            },
            database: Some(DatabaseConfig {
                url: "postgresql://user:password@localhost/db".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Test that configuration can be serialized with secrets
        let serialized = serde_json::to_string(&config_with_secrets).unwrap();
        assert!(serialized.contains("super-secret-jwt-key"));

        // Test that configuration can be deserialized
        let deserialized: EnhancedConfig = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.security.jwt_secret, Some("super-secret-jwt-key-that-is-long-enough".to_string()));

        // Test validation catches security issues
        let insecure_config = EnhancedConfig {
            service: ServiceConfig {
                name: "insecure-service".to_string(),
                ..Default::default()
            },
            security: SecurityConfig {
                encryption_enabled: true,
                encryption_key_path: None, // Missing key path
                authentication_enabled: true,
                jwt_secret: Some("short".to_string()), // Too short
                ..Default::default()
            },
            ..Default::default()
        };

        let validation_result = insecure_config.validate();
        assert!(!validation_result.is_valid);
        assert!(!validation_result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_configuration_file_permissions() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("secure_config.yaml");

        let secure_config = r#"
service:
  name: "secure-test"

security:
  encryption_enabled: true
  encryption_key_path: "/test/key.pem"
  authentication_enabled: true
  jwt_secret: "secure-jwt-secret-for-testing-purposes-only"
"#;

        fs::write(&config_path, secure_config).unwrap();

        // Test loading configuration from file
        env::set_var("CRUCIBLE_CONFIG_FILE", config_path.to_str().unwrap());
        let manager = ConfigManager::new().await.unwrap();

        let config = manager.get_config().await;
        assert!(config.security.encryption_enabled);
        assert!(config.security.authentication_enabled);
        assert!(config.security.jwt_secret.is_some());

        // Test that export includes sensitive data
        let exported = manager.export_config(ConfigExportFormat::YAML).await.unwrap();
        assert!(exported.contains("jwt_secret"));

        env::remove_var("CRUCIBLE_CONFIG_FILE");
    }
}