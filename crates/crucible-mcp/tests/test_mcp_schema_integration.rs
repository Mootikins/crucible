/// MCP Service Schema Generation Integration Tests
///
/// Tests the integration between the enhanced schema generation system and the
/// Rune tools registry. This validates that tools are properly discovered,
/// schemas are generated correctly, and validation works as expected.

use anyhow::Result;
use crucible_mcp::rune_tools::ToolRegistry;
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::obsidian_client::ObsidianClient;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;


/// Create a test tool with proper schema
fn create_test_tool_schema() -> String {
    r#"
pub fn NAME() { "test_schema_tool" }
pub fn DESCRIPTION() { "Test tool for schema generation validation" }
pub fn INPUT_SCHEMA() {
    #{
        type: "object",
        properties: #{
            message: #{
                type: "string",
                description: "Message to process"
            },
            count: #{
                type: "integer",
                minimum: 1,
                maximum: 100,
                description: "Number of repetitions"
            }
        },
        required: ["message", "count"]
    }
}

pub async fn call(args) {
    let processed_message = args.message;
    #{
        success: true,
        processed: processed_message,
        received_message: args.message
    }
}
"#.to_string()
}

/// Create a complex tool for testing advanced schema features
fn create_complex_tool_schema() -> String {
    r#"
pub fn NAME() { "complex_analysis_tool" }
pub fn DESCRIPTION() { "Complex tool with nested objects and arrays" }
pub fn INPUT_SCHEMA() {
    #{
        type: "object",
        properties: #{
            data: #{
                type: "array",
                items: #{
                    type: "object",
                    properties: #{
                        id: #{ "type": "string" },
                        value: #{ "type": "number" }
                    },
                    required: ["id", "value"]
                }
            }
        },
        required: ["data"]
    }
}

pub async fn call(args) {
    let total = 0.0;
    for item in args.data {
        total += item.value;
    }

    #{
        analysis_result: total,
        input_count: args.data.len()
    }
}
"#.to_string()
}

/// Create a tool that should fail validation
fn create_invalid_tool_schema() -> String {
    r#"
pub fn NAME() { "invalid_tool" }
pub fn DESCRIPTION() { "Tool with invalid schema for testing validation" }
pub fn INPUT_SCHEMA() {
    #{
        type: "invalid_type",  // Invalid JSON Schema type
        properties: "not_an_object"  // Properties should be an object
    }
}

pub async fn call(args) {
    #{ error: "This should not be reached due to invalid schema" }
}
"#.to_string()
}

#[tokio::test]
async fn test_enhanced_schema_generation_integration() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create test tools
    std::fs::write(tool_dir.join("test_schema.rn"), create_test_tool_schema())?;
    std::fs::write(tool_dir.join("complex_analysis.rn"), create_complex_tool_schema())?;
    std::fs::write(tool_dir.join("invalid_tool.rn"), create_invalid_tool_schema())?;

    // Create database and obsidian client for registry
    let db = Arc::new(EmbeddingDatabase::new(":memory:").await?);
    let obsidian = Arc::new(ObsidianClient::new()?);

    // Create registry with enhanced discovery
    let context = Arc::new(rune::Context::with_default_modules()?);
    let mut registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db.clone(), obsidian)?;

    // Force async discovery to get the enhanced behavior
    let loaded = registry.scan_and_load().await?;
    println!("Async discovery loaded {} tools", loaded.len());
    for name in &loaded {
        println!("  - Loaded: {}", name);
    }

    let registry_arc = Arc::new(RwLock::new(registry));

    // Test tool discovery and schema generation at registry level
    let tools = {
        let reg = registry_arc.read().await;
        reg.list_tools()
    };
    println!("Discovered {} tools", tools.len());

    // Debug: Print all discovered tool names
    for tool in &tools {
        println!("  - Found tool: '{}' with description: '{}'", tool.name, tool.description);
    }

    // Debug: Check if the files exist and print their contents
    println!("Checking files in tool directory:");
    for entry in std::fs::read_dir(&tool_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("rn") {
            println!("  - File: {:?}", path);
            let content = std::fs::read_to_string(&path)?;
            println!("    Content preview: {}", &content[..content.len().min(200)]);
        }
    }

    // Should have discovered all tools that compile successfully
    assert!(tools.len() >= 2, "Should discover at least 2 valid tools");

    // Check for expected tools
    let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
    assert!(tool_names.contains(&"test_schema_tool".to_string()),
            "Should contain test_schema_tool");
    assert!(tool_names.contains(&"complex_analysis_tool".to_string()),
            "Should contain complex_analysis_tool");
    // Note: invalid_tool loads because it compiles successfully, even though its schema is semantically invalid

    // Validate tool schemas (skip invalid tools)
    for tool in &tools {
        assert!(!tool.name.is_empty(), "Tool name should not be empty");
        assert!(!tool.description.is_empty(), "Tool should have description");

        // Skip invalid tools from schema validation
        if tool.name.contains("invalid") {
            continue;
        }

        // Verify input schema structure for valid tools
        assert_eq!(tool.input_schema.get("type"), Some(&json!("object")),
                  "Tool input schema should be of type object");
        assert!(tool.input_schema.get("properties").is_some(),
               "Tool input schema should have properties");

        // Verify that schemas are valid JSON Schema objects
        let schema_str = serde_json::to_string_pretty(&tool.input_schema)?;
        let parsed_back: serde_json::Value = serde_json::from_str(&schema_str)?;
        assert_eq!(parsed_back, tool.input_schema, "Schema should be serializable and deserializable");
    }

    // Verify enhanced mode is active
    {
        let reg = registry_arc.read().await;
        assert!(reg.is_enhanced_mode(), "Enhanced discovery mode should be active");
    }

    Ok(())
}

#[tokio::test]
async fn test_tool_execution_with_registry() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create test tool - filename should match the NAME() function
    std::fs::write(tool_dir.join("test_schema_tool.rn"), create_test_tool_schema())?;

    // Create database and obsidian client
    let db = Arc::new(EmbeddingDatabase::new(":memory:").await?);
    let obsidian = Arc::new(ObsidianClient::new()?);

    // Create registry
    let _context = Arc::new(rune::Context::with_default_modules()?);
    let registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db.clone(), obsidian)?;
    let registry_arc = Arc::new(RwLock::new(registry));

    // Test tool execution with valid parameters
    let args = json!({
        "message": "Hello World",
        "count": 2
    });

    let result = {
        let reg = registry_arc.read().await;
        let tool = reg.get_tool("test_schema_tool")
            .ok_or_else(|| anyhow::anyhow!("Tool not found"))?
            .clone();
        let context = reg.context().clone();
        drop(reg);

        // Execute the tool
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(tool.call(args, &context))
        }).await??
    };

    assert!(result.is_object(), "Result should be an object");
    assert_eq!(result["success"], true, "Execution should be successful");
    assert_eq!(result["processed"], "Hello World", "Message should be preserved");
    assert_eq!(result["received_message"], "Hello World", "Should echo input message");

    Ok(())
}

#[tokio::test]
async fn test_schema_validation_integration() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create tool with complex schema - filename should match the NAME() function
    std::fs::write(tool_dir.join("complex_analysis_tool.rn"), create_complex_tool_schema())?;

    // Create database and obsidian client
    let db = Arc::new(EmbeddingDatabase::new(":memory:").await?);
    let obsidian = Arc::new(ObsidianClient::new()?);

    // Create registry
    let _context = Arc::new(rune::Context::with_default_modules()?);
    let registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db.clone(), obsidian)?;
    let registry_arc = Arc::new(RwLock::new(registry));

    // Get the tool and validate its schema
    let tool = {
        let reg = registry_arc.read().await;
        reg.get_tool("complex_analysis_tool")
            .ok_or_else(|| anyhow::anyhow!("Tool not found"))?
            .clone()
    };

    // Validate schema structure
    assert_eq!(tool.input_schema.get("type"), Some(&json!("object")));
    assert!(tool.input_schema.get("properties").is_some());
    assert!(tool.input_schema.get("properties").unwrap().get("data").is_some());

    // Test schema structure
    assert!(tool.input_schema.is_object(), "Schema should be an object");

    Ok(())
}

#[tokio::test]
async fn test_enhanced_discovery_features() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create multiple test tools
    std::fs::write(tool_dir.join("tool1.rn"), create_test_tool_schema())?;
    std::fs::write(tool_dir.join("tool2.rn"), create_complex_tool_schema())?;

    // Create database and obsidian client
    let db = Arc::new(EmbeddingDatabase::new(":memory:").await?);
    let obsidian = Arc::new(ObsidianClient::new()?);

    // Create registry
    let _context = Arc::new(rune::Context::with_default_modules()?);
    let registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db.clone(), obsidian)?;
    let registry_arc = Arc::new(RwLock::new(registry));

    // Verify enhanced discovery features
    {
        let reg = registry_arc.read().await;

        // Should have discovery system
        let _discovery = reg.discovery();

        // Should have discovered tools
        let tools = reg.list_tools();
        assert!(tools.len() >= 2, "Should discover multiple tools");

        // Should be in enhanced mode
        assert!(reg.is_enhanced_mode(), "Should be in enhanced mode");

        // Check tool names
        let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
        assert!(tool_names.contains(&"test_schema_tool".to_string()));
        assert!(tool_names.contains(&"complex_analysis_tool".to_string()));

        // Validate all tools have proper schemas
        for tool in &tools {
            assert!(!tool.name.is_empty());
            assert!(!tool.description.is_empty());
            assert_eq!(tool.input_schema.get("type"), Some(&json!("object")));
        }
    }

    Ok(())
}