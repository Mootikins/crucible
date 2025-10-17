// Simple Test for MCP Tool Discovery
//
// This test bypasses the complex MCP API and tests the core functionality

use crucible_mcp::rune_tools::{ToolRegistry, RuneTool};
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_rune_tool_registry_directly() {
    // Test that Rune tools are loaded into registry correctly

    let temp_dir = tempdir().unwrap();
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).unwrap();

    // Create a simple test tool
    let test_tool_source = r#"
        pub fn NAME() { "test_rune_tool" }
        pub fn DESCRIPTION() { "A test Rune tool" }
        pub fn INPUT_SCHEMA() {
            #{
                type: "object",
                properties: #{
                    message: #{ type: "string", description: "Test message" }
                },
                required: ["message"]
            }
        }

        pub async fn call(args) {
            let message = args.get("message").unwrap_or("no message");
            #{
                success: true,
                echo: message
            }
        }
    "#;

    std::fs::write(tools_dir.join("test_rune_tool.rn"), test_tool_source).unwrap();

    // Set up database and registry (simplified version without obsidian)
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(
        crucible_mcp::database::EmbeddingDatabase::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create test database")
    );

    // Use basic context without obsidian
    let context = Arc::new(rune::Context::with_default_modules().unwrap());

    // Create tool registry (using basic constructor)
    let registry = ToolRegistry::new(tools_dir.clone(), context)
        .expect("Failed to create tool registry");

    // Test tool discovery
    println!("Tool registry created successfully");
    println!("Tool count: {}", registry.tool_count());

    // List available tools
    let tools = registry.list_tools();
    println!("Available tools:");
    for tool in &tools {
        println!("  - {} ({}): {}", tool.name, tool.description,
                  serde_json::to_string_pretty(&tool.input_schema).unwrap());
    }

    // Verify our test tool was loaded
    assert!(!tools.is_empty(), "Should have at least one tool");

    let test_tool = registry.get_tool("test_rune_tool")
        .expect("test_rune_tool should be loaded");

    assert_eq!(test_tool.name, "test_rune_tool");
    assert_eq!(test_tool.description, "A test Rune tool");

    println!("✅ test_rune_tool found and metadata verified");

    // Test the tool execution
    let args = serde_json::json!({
        "message": "Hello from test tool"
    });

    let result = test_tool.call(args, &registry.context).await;
    match result {
        Ok(result) => {
            println!("✅ Tool execution succeeded: {:?}", result);
            assert_eq!(result["success"], true);
            assert_eq!(result["echo"], "Hello from test tool");
        }
        Err(e) => {
            panic!("❌ Tool execution failed: {}", e);
        }
    }
}

#[test]
fn test_rune_tool_schema_conversion() {
    // Test schema conversion from Rune format to JSON Schema

    let test_tool_source = r#"
        pub fn NAME() { "schema_test" }
        pub fn DESCRIPTION() { "Test schema conversion" }
        pub fn INPUT_SCHEMA() {
            #{
                type: "object",
                properties: #{
                    title: #{ type: "string", description: "Document title" },
                    content: #{ type: "string", description: "Document content" },
                    tags: #{
                        type: "array",
                        items: #{ type: "string" },
                        description: "Tags"
                    },
                    metadata: #{
                        type: "object",
                        properties: #{
                            created: #{ type: "string" },
                            priority: #{ type: "number", minimum: 1, maximum: 10 }
                        }
                    }
                },
                required: ["title", "content"]
            }
        }

        pub async fn call(args) {
            #{ success: true }
        }
    "#;

    let context = Arc::new(rune::Context::with_default_modules().unwrap());
    let tool = RuneTool::from_source(test_tool_source, &context)
        .expect("Failed to create tool");

    // Verify schema structure
    let schema = &tool.input_schema;
    assert_eq!(schema["type"], "object");

    let properties = schema["properties"].as_object().unwrap();
    assert!(properties.contains_key("title"));
    assert!(properties.contains_key("content"));
    assert!(properties.contains_key("tags"));
    assert!(properties.contains_key("metadata"));

    // Test specific property types
    assert_eq!(properties["title"]["type"], "string");
    assert_eq!(properties["content"]["type"], "string");
    assert_eq!(properties["tags"]["type"], "array");
    assert_eq!(properties["tags"]["items"]["type"], "string");

    // Test nested object schema
    let metadata_props = properties["metadata"]["properties"].as_object().unwrap();
    assert_eq!(metadata_props["created"]["type"], "string");
    assert_eq!(metadata_props["priority"]["type"], "number");
    assert_eq!(metadata_props["priority"]["minimum"], 1);
    assert_eq!(metadata_props["priority"]["maximum"], 10);

    println!("✅ Schema conversion test passed");
    println!("Schema: {}", serde_json::to_string_pretty(&tool.input_schema).unwrap());
}

#[test]
fn test_multiple_rune_tools_discovery() {
    // Test discovery of multiple tools in the same directory

    let temp_dir = tempdir().unwrap();
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tools_dir).unwrap();

    // Create first tool
    let tool1_source = r#"
        pub fn NAME() { "tool_one" }
        pub fn DESCRIPTION() { "First test tool" }
        pub fn INPUT_SCHEMA() {
            #{ type: "object", properties: #{ name: #{ type: "string" } } }
        }

        pub async fn call(args) {
            #{ success: true, tool: "one", name: args.get("name").unwrap_or("") }
        }
    "#;

    // Create second tool
    let tool2_source = r#"
        pub fn NAME() { "tool_two" }
        pub fn DESCRIPTION() { "Second test tool" }
        pub fn INPUT_SCHEMA() {
            #{ type: "object", properties: #{ count: #{ type: "number" } } }
        }

        pub async fn call(args) {
            #{ success: true, tool: "two", count: args.get("count").unwrap_or(0) }
        }
    "#;

    std::fs::write(tools_dir.join("tool_one.rn"), tool1_source).unwrap();
    std::fs::write(tools_dir.join("tool_two.rn"), tool2_source).unwrap();

    let context = Arc::new(rune::Context::with_default_modules().unwrap());
    let registry = ToolRegistry::new(tools_dir, context)
        .expect("Failed to create tool registry");

    println!("✅ Multiple tools registry created");
    println!("Tool count: {}", registry.tool_count());
    println!("Tool names: {:?}", registry.tool_names());

    // Verify both tools are loaded
    assert_eq!(registry.tool_count(), 2, "Should have 2 tools");
    assert!(registry.has_tool("tool_one"));
    assert!(registry.has_tool("tool_two"));

    // Verify tool metadata
    let tool1 = registry.get_tool("tool_one").unwrap();
    let tool2 = registry.get_tool("tool_two").unwrap();

    assert_eq!(tool1.name, "tool_one");
    assert_eq!(tool2.name, "tool_two");

    println!("✅ Multiple tools discovery test passed");
}