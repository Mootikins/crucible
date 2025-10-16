// Test for Rune Tool Arguments Processing Bug
//
// This test reproduces the "Tool arguments must be a JSON object" error.
// It should initially fail and pass after fixing the argument parsing issue.

use crucible_mcp::rune_tools::{ToolRegistry, RuneTool};
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_rune_tool_with_json_object_args() {
    // Test that Rune tools can receive JSON object arguments correctly

    let temp_dir = tempdir().unwrap();
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir(&tools_dir).unwrap();

    // Set up database
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(
        crucible_mcp::database::EmbeddingDatabase::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create test database")
    );

    // Create mock obsidian client
    let obsidian = match crucible_mcp::obsidian_client::ObsidianClient::new() {
        Ok(client) => Arc::new(client),
        Err(_) => {
            println!("⚠️  Skipping test - Obsidian client not available");
            return;
        }
    };

    // Create a simple test tool that expects JSON object arguments
    let test_tool_source = r#"
        pub fn NAME() { "echo_tool" }
        pub fn DESCRIPTION() { "Echoes back the arguments" }
        pub fn INPUT_SCHEMA() {
            #{
                type: "object",
                properties: #{
                    message: #{ type: "string" },
                    value: #{ type: "number" }
                },
                required: ["message"]
            }
        }

        pub async fn call(args) {
            // This should receive args as a JSON object
            let message = args.get("message").unwrap_or("no message");
            let value = args.get("value").unwrap_or(0);

            #{
                success: true,
                received_message: message,
                received_value: value,
                args_type: "object"
            }
        }
    "#;

    std::fs::write(tools_dir.join("echo_tool.rn"), test_tool_source).unwrap();

    // Create tool registry
    let registry = ToolRegistry::new_with_stdlib(tools_dir, database, obsidian)
        .expect("Failed to create tool registry");

    // Verify tool is loaded
    assert!(registry.has_tool("echo_tool"), "echo_tool should be loaded");

    // Test calling the tool with JSON object arguments
    let tool = registry.get_tool("echo_tool").unwrap();
    let context = registry.context.clone();

    // Test with proper JSON object
    let args = serde_json::json!({
        "message": "Hello World",
        "value": 42
    });

    let result = tool.call(args.clone(), &context).await;

    match result {
        Ok(result) => {
            println!("✅ Tool call succeeded: {:?}", result);

            // Verify the result structure
            assert_eq!(result["success"], true);
            assert_eq!(result["received_message"], "Hello World");
            assert_eq!(result["received_value"], 42);
            assert_eq!(result["args_type"], "object");
        }
        Err(e) => {
            if e.to_string().contains("Tool arguments must be a JSON object") {
                panic!(
                    "❌ Bug confirmed: Tool arguments must be a JSON object error.\n\
                     This indicates the argument validation is failing despite receiving a valid JSON object.\n\
                     Error: {}\n\
                     Args sent: {:?}",
                    e, args
                );
            } else {
                panic!(
                    "❌ Unexpected error calling Rune tool: {}\n\
                     Args sent: {:?}",
                    e, args
                );
            }
        }
    }
}

#[tokio::test]
async fn test_rune_tool_with_empty_object_args() {
    // Test that Rune tools can receive empty JSON object arguments

    let temp_dir = tempdir().unwrap();
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir(&tools_dir).unwrap();

    // Set up database
    let db_path = temp_dir.path().join("test.db");
    let database = Arc::new(
        crucible_mcp::database::EmbeddingDatabase::new(db_path.to_str().unwrap())
            .await
            .expect("Failed to create test database")
    );

    // Create mock obsidian client
    let obsidian = match crucible_mcp::obsidian_client::ObsidianClient::new() {
        Ok(client) => Arc::new(client),
        Err(_) => {
            println!("⚠️  Skipping test - Obsidian client not available");
            return;
        }
    };

    // Create a simple test tool that works with empty arguments
    let test_tool_source = r#"
        pub fn NAME() { "simple_tool" }
        pub fn DESCRIPTION() { "Simple tool with no required args" }
        pub fn INPUT_SCHEMA() {
            #{
                type: "object",
                properties: #{},
                required: []
            }
        }

        pub async fn call(args) {
            #{
                success: true,
                message: "Simple tool executed",
                args_received: !args.is_empty()
            }
        }
    "#;

    std::fs::write(tools_dir.join("simple_tool.rn"), test_tool_source).unwrap();

    // Create tool registry
    let registry = ToolRegistry::new_with_stdlib(tools_dir, database, obsidian)
        .expect("Failed to create tool registry");

    // Verify tool is loaded
    assert!(registry.has_tool("simple_tool"), "simple_tool should be loaded");

    // Test calling the tool with empty JSON object
    let tool = registry.get_tool("simple_tool").unwrap();
    let context = registry.context.clone();

    let args = serde_json::json!({});

    let result = tool.call(args.clone(), &context).await;

    match result {
        Ok(result) => {
            println!("✅ Empty args test succeeded: {:?}", result);

            // Verify the result structure
            assert_eq!(result["success"], true);
            assert_eq!(result["args_received"], false);
        }
        Err(e) => {
            if e.to_string().contains("Tool arguments must be a JSON object") {
                panic!(
                    "❌ Bug confirmed: Empty JSON object arguments rejected.\n\
                     Error: {}\n\
                     Args sent: {:?} (type: {})",
                    e, args,
                    if args.is_object() { "object" } else { "not object" }
                );
            } else {
                panic!(
                    "❌ Unexpected error calling Rune tool: {}",
                    e
                );
            }
        }
    }
}

#[test]
fn test_rune_tool_validation_logic() {
    // Test the argument validation logic directly

    let temp_dir = tempdir().unwrap();
    let tools_dir = temp_dir.path().join("tools");
    std::fs::create_dir(&tools_dir).unwrap();

    // Create a simple test tool
    let test_tool_source = r#"
        pub fn NAME() { "validation_test_tool" }
        pub fn DESCRIPTION() { "Tool for testing argument validation" }
        pub fn INPUT_SCHEMA() {
            #{ type: "object", properties: #{} }
        }

        pub async fn call(args) {
            #{ success: true }
        }
    "#;

    std::fs::write(tools_dir.join("validation_test_tool.rn"), test_tool_source).unwrap();

    // Create context and tool directly (without registry)
    let context = Arc::new(rune::Context::with_default_modules().unwrap());
    let tool = RuneTool::from_source(test_tool_source, &context)
        .expect("Failed to create tool");

    // Test validation with different argument types
    let valid_object = serde_json::json!({});
    let invalid_string = serde_json::json!("not an object");
    let invalid_number = serde_json::json!(42);

    // Valid case should pass
    assert!(tool.validate_input(&valid_object).is_ok(),
           "Valid JSON object should pass validation");

    // Invalid cases should fail
    assert!(tool.validate_input(&invalid_string).is_err(),
           "String should fail validation");
    assert!(tool.validate_input(&invalid_number).is_err(),
           "Number should fail validation");

    println!("✅ Argument validation logic works correctly");
}