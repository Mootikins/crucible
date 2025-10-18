//! Manual test for #[tool] macro functionality

use std::sync::Arc;
use std::collections::HashMap;
use tempfile::TempDir;
use serde_json::{json, Value};

// Import our modules
use crucible_mcp::rune_tools::{
    ToolRegistry, ToolMetadataStorage, ToolMacroMetadata, ParameterMetadata, TypeSpec,
    build_crucible_module
};
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::obsidian_client::ObsidianClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing #[tool] macro functionality...\n");

    // Test 1: Metadata Storage
    println!("1️⃣ Testing metadata storage...");
    let storage = ToolMetadataStorage::global();

    let test_metadata = ToolMacroMetadata {
        name: "test_tool".to_string(),
        description: "A test tool".to_string(),
        parameters: vec![
            ParameterMetadata {
                name: "title".to_string(),
                type_spec: TypeSpec::String,
                is_optional: false,
            },
            ParameterMetadata {
                name: "limit".to_string(),
                type_spec: TypeSpec::Number,
                is_optional: true,
            },
        ],
    };

    storage.insert("test_tool".to_string(), test_metadata.clone());

    let retrieved = storage.get("test_tool").unwrap();
    assert_eq!(retrieved.name, "test_tool");
    assert_eq!(retrieved.description, "A test tool");
    assert_eq!(retrieved.parameters.len(), 2);
    println!("✅ Metadata storage works!\n");

    // Test 2: Schema Generation
    println!("2️⃣ Testing schema generation...");
    use crucible_mcp::rune_tools::schema_generator::generate_schema;

    let schema = generate_schema(&test_metadata);

    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["title"]["type"], "string");
    assert!(schema["properties"]["limit"]["type"], "number");

    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert_eq!(required[0], "title");

    println!("✅ Schema generation works!");
    println!("📋 Generated schema: {}", serde_json::to_string_pretty(&schema)?);
    println!();

    // Test 3: Tool Registry Integration
    println!("3️⃣ Testing tool registry integration...");

    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    let db = Arc::new(EmbeddingDatabase::new_in_memory()?);
    let obsidian = Arc::new(ObsidianClient::new_test());

    let mut registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db, obsidian)?;

    // Create a test tool file
    let tool_source = r#"
        #[tool(desc: "Create a test note")]
        pub async fn create_test_note(title: string, content: string, limit?: number) {
            #{
                success: true,
                title: title,
                content: content,
                limit: limit ?? 10
            }
        }
    "#;

    std::fs::write(tool_dir.join("test.rn"), tool_source)?;

    // Perform discovery
    let discoveries = registry.discovery().discover().await?;

    println!("✅ Discovered {} tools", discoveries.len());

    for discovery in &discoveries {
        println!("  📦 {}: {}", discovery.name, discovery.description.as_ref().unwrap_or(&"(no description)".to_string()));

        let schema = &discovery.input_schema;
        if let Some(properties) = schema["properties"].as_object() {
            for (name, prop) in properties {
                let prop_type = prop["type"].as_str().unwrap_or("unknown");
                let optional = if let Some(required) = schema["required"].as_array() {
                    !required.contains(&json!(name))
                } else {
                    true
                };
                let marker = if optional { "?" } else { "" };
                println!("    - {}{}: {}", name, marker, prop_type);
            }
        }
    }

    // Test 4: Check if our macro metadata was used
    println!("\n4️⃣ Checking macro metadata usage...");

    let create_note_tool = discoveries.iter()
        .find(|d| d.name == "create_test_note")
        .expect("Should have discovered create_test_note");

    println!("✅ Tool found: {}", create_note_tool.name);

    // Check if schema has expected structure
    let schema = &create_note_tool.input_schema;
    assert_eq!(schema["type"], "object");

    let required_params = schema["required"].as_array().unwrap();
    assert!(required_params.contains(&json!("title")));
    assert!(required_params.contains(&json!("content")));
    assert!(!required_params.contains(&json!("limit"))); // Should be optional

    println!("✅ Schema has correct required/optional parameters");
    println!("✅ Macro metadata was successfully used!");

    println!("\n🎉 All tests passed! #[tool] macro system is working correctly!");

    Ok(())
}