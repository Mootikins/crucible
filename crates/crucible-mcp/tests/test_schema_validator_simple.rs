/// Simple tests for SchemaValidator
/// Tests the basic functionality that is actually implemented

use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use crucible_mcp::rune_tools::{
    SchemaValidator, ValidationResult, ValidationError, ValidationWarning,
    ValidationConfig, ValidationContext, AsyncFunctionInfo, ParameterInfo,
    ValidationRule, RuneType
};

#[test]
fn test_schema_validator_creation() -> Result<()> {
    let config = ValidationConfig::default();
    let _validator = SchemaValidator::new(config);

    // Should create successfully
    Ok(())
}

#[test]
fn test_basic_schema_generation() -> Result<()> {
    let config = ValidationConfig::default();
    let validator = SchemaValidator::new(config);

    // Test string schema
    let string_schema = validator.generate_schema(&RuneType::String, "test")?;
    assert_eq!(string_schema["type"], "string");

    // Test number schema
    let number_schema = validator.generate_schema(&RuneType::Number, "test")?;
    assert_eq!(number_schema["type"], "number");

    // Test boolean schema
    let boolean_schema = validator.generate_schema(&RuneType::Boolean, "test")?;
    assert_eq!(boolean_schema["type"], "boolean");

    Ok(())
}

#[test]
fn test_parameter_schema_generation() -> Result<()> {
    let config = ValidationConfig::default();
    let validator = SchemaValidator::new(config);

    let param = ParameterInfo {
        name: "test_param".to_string(),
        type_name: "string".to_string(),
        type_constraints: vec!["string".to_string()],
        is_optional: false,
        default_value: None,
        description: Some("Test parameter".to_string()),
        validation_rules: vec![],
    };

    let schema = validator.generate_parameter_schema(&param)?;
    assert_eq!(schema["type"], "string");
    assert_eq!(schema["description"], "Test parameter");

    Ok(())
}

#[test]
fn test_function_schema_generation() -> Result<()> {
    let config = ValidationConfig::default();
    let validator = SchemaValidator::new(config);

    let function_info = AsyncFunctionInfo {
        name: "create_file".to_string(),
        is_async: true,
        is_public: true,
        parameters: vec![
            ParameterInfo {
                name: "path".to_string(),
                type_name: "string".to_string(),
                type_constraints: vec!["string".to_string()],
                is_optional: false,
                default_value: None,
                description: Some("File path to create".to_string()),
                validation_rules: vec![],
            },
            ParameterInfo {
                name: "content".to_string(),
                type_name: "string".to_string(),
                type_constraints: vec!["string".to_string()],
                is_optional: true,
                default_value: Some(json!("")),
                description: Some("Content to write".to_string()),
                validation_rules: vec![],
            }
        ],
        return_type: Some("Result".to_string()),
        module_path: vec!["file".to_string()],
        full_path: vec!["file".to_string(), "create_file".to_string()],
        description: Some("Create a file".to_string()),
        doc_comments: vec![],
        source_location: crucible_mcp::rune_tools::SourceLocation {
            line: None,
            column: None,
            file_path: None,
        },
        metadata: Default::default(),
    };

    let schema = validator.generate_function_schema(&function_info)?;

    assert_eq!(schema["type"], "object");
    assert!(schema["properties"].is_object());
    assert!(schema["required"].is_array());
    assert_eq!(schema["description"], "Create a file");

    Ok(())
}

#[test]
fn test_basic_validation() -> Result<()> {
    let config = ValidationConfig::default();
    let validator = SchemaValidator::new(config);

    let schema = json!({"type": "string"});
    let context = ValidationContext::new("test_param", "test_function", "test_module");

    // Test valid string
    let valid_value = json!("hello");
    let result = validator.validate(&valid_value, &schema, &context);
    assert!(result.is_valid);

    // Test invalid value
    let invalid_value = json!(123);
    let result = validator.validate(&invalid_value, &schema, &context);
    assert!(!result.is_valid);

    Ok(())
}

#[test]
fn test_function_parameter_validation() -> Result<()> {
    let config = ValidationConfig::default();
    let validator = SchemaValidator::new(config);

    let function_info = AsyncFunctionInfo {
        name: "test_func".to_string(),
        is_async: true,
        is_public: true,
        parameters: vec![
            ParameterInfo {
                name: "name".to_string(),
                type_name: "string".to_string(),
                type_constraints: vec!["string".to_string()],
                is_optional: false,
                default_value: None,
                description: Some("Name parameter".to_string()),
                validation_rules: vec![],
            }
        ],
        return_type: Some("String".to_string()),
        module_path: vec!["test".to_string()],
        full_path: vec!["test".to_string(), "test_func".to_string()],
        description: Some("Test function".to_string()),
        doc_comments: vec![],
        source_location: crucible_mcp::rune_tools::SourceLocation {
            line: None,
            column: None,
            file_path: None,
        },
        metadata: Default::default(),
    };

    // Test valid parameters
    let valid_params = json!({"name": "John Doe"});
    let result = validator.validate_function_parameters(&function_info, &valid_params);
    assert!(result.is_valid);

    // Test missing required parameter
    let invalid_params = json!({});
    let result = validator.validate_function_parameters(&function_info, &invalid_params);
    assert!(!result.is_valid);

    Ok(())
}

#[test]
fn test_validation_context() -> Result<()> {
    let context = ValidationContext::new("param1", "func1", "mod1");

    assert_eq!(context.parameter_name, "param1");
    assert_eq!(context.function_name, "func1");
    assert_eq!(context.module_name, "mod1");
    assert_eq!(context.field_path, "param1");

    let nested_context = context.with_field("nested");
    assert_eq!(nested_context.field_path, "param1.nested");

    Ok(())
}

#[test]
fn test_validation_config() -> Result<()> {
    let default_config = ValidationConfig::default();
    assert!(!default_config.strict_mode);
    assert!(default_config.allow_unknown_types);
    assert!(default_config.generate_descriptions);
    assert!(!default_config.include_examples);

    let custom_config = ValidationConfig {
        strict_mode: true,
        allow_unknown_types: false,
        generate_descriptions: false,
        include_examples: true,
        max_string_length: Some(1000),
        max_array_length: Some(500),
    };

    assert!(custom_config.strict_mode);
    assert!(!custom_config.allow_unknown_types);
    assert!(!custom_config.generate_descriptions);
    assert!(custom_config.include_examples);
    assert_eq!(custom_config.max_string_length, Some(1000));
    assert_eq!(custom_config.max_array_length, Some(500));

    Ok(())
}

#[test]
fn test_array_schema_generation() -> Result<()> {
    let config = ValidationConfig::default();
    let validator = SchemaValidator::new(config);

    let array_schema = validator.generate_schema(&RuneType::Array(Box::new(RuneType::String)), "test")?;
    assert_eq!(array_schema["type"], "array");
    assert!(array_schema["items"].is_object());

    Ok(())
}

#[test]
fn test_object_schema_generation() -> Result<()> {
    let config = ValidationConfig::default();
    let validator = SchemaValidator::new(config);

    let mut properties = HashMap::new();
    properties.insert("name".to_string(), RuneType::String);
    properties.insert("age".to_string(), RuneType::Integer);

    let object_schema = validator.generate_schema(&RuneType::Object(properties), "test")?;
    assert_eq!(object_schema["type"], "object");
    assert!(object_schema["properties"].is_object());
    assert!(object_schema["required"].is_array());

    Ok(())
}

#[test]
fn test_complex_schema_generation() -> Result<()> {
    let config = ValidationConfig::default();
    let validator = SchemaValidator::new(config);

    // Test Option type
    let option_schema = validator.generate_schema(&RuneType::Option(Box::new(RuneType::String)), "test")?;
    assert!(option_schema["anyOf"].is_array());

    // Test Tuple type
    let tuple_schema = validator.generate_schema(&RuneType::Tuple(vec![RuneType::String, RuneType::Integer]), "test")?;
    assert_eq!(tuple_schema["type"], "array");
    assert_eq!(tuple_schema["minItems"], 2);
    assert_eq!(tuple_schema["maxItems"], 2);

    Ok(())
}

#[test]
fn test_validation_result() -> Result<()> {
    // Test valid result
    let valid_result = ValidationResult::valid();
    assert!(valid_result.is_valid);
    assert!(valid_result.errors.is_empty());
    assert!(valid_result.warnings.is_empty());

    // Test invalid result
    let context = ValidationContext::new("test", "test", "test");
    let error = ValidationError::new("TEST_ERROR", "Test error", &context);
    let invalid_result = ValidationResult::invalid(vec![error]);
    assert!(!invalid_result.is_valid);
    assert_eq!(invalid_result.errors.len(), 1);

    Ok(())
}

#[test]
fn test_validation_error() -> Result<()> {
    let context = ValidationContext::new("test_param", "test_func", "test_mod");
    let error = ValidationError::new("TYPE_ERROR", "Wrong type", &context);

    assert_eq!(error.code, "TYPE_ERROR");
    assert_eq!(error.message, "Wrong type");
    assert_eq!(error.field_path, "test_param");

    let with_expected = error.with_expected("string");
    assert_eq!(with_expected.expected, Some("string".to_string()));

    let with_actual = with_expected.with_actual("123");
    assert_eq!(with_actual.actual, Some("123".to_string()));

    Ok(())
}

#[test]
fn test_validation_warning() -> Result<()> {
    let warning = ValidationWarning::new("DEPRECATED", "This is deprecated", "field1");

    assert_eq!(warning.code, "DEPRECATED");
    assert_eq!(warning.message, "This is deprecated");
    assert_eq!(warning.field_path, "field1");

    Ok(())
}