//! Unit tests for REPL command processing and tool integration
//!
//! These tests focus on testing the REPL logic directly without launching
//! external processes. They test command parsing, tool execution, and output
//! formatting through direct interfaces.

/// Test context for REPL unit tests
struct ReplUnitTestContext {
    temp_dir: TempDir,
    kiln_path: PathBuf,
    tool_dir: PathBuf,
    db_path: PathBuf,
}

impl ReplUnitTestContext {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let kiln_path = temp_dir.path().join("kiln");
        let tool_dir = temp_dir.path().join("tools");
        let db_path = temp_dir.path().join("test.db");

        // Create directories
        std::fs::create_dir_all(&kiln_path)?;
        std::fs::create_dir_all(&tool_dir)?;

        Ok(Self {
            temp_dir,
            kiln_path,
            tool_dir,
            db_path,
        })
    }

    /// Get kiln path for testing
    fn get_kiln_path(&self) -> &str {
        self.kiln_path.to_str().unwrap()
    }
}

/// Task 1: Test REPL command parsing and execution directly
#[tokio::test]
async fn test_repl_command_parsing_and_execution() -> Result<()> {
    let context = ReplUnitTestContext::new()?;

    // Test command parsing
    let parsed_tools_command = Input::parse(":tools")?;
    assert!(matches!(
        parsed_tools_command,
        Input::Command(Command::ListTools)
    ));

    let parsed_run_command = Input::parse(":run system_info")?;
    match parsed_run_command {
        Input::Command(Command::RunTool { tool_name, args }) => {
            assert_eq!(tool_name, "system_info");
            assert!(args.is_empty());
        }
        _ => panic!("Expected RunTool command"),
    }

    let parsed_run_with_args = Input::parse(":run list_files /tmp")?;
    match parsed_run_with_args {
        Input::Command(Command::RunTool { tool_name, args }) => {
            assert_eq!(tool_name, "list_files");
            assert_eq!(args, vec!["/tmp"]);
        }
        _ => panic!("Expected RunTool command with args"),
    }

    println!("✅ Command parsing works correctly");
    Ok(())
}

/// Task 2: Test UnifiedToolRegistry integration with REPL
#[tokio::test]
async fn test_unified_tool_registry_with_repl() -> Result<()> {
    let context = ReplUnitTestContext::new()?;

    // Create UnifiedToolRegistry directly (like REPL does)
    let registry = UnifiedToolRegistry::new(context.tool_dir.clone()).await?;

    // Test that we have system tools
    let all_tools = registry.list_tools().await;
    assert!(!all_tools.is_empty(), "Should have system tools available");

    // Test that we can get tools by group
    let grouped_tools = registry.list_tools_by_group().await;
    assert!(
        grouped_tools.contains_key("system"),
        "Should have system group"
    );

    let system_tools = grouped_tools.get("system").unwrap();
    assert!(!system_tools.is_empty(), "System group should have tools");

    // Test executing a system tool directly (like REPL would)
    if system_tools.contains(&"system_info".to_string()) {
        let result = registry.execute_tool("system_info", &[]).await?;
        assert!(
            result.is_success(),
            "system_info should execute successfully"
        );
        assert!(!result.output.is_empty(), "Should have output");

        // Verify output is valid JSON
        let _parsed: serde_json::Value = serde_json::from_str(&result.output)
            .map_err(|e| anyhow::anyhow!("Invalid JSON output: {}", e))?;

        println!(
            "✅ system_info executed successfully: {} chars",
            result.output.len()
        );
    }

    // Test tool group lookup
    for tool_name in system_tools.iter().take(5) {
        let group = registry.get_tool_group(tool_name).await;
        assert_eq!(
            group.unwrap(),
            "system",
            "Tool {} should belong to system group",
            tool_name
        );
    }

    println!("✅ UnifiedToolRegistry integration works correctly");
    println!("📊 Found {} system tools", system_tools.len());
    Ok(())
}

/// Task 3: Test REPL tool listing functionality
#[tokio::test]
async fn test_repl_tools_listing_functionality() -> Result<()> {
    let context = ReplUnitTestContext::new()?;

    // Create a minimal REPL-like setup to test tool listing
    let registry = Arc::new(UnifiedToolRegistry::new(context.tool_dir.clone()).await?);

    // Simulate the list_tools method from REPL
    let grouped_tools = registry.list_tools_by_group().await;

    // Verify the expected format matches what REPL :tools should show
    assert!(!grouped_tools.is_empty(), "Should have tool groups");

    let total_tools: usize = grouped_tools.values().map(|v| v.len()).sum();
    assert!(total_tools > 0, "Should have at least some tools");

    // Should have system group
    assert!(
        grouped_tools.contains_key("system"),
        "Should have 'system' group"
    );

    let system_tools = grouped_tools.get("system").unwrap();
    assert!(!system_tools.is_empty(), "System group should not be empty");

    // Check for expected system tools
    let expected_tools = vec![
        "system_info",
        "list_files",
        "search_documents",
        "get_kiln_stats",
    ];

    let mut found_tools = 0;
    for expected_tool in expected_tools {
        if system_tools.contains(&expected_tool.to_string()) {
            found_tools += 1;
        }
    }

    assert!(
        found_tools >= 2,
        "Should find at least 2 expected system tools, found {}",
        found_tools
    );

    println!("✅ Tool listing functionality works correctly");
    println!("📦 Found {} tool groups", grouped_tools.len());
    println!("🔧 System tools available: {}", system_tools.len());
    println!(
        "🎯 Expected tools found: {}/{}",
        found_tools,
        expected_tools.len()
    );

    Ok(())
}

/// Task 4: Test REPL tool execution with error handling
#[tokio::test]
async fn test_repl_tool_execution_with_error_handling() -> Result<()> {
    let context = ReplUnitTestContext::new()?;

    let registry = UnifiedToolRegistry::new(context.tool_dir.clone()).await?;

    // Test successful execution
    let result = registry.execute_tool("system_info", &[]).await?;
    assert!(result.is_success(), "system_info should succeed");
    assert!(!result.output.is_empty(), "Should have output");
    println!("✅ Successful execution: system_info");

    // Test execution with missing required arguments
    let result = registry.execute_tool("list_files", &[]).await;
    assert!(result.is_err(), "list_files without args should fail");
    println!("✅ Error handling: missing arguments");

    // Test execution of non-existent tool
    let result = registry.execute_tool("nonexistent_tool_12345", &[]).await;
    assert!(result.is_err(), "nonexistent tool should fail");
    println!("✅ Error handling: non-existent tool");

    // Test execution with too many arguments (if tool doesn't expect them)
    let result = registry
        .execute_tool("system_info", &["extra", "args"])
        .await;
    // This might succeed or fail depending on implementation
    println!("🔍 Extra args result: {}", result.is_ok());

    println!("✅ Tool execution error handling works correctly");
    Ok(())
}

/// Task 5: Test REPL input parsing edge cases
#[tokio::test]
async fn test_repl_input_parsing_edge_cases() -> Result<()> {
    // Test various input formats that REPL should handle

    // Empty input
    let empty_result = Input::parse("")?;
    assert!(matches!(empty_result, Input::Empty));

    let whitespace_result = Input::parse("   \t  ")?;
    assert!(matches!(whitespace_result, Input::Empty));

    // Commands with extra whitespace
    let tools_command = Input::parse("   :tools   ")?;
    assert!(matches!(tools_command, Input::Command(Command::ListTools)));

    let run_command = Input::parse("\t:run  \t  system_info   \t")?;
    match run_command {
        Input::Command(Command::RunTool { tool_name, args }) => {
            assert_eq!(tool_name, "system_info");
            assert!(args.is_empty());
        }
        _ => panic!("Expected RunTool command"),
    }

    // Complex run command with multiple arguments
    let complex_run = Input::parse(":run search_files pattern /path --recursive")?;
    match complex_run {
        Input::Command(Command::RunTool { tool_name, args }) => {
            assert_eq!(tool_name, "search_files");
            assert_eq!(args, vec!["pattern", "/path", "--recursive"]);
        }
        _ => panic!("Expected RunTool with multiple args"),
    }

    // Query input (non-command)
    let query_input = Input::parse("SELECT * FROM notes;")?;
    assert!(matches!(query_input, Input::Query(q) if q.contains("SELECT")));

    // Query with leading whitespace
    let query_whitespace = Input::parse("  SELECT title FROM notes;  ")?;
    assert!(matches!(query_whitespace, Input::Query(q) if q.contains("SELECT")));

    println!("✅ Input parsing edge cases handled correctly");
    Ok(())
}

/// Task 6: Test REPL tool execution with JSON output validation
#[tokio::test]
async fn test_repl_tool_output_validation() -> Result<()> {
    let context = ReplUnitTestContext::new()?;

    let registry = UnifiedToolRegistry::new(context.tool_dir.clone()).await?;

    // Get all available tools and test a few
    let tools = registry.list_tools().await;
    let mut tested_tools = 0;
    let mut successful_json = 0;

    for tool_name in tools.iter().take(5) {
        // Try to execute tools that typically don't require arguments
        let args = match tool_name.as_str() {
            "system_info" | "get_environment" => vec![],
            "list_files" => vec!["/tmp".to_string()],
            _ => continue, // Skip tools that might require complex arguments
        };

        match registry.execute_tool(tool_name, &args).await {
            Ok(result) => {
                tested_tools += 1;

                // Try to parse output as JSON
                match serde_json::from_str::<serde_json::Value>(&result.output) {
                    Ok(json_value) => {
                        successful_json += 1;
                        println!("✅ {} produces valid JSON", tool_name);

                        // Additional validation for system_info
                        if tool_name == "system_info" {
                            assert!(
                                json_value.as_object().is_some(),
                                "system_info should return object"
                            );
                        }
                    }
                    Err(_) => {
                        println!(
                            "⚠️  {} produces non-JSON output (might be expected)",
                            tool_name
                        );
                    }
                }
            }
            Err(e) => {
                println!("❌ {} failed: {}", tool_name, e);
            }
        }
    }

    assert!(tested_tools > 0, "Should have tested at least one tool");
    assert!(
        successful_json > 0,
        "Should have at least one tool producing valid JSON"
    );

    println!("✅ Tool output validation completed");
    println!(
        "📊 Tested: {}, Valid JSON: {}",
        tested_tools, successful_json
    );

    Ok(())
}

/// Task 7: Test REPL tool grouping and routing logic
#[tokio::test]
async fn test_repl_tool_grouping_and_routing() -> Result<()> {
    let context = ReplUnitTestContext::new()?;

    let registry = UnifiedToolRegistry::new(context.tool_dir.clone()).await?;

    // Test tool group lookup
    let tools = registry.list_tools().await;
    assert!(!tools.is_empty(), "Should have tools available");

    // Verify system tools are properly grouped
    let grouped = registry.list_tools_by_group().await;
    assert!(grouped.contains_key("system"), "Should have system group");

    let system_tools = grouped.get("system").unwrap();
    assert!(!system_tools.is_empty(), "System group should have tools");

    // Test that each system tool returns the correct group
    for tool_name in system_tools.iter().take(10) {
        let group = registry.get_tool_group(tool_name).await;
        assert_eq!(
            group.unwrap(),
            "system",
            "Tool {} should be in system group",
            tool_name
        );
    }

    // Test routing: system tools should be executed through group registry first
    let test_tools = vec!["system_info", "get_environment"];
    for tool_name in test_tools {
        if system_tools.contains(&tool_name.to_string()) {
            let result = registry.execute_tool(tool_name, &[]).await;
            assert!(result.is_ok(), "Should execute {} successfully", tool_name);

            if let Ok(result) = result {
                assert!(result.is_success(), "{} should return success", tool_name);
                println!("✅ {} routed and executed successfully", tool_name);
            }
        }
    }

    println!("✅ Tool grouping and routing works correctly");
    println!("🏷️  System tools properly grouped: {}", system_tools.len());
    Ok(())
}

#[tokio::test]
async fn test_repl_config_initialization() -> Result<()> {
    let context = ReplUnitTestContext::new()?;

    // Test path components for REPL config
    assert!(context.kiln_path.exists(), "Kiln path should exist");
    assert!(context.tool_dir.exists(), "Tool directory should exist");
    assert!(
        context.db_path.to_str().is_some(),
        "DB path should be valid"
    );

    println!("✅ REPL config initialization components work");
    Ok(())
}
use crate::test_utilities::{
    AssertUtils, MemoryUsage, PerformanceMeasurement, TestContext, TestDataGenerator,
};
use crate::test_utilities::{
    AssertUtils, MemoryUsage, PerformanceMeasurement, TestContext, TestDataGenerator,
};
use crate::test_utilities::{
    AssertUtils, MemoryUsage, PerformanceMeasurement, TestContext, TestDataGenerator,
};
use anyhow::Result;
use crucible_cli::commands::repl::command::Command;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::{mpsc, Mutex};
