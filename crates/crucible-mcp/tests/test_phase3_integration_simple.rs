/// Simplified Phase 3 Integration Tests
///
/// Focuses on testing the core Phase 3 workflow without complex dependencies.

use anyhow::Result;
use crucible_mcp::rune_tools::ToolRegistry;
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::obsidian_client::ObsidianClient;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;

/// Create a simple test tool for integration testing
fn create_simple_test_tool() -> String {
    r#"
pub fn NAME() { "simple_test_tool" }
pub fn DESCRIPTION() { "Simple test tool for Phase 3 integration" }

pub fn INPUT_SCHEMA() {
    #{
        type: "object",
        properties: #{
            message: #{
                type: "string",
                description: "Message to process"
            },
            multiplier: #{
                type: "integer",
                description: "Multiplier for processing",
                minimum: 1,
                maximum: 10
            }
        },
        required: ["message"]
    }
}

pub async fn call(args) {
    let multiplier = args.multiplier.unwrap_or(1);
    let processed = format!("{} (x{})", args.message, multiplier);

    #{
        success: true,
        original_message: args.message,
        processed_message: processed,
        multiplier: multiplier,
        timestamp: "2025-10-16T10:00:00Z"
    }
}
"#.to_string()
}

/// Create a type inference test tool
fn create_type_inference_tool() -> String {
    r#"
pub fn NAME() { "type_inference_tool" }
pub fn DESCRIPTION() { "Tool for testing type inference capabilities" }

pub fn INPUT_SCHEMA() {
    #{
        type: "object",
        properties: #{
            numbers: #{
                type: "array",
                items: #{ "type": "number" },
                description: "Array of numbers to process"
            },
            config: #{
                type: "object",
                properties: #{
                    operation: #{ "type": "string" },
                    precision: #{ "type": "integer", "minimum": 0, "maximum": 10 }
                }
            }
        },
        required: ["numbers"]
    }
}

pub async fn call(args) {
    let result = 0.0;
    let count = 0;

    if let Some(numbers) = args.numbers {
        for num in numbers {
            result += num;
        }
    }

    let operation = args.config.and_then(|c| c.operation).unwrap_or("sum".to_string());
    let final_result = match operation.as_str() {
        "avg" => if count > 0 { result / count as f64 } else { 0.0 },
        "max" => {
            let max_val = 0.0;
            if let Some(numbers) = args.numbers {
                for num in numbers {
                    if num > max_val { return num; }
                }
            }
            max_val
        },
        _ => result
    };

    #{
        success: true,
        operation: operation,
        result: final_result,
        input_count: args.numbers.and_then(|n| Some(n.len())).unwrap_or(0)
    }
}
"#.to_string()
}

/// Create an async operations test tool
fn create_async_tool() -> String {
    r#"
pub fn NAME() { "async_operations_tool" }
pub fn DESCRIPTION() { "Tool for testing async operation detection" }

pub fn INPUT_SCHEMA() {
    #{
        type: "object",
        properties: #{
            operation_type: #{ "type": "string" },
            delay_ms: #{
                type: "integer",
                minimum: 0,
                maximum: 500
            }
        },
        required: ["operation_type"]
    }
}

// This should be detected as async by naming conventions
pub async fn call(args) {

    match args.operation_type {
        "quick" => #{
            success: true,
            operation: "quick",
            result: "Immediate response",
            execution_time_ms: 0
        },
        "delayed" => #{
            success: true,
            operation: "delayed",
            result: "Delayed response (simulated)",
            execution_time_ms: args.delay_ms
        },
        "batch" => #{
            success: true,
            operation: "batch",
            result: "Batch processing completed",
            items_processed: 42
        },
        _ => #{
            success: false,
            error: "Unknown operation type"
        }
    }
}
"#.to_string()
}

#[tokio::test]
async fn test_enhanced_discovery_and_loading() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create test tools
    std::fs::write(tool_dir.join("simple_test.rn"), create_simple_test_tool())?;
    std::fs::write(tool_dir.join("type_inference.rn"), create_type_inference_tool())?;
    std::fs::write(tool_dir.join("async_operations.rn"), create_async_tool())?;

    // Create registry with enhanced discovery
    let db = Arc::new(EmbeddingDatabase::new(":memory:").await?);
    let obsidian = Arc::new(ObsidianClient::new()?);
    let mut registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db.clone(), obsidian)?;

    // Test enhanced discovery
    println!("🔍 Testing enhanced discovery...");
    let loaded_tools = registry.scan_and_load().await?;
    println!("✓ Enhanced discovery loaded {} tools", loaded_tools.len());

    assert!(loaded_tools.len() >= 3, "Should discover all test tools");

    let registry_arc = Arc::new(RwLock::new(registry));

    // Verify enhanced discovery features
    {
        let reg = registry_arc.read().await;
        let tools = reg.list_tools();
        println!("✓ Discovered {} tools with enhanced metadata", tools.len());

        // Check for expected tools
        let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
        assert!(tool_names.contains(&"simple_test_tool".to_string()));
        assert!(tool_names.contains(&"type_inference_tool".to_string()));
        assert!(tool_names.contains(&"async_operations_tool".to_string()));

        // Validate tool metadata
        for tool in &tools {
            assert!(!tool.name.is_empty());
            assert!(!tool.description.is_empty());
            assert_eq!(tool.input_schema.get("type"), Some(&json!("object")));
            assert!(tool.input_schema.get("properties").is_some());
            println!("  - Tool: {} ({})", tool.name, tool.description);
        }

        // Verify enhanced mode
        assert!(reg.is_enhanced_mode(), "Should be in enhanced discovery mode");
    }

    println!("✅ Enhanced discovery and loading test passed!");
    Ok(())
}

#[tokio::test]
async fn test_tool_execution_through_registry() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create test tool
    std::fs::write(tool_dir.join("simple_test_tool.rn"), create_simple_test_tool())?;

    // Create registry
    let db = Arc::new(EmbeddingDatabase::new(":memory:").await?);
    let obsidian = Arc::new(ObsidianClient::new()?);
    let mut registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db.clone(), obsidian)?;

    // Load tools
    let loaded_tools = registry.scan_and_load().await?;
    assert_eq!(loaded_tools.len(), 1);

    let registry_arc = Arc::new(RwLock::new(registry));

    // Test tool execution
    {
        let reg = registry_arc.read().await;
        let tool = reg.get_tool("simple_test_tool")
            .ok_or_else(|| anyhow::anyhow!("Tool not found"))?
            .clone();
        let context = reg.context().clone();
        drop(reg);

        // Execute the tool
        let args = json!({
            "message": "Hello Execution Test",
            "multiplier": 3
        });

        let result = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(tool.call(args, &context))
        }).await??;

        // Validate execution result
        assert!(result.is_object());
        assert_eq!(result["success"], true);
        assert_eq!(result["original_message"], "Hello Execution Test");
        assert_eq!(result["processed_message"], "Hello Execution Test (x3)");
        assert_eq!(result["multiplier"], 3);

        println!("✓ Tool execution successful: {}", result["processed_message"]);
    }

    println!("✅ Tool execution through registry test passed!");
    Ok(())
}

#[tokio::test]
async fn test_type_inference_and_validation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create type inference tool
    std::fs::write(tool_dir.join("type_inference_tool.rn"), create_type_inference_tool())?;

    // Create registry
    let db = Arc::new(EmbeddingDatabase::new(":memory:").await?);
    let obsidian = Arc::new(ObsidianClient::new()?);
    let mut registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db.clone(), obsidian)?;

    // Load tools
    let loaded_tools = registry.scan_and_load().await?;
    assert_eq!(loaded_tools.len(), 1);

    let registry_arc = Arc::new(RwLock::new(registry));

    // Test type inference with different data types
    {
        let reg = registry_arc.read().await;
        let tool = reg.get_tool("type_inference_tool")
            .ok_or_else(|| anyhow::anyhow!("Tool not found"))?
            .clone();
        let context = reg.context().clone();
        drop(reg);

        // Test with number array
        let args = json!({
            "numbers": [1.5, 2.0, 3.5, 4.0],
            "config": {
                "operation": "sum",
                "precision": 2
            }
        });

        let result = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(tool.call(args, &context))
        }).await??;

        assert!(result.is_object());
        assert_eq!(result["success"], true);
        assert_eq!(result["operation"], "sum");
        assert!((result["result"].as_f64().unwrap() - 11.0).abs() < 0.001);
        assert_eq!(result["input_count"], 4);

        println!("✓ Type inference successful: sum = {}", result["result"]);
    }

    println!("✅ Type inference and validation test passed!");
    Ok(())
}

#[tokio::test]
async fn test_async_detection_and_execution() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create async test tool
    std::fs::write(tool_dir.join("async_operations_tool.rn"), create_async_tool())?;

    // Create registry
    let db = Arc::new(EmbeddingDatabase::new(":memory:").await?);
    let obsidian = Arc::new(ObsidianClient::new()?);
    let mut registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db.clone(), obsidian)?;

    // Load tools
    let loaded_tools = registry.scan_and_load().await?;
    assert_eq!(loaded_tools.len(), 1);

    let registry_arc = Arc::new(RwLock::new(registry));

    // Test async operations
    {
        let reg = registry_arc.read().await;
        let tool = reg.get_tool("async_operations_tool")
            .ok_or_else(|| anyhow::anyhow!("Tool not found"))?
            .clone();
        let context = reg.context().clone();
        drop(reg);

        // Test quick operation
        let args = json!({
            "operation_type": "quick"
        });

        let result = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(tool.call(args, &context))
        }).await??;

        assert!(result.is_object());
        assert_eq!(result["success"], true);
        assert_eq!(result["operation"], "quick");
        assert_eq!(result["execution_time_ms"], 0);

        println!("✓ Async quick operation successful");
    }

    println!("✅ Async detection and execution test passed!");
    Ok(())
}

#[tokio::test]
async fn test_error_handling_workflow() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create a tool that handles errors gracefully
    let error_handling_tool = r#"
pub fn NAME() { "error_handling_tool" }
pub fn DESCRIPTION() { "Tool for testing error handling capabilities" }

pub fn INPUT_SCHEMA() {
    #{
        type: "object",
        properties: #{
            input_type: #{ "type": "string" },
            data: #{ "type": "string" }
        },
        required: ["input_type"]
    }
}

pub async fn call(args) {
    match args.input_type {
        "valid" => #{
            success: true,
            message: "Valid input processed",
            data: args.data
        },
        "invalid_type" => #{
            success: false,
            error: "Invalid input type provided",
            error_code: "INVALID_TYPE"
        },
        "missing_data" => #{
            success: false,
            error: "Required data field is missing",
            error_code: "MISSING_DATA"
        },
        _ => #{
            success: false,
            error: "Unknown input type",
            error_code: "UNKNOWN_TYPE"
        }
    }
}
"#.to_string();

    std::fs::write(tool_dir.join("error_handling_tool.rn"), error_handling_tool)?;

    // Create registry
    let db = Arc::new(EmbeddingDatabase::new(":memory:").await?);
    let obsidian = Arc::new(ObsidianClient::new()?);
    let mut registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db.clone(), obsidian)?;

    // Load tools
    let loaded_tools = registry.scan_and_load().await?;
    assert_eq!(loaded_tools.len(), 1);

    let registry_arc = Arc::new(RwLock::new(registry));

    // Test different error scenarios
    {
        let reg = registry_arc.read().await;

        // Test valid case
        let tool_clone = reg.get_tool("error_handling_tool")
            .ok_or_else(|| anyhow::anyhow!("Tool not found"))?
            .clone();
        let context_clone = reg.context().clone();
        drop(reg);

        let valid_args = json!({
            "input_type": "valid",
            "data": "test data"
        });

        let result = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(tool_clone.call(valid_args, &context_clone))
        }).await??;

        assert_eq!(result["success"], true);
        assert_eq!(result["message"], "Valid input processed");
        assert_eq!(result["data"], "test data");

        // Test error case
        let reg = registry_arc.read().await;
        let tool_clone = reg.get_tool("error_handling_tool")
            .ok_or_else(|| anyhow::anyhow!("Tool not found"))?
            .clone();
        let context_clone = reg.context().clone();
        drop(reg);

        let error_args = json!({
            "input_type": "invalid_type"
        });

        let result = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(tool_clone.call(error_args, &context_clone))
        }).await??;

        assert_eq!(result["success"], false);
        assert_eq!(result["error"], "Invalid input type provided");
        assert_eq!(result["error_code"], "INVALID_TYPE");

        println!("✓ Error handling workflow successful");
    }

    println!("✅ Error handling workflow test passed!");
    Ok(())
}

#[tokio::test]
async fn test_performance_and_scalability() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create performance test tool
    let perf_tool = r#"
pub fn NAME() { "performance_tool" }
pub fn DESCRIPTION() { "Tool for performance testing" }

pub fn INPUT_SCHEMA() {
    #{
        type: "object",
        properties: #{
            data_size: #{
                type: "integer",
                minimum: 10,
                maximum: 10000,
                description: "Size of data to process"
            },
            operation: #{ "type": "string" }
        },
        required: ["data_size"]
    }
}

pub async fn call(args) {
    let data_size = args.data_size;
    let operation = args.operation.unwrap_or("compute".to_string());

    let result = 0.0;
    for i in 1..=data_size {
        result += i as f64;
    }

    #{
        success: true,
        operation: operation,
        data_size: data_size,
        result: result,
        processing_time_ms: data_size / 100
    }
}
"#.to_string();

    std::fs::write(tool_dir.join("performance_tool.rn"), perf_tool)?;

    // Create registry
    let db = Arc::new(EmbeddingDatabase::new(":memory:").await?);
    let obsidian = Arc::new(ObsidianClient::new()?);
    let mut registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db.clone(), obsidian)?;

    // Load tools
    let loaded_tools = registry.scan_and_load().await?;
    assert_eq!(loaded_tools.len(), 1);

    let registry_arc = Arc::new(RwLock::new(registry));

    // Test performance with different data sizes
    {
        let reg = registry_arc.read().await;
        let tool = reg.get_tool("performance_tool")
            .ok_or_else(|| anyhow::anyhow!("Tool not found"))?
            .clone();
        let context = reg.context().clone();
        drop(reg);

        // Test with medium data size
        let args = json!({
            "data_size": 1000,
            "operation": "compute"
        });

        let start = std::time::Instant::now();
        let result = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(tool.call(args, &context))
        }).await??;
        let execution_time = start.elapsed();

        assert!(result.is_object());
        assert_eq!(result["success"], true);
        assert_eq!(result["data_size"], 1000);
        assert!((result["result"].as_f64().unwrap() - 500500.0).abs() < 0.001);

        // Performance should be reasonable
        assert!(execution_time.as_millis() < 1000, "Execution should be fast");

        println!("✓ Performance test: processed {} items in {:?}",
                result["data_size"], execution_time);
    }

    println!("✅ Performance and scalability test passed!");
    Ok(())
}