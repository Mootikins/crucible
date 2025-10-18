// tests/test_tool_macro.rs
//! Comprehensive tests for the #[tool] attribute macro system

use crucible_mcp::rune_tools::{
    ToolRegistry, build_crucible_module, ToolDiscovery, DiscoveredTool,
    ToolMetadataStorage, ToolMacroMetadata, ParameterMetadata, TypeSpec,
    generate_schema
};
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::obsidian_client::ObsidianClient;
use std::sync::Arc;
use tempfile::TempDir;
use serde_json::json;

/// Helper to create a test directory and tool registry
async fn setup_test_registry() -> (TempDir, ToolRegistry) {
    let temp_dir = TempDir::new().unwrap();
    let tool_dir = temp_dir.path().to_path_buf();

    // Build registry with stdlib
    let context = Arc::new(rune::Context::with_default_modules().unwrap());
    let registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), context);

    (temp_dir, registry)
}

/// Helper to write a test tool file
fn write_test_tool_file(tool_dir: &std::path::Path, content: &str) -> std::path::PathBuf {
    let tool_file = tool_dir.join("test_tool.rn");
    std::fs::write(&tool_file, content).unwrap();
    tool_file
}

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

#[tokio::test]
async fn test_macro_metadata_extraction() {
    let storage = ToolMetadataStorage::global();
    storage.clear(); // Ensure clean state

    // Simulate metadata extraction by inserting directly
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
}

#[tokio::test]
async fn test_schema_generation_required_params() {
    let metadata = create_mock_metadata("create_note", "Create a note");

    let schema = generate_schema(&metadata);

    // Verify schema structure
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"].is_object());

    // Verify required parameters only include non-optional params
    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 2); // title, content only
    assert!(required.contains(&json!("title")));
    assert!(required.contains(&json!("content")));
    assert!(!required.contains(&json!("folder")));

    // Verify all parameters exist in properties
    let properties = schema["properties"].as_object().unwrap();
    assert_eq!(properties.len(), 3);
    assert!(properties.contains_key("title"));
    assert!(properties.contains_key("content"));
    assert!(properties.contains_key("folder"));
}

#[tokio::test]
async fn test_schema_generation_optional_params() {
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
    assert!(required.contains(&json!("query")));

    // Verify all parameters exist in properties with correct types
    let properties = schema["properties"].as_object().unwrap();
    assert_eq!(properties["query"], json!({"type": "string"}));
    assert_eq!(properties["limit"], json!({"type": "number"}));
    assert_eq!(properties["tags"], json!({
        "type": "array",
        "items": {"type": "string"}
    }));
}

#[tokio::test]
async fn test_discovery_prefers_macro_metadata() {
    let (_temp_dir, registry) = setup_test_registry().await;

    // First, insert macro metadata for a tool
    let storage = ToolMetadataStorage::global();
    storage.clear();

    let metadata = create_mock_metadata("create_note", "Create a new markdown note");
    storage.insert("create_note".to_string(), metadata);

    // Write a tool file that would normally require AST inference
    let tool_source = r#"
        pub async fn create_note(title: string, content: string) {
            #{ success: true }
        }
    "#;

    write_test_tool_file(registry.tool_dir(), tool_source);

    // Discover tools - should use macro metadata
    let discovered = registry.discover_tools().await.unwrap();
    assert_eq!(discovered.len(), 1);

    let tool = &discovered[0];
    assert_eq!(tool.name, "create_note");
    assert_eq!(tool.description, "Create a new markdown note");

    // Verify schema matches macro metadata (has folder param from metadata)
    let schema = &tool.input_schema;
    assert_eq!(schema["properties"].as_object().unwrap().len(), 3);
    assert!(schema["properties"].contains_key("folder"));
}

#[tokio::test]
async fn test_fallback_to_ast_inference() {
    let (_temp_dir, registry) = setup_test_registry().await;

    // Clear storage to ensure no macro metadata exists
    let storage = ToolMetadataStorage::global();
    storage.clear();

    // Write tool without macro metadata
    let tool_source = r#"
        pub async fn simple_tool(param1: string, param2?: number) {
            #{ success: true }
        }
    "#;

    write_test_tool_file(registry.tool_dir(), tool_source);

    // Discover tools - should fall back to AST inference
    let discovered = registry.discover_tools().await.unwrap();
    assert_eq!(discovered.len(), 1);

    let tool = &discovered[0];
    assert_eq!(tool.name, "simple_tool");

    // Verify basic schema structure was inferred
    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"].is_object());
}

#[tokio::test]
async fn test_type_mapping_basic_types() {
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

    assert_eq!(properties["string_param"], json!({"type": "string"}));
    assert_eq!(properties["number_param"], json!({"type": "number"}));
    assert_eq!(properties["bool_param"], json!({"type": "boolean"}));

    // All should be required
    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 3);
}

#[tokio::test]
async fn test_type_mapping_arrays() {
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

    assert_eq!(properties["string_array"], json!({
        "type": "array",
        "items": {"type": "string"}
    }));

    assert_eq!(properties["number_array"], json!({
        "type": "array",
        "items": {"type": "number"}
    }));

    assert_eq!(properties["nested_array"], json!({
        "type": "array",
        "items": {
            "type": "array",
            "items": {"type": "string"}
        }
    }));

    // Verify required excludes optional array
    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.contains(&json!("string_array")));
    assert!(required.contains(&json!("nested_array")));
    assert!(!required.contains(&json!("number_array")));
}

#[test]
fn test_concurrent_storage_access() {
    use std::sync::Arc;
    use std::thread;

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
}

#[tokio::test]
async fn test_end_to_end_integration() {
    let (_temp_dir, registry) = setup_test_registry().await;

    // Clear and prepare storage with macro metadata
    let storage = ToolMetadataStorage::global();
    storage.clear();

    // Insert metadata for tools that will be "discovered"
    let create_note_meta = create_mock_metadata("create_note", "Create a new markdown note");
    storage.insert("create_note".to_string(), create_note_meta);

    let search_meta = ToolMacroMetadata {
        name: "search_notes".to_string(),
        description: "Search notes by query with optional limit".to_string(),
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
    storage.insert("search_notes".to_string(), search_meta);

    // Write corresponding tool file
    let tool_source = r#"
        pub async fn create_note(title: string, content: string, folder?: string) {
            #{ success: true, path: `/notes/${title}.md` }
        }

        pub async fn search_notes(query: string, limit?: number, tags?: [string]) {
            #{ results: ["note1.md", "note2.md"] }
        }
    "#;

    write_test_tool_file(registry.tool_dir(), tool_source);

    // Step 1: Compile and discover tools
    let discovered = registry.discover_tools().await.unwrap();
    assert_eq!(discovered.len(), 2);

    // Step 2: Verify metadata extraction
    assert!(registry.has_tool("create_note"));
    assert!(registry.has_tool("search_notes"));

    // Step 3: Verify schema generation from macro metadata
    let create_tool = registry.get_tool("create_note").unwrap();
    assert_eq!(create_tool.name, "create_note");
    assert_eq!(create_tool.description, "Create a new markdown note");

    let schema = &create_tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"].as_object().unwrap().len(), 3); // title, content, folder
    assert_eq!(schema["required"].as_array().unwrap().len(), 2); // title, content only

    // Step 4: Verify array type handling
    let search_tool = registry.get_tool("search_notes").unwrap();
    let search_schema = &search_tool.input_schema;
    let search_props = search_schema["properties"].as_object().unwrap();

    assert_eq!(search_props["tags"], json!({
        "type": "array",
        "items": {"type": "string"}
    }));

    // Step 5: Verify tool execution (mock execution)
    let tool_result = registry.execute_tool("create_note", json!({
        "title": "test_note",
        "content": "test content"
    })).await;

    assert!(tool_result.is_ok());
    let result = tool_result.unwrap();
    assert!(result["success"].as_bool().unwrap());
}

#[tokio::test]
async fn test_tool_with_no_parameters() {
    let storage = ToolMetadataStorage::global();
    storage.clear();

    let metadata = ToolMacroMetadata {
        name: "get_stats".to_string(),
        description: "Get statistics about the vault".to_string(),
        parameters: vec![],
    };

    storage.insert("get_stats".to_string(), metadata);

    let schema = generate_schema(&metadata);

    // Verify empty parameter schema
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"], json!({}));
    assert_eq!(schema["required"], json!([]));
}

#[tokio::test]
async fn test_all_optional_parameters() {
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

    storage.insert("optional_tool".to_string(), metadata);

    let schema = generate_schema(&metadata);

    // Verify no required parameters
    assert_eq!(schema["required"].as_array().unwrap().len(), 0);

    // But properties still exist
    assert_eq!(schema["properties"].as_object().unwrap().len(), 2);
}

#[tokio::test]
async fn test_storage_clear_and_count() {
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