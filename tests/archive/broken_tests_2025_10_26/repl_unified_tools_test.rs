//! Integration tests for unified REPL tool system
//!
//! This module tests that the REPL properly integrates with the unified tool system
//! and can discover and execute both system tools (crucible-tools) and Rune tools.

mod test_utilities;
use anyhow::Result;
use crucible_cli::commands::repl::tools::UnifiedToolRegistry;
use std::path::PathBuf;
use tempfile::TempDir;
use test_utilities::{
    AssertUtils, MemoryUsage, PerformanceMeasurement, TestContext, TestDataGenerator,
};

/// Test context for REPL unified tools tests
struct ReplUnifiedToolsTestContext {
    temp_dir: TempDir,
    tool_dir: PathBuf,
}

impl ReplUnifiedToolsTestContext {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let tool_dir = temp_dir.path().join("tools");

        // Create directories
        std::fs::create_dir_all(&tool_dir)?;

        Ok(Self { temp_dir, tool_dir })
    }
}

/// Task 8: Write integration test - UnifiedToolRegistry discovers system tools
///
/// This test verifies that the UnifiedToolRegistry successfully discovers and integrates
/// system tools through the unified tool group system.
#[tokio::test]
async fn test_unified_registry_discovers_system_tools() -> Result<()> {
    let context = ReplUnifiedToolsTestContext::new()?;

    // Initialize unified registry directly
    let registry = UnifiedToolRegistry::new(context.tool_dir).await?;

    // Test that registry has system tools
    let tools = registry.list_tools().await;
    assert!(
        !tools.is_empty(),
        "Registry should have discovered system tools, got empty list"
    );

    // Should contain known system tools
    let expected_system_tools = vec![
        "search_documents",
        "get_vault_stats",
        "system_info",
        "list_files",
    ];

    for expected_tool in expected_system_tools {
        assert!(
            tools.contains(&expected_tool.to_string()),
            "Registry should contain '{}' from system tools. Available: {:?}",
            expected_tool,
            tools
        );
    }

    // Test grouped tool listing
    let grouped_tools = registry.list_tools_by_group().await;
    assert!(
        grouped_tools.contains_key("system"),
        "Should have 'system' group. Groups: {:?}",
        grouped_tools.keys()
    );

    let system_tools = grouped_tools.get("system").unwrap();
    assert!(!system_tools.is_empty(), "System group should not be empty");

    println!(
        "✅ UnifiedRegistry successfully discovered {} system tools",
        system_tools.len()
    );
    println!("📋 System tools: {:?}", system_tools);

    // Test tool group lookup
    for tool in system_tools {
        let group_name = registry.get_tool_group(tool).await.unwrap();
        assert_eq!(
            group_name, "system",
            "Tool '{}' should belong to 'system' group",
            tool
        );
    }

    // Test tool execution (system_info requires no arguments)
    let result = registry.execute_tool("system_info", &[]).await?;
    assert!(result.is_success(), "system_info execution should succeed");
    assert!(
        result.output.contains("platform"),
        "Output should contain system info"
    );

    println!("✅ Tool execution working: system_info returned valid JSON");
    println!("📊 Sample output: {}", result.output);

    // Test registry statistics
    let stats = registry.get_stats().await;
    assert!(
        stats.contains_key("total_tools"),
        "Stats should contain total_tools"
    );
    assert!(
        stats.contains_key("system_tools"),
        "Stats should contain system_tools count"
    );

    println!("📈 Registry stats: {:?}", stats);

    Ok(())
}

/// Test unified registry tool execution with different argument patterns
#[tokio::test]
async fn test_unified_registry_tool_execution() -> Result<()> {
    let context = ReplUnifiedToolsTestContext::new()?;

    // Initialize registry
    let registry = UnifiedToolRegistry::new(context.tool_dir).await?;

    // Test executing various system tools with different argument patterns

    // Test no-argument tool
    let result = registry.execute_tool("get_environment", &[]).await?;
    assert!(
        result.is_success(),
        "get_environment should succeed with no args"
    );
    println!("✅ get_environment: {} chars", result.output.len());

    // Test single argument tool
    let result = registry
        .execute_tool("list_files", &["/tmp".to_string()])
        .await?;
    assert!(
        result.is_success(),
        "list_files should succeed with path argument"
    );
    println!("✅ list_files /tmp: {}", result.output);

    // Test multi-argument tool
    let result = registry
        .execute_tool("search_by_tags", &["tag1".to_string(), "tag2".to_string()])
        .await?;
    // This might not return results but should not error
    println!("✅ search_by_tags executed: {}", result.is_success());

    // Test error handling - missing arguments
    let result = registry.execute_tool("list_files", &[]).await;
    assert!(
        result.is_err(),
        "list_files should fail with missing arguments"
    );
    println!("✅ Proper error handling for missing args");

    // Test error handling - non-existent tool
    let result = registry.execute_tool("nonexistent_tool", &[]).await;
    assert!(result.is_err(), "nonexistent tool should fail");
    println!("✅ Proper error handling for missing tool");

    Ok(())
}

/// Test unified mode toggle functionality
#[tokio::test]
async fn test_unified_mode_functionality() -> Result<()> {
    let context = ReplUnifiedToolsTestContext::new()?;

    // Create unified registry directly
    let registry = UnifiedToolRegistry::new(context.tool_dir).await?;

    // Test unified mode is enabled by default
    assert!(
        registry.is_unified_enabled(),
        "Unified mode should be enabled by default"
    );

    // Test we can get grouped tools
    let grouped = registry.list_tools_by_group().await;
    println!("🔍 Grouped tools: {:?}", grouped);

    // Test we have system group
    assert!(grouped.contains_key("system"), "Should have system group");

    // Test we can list all tools
    let all_tools = registry.list_tools().await;
    assert!(!all_tools.is_empty(), "Should have tools available");
    println!("📋 Total tools available: {}", all_tools.len());

    // Test tool group lookup
    if let Some(tool_name) = all_tools.first() {
        let group = registry.get_tool_group(tool_name).await;
        assert!(group.is_some(), "First tool should have a group");
        println!(
            "🏷️  Tool '{}' belongs to group: {}",
            tool_name,
            group.unwrap()
        );
    }

    Ok(())
}
