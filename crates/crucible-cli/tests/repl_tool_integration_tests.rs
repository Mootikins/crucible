//! Integration tests for REPL tool system
//!
//! This module tests the unified tool system in the REPL, ensuring that
//! both system tools (crucible-tools) and rune tools are properly discovered,
//! listed, and executed through the REPL interface.

use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;
use crucible_cli::config::CliConfig;

/// Test context for tool system tests
struct ToolTestContext {
    temp_dir: TempDir,
}

impl ToolTestContext {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        Ok(Self { temp_dir })
    }
}

/// Task 1: Baseline test - crucible-tools functionality verification
///
/// This test verifies that crucible-tools work independently and provides
/// a baseline for what we expect to see in the REPL tool system.
#[tokio::test]
async fn test_crucible_tools_baseline() -> Result<()> {
    // Initialize crucible-tools
    crucible_tools::init();

    // Load all tools
    crucible_tools::load_all_tools().await?;

    // List registered tools
    let tools = crucible_tools::list_registered_tools().await;

    // Should have multiple tools
    assert!(!tools.is_empty(), "crucible-tools should register tools, got empty list");

    // Should contain expected core tools
    let expected_tools = vec![
        "search_documents",
        "get_vault_stats",
        "system_info",
        "list_files"
    ];

    for expected_tool in expected_tools {
        assert!(tools.contains(&expected_tool.to_string()),
               "crucible-tools should contain '{}'. Available tools: {:?}",
               expected_tool, tools);
    }

    // Should have at least 20+ tools (from the 4 categories)
    assert!(tools.len() >= 20,
           "Expected at least 20 tools, got {}. Available: {:?}",
           tools.len(), tools);

    println!("✅ crucible-tools baseline verified: {} tools available", tools.len());
    println!("📋 Available tools: {:?}", tools);

    Ok(())
}

/// Task 3: Failing test - ToolGroup registration and basic functionality
///
/// This test verifies that the ToolGroup trait interface works correctly.
/// It should initially FAIL because we haven't implemented any concrete ToolGroup
/// implementations yet, but the trait should be properly defined.
#[tokio::test]
async fn test_tool_group_registration_basic() -> Result<()> {
    use crucible_cli::commands::repl::tools::{ToolGroupRegistry, ToolGroup, ToolGroupError};

    // Create a registry
    let mut registry = ToolGroupRegistry::new();

    // Initially should have no groups and no tools
    assert!(registry.list_groups().is_empty(), "New registry should have no groups");
    assert!(registry.list_all_tools().is_empty(), "New registry should have no tools");

    // Mock tool group for testing the interface
    #[derive(Debug)]
    struct MockToolGroup {
        name: String,
        initialized: bool,
        tools: Vec<String>,
    }

    #[async_trait::async_trait]
    impl ToolGroup for MockToolGroup {
        fn group_name(&self) -> &str {
            &self.name
        }

        fn group_description(&self) -> &str {
            "Mock tool group for testing"
        }

        async fn discover_tools(&mut self) -> crucible_cli::commands::repl::tools::ToolGroupResult<Vec<String>> {
            Ok(self.tools.clone())
        }

        fn list_tools(&self) -> Vec<String> {
            self.tools.clone()
        }

        async fn get_tool_schema(&self, tool_name: &str) -> crucible_cli::commands::repl::tools::ToolGroupResult<Option<crucible_cli::commands::repl::tools::ToolSchema>> {
            if self.tools.contains(&tool_name.to_string()) {
                Ok(Some(crucible_cli::commands::repl::tools::ToolSchema {
                    name: tool_name.to_string(),
                    description: format!("Mock tool: {}", tool_name),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: None,
                }))
            } else {
                Ok(None)
            }
        }

        async fn execute_tool(&self, tool_name: &str, _args: &[String]) -> crucible_cli::commands::repl::tools::ToolGroupResult<crucible_cli::commands::repl::tools::ToolResult> {
            if self.tools.contains(&tool_name.to_string()) {
                Ok(crucible_cli::commands::repl::tools::ToolResult::success(format!("Mock execution of {}", tool_name)))
            } else {
                Err(crucible_cli::commands::repl::tools::ToolGroupError::ToolNotFound(tool_name.to_string()))
            }
        }

        fn is_initialized(&self) -> bool {
            self.initialized
        }

        async fn initialize(&mut self) -> crucible_cli::commands::repl::tools::ToolGroupResult<()> {
            self.initialized = true;
            Ok(())
        }
    }

    // Create and register a mock tool group
    let mock_group = MockToolGroup {
        name: "mock".to_string(),
        initialized: false,
        tools: vec!["test_tool1".to_string(), "test_tool2".to_string()],
    };

    let group_box: Box<dyn ToolGroup> = Box::new(mock_group);

    // Register the group
    registry.register_group(group_box).await
        .expect("Should be able to register mock tool group");

    // Should now have one group
    assert_eq!(registry.list_groups().len(), 1, "Should have one registered group");
    assert!(registry.list_groups().contains(&"mock".to_string()), "Should contain mock group");

    // Should have tools from the mock group
    let all_tools = registry.list_all_tools();
    assert_eq!(all_tools.len(), 2, "Should have two tools from mock group");
    assert!(all_tools.contains(&"test_tool1".to_string()), "Should contain test_tool1");
    assert!(all_tools.contains(&"test_tool2".to_string()), "Should contain test_tool2");

    // Should be able to execute tools
    let result = registry.execute_tool("test_tool1", &["arg1".to_string()]).await
        .expect("Should be able to execute test_tool1");
    assert!(result.is_success(), "Tool execution should succeed");
    assert!(result.output.contains("test_tool1"), "Output should mention tool name");

    // Should get proper group assignment
    let group_name = registry.get_tool_group("test_tool1")
        .expect("Should find group for test_tool1");
    assert_eq!(group_name, "mock", "test_tool1 should belong to mock group");

    // Should handle missing tools gracefully
    let missing_result = registry.execute_tool("nonexistent_tool", &[]).await;
    assert!(missing_result.is_err(), "Should fail when trying to execute nonexistent tool");
    match missing_result.unwrap_err() {
        crucible_cli::commands::repl::tools::ToolGroupError::ToolNotFound(_) => {
            // Expected error type
        }
        other => panic!("Expected ToolNotFound error, got: {:?}", other),
    }

    println!("✅ ToolGroup trait interface working correctly");
    Ok(())
}

/// Task 4: Test SystemToolGroup implementation
///
/// This test verifies that the SystemToolGroup properly wraps crucible-tools
/// and provides them through the ToolGroup interface.
#[tokio::test]
async fn test_system_tool_group_basic() -> Result<()> {
    use crucible_cli::commands::repl::tools::{SystemToolGroup, ToolGroup, ToolGroupRegistry, ParameterConverter};

    // Create a SystemToolGroup
    let mut system_group = SystemToolGroup::new();

    // Should not be initialized initially
    assert!(!system_group.is_initialized(), "New SystemToolGroup should not be initialized");
    assert_eq!(system_group.group_name(), "system", "Group name should be 'system'");
    assert!(!system_group.group_description().is_empty(), "Should have description");

    // Initialize the group
    system_group.initialize().await
        .expect("Should be able to initialize SystemToolGroup");

    // Should now be initialized
    assert!(system_group.is_initialized(), "SystemToolGroup should be initialized after initialize()");

    // Should have tools available
    let tools = system_group.list_tools();
    assert!(!tools.is_empty(), "SystemToolGroup should have tools after initialization");
    println!("✅ SystemToolGroup initialized with {} tools", tools.len());

    // Should contain expected tools
    let expected_tools = vec![
        "search_documents",
        "get_vault_stats",
        "system_info",
        "list_files"
    ];

    for expected_tool in expected_tools {
        assert!(tools.contains(&expected_tool.to_string()),
               "SystemToolGroup should contain '{}'. Available: {:?}", expected_tool, tools);
    }

    // Test parameter conversion for different tools
    let no_args_result = system_group.convert_args_to_params("system_info", &[]);
    assert!(no_args_result.is_ok(), "system_info should accept no arguments");

    let single_arg_result = system_group.convert_args_to_params("list_files", &["/tmp".to_string()]);
    assert!(single_arg_result.is_ok(), "list_files should accept single path argument");

    let multi_arg_result = system_group.convert_args_to_params("search_by_tags", &["tag1".to_string(), "tag2".to_string()]);
    assert!(multi_arg_result.is_ok(), "search_by_tags should accept multiple tag arguments");

    // Test registering with ToolGroupRegistry
    let mut registry = ToolGroupRegistry::new();
    registry.register_group(Box::new(system_group)).await
        .expect("Should be able to register SystemToolGroup");

    // Should have system group
    let groups = registry.list_groups();
    assert!(groups.contains(&"system".to_string()), "Registry should contain 'system' group");

    // Should have system tools in registry
    let all_tools = registry.list_all_tools();
    assert!(!all_tools.is_empty(), "Registry should have tools from SystemToolGroup");
    assert!(all_tools.contains(&"search_documents".to_string()), "Should have search_documents");

    // Test tool execution
    let result = registry.execute_tool("system_info", &[]).await;
    assert!(result.is_ok(), "Should be able to execute system_info tool");

    let tool_result = result.unwrap();
    assert!(tool_result.is_success(), "system_info execution should succeed");
    println!("✅ SystemToolGroup tool execution works: {}", tool_result.output);

    // Test error cases
    let missing_tool_result = registry.execute_tool("nonexistent_tool", &[]).await;
    assert!(missing_tool_result.is_err(), "Should fail when executing nonexistent tool");

    let bad_args_result = registry.execute_tool("list_files", &[]).await;
    assert!(bad_args_result.is_err(), "Should fail when list_files gets no arguments");

    println!("✅ SystemToolGroup implementation working correctly");
    Ok(())
}