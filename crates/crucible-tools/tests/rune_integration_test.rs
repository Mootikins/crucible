//! Integration test for Rune tools in crucible-tools

use crucible_tools::{RuneService, RuneServiceConfig, ToolService};
use std::collections::HashMap;
use tempfile::TempDir;
use std::fs;

#[tokio::test]
async fn test_rune_service_creation() {
    let config = RuneServiceConfig::default();
    let service = RuneService::new(config).await;
    assert!(service.is_ok());

    let service = service.unwrap();
    let tools = service.list_tools().await.unwrap();
    // Should have no tools initially since we didn't specify any directories
    assert_eq!(tools.len(), 0);
}

#[tokio::test]
async fn test_rune_tool_discovery() {
    // Create a temporary directory with a test Rune tool
    let temp_dir = TempDir::new().unwrap();
    let tool_path = temp_dir.path().join("test_tool.rn");

    let tool_source = r#"
        pub fn NAME() { "test_tool" }
        pub fn DESCRIPTION() { "A test tool for integration testing" }
        pub fn INPUT_SCHEMA() {
            #{
                type: "object",
                properties: {
                    message: { type: "string" }
                },
                required: ["message"]
            }
        }
        pub async fn call(args) {
            #{
                success: true,
                echo: args.message,
                timestamp: time::now()
            }
        }
    "#;

    fs::write(&tool_path, tool_source).unwrap();

    // Create service with the temp directory
    let mut config = RuneServiceConfig::default();
    config.discovery.tool_directories.push(temp_dir.path().to_path_buf());

    let service = RuneService::new(config).await.unwrap();

    // Discover tools
    let discovered_count = service.discover_tools_from_directory(temp_dir.path()).await.unwrap();
    assert_eq!(discovered_count, 1);

    // List tools
    let tools = service.list_tools().await.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "test_tool");
    assert_eq!(tools[0].description, "A test tool for integration testing");
}

#[tokio::test]
async fn test_rune_tool_execution() {
    // Create a temporary directory with a test Rune tool
    let temp_dir = TempDir::new().unwrap();
    let tool_path = temp_dir.path().join("echo_tool.rn");

    let tool_source = r#"
        pub fn NAME() { "echo_tool" }
        pub fn DESCRIPTION() { "Echo tool that returns the input message" }
        pub fn INPUT_SCHEMA() {
            #{
                type: "object",
                properties: {
                    message: { type: "string" }
                },
                required: ["message"]
            }
        }
        pub async fn call(args) {
            #{
                success: true,
                echo: args.message,
                tool_name: "echo_tool"
            }
        }
    "#;

    fs::write(&tool_path, tool_source).unwrap();

    // Create service with the temp directory
    let mut config = RuneServiceConfig::default();
    config.discovery.tool_directories.push(temp_dir.path().to_path_buf());

    let service = RuneService::new(config).await.unwrap();

    // Execute the tool
    let mut context_data = HashMap::new();
    context_data.insert("execution_id".to_string(), "test-123".to_string());

    let request = crucible_tools::ToolExecutionRequest {
        tool_name: "echo_tool".to_string(),
        parameters: serde_json::json!({
            "message": "Hello from Rune test!"
        }),
        context: crucible_tools::ToolExecutionContext::default(),
    };

    let result = service.execute_tool(request).await.unwrap();
    assert!(result.success);

    if let Some(output) = &result.result {
        assert_eq!(output["success"], true);
        assert_eq!(output["echo"], "Hello from Rune test!");
        assert_eq!(output["tool_name"], "echo_tool");
    }
}

#[tokio::test]
async fn test_system_info() {
    let config = RuneServiceConfig::default();
    let service = RuneService::new(config).await.unwrap();

    let info = service.system_info();
    assert!(!info.version.is_empty());
    assert!(!info.rune_version.is_empty());
    assert_eq!(info.supported_extensions, vec!["rn".to_string(), "rune".to_string()]);
}