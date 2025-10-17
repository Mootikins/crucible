/// Comprehensive tests for the TypeInferenceEngine
///
/// Tests the sophisticated type inference capabilities including:
/// - Naming convention analysis
/// - Parameter type inference
/// - Constraint generation
/// - Execution-based type verification
/// - Edge cases and error handling

use anyhow::Result;
use std::sync::Arc;
use crucible_mcp::rune_tools::{
    TypeInferenceEngine, AnalyzerConfig, RuneType, TypeConstraint, ConstraintType
};

#[test]
fn test_type_inference_engine_creation() -> Result<()> {
    let config = AnalyzerConfig::default();
    let engine = TypeInferenceEngine::new(config);

    // Engine should be created successfully
    // Test that we can infer types from naming conventions
    let function_path = &["file", "create_file"];
    let execution_results = &[];

    let inferred_types = engine.infer_parameter_types(
        &Arc::new(rune::Unit::default()),
        function_path,
        execution_results
    )?;

    // Should infer file operation parameters
    assert!(!inferred_types.is_empty());

    // Check that we get path and content parameters for file creation
    let param_names: Vec<_> = inferred_types.iter().map(|(name, _, _)| name).collect();
    assert!(param_names.iter().any(|&s| s == "path"));
    assert!(param_names.iter().any(|&s| s == "content"));

    Ok(())
}

#[test]
fn test_file_operations_type_inference() -> Result<()> {
    let config = AnalyzerConfig::default();
    let engine = TypeInferenceEngine::new(config);

    let test_cases = vec![
        ("file", "create_file", vec!["path", "content"]),
        ("file", "delete_file", vec!["path"]),
        ("file", "read_file", vec!["path"]),
        ("file", "write_file", vec!["path", "content"]),
        ("file", "copy_file", vec!["source", "destination"]),
        ("file", "move_file", vec!["source", "destination"]),
        ("file", "list_files", vec!["directory", "pattern", "recursive"]),
    ];

    for (module_name, function_name, expected_params) in test_cases {
        let function_path = vec![module_name, function_name];
        let inferred_types = engine.infer_parameter_types(
            &Arc::new(rune::Unit::default()),
            function_path.as_slice(),
            &[]
        )?;

        let param_names: Vec<_> = inferred_types.iter().map(|(name, _, _)| name).collect();

        for expected_param in expected_params {
            assert!(param_names.iter().any(|&s| s == expected_param),
                    "Expected parameter '{}' not found for {}.{}",
                    expected_param, module_name, function_name);
        }
    }

    Ok(())
}

#[test]
fn test_search_operations_type_inference() -> Result<()> {
    let config = AnalyzerConfig::default();
    let engine = TypeInferenceEngine::new(config);

    let test_cases = vec![
        ("search", "search", vec!["query", "scope", "case_sensitive"]),
        ("search", "search_files", vec!["pattern", "directory", "max_results"]),
        ("search", "search_content", vec!["content", "files", "regex"]),
        ("query", "find", vec!["query", "scope"]),
        ("find", "filter", vec!["pattern", "case_sensitive"]),
    ];

    for (module_name, function_name, expected_params) in test_cases {
        let function_path = vec![module_name, function_name];
        let inferred_types = engine.infer_parameter_types(
            &Arc::new(rune::Unit::default()),
            function_path.as_slice(),
            &[]
        )?;

        let param_names: Vec<_> = inferred_types.iter().map(|(name, _, _)| name).collect();

        for expected_param in expected_params {
            assert!(param_names.iter().any(|&s| s == expected_param),
                    "Expected parameter '{}' not found for {}.{}",
                    expected_param, module_name, function_name);
        }
    }

    Ok(())
}

#[test]
fn test_ui_operations_type_inference() -> Result<()> {
    let config = AnalyzerConfig::default();
    let engine = TypeInferenceEngine::new(config);

    let test_cases = vec![
        ("ui", "format_results", vec!["data", "format", "options"]),
        ("ui", "get_suggestions", vec!["input", "context", "max_suggestions"]),
        ("ui", "prompt", vec!["message", "default", "options"]),
        ("format", "display", vec!["data", "format"]),
        ("display", "show", vec!["content", "style"]),
    ];

    for (module_name, function_name, expected_params) in test_cases {
        let function_path = vec![module_name, function_name];
        let inferred_types = engine.infer_parameter_types(
            &Arc::new(rune::Unit::default()),
            function_path.as_slice(),
            &[]
        )?;

        let param_names: Vec<_> = inferred_types.iter().map(|(name, _, _)| name).collect();

        for expected_param in expected_params {
            assert!(param_names.iter().any(|&s| s == expected_param),
                    "Expected parameter '{}' not found for {}.{}",
                    expected_param, module_name, function_name);
        }
    }

    Ok(())
}

#[test]
fn test_agent_operations_type_inference() -> Result<()> {
    let config = AnalyzerConfig::default();
    let engine = TypeInferenceEngine::new(config);

    let test_cases = vec![
        ("agent", "analyze", vec!["data", "context", "options"]),
        ("agent", "recommend", vec!["data", "context", "options"]),
        ("ai", "optimize", vec!["target", "strategy", "constraints"]),
        ("assist", "assist", vec!["target", "options"]),
    ];

    for (module_name, function_name, expected_params) in test_cases {
        let function_path = vec![module_name, function_name];
        let inferred_types = engine.infer_parameter_types(
            &Arc::new(rune::Unit::default()),
            function_path.as_slice(),
            &[]
        )?;

        let param_names: Vec<_> = inferred_types.iter().map(|(name, _, _)| name).collect();

        for expected_param in expected_params {
            assert!(param_names.iter().any(|&s| s == expected_param),
                    "Expected parameter '{}' not found for {}.{}",
                    expected_param, module_name, function_name);
        }
    }

    Ok(())
}

#[test]
fn test_advanced_operations_type_inference() -> Result<()> {
    let config = AnalyzerConfig::default();
    let engine = TypeInferenceEngine::new(config);

    let test_cases = vec![
        ("advanced", "test", vec!["target", "config", "iterations"]),
        ("advanced", "benchmark", vec!["target", "config", "iterations"]),
        ("utils", "validate", vec!["input", "rules", "strict"]),
        ("helpers", "sanitize", vec!["input", "rules", "strict"]),
        ("tools", "transform", vec!["data", "operation", "options"]),
    ];

    for (module_name, function_name, expected_params) in test_cases {
        let function_path = vec![module_name, function_name];
        let inferred_types = engine.infer_parameter_types(
            &Arc::new(rune::Unit::default()),
            function_path.as_slice(),
            &[]
        )?;

        let param_names: Vec<_> = inferred_types.iter().map(|(name, _, _)| name).collect();

        for expected_param in expected_params {
            assert!(param_names.iter().any(|&s| s == expected_param),
                    "Expected parameter '{}' not found for {}.{}",
                    expected_param, module_name, function_name);
        }
    }

    Ok(())
}

#[test]
fn test_constraint_generation_for_types() -> Result<()> {
    let config = AnalyzerConfig::default();
    let engine = TypeInferenceEngine::new(config);

    // Test constraint generation for different types
    let function_path = vec!["file", "create_file"];
    let constraints = engine.generate_constraints_for_type(&RuneType::String, &function_path);

    assert!(!constraints.is_empty());

    // Should generate length constraint for strings
    let length_constraints: Vec<_> = constraints.iter()
        .filter(|c| matches!(c.constraint_type, ConstraintType::Length { .. }))
        .collect();

    assert!(!length_constraints.is_empty(), "Should generate length constraints for string types");

    // Test number constraints
    let number_constraints = engine.generate_constraints_for_type(&RuneType::Number, &function_path);
    let range_constraints: Vec<_> = number_constraints.iter()
        .filter(|c| matches!(c.constraint_type, ConstraintType::Range { .. }))
        .collect();

    assert!(!range_constraints.is_empty(), "Should generate range constraints for number types");

    Ok(())
}

#[test]
fn test_path_specific_constraints() -> Result<()> {
    let config = AnalyzerConfig::default();
    let engine = TypeInferenceEngine::new(config);

    // Test that path-related functions get pattern constraints
    let path_function_path = vec!["file", "create_file"];
    let path_constraints = engine.generate_constraints_for_type(&RuneType::String, &path_function_path);

    let pattern_constraints: Vec<_> = path_constraints.iter()
        .filter(|c| matches!(c.constraint_type, ConstraintType::Pattern(..)))
        .collect();

    assert!(!pattern_constraints.is_empty(),
           "Should generate pattern constraints for file path parameters");

    // Test pattern constraint contains path-like pattern
    for constraint in pattern_constraints {
        if let ConstraintType::Pattern(pattern) = &constraint.constraint_type {
            assert!(pattern.contains("[a-zA-Z0-9_\\-./]+") ||
                   pattern.contains("path") ||
                   pattern.contains("file"),
                   "Pattern constraint should validate file paths: {}", pattern);
        }
    }

    Ok(())
}

#[test]
fn test_inferred_parameter_types_structure() -> Result<()> {
    let config = AnalyzerConfig::default();
    let engine = TypeInferenceEngine::new(config);

    let function_path = vec!["search", "search_files"];
    let inferred_types = engine.infer_parameter_types(
        &Arc::new(rune::Unit::default()),
        &function_path,
        &[]
    )?;

    // Should have multiple inferred parameters
    assert!(!inferred_types.is_empty(), "Should infer multiple parameters");

    // Each inference should contain name, type, and constraints
    for (param_name, rune_type, constraints) in inferred_types {
        assert!(!param_name.is_empty(), "Parameter name should not be empty");
        assert!(!constraints.is_empty(), "Should generate constraints for parameter {}", param_name);

        // Verify type is a valid RuneType
        match rune_type {
            RuneType::String | RuneType::Number | RuneType::Integer | RuneType::Boolean |
            RuneType::Array(_) | RuneType::Object(_) | RuneType::Any => {
                // These are all valid types
            },
            RuneType::Unknown(desc) => {
                assert!(!desc.is_empty(), "Unknown type should have description");
            },
            _ => {
                // Other types should also be valid
            }
        }
    }

    Ok(())
}

#[test]
fn test_function_name_pattern_analysis() -> Result<()> {
    let config = AnalyzerConfig::default();
    let engine = TypeInferenceEngine::new(config);

    // Test function name pattern analysis for common operations
    let test_cases = vec![
        ("unknown_module", "create_user", vec!["input"]), // create -> input object
        ("unknown_module", "delete_record", vec!["id"]), // delete -> id
        ("unknown_module", "update_profile", vec!["id", "data"]), // update -> id + data
        ("unknown_module", "get_settings", vec!["query", "fields"]), // get -> query + fields
        ("unknown_module", "list_users", vec!["filter", "limit"]), // list -> filter + limit
        ("unknown_module", "count_items", vec!["collection"]), // count -> collection
        ("unknown_module", "is_valid", vec!["input"]), // is_ -> boolean check
        ("unknown_module", "can_edit", vec!["input"]), // can_ -> boolean check
        ("unknown_module", "has_permission", vec!["input"]), // has_ -> boolean check
    ];

    for (module_name, function_name, expected_params) in test_cases {
        let function_path = vec![module_name, function_name];
        let inferred_types = engine.infer_parameter_types(
            &Arc::new(rune::Unit::default()),
            function_path.as_slice(),
            &[]
        )?;

        let param_names: Vec<_> = inferred_types.iter().map(|(name, _, _)| name).collect();

        for expected_param in expected_params {
            assert!(param_names.iter().any(|&s| s == expected_param),
                    "Expected parameter '{}' not found for {}.{}",
                    expected_param, module_name, function_name);
        }
    }

    Ok(())
}

#[test]
fn test_inference_with_empty_constraints() -> Result<()> {
    let config = AnalyzerConfig::default();
    let engine = TypeInferenceEngine::new(config);

    // Test inference when constraints are empty (should return Any type)
    let function_path = vec!["unknown", "unknown_function"];
    let inferred_types = engine.infer_parameter_types(
        &Arc::new(rune::Unit::default()),
        &function_path,
        &[]
    )?;

    // Should provide at least one generic parameter
    assert!(!inferred_types.is_empty(), "Should provide at least one parameter for unknown function");

    // The parameter should have some type (likely Any or Object)
    for (_, rune_type, _) in &inferred_types {
        match rune_type {
            RuneType::Any | RuneType::Object(_) => {
                // Expected fallback types
            },
            _ => {
                // Other types are also acceptable
            }
        }
    }

    Ok(())
}

#[test]
fn test_edge_cases_and_error_handling() -> Result<()> {
    let config = AnalyzerConfig::default();
    let engine = TypeInferenceEngine::new(config);

    // Test with empty function path
    let empty_path: &[&str] = &[];
    let result = engine.infer_parameter_types(
        &Arc::new(rune::Unit::default()),
        empty_path,
        &[]
    );

    // Should handle empty path gracefully
    assert!(result.is_ok(), "Should handle empty function path gracefully");

    // Test with very long function and module names
    let long_path = vec!["very_long_module_name_for_testing", "very_long_function_name_for_testing_purposes"];
    let result = engine.infer_parameter_types(
        &Arc::new(rune::Unit::default()),
        long_path.as_slice(),
        &[]
    );

    assert!(result.is_ok(), "Should handle long names gracefully");

    // Verify result is not empty
    if let Ok(inferred_types) = result {
        assert!(!inferred_types.is_empty(), "Should still infer parameters for long names");
    }

    Ok(())
}

#[test]
fn test_type_constraint_validation() -> Result<()> {
    // Test that generated constraints are properly structured
    let constraint = TypeConstraint {
        constraint_type: ConstraintType::Length { min: Some(1), max: Some(1000) },
        parameters: std::collections::HashMap::new(),
        description: "Test length constraint".to_string(),
    };

    // Verify constraint structure
    match constraint.constraint_type {
        ConstraintType::Length { min, max } => {
            assert_eq!(min, Some(1));
            assert_eq!(max, Some(1000));
        },
        _ => panic!("Expected Length constraint type"),
    }

    assert_eq!(constraint.description, "Test length constraint");
    assert!(constraint.parameters.is_empty());

    Ok(())
}