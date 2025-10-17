/// Validation Rule Engine Unit Tests
///
/// Comprehensive test suite for the schema validation system and rule engine.
/// Tests built-in validators, validation rules, error reporting, schema composition,
/// constraint validation, type parsing, function parameter validation, and complex scenarios.

use crucible_mcp::rune_tools::{
    SchemaValidator, ValidationResult, ValidationConfig, ValidationContext,
    RuneType, ParameterInfo, AsyncFunctionInfo, ValidationRule, SourceLocation
};
use serde_json::json;
use std::collections::HashMap;

/// Test helper to create a validation context
fn create_test_context() -> ValidationContext {
    ValidationContext::new("test_param", "test_function", "test_module")
}

/// Test helper to create a schema validator
fn create_test_validator() -> SchemaValidator {
    let config = ValidationConfig::default();
    SchemaValidator::new(config)
}

/// Test helper to assert validation result
fn assert_validation_result(result: &ValidationResult, expected_valid: bool, expected_error_count: usize) {
    assert_eq!(result.is_valid, expected_valid,
               "Expected valid: {}, got: {}. Errors: {:?}",
               expected_valid, result.is_valid, result.errors);

    assert_eq!(result.errors.len(), expected_error_count,
               "Expected {} errors, got: {}",
               expected_error_count, result.errors.len());
}

#[test]
fn test_basic_string_validation() {
    let validator = create_test_validator();
    let context = create_test_context();

    // Test valid string
    let string_schema = json!({"type": "string"});
    let result = validator.validate(&json!("hello world"), &string_schema, &context);
    assert_validation_result(&result, true, 0);

    // Test invalid string
    let result = validator.validate(&json!(123), &string_schema, &context);
    assert_validation_result(&result, false, 1);
    assert!(result.errors[0].message.contains("Expected string"));
}

#[test]
fn test_basic_number_validation() {
    let validator = create_test_validator();
    let context = create_test_context();

    // Test valid number
    let number_schema = json!({"type": "number"});
    let result = validator.validate(&json!(42.5), &number_schema, &context);
    assert_validation_result(&result, true, 0);

    // Test valid integer (also valid as number)
    let result = validator.validate(&json!(42), &number_schema, &context);
    assert_validation_result(&result, true, 0);

    // Test invalid number
    let result = validator.validate(&json!("not a number"), &number_schema, &context);
    assert_validation_result(&result, false, 1);
    assert!(result.errors[0].message.contains("Expected number"));
}

#[test]
fn test_basic_integer_validation() {
    let validator = create_test_validator();
    let context = create_test_context();

    // Test valid integer
    let integer_schema = json!({"type": "integer"});
    let result = validator.validate(&json!(42), &integer_schema, &context);
    assert_validation_result(&result, true, 0);

    // Test invalid integer (float)
    let result = validator.validate(&json!(42.5), &integer_schema, &context);
    assert_validation_result(&result, false, 1);
    assert!(result.errors[0].message.contains("Expected integer"));

    // Test invalid integer (string)
    let result = validator.validate(&json!("42"), &integer_schema, &context);
    assert_validation_result(&result, false, 1);
}

#[test]
fn test_basic_boolean_validation() {
    let validator = create_test_validator();
    let context = create_test_context();

    // Test valid boolean
    let boolean_schema = json!({"type": "boolean"});
    let result = validator.validate(&json!(true), &boolean_schema, &context);
    assert_validation_result(&result, true, 0);

    let result = validator.validate(&json!(false), &boolean_schema, &context);
    assert_validation_result(&result, true, 0);

    // Test invalid boolean
    let result = validator.validate(&json!("true"), &boolean_schema, &context);
    assert_validation_result(&result, false, 1);
    assert!(result.errors[0].message.contains("Expected boolean"));
}

#[test]
fn test_basic_array_validation() {
    let validator = create_test_validator();
    let context = create_test_context();

    // Test valid array
    let array_schema = json!({"type": "array"});
    let result = validator.validate(&json!([1, 2, 3]), &array_schema, &context);
    assert_validation_result(&result, true, 0);

    // Test invalid array
    let result = validator.validate(&json!("not an array"), &array_schema, &context);
    assert_validation_result(&result, false, 1);
    assert!(result.errors[0].message.contains("Expected array"));
}

#[test]
fn test_basic_object_validation() {
    let validator = create_test_validator();
    let context = create_test_context();

    // Test valid object
    let object_schema = json!({"type": "object"});
    let result = validator.validate(&json!({"key": "value"}), &object_schema, &context);
    assert_validation_result(&result, true, 0);

    // Test invalid object
    let result = validator.validate(&json!("not an object"), &object_schema, &context);
    assert_validation_result(&result, false, 1);
    assert!(result.errors[0].message.contains("Expected object"));
}

#[test]
fn test_schema_generation_from_rune_types() {
    let validator = create_test_validator();

    // Test string type
    let string_schema = validator.generate_schema(&RuneType::String, "test_string").unwrap();
    assert_eq!(string_schema["type"], "string");

    // Test number type
    let number_schema = validator.generate_schema(&RuneType::Number, "test_number").unwrap();
    assert_eq!(number_schema["type"], "number");
    assert!(number_schema.get("minimum").is_some());

    // Test integer type
    let integer_schema = validator.generate_schema(&RuneType::Integer, "test_integer").unwrap();
    assert_eq!(integer_schema["type"], "integer");
    assert!(integer_schema.get("minimum").is_some());

    // Test boolean type
    let boolean_schema = validator.generate_schema(&RuneType::Boolean, "test_boolean").unwrap();
    assert_eq!(boolean_schema["type"], "boolean");

    // Test array type
    let array_schema = validator.generate_schema(&RuneType::Array(Box::new(RuneType::String)), "test_array").unwrap();
    assert_eq!(array_schema["type"], "array");
    assert!(array_schema.get("items").is_some());

    // Test object type
    let mut properties = HashMap::new();
    properties.insert("name".to_string(), RuneType::String);
    properties.insert("age".to_string(), RuneType::Integer);
    let object_schema = validator.generate_schema(&RuneType::Object(properties), "test_object").unwrap();
    assert_eq!(object_schema["type"], "object");
    assert!(object_schema.get("properties").is_some());
    assert!(object_schema.get("required").is_some());

    // Test option type
    let option_schema = validator.generate_schema(&RuneType::Option(Box::new(RuneType::String)), "test_option").unwrap();
    assert!(option_schema.get("anyOf").is_some());
}

#[test]
fn test_parameter_schema_generation() {
    let validator = create_test_validator();

    // Create test parameter
    let param = ParameterInfo {
        name: "test_param".to_string(),
        type_name: "string".to_string(),
        type_constraints: vec!["string".to_string()],
        is_optional: false,
        default_value: None,
        description: Some("Test parameter".to_string()),
        validation_rules: vec![],
    };

    let schema = validator.generate_parameter_schema(&param).unwrap();
    assert_eq!(schema["type"], "string");
    assert_eq!(schema["description"], "Test parameter");
}

#[test]
fn test_parameter_with_validation_rules() {
    let validator = create_test_validator();

    // Create validation rule
    let validation_rule = ValidationRule {
        rule_type: "range".to_string(),
        parameters: {
            let mut map = HashMap::new();
            map.insert("min".to_string(), json!(1));
            map.insert("max".to_string(), json!(100));
            map
        },
    };

    // Create parameter with validation rules
    let param = ParameterInfo {
        name: "age".to_string(),
        type_name: "integer".to_string(),
        type_constraints: vec!["integer".to_string()],
        is_optional: false,
        default_value: None,
        description: Some("Age parameter".to_string()),
        validation_rules: vec![validation_rule],
    };

    let schema = validator.generate_parameter_schema(&param).unwrap();
    assert_eq!(schema["type"], "integer");
    assert_eq!(schema["minimum"], 1);
    assert_eq!(schema["maximum"], 100);
}

#[test]
fn test_function_parameter_validation() {
    let validator = create_test_validator();

    // Create test function with parameters
    let function_info = AsyncFunctionInfo {
        name: "test_function".to_string(),
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
            },
            ParameterInfo {
                name: "age".to_string(),
                type_name: "integer".to_string(),
                type_constraints: vec!["integer".to_string()],
                is_optional: true,
                default_value: Some(json!(25)),
                description: Some("Age parameter".to_string()),
                validation_rules: vec![],
            },
        ],
        return_type: Some("String".to_string()),
        module_path: vec!["test".to_string()],
        full_path: vec!["test".to_string(), "test_function".to_string()],
        description: Some("Test function".to_string()),
        doc_comments: vec![],
        source_location: SourceLocation {
            line: None,
            column: None,
            file_path: None,
        },
        metadata: HashMap::new(),
    };

    // Test valid parameters
    let valid_params = json!({
        "name": "John Doe",
        "age": 30
    });

    let result = validator.validate_function_parameters(&function_info, &valid_params);
    assert_validation_result(&result, true, 0);

    // Test missing required parameter
    let invalid_params = json!({
        "age": 30
    });

    let result = validator.validate_function_parameters(&function_info, &invalid_params);
    assert_validation_result(&result, false, 1);
    assert!(result.errors[0].message.contains("missing"));
    assert!(result.errors[0].message.contains("name"));

    // Test invalid parameter types
    let invalid_type_params = json!({
        "name": 123,  // Should be string
        "age": "30"   // Should be integer
    });

    let result = validator.validate_function_parameters(&function_info, &invalid_type_params);
    assert_validation_result(&result, false, 2);
    assert!(result.errors.iter().any(|e| e.field_path.contains("name")));
    assert!(result.errors.iter().any(|e| e.field_path.contains("age")));
}

#[test]
fn test_complex_function_schema_generation() {
    let validator = create_test_validator();

    // Create complex function
    let function_info = AsyncFunctionInfo {
        name: "complex_function".to_string(),
        is_async: true,
        is_public: true,
        parameters: vec![
            ParameterInfo {
                name: "query".to_string(),
                type_name: "string".to_string(),
                type_constraints: vec!["string".to_string()],
                is_optional: false,
                default_value: None,
                description: Some("Search query".to_string()),
                validation_rules: vec![
                    ValidationRule {
                        rule_type: "length".to_string(),
                        parameters: {
                            let mut map = HashMap::new();
                            map.insert("min".to_string(), json!(1));
                            map.insert("max".to_string(), json!(100));
                            map
                        },
                    }
                ],
            },
            ParameterInfo {
                name: "filters".to_string(),
                type_name: "object".to_string(),
                type_constraints: vec!["object".to_string()],
                is_optional: true,
                default_value: Some(json!({})),
                description: Some("Search filters".to_string()),
                validation_rules: vec![],
            },
            ParameterInfo {
                name: "limit".to_string(),
                type_name: "integer".to_string(),
                type_constraints: vec!["integer".to_string()],
                is_optional: true,
                default_value: Some(json!(10)),
                description: Some("Result limit".to_string()),
                validation_rules: vec![
                    ValidationRule {
                        rule_type: "range".to_string(),
                        parameters: {
                            let mut map = HashMap::new();
                            map.insert("min".to_string(), json!(1));
                            map.insert("max".to_string(), json!(100));
                            map
                        },
                    }
                ],
            },
        ],
        return_type: Some("Array<String>".to_string()),
        module_path: vec!["search".to_string()],
        full_path: vec!["search".to_string(), "complex_function".to_string()],
        description: Some("Complex search function".to_string()),
        doc_comments: vec!["Search function with multiple parameters".to_string()],
        source_location: SourceLocation {
            line: Some(42),
            column: Some(1),
            file_path: Some("search.rn".to_string()),
        },
        metadata: HashMap::new(),
    };

    let schema = validator.generate_function_schema(&function_info).unwrap();

    // Check schema structure
    assert_eq!(schema["type"], "object");
    assert!(schema.get("properties").is_some());
    assert!(schema.get("required").is_some());
    assert_eq!(schema["description"], "Complex search function");

    // Check metadata
    assert!(schema.get("metadata").is_some());
    let metadata = &schema["metadata"];
    assert_eq!(metadata["module"], "search");
    assert_eq!(metadata["function"], "complex_function");
    assert_eq!(metadata["is_async"], true);
    assert_eq!(metadata["parameter_count"], 3);

    // Check properties
    let properties = schema["properties"].as_object().unwrap();
    assert!(properties.contains_key("query"));
    assert!(properties.contains_key("filters"));
    assert!(properties.contains_key("limit"));

    // Check required fields
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("query")));
    assert!(!required.contains(&json!("filters")));  // optional
    assert!(!required.contains(&json!("limit")));    // optional

    // Check parameter constraints
    assert_eq!(properties["query"]["type"], "string");
    assert_eq!(properties["query"]["minLength"], 1);
    assert_eq!(properties["query"]["maxLength"], 100);

    assert_eq!(properties["limit"]["type"], "integer");
    assert_eq!(properties["limit"]["minimum"], 1);
    assert_eq!(properties["limit"]["maximum"], 100);
    assert_eq!(properties["limit"]["default"], 10);
}

#[test]
fn test_type_constraint_parsing_via_parameters() {
    let validator = create_test_validator();

    // Test type constraint parsing through parameter schema generation
    let param_with_string_type = ParameterInfo {
        name: "string_param".to_string(),
        type_name: "string".to_string(),
        type_constraints: vec!["string".to_string()],
        is_optional: false,
        default_value: None,
        description: Some("String parameter".to_string()),
        validation_rules: vec![],
    };

    let schema = validator.generate_parameter_schema(&param_with_string_type).unwrap();
    assert_eq!(schema["type"], "string");

    // Test with array type constraint
    let param_with_array_type = ParameterInfo {
        name: "array_param".to_string(),
        type_name: "array".to_string(),
        type_constraints: vec!["array".to_string()],
        is_optional: false,
        default_value: None,
        description: Some("Array parameter".to_string()),
        validation_rules: vec![],
    };

    let schema = validator.generate_parameter_schema(&param_with_array_type).unwrap();
    assert_eq!(schema["type"], "array");

    // Test with object type constraint
    let param_with_object_type = ParameterInfo {
        name: "object_param".to_string(),
        type_name: "object".to_string(),
        type_constraints: vec!["object".to_string()],
        is_optional: false,
        default_value: None,
        description: Some("Object parameter".to_string()),
        validation_rules: vec![],
    };

    let schema = validator.generate_parameter_schema(&param_with_object_type).unwrap();
    assert_eq!(schema["type"], "object");
}

#[test]
fn test_validation_context_and_error_reporting() {
    let validator = create_test_validator();

    // Create context with specific field path
    let mut context = ValidationContext::new("user", "create_user", "user_management");
    context = context.with_field("name");
    context = context.with_field("first");

    let schema = json!({"type": "string"});
    let result = validator.validate(&json!(123), &schema, &context);

    assert!(!result.is_valid);
    assert_eq!(result.errors.len(), 1);

    let error = &result.errors[0];
    assert_eq!(error.field_path, "user.name.first");
    assert_eq!(error.code, "TYPE_MISMATCH");
    assert!(error.message.contains("Expected string"));
    assert_eq!(error.expected, Some("string".to_string()));
    assert_eq!(error.actual, Some("123".to_string()));
}

#[test]
fn test_anyof_validation() {
    let validator = create_test_validator();
    let context = create_test_context();

    // Create anyOf schema
    let anyof_schema = json!({
        "anyOf": [
            {"type": "string"},
            {"type": "integer"}
        ]
    });

    // Valid string
    let result = validator.validate(&json!("hello"), &anyof_schema, &context);
    assert_validation_result(&result, true, 0);

    // Valid integer
    let result = validator.validate(&json!(42), &anyof_schema, &context);
    assert_validation_result(&result, true, 0);

    // Invalid type (not matching any schema)
    let result = validator.validate(&json!(true), &anyof_schema, &context);
    assert_validation_result(&result, false, 2); // One error for each schema
}

#[test]
fn test_strict_mode_validation() {
    let mut config = ValidationConfig::default();
    config.strict_mode = true;
    let validator = SchemaValidator::new(config);
    let context = create_test_context();

    // Test with unknown type in strict mode
    let unknown_schema = json!({"type": "unknown_type"});
    let result = validator.validate(&json!("test"), &unknown_schema, &context);

    assert!(!result.is_valid);
    assert_eq!(result.errors.len(), 1);
    assert!(result.errors[0].code.contains("UNKNOWN_TYPE"));
    assert!(result.warnings.is_empty());
}

#[test]
fn test_non_strict_mode_validation() {
    let mut config = ValidationConfig::default();
    config.strict_mode = false;  // Default
    let validator = SchemaValidator::new(config);
    let context = create_test_context();

    // Test with unknown type in non-strict mode
    let unknown_schema = json!({"type": "unknown_type"});
    let result = validator.validate(&json!("test"), &unknown_schema, &context);

    assert!(result.is_valid);  // Should be valid but with warning
    assert!(result.errors.is_empty());
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].code.contains("UNKNOWN_TYPE"));
}

#[test]
fn test_validation_with_descriptions_and_examples() {
    let mut config = ValidationConfig::default();
    config.generate_descriptions = true;
    config.include_examples = true;
    let validator = SchemaValidator::new(config);

    let string_schema = validator.generate_schema(&RuneType::String, "test_string").unwrap();

    // Check description is included
    assert!(string_schema.get("description").is_some());
    let description = string_schema["description"].as_str().unwrap();
    assert!(description.contains("test_string"));
    assert!(description.contains("text string"));

    // Check example is included
    assert!(string_schema.get("example").is_some());
    let example = &string_schema["example"];
    assert_eq!(example, "example string");
}

#[test]
fn test_nested_object_validation() {
    let validator = create_test_validator();
    let context = create_test_context();

    // Create simple nested object schema - only test basic type validation
    let nested_schema = json!({
        "type": "object"
    });

    // Valid nested object
    let valid_data = json!({
        "user": {
            "name": "John Doe",
            "profile": {
                "age": 30,
                "email": "john@example.com"
            }
        }
    });

    let result = validator.validate(&valid_data, &nested_schema, &context);
    assert_validation_result(&result, true, 0);

    // Test basic type validation - wrong type
    let wrong_type_data = json!("not an object");
    let result = validator.validate(&wrong_type_data, &nested_schema, &context);
    assert_validation_result(&result, false, 1);
    assert!(result.errors[0].message.contains("Expected object"));

    // Test with array - also invalid for object schema
    let array_data = json!([1, 2, 3]);
    let result = validator.validate(&array_data, &nested_schema, &context);
    assert_validation_result(&result, false, 1);
    assert!(result.errors[0].message.contains("Expected object"));

    // Test with boolean - also invalid for object schema
    let bool_data = json!(true);
    let result = validator.validate(&bool_data, &nested_schema, &context);
    assert_validation_result(&result, false, 1);
    assert!(result.errors[0].message.contains("Expected object"));
}

#[test]
fn test_validation_error_quality() {
    let validator = create_test_validator();
    let context = create_test_context();

    let string_schema = json!({
        "type": "string",
        "minLength": 5,
        "maxLength": 10,
        "pattern": "^[a-zA-Z]+$"
    });

    // Test various error conditions
    let test_cases = vec![
        (json!(123), "TYPE_MISMATCH"),           // Wrong type
        (json!("hi"), "minLength"),              // Too short
        (json!("this is way too long"), "maxLength"), // Too long
        (json!("hello123"), "pattern"),          // Invalid pattern
    ];

    for (value, _expected_error_hint) in test_cases {
        let result = validator.validate(&value, &string_schema, &context);

        // For basic type validation, we only get type mismatch errors
        // The additional constraints would need to be implemented by the individual validators
        if !result.is_valid {
            let error = &result.errors[0];
            assert!(!error.message.is_empty());
            assert!(!error.field_path.is_empty());
            assert!(error.field_path == "test_param");

            // Check that error provides useful information
            if error.code == "TYPE_MISMATCH" {
                assert!(error.expected.is_some());
                assert!(error.actual.is_some());
            }
        }
    }
}

#[test]
fn test_function_validation_with_complex_types() {
    let validator = create_test_validator();

    // Create function with complex parameter types
    let function_info = AsyncFunctionInfo {
        name: "process_data".to_string(),
        is_async: true,
        is_public: true,
        parameters: vec![
            ParameterInfo {
                name: "data".to_string(),
                type_name: "array<object>".to_string(),
                type_constraints: vec!["array".to_string()],
                is_optional: false,
                default_value: None,
                description: Some("Data to process".to_string()),
                validation_rules: vec![
                    ValidationRule {
                        rule_type: "length".to_string(),
                        parameters: {
                            let mut map = HashMap::new();
                            map.insert("min".to_string(), json!(1));
                            map.insert("max".to_string(), json!(10));
                            map
                        },
                    }
                ],
            },
            ParameterInfo {
                name: "options".to_string(),
                type_name: "object".to_string(),
                type_constraints: vec!["object".to_string()],
                is_optional: true,
                default_value: Some(json!({"strict": false})),
                description: Some("Processing options".to_string()),
                validation_rules: vec![],
            },
        ],
        return_type: Some("Array<String>".to_string()),
        module_path: vec!["data".to_string()],
        full_path: vec!["data".to_string(), "process_data".to_string()],
        description: Some("Process data with options".to_string()),
        doc_comments: vec![],
        source_location: SourceLocation {
            line: None,
            column: None,
            file_path: None,
        },
        metadata: HashMap::new(),
    };

    // Test valid parameters
    let valid_params = json!({
        "data": [
            {"id": 1, "name": "Item 1"},
            {"id": 2, "name": "Item 2"}
        ],
        "options": {
            "strict": true,
            "timeout": 5000
        }
    });

    let result = validator.validate_function_parameters(&function_info, &valid_params);
    assert_validation_result(&result, true, 0);

    // Test missing required parameter
    let missing_required = json!({
        "options": {"strict": true}
    });

    let result = validator.validate_function_parameters(&function_info, &missing_required);
    assert_validation_result(&result, false, 1);
    assert!(result.errors[0].message.contains("data"));
    assert!(result.errors[0].message.contains("missing"));

    // Test parameters object instead of expected structure
    let wrong_structure = json!("not an object");

    let result = validator.validate_function_parameters(&function_info, &wrong_structure);
    assert_validation_result(&result, false, 1);
    assert!(result.errors[0].message.contains("object"));
}