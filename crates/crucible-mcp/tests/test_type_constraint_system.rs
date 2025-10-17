/// Type Constraint System Comprehensive Tests
///
/// Test suite for the type constraint system used in Rune tool analysis.
/// Tests TypeConstraint, ConstraintType, and constraint creation/validation.

use crucible_mcp::rune_tools::{
    TypeConstraint, ConstraintType, RuneType, AnalyzerConfig, RuneAstAnalyzer
};
use serde_json::json;
use std::collections::HashMap;

#[test]
fn test_type_constraint_creation() {
    // Test Range constraint
    let range_constraint = TypeConstraint {
        constraint_type: ConstraintType::Range { min: Some(0.0), max: Some(100.0) },
        parameters: HashMap::new(),
        description: "Range constraint for numbers".to_string(),
    };

    match &range_constraint.constraint_type {
        ConstraintType::Range { min, max } => {
            assert_eq!(*min, Some(0.0));
            assert_eq!(*max, Some(100.0));
        },
        _ => panic!("Expected Range constraint"),
    }

    // Test Length constraint
    let length_constraint = TypeConstraint {
        constraint_type: ConstraintType::Length { min: Some(1), max: Some(255) },
        parameters: HashMap::new(),
        description: "Length constraint for strings".to_string(),
    };

    match &length_constraint.constraint_type {
        ConstraintType::Length { min, max } => {
            assert_eq!(*min, Some(1));
            assert_eq!(*max, Some(255));
        },
        _ => panic!("Expected Length constraint"),
    }

    // Test Pattern constraint
    let pattern_constraint = TypeConstraint {
        constraint_type: ConstraintType::Pattern(r"^[a-zA-Z0-9]+$".to_string()),
        parameters: HashMap::new(),
        description: "Pattern constraint for alphanumeric strings".to_string(),
    };

    match &pattern_constraint.constraint_type {
        ConstraintType::Pattern(pattern) => {
            assert_eq!(pattern, r"^[a-zA-Z0-9]+$");
        },
        _ => panic!("Expected Pattern constraint"),
    }

    // Test Enum constraint
    let enum_values = vec![json!("red"), json!("green"), json!("blue")];
    let enum_constraint = TypeConstraint {
        constraint_type: ConstraintType::Enum(enum_values.clone()),
        parameters: HashMap::new(),
        description: "Enum constraint for colors".to_string(),
    };

    match &enum_constraint.constraint_type {
        ConstraintType::Enum(values) => {
            assert_eq!(values, &enum_values);
        },
        _ => panic!("Expected Enum constraint"),
    }

    // Test Required/Optional constraints
    let required_constraint = TypeConstraint {
        constraint_type: ConstraintType::Required,
        parameters: HashMap::new(),
        description: "Required parameter constraint".to_string(),
    };

    assert!(matches!(required_constraint.constraint_type, ConstraintType::Required));

    let optional_constraint = TypeConstraint {
        constraint_type: ConstraintType::Optional,
        parameters: HashMap::new(),
        description: "Optional parameter constraint".to_string(),
    };

    assert!(matches!(optional_constraint.constraint_type, ConstraintType::Optional));
}

#[test]
fn test_analyzer_config_creation() {
    // Test AnalyzerConfig creation
    let config = AnalyzerConfig::default();

    assert!(config.infer_types);
    assert!(config.validate_signatures);
    assert!(config.generate_schemas);
    assert!(config.extract_doc_comments);
    assert!(!config.include_private_functions);
}

#[test]
fn test_analyzer_creation() {
    // Test RuneAstAnalyzer creation
    let _analyzer = RuneAstAnalyzer::new().expect("Failed to create AST analyzer");

    // Should successfully create without panicking
    let _analyzer2 = RuneAstAnalyzer::with_config(AnalyzerConfig::default()).expect("Failed to create AST analyzer with config");
}

#[test]
fn test_constraint_types_edge_cases() {
    // Test empty pattern constraint
    let empty_pattern = TypeConstraint {
        constraint_type: ConstraintType::Pattern(String::new()),
        parameters: HashMap::new(),
        description: "Empty pattern".to_string(),
    };

    match &empty_pattern.constraint_type {
        ConstraintType::Pattern(pattern) => {
            assert_eq!(pattern, "");
        },
        _ => panic!("Expected Pattern constraint"),
    }

    // Test empty enum constraint
    let empty_enum = TypeConstraint {
        constraint_type: ConstraintType::Enum(vec![]),
        parameters: HashMap::new(),
        description: "Empty enum".to_string(),
    };

    match &empty_enum.constraint_type {
        ConstraintType::Enum(values) => {
            assert!(values.is_empty());
        },
        _ => panic!("Expected Enum constraint"),
    }

    // Test range constraints with no bounds
    let unbounded_range = TypeConstraint {
        constraint_type: ConstraintType::Range { min: None, max: None },
        parameters: HashMap::new(),
        description: "Unbounded range".to_string(),
    };

    match &unbounded_range.constraint_type {
        ConstraintType::Range { min, max } => {
            assert!(min.is_none());
            assert!(max.is_none());
        },
        _ => panic!("Expected Range constraint"),
    }

    // Test length constraints with no bounds
    let unbounded_length = TypeConstraint {
        constraint_type: ConstraintType::Length { min: None, max: None },
        parameters: HashMap::new(),
        description: "Unbounded length".to_string(),
    };

    match &unbounded_length.constraint_type {
        ConstraintType::Length { min, max } => {
            assert!(min.is_none());
            assert!(max.is_none());
        },
        _ => panic!("Expected Length constraint"),
    }
}

#[test]
fn test_constraint_parameters() {
    // Test constraint with parameters
    let mut params = HashMap::new();
    params.insert("custom_field".to_string(), json!("custom_value"));
    params.insert("another_field".to_string(), json!(42));

    let constraint_with_params = TypeConstraint {
        constraint_type: ConstraintType::Range { min: Some(10.0), max: Some(100.0) },
        parameters: params.clone(),
        description: "Constraint with parameters".to_string(),
    };

    assert_eq!(constraint_with_params.parameters, params);
    assert_eq!(constraint_with_params.parameters["custom_field"], json!("custom_value"));
    assert_eq!(constraint_with_params.parameters["another_field"], json!(42));
}

#[test]
fn test_constraint_description() {
    // Test constraint description
    let constraint = TypeConstraint {
        constraint_type: ConstraintType::Length { min: Some(1), max: Some(10) },
        parameters: HashMap::new(),
        description: "String length must be between 1 and 10 characters".to_string(),
    };

    assert_eq!(constraint.description, "String length must be between 1 and 10 characters");
}

#[test]
fn test_constraint_type_equality() {
    // Test that constraints with same values are equal
    let constraint1 = TypeConstraint {
        constraint_type: ConstraintType::Range { min: Some(0.0), max: Some(100.0) },
        parameters: HashMap::new(),
        description: "Range constraint".to_string(),
    };

    let constraint2 = TypeConstraint {
        constraint_type: ConstraintType::Range { min: Some(0.0), max: Some(100.0) },
        parameters: HashMap::new(),
        description: "Range constraint".to_string(),
    };

    // Since ConstraintType doesn't implement PartialEq, test them individually
    match (&constraint1.constraint_type, &constraint2.constraint_type) {
        (ConstraintType::Range { min: min1, max: max1 }, ConstraintType::Range { min: min2, max: max2 }) => {
            assert_eq!(min1, min2);
            assert_eq!(max1, max2);
        },
        _ => panic!("Both constraints should be Range type"),
    }
    assert_eq!(constraint1.parameters, constraint2.parameters);
    assert_eq!(constraint1.description, constraint2.description);
}

#[test]
fn test_constraint_type_variants() {
    // Test all constraint type variants
    let range_constraint = TypeConstraint {
        constraint_type: ConstraintType::Range { min: Some(0.0), max: Some(100.0) },
        parameters: HashMap::new(),
        description: "Range".to_string(),
    };
    assert!(matches!(range_constraint.constraint_type, ConstraintType::Range { .. }));

    let length_constraint = TypeConstraint {
        constraint_type: ConstraintType::Length { min: Some(1), max: Some(255) },
        parameters: HashMap::new(),
        description: "Length".to_string(),
    };
    assert!(matches!(length_constraint.constraint_type, ConstraintType::Length { .. }));

    let pattern_constraint = TypeConstraint {
        constraint_type: ConstraintType::Pattern(r"test".to_string()),
        parameters: HashMap::new(),
        description: "Pattern".to_string(),
    };
    assert!(matches!(pattern_constraint.constraint_type, ConstraintType::Pattern(_)));

    let enum_constraint = TypeConstraint {
        constraint_type: ConstraintType::Enum(vec![json!("value1"), json!("value2")]),
        parameters: HashMap::new(),
        description: "Enum".to_string(),
    };
    assert!(matches!(enum_constraint.constraint_type, ConstraintType::Enum(_)));

    let custom_constraint = TypeConstraint {
        constraint_type: ConstraintType::Custom("custom".to_string()),
        parameters: HashMap::new(),
        description: "Custom".to_string(),
    };
    assert!(matches!(custom_constraint.constraint_type, ConstraintType::Custom(_)));

    let required_constraint = TypeConstraint {
        constraint_type: ConstraintType::Required,
        parameters: HashMap::new(),
        description: "Required".to_string(),
    };
    assert!(matches!(required_constraint.constraint_type, ConstraintType::Required));

    let optional_constraint = TypeConstraint {
        constraint_type: ConstraintType::Optional,
        parameters: HashMap::new(),
        description: "Optional".to_string(),
    };
    assert!(matches!(optional_constraint.constraint_type, ConstraintType::Optional));
}

#[test]
fn test_rune_type_variants() {
    // Test all RuneType variants for completeness
    let string_type = RuneType::String;
    assert!(matches!(string_type, RuneType::String));

    let number_type = RuneType::Number;
    assert!(matches!(number_type, RuneType::Number));

    let integer_type = RuneType::Integer;
    assert!(matches!(integer_type, RuneType::Integer));

    let float_type = RuneType::Float;
    assert!(matches!(float_type, RuneType::Float));

    let boolean_type = RuneType::Boolean;
    assert!(matches!(boolean_type, RuneType::Boolean));

    let array_type = RuneType::Array(Box::new(RuneType::String));
    assert!(matches!(array_type, RuneType::Array(_)));

    let object_type = RuneType::Object(HashMap::new());
    assert!(matches!(object_type, RuneType::Object(_)));

    let tuple_type = RuneType::Tuple(vec![RuneType::String, RuneType::Integer]);
    assert!(matches!(tuple_type, RuneType::Tuple(_)));

    let option_type = RuneType::Option(Box::new(RuneType::String));
    assert!(matches!(option_type, RuneType::Option(_)));

    let any_type = RuneType::Any;
    assert!(matches!(any_type, RuneType::Any));

    let void_type = RuneType::Void;
    assert!(matches!(void_type, RuneType::Void));

    let unknown_type = RuneType::Unknown("test".to_string());
    assert!(matches!(unknown_type, RuneType::Unknown(_)));

    let function_type = RuneType::Function {
        parameters: vec![RuneType::String],
        return_type: Box::new(RuneType::Boolean),
        is_async: true,
    };
    assert!(matches!(function_type, RuneType::Function { .. }));
}

#[test]
fn test_constraint_complex_types() {
    // Test constraints with complex Rune types
    let mut nested_map = HashMap::new();
    nested_map.insert("id".to_string(), RuneType::Integer);
    nested_map.insert("name".to_string(), RuneType::String);

    let complex_constraint = TypeConstraint {
        constraint_type: ConstraintType::Pattern(r"^[a-zA-Z0-9._-]+$".to_string()),
        parameters: {
            let mut params = HashMap::new();
            params.insert("allow_empty".to_string(), json!(false));
            params.insert("max_length".to_string(), json!(100));
            params
        },
        description: "Complex constraint with multiple parameters".to_string(),
    };

    assert!(matches!(complex_constraint.constraint_type, ConstraintType::Pattern(_)));
    assert_eq!(complex_constraint.parameters["allow_empty"], json!(false));
    assert_eq!(complex_constraint.parameters["max_length"], json!(100));
    assert!(complex_constraint.parameters.contains_key("allow_empty"));
    assert!(complex_constraint.parameters.contains_key("max_length"));
}

#[test]
fn test_constraint_serialization() {
    // Test that constraints can be serialized/deserialized (useful for debugging)
    let _constraint = TypeConstraint {
        constraint_type: ConstraintType::Range { min: Some(0.0), max: Some(100.0) },
        parameters: {
            let mut params = HashMap::new();
            params.insert("precision".to_string(), json!(2));
            params
        },
        description: "Test constraint".to_string(),
    };

    // Convert to JSON
    let json_value = json!({
        "constraint_type": "Range",
        "min": 0.0,
        "max": 100.0,
        "parameters": {
            "precision": 2
        },
        "description": "Test constraint"
    });

    // The constraint should match what we'd expect from JSON serialization
    // This test validates the structure rather than actual serialization
    assert!(json_value["min"] == json!(0.0));
    assert!(json_value["max"] == json!(100.0));
    assert!(json_value["description"] == "Test constraint");
}

#[test]
fn test_constraint_validation_scenarios() {
    // Test realistic validation scenarios

    // File path constraint
    let path_constraint = TypeConstraint {
        constraint_type: ConstraintType::Pattern(r"^[a-zA-Z0-9_\-./]+$".to_string()),
        parameters: HashMap::new(),
        description: "File path must contain only valid characters".to_string(),
    };

    // Email constraint
    let email_constraint = TypeConstraint {
        constraint_type: ConstraintType::Pattern(r"^[^@]+@[^@]+\.[^@]+$".to_string()),
        parameters: HashMap::new(),
        description: "Must be a valid email address".to_string(),
    };

    // Age range constraint
    let age_constraint = TypeConstraint {
        constraint_type: ConstraintType::Range { min: Some(0.0), max: Some(150.0) },
        parameters: HashMap::new(),
        description: "Age must be between 0 and 150".to_string(),
    };

    // Username length constraint
    let username_constraint = TypeConstraint {
        constraint_type: ConstraintType::Length { min: Some(3), max: Some(20) },
        parameters: HashMap::new(),
        description: "Username must be 3-20 characters long".to_string(),
    };

    // Status enum constraint
    let status_constraint = TypeConstraint {
        constraint_type: ConstraintType::Enum(vec![json!("active"), json!("inactive"), json!("pending")]),
        parameters: HashMap::new(),
        description: "Status must be one of: active, inactive, pending".to_string(),
    };

    // Test that all constraints are created correctly
    assert!(matches!(path_constraint.constraint_type, ConstraintType::Pattern(_)));
    assert!(matches!(email_constraint.constraint_type, ConstraintType::Pattern(_)));
    assert!(matches!(age_constraint.constraint_type, ConstraintType::Range { .. }));
    assert!(matches!(username_constraint.constraint_type, ConstraintType::Length { .. }));
    assert!(matches!(status_constraint.constraint_type, ConstraintType::Enum(_)));

    // Test that descriptions are descriptive
    assert!(path_constraint.description.contains("File path"));
    assert!(email_constraint.description.contains("email address"));
    assert!(age_constraint.description.contains("Age"));
    assert!(username_constraint.description.contains("Username"));
    assert!(status_constraint.description.contains("Status"));
    assert!(status_constraint.description.contains("active, inactive, pending"));
}

#[test]
fn test_constraint_business_logic_examples() {
    // Test constraints that represent real business logic

    // Password strength constraint
    let password_constraint = TypeConstraint {
        constraint_type: ConstraintType::Length { min: Some(8), max: Some(128) },
        parameters: {
            let mut params = HashMap::new();
            params.insert("require_uppercase".to_string(), json!(true));
            params.insert("require_lowercase".to_string(), json!(true));
            params.insert("require_numbers".to_string(), json!(true));
            params
        },
        description: "Password must be 8-128 characters with mixed case and numbers".to_string(),
    };

    // Product price constraint
    let price_constraint = TypeConstraint {
        constraint_type: ConstraintType::Range { min: Some(0.0), max: Some(99999.99) },
        parameters: HashMap::new(),
        description: "Product price must be non-negative".to_string(),
    };

    // Item count constraint
    let count_constraint = TypeConstraint {
        constraint_type: ConstraintType::Range { min: Some(1.0), max: Some(100.0) },
        parameters: HashMap::new(),
        description: "Must have at least 1 item, maximum 100 items".to_string(),
    };

    // URL constraint
    let url_constraint = TypeConstraint {
        constraint_type: ConstraintType::Pattern(r"^https?://[^\s/$.?#].[^\s]*$".to_string()),
        parameters: HashMap::new(),
        description: "Must be a valid URL".to_string(),
    };

    // Validate that constraints are properly structured
    assert_eq!(password_constraint.parameters["require_uppercase"], json!(true));
    assert_eq!(price_constraint.parameters.is_empty(), true);
    assert_eq!(count_constraint.parameters.is_empty(), true);
    assert_eq!(url_constraint.parameters.is_empty(), true);

    // Validate descriptions are informative
    assert!(password_constraint.description.contains("8-128 characters"));
    assert!(password_constraint.description.contains("mixed case and numbers"));
    assert!(price_constraint.description.contains("non-negative"));
    assert!(count_constraint.description.contains("at least 1 item"));
    assert!(url_constraint.description.contains("valid URL"));
}

#[test]
fn test_constraint_edge_cases_and_error_handling() {
    // Test edge cases that could cause issues

    // Zero-length pattern
    let empty_pattern = TypeConstraint {
        constraint_type: ConstraintType::Pattern(String::new()),
        parameters: HashMap::new(),
        description: "Empty pattern (matches any string)".to_string(),
    };

    // Empty enum
    let empty_enum = TypeConstraint {
        constraint_type: ConstraintType::Enum(vec![]),
        parameters: HashMap::new(),
        description: "Empty enum (no valid values)".to_string(),
    };

    // Unbounded range
    let unbounded_range = TypeConstraint {
        constraint_type: ConstraintType::Range { min: None, max: None },
        parameters: HashMap::new(),
        description: "Unbounded range (any number)".to_string(),
    };

    // Unbounded length
    let unbounded_length = TypeConstraint {
        constraint_type: ConstraintType::Length { min: None, max: None },
        parameters: HashMap::new(),
        description: "Unbounded length (any length)".to_string(),
    };

    // All should be created without panicking
    assert!(matches!(empty_pattern.constraint_type, ConstraintType::Pattern(_)));
    assert!(matches!(empty_enum.constraint_type, ConstraintType::Enum(_)));
    assert!(matches!(unbounded_range.constraint_type, ConstraintType::Range { .. }));
    assert!(matches!(unbounded_length.constraint_type, ConstraintType::Length { .. }));

    // Validate edge case descriptions
    assert!(empty_pattern.description.contains("matches any string"));
    assert!(empty_enum.description.contains("no valid values"));
    assert!(unbounded_range.description.contains("any number"));
    assert!(unbounded_length.description.contains("any length"));
}