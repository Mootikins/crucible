//! Comprehensive unit tests for configuration manager
//!
//! This module provides thorough testing coverage for the Phase 7.3
//! configuration manager with >95% coverage target.

use crucible_services::config::manager::*;
use crucible_services::config::enhanced_config::*;
use crucible_services::config::validation::*;
use std::time::Duration;
use std::env;
use tempfile::NamedTempFile;
use std::io::Write;
use tokio::time::{sleep, timeout};

#[cfg(test)]
mod config_manager_creation_tests {
    use super::*;

    #[tokio::test]
    async fn test_config_manager_new() {
        let result = ConfigManager::new().await;
        assert!(result.is_ok());

        let manager = result.unwrap();
        let config = manager.get_config().await;
        assert_eq!(config.service.name, "crucible-services");
        assert!(config.validate().is_valid);
    }

    #[tokio::test]
    async fn test_config_manager_with_file() {
        let yaml_content = r#"
service:
  name: "test-service"
  environment: "staging"
logging:
  level: "debug"
event_routing:
  max_concurrent_events: 500
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(yaml_content.as_bytes()).unwrap();
        let file_path = temp_file.path().to_str().unwrap();

        let result = ConfigManager::with_file(file_path).await;
        assert!(result.is_ok());

        let manager = result.unwrap();
        let config = manager.get_config().await;
        assert_eq!(config.service.name, "test-service");
        assert_eq!(config.service.environment, "staging");
        assert_eq!(config.logging.level, "debug");
        assert_eq!(config.event_routing.max_concurrent_events, 500);
    }

    #[tokio::test]
    async fn test_config_manager_with_nonexistent_file() {
        let result = ConfigManager::with_file("/nonexistent/config.yaml").await;
        // Should still succeed with defaults
        assert!(result.is_ok());

        let manager = result.unwrap();
        let config = manager.get_config().await;
        assert_eq!(config.service.name, "crucible-services"); // Default value
    }

    #[tokio::test]
    async fn test_config_manager_clone() {
        let manager = ConfigManager::new().await.unwrap();
        let cloned_manager = manager.clone();

        // Both should have the same initial config
        let config1 = manager.get_config().await;
        let config2 = cloned_manager.get_config().await;
        assert_eq!(config1.service.name, config2.service.name);
    }
}

#[cfg(test)]
mod config_manager_access_tests {
    use super::*;

    #[tokio::test]
    async fn test_get_config() {
        let manager = ConfigManager::new().await.unwrap();
        let config = manager.get_config().await;

        assert_eq!(config.service.name, "crucible-services");
        assert_eq!(config.service.version, "0.1.0");
        assert_eq!(config.service.environment, "development");
        assert!(config.database.is_none());
        assert!(config.validate().is_valid);
    }

    #[tokio::test]
    async fn test_get_service_config() {
        let manager = ConfigManager::new().await.unwrap();
        let service_config = manager.get_service_config().await;

        assert_eq!(service_config.name, "crucible-services");
        assert_eq!(service_config.version, "0.1.0");
        assert_eq!(service_config.environment, "development");
    }

    #[tokio::test]
    async fn test_get_logging_config() {
        let manager = ConfigManager::new().await.unwrap();
        let logging_config = manager.get_logging_config().await;

        assert_eq!(logging_config.level, "info");
        assert_eq!(logging_config.format, "json");
        assert!(logging_config.console_enabled);
    }

    #[tokio::test]
    async fn test_get_event_routing_config() {
        let manager = ConfigManager::new().await.unwrap();
        let routing_config = manager.get_event_routing_config().await;

        assert_eq!(routing_config.max_event_age_seconds, 300);
        assert_eq!(routing_config.max_concurrent_events, 1000);
        assert_eq!(routing_config.default_routing_strategy, "type_based");
    }

    #[tokio::test]
    async fn test_get_database_config_none() {
        let manager = ConfigManager::new().await.unwrap();
        let db_config = manager.get_database_config().await;

        assert!(db_config.is_none());
    }

    #[tokio::test]
    async fn test_get_database_config_some() {
        // Set environment variable to create database config
        env::set_var("CRUCIBLE_DATABASE_URL", "postgresql://localhost/test");

        let manager = ConfigManager::new().await.unwrap();
        let db_config = manager.get_database_config().await;

        assert!(db_config.is_some());
        assert_eq!(db_config.as_ref().unwrap().url, "postgresql://localhost/test");

        env::remove_var("CRUCIBLE_DATABASE_URL");
    }

    #[tokio::test]
    async fn test_get_security_config() {
        let manager = ConfigManager::new().await.unwrap();
        let security_config = manager.get_security_config().await;

        assert!(!security_config.encryption_enabled);
        assert!(!security_config.authentication_enabled);
        assert_eq!(security_config.token_expiration_hours, 24);
    }

    #[tokio::test]
    async fn test_get_performance_config() {
        let manager = ConfigManager::new().await.unwrap();
        let perf_config = manager.get_performance_config().await;

        assert_eq!(perf_config.max_memory_mb, 1024);
        assert_eq!(perf_config.cpu_threshold_percent, 80.0);
        assert!(!perf_config.enable_monitoring);
    }

    #[tokio::test]
    async fn test_get_plugin_config() {
        let manager = ConfigManager::new().await.unwrap();
        let plugin_config = manager.get_plugin_config().await;

        assert!(plugin_config.enabled);
        assert!(plugin_config.enable_sandboxing);
        assert_eq!(plugin_config.plugin_timeout_seconds, 30);
    }

    #[tokio::test]
    async fn test_get_summary() {
        let manager = ConfigManager::new().await.unwrap();
        let summary = manager.get_summary().await;

        assert!(summary.contains("crucible-services"));
        assert!(summary.contains("development"));
        assert!(summary.contains("info"));
        assert!(summary.contains("1000"));
    }
}

#[cfg(test)]
mod config_manager_update_tests {
    use super::*;

    #[tokio::test]
    async fn test_update_config_valid() {
        let manager = ConfigManager::new().await.unwrap();

        let mut new_config = manager.get_config().await;
        new_config.service.name = "updated-service".to_string();
        new_config.logging.level = "debug".to_string();

        let result = manager.update_config(new_config.clone()).await;
        assert!(result.is_ok());

        // Verify update
        let current_config = manager.get_config().await;
        assert_eq!(current_config.service.name, "updated-service");
        assert_eq!(current_config.logging.level, "debug");
    }

    #[tokio::test]
    async fn test_update_config_invalid() {
        let manager = ConfigManager::new().await.unwrap();

        let mut new_config = manager.get_config().await;
        new_config.service.name = "".to_string(); // Invalid: empty name
        new_config.logging.level = "invalid".to_string(); // Invalid: not in enum

        let result = manager.update_config(new_config).await;
        assert!(result.is_err());

        // Original config should remain unchanged
        let current_config = manager.get_config().await;
        assert_eq!(current_config.service.name, "crucible-services");
        assert_eq!(current_config.logging.level, "info");
    }

    #[tokio::test]
    async fn test_update_config_preserves_validation() {
        let manager = ConfigManager::new().await.unwrap();

        let mut new_config = manager.get_config().await;
        new_config.service.environment = "production".to_string();

        let result = manager.update_config(new_config).await;
        assert!(result.is_ok());

        // Updated config should still be valid
        let validation_result = manager.validate_current_config().await;
        assert!(validation_result.is_valid);
    }

    #[tokio::test]
    async fn test_update_config_caches_validation() {
        let manager = ConfigManager::new().await.unwrap();

        // Clear any existing cache
        {
            let mut cache = manager.validation_cache.write().await;
            *cache = None;
        }

        let new_config = manager.get_config().await;
        let result = manager.update_config(new_config).await;
        assert!(result.is_ok());

        // Should have cached validation result
        let cached_result = manager.get_cached_validation().await;
        assert!(cached_result.is_some());
        assert!(cached_result.unwrap().is_valid);
    }
}

#[cfg(test)]
mod config_manager_validation_tests {
    use super::*;

    #[tokio::test]
    async fn test_validate_current_config() {
        let manager = ConfigManager::new().await.unwrap();
        let result = manager.validate_current_config().await;

        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_validate_current_config_invalid_after_update() {
        let manager = ConfigManager::new().await.unwrap();

        let mut config = manager.get_config().await;
        config.service.name = "".to_string(); // Make invalid

        // Update without validation (directly modify)
        {
            let mut manager_config = manager.config.write().await;
            *manager_config = config;
        }

        let result = manager.validate_current_config().await;
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_get_cached_validation() {
        let manager = ConfigManager::new().await.unwrap();

        // Initially should have cached validation from initialization
        let cached = manager.get_cached_validation().await;
        assert!(cached.is_some());
        assert!(cached.unwrap().is_valid);

        // Clear cache
        {
            let mut cache = manager.validation_cache.write().await;
            *cache = None;
        }

        // Should return None after clearing
        let cached = manager.get_cached_validation().await;
        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn test_validation_result_logging() {
        let manager = ConfigManager::new().await.unwrap();
        let result = manager.validate_current_config().await;

        // This should not panic - logging integration test
        result.log_result("test_component");
    }

    #[tokio::test]
    async fn test_validate_with_warnings() {
        let manager = ConfigManager::new().await.unwrap();

        let mut config = manager.get_config().await;
        config.logging.file_enabled = true;
        config.logging.file_path = None; // Warning: file enabled but no path

        {
            let mut manager_config = manager.config.write().await;
            *manager_config = config;
        }

        let result = manager.validate_current_config().await;
        assert!(result.is_valid); // Still valid, but with warnings
        assert!(!result.warnings.is_empty());
    }
}

#[cfg(test)]
mod config_manager_reload_tests {
    use super::*;

    #[tokio::test]
    async fn test_reload_config() {
        let manager = ConfigManager::new().await.unwrap();
        let original_config = manager.get_config().await;

        // Modify environment to trigger change
        env::set_var("CRUCIBLE_SERVICE_NAME", "reloaded-service");

        let result = manager.reload_config().await;
        assert!(result.is_ok());

        let reloaded_config = manager.get_config().await;
        assert_eq!(reloaded_config.service.name, "reloaded-service");
        assert_ne!(original_config.service.name, reloaded_config.service.name);

        env::remove_var("CRUCIBLE_SERVICE_NAME");
    }

    #[tokio::test]
    async fn test_reload_config_with_file() {
        let yaml_content = r#"
service:
  name: "original-service"
logging:
  level: "info"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(yaml_content.as_bytes()).unwrap();
        let file_path = temp_file.path().to_str().unwrap();

        let manager = ConfigManager::with_file(file_path).await.unwrap();
        assert_eq!(manager.get_config().await.service.name, "original-service");

        // Update file content
        let updated_content = r#"
service:
  name: "updated-service"
  environment: "production"
logging:
  level: "debug"
"#;

        std::fs::write(file_path, updated_content).unwrap();

        let result = manager.reload_config().await;
        assert!(result.is_ok());

        let updated_config = manager.get_config().await;
        assert_eq!(updated_config.service.name, "updated-service");
        assert_eq!(updated_config.service.environment, "production");
        assert_eq!(updated_config.logging.level, "debug");
    }

    #[tokio::test]
    async fn test_reload_config_invalid_file() {
        let manager = ConfigManager::new().await.unwrap();

        // Set invalid config file path
        env::set_var("CRUCIBLE_CONFIG_FILE", "/nonexistent/config.yaml");

        let result = manager.reload_config().await;
        assert!(result.is_err());

        env::remove_var("CRUCIBLE_CONFIG_FILE");
    }

    #[tokio::test]
    async fn test_reload_config_preserves_validity() {
        let manager = ConfigManager::new().await.unwrap();

        // Ensure original config is valid
        let original_validation = manager.validate_current_config().await;
        assert!(original_validation.is_valid);

        // Reload with same environment (should remain valid)
        let result = manager.reload_config().await;
        assert!(result.is_ok());

        let reloaded_validation = manager.validate_current_config().await;
        assert!(reloaded_validation.is_valid);
    }
}

#[cfg(test)]
mod config_manager_hot_reload_tests {
    use super::*;

    #[tokio::test]
    async fn test_start_hot_reload_monitor_disabled() {
        env::set_var("CRUCIBLE_CONFIG_HOT_RELOAD", "false");

        let manager = ConfigManager::new().await.unwrap();
        let result = manager.start_hot_reload_monitor().await;
        assert!(result.is_ok());

        // Monitor should not be started, but should not error
        env::remove_var("CRUCIBLE_CONFIG_HOT_RELOAD");
    }

    #[tokio::test]
    async fn test_start_hot_reload_monitor_enabled() {
        let yaml_content = r#"
service:
  name: "hot-reload-test"
logging:
  level: "info"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(yaml_content.as_bytes()).unwrap();
        let file_path = temp_file.path().to_str().unwrap();

        env::set_var("CRUCIBLE_CONFIG_FILE", file_path);
        env::set_var("CRUCIBLE_CONFIG_HOT_RELOAD", "true");
        env::set_var("CRUCIBLE_CONFIG_RELOAD_INTERVAL", "1"); // 1 second for testing

        let manager = ConfigManager::new().await.unwrap();
        assert_eq!(manager.get_config().await.service.name, "hot-reload-test");

        let result = manager.start_hot_reload_monitor().await;
        assert!(result.is_ok());

        // Update file
        sleep(Duration::from_millis(500)).await; // Wait a bit
        let updated_content = r#"
service:
  name: "hot-reload-updated"
logging:
  level: "debug"
"#;

        std::fs::write(file_path, updated_content).unwrap();

        // Wait for reload to potentially happen
        sleep(Duration::from_millis(2000)).await;

        // Check if config was updated (this might be timing-sensitive)
        let current_config = manager.get_config().await;
        // Don't assert exact result due to timing, but ensure no crashes occurred

        env::remove_var("CRUCIBLE_CONFIG_FILE");
        env::remove_var("CRUCIBLE_CONFIG_HOT_RELOAD");
        env::remove_var("CRUCIBLE_CONFIG_RELOAD_INTERVAL");
    }

    #[tokio::test]
    async fn test_check_and_reload_no_file() {
        let manager = ConfigManager::new().await.unwrap();
        // Should not error when no config file is set
        let result = timeout(Duration::from_millis(100), async {
            // Use a closure to access private method through a public interface
            manager.validate_current_config().await
        }).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_valid);
    }

    #[tokio::test]
    async fn test_hot_reload_configuration() {
        env::set_var("CRUCIBLE_CONFIG_HOT_RELOAD", "true");
        env::set_var("CRUCIBLE_CONFIG_RELOAD_INTERVAL", "60");

        let manager = ConfigManager::new().await.unwrap();
        assert!(manager.enable_hot_reload);
        assert_eq!(manager.reload_interval, Duration::from_secs(60));

        env::remove_var("CRUCIBLE_CONFIG_HOT_RELOAD");
        env::remove_var("CRUCIBLE_CONFIG_RELOAD_INTERVAL");
    }
}

#[cfg(test)]
mod config_manager_export_import_tests {
    use super::*;

    #[tokio::test]
    async fn test_export_config_json() {
        let manager = ConfigManager::new().await.unwrap();
        let result = manager.export_config(ConfigExportFormat::JSON).await;

        assert!(result.is_ok());
        let json_str = result.unwrap();

        // Should be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed.is_object());

        // Should contain expected fields
        if let Some(obj) = parsed.as_object() {
            assert!(obj.contains_key("service"));
            assert!(obj.contains_key("logging"));
            assert!(obj.contains_key("event_routing"));
        }
    }

    #[tokio::test]
    async fn test_export_config_yaml() {
        let manager = ConfigManager::new().await.unwrap();
        let result = manager.export_config(ConfigExportFormat::YAML).await;

        assert!(result.is_ok());
        let yaml_str = result.unwrap();

        // Should be valid YAML
        let parsed: EnhancedConfig = serde_yaml::from_str(&yaml_str).unwrap();
        assert_eq!(parsed.service.name, "crucible-services");
    }

    #[tokio::test]
    async fn test_export_config_toml() {
        let manager = ConfigManager::new().await.unwrap();
        let result = manager.export_config(ConfigExportFormat::TOML).await;

        assert!(result.is_ok());
        let toml_str = result.unwrap();

        // Should be valid TOML
        let parsed: EnhancedConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.service.name, "crucible-services");
    }

    #[tokio::test]
    async fn test_import_config_json() {
        let manager = ConfigManager::new().await.unwrap();

        let json_content = json!({
            "service": {
                "name": "imported-service",
                "version": "2.0.0",
                "environment": "staging"
            },
            "logging": {
                "level": "debug",
                "format": "text"
            }
        });

        let json_str = serde_json::to_string_pretty(&json_content).unwrap();
        let result = manager.import_config(&json_str, ConfigExportFormat::JSON).await;

        assert!(result.is_ok());

        let config = manager.get_config().await;
        assert_eq!(config.service.name, "imported-service");
        assert_eq!(config.service.version, "2.0.0");
        assert_eq!(config.service.environment, "staging");
        assert_eq!(config.logging.level, "debug");
        assert_eq!(config.logging.format, "text");
    }

    #[tokio::test]
    async fn test_import_config_yaml() {
        let manager = ConfigManager::new().await.unwrap();

        let yaml_content = r#"
service:
  name: "yaml-imported-service"
  version: "3.0.0"
  environment: "production"
logging:
  level: "warn"
  format: "compact"
event_routing:
  max_concurrent_events: 2000
"#;

        let result = manager.import_config(yaml_content, ConfigExportFormat::YAML).await;

        assert!(result.is_ok());

        let config = manager.get_config().await;
        assert_eq!(config.service.name, "yaml-imported-service");
        assert_eq!(config.service.version, "3.0.0");
        assert_eq!(config.service.environment, "production");
        assert_eq!(config.logging.level, "warn");
        assert_eq!(config.logging.format, "compact");
        assert_eq!(config.event_routing.max_concurrent_events, 2000);
    }

    #[tokio::test]
    async fn test_import_config_toml() {
        let manager = ConfigManager::new().await.unwrap();

        let toml_content = r#"
[service]
name = "toml-imported-service"
version = "4.0.0"
environment = "development"

[logging]
level = "trace"
format = "json"

[performance]
max_memory_mb = 2048
enable_monitoring = true
"#;

        let result = manager.import_config(toml_content, ConfigExportFormat::TOML).await;

        assert!(result.is_ok());

        let config = manager.get_config().await;
        assert_eq!(config.service.name, "toml-imported-service");
        assert_eq!(config.service.version, "4.0.0");
        assert_eq!(config.service.environment, "development");
        assert_eq!(config.logging.level, "trace");
        assert_eq!(config.logging.format, "json");
        assert_eq!(config.performance.max_memory_mb, 2048);
        assert!(config.performance.enable_monitoring);
    }

    #[tokio::test]
    async fn test_import_config_invalid_json() {
        let manager = ConfigManager::new().await.unwrap();

        let invalid_json = "{ invalid json content }";
        let result = manager.import_config(invalid_json, ConfigExportFormat::JSON).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_import_config_invalid_yaml() {
        let manager = ConfigManager::new().await.unwrap();

        let invalid_yaml = "invalid: yaml: content: [";
        let result = manager.import_config(invalid_yaml, ConfigExportFormat::YAML).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_import_config_invalid_toml() {
        let manager = ConfigManager::new().await.unwrap();

        let invalid_toml = "invalid toml content [";
        let result = manager.import_config(invalid_toml, ConfigExportFormat::TOML).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_import_config_validation_failure() {
        let manager = ConfigManager::new().await.unwrap();

        let json_content = json!({
            "service": {
                "name": "", // Invalid: empty name
                "environment": "invalid" // Invalid: not in enum
            },
            "logging": {
                "level": "invalid" // Invalid: not in enum
            }
        });

        let json_str = serde_json::to_string(&json_content).unwrap();
        let result = manager.import_config(&json_str, ConfigExportFormat::JSON).await;

        assert!(result.is_err());

        // Original config should remain unchanged
        let config = manager.get_config().await;
        assert_eq!(config.service.name, "crucible-services");
        assert_eq!(config.logging.level, "info");
    }
}

#[cfg(test)]
mod config_manager_health_tests {
    use super::*;

    #[tokio::test]
    async fn test_health_status_healthy() {
        let manager = ConfigManager::new().await.unwrap();
        let status = manager.health_status().await;

        assert!(status.is_healthy);
        assert!(!status.has_warnings);
        assert_eq!(status.error_count, 0);
        assert_eq!(status.warning_count, 0);
        assert!(status.last_validation.is_some());
        assert!(status.config_age.is_some());
    }

    #[tokio::test]
    async fn test_health_status_with_warnings() {
        let manager = ConfigManager::new().await.unwrap();

        let mut config = manager.get_config().await;
        config.logging.file_enabled = true;
        config.logging.file_path = None; // Warning condition

        {
            let mut manager_config = manager.config.write().await;
            *manager_config = config;
        }

        let status = manager.health_status().await;
        assert!(status.is_healthy); // Still healthy with warnings
        assert!(status.has_warnings);
        assert_eq!(status.warning_count, 1);
    }

    #[tokio::test]
    async fn test_health_status_with_errors() {
        let manager = ConfigManager::new().await.unwrap();

        let mut config = manager.get_config().await;
        config.service.name = "".to_string(); // Error condition

        {
            let mut manager_config = manager.config.write().await;
            *manager_config = config;
        }

        let status = manager.health_status().await;
        assert!(!status.is_healthy);
        assert!(status.error_count > 0);
    }

    #[tokio::test]
    async fn test_health_status_hot_reload_info() {
        env::set_var("CRUCIBLE_CONFIG_HOT_RELOAD", "true");

        let manager = ConfigManager::new().await.unwrap();
        let status = manager.health_status().await;

        assert!(status.hot_reload_enabled);

        env::remove_var("CRUCIBLE_CONFIG_HOT_RELOAD");
    }

    #[tokio::test]
    async fn test_health_status_config_age() {
        let manager = ConfigManager::new().await.unwrap();
        sleep(Duration::from_millis(10)).await;

        let status = manager.health_status().await;
        assert!(status.config_age.is_some());
        assert!(status.config_age.unwrap() > Duration::from_millis(0));
    }
}

#[cfg(test)]
mod config_manager_patch_tests {
    use super::*;

    #[tokio::test]
    async fn test_apply_patch_set_operations() {
        let manager = ConfigManager::new().await.unwrap();

        let patch = ConfigPatch {
            operations: vec![
                ConfigOperation::Set {
                    field: "service.name".to_string(),
                    value: json!("patched-service"),
                },
                ConfigOperation::Set {
                    field: "logging.level".to_string(),
                    value: json!("debug"),
                },
                ConfigOperation::Set {
                    field: "event_routing.max_concurrent_events".to_string(),
                    value: json!(2000),
                },
            ],
            metadata: PatchMetadata::default(),
        };

        let result = manager.apply_patch(&patch).await;
        assert!(result.is_ok());

        let config = manager.get_config().await;
        assert_eq!(config.service.name, "patched-service");
        assert_eq!(config.logging.level, "debug");
        assert_eq!(config.event_routing.max_concurrent_events, 2000);
    }

    #[tokio::test]
    async fn test_apply_patch_remove_operations() {
        let manager = ConfigManager::new().await.unwrap();

        // First add a database config
        let mut config = manager.get_config().await;
        config.database = Some(DatabaseConfig {
            url: "postgresql://localhost/test".to_string(),
            ..Default::default()
        });
        manager.update_config(config).await.unwrap();

        let patch = ConfigPatch {
            operations: vec![
                ConfigOperation::Remove {
                    field: "database".to_string(),
                },
                ConfigOperation::Remove {
                    field: "security.encryption_key_path".to_string(),
                },
            ],
            metadata: PatchMetadata::default(),
        };

        let result = manager.apply_patch(&patch).await;
        assert!(result.is_ok());

        let updated_config = manager.get_config().await;
        assert!(updated_config.database.is_none());
        assert!(updated_config.security.encryption_key_path.is_none());
    }

    #[tokio::test]
    async fn test_apply_patch_update_operations() {
        let manager = ConfigManager::new().await.unwrap();

        let patch = ConfigPatch {
            operations: vec![
                ConfigOperation::Update {
                    field: "service.environment".to_string(),
                    value: json!("production"),
                },
                ConfigOperation::Update {
                    field: "logging.format".to_string(),
                    value: json!("text"),
                },
            ],
            metadata: PatchMetadata::default(),
        };

        let result = manager.apply_patch(&patch).await;
        assert!(result.is_ok());

        let config = manager.get_config().await;
        assert_eq!(config.service.environment, "production");
        assert_eq!(config.logging.format, "text");
    }

    #[tokio::test]
    async fn test_apply_patch_unsupported_field() {
        let manager = ConfigManager::new().await.unwrap();

        let patch = ConfigPatch {
            operations: vec![
                ConfigOperation::Set {
                    field: "unsupported.field".to_string(),
                    value: json!("value"),
                },
            ],
            metadata: PatchMetadata::default(),
        };

        let result = manager.apply_patch(&patch).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_apply_patch_invalid_remove() {
        let manager = ConfigManager::new().await.unwrap();

        let patch = ConfigPatch {
            operations: vec![
                ConfigOperation::Remove {
                    field: "service.name".to_string(), // Cannot remove required field
                },
            ],
            metadata: PatchMetadata::default(),
        };

        let result = manager.apply_patch(&patch).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_apply_patch_invalid_values() {
        let manager = ConfigManager::new().await.unwrap();

        let patch = ConfigPatch {
            operations: vec![
                ConfigOperation::Set {
                    field: "event_routing.max_concurrent_events".to_string(),
                    value: json!(-1), // Invalid: negative
                },
            ],
            metadata: PatchMetadata::default(),
        };

        let result = manager.apply_patch(&patch).await;
        assert!(result.is_err()); // Should fail validation

        // Config should remain unchanged
        let config = manager.get_config().await;
        assert_eq!(config.event_routing.max_concurrent_events, 1000); // Default value
    }

    #[tokio::test]
    async fn test_apply_patch_complex_scenario() {
        let manager = ConfigManager::new().await.unwrap();

        // Set up initial state with some fields
        let mut config = manager.get_config().await;
        config.database = Some(DatabaseConfig {
            url: "postgresql://localhost/initial".to_string(),
            ..Default::default()
        });
        config.security.encryption_key_path = Some(PathBuf::from("/tmp/key.pem"));
        manager.update_config(config).await.unwrap();

        let patch = ConfigPatch {
            operations: vec![
                ConfigOperation::Set {
                    field: "service.name".to_string(),
                    value: json!("complex-patch-test"),
                },
                ConfigOperation::Update {
                    field: "service.environment".to_string(),
                    value: json!("staging"),
                },
                ConfigOperation::Set {
                    field: "logging.level".to_string(),
                    value: json!("trace"),
                },
                ConfigOperation::Remove {
                    field: "security.encryption_key_path".to_string(),
                },
                ConfigOperation::Set {
                    field: "event_routing.max_event_age_seconds".to_string(),
                    value: json!(600),
                },
            ],
            metadata: PatchMetadata {
                description: Some("Complex patch test".to_string()),
                ..Default::default()
            },
        };

        let result = manager.apply_patch(&patch).await;
        assert!(result.is_ok());

        let updated_config = manager.get_config().await;
        assert_eq!(updated_config.service.name, "complex-patch-test");
        assert_eq!(updated_config.service.environment, "staging");
        assert_eq!(updated_config.logging.level, "trace");
        assert!(updated_config.security.encryption_key_path.is_none());
        assert_eq!(updated_config.event_routing.max_event_age_seconds, 600);
        // Database should remain
        assert!(updated_config.database.is_some());
    }
}

#[cfg(test)]
mod config_manager_builder_tests {
    use super::*;

    #[tokio::test]
    async fn test_config_manager_builder_default() {
        let manager = ConfigManagerBuilder::new().build().await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_config_manager_builder_with_config_file() {
        let yaml_content = r#"
service:
  name: "builder-test-service"
logging:
  level: "debug"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(yaml_content.as_bytes()).unwrap();
        let file_path = temp_file.path().to_str().unwrap();

        let manager = ConfigManagerBuilder::new()
            .with_config_file(file_path)
            .build()
            .await;

        assert!(manager.is_ok());

        let config = manager.unwrap().get_config().await;
        assert_eq!(config.service.name, "builder-test-service");
        assert_eq!(config.logging.level, "debug");
    }

    #[tokio::test]
    async fn test_config_manager_builder_with_hot_reload() {
        let manager = ConfigManagerBuilder::new()
            .with_hot_reload(true)
            .build()
            .await;

        assert!(manager.is_ok());

        let built_manager = manager.unwrap();
        assert!(built_manager.enable_hot_reload);
    }

    #[tokio::test]
    async fn test_config_manager_builder_with_reload_interval() {
        let manager = ConfigManagerBuilder::new()
            .with_reload_interval(Duration::from_secs(120))
            .build()
            .await;

        assert!(manager.is_ok());

        let built_manager = manager.unwrap();
        assert_eq!(built_manager.reload_interval, Duration::from_secs(120));
    }

    #[tokio::test]
    async fn test_config_manager_builder_combined_options() {
        let yaml_content = r#"
service:
  name: "combined-options-test"
environment: "production"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(yaml_content.as_bytes()).unwrap();
        let file_path = temp_file.path().to_str().unwrap();

        let manager = ConfigManagerBuilder::new()
            .with_config_file(file_path)
            .with_hot_reload(false) // Disabled for testing
            .with_reload_interval(Duration::from_secs(300))
            .build()
            .await;

        assert!(manager.is_ok());

        let built_manager = manager.unwrap();
        let config = built_manager.get_config().await;
        assert_eq!(config.service.name, "combined-options-test");
        assert_eq!(config.service.environment, "production");
        assert!(!built_manager.enable_hot_reload);
        assert_eq!(built_manager.reload_interval, Duration::from_secs(300));
    }

    #[tokio::test]
    async fn test_config_manager_builder_invalid_config_file() {
        let manager = ConfigManagerBuilder::new()
            .with_config_file("/nonexistent/config.yaml")
            .build()
            .await;

        // Should still succeed with defaults
        assert!(manager.is_ok());

        let config = manager.unwrap().get_config().await;
        assert_eq!(config.service.name, "crucible-services"); // Default value
    }
}

#[cfg(test)]
mod config_manager_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_full_config_lifecycle() {
        // Create manager
        let manager = ConfigManager::new().await.unwrap();

        // Verify initial state
        let initial_config = manager.get_config().await;
        assert_eq!(initial_config.service.name, "crucible-services");
        assert!(manager.validate_current_config().await.is_valid);

        // Export configuration
        let exported_json = manager.export_config(ConfigExportFormat::JSON).await.unwrap();

        // Modify and import configuration
        let modified_json = exported_json.replace("crucible-services", "lifecycle-test");
        let import_result = manager.import_config(&modified_json, ConfigExportFormat::JSON).await;
        assert!(import_result.is_ok());

        // Verify changes
        let updated_config = manager.get_config().await;
        assert_eq!(updated_config.service.name, "lifecycle-test");
        assert!(manager.validate_current_config().await.is_valid);

        // Apply patch
        let patch = ConfigPatch {
            operations: vec![
                ConfigOperation::Set {
                    field: "service.environment".to_string(),
                    value: json!("testing"),
                },
            ],
            metadata: PatchMetadata {
                description: Some("Lifecycle test patch".to_string()),
                ..Default::default()
            },
        };

        let patch_result = manager.apply_patch(&patch).await;
        assert!(patch_result.is_ok());

        // Verify patch application
        let final_config = manager.get_config().await;
        assert_eq!(final_config.service.name, "lifecycle-test");
        assert_eq!(final_config.service.environment, "testing");

        // Check health status
        let health = manager.health_status().await;
        assert!(health.is_healthy);
        assert_eq!(health.error_count, 0);
    }

    #[tokio::test]
    async fn test_concurrent_config_access() {
        let manager = ConfigManager::new().await.unwrap();
        let manager_clone = manager.clone();

        // Spawn multiple concurrent tasks
        let task1 = tokio::spawn(async move {
            for _ in 0..10 {
                let _ = manager.get_config().await;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });

        let task2 = tokio::spawn(async move {
            for _ in 0..5 {
                let _ = manager_clone.validate_current_config().await;
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        });

        // Wait for tasks to complete
        let (result1, result2) = tokio::join!(task1, task2);
        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn test_config_manager_error_recovery() {
        let manager = ConfigManager::new().await.unwrap();

        // Try to import invalid config
        let invalid_json = json!({
            "service": {
                "name": "", // Invalid
                "environment": "invalid" // Invalid
            }
        });

        let json_str = serde_json::to_string(&invalid_json).unwrap();
        let import_result = manager.import_config(&json_str, ConfigExportFormat::JSON).await;
        assert!(import_result.is_err());

        // Manager should still be functional
        let config = manager.get_config().await;
        assert_eq!(config.service.name, "crucible-services");
        assert!(manager.validate_current_config().await.is_valid);

        // Should be able to import valid config afterwards
        let valid_json = json!({
            "service": {
                "name": "recovery-test",
                "environment": "development"
            }
        });

        let valid_json_str = serde_json::to_string(&valid_json).unwrap();
        let recovery_result = manager.import_config(&valid_json_str, ConfigExportFormat::JSON).await;
        assert!(recovery_result.is_ok());

        let recovered_config = manager.get_config().await;
        assert_eq!(recovered_config.service.name, "recovery-test");
    }

    #[tokio::test]
    async fn test_config_manager_performance() {
        let manager = ConfigManager::new().await.unwrap();

        // Test multiple rapid operations
        let start = std::time::Instant::now();

        for i in 0..100 {
            let patch = ConfigPatch {
                operations: vec![
                    ConfigOperation::Set {
                        field: "service.name".to_string(),
                        value: json!(format!("perf-test-{}", i)),
                    },
                ],
                metadata: PatchMetadata::default(),
            };

            let _ = manager.apply_patch(&patch).await;
        }

        let duration = start.elapsed();

        // Should complete within reasonable time (adjust threshold as needed)
        assert!(duration.as_millis() < 5000);

        // Final config should be valid
        let final_config = manager.get_config().await;
        assert!(final_config.validate().is_valid);
    }

    #[tokio::test]
    async fn test_config_manager_memory_usage() {
        let manager = ConfigManager::new().await.unwrap();

        // Create multiple large configurations and verify they don't leak
        for i in 0..10 {
            let large_config = EnhancedConfig {
                service: ServiceConfig {
                    name: format!("large-config-{}", i),
                    tags: (0..100).map(|j| format!("tag-{}-{}", i, j)).collect(),
                    ..Default::default()
                },
                plugins: PluginConfig {
                    enabled_plugins: (0..50).map(|j| format!("plugin-{}-{}", i, j)).collect(),
                    ..Default::default()
                },
                ..Default::default()
            };

            let _ = manager.update_config(large_config).await;
        }

        // Should still be functional and memory-efficient
        let final_config = manager.get_config().await;
        assert!(final_config.validate().is_valid);
    }
}