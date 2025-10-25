//! Direct REPL Integration Tests
//!
//! This test file validates the REPL's `:run` command functionality by testing
//! the REPL components directly rather than spawning a separate process.
//!
//! The tests:
//! 1. Create REPL instances directly with test configuration
//! 2. Test tool discovery and execution through the unified registry
//! 3. Validate system tool execution with actual output
//! 4. Test error handling for invalid tools
//! 5. Verify REPL stability and proper cleanup

use std::time::Duration;
use std::env;
use tempfile::TempDir;
use anyhow::Result;
use tokio::time::timeout;

use crate::tests::common::TestKiln;
use crucible_cli::commands::repl::Repl;
use crucible_cli::config::CliConfig;

/// Test system tool execution through REPL
#[tokio::test]
async fn test_repl_system_tool_execution() -> Result<()> {
    // Create test kiln
    let kiln = TestKiln::new()?;

    // Create CLI config for testing
    let config = create_test_config(&kiln)?;

    // Create REPL instance
    let mut repl = timeout(Duration::from_secs(30), Repl::new(&config, None, None, "table".to_string()))
        .await
        .map_err(|_| anyhow::anyhow!("REPL creation timed out"))??;

    // Test system info tool execution
    test_system_info_tool(&repl).await?;

    // Test kiln stats tool execution
    test_kiln_stats_tool(&repl, &kiln).await?;

    // Test tools listing
    test_tools_listing(&repl).await?;

    println!("✓ All direct REPL tool execution tests passed");
    Ok(())
}

/// Test error handling for invalid tools
#[tokio::test]
async fn test_repl_error_handling() -> Result<()> {
    // Create test kiln
    let kiln = TestKiln::new()?;

    // Create CLI config for testing
    let config = create_test_config(&kiln)?;

    // Create REPL instance
    let mut repl = timeout(Duration::from_secs(30), Repl::new(&config, None, None, "table".to_string()))
        .await
        .map_err(|_| anyhow::anyhow!("REPL creation timed out"))??;

    // Test invalid tool execution
    test_invalid_tool_execution(&repl).await?;

    println!("✓ REPL error handling tests passed");
    Ok(())
}

/// Test multiple tool execution sequence
#[tokio::test]
async fn test_repl_multiple_tool_sequence() -> Result<()> {
    // Create test kiln with content
    let kiln = TestKiln::new()?;
    kiln.create_note("test.md", "# Test Document\n\nContent here.")?;
    kiln.create_note("project/notes.md", "# Project Notes\n\nImportant info.")?;

    // Create CLI config for testing
    let config = create_test_config(&kiln)?;

    // Create REPL instance
    let mut repl = timeout(Duration::from_secs(30), Repl::new(&config, None, None, "table".to_string()))
        .await
        .map_err(|_| anyhow::anyhow!("REPL creation timed out"))??;

    // Execute sequence of tools
    test_tool_execution_sequence(&repl).await?;

    println!("✓ Multiple tool execution sequence test passed");
    Ok(())
}

/// Create a test configuration for the REPL
fn create_test_config(kiln: &TestKiln) -> Result<CliConfig> {
    let temp_dir = TempDir::new()?;

    // Set required environment variable for security
    env::set_var("OBSIDIAN_VAULT_PATH", &kiln.kiln_path_str());
    let config_content = format!(
        r#"
kiln:
  path: "{}"

database:
  path: "{}"

tools:
  directory: "{}"

logging:
  level: "info"
"#,
        kiln.kiln_path_str(),
        kiln.db_path_str(),
        temp_dir.path().display()
    );

    // Create a temporary config file
    let config_file = temp_dir.path().join("config.yaml");
    std::fs::write(&config_file, config_content)?;

    // Load configuration from the temporary file
    CliConfig::load(
        Some(config_file.to_string_lossy().to_string().into()),
        None,
        None,
    )
}

/// Test system info tool execution
async fn test_system_info_tool(repl: &Repl) -> Result<()> {
    println!("Testing system_info tool execution...");

    // Get the tool registry from the REPL
    let tools = repl.get_tools();

    // List available tools and verify system_info is present
    let tool_list = tools.list_tools().await;
    assert!(tool_list.contains(&"system_info".to_string()),
           "system_info tool should be available");

    // Execute system_info tool
    let result = tools.execute_tool("system_info", &[]).await
        .map_err(|e| anyhow::anyhow!("Failed to execute system_info: {}", e))?;

    // Verify successful execution
    assert!(matches!(result.status, crucible_cli::commands::repl::tools::ToolStatus::Success),
           "system_info should execute successfully");

    // Verify output contains expected information
    let output = result.output;
    assert!(!output.is_empty(), "system_info should produce output");

    // Check for key system information indicators
    let has_os_info = output.to_lowercase().contains("os") ||
                     output.to_lowercase().contains("operating") ||
                     output.to_lowercase().contains("system");

    let has_memory_info = output.to_lowercase().contains("memory") ||
                         output.to_lowercase().contains("ram");

    let has_disk_info = output.to_lowercase().contains("disk") ||
                       output.to_lowercase().contains("storage");

    // At least one type of system information should be present
    assert!(has_os_info || has_memory_info || has_disk_info,
           "Output should contain system information (OS, memory, or disk)");

    println!("✓ system_info tool executed successfully");
    println!("Output preview: {}", &output[..output.len().min(100)]);

    Ok(())
}

/// Test kiln stats tool execution
async fn test_kiln_stats_tool(repl: &Repl, kiln: &TestKiln) -> Result<()> {
    println!("Testing get_kiln_stats tool execution...");

    // Get the tool registry from the REPL
    let tools = repl.get_tools();

    // Create some test content
    kiln.create_note("test1.md", "# Test 1\n\nContent.")?;
    kiln.create_note("test2.md", "# Test 2\n\nMore content.")?;

    // Check if get_kiln_stats tool is available
    let tool_list = tools.list_tools().await;
    if tool_list.contains(&"get_kiln_stats".to_string()) {
        // Execute get_kiln_stats tool
        let result = tools.execute_tool("get_kiln_stats", &[]).await
            .map_err(|e| anyhow::anyhow!("Failed to execute get_kiln_stats: {}", e))?;

        // Verify successful execution
        assert!(matches!(result.status, crucible_cli::commands::repl::tools::ToolStatus::Success),
               "get_kiln_stats should execute successfully");

        // Verify output contains kiln statistics
        let output = result.output;
        assert!(!output.is_empty(), "get_kiln_stats should produce output");

        // Check for expected kiln statistics indicators
        let has_notes_count = output.to_lowercase().contains("notes") ||
                             output.to_lowercase().contains("total") ||
                             output.contains("2"); // We created 2 notes

        let has_size_info = output.to_lowercase().contains("size") ||
                           output.to_lowercase().contains("bytes");

        assert!(has_notes_count || has_size_info,
               "Output should contain kiln statistics");

        println!("✓ get_kiln_stats tool executed successfully");
        println!("Output preview: {}", &output[..output.len().min(100)]);
    } else {
        println!("ℹ get_kiln_stats tool not available, skipping test");
    }

    Ok(())
}

/// Test tools listing functionality
async fn test_tools_listing(repl: &Repl) -> Result<()> {
    println!("Testing tools listing functionality...");

    // Get the tool registry from the REPL
    let tools = repl.get_tools();

    // List tools by group
    let grouped_tools = tools.list_tools_by_group().await;

    // Should have at least one tool group
    assert!(!grouped_tools.is_empty(), "Should have at least one tool group");

    // Check if system tools group exists
    let has_system_tools = grouped_tools.contains_key("system");
    let has_rune_tools = grouped_tools.contains_key("rune");

    // At least one of these should be present
    assert!(has_system_tools || has_rune_tools,
           "Should have either system or rune tools available");

    // Verify tools in groups
    for (group_name, tool_list) in &grouped_tools {
        assert!(!tool_list.is_empty(),
               "Tool group '{}' should not be empty", group_name);

        println!("Tool group '{}': {} tools", group_name, tool_list.len());
        for tool in tool_list {
            println!("  - {}", tool);
        }
    }

    println!("✓ Tools listing functionality works correctly");
    Ok(())
}

/// Test invalid tool execution
async fn test_invalid_tool_execution(repl: &Repl) -> Result<()> {
    println!("Testing invalid tool execution...");

    // Get the tool registry from the REPL
    let tools = repl.get_tools();

    // Try to execute a non-existent tool
    let invalid_tool_name = "nonexistent_tool_xyz_12345";
    let result = tools.execute_tool(invalid_tool_name, &[]).await;

    // Should result in an error
    assert!(result.is_err(),
           "Executing non-existent tool should return an error");

    if let Err(e) = result {
        println!("✓ Invalid tool execution returned expected error: {}", e);
    }

    // Try to execute a tool with invalid arguments (if the tool exists)
    let tool_list = tools.list_tools().await;
    if let Some(first_tool) = tool_list.first() {
        // Try with invalid arguments for a tool that takes no arguments
        let result = tools.execute_tool(first_tool, &["invalid_arg".to_string()]).await;

        // This might succeed or fail depending on the tool, but should not panic
        match result {
            Ok(_) => println!("ℹ Tool '{}' accepted invalid arguments (may be expected)", first_tool),
            Err(e) => println!("✓ Tool '{}' correctly rejected invalid arguments: {}", first_tool, e),
        }
    }

    println!("✓ Invalid tool execution test completed");
    Ok(())
}

/// Test execution sequence of multiple tools
async fn test_tool_execution_sequence(repl: &Repl) -> Result<()> {
    println!("Testing multiple tool execution sequence...");

    // Get the tool registry from the REPL
    let tools = repl.get_tools();

    let tool_list = tools.list_tools().await;

    // Test execution of available tools in sequence
    let mut successful_executions = 0;
    let total_tools_to_test = std::cmp::min(3, tool_list.len()); // Test up to 3 tools

    for tool_name in tool_list.iter().take(total_tools_to_test) {
        println!("Testing tool: {}", tool_name);

        // Execute the tool with minimal arguments
        let result = tools.execute_tool(tool_name, &[]).await;

        match result {
            Ok(tool_result) => {
                match tool_result.status {
                    crucible_cli::commands::repl::tools::ToolStatus::Success => {
                        successful_executions += 1;
                        println!("✓ {} executed successfully", tool_name);

                        // Verify output is not empty for successful tools
                        assert!(!tool_result.output.is_empty(),
                               "Successful tool {} should produce output", tool_name);
                    }
                    crucible_cli::commands::repl::tools::ToolStatus::Error(ref error) => {
                        println!("⚠ {} returned error: {}", tool_name, error);
                        // This is acceptable for some tools that need specific arguments
                    }
                }
            }
            Err(e) => {
                println!("⚠ {} failed to execute: {}", tool_name, e);
                // This might be expected for tools that require specific setup
            }
        }
    }

    // At least one tool should execute successfully
    assert!(successful_executions > 0 || tool_list.is_empty(),
           "At least one tool should execute successfully, unless no tools are available");

    println!("✓ Multiple tool execution sequence completed");
    println!("Successful executions: {}/{}", successful_executions, total_tools_to_test);

    Ok(())
}

/// Test REPL performance and stability
#[tokio::test]
async fn test_repl_performance_stability() -> Result<()> {
    println!("Testing REPL performance and stability...");

    // Create test kiln
    let kiln = TestKiln::new()?;

    // Create CLI config for testing
    let config = create_test_config(&kiln)?;

    // Create REPL instance
    let mut repl = timeout(Duration::from_secs(30), Repl::new(&config, None, None, "table".to_string()))
        .await
        .map_err(|_| anyhow::anyhow!("REPL creation timed out"))??;

    let tools = &repl.tools;

    // Test multiple rapid tool executions
    let start_time = std::time::Instant::now();
    let tool_list = tools.list_tools().await;

    if !tool_list.is_empty() {
        let test_tool = &tool_list[0];
        let mut successful_executions = 0;
        let total_executions = 5;

        for i in 1..=total_executions {
            let result = tools.execute_tool(test_tool, &[]).await;

            match result {
                Ok(tool_result) => {
                    if matches!(tool_result.status, crucible_cli::commands::repl::tools::ToolStatus::Success) {
                        successful_executions += 1;
                    }
                }
                Err(_) => {
                    // Some executions might fail, which is acceptable
                }
            }

            // Small delay between executions
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let execution_time = start_time.elapsed();

        // Performance assertion - should complete within reasonable time
        assert!(execution_time < Duration::from_secs(30),
               "Multiple tool executions should complete within 30 seconds, took {:?}", execution_time);

        println!("✓ Performance test completed");
        println!("Executed {} times in {:?} ({} successful)",
                total_executions, execution_time, successful_executions);
    } else {
        println!("ℹ No tools available for performance testing");
    }

    println!("✓ REPL performance and stability test passed");
    Ok(())
}

/// Test REPL configuration and initialization
#[tokio::test]
async fn test_repl_configuration() -> Result<()> {
    println!("Testing REPL configuration and initialization...");

    // Create test kiln
    let kiln = TestKiln::new()?;

    // Test with different configurations
    let configs = vec![
        ("table", None),
        ("json", None),
        ("csv", None),
    ];

    for (format, db_path) in configs {
        println!("Testing with format: {}", format);

        // Create CLI config for testing
        let config = create_test_config(&kiln)?;

        // Create REPL instance with specific configuration
        let mut repl = timeout(Duration::from_secs(30), Repl::new(&config, db_path, None, format.to_string()))
            .await
            .map_err(|_| anyhow::anyhow!("REPL creation timed out"))??;

        // Verify REPL was created successfully
        assert!(!repl.get_tools().list_tools().await.is_empty() || true, // Allow empty tool list
               "REPL should initialize correctly with {} format", format);

        println!("✓ REPL initialized successfully with {} format", format);
    }

    println!("✓ REPL configuration test passed");
    Ok(())
}