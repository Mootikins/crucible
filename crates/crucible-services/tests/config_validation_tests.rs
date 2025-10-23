//! Comprehensive unit tests for configuration validation system
//!
//! This module provides thorough testing coverage for the Phase 7.3
//! configuration validation framework with >95% coverage target.

use crucible_services::config::validation::*;
use serde_json::json;
use std::collections::HashMap;

#[cfg(test)]
mod validation_error_tests {
    use super::*;

    #[test]
    fn test_missing_field_error() {
        let context = ValidationContext::new("test.yaml")
            .with_section("database")
            .with_field("connection_string");

        let error = ValidationError::MissingField {
            field: "database_url".to_string(),
            context: context.clone(),
        };

        assert_eq!(error.field(), Some("database_url"));
        assert_eq!(error.context(), Some(&context));
        assert!(!error.is_recoverable());
        assert_eq!(error.severity(), ValidationSeverity::Error);

        let description = error.description();
        assert!(description.contains("database_url"));
        assert!(description.contains("test.yaml"));
        assert!(description.contains("required"));
    }

    #[test]
    fn test_invalid_value_error_with_suggestion() {
        let context = ValidationContext::new("env")
            .with_section("logging")
            .with_field("level");

        let error = ValidationError::InvalidValue {
            field: "log_level".to_string(),
            value: "verbose".to_string(),
            reason: "Invalid log level".to_string(),
            context: context.clone(),
            suggested_fix: Some("Use one of: trace, debug, info, warn, error".to_string()),
        };

        assert_eq!(error.field(), Some("log_level"));
        assert!(error.is_recoverable());
        assert_eq!(error.severity(), ValidationSeverity::Error);

        let description = error.description();
        assert!(description.contains("verbose"));
        assert!(description.contains("Use one of:"));
    }

    #[test]
    fn test_invalid_value_error_without_suggestion() {
        let context = ValidationContext::new("test.json");

        let error = ValidationError::InvalidValue {
            field: "port".to_string(),
            value: "abc".to_string(),
            reason: "Port must be a number".to_string(),
            context,
            suggested_fix: None,
        };

        assert!(!error.is_recoverable());
        assert_eq!(error.severity(), ValidationSeverity::Error);
    }

    #[test]
    fn test_dependency_violation_error() {
        let context = ValidationContext::new("config.yaml")
            .with_section("security");

        let error = ValidationError::DependencyViolation {
            message: "SSL enabled but certificate not provided".to_string(),
            field: "ssl_enabled".to_string(),
            depends_on: "ssl_certificate_path".to_string(),
            context,
        };

        assert_eq!(error.field(), Some("ssl_enabled"));
        assert!(!error.is_recoverable());
        assert_eq!(error.severity(), ValidationSeverity::Error);
    }

    #[test]
    fn test_type_mismatch_error() {
        let context = ValidationContext::new("config.toml");

        let error = ValidationError::TypeMismatch {
            field: "max_connections".to_string(),
            expected_type: "number".to_string(),
            actual_type: "string".to_string(),
            context,
        };

        assert_eq!(error.field(), Some("max_connections"));
        assert!(!error.is_recoverable());
        assert_eq!(error.severity(), ValidationSeverity::Error);
    }

    #[test]
    fn test_constraint_violation_error() {
        let context = ValidationContext::new("config.yaml")
            .with_section("performance");

        let error = ValidationError::ConstraintViolation {
            field: "memory_limit_mb".to_string(),
            constraint: "minimum value".to_string(),
            details: "Value 64 is less than minimum 128".to_string(),
            context,
        };

        assert_eq!(error.field(), Some("memory_limit_mb"));
        assert!(!error.is_recoverable());
        assert_eq!(error.severity(), ValidationSeverity::Warning);
    }

    #[test]
    fn test_parse_error() {
        let error = ValidationError::ParseError {
            file_source: "config.yaml".to_string(),
            error: "invalid YAML syntax".to_string(),
            line: Some(15),
            column: Some(8),
        };

        assert!(error.field().is_none());
        assert!(error.context().is_none());
        assert!(!error.is_recoverable());
        assert_eq!(error.severity(), ValidationSeverity::Error);

        let description = error.description();
        assert!(description.contains("config.yaml"));
        assert!(description.contains("line 15"));
        assert!(description.contains("column 8"));
    }

    #[test]
    fn test_environment_error() {
        let error = ValidationError::EnvironmentError {
            variable: "DATABASE_URL".to_string(),
            error: "variable not set".to_string(),
            env_source: Some("system".to_string()),
        };

        assert!(error.field().is_none());
        assert!(error.context().is_none());
        assert!(error.is_recoverable());
        assert_eq!(error.severity(), ValidationSeverity::Warning);
    }

    #[test]
    fn test_multiple_errors() {
        let error1 = ValidationError::MissingField {
            field: "host".to_string(),
            context: ValidationContext::new("test.yaml"),
        };
        let error2 = ValidationError::InvalidValue {
            field: "port".to_string(),
            value: "invalid".to_string(),
            reason: "not a number".to_string(),
            context: ValidationContext::new("test.yaml"),
            suggested_fix: None,
        };

        let multi_error = ValidationError::MultipleErrors {
            count: 2,
            errors: vec![error1, error2],
        };

        assert!(multi_error.field().is_none());
        assert!(multi_error.context().is_none());
        assert!(!multi_error.is_recoverable());
        assert_eq!(multi_error.severity(), ValidationSeverity::Error);
    }

    #[test]
    fn test_error_severity_classification() {
        let cases = vec![
            (ValidationError::MissingField { field: "test".to_string(), context: ValidationContext::new("test") }, ValidationSeverity::Error),
            (ValidationError::ParseError { file_source: "test".to_string(), error: "error".to_string(), line: None, column: None }, ValidationSeverity::Error),
            (ValidationError::ConstraintViolation { field: "test".to_string(), constraint: "test".to_string(), details: "test".to_string(), context: ValidationContext::new("test") }, ValidationSeverity::Warning),
            (ValidationError::EnvironmentError { variable: "TEST".to_string(), error: "error".to_string(), env_source: None }, ValidationSeverity::Warning),
        ];

        for (error, expected_severity) in cases {
            assert_eq!(error.severity(), expected_severity);
        }
    }
}

#[cfg(test)]
mod validation_context_tests {
    use super::*;

    #[test]
    fn test_validation_context_creation() {
        let context = ValidationContext::new("test.yaml");

        assert_eq!(context.source, "test.yaml");
        assert!(context.section.is_none());
        assert!(context.field_path.is_empty());
        assert!(context.metadata.is_empty());
    }

    #[test]
    fn test_validation_context_builder() {
        let context = ValidationContext::new("config.yaml")
            .with_section("database")
            .with_field("host")
            .with_field("port")
            .with_metadata("environment", "production")
            .with_metadata("version", "1.0");

        assert_eq!(context.source, "config.yaml");
        assert_eq!(context.section, Some("database".to_string()));
        assert_eq!(context.field_path, vec!["host", "port"]);
        assert_eq!(context.metadata.get("environment"), Some(&"production".to_string()));
        assert_eq!(context.metadata.get("version"), Some(&"1.0".to_string()));
    }

    #[test]
    fn test_full_field_path() {
        let empty_context = ValidationContext::new("test");
        assert_eq!(empty_context.full_field_path(), "root");

        let context = ValidationContext::new("test")
            .with_field("database")
            .with_field("host");
        assert_eq!(context.full_field_path(), "database.host");
    }

    #[test]
    fn test_context_serialization() {
        let context = ValidationContext::new("test.yaml")
            .with_section("logging")
            .with_field("level")
            .with_metadata("env", "prod");

        // Test that context can be serialized/deserialized
        let serialized = serde_json::to_string(&context).unwrap();
        let deserialized: ValidationContext = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.source, "test.yaml");
        assert_eq!(deserialized.section, Some("logging".to_string()));
        assert_eq!(deserialized.field_path, vec!["level"]);
    }
}

#[cfg(test)]
mod validation_result_tests {
    use super::*;

    #[test]
    fn test_validation_result_success() {
        let result = ValidationResult::success();

        assert!(result.is_valid);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
        assert!(result.info.is_empty());
        assert!(!result.has_issues());
    }

    #[test]
    fn test_validation_result_with_errors() {
        let error = ValidationError::MissingField {
            field: "required_field".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = ValidationResult::with_errors(vec![error]);

        assert!(!result.is_valid);
        assert_eq!(result.errors.len(), 1);
        assert!(result.warnings.is_empty());
        assert!(result.has_issues());
    }

    #[test]
    fn test_validation_result_with_warnings() {
        let warning = ValidationError::ConstraintViolation {
            field: "timeout".to_string(),
            constraint: "recommended range".to_string(),
            details: "Value is outside recommended range".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = ValidationResult::with_errors(vec![warning]);

        assert!(result.is_valid); // Warnings don't make it invalid
        assert!(result.errors.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert!(result.has_issues());
    }

    #[test]
    fn test_validation_result_builder_pattern() {
        let error = ValidationError::InvalidValue {
            field: "port".to_string(),
            value: "invalid".to_string(),
            reason: "not a number".to_string(),
            context: ValidationContext::new("test"),
            suggested_fix: Some("Use a valid port number".to_string()),
        };

        let result = ValidationResult::success()
            .with_error(error.clone())
            .with_warning(error.clone())
            .with_info("Configuration loaded successfully".to_string());

        assert!(!result.is_valid);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.info.len(), 1);
        assert!(result.has_issues());
    }

    #[test]
    fn test_validation_result_mixed_severities() {
        let errors = vec![
            ValidationError::MissingField {
                field: "host".to_string(),
                context: ValidationContext::new("test"),
            },
            ValidationError::ConstraintViolation {
                field: "timeout".to_string(),
                constraint: "minimum".to_string(),
                details: "Below minimum value".to_string(),
                context: ValidationContext::new("test"),
            },
        ];

        let result = ValidationResult::with_errors(errors);

        assert!(!result.is_valid);
        assert_eq!(result.errors.len(), 1); // Only the error
        assert_eq!(result.warnings.len(), 1); // The constraint violation
    }

    #[test]
    fn test_validation_result_into_service_result() {
        let success_result = ValidationResult::success();
        assert!(success_result.into_service_result().is_ok());

        let error = ValidationError::MissingField {
            field: "test".to_string(),
            context: ValidationContext::new("test"),
        };
        let error_result = ValidationResult::with_errors(vec![error]);
        assert!(error_result.into_service_result().is_err());
    }

    #[test]
    fn test_validation_result_logging() {
        // This test ensures the logging methods don't panic
        let result = ValidationResult::success()
            .with_info("Test info message".to_string())
            .with_warning(ValidationError::ConstraintViolation {
                field: "test".to_string(),
                constraint: "test".to_string(),
                details: "test".to_string(),
                context: ValidationContext::new("test"),
            });

        // These should not panic
        result.log_result("test_component");
    }
}

#[cfg(test)]
mod validation_engine_tests {
    use super::*;

    #[test]
    fn test_validation_engine_creation() {
        let engine = ValidationEngine::new();
        assert!(engine.rules.is_empty());
    }

    #[test]
    fn test_validation_engine_add_rule() {
        let mut engine = ValidationEngine::new();
        let rule = ValidationRule {
            field: "test_field".to_string(),
            rule_type: ValidationRuleType::Required,
            parameters: HashMap::new(),
            error_message: "Field is required".to_string(),
            required: true,
        };

        engine.add_rule("test_field", rule);
        assert_eq!(engine.rules.get("test_field").unwrap().len(), 1);
    }

    #[test]
    fn test_validation_engine_add_multiple_rules() {
        let mut engine = ValidationEngine::new();
        let rules = vec![
            ValidationRule {
                field: "port".to_string(),
                rule_type: ValidationRuleType::Required,
                parameters: HashMap::new(),
                error_message: "Port is required".to_string(),
                required: true,
            },
            ValidationRule {
                field: "port".to_string(),
                rule_type: ValidationRuleType::Range { min: Some(1.0), max: Some(65535.0) },
                parameters: HashMap::new(),
                error_message: "Port must be in range 1-65535".to_string(),
                required: true,
            },
        ];

        engine.add_rules("port", rules);
        assert_eq!(engine.rules.get("port").unwrap().len(), 2);
    }

    #[test]
    fn test_validation_rule_required() {
        let mut engine = ValidationEngine::new();
        engine.add_rule("host", ValidationRule {
            field: "host".to_string(),
            rule_type: ValidationRuleType::Required,
            parameters: HashMap::new(),
            error_message: "Host is required".to_string(),
            required: true,
        });

        let context = ValidationContext::new("test");

        // Test missing required field
        let value = json!(null);
        let result = engine.validate_field("host", &value, &context);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Test present required field
        let value = json!("localhost");
        let result = engine.validate_field("host", &value, &context);
        assert!(result.is_valid);
    }

    #[test]
    fn test_validation_rule_pattern() {
        let mut engine = ValidationEngine::new();
        engine.add_rule("email", ValidationRule {
            field: "email".to_string(),
            rule_type: ValidationRuleType::Pattern(r"^[^@]+@[^@]+\.[^@]+$".to_string()),
            parameters: HashMap::new(),
            error_message: "Invalid email format".to_string(),
            required: true,
        });

        let context = ValidationContext::new("test");

        // Test valid email
        let value = json!("test@example.com");
        let result = engine.validate_field("email", &value, &context);
        assert!(result.is_valid);

        // Test invalid email
        let value = json!("invalid-email");
        let result = engine.validate_field("email", &value, &context);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validation_rule_range() {
        let mut engine = ValidationEngine::new();
        engine.add_rule("port", ValidationRule {
            field: "port".to_string(),
            rule_type: ValidationRuleType::Range { min: Some(1.0), max: Some(65535.0) },
            parameters: HashMap::new(),
            error_message: "Port out of range".to_string(),
            required: true,
        });

        let context = ValidationContext::new("test");

        // Test valid range
        let value = json!(8080);
        let result = engine.validate_field("port", &value, &context);
        assert!(result.is_valid);

        // Test below minimum
        let value = json!(0);
        let result = engine.validate_field("port", &value, &context);
        assert!(!result.is_valid);

        // Test above maximum
        let value = json!(70000);
        let result = engine.validate_field("port", &value, &context);
        assert!(!result.is_valid);

        // Test non-numeric value
        let value = json!("not-a-number");
        let result = engine.validate_field("port", &value, &context);
        assert!(result.is_valid); // Non-numeric values pass range validation
    }

    #[test]
    fn test_validation_rule_enum() {
        let mut engine = ValidationEngine::new();
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

        let context = ValidationContext::new("test");

        // Test valid enum values
        for env in ["development", "staging", "production"] {
            let value = json!(env);
            let result = engine.validate_field("environment", &value, &context);
            assert!(result.is_valid);
        }

        // Test invalid enum value
        let value = json!("testing");
        let result = engine.validate_field("environment", &value, &context);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validation_rule_non_empty() {
        let mut engine = ValidationEngine::new();
        engine.add_rule("name", ValidationRule {
            field: "name".to_string(),
            rule_type: ValidationRuleType::NonEmpty,
            parameters: HashMap::new(),
            error_message: "Name cannot be empty".to_string(),
            required: true,
        });

        let context = ValidationContext::new("test");

        // Test non-empty string
        let value = json!("John Doe");
        let result = engine.validate_field("name", &value, &context);
        assert!(result.is_valid);

        // Test empty string
        let value = json!("");
        let result = engine.validate_field("name", &value, &context);
        assert!(!result.is_valid);

        // Test whitespace-only string
        let value = json!("   ");
        let result = engine.validate_field("name", &value, &context);
        assert!(!result.is_valid);

        // Test non-string value
        let value = json!(123);
        let result = engine.validate_field("name", &value, &context);
        assert!(result.is_valid); // Non-strings pass non-empty validation
    }

    #[test]
    fn test_validation_rule_positive() {
        let mut engine = ValidationEngine::new();
        engine.add_rule("count", ValidationRule {
            field: "count".to_string(),
            rule_type: ValidationRuleType::Positive,
            parameters: HashMap::new(),
            error_message: "Count must be positive".to_string(),
            required: true,
        });

        let context = ValidationContext::new("test");

        // Test positive numbers
        let value = json!(42);
        let result = engine.validate_field("count", &value, &context);
        assert!(result.is_valid);

        // Test zero
        let value = json!(0);
        let result = engine.validate_field("count", &value, &context);
        assert!(!result.is_valid);

        // Test negative number
        let value = json!(-5);
        let result = engine.validate_field("count", &value, &context);
        assert!(!result.is_valid);

        // Test non-numeric value
        let value = json!("not-a-number");
        let result = engine.validate_field("count", &value, &context);
        assert!(result.is_valid); // Non-numeric values pass positive validation
    }

    #[test]
    fn test_validation_engine_config_validation() {
        let mut engine = ValidationEngine::new();

        // Add multiple rules for different fields
        engine.add_rules("host", vec![
            ValidationRule {
                field: "host".to_string(),
                rule_type: ValidationRuleType::Required,
                parameters: HashMap::new(),
                error_message: "Host is required".to_string(),
                required: true,
            },
            ValidationRule {
                field: "host".to_string(),
                rule_type: ValidationRuleType::NonEmpty,
                parameters: HashMap::new(),
                error_message: "Host cannot be empty".to_string(),
                required: true,
            },
        ]);

        engine.add_rule("port", ValidationRule {
            field: "port".to_string(),
            rule_type: ValidationRuleType::Range { min: Some(1.0), max: Some(65535.0) },
            parameters: HashMap::new(),
            error_message: "Port out of range".to_string(),
            required: true,
        });

        let config = json!({
            "host": "localhost",
            "port": 8080,
            "extra_field": "should_not_cause_error"
        });

        let context = ValidationContext::new("test");
        let result = engine.validate_config(&config, &context);

        assert!(result.is_valid);
    }

    #[test]
    fn test_validation_engine_complex_config() {
        let mut engine = ValidationEngine::new();

        engine.add_rule("database_url", ValidationRule {
            field: "database_url".to_string(),
            rule_type: ValidationRuleType::Required,
            parameters: HashMap::new(),
            error_message: "Database URL is required".to_string(),
            required: true,
        });

        engine.add_rules("max_connections", vec![
            ValidationRule {
                field: "max_connections".to_string(),
                rule_type: ValidationRuleType::Required,
                parameters: HashMap::new(),
                error_message: "Max connections is required".to_string(),
                required: true,
            },
            ValidationRule {
                field: "max_connections".to_string(),
                rule_type: ValidationRuleType::Positive,
                parameters: HashMap::new(),
                error_message: "Max connections must be positive".to_string(),
                required: true,
            },
        ]);

        // Config with missing required field and invalid value
        let config = json!({
            "max_connections": -5,  // Invalid: negative
            "timeout": 30           // Should not cause error
        });

        let context = ValidationContext::new("test");
        let result = engine.validate_config(&config, &context);

        assert!(!result.is_valid);
        // Should have error for missing database_url and negative max_connections
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validation_rule_invalid_regex() {
        let mut engine = ValidationEngine::new();

        // Add rule with invalid regex pattern
        engine.add_rule("test_field", ValidationRule {
            field: "test_field".to_string(),
            rule_type: ValidationRuleType::Pattern("[invalid regex".to_string()),
            parameters: HashMap::new(),
            error_message: "Invalid pattern".to_string(),
            required: true,
        });

        let value = json!("test");
        let context = ValidationContext::new("test");
        let result = engine.validate_field("test_field", &value, &context);

        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validation_engine_default() {
        let engine = ValidationEngine::default();
        assert!(engine.rules.is_empty());
    }

    #[test]
    fn test_complex_validation_scenario() {
        let mut engine = ValidationEngine::new();

        // Simulate a complex configuration validation
        engine.add_rules("service", vec![
            ValidationRule {
                field: "service".to_string(),
                rule_type: ValidationRuleType::Required,
                parameters: HashMap::new(),
                error_message: "Service name is required".to_string(),
                required: true,
            },
            ValidationRule {
                field: "service".to_string(),
                rule_type: ValidationRuleType::Pattern(r"^[a-z][a-z0-9-]*$".to_string()),
                parameters: HashMap::new(),
                error_message: "Service name must be lowercase with hyphens".to_string(),
                required: true,
            },
        ]);

        engine.add_rules("port", vec![
            ValidationRule {
                field: "port".to_string(),
                rule_type: ValidationRuleType::Required,
                parameters: HashMap::new(),
                error_message: "Port is required".to_string(),
                required: true,
            },
            ValidationRule {
                field: "port".to_string(),
                rule_type: ValidationRuleType::Range { min: Some(1024.0), max: Some(65535.0) },
                parameters: HashMap::new(),
                error_message: "Port must be in range 1024-65535".to_string(),
                required: true,
            },
        ]);

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
        let valid_config = json!({
            "service": "my-service",
            "port": 8080,
            "environment": "development"
        });

        let context = ValidationContext::new("valid_test");
        let result = engine.validate_config(&valid_config, &context);
        assert!(result.is_valid);

        // Test invalid configuration
        let invalid_config = json!({
            "service": "Invalid-Service",  // Uppercase and invalid chars
            "port": 80,                    // Below minimum
            "environment": "testing"       // Invalid enum value
        });

        let context = ValidationContext::new("invalid_test");
        let result = engine.validate_config(&invalid_config, &context);
        assert!(!result.is_valid);
        assert!(result.errors.len() >= 3); // At least one error per field
    }

    #[test]
    fn test_performance_large_config_validation() {
        let mut engine = ValidationEngine::new();

        // Add rules for many fields
        for i in 0..100 {
            engine.add_rule(&format!("field_{}", i), ValidationRule {
                field: format!("field_{}", i),
                rule_type: ValidationRuleType::Required,
                parameters: HashMap::new(),
                error_message: "Field is required".to_string(),
                required: true,
            });
        }

        // Create large config
        let mut config_map = serde_json::Map::new();
        for i in 0..100 {
            config_map.insert(format!("field_{}", i), json!(format!("value_{}", i)));
        }
        let config = json!(config_map);

        let context = ValidationContext::new("performance_test");
        let start = std::time::Instant::now();
        let result = engine.validate_config(&config, &context);
        let duration = start.elapsed();

        assert!(result.is_valid);
        assert!(duration.as_millis() < 100); // Should complete quickly
    }
}