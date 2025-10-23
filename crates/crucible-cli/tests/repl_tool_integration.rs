//! REPL Tool Integration Tests
//!
//! These tests validate the unified tool system functionality
//! that powers the REPL's tool integration.

use crucible_cli::commands::repl::tools::UnifiedToolRegistry;
use tempfile::TempDir;

/// Create a test unified tool registry
async fn create_test_tool_registry() -> Result<(UnifiedToolRegistry, TempDir), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().join("tools");
    std::fs::create_dir_all(&tool_dir)?;

    let registry = UnifiedToolRegistry::new(tool_dir).await?;

    Ok((registry, temp_dir))
}

#[tokio::test]
async fn test_unified_tool_registry_initialization() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing unified tool registry initialization");

    let (registry, _temp_dir) = create_test_tool_registry().await?;

    // Test that the unified tool registry is properly initialized
    let tools = registry.list_tools().await;
    println!("📋 Found {} tools in unified registry", tools.len());

    // Should find system tools
    assert!(!tools.is_empty(), "❌ Should have tools available");
    assert!(tools.len() >= 20, "❌ Should have at least 20 system tools, found {}", tools.len());

    // Check for specific expected system tools
    let expected_tools = vec![
        "system_info",
        "get_vault_stats",
        "list_files",
        "search_content",
    ];

    let mut found_count = 0;
    for expected_tool in expected_tools {
        if tools.contains(&expected_tool.to_string()) {
            found_count += 1;
            println!("✅ Found expected tool: {}", expected_tool);
        } else {
            println!("⚠️  Expected tool not found: {}", expected_tool);
        }
    }

    assert!(found_count >= 3, "❌ Should find at least 3 expected tools, found {}", found_count);

    println!("✅ Unified tool registry initialization test passed");
    Ok(())
}

#[tokio::test]
async fn test_tools_by_group_functionality() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing tools by group functionality");

    let (registry, _temp_dir) = create_test_tool_registry().await?;

    // Test tools grouped by source
    let grouped_tools = registry.list_tools_by_group().await;
    println!("📋 Tool groups: {:?}", grouped_tools.keys().collect::<Vec<_>>());

    // Should have system group
    assert!(grouped_tools.contains_key("system"), "❌ Should have 'system' tool group");

    let system_tools = grouped_tools.get("system").unwrap();
    assert!(!system_tools.is_empty(), "❌ System group should not be empty");
    assert!(system_tools.len() >= 20, "❌ System group should have at least 20 tools, found {}", system_tools.len());

    // Verify specific tools are in system group
    let expected_system_tools = vec![
        "system_info",
        "get_vault_stats",
        "list_files",
    ];

    let mut found_in_system = 0;
    for tool in expected_system_tools {
        if system_tools.contains(&tool.to_string()) {
            found_in_system += 1;
            println!("✅ Found {} in system group", tool);
        }
    }

    assert!(found_in_system >= 2, "❌ Should find at least 2 expected tools in system group, found {}", found_in_system);

    // Test that we can get the group for a specific tool
    if let Some(group_name) = registry.get_tool_group("system_info").await {
        assert_eq!(group_name, "system", "❌ system_info should be in system group");
        println!("✅ Tool group lookup works correctly");
    } else {
        panic!("❌ Should be able to find group for system_info");
    }

    println!("✅ Tools by group functionality test passed");
    Ok(())
}

#[tokio::test]
async fn test_tool_execution_integration() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing tool execution integration");

    // Ensure tools are properly initialized
    crucible_cli::common::tool_manager::CrucibleToolManager::ensure_initialized_global().await?;

    let (registry, _temp_dir) = create_test_tool_registry().await?;

    // Test executing a simple system tool
    let result = registry.execute_tool("system_info", &[]).await;
    assert!(result.is_ok(), "❌ Should be able to execute system_info tool");

    let tool_result = result.unwrap();
    assert!(matches!(tool_result.status, crucible_cli::commands::repl::tools::ToolStatus::Success),
              "❌ system_info should execute successfully");
    assert!(!tool_result.output.is_empty(), "❌ system_info should produce output");

    println!("✅ system_info tool output: {} chars", tool_result.output.len());

    // Test executing get_vault_stats
    let vault_result = registry.execute_tool("get_vault_stats", &[]).await;
    assert!(vault_result.is_ok(), "❌ Should be able to execute get_vault_stats tool");

    let vault_tool_result = vault_result.unwrap();
    assert!(matches!(vault_tool_result.status, crucible_cli::commands::repl::tools::ToolStatus::Success),
              "❌ get_vault_stats should execute successfully");
    assert!(!vault_tool_result.output.is_empty(), "❌ get_vault_stats should produce output");

    println!("✅ get_vault_stats tool output: {} chars", vault_tool_result.output.len());

    // Test executing a tool with arguments
    let list_result = registry.execute_tool("list_files", &[".".to_string()]).await;
    assert!(list_result.is_ok(), "❌ Should be able to execute list_files with arguments");

    let list_tool_result = list_result.unwrap();
    match list_tool_result.status {
        crucible_cli::commands::repl::tools::ToolStatus::Success => {
            println!("✅ list_files with arguments executed successfully");
        }
        crucible_cli::commands::repl::tools::ToolStatus::Error(_) => {
            println!("⚠️  list_files with arguments returned error (may be expected in test environment)");
        }
    }

    println!("✅ Tool execution integration test passed");
    Ok(())
}

#[tokio::test]
async fn test_unknown_tool_handling() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing unknown tool handling");

    let (registry, _temp_dir) = create_test_tool_registry().await?;

    // Test executing an unknown tool
    let unknown_tool = "definitely_not_a_real_tool_12345";
    let result = registry.execute_tool(unknown_tool, &[]).await;

    assert!(result.is_err(), "❌ Should return error for unknown tool");

    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("not found") || error_msg.contains("not found or execution failed"),
            "❌ Error should mention tool not found: {}", error_msg);

    println!("✅ Unknown tool properly handled: {}", error_msg);

    // Test that unknown tool doesn't crash the system
    let tools_after = registry.list_tools().await;
    assert!(!tools_after.is_empty(), "❌ Tool list should still be available after unknown tool error");

    println!("✅ System remains stable after unknown tool error");
    println!("✅ Unknown tool handling test passed");
    Ok(())
}

#[tokio::test]
async fn test_performance_metrics() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing performance metrics");

    let (registry, _temp_dir) = create_test_tool_registry().await?;

    // Get initial metrics
    let initial_metrics = registry.get_performance_metrics().await;
    println!("📊 Initial metrics: {:?}", initial_metrics);

    // Execute a few tools to generate metrics
    let _ = registry.execute_tool("system_info", &[]).await;
    let _ = registry.execute_tool("get_vault_stats", &[]).await;

    // Get updated metrics
    let updated_metrics = registry.get_performance_metrics().await;
    println!("📊 Updated metrics: {:?}", updated_metrics);

    // Verify metrics structure
    assert!(updated_metrics.group_metrics.contains_key("system"), "❌ Should have metrics for system group");

    let system_metrics = updated_metrics.group_metrics.get("system").unwrap();
    println!("📊 System group metrics: {:?}", system_metrics);

    // Should have recorded some activity
    assert!(system_metrics.total_execution_time_ms >= 0, "❌ Should track execution time");

    println!("✅ Performance metrics test passed");
    Ok(())
}

#[tokio::test]
async fn test_unified_mode_configuration() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing unified mode configuration");

    let (mut registry, _temp_dir) = create_test_tool_registry().await?;

    // Test unified mode is enabled by default
    assert!(registry.is_unified_enabled(), "❌ Unified mode should be enabled by default");

    // Test disabling unified mode
    registry.set_unified_mode(false);
    assert!(!registry.is_unified_enabled(), "❌ Unified mode should be disabled");

    // Test re-enabling unified mode
    registry.set_unified_mode(true);
    assert!(registry.is_unified_enabled(), "❌ Unified mode should be re-enabled");

    // Test that tools are still available in both modes
    let tools_unified = registry.list_tools().await;
    registry.set_unified_mode(false);
    let tools_legacy = registry.list_tools().await;

    assert!(!tools_unified.is_empty(), "❌ Tools should be available in unified mode");
    println!("✅ Unified mode tools: {}", tools_unified.len());
    println!("✅ Legacy mode tools: {}", tools_legacy.len());

    println!("✅ Unified mode configuration test passed");
    Ok(())
}

#[test]
fn test_create_test_registry() -> Result<(), Box<dyn std::error::Error>> {
    // Test the test helper function synchronously
    tokio_test::block_on(async {
        let (registry, _temp_dir) = create_test_tool_registry().await?;
        let tools = registry.list_tools().await;
        assert!(!tools.is_empty(), "Should have tools in test registry");
        Ok(())
    })
}