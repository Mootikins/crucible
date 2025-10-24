//! Comprehensive unit tests for configuration error handling
//!
//! This module provides thorough testing coverage for the Phase 7.3
//! configuration error handling framework with >95% coverage target.

use crucible_services::config::error_handling::*;
use crucible_services::config::validation::*;
use std::collections::HashMap;
use std::env;
use chrono::Utc;

#[cfg(test)]
mod config_error_handler_tests {
    use super::*;

    #[test]
    fn test_config_error_handler_new() {
        let handler = ConfigErrorHandler::new();

        assert_eq!(handler.error_history().len(), 0);
        assert_eq!(handler.max_history_size()(), 1000);
        assert!(!handler.reporting_config().enable_reporting);
        assert!(!handler.recovery_strategies()().is_empty());
    }

    #[test]
    fn test_config_error_handler_with_config() {
        let reporting_config = ErrorReportingConfig {
            enable_reporting: true,
            reporting_endpoint: Some("http://example.com/errors".to_string()),
            report_warnings: true,
            report_info: true,
            max_reports_per_hour: Some(50),
            include_context: false,
        };

        let handler = ConfigErrorHandler::with_config(reporting_config.clone());

        assert_eq!(handler.reporting_config().enable_reporting, true);
        assert_eq!(handler.reporting_config().reporting_endpoint, reporting_config.reporting_endpoint);
        assert_eq!(handler.reporting_config().max_reports_per_hour, Some(50));
    }

    #[test]
    fn test_config_error_handler_default() {
        let handler = ConfigErrorHandler::default();

        assert_eq!(handler.error_history().len(), 0);
        assert!(!handler.reporting_config().enable_reporting);
    }

    #[test]
    fn test_handle_validation_error_missing_field() {
        let mut handler = ConfigErrorHandler::new();

        let error = ValidationError::MissingField {
            field: "required_field".to_string(),
            context: ValidationContext::new("test.yaml"),
        };

        let result = handler.handle_validation_error(error.clone());

        match result {
            ErrorHandlingResult::Failed(handled_error) => {
                assert_eq!(handled_error.field(), Some("required_field"));
            }
            _ => panic!("Expected Failed result"),
        }

        assert_eq!(handler.error_history().len(), 1);
        assert_eq!(handler.error_history()[0].field, Some("required_field".to_string()));
    }

    #[test]
    fn test_handle_validation_error_recoverable() {
        let mut handler = ConfigErrorHandler::new();

        let error = ValidationError::InvalidValue {
            field: "log_level".to_string(),
            value: "invalid".to_string(),
            reason: "Invalid log level".to_string(),
            context: ValidationContext::new("test"),
            suggested_fix: Some("Use valid log level".to_string()),
        };

        let result = handler.handle_validation_error(error.clone());

        match result {
            ErrorHandlingResult::Recovered(recovery_result) => {
                assert_eq!(recovery_result.strategy, "default_value");
                assert!(recovery_result.description.contains("Applied default value"));
            }
            _ => panic!("Expected Recovered result"),
        }

        assert_eq!(handler.error_history().len(), 1);
        assert!(handler.error_history()[0].recovery_attempted);
        assert!(handler.error_history()[0].recovery_successful);
    }

    #[test]
    fn test_handle_validation_error_warning() {
        let mut handler = ConfigErrorHandler::new();

        let error = ValidationError::ConstraintViolation {
            field: "timeout".to_string(),
            constraint: "recommended range".to_string(),
            details: "Value is outside recommended range".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = handler.handle_validation_error(error.clone());

        match result {
            ErrorHandlingResult::Warning(warning) => {
                assert_eq!(warning.field(), Some("timeout"));
            }
            _ => panic!("Expected Warning result"),
        }

        assert_eq!(handler.error_history().len(), 1);
        assert_eq!(handler.error_history()[0].severity, ValidationSeverity::Warning);
    }

    #[test]
    fn test_handle_validation_error_critical() {
        let mut handler = ConfigErrorHandler::new();

        let error = ValidationError::ParseError {
            file_source: "config.yaml".to_string(),
            error: "YAML syntax error".to_string(),
            line: Some(10),
            column: Some(5),
        };

        let result = handler.handle_validation_error(error.clone());

        match result {
            ErrorHandlingResult::Critical(critical_error) => {
                assert!(matches!(critical_error, ValidationError::ParseError { .. }));
            }
            _ => panic!("Expected Critical result"),
        }

        assert_eq!(handler.error_history().len(), 1);
        assert_eq!(handler.error_history()[0].severity, ValidationSeverity::Error);
    }

    #[test]
    fn test_handle_validation_result_success() {
        let mut handler = ConfigErrorHandler::new();
        let result = ValidationResult::success();

        let handling_result = handler.handle_validation_result(result);

        match handling_result {
            ErrorHandlingResult::Success => {
                // Expected
            }
            _ => panic!("Expected Success result"),
        }

        assert_eq!(handler.error_history().len(), 0);
    }

    #[test]
    fn test_handle_validation_result_with_errors() {
        let mut handler = ConfigErrorHandler::new();

        let errors = vec![
            ValidationError::MissingField {
                field: "host".to_string(),
                context: ValidationContext::new("test"),
            },
            ValidationError::InvalidValue {
                field: "port".to_string(),
                value: "invalid".to_string(),
                reason: "not a number".to_string(),
                context: ValidationContext::new("test"),
                suggested_fix: Some("Use valid port number".to_string()),
            },
        ];

        let result = ValidationResult::with_errors(errors.clone());
        let handling_result = handler.handle_validation_result(result);

        match handling_result {
            ErrorHandlingResult::Failed(multi_error) => {
                if let ValidationError::MultipleErrors { count, .. } = multi_error {
                    assert_eq!(count, 2);
                } else {
                    panic!("Expected MultipleErrors");
                }
            }
            _ => panic!("Expected Failed result"),
        }

        assert_eq!(handler.error_history().len(), 2);
    }

    #[test]
    fn test_handle_validation_result_with_warnings() {
        let mut handler = ConfigErrorHandler::new();

        let warnings = vec![
            ValidationError::ConstraintViolation {
                field: "timeout".to_string(),
                constraint: "recommended".to_string(),
                details: "Above recommended value".to_string(),
                context: ValidationContext::new("test"),
            },
        ];

        let result = ValidationResult::with_errors(warnings.clone());
        let handling_result = handler.handle_validation_result(result);

        match handling_result {
            ErrorHandlingResult::Warning(warning) => {
                if let ValidationError::MultipleErrors { count, .. } = warning {
                    assert_eq!(count, 1);
                } else {
                    panic!("Expected MultipleErrors");
                }
            }
            _ => panic!("Expected Warning result"),
        }

        assert_eq!(handler.error_history().len(), 1);
    }

    #[test]
    fn test_handle_validation_result_mixed_severities() {
        let mut handler = ConfigErrorHandler::new();

        let errors = vec![
            ValidationError::ConstraintViolation {
                field: "memory".to_string(),
                constraint: "recommended".to_string(),
                details: "High memory usage".to_string(),
                context: ValidationContext::new("test"),
            },
            ValidationError::MissingField {
                field: "critical_field".to_string(),
                context: ValidationContext::new("test"),
            },
        ];

        let result = ValidationResult::with_errors(errors);
        let handling_result = handler.handle_validation_result(result);

        // Should prioritize errors over warnings
        match handling_result {
            ErrorHandlingResult::Failed(_) => {
                // Expected - errors take precedence
            }
            _ => panic!("Expected Failed result due to error severity"),
        }

        assert_eq!(handler.error_history().len(), 2);
    }

    #[test]
    fn test_error_history_management() {
        let mut handler = ConfigErrorHandler::new();
        handler.max_history_size() = 5;

        // Add more errors than the history size
        for i in 0..10 {
            let error = ValidationError::MissingField {
                field: format!("field_{}", i),
                context: ValidationContext::new("test"),
            };

            handler.handle_validation_error(error);
        }

        // Should only keep the most recent errors
        assert_eq!(handler.error_history().len(), 5);
        assert_eq!(handler.error_history()[0].field, Some("field_5".to_string()));
        assert_eq!(handler.error_history()[4].field, Some("field_9".to_string()));
    }

    #[test]
    fn test_clear_history() {
        let mut handler = ConfigErrorHandler::new();

        // Add some errors
        for i in 0..3 {
            let error = ValidationError::MissingField {
                field: format!("field_{}", i),
                context: ValidationContext::new("test"),
            };

            handler.handle_validation_error(error);
        }

        assert_eq!(handler.error_history().len(), 3);

        handler.clear_history();
        assert_eq!(handler.error_history().len(), 0);
    }

    #[test]
    fn test_get_recent_errors() {
        let mut handler = ConfigErrorHandler::new();

        // Add some errors
        for i in 0..10 {
            let error = ValidationError::MissingField {
                field: format!("field_{}", i),
                context: ValidationContext::new("test"),
            };

            handler.handle_validation_error(error);
        }

        let recent = handler.get_recent_errors(5);
        assert_eq!(recent.len(), 5);
        assert_eq!(recent[0].field, Some("field_5".to_string()));
        assert_eq!(recent[4].field, Some("field_9".to_string()));

        let all_recent = handler.get_recent_errors(100);
        assert_eq!(all_recent.len(), 10);
    }

    #[test]
    fn test_add_recovery_strategy() {
        let mut handler = ConfigErrorHandler::new();

        let custom_strategy = RecoveryStrategy::Custom("Custom recovery logic".to_string());
        handler.add_recovery_strategy("custom_field", custom_strategy.clone());

        // Test that the strategy is available
        let strategy = handler.recovery_strategies().get("custom_field").unwrap();
        assert_eq!(strategy.name(), "custom");
    }

    #[test]
    fn test_default_recovery_strategies()() {
        let handler = ConfigErrorHandler::new();

        // Check that default strategies exist
        assert!(handler.recovery_strategies().contains_key("logging.level"));
        assert!(handler.recovery_strategies().contains_key("logging.format"));
        assert!(handler.recovery_strategies().contains_key("service.environment"));
        assert!(handler.recovery_strategies().contains_key("event_routing.max_concurrent_events"));
        assert!(handler.recovery_strategies().contains_key("event_routing.max_event_age_seconds"));
    }
}

#[cfg(test)]
mod recovery_strategy_tests {
    use super::*;

    #[test]
    fn test_recovery_strategy_default_value() {
        let strategy = RecoveryStrategy::DefaultValue("test_value".to_string());

        let error = ValidationError::MissingField {
            field: "test_field".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = strategy.attempt_recovery(&error);
        assert!(result.is_ok());

        let recovery = result.unwrap();
        assert_eq!(recovery.strategy, "default_value");
        assert_eq!(recovery.recovered_value, "test_value");
        assert_eq!(recovery.original_value, Some("test_field".to_string()));
        assert!(recovery.description.contains("Applied default value"));
    }

    #[test]
    fn test_recovery_strategy_environment_variable() {
        let strategy = RecoveryStrategy::EnvironmentVariable("TEST_VAR".to_string());

        // Set environment variable
        env::set_var("TEST_VAR", "env_test_value");

        let error = ValidationError::MissingField {
            field: "test_field".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = strategy.attempt_recovery(&error);
        assert!(result.is_ok());

        let recovery = result.unwrap();
        assert_eq!(recovery.strategy, "environment_variable");
        assert_eq!(recovery.recovered_value, "env_test_value");
        assert!(recovery.description.contains("TEST_VAR"));

        env::remove_var("TEST_VAR");
    }

    #[test]
    fn test_recovery_strategy_environment_variable_not_found() {
        let strategy = RecoveryStrategy::EnvironmentVariable("NONEXISTENT_VAR".to_string());

        let error = ValidationError::MissingField {
            field: "test_field".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = strategy.attempt_recovery(&error);
        assert!(result.is_err());
    }

    #[test]
    fn test_recovery_strategy_computed_value() {
        let strategy = RecoveryStrategy::ComputedValue("computed_value".to_string());

        let error = ValidationError::MissingField {
            field: "test_field".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = strategy.attempt_recovery(&error);
        assert!(result.is_ok());

        let recovery = result.unwrap();
        assert_eq!(recovery.strategy, "computed_value");
        assert_eq!(recovery.recovered_value, "computed_value");
        assert!(recovery.description.contains("computed value"));
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
        assert_eq!(recovery.strategy, "skip_field");
        assert_eq!(recovery.recovered_value, "[SKIPPED]");
        assert!(recovery.description.contains("not required"));
    }

    #[test]
    fn test_recovery_strategy_custom() {
        let strategy = RecoveryStrategy::Custom("Custom recovery implementation".to_string());

        let error = ValidationError::MissingField {
            field: "custom_field".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = strategy.attempt_recovery(&error);
        assert!(result.is_ok());

        let recovery = result.unwrap();
        assert_eq!(recovery.strategy, "custom");
        assert_eq!(recovery.recovered_value, "[CUSTOM]");
        assert!(recovery.description.contains("Custom recovery implementation"));
    }

    #[test]
    fn test_recovery_strategy_names() {
        let cases = vec![
            (RecoveryStrategy::DefaultValue("test".to_string()), "default_value"),
            (RecoveryStrategy::EnvironmentVariable("TEST".to_string()), "environment_variable"),
            (RecoveryStrategy::ComputedValue("computed".to_string()), "computed_value"),
            (RecoveryStrategy::SkipField, "skip_field"),
            (RecoveryStrategy::Custom("custom".to_string()), "custom"),
        ];

        for (strategy, expected_name) in cases {
            assert_eq!(strategy.name(), expected_name);
        }
    }
}

#[cfg(test)]
mod error_handling_result_tests {
    use super::*;

    #[test]
    fn test_error_handling_result_success() {
        let result = ErrorHandlingResult::Success;

        assert!(result.is_success());
        assert!(!result.has_errors());
        assert!(result.error_message().is_none());
        assert!(result.into_service_result().is_ok());
    }

    #[test]
    fn test_error_handling_result_recovered() {
        let recovery = RecoveryResult {
            strategy: "default_value",
            original_value: Some("invalid".to_string()),
            recovered_value: "default".to_string(),
            description: "Applied default value".to_string(),
        };

        let result = ErrorHandlingResult::Recovered(recovery.clone());

        assert!(result.is_success());
        assert!(!result.has_errors());
        assert!(result.error_message().is_some());
        assert!(result.error_message().unwrap().contains("Recovered"));
        assert!(result.into_service_result().is_ok());
    }

    #[test]
    fn test_error_handling_result_warning() {
        let error = ValidationError::ConstraintViolation {
            field: "timeout".to_string(),
            constraint: "recommended".to_string(),
            details: "Above recommended value".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = ErrorHandlingResult::Warning(error.clone());

        assert!(result.is_success());
        assert!(!result.has_errors());
        assert!(result.error_message().is_some());
        assert!(result.into_service_result().is_ok());
    }

    #[test]
    fn test_error_handling_result_failed() {
        let error = ValidationError::MissingField {
            field: "required".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = ErrorHandlingResult::Failed(error.clone());

        assert!(!result.is_success());
        assert!(result.has_errors());
        assert!(result.error_message().is_some());
        assert!(result.into_service_result().is_err());
    }

    #[test]
    fn test_error_handling_result_critical() {
        let error = ValidationError::ParseError {
            file_source: "config.yaml".to_string(),
            error: "Invalid syntax".to_string(),
            line: None,
            column: None,
        };

        let result = ErrorHandlingResult::Critical(error.clone());

        assert!(!result.is_success());
        assert!(result.has_errors());
        assert!(result.error_message().is_some());
        assert!(result.into_service_result().is_err());

        let service_result = result.into_service_result();
        if let Err(e) = service_result {
            assert!(e.to_string().contains("CRITICAL"));
        }
    }

    #[test]
    fn test_error_handling_result_info() {
        let error = ValidationError::EnvironmentError {
            variable: "OPTIONAL_VAR".to_string(),
            error: "not set".to_string(),
            env_source: None,
        };

        let result = ErrorHandlingResult::Info(error.clone());

        assert!(result.is_success());
        assert!(!result.has_errors());
        assert!(result.error_message().is_some());
        assert!(result.into_service_result().is_ok());
    }
}

#[cfg(test)]
mod error_record_tests {
    use super::*;

    #[test]
    fn test_error_record_from_validation_error() {
        let error = ValidationError::MissingField {
            field: "test_field".to_string(),
            context: ValidationContext::new("test.yaml")
                .with_section("database")
                .with_field("connection"),
        };

        let record = ErrorRecord::from_validation_error(&error);

        assert_eq!(record.error_type, "validation_error");
        assert_eq!(record.field, Some("test_field".to_string()));
        assert_eq!(record.severity, ValidationSeverity::Error);
        assert!(record.message.contains("test_field"));
        assert!(record.context.is_some());
        assert!(record.recovery_attempted);
        assert!(!record.recovery_successful);
        assert!(!record.id.is_empty());
    }

    #[test]
    fn test_error_record_from_service_error() {
        let service_error = crucible_services::errors::ServiceError::ConfigurationError(
            "Test configuration error".to_string()
        );

        let record = ErrorRecord::from_service_error(&service_error);

        assert_eq!(record.error_type, "service_error");
        assert!(record.field.is_none());
        assert_eq!(record.severity, ValidationSeverity::Error);
        assert!(record.message.contains("Test configuration error"));
        assert!(record.context.is_none());
        assert!(!record.recovery_attempted);
        assert!(!record.recovery_successful);
    }

    #[test]
    fn test_error_record_unique_ids() {
        let error = ValidationError::MissingField {
            field: "test".to_string(),
            context: ValidationContext::new("test"),
        };

        let record1 = ErrorRecord::from_validation_error(&error);
        let record2 = ErrorRecord::from_validation_error(&error);

        assert_ne!(record1.id, record2.id);
        assert!(record1.id.len() > 0);
        assert!(record2.id.len() > 0);
    }

    #[test]
    fn test_error_record_timestamp() {
        let error = ValidationError::MissingField {
            field: "test".to_string(),
            context: ValidationContext::new("test"),
        };

        let before = Utc::now();
        let record = ErrorRecord::from_validation_error(&error);
        let after = Utc::now();

        assert!(record.timestamp >= before);
        assert!(record.timestamp <= after);
    }
}

#[cfg(test)]
mod error_statistics_tests {
    use super::*;

    #[test]
    fn test_error_statistics_default() {
        let stats = ErrorStatistics::default();

        assert_eq!(stats.total_errors, 0);
        assert_eq!(stats.critical_errors, 0);
        assert_eq!(stats.error_errors, 0);
        assert_eq!(stats.warnings, 0);
        assert_eq!(stats.info_messages, 0);
        assert!(stats.field_error_counts.is_empty());
    }

    #[test]
    fn test_error_statistics_calculation() {
        let mut stats = ErrorStatistics::default();

        // Add various types of errors
        stats.total_errors = 10;
        stats.critical_errors = 2;
        stats.error_errors = 3;
        stats.warnings = 4;
        stats.info_messages = 1;

        stats.field_error_counts.insert("host".to_string(), 3);
        stats.field_error_counts.insert("port".to_string(), 2);
        stats.field_error_counts.insert("timeout".to_string(), 1);

        assert_eq!(stats.total_errors, 10);
        assert_eq!(stats.critical_errors, 2);
        assert_eq!(stats.error_errors, 3);
        assert_eq!(stats.warnings, 4);
        assert_eq!(stats.info_messages, 1);
        assert_eq!(stats.field_error_counts.get("host"), Some(&3));
        assert_eq!(stats.field_error_counts.get("port"), Some(&2));
        assert_eq!(stats.field_error_counts.get("timeout"), Some(&1));
    }

    #[test]
    fn test_error_statistics_error_rate() {
        let mut stats = ErrorStatistics::default();

        // Test with zero hours
        assert_eq!(stats.error_rate_per_hour(0.0), 0.0);

        // Test with normal values
        stats.total_errors = 60;
        assert_eq!(stats.error_rate_per_hour(1.0), 60.0);
        assert_eq!(stats.error_rate_per_hour(2.0), 30.0);
        assert_eq!(stats.error_rate_per_hour(0.5), 120.0);
    }

    #[test]
    fn test_error_statistics_most_problematic_fields() {
        let mut stats = ErrorStatistics::default();

        stats.field_error_counts.insert("host".to_string(), 10);
        stats.field_error_counts.insert("port".to_string(), 5);
        stats.field_error_counts.insert("timeout".to_string(), 8);
        stats.field_error_counts.insert("retries".to_string(), 2);

        let problematic = stats.most_problematic_fields(3);
        assert_eq!(problematic.len(), 3);
        assert_eq!(problematic[0], ("host".to_string(), 10));
        assert_eq!(problematic[1], ("timeout".to_string(), 8));
        assert_eq!(problematic[2], ("port".to_string(), 5));

        // Test with limit larger than available fields
        let all_problematic = stats.most_problematic_fields(10);
        assert_eq!(all_problematic.len(), 4);
    }

    #[test]
    fn test_error_statistics_high_error_rate() {
        let mut stats = ErrorStatistics::default();

        // Test low error rate
        stats.total_errors = 50;
        assert!(!stats.is_high_error_rate(24.0)); // 50 errors in 24 hours = ~2/hour
        assert!(stats.is_high_error_rate(1.0));  // 50 errors in 1 hour = 50/hour

        // Test high error rate
        stats.total_errors = 500;
        assert!(stats.is_high_error_rate(24.0)); // 500 errors in 24 hours = ~21/hour
        assert!(stats.is_high_error_rate(48.0)); // 500 errors in 48 hours = ~10/hour (borderline)
    }

    #[test]
    fn test_error_statistics_summary() {
        let mut stats = ErrorStatistics::default();
        stats.total_errors = 15;
        stats.critical_errors = 2;
        stats.error_errors = 5;
        stats.warnings = 6;
        stats.info_messages = 2;

        let summary = stats.summary();
        assert!(summary.contains("15 total"));
        assert!(summary.contains("2 critical"));
        assert!(summary.contains("5 errors"));
        assert!(summary.contains("6 warnings"));
        assert!(summary.contains("2 info"));
    }
}

#[cfg(test)]
mod error_reporting_config_tests {
    use super::*;

    #[test]
    fn test_error_reporting_config_default() {
        let config = ErrorReportingConfig::default();

        assert!(!config.enable_reporting);
        assert!(config.reporting_endpoint.is_none());
        assert!(!config.report_warnings);
        assert!(!config.report_info);
        assert_eq!(config.max_reports_per_hour, Some(100));
        assert!(config.include_context);
    }

    #[test]
    fn test_error_reporting_config_custom() {
        let config = ErrorReportingConfig {
            enable_reporting: true,
            reporting_endpoint: Some("http://localhost:8080/errors".to_string()),
            report_warnings: true,
            report_info: false,
            max_reports_per_hour: Some(50),
            include_context: false,
        };

        assert!(config.enable_reporting);
        assert_eq!(config.reporting_endpoint, Some("http://localhost:8080/errors".to_string()));
        assert!(config.report_warnings);
        assert!(!config.report_info);
        assert_eq!(config.max_reports_per_hour, Some(50));
        assert!(!config.include_context);
    }
}

#[cfg(test)]
mod error_handling_utility_tests {
    use super::*;

    #[test]
    fn test_create_error_context() {
        let context = utils::create_error_context("test_source");

        assert_eq!(context.source, "test_source");
        assert!(context.metadata.contains_key("hostname"));
        assert!(context.metadata.contains_key("process_id"));
        assert!(context.metadata.contains_key("timestamp"));
    }

    #[test]
    fn test_format_error_for_logging() {
        let error = ValidationError::MissingField {
            field: "test_field".to_string(),
            context: ValidationContext::new("test.yaml")
                .with_section("database")
                .with_field("host"),
        };

        // Test without context
        let formatted = utils::format_error_for_logging(&error, false);
        assert!(formatted.contains("[ERROR]"));
        assert!(formatted.contains("Missing required field: test_field"));

        // Test with context
        let formatted_with_context = utils::format_error_for_logging(&error, true);
        assert!(formatted_with_context.contains("[ERROR]"));
        assert!(formatted_with_context.contains("test.yaml"));
        assert!(formatted_with_context.contains("database.host"));
    }

    #[test]
    fn test_create_user_friendly_message() {
        let cases = vec![
            (
                ValidationError::MissingField {
                    field: "database_url".to_string(),
                    context: ValidationContext::new("test"),
                },
                "database_url"
            ),
            (
                ValidationError::InvalidValue {
                    field: "port".to_string(),
                    value: "invalid".to_string(),
                    reason: "not a number".to_string(),
                    context: ValidationContext::new("test"),
                    suggested_fix: Some("Use a valid port number (1-65535)".to_string()),
                },
                "port"
            ),
            (
                ValidationError::ConstraintViolation {
                    field: "timeout".to_string(),
                    constraint: "maximum".to_string(),
                    details: "Value exceeds maximum allowed".to_string(),
                    context: ValidationContext::new("test"),
                },
                "timeout"
            ),
        ];

        for (error, expected_field) in cases {
            let message = utils::create_user_friendly_message(&error);
            assert!(message.contains(expected_field));
            assert!(!message.is_empty());
        }
    }

    #[test]
    fn test_should_escalate_error() {
        let escalate_cases = vec![
            ValidationError::ParseError {
                file_source: "config.yaml".to_string(),
                error: "syntax error".to_string(),
                line: None,
                column: None,
            },
            ValidationError::DependencyViolation {
                message: "circular dependency".to_string(),
                field: "field1".to_string(),
                depends_on: "field2".to_string(),
                context: ValidationContext::new("test"),
            },
        ];

        for error in escalate_cases {
            assert!(utils::should_escalate_error(&error));
        }

        let non_escalate_cases = vec![
            ValidationError::MissingField {
                field: "optional".to_string(),
                context: ValidationContext::new("test"),
            },
            ValidationError::ConstraintViolation {
                field: "performance".to_string(),
                constraint: "recommended".to_string(),
                details: "suboptimal value".to_string(),
                context: ValidationContext::new("test"),
            },
        ];

        for error in non_escalate_cases {
            assert!(!utils::should_escalate_error(&error));
        }
    }

    #[test]
    fn test_generate_error_report_empty() {
        let errors = vec![];
        let report = utils::generate_error_report(&errors);

        assert_eq!(report, "No errors to report.");
    }

    #[test]
    fn test_generate_error_report_with_errors() {
        let errors = vec![
            ErrorRecord::from_validation_error(&ValidationError::MissingField {
                field: "host".to_string(),
                context: ValidationContext::new("config.yaml"),
            }),
            ErrorRecord::from_validation_error(&ValidationError::InvalidValue {
                field: "port".to_string(),
                value: "invalid".to_string(),
                reason: "not a number".to_string(),
                context: ValidationContext::new("config.yaml"),
                suggested_fix: None,
            }),
            ErrorRecord::from_validation_error(&ValidationError::ConstraintViolation {
                field: "timeout".to_string(),
                constraint: "recommended".to_string(),
                details: "too high".to_string(),
                context: ValidationContext::new("config.yaml"),
            }),
        ];

        let report = utils::generate_error_report(&errors);

        assert!(report.contains("# Configuration Error Report"));
        assert!(report.contains("## Summary"));
        assert!(report.contains("## Most Problematic Fields"));
        assert!(report.contains("## Recent Errors"));
        assert!(report.contains("host"));
        assert!(report.contains("port"));
        assert!(report.contains("timeout"));
    }

    #[test]
    fn test_calculate_error_stats() {
        // This is tested indirectly through generate_error_report
        // but we can verify the logic by creating a scenario with known field counts

        let errors = vec![
            ErrorRecord::from_validation_error(&ValidationError::MissingField {
                field: "host".to_string(),
                context: ValidationContext::new("test"),
            }),
            ErrorRecord::from_validation_error(&ValidationError::InvalidValue {
                field: "host".to_string(),
                value: "invalid".to_string(),
                reason: "invalid format".to_string(),
                context: ValidationContext::new("test"),
                suggested_fix: None,
            }),
            ErrorRecord::from_validation_error(&ValidationError::ConstraintViolation {
                field: "port".to_string(),
                constraint: "range".to_string(),
                details: "out of range".to_string(),
                context: ValidationContext::new("test"),
            }),
        ];

        let report = utils::generate_error_report(&errors);
        assert!(report.contains("host")); // Should appear twice
        assert!(report.contains("port")); // Should appear once
    }
}

#[cfg(test)]
mod integration_error_handling_tests {
    use super::*;

    #[test]
    fn test_full_error_handling_workflow() {
        let mut handler = ConfigErrorHandler::new();

        // Simulate a series of validation errors
        let errors = vec![
            ValidationError::MissingField {
                field: "database_url".to_string(),
                context: ValidationContext::new("config.yaml")
                    .with_section("database"),
            },
            ValidationError::InvalidValue {
                field: "log_level".to_string(),
                value: "verbose".to_string(),
                reason: "invalid log level".to_string(),
                context: ValidationContext::new("config.yaml")
                    .with_section("logging"),
                suggested_fix: Some("Use: trace, debug, info, warn, error".to_string()),
            },
            ValidationError::ConstraintViolation {
                field: "max_connections".to_string(),
                constraint: "recommended range".to_string(),
                details: "Value is very high, may cause performance issues".to_string(),
                context: ValidationContext::new("config.yaml")
                    .with_section("database"),
            },
        ];

        // Handle each error
        let mut results = vec![];
        for error in errors {
            results.push(handler.handle_validation_error(error));
        }

        // Verify handling results
        assert_eq!(results.len(), 3);
        assert!(results.iter().any(|r| matches!(r, ErrorHandlingResult::Failed(_))));
        assert!(results.iter().any(|r| matches!(r, ErrorHandlingResult::Recovered(_))));
        assert!(results.iter().any(|r| matches!(r, ErrorHandlingResult::Warning(_))));

        // Verify error history
        assert_eq!(handler.error_history().len(), 3);

        // Get statistics
        let stats = handler.get_error_statistics();
        assert_eq!(stats.total_errors, 3);
        assert!(stats.field_error_counts.contains_key("database_url"));
        assert!(stats.field_error_counts.contains_key("log_level"));
        assert!(stats.field_error_counts.contains_key("max_connections"));
    }

    #[test]
    fn test_error_handling_with_reporting() {
        let reporting_config = ErrorReportingConfig {
            enable_reporting: true,
            report_warnings: true,
            include_context: true,
            ..Default::default()
        };

        let mut handler = ConfigErrorHandler::with_config(reporting_config);

        let error = ValidationError::ParseError {
            file_source: "production.yaml".to_string(),
            error: "invalid YAML syntax".to_string(),
            line: Some(42),
            column: Some(15),
        };

        let result = handler.handle_validation_error(error);

        match result {
            ErrorHandlingResult::Critical(_) => {
                // Expected for parse errors
            }
            _ => panic!("Expected Critical result for parse error"),
        }

        assert_eq!(handler.error_history().len(), 1);
        assert!(handler.error_history()[0].context.is_some());
    }

    #[test]
    fn test_recovery_strategy_integration() {
        let mut handler = ConfigErrorHandler::new();

        // Add custom recovery strategy
        handler.add_recovery_strategy(
            "custom_field",
            RecoveryStrategy::ComputedValue("computed_default".to_string())
        );

        let error = ValidationError::MissingField {
            field: "custom_field".to_string(),
            context: ValidationContext::new("test"),
        };

        let result = handler.handle_validation_error(error);

        match result {
            ErrorHandlingResult::Recovered(recovery) => {
                assert_eq!(recovery.strategy, "computed_value");
                assert_eq!(recovery.recovered_value, "computed_default");
            }
            _ => panic!("Expected Recovered result"),
        }
    }

    #[test]
    fn test_error_handling_performance() {
        let mut handler = ConfigErrorHandler::new();

        let start = std::time::Instant::now();

        // Handle many errors quickly
        for i in 0..1000 {
            let error = ValidationError::MissingField {
                field: format!("field_{}", i),
                context: ValidationContext::new("test"),
            };

            handler.handle_validation_error(error);
        }

        let duration = start.elapsed();

        // Should handle 1000 errors quickly (adjust threshold as needed)
        assert!(duration.as_millis() < 1000);

        // Verify all errors were recorded
        assert_eq!(handler.error_history().len(), 1000);

        // Verify statistics are calculated efficiently
        let stats = handler.get_error_statistics();
        assert_eq!(stats.total_errors, 1000);
    }

    #[test]
    fn test_error_scenarios() {
        let mut handler = ConfigErrorHandler::new();

        let scenarios = vec![
            // Critical scenario
            ValidationError::ParseError {
                file_source: "critical_config.yaml".to_string(),
                error: "cannot parse file".to_string(),
                line: Some(1),
                column: Some(1),
            },
            // Recoverable scenario
            ValidationError::InvalidValue {
                field: "logging.level".to_string(),
                value: "invalid".to_string(),
                reason: "not a valid log level".to_string(),
                context: ValidationContext::new("test"),
                suggested_fix: Some("Use one of: trace, debug, info, warn, error".to_string()),
            },
            // Warning scenario
            ValidationError::ConstraintViolation {
                field: "performance.memory_limit".to_string(),
                constraint: "recommended".to_string(),
                details: "Very high memory limit may impact system performance".to_string(),
                context: ValidationContext::new("test"),
            },
            // Environment error scenario
            ValidationError::EnvironmentError {
                variable: "OPTIONAL_CONFIG".to_string(),
                error: "not set".to_string(),
                env_source: Some("system".to_string()),
            },
        ];

        for (i, error) in scenarios.into_iter().enumerate() {
            let result = handler.handle_validation_error(error);

            match i {
                0 => assert!(matches!(result, ErrorHandlingResult::Critical(_))),
                1 => assert!(matches!(result, ErrorHandlingResult::Recovered(_))),
                2 => assert!(matches!(result, ErrorHandlingResult::Warning(_))),
                3 => assert!(matches!(result, ErrorHandlingResult::Warning(_))),
                _ => {}
            }
        }

        // Verify mixed error types in history
        let stats = handler.get_error_statistics();
        assert_eq!(stats.total_errors, 4);
        assert!(stats.critical_errors > 0);
        assert!(stats.warnings > 0);
    }

    #[test]
    fn test_error_handling_edge_cases() {
        let mut handler = ConfigErrorHandler::new();

        // Test with empty field name
        let error1 = ValidationError::MissingField {
            field: "".to_string(),
            context: ValidationContext::new("test"),
        };

        let result1 = handler.handle_validation_error(error1);
        assert!(matches!(result1, ErrorHandlingResult::Failed(_)));

        // Test with very long field name
        let long_field_name = "a".repeat(1000);
        let error2 = ValidationError::MissingField {
            field: long_field_name.clone(),
            context: ValidationContext::new("test"),
        };

        let result2 = handler.handle_validation_error(error2);
        assert!(matches!(result2, ErrorHandlingResult::Failed(_)));

        // Test with Unicode characters
        let error3 = ValidationError::InvalidValue {
            field: "测试字段".to_string(),
            value: "无效值".to_string(),
            reason: "无效的配置值".to_string(),
            context: ValidationContext::new("测试配置"),
            suggested_fix: Some("使用有效的值".to_string()),
        };

        let result3 = handler.handle_validation_error(error3);
        assert!(matches!(result3, ErrorHandlingResult::Failed(_)));

        // Verify all errors were recorded correctly
        assert_eq!(handler.error_history().len(), 3);
    }
}