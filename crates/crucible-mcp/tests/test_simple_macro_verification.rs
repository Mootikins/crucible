//! Simple verification test for #[tool] macro functionality

use super::*;
use crate::rune_tools::{ToolRegistry, build_crucible_module};
use crate::database::EmbeddingDatabase;
use crate::obsidian_client::ObsidianClient;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_simple_tool_macro_verification() {
    // Create temporary directory for test tools
    let temp_dir = TempDir::new().unwrap();
    let tool_dir = temp_dir.path().to_path_buf();

    // Create test database and obsidian client
    let db = Arc::new(EmbeddingDatabase::new_in_memory().unwrap());
    let obsidian = Arc::new(ObsidianClient::new_test());

    // Create registry with stdlib
    let mut registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db, obsidian).unwrap();

    // Write a simple test tool with #[tool] macro
    let tool_source = r#"
        #[tool(desc: "Create a test note")]
        pub async fn create_test_note(title: string, content: string) {
            #{
                success: true,
                title: title,
                content: content,
                message: `Created note: ${title}`
            }
        }
    "#;

    std::fs::write(tool_dir.join("test_tool.rn"), tool_source).unwrap();

    // Perform discovery
    let discoveries = registry.discovery().discover().await.unwrap();

    // Verify discovery worked
    assert!(!discoveries.is_empty(), "Should have discovered tools");

    // Look for our test tool
    let test_tool = discoveries.iter()
        .find(|d| d.name == "create_test_note")
        .expect("Should have discovered create_test_note");

    // Verify metadata
    assert_eq!(test_tool.name, "create_test_note");
    assert!(test_tool.description.is_some(), "Should have description");

    // Verify schema was generated
    let schema = &test_tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"].is_object());
    assert!(schema["properties"]["title"].is_object());
    assert!(schema["properties"]["content"].is_object());

    // Verify required parameters
    if let Some(required) = schema["required"].as_array() {
        assert!(required.contains(&serde_json::Value::String("title".to_string())));
        assert!(required.contains(&serde_json::Value::String("content".to_string())));
        assert_eq!(required.len(), 2);
    }

    println!("✅ Tool discovered: {}", test_tool.name);
    println!("✅ Description: {:?}", test_tool.description);
    println!("✅ Schema: {}", serde_json::to_string_pretty(schema).unwrap());
}

#[tokio::test]
async fn test_macro_vs_fallback_behavior() {
    let temp_dir = TempDir::new().unwrap();
    let tool_dir = temp_dir.path().to_path_buf();

    let db = Arc::new(EmbeddingDatabase::new_in_memory().unwrap());
    let obsidian = Arc::new(ObsidianClient::new_test());

    let mut registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db, obsidian).unwrap();

    // Create a tool without #[tool] macro (fallback)
    let fallback_tool_source = r#"
        pub fn NAME() { "fallback_tool" }
        pub fn DESCRIPTION() { "Tool without macro" }
        pub fn INPUT_SCHEMA() {
            #{
                type: "object",
                properties: #{
                    message: #{ type: "string" }
                },
                required: ["message"]
            }
        }
        pub async fn call(args) {
            #{ success: true, message: args.message }
        }
    "#;

    std::fs::write(tool_dir.join("fallback_tool.rn"), fallback_tool_source).unwrap();

    // Perform discovery
    let discoveries = registry.discovery().discover().await.unwrap();

    // Both tools should be discovered
    assert_eq!(discoveries.len(), 1, "Should have discovered fallback tool");

    let fallback_tool = discoveries.first().unwrap();
    assert_eq!(fallback_tool.name, "fallback_tool");
    assert_eq!(fallback_tool.description, Some("Tool without macro".to_string()));

    println!("✅ Fallback tool discovered: {}", fallback_tool.name);
}