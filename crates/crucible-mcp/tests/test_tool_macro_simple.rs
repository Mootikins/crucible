// tests/test_tool_macro_simple.rs
//! Simplified tests for the #[tool] attribute macro system
//! Tests only the working components without compilation errors

use crucible_mcp::rune_tools::{
    ToolMetadataStorage, ToolMacroMetadata, ParameterMetadata, TypeSpec,
    generate_schema
};
use std::sync::Arc;
use std::thread;

/// Helper to create mock metadata for testing
fn create_mock_metadata(name: &str, desc: &str) -> ToolMacroMetadata {
    ToolMacroMetadata {
        name: name.to_string(),
        description: desc.to_string(),
        parameters: vec![
            ParameterMetadata {
                name: "title".to_string(),
                type_spec: TypeSpec::String,
                is_optional: false,
            },
            ParameterMetadata {
                name: "content".to_string(),
                type_spec: TypeSpec::String,
                is_optional: false,
            },
            ParameterMetadata {
                name: "folder".to_string(),
                type_spec: TypeSpec::String,
                is_optional: true,
            },
        ],
    }
}

#[test]
fn test_macro_metadata_storage() {
    let storage = ToolMetadataStorage::global();
    storage.clear(); // Ensure clean state

    // Test inserting and retrieving metadata
    let metadata = create_mock_metadata("create_note", "Create a new markdown note");
    storage.insert("create_note".to_string(), metadata.clone());

    // Verify metadata is stored and retrievable
    let retrieved = storage.get("create_note").unwrap();
    assert_eq!(retrieved.name, "create_note");
    assert_eq!(retrieved.description, "Create a new markdown note");
    assert_eq!(retrieved.parameters.len(), 3);

    // Verify parameter details
    let title_param = &retrieved.parameters[0];
    assert_eq!(title_param.name, "title");
    assert_eq!(title_param.type_spec, TypeSpec::String);
    assert!(!title_param.is_optional);

    let folder_param = &retrieved.parameters[2];
    assert_eq!(folder_param.name, "folder");
    assert_eq!(folder_param.type_spec, TypeSpec::String);
    assert!(folder_param.is_optional);

    // Clean up
    storage.clear();
}

#[test]
fn test_schema_generation_required_params() {
    let metadata = create_mock_metadata("create_note", "Create a note");

    let schema = generate_schema(&metadata);

    // Verify schema structure
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"].is_object());

    // Verify required parameters only include non-optional params
    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 2); // title, content only
    assert!(required.contains(&serde_json::json!("title")));
    assert!(required.contains(&serde_json::json!("content")));
    assert!(!required.contains(&serde_json::json!("folder")));

    // Verify all parameters exist in properties
    let properties = schema["properties"].as_object().unwrap();
    assert_eq!(properties.len(), 3);
    assert!(properties.contains_key("title"));
    assert!(properties.contains_key("content"));
    assert!(properties.contains_key("folder"));
}

#[test]
fn test_schema_generation_optional_params() {
    let metadata = ToolMacroMetadata {
        name: "search_notes".to_string(),
        description: "Search notes by query".to_string(),
        parameters: vec![
            ParameterMetadata {
                name: "query".to_string(),
                type_spec: TypeSpec::String,
                is_optional: false,
            },
            ParameterMetadata {
                name: "limit".to_string(),
                type_spec: TypeSpec::Number,
                is_optional: true,
            },
            ParameterMetadata {
                name: "tags".to_string(),
                type_spec: TypeSpec::Array(Box::new(TypeSpec::String)),
                is_optional: true,
            },
        ],
    };

    let schema = generate_schema(&metadata);

    // Verify only query is required
    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert!(required.contains(&serde_json::json!("query")));

    // Verify all parameters exist in properties with correct types
    let properties = schema["properties"].as_object().unwrap();
    assert_eq!(properties["query"], serde_json::json!({"type": "string"}));
    assert_eq!(properties["limit"], serde_json::json!({"type": "number"}));
    assert_eq!(properties["tags"], serde_json::json!({
        "type": "array",
        "items": {"type": "string"}
    }));
}

#[test]
fn test_type_mapping_basic_types() {
    let metadata = ToolMacroMetadata {
        name: "type_test".to_string(),
        description: "Test basic types".to_string(),
        parameters: vec![
            ParameterMetadata {
                name: "string_param".to_string(),
                type_spec: TypeSpec::String,
                is_optional: false,
            },
            ParameterMetadata {
                name: "number_param".to_string(),
                type_spec: TypeSpec::Number,
                is_optional: false,
            },
            ParameterMetadata {
                name: "bool_param".to_string(),
                type_spec: TypeSpec::Boolean,
                is_optional: false,
            },
        ],
    };

    let schema = generate_schema(&metadata);
    let properties = schema["properties"].as_object().unwrap();

    assert_eq!(properties["string_param"], serde_json::json!({"type": "string"}));
    assert_eq!(properties["number_param"], serde_json::json!({"type": "number"}));
    assert_eq!(properties["bool_param"], serde_json::json!({"type": "boolean"}));

    // All should be required
    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 3);
}

#[test]
fn test_type_mapping_arrays() {
    let metadata = ToolMacroMetadata {
        name: "array_test".to_string(),
        description: "Test array types".to_string(),
        parameters: vec![
            ParameterMetadata {
                name: "string_array".to_string(),
                type_spec: TypeSpec::Array(Box::new(TypeSpec::String)),
                is_optional: false,
            },
            ParameterMetadata {
                name: "number_array".to_string(),
                type_spec: TypeSpec::Array(Box::new(TypeSpec::Number)),
                is_optional: true,
            },
            ParameterMetadata {
                name: "nested_array".to_string(),
                type_spec: TypeSpec::Array(Box::new(TypeSpec::Array(Box::new(TypeSpec::String)))),
                is_optional: false,
            },
        ],
    };

    let schema = generate_schema(&metadata);
    let properties = schema["properties"].as_object().unwrap();

    assert_eq!(properties["string_array"], serde_json::json!({
        "type": "array",
        "items": {"type": "string"}
    }));

    assert_eq!(properties["number_array"], serde_json::json!({
        "type": "array",
        "items": {"type": "number"}
    }));

    assert_eq!(properties["nested_array"], serde_json::json!({
        "type": "array",
        "items": {
            "type": "array",
            "items": {"type": "string"}
        }
    }));

    // Verify required excludes optional array
    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.contains(&serde_json::json!("string_array")));
    assert!(required.contains(&serde_json::json!("nested_array")));
    assert!(!required.contains(&serde_json::json!("number_array")));
}

#[test]
fn test_concurrent_storage_access() {
    let storage = Arc::new(ToolMetadataStorage::global());
    storage.clear();

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let storage = Arc::clone(&storage);
            thread::spawn(move || {
                let metadata = ToolMacroMetadata {
                    name: format!("tool_{}", i),
                    description: format!("Test tool {}", i),
                    parameters: vec![],
                };

                // Insert metadata
                storage.insert(format!("tool_{}", i), metadata);

                // Try to read it back
                let retrieved = storage.get(&format!("tool_{}", i));
                assert!(retrieved.is_some());
                assert_eq!(retrieved.unwrap().name, format!("tool_{}", i));
            })
        })
        .collect();

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all tools were stored
    assert_eq!(storage.count(), 10);

    let mut tool_names = storage.list_tools();
    tool_names.sort();
    for i in 0..10 {
        assert!(tool_names.contains(&format!("tool_{}", i)));
    }

    // Clean up
    storage.clear();
}

#[test]
fn test_tool_with_no_parameters() {
    let storage = ToolMetadataStorage::global();
    storage.clear();

    let metadata = ToolMacroMetadata {
        name: "get_stats".to_string(),
        description: "Get statistics about the vault".to_string(),
        parameters: vec![],
    };

    storage.insert("get_stats".to_string(), metadata.clone());

    let schema = generate_schema(&metadata);

    // Verify empty parameter schema
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"], serde_json::json!({}));
    assert_eq!(schema["required"], serde_json::json!([]));

    // Clean up
    storage.clear();
}

#[test]
fn test_all_optional_parameters() {
    let storage = ToolMetadataStorage::global();
    storage.clear();

    let metadata = ToolMacroMetadata {
        name: "optional_tool".to_string(),
        description: "Tool with all optional parameters".to_string(),
        parameters: vec![
            ParameterMetadata {
                name: "param1".to_string(),
                type_spec: TypeSpec::String,
                is_optional: true,
            },
            ParameterMetadata {
                name: "param2".to_string(),
                type_spec: TypeSpec::Number,
                is_optional: true,
            },
        ],
    };

    storage.insert("optional_tool".to_string(), metadata.clone());

    let schema = generate_schema(&metadata);

    // Verify no required parameters
    assert_eq!(schema["required"].as_array().unwrap().len(), 0);

    // But properties still exist
    assert_eq!(schema["properties"].as_object().unwrap().len(), 2);

    // Clean up
    storage.clear();
}

#[test]
fn test_storage_clear_and_count() {
    let storage = ToolMetadataStorage::global();
    storage.clear();

    assert_eq!(storage.count(), 0);

    // Add some tools
    for i in 0..5 {
        let metadata = ToolMacroMetadata {
            name: format!("tool_{}", i),
            description: format!("Tool {}", i),
            parameters: vec![],
        };
        storage.insert(format!("tool_{}", i), metadata);
    }

    assert_eq!(storage.count(), 5);

    // Clear and verify
    storage.clear();
    assert_eq!(storage.count(), 0);
    assert!(storage.get("tool_0").is_none());
}

#[test]
fn test_type_spec_from_rune_type_comprehensive() {
    // Basic types
    assert_eq!(TypeSpec::from_rune_type("string"), TypeSpec::String);
    assert_eq!(TypeSpec::from_rune_type("number"), TypeSpec::Number);
    assert_eq!(TypeSpec::from_rune_type("boolean"), TypeSpec::Boolean);

    // Alternative names
    assert_eq!(TypeSpec::from_rune_type("str"), TypeSpec::String);
    assert_eq!(TypeSpec::from_rune_type("int"), TypeSpec::Number);
    assert_eq!(TypeSpec::from_rune_type("bool"), TypeSpec::Boolean);

    // Array syntax variations
    assert_eq!(
        TypeSpec::from_rune_type("[string]"),
        TypeSpec::Array(Box::new(TypeSpec::String))
    );
    assert_eq!(
        TypeSpec::from_rune_type("array<number>"),
        TypeSpec::Array(Box::new(TypeSpec::Number))
    );

    // Nested arrays
    assert_eq!(
        TypeSpec::from_rune_type("[[string]]"),
        TypeSpec::Array(Box::new(TypeSpec::Array(Box::new(TypeSpec::String))))
    );

    // Unknown types default to object
    assert_eq!(TypeSpec::from_rune_type("CustomType"), TypeSpec::Object);
    assert_eq!(TypeSpec::from_rune_type("vector<string>"), TypeSpec::Object);
}

#[test]
fn test_complex_parameter_types() {
    let metadata = ToolMacroMetadata {
        name: "complex_tool".to_string(),
        description: "A tool with complex parameter types".to_string(),
        parameters: vec![
            ParameterMetadata {
                name: "tags".to_string(),
                type_spec: TypeSpec::Array(Box::new(TypeSpec::String)),
                is_optional: false,
            },
            ParameterMetadata {
                name: "metadata".to_string(),
                type_spec: TypeSpec::Object,
                is_optional: true,
            },
            ParameterMetadata {
                name: "active".to_string(),
                type_spec: TypeSpec::Boolean,
                is_optional: false,
            },
        ],
    };

    let schema = generate_schema(&metadata);

    // Verify complex type mapping
    let properties = schema["properties"].as_object().unwrap();
    assert_eq!(
        properties.get("tags"),
        Some(&serde_json::json!({
            "type": "array",
            "items": {"type": "string"}
        }))
    );
    assert_eq!(properties.get("metadata"), Some(&serde_json::json!({"type": "object"})));
    assert_eq!(properties.get("active"), Some(&serde_json::json!({"type": "boolean"})));

    // Verify required array
    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.contains(&serde_json::json!("tags")));
    assert!(required.contains(&serde_json::json!("active")));
    assert!(!required.contains(&serde_json::json!("metadata")));
}