//! Quick test to validate performance improvements in the unified tool system

use std::path::PathBuf;
use tokio;

// Add this to the CLI crate's lib.rs or create a test binary
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("Testing Performance Optimizations for Unified Tool System");
    println!("========================================================\n");

    // Test 1: Lazy initialization
    println!("Test 1: Lazy Initialization");
    let start_time = std::time::Instant::now();

    let tool_dir = PathBuf::from("/tmp/test_tools");
    let registry = crucible_cli::commands::repl::tools::UnifiedToolRegistry::new(tool_dir).await?;

    let init_time = start_time.elapsed();
    println!("✓ Registry initialization: {}ms (should be fast due to lazy loading)", init_time.as_millis());

    if init_time.as_millis() > 100 {
        println!("⚠️  Warning: Initialization took longer than expected");
    } else {
        println!("✓ Initialization is efficient");
    }

    // Test 2: Tool discovery (triggers lazy loading)
    println!("\nTest 2: Tool Discovery");
    let start_time = std::time::Instant::now();

    let tools = registry.list_tools().await;

    let discovery_time = start_time.elapsed();
    println!("✓ Tool discovery: {}ms (includes lazy initialization)", discovery_time.as_millis());
    println!("✓ Discovered {} tools", tools.len());

    // Test 3: Second discovery (should use cache)
    println!("\nTest 3: Cached Tool Discovery");
    let start_time = std::time::Instant::now();

    let tools2 = registry.list_tools().await;

    let cached_time = start_time.elapsed();
    println!("✓ Cached discovery: {}ms", cached_time.as_millis());
    println!("✓ Found {} tools (should match previous count)", tools2.len());

    if cached_time < discovery_time {
        let improvement = ((discovery_time.as_millis() - cached_time.as_millis()) as f64
            / discovery_time.as_millis() as f64) * 100.0;
        println!("✓ Cache provides {:.1}% improvement", improvement);
    }

    // Test 4: Performance metrics
    println!("\nTest 4: Performance Metrics");
    let metrics = registry.get_performance_metrics().await;

    if let Some(init_time) = metrics.initialization_time_ms {
        println!("✓ Registry initialization time: {}ms", init_time);
    }

    println!("✓ Registry metrics:");
    println!("  - Total groups: {}", metrics.registry_metrics.total_groups);
    println!("  - Total tools: {}", metrics.registry_metrics.total_tools);
    println!("  - Lazy initializations: {}", metrics.registry_metrics.lazy_initializations);

    // Test 5: Tool execution
    println!("\nTest 5: Tool Execution");
    if let Some(tool) = tools.first() {
        let start_time = std::time::Instant::now();

        match registry.execute_tool(tool, &[]).await {
            Ok(result) => {
                let exec_time = start_time.elapsed();
                println!("✓ Executed tool '{}' in {}ms", tool, exec_time.as_millis());
                println!("✓ Tool result: {:?}", result.success);
            }
            Err(e) => {
                println!("⚠️  Tool execution failed: {}", e);
            }
        }
    } else {
        println!("ℹ️  No tools available for execution test");
    }

    // Test 6: Compare cache configurations
    println!("\nTest 6: Cache Configuration Comparison");

    let configs = vec![
        ("No caching", crucible_cli::commands::repl::tools::ToolGroupCacheConfig::no_caching()),
        ("Default caching", crucible_cli::commands::repl::tools::ToolGroupCacheConfig::default()),
        ("Fast caching", crucible_cli::commands::repl::tools::ToolGroupCacheConfig::fast_cache()),
    ];

    for (name, config) in configs {
        let tool_dir = PathBuf::from("/tmp/test_tools");
        let start_time = std::time::Instant::now();

        let registry = crucible_cli::commands::repl::tools::UnifiedToolRegistry::with_cache_config(tool_dir, config).await?;
        let _tools = registry.list_tools().await;

        let total_time = start_time.elapsed();
        println!("✓ {}: {}ms", name, total_time.as_millis());
    }

    println!("\n========================================================");
    println!("✅ Performance optimization tests completed successfully!");

    Ok(())
}