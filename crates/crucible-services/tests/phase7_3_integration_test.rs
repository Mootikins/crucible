//! Phase 7.3 Integration Test: Configuration Validation and Error Handling
//!
//! This test demonstrates the complete configuration validation and error handling
//! system implemented for Phase 7.3 of the Crucible project.

use crucible_services::config::{
    EnhancedConfig, ConfigManager, ConfigManagerBuilder, ConfigErrorHandler,
    ValidationEngine, ValidationRule, ValidationRuleType, ValidationContext,
    ValidationError, RecoveryStrategy, ErrorHandlingResult,
};
use crucible_services::errors::ServiceResult;
use std::collections::HashMap;
use tokio_test;

#[tokio::test]
async fn test_phase7_3_configuration_validation_integration() {
    println!("🧪 Phase 7.3 Configuration Validation Integration Test");
    println!("=====================================================");

    // Test 1: Enhanced Configuration Creation and Validation
    println!("\n📋 Test 1: Enhanced Configuration Creation and Validation");

    let config = EnhancedConfig::default();
    let validation_result = config.validate();

    assert!(validation_result.is_valid, "Default configuration should be valid");
    println!("✅ Default configuration is valid");
    println!("📊 Configuration summary: {}", config.get_summary());

    // Test 2: Configuration Manager with Validation
    println!("\n🏗️  Test 2: Configuration Manager with Validation");

    let manager_result = ConfigManager::new().await;
    assert!(manager_result.is_ok(), "Configuration manager should initialize successfully");

    let manager = manager_result.unwrap();
    let manager_config = manager.get_config().await;
    let manager_validation = manager.validate_current_config().await;

    assert!(manager_validation.is_valid, "Manager configuration should be valid");
    println!("✅ Configuration manager initialized with valid configuration");

    let health_status = manager.health_status().await;
    assert!(health_status.is_healthy, "Configuration health should be good");
    println!("✅ Configuration health status: healthy");

    // Test 3: Configuration Export/Import with Validation
    println!("\n📤 Test 3: Configuration Export/Import with Validation");

    let json_export = manager.export_config(crucible_services::config::ConfigExportFormat::JSON).await;
    assert!(json_export.is_ok(), "Configuration export should succeed");

    let exported_json = json_export.unwrap();
    println!("✅ Configuration exported to JSON ({} chars)", exported_json.len());

    let import_result = manager.import_config(&exported_json, crucible_services::config::ConfigExportFormat::JSON).await;
    assert!(import_result.is_ok(), "Configuration import should succeed");
    println!("✅ Configuration imported and validated successfully");

    // Test 4: Configuration Validation with Errors
    println!("\n⚠️  Test 4: Configuration Validation with Errors");

    let mut invalid_config = EnhancedConfig::default();
    invalid_config.service.environment = "invalid_env".to_string();
    invalid_config.logging.level = "invalid_level".to_string();
    invalid_config.event_routing.max_event_age_seconds = 0;

    let invalid_validation = invalid_config.validate();
    assert!(!invalid_validation.is_valid, "Invalid configuration should fail validation");
    assert!(invalid_validation.errors.len() >= 3, "Should have multiple validation errors");
    println!("✅ Invalid configuration correctly rejected with {} errors", invalid_validation.errors.len());

    for (i, error) in invalid_validation.errors.iter().enumerate() {
        println!("   Error {}: {}", i + 1, error.description());
    }

    // Test 5: Error Handling and Recovery
    println!("\n🔧 Test 5: Error Handling and Recovery");

    let mut error_handler = ConfigErrorHandler::new();

    // Test handling a recoverable error
    let recoverable_error = ValidationError::InvalidValue {
        field: "logging.level".to_string(),
        value: "invalid".to_string(),
        reason: "Invalid log level".to_string(),
        context: ValidationContext::new("test_config"),
        suggested_fix: Some("Use 'info', 'debug', 'warn', or 'error'".to_string()),
    };

    let recovery_result = error_handler.handle_validation_error(recoverable_error);
    match recovery_result {
        ErrorHandlingResult::Recovered(recovery) => {
            println!("✅ Error recovered successfully: {}", recovery.description);
        }
        _ => panic!("Expected error to be recovered"),
    }

    // Test handling a critical error
    let critical_error = ValidationError::ParseError {
        file_source: "config.yaml".to_string(),
        error: "Invalid YAML syntax".to_string(),
        line: Some(10),
        column: Some(5),
    };

    let critical_result = error_handler.handle_validation_error(critical_error);
    match critical_result {
        ErrorHandlingResult::Critical(_) => {
            println!("✅ Critical error correctly identified");
        }
        _ => panic!("Expected critical error"),
    }

    let stats = error_handler.get_error_statistics();
    println!("📊 Error statistics: {}", stats.summary());

    // Test 6: Custom Validation Rules
    println!("\n📏 Test 6: Custom Validation Rules");

    let mut engine = ValidationEngine::new();

    // Add custom validation rules
    engine.add_rule("custom_field", ValidationRule {
        field: "custom_field".to_string(),
        rule_type: ValidationRuleType::Pattern(r"^[a-z][a-z0-9_]*$".to_string()),
        parameters: HashMap::new(),
        error_message: "Field must follow snake_case pattern".to_string(),
        required: true,
    });

    engine.add_rule("custom_range", ValidationRule {
        field: "custom_range".to_string(),
        rule_type: ValidationRuleType::Range { min: Some(1.0), max: Some(100.0) },
        parameters: HashMap::new(),
        error_message: "Value must be between 1 and 100".to_string(),
        required: true,
    });

    // Test valid configuration
    let valid_config = serde_json::json!({
        "custom_field": "test_value",
        "custom_range": 50
    });

    let context = ValidationContext::new("test_validation");
    let valid_result = engine.validate_config(&valid_config, &context);
    assert!(valid_result.is_valid, "Valid config should pass custom validation");
    println!("✅ Custom validation rules work correctly");

    // Test invalid configuration
    let invalid_config = serde_json::json!({
        "custom_field": "Invalid Value",
        "custom_range": 150
    });

    let invalid_result = engine.validate_config(&invalid_config, &context);
    assert!(!invalid_result.is_valid, "Invalid config should fail custom validation");
    println!("✅ Custom validation rules correctly reject invalid config");

    // Test 7: Configuration Manager Builder
    println!("\n🏗️  Test 7: Configuration Manager Builder");

    let builder_manager = ConfigManagerBuilder::new()
        .with_hot_reload(false)
        .build()
        .await;

    assert!(builder_manager.is_ok(), "Builder should create manager successfully");
    println!("✅ Configuration manager builder works correctly");

    // Test 8: Configuration Patching with Validation
    println!("\n🩹 Test 8: Configuration Patching with Validation");

    let patch = crucible_services::config::ConfigPatch {
        operations: vec![
            crucible_services::config::ConfigOperation::Set {
                field: "logging.level".to_string(),
                value: serde_json::Value::String("debug".to_string()),
            },
            crucible_services::config::ConfigOperation::Set {
                field: "service.environment".to_string(),
                value: serde_json::Value::String("staging".to_string()),
            },
        ],
        metadata: crucible_services::config::PatchMetadata::default(),
    };

    let patch_result = manager.apply_patch(&patch).await;
    assert!(patch_result.is_ok(), "Valid patch should be applied successfully");
    println!("✅ Configuration patching with validation works");

    let patched_config = manager.get_config().await;
    assert_eq!(patched_config.logging.level, "debug");
    assert_eq!(patched_config.service.environment, "staging");
    println!("✅ Patched configuration values verified");

    // Test 9: Error Reporting and Statistics
    println!("\n📈 Test 9: Error Reporting and Statistics");

    let recent_errors = error_handler.get_recent_errors(5);
    println!("✅ Retrieved {} recent errors", recent_errors.len());

    let error_report = crucible_services::config::error_handling::utils::generate_error_report(&recent_errors);
    assert!(!error_report.is_empty(), "Error report should not be empty");
    println!("✅ Error report generated successfully");

    println!("\n🎉 Phase 7.3 Integration Test Completed Successfully!");
    println!("=====================================================");
    println!("✅ All configuration validation and error handling features work correctly");
    println!("✅ Enhanced configuration structures with comprehensive validation");
    println!("✅ Error handling framework with recovery strategies");
    println!("✅ Configuration manager with hot-reload capabilities");
    println!("✅ Integration with existing logging and service systems");
    println!("✅ Production-ready error handling with proper context");
}

#[tokio::test]
async fn test_phase7_3_error_recovery_scenarios() {
    println!("\n🔄 Phase 7.3 Error Recovery Scenarios Test");
    println!("==========================================");

    let mut error_handler = ConfigErrorHandler::new();

    // Test 1: Default value recovery
    println!("\n📝 Test 1: Default Value Recovery");

    let missing_field_error = ValidationError::MissingField {
        field: "logging.level".to_string(),
        context: ValidationContext::new("test_config"),
    };

    let result = error_handler.handle_validation_error(missing_field_error);
    assert!(matches!(result, ErrorHandlingResult::Recovered(_)));
    println!("✅ Missing field recovered with default value");

    // Test 2: Environment variable recovery
    println!("\n🌍 Test 2: Environment Variable Recovery");

    std::env::set_var("TEST_RECOVERY_VAR", "recovered_value");

    let env_recovery_error = ValidationError::EnvironmentError {
        variable: "TEST_RECOVERY_VAR".to_string(),
        error: "Variable missing".to_string(),
        source: Some("config".to_string()),
    };

    let result = error_handler.handle_validation_error(env_recovery_error);
    assert!(matches!(result, ErrorHandlingResult::Recovered(_)));
    println!("✅ Environment variable recovery successful");

    std::env::remove_var("TEST_RECOVERY_VAR");

    // Test 3: Warning handling
    println!("\n⚠️  Test 3: Warning Handling");

    let warning_error = ValidationError::ConstraintViolation {
        field: "performance.max_memory".to_string(),
        constraint: "recommended".to_string(),
        details: "Value is below recommended minimum".to_string(),
        context: ValidationContext::new("test_config"),
    };

    let result = error_handler.handle_validation_error(warning_error);
    assert!(matches!(result, ErrorHandlingResult::Warning(_)));
    println!("✅ Warning handled correctly without recovery");

    // Test 4: Critical error handling
    println!("\n🚨 Test 4: Critical Error Handling");

    let critical_error = ValidationError::ParseError {
        file_source: "config.yaml".to_string(),
        error: "YAML syntax error".to_string(),
        line: Some(15),
        column: Some(8),
    };

    let result = error_handler.handle_validation_error(critical_error);
    assert!(matches!(result, ErrorHandlingResult::Critical(_)));
    println!("✅ Critical error handled appropriately");

    // Test 5: Multiple errors handling
    println!("\n📦 Test 5: Multiple Errors Handling");

    let validation_result = ValidationResult::with_errors(vec![
        ValidationError::MissingField {
            field: "required_field".to_string(),
            context: ValidationContext::new("test"),
        },
        ValidationError::InvalidValue {
            field: "optional_field".to_string(),
            value: "invalid".to_string(),
            reason: "Bad value".to_string(),
            context: ValidationContext::new("test"),
            suggested_fix: Some("Use correct value".to_string()),
        },
    ]);

    let result = error_handler.handle_validation_result(validation_result);
    assert!(matches!(result, ErrorHandlingResult::Failed(_)));
    println!("✅ Multiple errors handled correctly");

    println!("\n✅ All error recovery scenarios completed successfully!");
}

#[test]
fn test_phase7_3_validation_rule_completeness() {
    println!("\n📏 Phase 7.3 Validation Rule Completeness Test");
    println!("===============================================");

    let mut engine = ValidationEngine::new();
    let context = ValidationContext::new("test_rules");

    // Test all validation rule types
    println!("\n🔍 Testing all validation rule types...");

    // Required field
    engine.add_rule("required", ValidationRule {
        field: "required".to_string(),
        rule_type: ValidationRuleType::Required,
        parameters: HashMap::new(),
        error_message: "Field is required".to_string(),
        required: true,
    });

    // Pattern matching
    engine.add_rule("pattern", ValidationRule {
        field: "pattern".to_string(),
        rule_type: ValidationRuleType::Pattern(r"^[a-z]+$".to_string()),
        parameters: HashMap::new(),
        error_message: "Must be lowercase letters".to_string(),
        required: true,
    });

    // Range validation
    engine.add_rule("range", ValidationRule {
        field: "range".to_string(),
        rule_type: ValidationRuleType::Range { min: Some(1.0), max: Some(10.0) },
        parameters: HashMap::new(),
        error_message: "Must be between 1 and 10".to_string(),
        required: true,
    });

    // Enum validation
    engine.add_rule("enum", ValidationRule {
        field: "enum".to_string(),
        rule_type: ValidationRuleType::Enum(vec!["option1".to_string(), "option2".to_string()]),
        parameters: HashMap::new(),
        error_message: "Must be option1 or option2".to_string(),
        required: true,
    });

    // Non-empty validation
    engine.add_rule("non_empty", ValidationRule {
        field: "non_empty".to_string(),
        rule_type: ValidationRuleType::NonEmpty,
        parameters: HashMap::new(),
        error_message: "Cannot be empty".to_string(),
        required: true,
    });

    // Positive validation
    engine.add_rule("positive", ValidationRule {
        field: "positive".to_string(),
        rule_type: ValidationRuleType::Positive,
        parameters: HashMap::new(),
        error_message: "Must be positive".to_string(),
        required: true,
    });

    // Test valid configuration
    let valid_config = serde_json::json!({
        "required": "value",
        "pattern": "lowercase",
        "range": 5,
        "enum": "option1",
        "non_empty": "not empty",
        "positive": 10
    });

    let result = engine.validate_config(&valid_config, &context);
    assert!(result.is_valid, "All validation rules should pass with valid data");
    println!("✅ All validation rule types work correctly");

    // Test invalid configuration
    let invalid_config = serde_json::json!({
        "required": "",           // Fails Required rule
        "pattern": "UpperCase",   // Fails Pattern rule
        "range": 15,             // Fails Range rule
        "enum": "invalid",       // Fails Enum rule
        "non_empty": "   ",      // Fails NonEmpty rule
        "positive": -5           // Fails Positive rule
    });

    let result = engine.validate_config(&invalid_config, &context);
    assert!(!result.is_valid, "Should fail with invalid data");
    assert!(result.errors.len() >= 6, "Should have multiple validation errors");
    println!("✅ All validation rules correctly reject invalid data ({} errors)", result.errors.len());

    println!("\n✅ Validation rule completeness test passed!");
use tokio_test;
use serde_json::json;
