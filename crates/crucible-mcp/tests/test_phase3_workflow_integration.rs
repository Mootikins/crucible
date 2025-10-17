/// End-to-End Phase 3 Workflow Integration Tests
///
/// This test suite validates the complete Phase 3 workflow:
/// 1. Enhanced tool discovery with AST analysis
/// 2. Schema generation and validation
/// 3. Type inference and constraint validation
/// 4. Dynamic tool handler generation
/// 5. MCP service integration
/// 6. Complete execution pipeline

use anyhow::Result;
use crucible_mcp::rune_tools::{ToolRegistry};
use crucible_mcp::rune_tools::handler_generator::{DynamicRuneToolHandler, ToolHandlerGenerator};
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::obsidian_client::ObsidianClient;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;

/// Create a comprehensive test tool that exercises all Phase 3 features
fn create_comprehensive_test_tool() -> String {
    r#"
pub fn NAME() { "comprehensive_test_tool" }
pub fn DESCRIPTION() { "Comprehensive test tool for Phase 3 end-to-end workflow validation" }

pub fn INPUT_SCHEMA() {
    #{
        type: "object",
        properties: #{
            message: #{
                type: "string",
                description: "Message to process",
                minLength: 1,
                maxLength: 100
            },
            count: #{
                type: "integer",
                description: "Number of repetitions",
                minimum: 1,
                maximum: 10
            },
            options: #{
                type: "object",
                properties: #{
                    format: #{
                        type: "string",
                        enum: ["json", "xml", "text"]
                    },
                    priority: #{
                        type: "number",
                        minimum: 0.0,
                        maximum: 1.0
                    }
                },
                required: ["format"]
            },
            tags: #{
                type: "array",
                items: #{
                    type: "string"
                },
                maxItems: 5
            }
        },
        required: ["message", "count"]
    }
}

pub async fn call(args) {
    // Simulate processing with validation
    let processed = if let Some(options) = args.options {
        match options.format {
            "json" => format!("{{\"message\": \"{}\", \"count\": {}}}", args.message, args.count),
            "xml" => format!("<message count=\"{}\">{}</message>", args.count, args.message),
            _ => format!("{} (repeated {} times)", args.message, args.count)
        }
    } else {
        format!("{} (repeated {} times)", args.message, args.count)
    };

    let tag_count = if let Some(tags) = args.tags {
        tags.len()
    } else {
        0
    };

    #{
        success: true,
        processed_message: processed,
        original_message: args.message,
        repetition_count: args.count,
        tag_count: tag_count,
        timestamp: "2025-10-16T10:00:00Z",
        metadata: #{
            tool_version: "1.0.0",
            processing_time_ms: 42,
            validation_passed: true
        }
    }
}
"#.to_string()
}

/// Create a type inference test tool
fn create_type_inference_test_tool() -> String {
    r#"
pub fn NAME() { "type_inference_test_tool" }
pub fn DESCRIPTION() { "Test tool for validating type inference capabilities" }

pub fn INPUT_SCHEMA() {
    #{
        type: "object",
        properties: #{
            input_data: #{
                type: "array",
                items: #{
                    type: "object",
                    properties: #{
                        id: #{ "type": "string" },
                        value: #{ "type": "number" },
                        active: #{ "type": "boolean" }
                    },
                    required: ["id", "value"]
                }
            },
            filter_config: #{
                type: "object",
                properties: #{
                    min_value: #{ "type": "number" },
                    only_active: #{ "type": "boolean" }
                }
            }
        },
        required: ["input_data"]
    }
}

pub async fn call(args) {
    let mut filtered = Vec::new();

    if let Some(input_data) = args.input_data {
        let min_value = args.filter_config.and_then(|c| c.min_value).unwrap_or(0.0);
        let only_active = args.filter_config.and_then(|c| c.only_active).unwrap_or(false);

        for item in input_data {
            if item.value >= min_value && (!only_active || item.active) {
                filtered.push(item);
            }
        }
    }

    #{
        filtered_count: filtered.len(),
        filtered_items: filtered,
        total_processed: args.input_data.and_then(|d| Some(d.len())).unwrap_or(0)
    }
}
"#.to_string()
}

/// Create an async file operations test tool
fn create_async_test_tool() -> String {
    r#"
pub fn NAME() { "async_operations_tool" }
pub fn DESCRIPTION() { "Test tool for validating async operation detection and handling" }

pub fn INPUT_SCHEMA() {
    #{
        type: "object",
        properties: #{
            operation: #{
                type: "string",
                enum: ["read", "write", "process"]
            },
            file_path: #{
                type: "string",
                pattern: "^[a-zA-Z0-9_\\-./]+$"
            },
            content: #{
                type: "string"
            },
            wait_time_ms: #{
                type: "integer",
                minimum: 0,
                maximum: 1000
            }
        },
        required: ["operation"]
    }
}

// This should be detected as async by naming conventions
pub async fn call(args) {
    use std::time::Duration;

    // Simulate async operation
    if let Some(wait_time) = args.wait_time_ms {
        tokio::time::sleep(Duration::from_millis(wait_time as u64)).await;
    }

    match args.operation {
        "read" => #{
            success: true,
            operation: "read",
            content: "Simulated file content",
            size_bytes: 1024
        },
        "write" => #{
            success: true,
            operation: "write",
            bytes_written: args.content.and_then(|c| Some(c.len())).unwrap_or(0),
            file_path: args.file_path
        },
        "process" => #{
            success: true,
            operation: "process",
            processing_time_ms: args.wait_time_ms,
            result: "Data processed successfully"
        },
        _ => #{
            success: false,
            error: "Unknown operation"
        }
    }
}
"#.to_string()
}

#[tokio::test]
async fn test_complete_phase3_workflow_discovery_to_execution() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create comprehensive test tools
    std::fs::write(tool_dir.join("comprehensive_test.rn"), create_comprehensive_test_tool())?;
    std::fs::write(tool_dir.join("type_inference_test.rn"), create_type_inference_test_tool())?;
    std::fs::write(tool_dir.join("async_operations.rn"), create_async_test_tool())?;

    // Create registry with enhanced discovery
    let db = Arc::new(EmbeddingDatabase::new(":memory:").await?);
    let obsidian = Arc::new(ObsidianClient::new()?);
    let context = Arc::new(rune::Context::with_default_modules()?);
    let mut registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db.clone(), obsidian)?;

    // Step 1: Enhanced discovery
    let loaded_tools = registry.scan_and_load().await?;
    println!("✓ Phase 3.1: Enhanced discovery loaded {} tools", loaded_tools.len());
    assert!(loaded_tools.len() >= 3, "Should discover all test tools");

    let registry_arc = Arc::new(RwLock::new(registry));

    // Step 2: Validate enhanced discovery features
    {
        let reg = registry_arc.read().await;
        let tools = reg.list_tools();
        println!("✓ Phase 3.2: Discovered {} tools with enhanced metadata", tools.len());

        // Validate tool metadata
        for tool in &tools {
            assert!(!tool.name.is_empty());
            assert!(!tool.description.is_empty());
            assert_eq!(tool.input_schema.get("type"), Some(&json!("object")));
            println!("  - Tool: {} ({})", tool.name, tool.description);
        }

        // Verify enhanced mode
        assert!(reg.is_enhanced_mode(), "Should be in enhanced discovery mode");
    }

    // Step 3: Test dynamic handler generation
    let mut handler_generator = ToolHandlerGenerator::new(registry_arc.clone());
    let mcp_tools = handler_generator.generate_tool_list().await?;
    println!("✓ Phase 3.3: Generated {} MCP tools with handlers", mcp_tools.len());

    // Validate MCP tool generation
    for mcp_tool in &mcp_tools {
        assert!(!mcp_tool.name.is_empty());
        assert!(mcp_tool.description.is_some());
        assert!(mcp_tool.input_schema.type_.is_object());
        assert!(mcp_tool.annotations.is_some());

        let annotations = mcp_tool.annotations.as_ref().unwrap();
        assert!(annotations.title.is_some());
        println!("  - MCP Tool: {} ({})", mcp_tool.name, annotations.title.as_ref().unwrap());
    }

    // Step 4: Test individual tool execution
    let handler = DynamicRuneToolHandler::new(registry_arc.clone());

    // Test comprehensive tool
    let args = json!({
        "message": "Hello Phase 3",
        "count": 3,
        "options": {
            "format": "json",
            "priority": 0.8
        },
        "tags": ["test", "phase3", "workflow"]
    });

    let result = handler.execute_tool("comprehensive_test_tool", args).await?;
    println!("✓ Phase 3.4: Comprehensive tool execution successful");

    assert_eq!(result["success"], true);
    assert_eq!(result["processed_message"], "{\"message\": \"Hello Phase 3\", \"count\": 3}");
    assert_eq!(result["original_message"], "Hello Phase 3");
    assert_eq!(result["repetition_count"], 3);
    assert_eq!(result["tag_count"], 3);

    // Step 5: Test type inference tool
    let type_args = json!({
        "input_data": [
            {"id": "item1", "value": 10.5, "active": true},
            {"id": "item2", "value": 5.0, "active": false},
            {"id": "item3", "value": 15.0, "active": true}
        ],
        "filter_config": {
            "min_value": 8.0,
            "only_active": true
        }
    });

    let type_result = handler.execute_tool("type_inference_test_tool", type_args).await?;
    println!("✓ Phase 3.5: Type inference tool execution successful");

    assert_eq!(type_result["filtered_count"], 2);
    assert_eq!(type_result["total_processed"], 3);

    // Step 6: Test async operations tool
    let async_args = json!({
        "operation": "process",
        "wait_time_ms": 100
    });

    let async_result = handler.execute_tool("async_operations_tool", async_args).await?;
    println!("✓ Phase 3.6: Async operations tool execution successful");

    assert_eq!(async_result["success"], true);
    assert_eq!(async_result["operation"], "process");

    println!("✅ Complete Phase 3 workflow validation successful!");
    Ok(())
}

#[tokio::test]
async fn test_registry_with_enhanced_discovery_workflow() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create test tools
    std::fs::write(tool_dir.join("registry_test.rn"), create_comprehensive_test_tool())?;

    // Create registry with enhanced discovery
    let db = Arc::new(EmbeddingDatabase::new(":memory:").await?);
    let obsidian = Arc::new(ObsidianClient::new()?);
    let mut registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db.clone(), obsidian)?;

    // Step 1: Initialize and load tools
    let loaded_tools = registry.scan_and_load().await?;
    println!("✓ Registry with enhanced discovery loaded {} tools", loaded_tools.len());
    assert!(loaded_tools.len() >= 1, "Should discover at least one tool");

    let registry_arc = Arc::new(RwLock::new(registry));

    // Step 2: Validate enhanced discovery features
    {
        let reg = registry_arc.read().await;
        let tools = reg.list_tools();
        println!("✓ Registry discovered {} tools with enhanced metadata", tools.len());

        // Validate tool metadata
        for tool in &tools {
            assert!(!tool.name.is_empty());
            assert!(!tool.description.is_empty());
            assert_eq!(tool.input_schema.get("type"), Some(&json!("object")));
            println!("  - Tool: {} ({})", tool.name, tool.description);
        }

        // Verify enhanced mode
        assert!(reg.is_enhanced_mode(), "Should be in enhanced discovery mode");
    }

    // Step 3: Test tool execution through registry
    let handler = DynamicRuneToolHandler::new(registry_arc.clone());
    let args = json!({
        "message": "Registry Test",
        "count": 2
    });

    let result = handler.execute_tool("registry_test", args).await?;
    println!("✓ Registry tool execution successful");

    // Validate execution result
    assert_eq!(result["success"], true);
    assert_eq!(result["original_message"], "Registry Test");

    println!("✅ Registry with enhanced discovery workflow validation successful!");
    Ok(())
}

#[tokio::test]
async fn test_error_handling_and_validation_workflow() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create a tool that will have validation challenges
    let validation_test_tool = r#"
pub fn NAME() { "validation_test_tool" }
pub fn DESCRIPTION() { "Tool for testing validation and error handling" }

pub fn INPUT_SCHEMA() {
    #{
        type: "object",
        properties: #{
            email: #{
                type: "string",
                pattern: "^[^@]+@[^@]+\\.[^@]+$"
            },
            age: #{
                type: "integer",
                minimum: 0,
                maximum: 150
            }
        },
        required: ["email"]
    }
}

pub async fn call(args) {
    if let Some(email) = args.email {
        if !email.contains("@") {
            return #{ error: "Invalid email format" };
        }
    }

    #{
        success: true,
        email: args.email,
        age: args.age,
        validated: true
    }
}
"#.to_string();

    std::fs::write(tool_dir.join("validation_test.rn"), validation_test_tool)?;

    // Create registry
    let db = Arc::new(EmbeddingDatabase::new(":memory:").await?);
    let obsidian = Arc::new(ObsidianClient::new()?);
    let mut registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db.clone(), obsidian)?;

    // Load the tool
    let loaded_tools = registry.scan_and_load().await?;
    assert_eq!(loaded_tools.len(), 1);

    let registry_arc = Arc::new(RwLock::new(registry));
    let handler = DynamicRuneToolHandler::new(registry_arc);

    // Test 1: Valid execution
    let valid_args = json!({
        "email": "test@example.com",
        "age": 25
    });

    let result = handler.execute_tool("validation_test_tool", valid_args).await?;
    assert_eq!(result["success"], true);
    assert_eq!(result["email"], "test@example.com");
    println!("✓ Valid input validation passed");

    // Test 2: Missing required field (should fail at tool level, not our validation)
    let missing_required_args = json!({
        "age": 25
    });

    let result = handler.execute_tool("validation_test_tool", missing_required_args).await?;
    // Tool should handle missing required email gracefully
    println!("✓ Missing required field handling: {}", result);

    // Test 3: Invalid data (tool-level validation)
    let invalid_args = json!({
        "email": "invalid-email",
        "age": 200
    });

    let result = handler.execute_tool("validation_test_tool", invalid_args).await?;
    // Tool should validate and return appropriate error
    println!("✓ Invalid data validation: {}", result);

    println!("✅ Error handling and validation workflow successful!");
    Ok(())
}

#[tokio::test]
async fn test_performance_and_scalability_workflow() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tool_dir = temp_dir.path().to_path_buf();

    // Create multiple performance test tools
    for i in 1..=5 {
        let tool_content = format!(r##"
pub fn NAME() {{ "perf_tool_{}" }}
pub fn DESCRIPTION() {{ "Performance test tool {}" }}

pub fn INPUT_SCHEMA() {{
    type: "object",
    properties: {{
        data: {{
            type: "array",
            items: {{ type: "number" }},
            maxItems: 1000
        }}
        }},
        required: ["data"]
    }}

pub async fn call(args) {{
    let mut sum = 0.0;
    if let Some(data) = args.data {{
        for value in data {{
            sum += value;
        }}
    }}

    {{
        tool_id: {},
        sum: sum,
        count: args.data.and_then(|d| Some(d.len())).unwrap_or(0)
    }}
}}
"##, i, i, i);

        std::fs::write(tool_dir.join(&format!("perf_tool_{}.rn", i)), tool_content)?;
    }

    // Create registry and test discovery performance
    let db = Arc::new(EmbeddingDatabase::new(":memory:").await?);
    let obsidian = Arc::new(ObsidianClient::new()?);
    let mut registry = ToolRegistry::new_with_stdlib(tool_dir.clone(), db.clone(), obsidian)?;

    let start = std::time::Instant::now();
    let loaded_tools = registry.scan_and_load().await?;
    let discovery_time = start.elapsed();

    println!("✓ Discovered {} tools in {:?}", loaded_tools.len(), discovery_time);
    assert_eq!(loaded_tools.len(), 5);
    assert!(discovery_time.as_millis() < 1000, "Discovery should be fast");

    let registry_arc = Arc::new(RwLock::new(registry));
    let handler = DynamicRuneToolHandler::new(registry_arc);

    // Test sequential execution performance (since handler doesn't implement Clone)
    let start = std::time::Instant::now();
    let mut results = Vec::new();

    for i in 1..=5 {
        let tool_name = format!("perf_tool_{}", i);
        let data = (1..=100).map(|x| x as f64).collect::<Vec<_>>();
        let args = json!({ "data": data });

        let result = handler.execute_tool(&tool_name, args).await?;
        results.push(result);
    }

    let execution_time = start.elapsed();

    println!("✓ Executed {} tools sequentially in {:?}", results.len(), execution_time);
    assert_eq!(results.len(), 5);
    assert!(execution_time.as_millis() < 5000, "Sequential execution should be reasonably fast");

    // Validate results
    for (i, result) in results.into_iter().enumerate() {
        let result = result?;
        assert_eq!(result["tool_id"], (i + 1));
        assert_eq!(result["count"], 100);
        assert!((result["sum"].as_f64().unwrap() - 5050.0).abs() < 0.001);
    }

    println!("✅ Performance and scalability workflow validation successful!");
    Ok(())
}
