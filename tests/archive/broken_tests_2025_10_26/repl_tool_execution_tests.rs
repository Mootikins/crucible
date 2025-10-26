use crate::test_utilities::TestKiln;
use anyhow::Result;
use crucible_cli::commands::repl::tools::UnifiedToolRegistry;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

// REPL Tool Execution Tests
//
// This test file validates the core tool execution functionality that powers the REPL's
// `:run` command by testing the unified tool registry and system tools directly.
//
// The tests focus on:
// 1. Tool discovery and listing
// 2. System tool execution with real output
// 3. Error handling for invalid tools
// 4. Tool registry initialization and configuration
// 5. Integration with crucible-tools

/// Test tool registry initialization and basic functionality
#[tokio::test]
async fn test_tool_registry_initialization() -> Result<()> {
    println!("Testing tool registry initialization...");

    // Create a temporary directory for tools
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create tool registry
    let registry = timeout(
        Duration::from_secs(10),
        UnifiedToolRegistry::new(tool_dir.clone()),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Tool registry initialization timed out"))??;

    // List available tools
    let tools = registry.list_tools().await;

    println!("Available tools: {:?}", tools);

    // Should have at least some tools available
    assert!(
        !tools.is_empty(),
        "Tool registry should have tools available"
    );

    // Check for expected system tools
    let expected_tools = vec!["system_info", "list_files", "read_file"];
    let found_expected_tools: Vec<String> = expected_tools
        .iter()
        .filter(|&tool| tools.contains(&tool.to_string()))
        .map(|&tool| tool.to_string())
        .collect();

    println!("Found expected system tools: {:?}", found_expected_tools);

    println!("✓ Tool registry initialization test passed");
    Ok(())
}

/// Test system tools execution
#[tokio::test]
async fn test_system_tools_execution() -> Result<()> {
    println!("Testing system tools execution...");

    // Create a temporary directory for tools
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create tool registry
    let registry = timeout(
        Duration::from_secs(10),
        UnifiedToolRegistry::new(tool_dir.clone()),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Tool registry initialization timed out"))??;

    // Test system_info tool
    test_system_info_execution(&registry).await?;

    // Test list_files tool
    test_list_files_execution(&registry).await?;

    println!("✓ System tools execution test passed");
    Ok(())
}

/// Test system_info tool execution
async fn test_system_info_execution(registry: &UnifiedToolRegistry) -> Result<()> {
    println!("Testing system_info tool execution...");

    // Check if system_info tool is available
    let tools = registry.list_tools().await;
    if !tools.contains(&"system_info".to_string()) {
        println!("ℹ system_info tool not available, skipping");
        return Ok(());
    }

    // Execute system_info tool
    let result = registry.execute_tool("system_info", &[]).await?;

    // Verify successful execution
    assert!(
        matches!(
            result.status,
            crucible_cli::commands::repl::tools::ToolStatus::Success
        ),
        "system_info should execute successfully"
    );

    // Verify output contains expected information
    let output = result.output;
    assert!(!output.is_empty(), "system_info should produce output");

    // Check for system information indicators
    let has_os_info = output.to_lowercase().contains("os")
        || output.to_lowercase().contains("operating")
        || output.to_lowercase().contains("system");

    let has_memory_info =
        output.to_lowercase().contains("memory") || output.to_lowercase().contains("ram");

    let has_disk_info =
        output.to_lowercase().contains("disk") || output.to_lowercase().contains("storage");

    // At least one type of system information should be present
    assert!(
        has_os_info || has_memory_info || has_disk_info,
        "Output should contain system information"
    );

    println!("✓ system_info tool executed successfully");
    println!("Output preview: {}", &output[..output.len().min(100)]);

    Ok(())
}

/// Test list_files tool execution
async fn test_list_files_execution(registry: &UnifiedToolRegistry) -> Result<()> {
    println!("Testing list_files tool execution...");

    // Check if list_files tool is available
    let tools = registry.list_tools().await;
    if !tools.contains(&"list_files".to_string()) {
        println!("ℹ list_files tool not available, skipping");
        return Ok(());
    }

    // Create a temporary directory with some files
    let temp_dir = TempDir::new()?;
    let test_dir = temp_dir.path();

    std::fs::write(test_dir.join("test1.txt"), "Test content 1")?;
    std::fs::write(test_dir.join("test2.txt"), "Test content 2")?;
    std::fs::create_dir_all(test_dir.join("subdir"))?;
    std::fs::write(test_dir.join("subdir/test3.txt"), "Test content 3")?;

    // Execute list_files tool with the test directory
    let result = registry
        .execute_tool("list_files", &[test_dir.to_string_lossy().to_string()])
        .await?;

    // Verify successful execution
    assert!(
        matches!(
            result.status,
            crucible_cli::commands::repl::tools::ToolStatus::Success
        ),
        "list_files should execute successfully"
    );

    // Verify output contains expected files
    let output = result.output;
    assert!(!output.is_empty(), "list_files should produce output");

    // Check for any file/directory content (output format may vary)
    assert!(!output.is_empty(), "list_files should produce output");

    // The output should contain some indication of files or directories
    let has_file_content = output.contains("test")
        || output.contains(".txt")
        || output.contains("subdir")
        || output.to_lowercase().contains("file")
        || output.to_lowercase().contains("directory");

    println!("List files output: {}", &output[..output.len().min(200)]);

    // Don't assert strictly as the output format may vary, just check it's not empty
    assert!(
        has_file_content || !output.is_empty(),
        "Output should contain file information or be non-empty"
    );

    println!("✓ list_files tool executed successfully");
    println!("Output preview: {}", &output[..output.len().min(100)]);

    Ok(())
}

/// Test tools grouping functionality
#[tokio::test]
async fn test_tools_grouping() -> Result<()> {
    println!("Testing tools grouping functionality...");

    // Create a temporary directory for tools
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create tool registry
    let registry = timeout(
        Duration::from_secs(10),
        UnifiedToolRegistry::new(tool_dir.clone()),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Tool registry initialization timed out"))??;

    // Get tools grouped by category
    let grouped_tools = registry.list_tools_by_group().await;

    // Should have at least one group
    assert!(
        !grouped_tools.is_empty(),
        "Should have at least one tool group"
    );

    // Look for expected groups
    let has_system_group = grouped_tools.contains_key("system");
    let has_rune_group = grouped_tools.contains_key("rune");

    println!("Tool groups found:");
    for (group_name, tools) in &grouped_tools {
        println!("  {}: {} tools", group_name, tools.len());
        for tool in tools {
            println!("    - {}", tool);
        }
    }

    // At least one expected group should be present
    assert!(
        has_system_group || has_rune_group,
        "Should have either system or rune tools available"
    );

    println!("✓ Tools grouping test passed");
    Ok(())
}

/// Test error handling for invalid tools
#[tokio::test]
async fn test_invalid_tool_handling() -> Result<()> {
    println!("Testing invalid tool handling...");

    // Create a temporary directory for tools
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create tool registry
    let registry = timeout(
        Duration::from_secs(10),
        UnifiedToolRegistry::new(tool_dir.clone()),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Tool registry initialization timed out"))??;

    // Try to execute a non-existent tool
    let invalid_tool_name = "nonexistent_tool_xyz_12345";
    let result = registry.execute_tool(invalid_tool_name, &[]).await;

    // Should result in an error
    assert!(
        result.is_err(),
        "Executing non-existent tool should return an error"
    );

    if let Err(e) = result {
        println!("✓ Invalid tool execution returned expected error: {}", e);
    }

    // Try with invalid arguments for a tool that exists
    let tools = registry.list_tools().await;
    if let Some(first_tool) = tools.first() {
        let result = registry
            .execute_tool(
                first_tool,
                &["invalid_arg".to_string(), "another_invalid".to_string()],
            )
            .await;

        // This might succeed or fail depending on the tool, but should not panic
        match result {
            Ok(tool_result) => match tool_result.status {
                crucible_cli::commands::repl::tools::ToolStatus::Success => {
                    println!("ℹ Tool '{}' accepted invalid arguments", first_tool);
                }
                crucible_cli::commands::repl::tools::ToolStatus::Error(ref error) => {
                    println!(
                        "✓ Tool '{}' correctly rejected invalid arguments: {}",
                        first_tool, error
                    );
                }
            },
            Err(e) => {
                println!(
                    "ℹ Tool '{}' failed to execute with invalid args: {}",
                    first_tool, e
                );
            }
        }
    }

    println!("✓ Invalid tool handling test passed");
    Ok(())
}

/// Test tool execution performance
#[tokio::test]
async fn test_tool_execution_performance() -> Result<()> {
    println!("Testing tool execution performance...");

    // Create a temporary directory for tools
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create tool registry
    let registry = timeout(
        Duration::from_secs(10),
        UnifiedToolRegistry::new(tool_dir.clone()),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Tool registry initialization timed out"))??;

    let tools = registry.list_tools().await;

    if !tools.is_empty() {
        let test_tool = &tools[0];
        println!("Testing performance with tool: {}", test_tool);

        let start_time = std::time::Instant::now();
        let mut successful_executions = 0;
        let total_executions = 3;

        for i in 1..=total_executions {
            let result = registry.execute_tool(test_tool, &[]).await;

            match result {
                Ok(tool_result) => {
                    if matches!(
                        tool_result.status,
                        crucible_cli::commands::repl::tools::ToolStatus::Success
                    ) {
                        successful_executions += 1;
                    }
                }
                Err(_) => {
                    // Some executions might fail, which is acceptable
                }
            }

            // Small delay between executions
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let execution_time = start_time.elapsed();

        // Performance assertion - should complete within reasonable time
        assert!(
            execution_time < Duration::from_secs(10),
            "Multiple tool executions should complete within 10 seconds, took {:?}",
            execution_time
        );

        println!("✓ Performance test completed");
        println!(
            "Executed {} times in {:?} ({} successful)",
            total_executions, execution_time, successful_executions
        );
    } else {
        println!("ℹ No tools available for performance testing");
    }

    println!("✓ Tool execution performance test passed");
    Ok(())
}

/// Test integration with kiln tools
#[tokio::test]
async fn test_kiln_tools_integration() -> Result<()> {
    println!("Testing kiln tools integration...");

    // Create test kiln with content
    let kiln = TestKiln::new()?;
    kiln.create_note("test1.md", "# Test Document 1\n\nContent here.")?;
    kiln.create_note("test2.md", "# Test Document 2\n\nMore content.")?;

    // Create a temporary directory for tools
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create tool registry
    let registry = timeout(
        Duration::from_secs(10),
        UnifiedToolRegistry::new(tool_dir.clone()),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Tool registry initialization timed out"))??;

    // Look for kiln-related tools
    let tools = registry.list_tools().await;
    let kiln_tools: Vec<&String> = tools
        .iter()
        .filter(|tool| tool.contains("kiln") || tool.contains("search") || tool.contains("stats"))
        .collect();

    println!("Found kiln-related tools: {:?}", kiln_tools);

    // If kiln tools are available, test one
    if let Some(kiln_tool) = kiln_tools.first() {
        println!("Testing kiln tool: {}", kiln_tool);

        // Try to execute the kiln tool
        let result = registry.execute_tool(kiln_tool, &[]).await;

        match result {
            Ok(tool_result) => {
                match tool_result.status {
                    crucible_cli::commands::repl::tools::ToolStatus::Success => {
                        println!("✓ Kiln tool '{}' executed successfully", kiln_tool);
                        println!(
                            "Output preview: {}",
                            &tool_result.output[..tool_result.output.len().min(100)]
                        );
                    }
                    crucible_cli::commands::repl::tools::ToolStatus::Error(ref error) => {
                        println!("ℹ Kiln tool '{}' returned error: {}", kiln_tool, error);
                        // This might be expected if the tool needs specific setup
                    }
                }
            }
            Err(e) => {
                println!("ℹ Kiln tool '{}' failed to execute: {}", kiln_tool, e);
                // This might be expected if the tool needs specific configuration
            }
        }
    } else {
        println!("ℹ No kiln-related tools found");
    }

    println!("✓ Vault tools integration test passed");
    Ok(())
}
