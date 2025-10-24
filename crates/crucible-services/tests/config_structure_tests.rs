//! Comprehensive unit tests for configuration structures
//!
//! This module provides thorough testing coverage for the Phase 7.3
//! enhanced configuration structures with >95% coverage target.

use crucible_services::config::enhanced_config::*;
use crucible_services::config::validation::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::env;
use tempfile::NamedTempFile;
use std::io::Write;

#[cfg(test)]
mod enhanced_config_tests {
    use super::*;

    #[test]
    fn test_enhanced_config_default() {
        let config = EnhancedConfig::default();

        assert_eq!(config.service.name, "crucible-services");
        assert_eq!(config.service.version, "0.1.0");
        assert_eq!(config.service.environment, "development");
        assert_eq!(config.logging.level, "info");
        assert_eq!(config.logging.format, "json");
        assert_eq!(config.event_routing.max_event_age_seconds, 300);
        assert_eq!(config.event_routing.max_concurrent_events, 1000);
        assert!(config.database.is_none());
        assert!(!config.security.encryption_enabled);
        assert!(config.performance.max_memory_mb > 0);
        assert!(config.plugins.enabled);
        assert!(config.environment_overrides.is_empty());
    }

    #[test]
    fn test_enhanced_config_serialization() {
        let config = EnhancedConfig::default();

        // Test JSON serialization
        let json_str = serde_json::to_string(&config).unwrap();
        let deserialized: EnhancedConfig = serde_json::from_str(&json_str).unwrap();
        assert_eq!(config.service.name, deserialized.service.name);

        // Test YAML serialization
        let yaml_str = serde_yaml::to_string(&config).unwrap();
        let deserialized: EnhancedConfig = serde_yaml::from_str(&yaml_str).unwrap();
        assert_eq!(config.service.name, deserialized.service.name);
    }

    #[test]
    fn test_enhanced_config_get_summary() {
        let config = EnhancedConfig::default();
        let summary = config.get_summary();

        assert!(summary.contains("crucible-services"));
        assert!(summary.contains("development"));
        assert!(summary.contains("info"));
        assert!(summary.contains("1000")); // max_concurrent_events
        assert!(summary.contains("none")); // database
        assert!(summary.contains("0")); // plugins enabled count
    }

    #[tokio::test]
    async fn test_enhanced_config_apply_environment_overrides() {
        // Set environment variables
        env::set_var("CRUCIBLE_SERVICE_NAME", "test-service");
        env::set_var("CRUCIBLE_SERVICE_VERSION", "2.0.0");
        env::set_var("CRUCIBLE_ENVIRONMENT", "production");
        env::set_var("CRUCIBLE_LOG_LEVEL", "debug");
        env::set_var("CRUCIBLE_LOG_FORMAT", "text");
        env::set_var("CRUCIBLE_MAX_EVENT_AGE", "600");
        env::set_var("CRUCIBLE_MAX_CONCURRENT_EVENTS", "2000");
        env::set_var("CRUCIBLE_DATABASE_URL", "postgresql://localhost/test");

        let config = EnhancedConfig::default().apply_environment_overrides();

        assert_eq!(config.service.name, "test-service");
        assert_eq!(config.service.version, "2.0.0");
        assert_eq!(config.service.environment, "production");
        assert_eq!(config.logging.level, "debug");
        assert_eq!(config.logging.format, "text");
        assert_eq!(config.event_routing.max_event_age_seconds, 600);
        assert_eq!(config.event_routing.max_concurrent_events, 2000);
        assert!(config.database.is_some());
        assert_eq!(config.database.as_ref().unwrap().url, "postgresql://localhost/test");

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
    async fn test_enhanced_config_load_from_file_yaml() {
        let yaml_content = r#"
service:
  name: "test-service"
  version: "1.0.0"
  environment: "staging"
logging:
  level: "debug"
  format: "text"
  file_enabled: true
event_routing:
  max_event_age_seconds: 120
  max_concurrent_events: 500
database:
  url: "sqlite:test.db"
  max_connections: 5
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(yaml_content.as_bytes()).unwrap();
        let file_path = temp_file.path().to_str().unwrap();

        env::set_var("CRUCIBLE_CONFIG_FILE", file_path);

        let result = EnhancedConfig::load_from_sources().await;
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.service.name, "test-service");
        assert_eq!(config.service.version, "1.0.0");
        assert_eq!(config.service.environment, "staging");
        assert_eq!(config.logging.level, "debug");
        assert_eq!(config.logging.format, "text");
        assert!(config.logging.file_enabled);
        assert_eq!(config.event_routing.max_event_age_seconds, 120);
        assert_eq!(config.event_routing.max_concurrent_events, 500);
        assert!(config.database.is_some());
        assert_eq!(config.database.as_ref().unwrap().url, "sqlite:test.db");

        env::remove_var("CRUCIBLE_CONFIG_FILE");
    }

    #[tokio::test]
    async fn test_enhanced_config_load_from_file_json() {
        let json_content = json!({
            "service": {
                "name": "json-service",
                "version": "2.0.0",
                "environment": "production"
            },
            "logging": {
                "level": "error",
                "format": "json"
            },
            "security": {
                "encryption_enabled": true,
                "authentication_enabled": true
            }
        });

        let mut temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path();
        let json_path = file_path.with_extension("json");

        // Rename to .json extension
        std::fs::rename(file_path, &json_path).unwrap();

        std::fs::write(&json_path, json_content.to_string()).unwrap();
        env::set_var("CRUCIBLE_CONFIG_FILE", json_path.to_str().unwrap());

        let result = EnhancedConfig::load_from_sources().await;
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.service.name, "json-service");
        assert_eq!(config.service.version, "2.0.0");
        assert_eq!(config.service.environment, "production");
        assert_eq!(config.logging.level, "error");
        assert!(config.security.encryption_enabled);
        assert!(config.security.authentication_enabled);

        env::remove_var("CRUCIBLE_CONFIG_FILE");
    }

    #[tokio::test]
    async fn test_enhanced_config_load_invalid_file() {
        env::set_var("CRUCIBLE_CONFIG_FILE", "/nonexistent/config.yaml");

        let result = EnhancedConfig::load_from_sources().await;
        // Should succeed with defaults since file loading is optional
        assert!(result.is_ok());

        env::remove_var("CRUCIBLE_CONFIG_FILE");
    }

    #[test]
    fn test_enhanced_config_merge_with() {
        let mut base_config = EnhancedConfig::default();
        let mut file_config = EnhancedConfig::default();

        // Modify file config
        file_config.service.name = "merged-service".to_string();
        file_config.service.environment = "production".to_string();
        file_config.logging.level = "debug".to_string();
        file_config.database = Some(DatabaseConfig {
            url: "postgresql://localhost/merged".to_string(),
            ..Default::default()
        });

        // Merge
        base_config.merge_with(file_config);

        assert_eq!(base_config.service.name, "merged-service");
        assert_eq!(base_config.service.environment, "production");
        assert_eq!(base_config.logging.level, "debug");
        assert!(base_config.database.is_some());
        assert_eq!(base_config.database.as_ref().unwrap().url, "postgresql://localhost/merged");
    }

    #[test]
    fn test_enhanced_config_validate() {
        let config = EnhancedConfig::default();
        let result = config.validate();
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_enhanced_config_validation_with_context() {
        let config = EnhancedConfig::default();
        let context = ValidationContext::new("test_config")
            .with_section("validation_test");

        let result = config.validate_with_context(context);
        assert!(result.is_valid);
    }

    #[test]
    fn test_enhanced_config_validation_rules() {
        let config = EnhancedConfig::default();
        let rules = config.validation_rules();
        assert!(!rules.is_empty());

        // Check that rules from all sections are included
        let sections: std::collections::HashSet<_> = rules.iter()
            .map(|rule| rule.field.split('.').next().unwrap_or(""))
            .collect();

        assert!(sections.contains("name")); // From service
        assert!(sections.contains("level")); // From logging
        assert!(sections.contains("max_event_age_seconds")); // From event routing
    }
}

#[cfg(test)]
mod service_config_tests {
    use super::*;

    #[test]
    fn test_service_config_default() {
        let config = ServiceConfig::default();

        assert_eq!(config.name, "crucible-services");
        assert_eq!(config.version, "0.1.0");
        assert_eq!(config.environment, "development");
        assert!(config.description.is_none());
        assert!(config.tags.is_empty());
    }

    #[test]
    fn test_service_config_validation_valid() {
        let config = ServiceConfig {
            name: "my-service".to_string(),
            version: "1.0.0".to_string(),
            environment: "production".to_string(),
            description: Some("Test service".to_string()),
            tags: vec!["web".to_string(), "api".to_string()],
        };

        let result = config.validate();
        assert!(result.is_valid);
    }

    #[test]
    fn test_service_config_validation_invalid_name() {
        let mut config = ServiceConfig::default();
        config.name = "".to_string(); // Empty name

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_service_config_validation_invalid_name_pattern() {
        let mut config = ServiceConfig::default();
        config.name = "Invalid Name!".to_string(); // Contains invalid characters

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_service_config_validation_invalid_environment() {
        let mut config = ServiceConfig::default();
        config.environment = "testing".to_string(); // Not in enum

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_service_config_validation_rules() {
        let config = ServiceConfig::default();
        let rules = config.validation_rules();
        assert!(!rules.is_empty());
        assert!(rules.iter().any(|r| r.field == "name"));
        assert!(rules.iter().any(|r| r.field == "environment"));
    }

    #[test]
    fn test_service_config_edge_cases() {
        let valid_names = vec![
            "service",
            "my-service",
            "my_service",
            "service123",
            "a", // Single character
        ];

        for name in valid_names {
            let config = ServiceConfig {
                name: name.to_string(),
                ..Default::default()
            };
            assert!(config.validate().is_valid, "Name '{}' should be valid", name);
        }

        let invalid_names = vec![
            "",
            "123service", // Starts with number
            "Service", // Starts with uppercase
            "my service", // Contains space
            "my@service", // Contains special char
            "my.service", // Contains dot
        ];

        for name in invalid_names {
            let config = ServiceConfig {
                name: name.to_string(),
                ..Default::default()
            };
            assert!(!config.validate().is_valid, "Name '{}' should be invalid", name);
        }
    }
}

#[cfg(test)]
mod logging_config_tests {
    use super::*;

    #[test]
    fn test_logging_config_default() {
        let config = LoggingConfig::default();

        assert_eq!(config.level, "info");
        assert_eq!(config.format, "json");
        assert!(!config.file_enabled);
        assert!(config.file_path.is_none());
        assert_eq!(config.max_file_size, Some(10 * 1024 * 1024));
        assert_eq!(config.max_files, Some(5));
        assert!(config.console_enabled);
        assert!(config.component_levels.is_empty());
        assert!(config.structured);
        assert_eq!(config.correlation_field, "trace_id");
    }

    #[test]
    fn test_logging_config_validation_valid() {
        let config = LoggingConfig {
            level: "debug".to_string(),
            format: "text".to_string(),
            file_enabled: true,
            file_path: Some(PathBuf::from("/var/log/app.log")),
            max_file_size: Some(100 * 1024 * 1024),
            max_files: Some(10),
            console_enabled: false,
            component_levels: HashMap::new(),
            structured: false,
            correlation_field: "request_id".to_string(),
        };

        let result = config.validate();
        assert!(result.is_valid);
    }

    #[test]
    fn test_logging_config_validation_invalid_level() {
        let mut config = LoggingConfig::default();
        config.level = "verbose".to_string(); // Invalid level

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_logging_config_validation_invalid_format() {
        let mut config = LoggingConfig::default();
        config.format = "xml".to_string(); // Invalid format

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_logging_config_validation_file_enabled_no_path() {
        let mut config = LoggingConfig::default();
        config.file_enabled = true;
        config.file_path = None; // No file path

        let result = config.validate();
        assert!(result.is_valid); // Still valid, but with warning
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_logging_config_validation_invalid_file_size() {
        let mut config = LoggingConfig::default();
        config.max_file_size = Some(0); // Invalid: zero size

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_logging_config_validation_invalid_max_files() {
        let mut config = LoggingConfig::default();
        config.max_files = Some(0); // Below minimum

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        config.max_files = Some(150); // Above maximum
        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_logging_config_validation_all_valid_levels() {
        let valid_levels = vec!["trace", "debug", "info", "warn", "error"];

        for level in valid_levels {
            let config = LoggingConfig {
                level: level.to_string(),
                ..Default::default()
            };
            assert!(config.validate().is_valid, "Level '{}' should be valid", level);
        }
    }

    #[test]
    fn test_logging_config_validation_all_valid_formats() {
        let valid_formats = vec!["json", "text", "compact"];

        for format in valid_formats {
            let config = LoggingConfig {
                format: format.to_string(),
                ..Default::default()
            };
            assert!(config.validate().is_valid, "Format '{}' should be valid", format);
        }
    }
}

#[cfg(test)]
mod event_routing_config_tests {
    use super::*;

    #[test]
    fn test_event_routing_config_default() {
        let config = EventRoutingConfig::default();

        assert_eq!(config.max_event_age_seconds, 300);
        assert_eq!(config.max_concurrent_events, 1000);
        assert_eq!(config.default_routing_strategy, "type_based");
        assert!(!config.enable_detailed_tracing);
        assert_eq!(config.routing_history_limit, 1000);
        assert_eq!(config.event_buffer_size, 10000);
        assert!(!config.enable_persistence);
        assert!(config.storage_path.is_none());
    }

    #[test]
    fn test_event_routing_config_validation_valid() {
        let config = EventRoutingConfig {
            max_event_age_seconds: 600,
            max_concurrent_events: 2000,
            default_routing_strategy: "round_robin".to_string(),
            enable_detailed_tracing: true,
            routing_history_limit: 5000,
            event_buffer_size: 50000,
            enable_persistence: true,
            storage_path: Some(PathBuf::from("/var/lib/crucible/events")),
        };

        let result = config.validate();
        assert!(result.is_valid);
    }

    #[test]
    fn test_event_routing_config_validation_invalid_max_age() {
        let mut config = EventRoutingConfig::default();
        config.max_event_age_seconds = 0; // Below minimum

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        config.max_event_age_seconds = 4000; // Above maximum
        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_event_routing_config_validation_invalid_concurrent_events() {
        let mut config = EventRoutingConfig::default();
        config.max_concurrent_events = 0; // Below minimum

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        config.max_concurrent_events = 200000; // Above maximum
        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_event_routing_config_validation_invalid_buffer_size() {
        let mut config = EventRoutingConfig::default();
        config.event_buffer_size = 50; // Below minimum

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        config.event_buffer_size = 2000000; // Above maximum
        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_event_routing_config_validation_persistence_no_path() {
        let mut config = EventRoutingConfig::default();
        config.enable_persistence = true;
        config.storage_path = None; // No storage path

        let result = config.validate();
        assert!(result.is_valid); // Still valid, but with warning
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_event_routing_config_boundary_values() {
        let test_cases = vec![
            (1, 1, 100), // Minimum values
            (3600, 100000, 1000000), // Maximum values
            (300, 1000, 10000), // Default values
        ];

        for (max_age, concurrent, buffer) in test_cases {
            let config = EventRoutingConfig {
                max_event_age_seconds: max_age,
                max_concurrent_events: concurrent,
                event_buffer_size: buffer,
                ..Default::default()
            };
            assert!(config.validate().is_valid,
                "Values (max_age: {}, concurrent: {}, buffer: {}) should be valid",
                max_age, concurrent, buffer);
        }
    }
}

#[cfg(test)]
mod database_config_tests {
    use super::*;

    #[test]
    fn test_database_config_default() {
        let config = DatabaseConfig::default();

        assert_eq!(config.url, "sqlite:crucible.db");
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.timeout_seconds, 30);
        assert!(config.enable_pooling);
        assert_eq!(config.db_type, "sqlite");
    }

    #[test]
    fn test_database_config_validation_valid() {
        let config = DatabaseConfig {
            url: "postgresql://user:pass@localhost:5432/dbname".to_string(),
            max_connections: 20,
            timeout_seconds: 60,
            enable_pooling: true,
            db_type: "postgres".to_string(),
        };

        let result = config.validate();
        assert!(result.is_valid);
    }

    #[test]
    fn test_database_config_validation_invalid_url() {
        let mut config = DatabaseConfig::default();
        config.url = "".to_string(); // Empty URL

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_database_config_validation_invalid_max_connections() {
        let mut config = DatabaseConfig::default();
        config.max_connections = 0; // Below minimum

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        config.max_connections = 2000; // Above maximum
        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_database_config_validation_invalid_db_type() {
        let mut config = DatabaseConfig::default();
        config.db_type = "oracle".to_string(); // Invalid type

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_database_config_validation_all_valid_types() {
        let valid_types = vec!["sqlite", "postgres", "mysql"];

        for db_type in valid_types {
            let config = DatabaseConfig {
                db_type: db_type.to_string(),
                ..Default::default()
            };
            assert!(config.validate().is_valid, "DB type '{}' should be valid", db_type);
        }
    }

    #[test]
    fn test_database_config_edge_cases() {
        let test_cases = vec![
            ("sqlite:memory.db", 1, 1, "sqlite"),
            ("postgresql://localhost/test", 1000, 3600, "postgres"),
            ("mysql://localhost/test", 500, 1800, "mysql"),
        ];

        for (url, max_conn, timeout, db_type) in test_cases {
            let config = DatabaseConfig {
                url: url.to_string(),
                max_connections: max_conn,
                timeout_seconds: timeout,
                db_type: db_type.to_string(),
                ..Default::default()
            };
            assert!(config.validate().is_valid,
                "Config (url: {}, max_conn: {}, timeout: {}, type: {}) should be valid",
                url, max_conn, timeout, db_type);
        }
    }
}

#[cfg(test)]
mod security_config_tests {
    use super::*;

    #[test]
    fn test_security_config_default() {
        let config = SecurityConfig::default();

        assert!(!config.encryption_enabled);
        assert!(config.encryption_key_path.is_none());
        assert!(!config.authentication_enabled);
        assert!(config.jwt_secret.is_none());
        assert_eq!(config.token_expiration_hours, 24);
        assert!(!config.rate_limiting_enabled);
        assert_eq!(config.rate_limit_rpm, 100);
    }

    #[test]
    fn test_security_config_validation_valid() {
        let config = SecurityConfig {
            encryption_enabled: true,
            encryption_key_path: Some(PathBuf::from("/etc/crucible/key.pem")),
            authentication_enabled: true,
            jwt_secret: Some("very-long-secure-jwt-secret-key-for-testing".to_string()),
            token_expiration_hours: 12,
            rate_limiting_enabled: true,
            rate_limit_rpm: 200,
        };

        let result = config.validate();
        assert!(result.is_valid);
    }

    #[test]
    fn test_security_config_validation_encryption_no_key() {
        let mut config = SecurityConfig::default();
        config.encryption_enabled = true;
        config.encryption_key_path = None; // Missing key path

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_security_config_validation_auth_no_jwt() {
        let mut config = SecurityConfig::default();
        config.authentication_enabled = true;
        config.jwt_secret = None; // Missing JWT secret

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_security_config_validation_short_jwt_secret() {
        let mut config = SecurityConfig::default();
        config.authentication_enabled = true;
        config.jwt_secret = Some("short".to_string()); // Too short

        let result = config.validate();
        assert!(result.is_valid); // Still valid, but with warning
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_security_config_validation_long_jwt_secret() {
        let config = SecurityConfig {
            authentication_enabled: true,
            jwt_secret: Some("this-is-a-very-long-secure-jwt-secret-key-that-meets-the-minimum-requirements".to_string()),
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_valid);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_security_config_edge_cases() {
        let test_cases = vec![
            (false, false, None), // No security features
            (true, false, Some(PathBuf::from("/path/to/key"))), // Only encryption
            (false, true, Some("valid-jwt-secret-32-chars".to_string())), // Only auth
            (true, true, Some(PathBuf::from("/path/to/key"))), // Both but missing JWT
        ];

        for (enc, auth, path_or_jwt) in test_cases {
            let config = match path_or_jwt {
                Some(path) if enc => SecurityConfig {
                    encryption_enabled: enc,
                    authentication_enabled: auth,
                    encryption_key_path: Some(path),
                    jwt_secret: if auth {
                        Some("valid-jwt-secret-32-chars".to_string())
                    } else {
                        None
                    },
                    ..Default::default()
                },
                Some(jwt) if auth => SecurityConfig {
                    encryption_enabled: enc,
                    authentication_enabled: auth,
                    jwt_secret: Some(jwt),
                    ..Default::default()
                },
                _ => SecurityConfig {
                    encryption_enabled: enc,
                    authentication_enabled: auth,
                    ..Default::default()
                }
            };

            let result = config.validate();
            // Should be valid unless encryption is enabled without key or auth without JWT
            let expected_valid = !(enc && config.encryption_key_path.is_none())
                && !(auth && config.jwt_secret.is_none());
            assert_eq!(result.is_valid, expected_valid);
        }
    }
}

#[cfg(test)]
mod performance_config_tests {
    use super::*;

    #[test]
    fn test_performance_config_default() {
        let config = PerformanceConfig::default();

        assert_eq!(config.max_memory_mb, 1024);
        assert!(!config.enable_memory_profiling);
        assert_eq!(config.cpu_threshold_percent, 80.0);
        assert!(!config.enable_monitoring);
        assert_eq!(config.metrics_interval_seconds, 60);
    }

    #[test]
    fn test_performance_config_validation_valid() {
        let config = PerformanceConfig {
            max_memory_mb: 2048,
            enable_memory_profiling: true,
            cpu_threshold_percent: 90.0,
            enable_monitoring: true,
            metrics_interval_seconds: 30,
        };

        let result = config.validate();
        assert!(result.is_valid);
    }

    #[test]
    fn test_performance_config_validation_invalid_memory() {
        let mut config = PerformanceConfig::default();
        config.max_memory_mb = 32; // Below minimum

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        config.max_memory_mb = 50000; // Above maximum
        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_performance_config_validation_invalid_cpu_threshold() {
        let mut config = PerformanceConfig::default();
        config.cpu_threshold_percent = 0.0; // Below minimum

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        config.cpu_threshold_percent = 150.0; // Above maximum
        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_performance_config_boundary_values() {
        let test_cases = vec![
            (64, 1.0), // Minimum values
            (32768, 100.0), // Maximum values
            (1024, 80.0), // Default values
        ];

        for (memory, cpu) in test_cases {
            let config = PerformanceConfig {
                max_memory_mb: memory,
                cpu_threshold_percent: cpu,
                ..Default::default()
            };
            assert!(config.validate().is_valid,
                "Values (memory: {}, cpu: {}) should be valid", memory, cpu);
        }
    }
}

#[cfg(test)]
mod plugin_config_tests {
    use super::*;

    #[test]
    fn test_plugin_config_default() {
        let config = PluginConfig::default();

        assert!(config.enabled);
        assert_eq!(config.search_paths, vec![PathBuf::from("./plugins")]);
        assert!(config.enabled_plugins.is_empty());
        assert!(config.disabled_plugins.is_empty());
        assert!(config.plugin_configs.is_empty());
        assert!(config.enable_sandboxing);
        assert_eq!(config.plugin_timeout_seconds, 30);
    }

    #[test]
    fn test_plugin_config_validation_valid() {
        let mut plugin_configs = HashMap::new();
        plugin_configs.insert("test_plugin".to_string(), json!({"setting": "value"}));

        let config = PluginConfig {
            enabled: true,
            search_paths: vec![PathBuf::from("/usr/lib/crucible/plugins")],
            enabled_plugins: vec!["plugin1".to_string(), "plugin2".to_string()],
            disabled_plugins: vec!["old_plugin".to_string()],
            plugin_configs,
            enable_sandboxing: true,
            plugin_timeout_seconds: 60,
        };

        let result = config.validate();
        assert!(result.is_valid);
    }

    #[test]
    fn test_plugin_config_validation_duplicate_plugin() {
        let mut config = PluginConfig::default();
        config.enabled_plugins.push("test_plugin".to_string());
        config.disabled_plugins.push("test_plugin".to_string()); // Same plugin in both lists

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_plugin_config_validation_nonexistent_search_path() {
        let mut config = PluginConfig::default();
        config.search_paths.push(PathBuf::from("/nonexistent/path"));

        let result = config.validate();
        assert!(result.is_valid); // Still valid, but with warning
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_plugin_config_validation_timeout() {
        let config = PluginConfig {
            plugin_timeout_seconds: 0, // Invalid: zero timeout
            ..Default::default()
        };

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_plugin_config_complex_scenario() {
        let mut config = PluginConfig::default();

        // Add multiple search paths (some may not exist)
        config.search_paths.extend_from_slice(&[
            PathBuf::from("./plugins"),
            PathBuf::from("/tmp/test_plugins"), // Likely doesn't exist
            PathBuf::from("/var/lib/crucible/plugins"),
        ]);

        // Add enabled and disabled plugins
        config.enabled_plugins.extend_from_slice(&[
            "logger_plugin".to_string(),
            "auth_plugin".to_string(),
            "metrics_plugin".to_string(),
        ]);

        config.disabled_plugins.extend_from_slice(&[
            "deprecated_plugin".to_string(),
            "experimental_plugin".to_string(),
        ]);

        // Add plugin-specific configurations
        let mut plugin_configs = HashMap::new();
        plugin_configs.insert("logger_plugin".to_string(), json!({
            "level": "info",
            "format": "json"
        }));
        plugin_configs.insert("auth_plugin".to_string(), json!({
            "provider": "oauth2",
            "timeout": 30
        }));
        config.plugin_configs = plugin_configs;

        let result = config.validate();
        assert!(result.is_valid);
        // Should have warnings for nonexistent search paths
        assert!(!result.warnings.is_empty());
    }
}

#[cfg(test)]
mod config_integration_tests {
    use super::*;

    #[test]
    fn test_full_config_validation() {
        let mut plugin_configs = HashMap::new();
        plugin_configs.insert("test_plugin".to_string(), json!({"enabled": true}));

        let config = EnhancedConfig {
            service: ServiceConfig {
                name: "integration-test".to_string(),
                environment: "staging".to_string(),
                ..Default::default()
            },
            logging: LoggingConfig {
                level: "debug".to_string(),
                format: "json".to_string(),
                file_enabled: true,
                file_path: Some(PathBuf::from("/tmp/test.log")),
                ..Default::default()
            },
            event_routing: EventRoutingConfig {
                max_event_age_seconds: 120,
                max_concurrent_events: 500,
                enable_persistence: true,
                storage_path: Some(PathBuf::from("/tmp/events")),
                ..Default::default()
            },
            database: Some(DatabaseConfig {
                url: "sqlite:/tmp/test.db".to_string(),
                max_connections: 5,
                db_type: "sqlite".to_string(),
                ..Default::default()
            }),
            security: SecurityConfig {
                encryption_enabled: true,
                encryption_key_path: Some(PathBuf::from("/tmp/test.key")),
                authentication_enabled: true,
                jwt_secret: Some("test-jwt-secret-that-is-long-enough".to_string()),
                ..Default::default()
            },
            performance: PerformanceConfig {
                max_memory_mb: 512,
                enable_monitoring: true,
                ..Default::default()
            },
            plugins: PluginConfig {
                enabled_plugins: vec!["test_plugin".to_string()],
                plugin_configs,
                ..Default::default()
            },
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_config_with_validation_errors() {
        let config = EnhancedConfig {
            service: ServiceConfig {
                name: "".to_string(), // Invalid: empty name
                environment: "invalid".to_string(), // Invalid: not in enum
                ..Default::default()
            },
            logging: LoggingConfig {
                level: "invalid".to_string(), // Invalid: not in enum
                format: "invalid".to_string(), // Invalid: not in enum
                ..Default::default()
            },
            event_routing: EventRoutingConfig {
                max_event_age_seconds: 0, // Invalid: below minimum
                max_concurrent_events: 0, // Invalid: below minimum
                ..Default::default()
            },
            database: Some(DatabaseConfig {
                url: "".to_string(), // Invalid: empty URL
                max_connections: 0, // Invalid: below minimum
                db_type: "invalid".to_string(), // Invalid: not in enum
                ..Default::default()
            }),
            security: SecurityConfig {
                encryption_enabled: true,
                encryption_key_path: None, // Invalid: missing key path
                authentication_enabled: true,
                jwt_secret: None, // Invalid: missing JWT secret
                ..Default::default()
            },
            performance: PerformanceConfig {
                max_memory_mb: 0, // Invalid: below minimum
                cpu_threshold_percent: 0.0, // Invalid: below minimum
                ..Default::default()
            },
            plugins: PluginConfig {
                enabled_plugins: vec!["test".to_string()],
                disabled_plugins: vec!["test".to_string()], // Invalid: duplicate
                plugin_timeout_seconds: 0, // Invalid: zero timeout
                ..Default::default()
            },
            ..Default::default()
        };

        let result = config.validate();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Should have errors from multiple sections
        let error_fields: std::collections::HashSet<_> = result.errors
            .iter()
            .filter_map(|e| e.field())
            .collect();

        assert!(error_fields.contains("name"));
        assert!(error_fields.contains("environment"));
        assert!(error_fields.contains("level"));
        assert!(error_fields.contains("format"));
    }

    #[tokio::test]
    async fn test_config_loading_with_environment_overrides() {
        // Set multiple environment variables
        env::set_var("CRUCIBLE_SERVICE_NAME", "env-test-service");
        env::set_var("CRUCIBLE_ENVIRONMENT", "production");
        env::set_var("CRUCIBLE_LOG_LEVEL", "warn");
        env::set_var("CRUCIBLE_MAX_EVENT_AGE", "900");
        env::set_var("CRUCIBLE_MAX_CONCURRENT_EVENTS", "1500");

        let result = EnhancedConfig::load_from_sources().await;
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.service.name, "env-test-service");
        assert_eq!(config.service.environment, "production");
        assert_eq!(config.logging.level, "warn");
        assert_eq!(config.event_routing.max_event_age_seconds, 900);
        assert_eq!(config.event_routing.max_concurrent_events, 1500);

        // Should still be valid after environment overrides
        let validation_result = config.validate();
        assert!(validation_result.is_valid);

        // Cleanup
        env::remove_var("CRUCIBLE_SERVICE_NAME");
        env::remove_var("CRUCIBLE_ENVIRONMENT");
        env::remove_var("CRUCIBLE_LOG_LEVEL");
        env::remove_var("CRUCIBLE_MAX_EVENT_AGE");
        env::remove_var("CRUCIBLE_MAX_CONCURRENT_EVENTS");
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let original_config = EnhancedConfig {
            service: ServiceConfig {
                name: "roundtrip-test".to_string(),
                version: "2.1.0".to_string(),
                environment: "production".to_string(),
                description: Some("Test config serialization".to_string()),
                tags: vec!["test".to_string(), "serialization".to_string()],
            },
            database: Some(DatabaseConfig {
                url: "postgresql://localhost/roundtrip".to_string(),
                max_connections: 15,
                timeout_seconds: 45,
                enable_pooling: false,
                db_type: "postgres".to_string(),
            }),
            ..Default::default()
        };

        // Test JSON roundtrip
        let json_str = serde_json::to_string_pretty(&original_config).unwrap();
        let json_config: EnhancedConfig = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original_config.service.name, json_config.service.name);
        assert_eq!(original_config.database.as_ref().unwrap().url,
                  json_config.database.as_ref().unwrap().url);

        // Test YAML roundtrip
        let yaml_str = serde_yaml::to_string(&original_config).unwrap();
        let yaml_config: EnhancedConfig = serde_yaml::from_str(&yaml_str).unwrap();
        assert_eq!(original_config.service.name, yaml_config.service.name);
        assert_eq!(original_config.database.as_ref().unwrap().url,
                  yaml_config.database.as_ref().unwrap().url);

        // Test TOML roundtrip
        let toml_str = toml::to_string_pretty(&original_config).unwrap();
        let toml_config: EnhancedConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(original_config.service.name, toml_config.service.name);
        assert_eq!(original_config.database.as_ref().unwrap().url,
                  toml_config.database.as_ref().unwrap().url);
    }

    #[test]
    fn test_config_edge_cases_and_boundary_values() {
        // Test with empty environment overrides
        let config = EnhancedConfig {
            environment_overrides: HashMap::new(),
            ..Default::default()
        };
        assert!(config.validate().is_valid);

        // Test with various boundary values
        let boundary_config = EnhancedConfig {
            event_routing: EventRoutingConfig {
                max_event_age_seconds: 1, // Minimum
                max_concurrent_events: 1, // Minimum
                event_buffer_size: 100, // Minimum
                ..Default::default()
            },
            performance: PerformanceConfig {
                max_memory_mb: 64, // Minimum
                cpu_threshold_percent: 1.0, // Minimum
                metrics_interval_seconds: 1, // Custom minimum
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(boundary_config.validate().is_valid);

        // Test with maximum values
        let max_config = EnhancedConfig {
            event_routing: EventRoutingConfig {
                max_event_age_seconds: 3600, // Maximum
                max_concurrent_events: 100000, // Maximum
                event_buffer_size: 1000000, // Maximum
                ..Default::default()
            },
            performance: PerformanceConfig {
                max_memory_mb: 32768, // Maximum
                cpu_threshold_percent: 100.0, // Maximum
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(max_config.validate().is_valid);
    }
use std::io::Write;
use serde_json::json;
