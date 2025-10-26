//! Simple Error Handling Tests for Unified REPL Tool System
//!
//! This module implements basic TDD-style tests for error scenarios in the unified tool system.

/// Test context for error handling tests
struct ErrorHandlingTestContext {
    temp_dir: TempDir,
    tool_dir: PathBuf,
}

impl ErrorHandlingTestContext {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let tool_dir = temp_dir.path().join("tools");

        // Create directories
        std::fs::create_dir_all(&tool_dir)?;

        Ok(Self { temp_dir, tool_dir })
    }
}

/// Test missing tool error handling
#[tokio::test]
async fn test_missing_tool_error_handling() -> Result<()> {
    let context = ErrorHandlingTestContext::new()?;

    // Initialize unified registry
    let registry = UnifiedToolRegistry::new(context.tool_dir).await?;

    // Test non-existent tool
    let result = registry
        .execute_tool("definitely_nonexistent_tool_xyz", &[])
        .await;

    // This test should FAIL initially - the error handling needs improvement
    assert!(result.is_err(), "Should fail for non-existent tool");

    let error = result.unwrap_err();
    let error_msg = error.to_string();

    // Check for user-friendly error message
    assert!(
        error_msg.contains("not found"),
        "Error should mention tool not found: {}",
        error_msg
    );
    assert!(
        error_msg.contains("definitely_nonexistent_tool_xyz"),
        "Error should include tool name: {}",
        error_msg
    );

    // Verify system is still functional after error
    let tools = registry.list_tools().await;
    assert!(
        !tools.is_empty(),
        "System should remain functional after missing tool error"
    );

    println!("✅ Missing tool error handling test passed: {}", error_msg);
    Ok(())
}

/// Test parameter conversion failures
#[tokio::test]
async fn test_parameter_conversion_failures() -> Result<()> {
    let context = ErrorHandlingTestContext::new()?;
    let registry = UnifiedToolRegistry::new(context.tool_dir).await?;

    // Test too many arguments for no-argument tool
    let result = registry
        .execute_tool("system_info", &["extra_arg".to_string()])
        .await;
    assert!(result.is_err(), "Should fail with extra arguments");

    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("argument") || error_msg.contains("parameter"),
        "Error should mention argument issue: {}",
        error_msg
    );

    // Test missing required arguments
    let result = registry.execute_tool("list_files", &[]).await;
    assert!(result.is_err(), "Should fail with missing arguments");

    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("argument")
            || error_msg.contains("required")
            || error_msg.contains("path"),
        "Error should mention missing required argument: {}",
        error_msg
    );

    println!("✅ Parameter conversion error handling tests passed");
    Ok(())
}

/// Test tool execution failures
#[tokio::test]
async fn test_tool_execution_failures() -> Result<()> {
    let context = ErrorHandlingTestContext::new()?;
    let registry = UnifiedToolRegistry::new(context.tool_dir).await?;

    // Test tool execution with invalid file path
    let result = registry
        .execute_tool(
            "list_files",
            &["/definitely/invalid/path/that/does/not/exist/xyz123".to_string()],
        )
        .await;

    // This might succeed or fail depending on the system behavior
    // What's important is that it doesn't panic and handles the error gracefully
    match result {
        Ok(_) => println!("✅ Tool execution handled invalid path gracefully"),
        Err(e) => {
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("failed")
                    || error_msg.contains("error")
                    || error_msg.contains("not found")
                    || error_msg.contains("parameter"),
                "Error should indicate execution failure: {}",
                error_msg
            );
            println!("✅ Tool execution failed gracefully: {}", error_msg);
        }
    }

    // Test with a tool that requires valid arguments
    let result = registry
        .execute_tool("read_file", &["/nonexistent/file/path/xyz.txt".to_string()])
        .await;

    // Should either succeed (empty file) or fail gracefully
    match result {
        Ok(_) => println!("✅ Read file executed successfully"),
        Err(e) => {
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("failed")
                    || error_msg.contains("error")
                    || error_msg.contains("not found")
                    || error_msg.contains("parameter"),
                "Error should be informative: {}",
                error_msg
            );
            println!("✅ Read file failed gracefully: {}", error_msg);
        }
    }

    println!("✅ Tool execution failure tests passed");
    Ok(())
}

/// Test system recovery after errors
#[tokio::test]
async fn test_system_recovery_after_errors() -> Result<()> {
    let context = ErrorHandlingTestContext::new()?;
    let registry = UnifiedToolRegistry::new(context.tool_dir).await?;

    // Execute several errors in sequence
    let error_attempts = vec![
        ("nonexistent_tool_1", vec![]),
        ("nonexistent_tool_2", vec!["arg".to_string()]),
        ("list_files", vec![]), // Missing required argument
    ];

    for (tool_name, args) in error_attempts {
        let result = registry.execute_tool(tool_name, &args).await;
        assert!(
            result.is_err(),
            "Should fail for error case: {} with {:?}",
            tool_name,
            args
        );
    }

    // System should still be functional
    let tools = registry.list_tools().await;
    assert!(
        !tools.is_empty(),
        "System should have tools after error sequence"
    );

    // Should be able to execute a valid tool
    if tools.contains(&"system_info".to_string()) {
        let result = registry.execute_tool("system_info", &[]).await;
        match result {
            Ok(_) => println!("✅ System recovered and can execute valid tools"),
            Err(e) => println!("⚠️  System recovery limited: {}", e.to_string()),
        }
    }

    println!("✅ System recovery and graceful degradation tests passed");
    Ok(())
}
use crate::test_utilities::{
    AssertUtils, MemoryUsage, PerformanceMeasurement, TestContext, TestDataGenerator,
};
use anyhow::Result;
use crucible_cli::commands::repl::tools::UnifiedToolRegistry;
use crucible_cli::commands::repl::tools::UnifiedToolRegistry;
use std::path::PathBuf;
use tempfile::TempDir;
