/// AST Integration Tests with Real Rune Files
///
/// Tests the complete pipeline:
/// 1. Load and compile real Rune source files
/// 2. Perform AST analysis and module discovery
/// 3. Generate type information using TypeInferenceEngine
/// 4. Create JSON schemas using SchemaValidator
/// 5. Validate the complete workflow

use anyhow::Result;
use serde_json::json;
use std::sync::Arc;
use crucible_mcp::rune_tools::{
    RuneAstAnalyzer, AsyncFunctionInfo, SchemaValidator, ValidationConfig,
    SourceLocation
};

/// Helper function to load and compile a Rune source file
fn load_and_compile_rune_file(file_path: &str) -> Result<Arc<rune::Unit>> {
    let source = std::fs::read_to_string(file_path)?;
    let context = rune::Context::with_default_modules()?;
    let source_obj = rune::Source::memory(&source)?;
    let mut sources = rune::Sources::new();
    sources.insert(source_obj)?;

    let mut diagnostics = rune::Diagnostics::new();
    let result = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build();

    // Report warnings but don't fail on them
    if !diagnostics.is_empty() {
        let mut writer = rune::termcolor::StandardStream::stderr(rune::termcolor::ColorChoice::Always);
        diagnostics.emit(&mut writer, &sources)?;
    }

    let unit = result?;
    Ok(Arc::new(unit))
}

/// Create a mock AsyncFunctionInfo for testing
fn create_mock_function_info(
    name: &str,
    module_path: Vec<String>,
    parameters: Vec<crucible_mcp::rune_tools::ParameterInfo>
) -> AsyncFunctionInfo {
    AsyncFunctionInfo {
        name: name.to_string(),
        is_async: true,
        is_public: true,
        parameters,
        return_type: Some("Result".to_string()),
        module_path: module_path.clone(),
        full_path: [module_path, vec![name.to_string()]].concat(),
        description: Some(format!("{} function", name)),
        doc_comments: vec![],
        source_location: SourceLocation {
            line: None,
            column: None,
            file_path: None,
        },
        metadata: std::collections::HashMap::new(),
    }
}

#[test]
fn test_file_operations_integration() -> Result<()> {
    let analyzer = RuneAstAnalyzer::new()?;
    let validator = SchemaValidator::new(ValidationConfig::default());

    // Load and compile the file operations module
    let unit = load_and_compile_rune_file("test_data/file_operations.rn")?;

    // Analyze modules
    let modules = analyzer.analyze_modules(&unit)?;
    assert!(!modules.is_empty(), "Should discover at least one module");

    // Check that we found the expected modules
    let module_names: Vec<String> = modules.iter().map(|m| m.name.clone()).collect();
    println!("DEBUG: Found modules: {:?}", module_names);
    assert!(module_names.contains(&"file".to_string()), "Should discover 'file' module");
    assert!(module_names.contains(&"search".to_string()), "Should discover 'search' module");

    // Test file module functions
    if let Some(file_module) = modules.iter().find(|m| m.name == "file") {
        assert!(!file_module.functions.is_empty(), "File module should have functions");

        // Test function discovery
        let function_names: Vec<String> = file_module.functions.iter()
            .map(|f| f.name.clone())
            .collect();

        assert!(function_names.contains(&"create_file".to_string()));
        assert!(function_names.contains(&"read_file".to_string()));
        assert!(function_names.contains(&"delete_file".to_string()));
        assert!(function_names.contains(&"copy_file".to_string()));
        assert!(function_names.contains(&"list_files".to_string()));

        // Test schema generation for a function
        if let Some(create_file_func) = file_module.functions.iter()
            .find(|f| f.name == "create_file") {

            let schema = validator.generate_function_schema(create_file_func)?;

            // Verify schema structure
            assert_eq!(schema["type"], "object");
            assert!(schema["properties"].is_object());
            assert!(schema["required"].is_array());

            let properties = schema["properties"].as_object().unwrap();
            assert!(properties.contains_key("path"));
            assert!(properties.contains_key("content"));
        }
    }

    // Test search module functions
    if let Some(search_module) = modules.iter().find(|m| m.name == "search") {
        assert!(!search_module.functions.is_empty(), "Search module should have functions");

        let function_names: Vec<String> = search_module.functions.iter()
            .map(|f| f.name.clone())
            .collect();

        assert!(function_names.contains(&"search_files".to_string()));
        assert!(function_names.contains(&"search_content".to_string()));
    }

    Ok(())
}

#[test]
fn test_ui_helpers_integration() -> Result<()> {
    let analyzer = RuneAstAnalyzer::new()?;
    let validator = SchemaValidator::new(ValidationConfig::default());

    // Load and compile the UI helpers module
    let unit = load_and_compile_rune_file("test_data/ui_helpers.rn")?;

    // Analyze modules
    let modules = analyzer.analyze_modules(&unit)?;
    assert!(!modules.is_empty(), "Should discover UI modules");

    // Check for UI-specific modules
    let module_names: Vec<String> = modules.iter().map(|m| m.name.clone()).collect();
    println!("DEBUG: UI test found modules: {:?}", module_names);
    assert!(module_names.contains(&"ui".to_string()), "Should discover 'ui' module");
    assert!(module_names.contains(&"ui_helpers".to_string()), "Should discover 'ui_helpers' module");

    // Test UI module functions
    if let Some(ui_module) = modules.iter().find(|m| m.name == "ui") {
        let function_names: Vec<String> = ui_module.functions.iter()
            .map(|f| f.name.clone())
            .collect();

        assert!(function_names.contains(&"format_results".to_string()));
        assert!(function_names.contains(&"get_suggestions".to_string()));
        assert!(function_names.contains(&"prompt_user".to_string()));
        assert!(function_names.contains(&"display_data".to_string()));

        // Test schema generation for format_results
        if let Some(format_func) = ui_module.functions.iter()
            .find(|f| f.name == "format_results") {

            let schema = validator.generate_function_schema(format_func)?;

            let properties = schema["properties"].as_object().unwrap();
            assert!(properties.contains_key("format"));

            // Check that format has validation (enum-like behavior)
            let format_schema = &properties["format"];
            assert_eq!(format_schema["type"], "string");
        }
    }

    Ok(())
}

#[test]
fn test_agent_tools_integration() -> Result<()> {
    let analyzer = RuneAstAnalyzer::new()?;
    let validator = SchemaValidator::new(ValidationConfig::default());

    // Load and compile the agent tools module
    let unit = load_and_compile_rune_file("test_data/agent_tools.rn")?;

    // Analyze modules
    let modules = analyzer.analyze_modules(&unit)?;
    assert!(!modules.is_empty(), "Should discover agent modules");

    // Check for agent-specific modules
    let module_names: Vec<String> = modules.iter().map(|m| m.name.clone()).collect();
    assert!(module_names.contains(&"agent".to_string()), "Should discover 'agent' module");
    assert!(module_names.contains(&"ai".to_string()), "Should discover 'ai' module");

    // Test agent module functions
    if let Some(agent_module) = modules.iter().find(|m| m.name == "agent") {
        let function_names: Vec<String> = agent_module.functions.iter()
            .map(|f| f.name.clone())
            .collect();

        assert!(function_names.contains(&"analyze_data".to_string()));
        assert!(function_names.contains(&"recommend".to_string()));
        assert!(function_names.contains(&"optimize".to_string()));

        // Test schema generation for analyze_data
        if let Some(analyze_func) = agent_module.functions.iter()
            .find(|f| f.name == "analyze_data") {

            let schema = validator.generate_function_schema(analyze_func)?;

            let properties = schema["properties"].as_object().unwrap();
            assert!(properties.contains_key("data"));
            assert!(properties.contains_key("context"));
            assert!(properties.contains_key("options"));
        }
    }

    Ok(())
}

#[test]
fn test_advanced_features_integration() -> Result<()> {
    let analyzer = RuneAstAnalyzer::new()?;
    let validator = SchemaValidator::new(ValidationConfig::default());

    // Load and compile the advanced features module
    let unit = load_and_compile_rune_file("test_data/advanced_features.rn")?;

    // Analyze modules
    let modules = analyzer.analyze_modules(&unit)?;
    assert!(!modules.is_empty(), "Should discover advanced modules");

    // Check for advanced modules
    let module_names: Vec<String> = modules.iter().map(|m| m.name.clone()).collect();
    assert!(module_names.contains(&"advanced".to_string()), "Should discover 'advanced' module");
    assert!(module_names.contains(&"utils".to_string()), "Should discover 'utils' module");
    assert!(module_names.contains(&"tools".to_string()), "Should discover 'tools' module");

    // Test advanced module functions
    if let Some(advanced_module) = modules.iter().find(|m| m.name == "advanced") {
        let function_names: Vec<String> = advanced_module.functions.iter()
            .map(|f| f.name.clone())
            .collect();

        assert!(function_names.contains(&"benchmark".to_string()));
        assert!(function_names.contains(&"test_system".to_string()));
        assert!(function_names.contains(&"validate_complex".to_string()));

        // Test schema generation for benchmark
        if let Some(benchmark_func) = advanced_module.functions.iter()
            .find(|f| f.name == "benchmark") {

            let schema = validator.generate_function_schema(benchmark_func)?;

            let properties = schema["properties"].as_object().unwrap();
            assert!(properties.contains_key("target"));
            assert!(properties.contains_key("config"));
            assert!(properties.contains_key("iterations"));

            // iterations should be an integer
            let iterations_schema = &properties["iterations"];
            assert_eq!(iterations_schema["type"], "integer");
        }
    }

    Ok(())
}

#[test]
fn test_cross_module_validation() -> Result<()> {
    let validator = SchemaValidator::new(ValidationConfig::default());

    // Create a complex function with multiple parameter types
    let complex_function = create_mock_function_info(
        "complex_operation",
        vec!["test".to_string()],
        vec![
            crucible_mcp::rune_tools::ParameterInfo {
                name: "config".to_string(),
                type_name: "Object".to_string(),
                type_constraints: vec!["Object".to_string()],
                is_optional: false,
                default_value: None,
                description: Some("Configuration object".to_string()),
                validation_rules: vec![],
            },
            crucible_mcp::rune_tools::ParameterInfo {
                name: "iterations".to_string(),
                type_name: "Integer".to_string(),
                type_constraints: vec!["Integer".to_string()],
                is_optional: true,
                default_value: Some(json!(10)),
                description: Some("Number of iterations".to_string()),
                validation_rules: vec![],
            },
            crucible_mcp::rune_tools::ParameterInfo {
                name: "verbose".to_string(),
                type_name: "Boolean".to_string(),
                type_constraints: vec!["Boolean".to_string()],
                is_optional: true,
                default_value: Some(json!(false)),
                description: Some("Enable verbose output".to_string()),
                validation_rules: vec![],
            }
        ]
    );

    // Generate schema
    let schema = validator.generate_function_schema(&complex_function)?;

    // Validate schema structure
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"].is_object());
    assert!(schema["required"].is_array());

    let properties = schema["properties"].as_object().unwrap();
    assert_eq!(properties.len(), 3);

    // Test parameter validation
    let valid_params = json!({
        "config": {"key": "value"},
        "iterations": 20,
        "verbose": true
    });

    let result = validator.validate_function_parameters(&complex_function, &valid_params);
    assert!(result.is_valid, "Valid parameters should pass validation");

    // Test missing required parameter
    let invalid_params = json!({
        "iterations": 20,
        "verbose": true
    });

    let result = validator.validate_function_parameters(&complex_function, &invalid_params);
    assert!(!result.is_valid, "Missing required parameter should fail validation");

    Ok(())
}

#[test]
fn test_schema_validation_workflow() -> Result<()> {
    let analyzer = RuneAstAnalyzer::new()?;
    let validator = SchemaValidator::new(ValidationConfig::default());

    // Load a real module
    let unit = load_and_compile_rune_file("test_data/file_operations.rn")?;
    let modules = analyzer.analyze_modules(&unit)?;

    // Find the file module and create_file function
    if let Some(file_module) = modules.iter().find(|m| m.name == "file") {
        if let Some(create_file_func) = file_module.functions.iter()
            .find(|f| f.name == "create_file") {

            // Generate schema
            let _schema = validator.generate_function_schema(create_file_func)?;

            // Create test parameters that match the expected schema
            let test_params = json!({
                "path": "/tmp/test.txt",
                "content": "Hello, world!"
            });

            // Validate parameters against schema
            let result = validator.validate_function_parameters(create_file_func, &test_params);
            assert!(result.is_valid, "Valid parameters should pass schema validation");

            // Test with invalid parameters
            let invalid_params = json!({
                // Missing required 'path' parameter
                "content": "Hello, world!"
            });

            let result = validator.validate_function_parameters(create_file_func, &invalid_params);
            assert!(!result.is_valid, "Invalid parameters should fail schema validation");
        }
    }

    Ok(())
}

#[test]
fn test_error_handling_integration() -> Result<()> {
    let analyzer = RuneAstAnalyzer::new()?;

    // Test with invalid Rune source file
    let invalid_source = r#"
        pub mod invalid {
            pub async fn broken_function(args) {
                // Missing closing brace - this should fail compilation
        "#;

    let context = rune::Context::with_default_modules()?;
    let source_obj = rune::Source::memory(invalid_source)?;
    let mut sources = rune::Sources::new();
    sources.insert(source_obj)?;

    let mut diagnostics = rune::Diagnostics::new();
    let result = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build();

    // Should fail to compile
    assert!(result.is_err(), "Invalid source should fail to compile");

    // Test with non-existent file
    let result = load_and_compile_rune_file("non_existent_file.rn");
    assert!(result.is_err(), "Non-existent file should return error");

    Ok(())
}

#[test]
fn test_performance_considerations() -> Result<()> {
    let analyzer = RuneAstAnalyzer::new()?;
    let validator = SchemaValidator::new(ValidationConfig::default());

    // Load multiple files and test performance
    let files = vec![
        "test_data/file_operations.rn",
        "test_data/ui_helpers.rn",
        "test_data/agent_tools.rn",
        "test_data/advanced_features.rn"
    ];

    let mut total_modules = 0;
    let mut total_functions = 0;

    for file_path in files {
        if let Ok(unit) = load_and_compile_rune_file(file_path) {
            let modules = analyzer.analyze_modules(&unit)?;
            total_modules += modules.len();
            total_functions += modules.iter().map(|m| m.functions.len()).sum::<usize>();

            // Generate schemas for all functions to test performance
            for module in &modules {
                for function in &module.functions {
                    let _schema = validator.generate_function_schema(function)?;
                }
            }
        }
    }

    assert!(total_modules > 0, "Should discover modules from files");
    assert!(total_functions > 0, "Should discover functions from modules");

    // Basic performance assertions (these numbers may need adjustment)
    assert!(total_modules >= 8, "Should discover at least 8 modules total");
    assert!(total_functions >= 15, "Should discover at least 15 functions total");

    Ok(())
}

#[test]
fn test_edge_cases_integration() -> Result<()> {
    let analyzer = RuneAstAnalyzer::new()?;

    // Test with empty module
    let empty_source = r#"
        pub mod empty_module {
            // No functions in this module
        }
    "#;

    let context = rune::Context::with_default_modules()?;
    let source_obj = rune::Source::memory(empty_source)?;
    let mut sources = rune::Sources::new();
    sources.insert(source_obj)?;

    let mut diagnostics = rune::Diagnostics::new();
    let result = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build();

    if let Ok(unit) = result {
        let arc_unit = Arc::new(unit);
        let modules = analyzer.analyze_modules(&arc_unit)?;
        // Should not discover any async functions in empty module
        let total_functions: usize = modules.iter().map(|m| m.functions.len()).sum();
        assert_eq!(total_functions, 0, "Empty module should have no discovered functions");
    }

    Ok(())
}