//! Basic Integration Tests for Crucible Tools
//!
//! This module provides essential integration tests that validate the core
//! functionality of the tools layer without complex dependencies.

use std::collections::HashMap;
use std::time::Duration;

use crucible_tools::{
    RuneService, RuneServiceConfig, ToolService, create_tool_manager, init,
    system_tools::ToolManager,
    types::{ContextRef, ServiceResult},
};
use uuid::Uuid;

#[cfg(test)]
mod tools_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_registry_initialization() -> Result<(), Box<dyn std::error::Error>> {
        // Initialize the tool registry
        let registry = init();

        // Verify registry is populated
        assert!(!registry.tools.is_empty());
        println!("Registry initialized with {} tools", registry.tools.len());

        // Verify categories exist
        assert!(!registry.categories.is_empty());
        println!("Registry has {} categories", registry.categories.len());

        // Test getting tools by category
        let system_tools = registry.get_tools_by_category("system");
        assert!(!system_tools.is_empty());
        println!("Found {} system tools", system_tools.len());

        // Test getting all tool names
        let tool_names = registry.list_tools();
        assert!(!tool_names.is_empty());
        println!("Available tools: {:?}", tool_names);

        Ok(())
    }

    #[tokio::test]
    async fn test_tool_manager_integration() -> Result<(), Box<dyn std::error::Error>> {
        // Create tool manager
        let manager = create_tool_manager();

        // Test tool manager operations
        let available_tools = manager.list_tools();
        assert!(!available_tools.is_empty());
        println!("Tool manager has {} tools", available_tools.len());

        // Test getting tool by name
        if let Some(tool_name) = available_tools.first() {
            let tool = manager.get_tool(tool_name);
            assert!(tool.is_some());

            let tool = tool.unwrap();
            assert_eq!(tool.name(), *tool_name);
            assert!(!tool.description().is_empty());

            println!("Tool: {} - {}", tool.name(), tool.description());
        }

        // Test getting tools by category
        let system_tools = manager.get_tools_by_category("system");
        assert!(!system_tools.is_empty());
        println!("Found {} system tools via manager", system_tools.len());

        Ok(())
    }

    #[tokio::test]
    async fn test_rune_service_basic_operations() -> Result<(), Box<dyn std::error::Error>> {
        // Create RuneService
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await?;

        // Test listing tools
        let tools = service.list_tools().await?;
        assert!(!tools.is_empty());
        println!("RuneService has {} tools", tools.len());

        // Test getting tool definitions
        for tool_name in tools.iter().take(3) {
            let tool_def = service.get_tool(tool_name).await?;
            assert!(tool_def.is_some());

            let tool_def = tool_def.unwrap();
            assert_eq!(tool_def.name, *tool_name);
            assert!(!tool_def.description.is_empty());
            assert!(!tool_def.parameters.is_empty());

            println!("Tool definition: {} - {}", tool_def.name, tool_def.description);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_tool_validation() -> Result<(), Box<dyn std::error::Error>> {
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await?;

        // Get available tools
        let tools = service.list_tools().await?;
        assert!(!tools.is_empty());

        // Test tool validation
        for tool_name in tools.iter().take(3) {
            let validation_result = service.validate_tool(tool_name).await?;
            assert!(validation_result.valid, "Tool '{}' should be valid", tool_name);

            if !validation_result.warnings.is_empty() {
                println!("Tool '{}' has warnings: {:?}", tool_name, validation_result.warnings);
            }
        }

        // Test validation of non-existent tool
        let invalid_validation = service.validate_tool("non_existent_tool").await?;
        assert!(!invalid_validation.valid);
        assert!(!invalid_validation.errors.is_empty());

        println!("Non-existent tool validation: {:?}", invalid_validation.errors);

        Ok(())
    }

    #[tokio::test]
    async fn test_service_health_and_metrics() -> Result<(), Box<dyn std::error::Error>> {
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await?;

        // Test service health
        let health = service.service_health().await?;
        println!("Service health: {:?}", health.status);
        assert!(health.message.is_some());

        // Test service metrics
        let metrics = service.get_metrics().await?;
        println!("Service metrics: {:?}", metrics);
        assert!(metrics.uptime >= Duration::ZERO);

        Ok(())
    }

    #[tokio::test]
    async fn test_context_creation() -> Result<(), Box<dyn std::error::Error>> {
        // Create a basic context
        let context = ContextRef::new();
        assert!(!context.id.is_empty());
        assert!(context.created_at <= chrono::Utc::now());

        println!("Created context: {}", context.id);

        // Create context with metadata
        let mut metadata = HashMap::new();
        metadata.insert("user_id".to_string(), serde_json::Value::String("test-user".to_string()));
        metadata.insert("session_id".to_string(), serde_json::Value::String(Uuid::new_v4().to_string()));

        let context_with_metadata = ContextRef {
            id: Uuid::new_v4().to_string(),
            metadata,
            parent_id: None,
            created_at: chrono::Utc::now(),
        };

        assert_eq!(context_with_metadata.metadata.len(), 2);
        println!("Context with metadata: {}", context_with_metadata.id);

        Ok(())
    }

    #[tokio::test]
    async fn test_tool_execution_basic() -> Result<(), Box<dyn std::error::Error>> {
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await?;

        // Get available tools
        let tools = service.list_tools().await?;
        if tools.is_empty() {
            println!("No tools available for execution test");
            return Ok(());
        }

        // Try to execute a simple tool (one that might work with minimal parameters)
        for tool_name in tools.iter().take(3) {
            let tool_def = service.get_tool(tool_name).await?;
            if let Some(tool_def) = tool_def {
                // Check if tool has optional or no parameters
                let has_required_params = tool_def.parameters.iter()
                    .any(|param| param.required && param.default_value.is_none());

                if !has_required_params {
                    println!("Attempting to execute tool: {}", tool_name);

                    // Execute tool with minimal parameters
                    let execution_request = crucible_tools::ToolExecutionRequest {
                        tool_name: tool_name.clone(),
                        parameters: HashMap::new(),
                        context: None,
                        timeout: Some(Duration::from_secs(5)),
                    };

                    match service.execute_tool(execution_request).await {
                        Ok(result) => {
                            println!("Tool '{}' executed successfully", tool_name);
                            println!("Execution ID: {}", result.execution_id);
                            println!("Execution time: {:?}", result.execution_time);
                            break;
                        }
                        Err(e) => {
                            println!("Tool '{}' execution failed: {}", tool_name, e);
                            // Continue to next tool
                        }
                    }
                }
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_error_handling() -> Result<(), Box<dyn std::error::Error>> {
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await?;

        // Test executing non-existent tool
        let execution_request = crucible_tools::ToolExecutionRequest {
            tool_name: "non_existent_tool".to_string(),
            parameters: HashMap::new(),
            context: None,
            timeout: Some(Duration::from_secs(5)),
        };

        let result = service.execute_tool(execution_request).await;
        assert!(result.is_err());
        println!("Non-existent tool execution failed as expected");

        // Test validation of non-existent tool
        let validation_result = service.validate_tool("another_non_existent_tool").await?;
        assert!(!validation_result.valid);
        assert!(!validation_result.errors.is_empty());
        println!("Non-existent tool validation failed as expected");

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_tool_operations() -> Result<(), Box<dyn std::error::Error>> {
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await?;

        // Get available tools
        let tools = service.list_tools().await?;
        if tools.len() < 2 {
            println!("Not enough tools for concurrent test");
            return Ok(());
        }

        // Test concurrent tool validation
        let mut handles = Vec::new();

        for tool_name in tools.into_iter().take(3) {
            let service_clone = service.clone();
            let handle = tokio::spawn(async move {
                service_clone.validate_tool(&tool_name).await
            });
            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            let result = handle.await?;
            results.push(result);
        }

        // Verify all validations completed
        assert_eq!(results.len(), 3);
        for result in results {
            assert!(result.is_ok());
        }

        println!("Concurrent tool validations completed successfully");

        Ok(())
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_listing_performance() -> Result<(), Box<dyn std::error::Error>> {
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await?;

        let start_time = std::time::Instant::now();
        let iterations = 50;

        for _ in 0..iterations {
            let _tools = service.list_tools().await?;
        }

        let total_time = start_time.elapsed();
        let avg_time = total_time / iterations;

        println!("Listed tools {} times in {:?} (avg: {:?})",
                 iterations, total_time, avg_time);

        // Performance assertion - should be fast
        assert!(avg_time < Duration::from_millis(100));

        Ok(())
    }

    #[tokio::test]
    async fn test_tool_validation_performance() -> Result<(), Box<dyn std::error::Error>> {
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await?;

        let tools = service.list_tools().await?;
        if tools.is_empty() {
            return Ok(());
        }

        let tool_name = &tools[0];
        let start_time = std::time::Instant::now();
        let iterations = 100;

        for _ in 0..iterations {
            let _validation = service.validate_tool(tool_name).await?;
        }

        let total_time = start_time.elapsed();
        let avg_time = total_time / iterations;

        println!("Validated tool '{}' {} times in {:?} (avg: {:?})",
                 tool_name, iterations, total_time, avg_time);

        // Performance assertion
        assert!(avg_time < Duration::from_millis(50));

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_performance() -> Result<(), Box<dyn std::error::Error>> {
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await?;

        let tools = service.list_tools().await?;
        if tools.len() < 5 {
            println!("Not enough tools for concurrent performance test");
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        let mut handles = Vec::new();

        // Spawn concurrent tool validations
        for tool_name in tools.into_iter().take(5) {
            let service_clone = service.clone();
            let handle = tokio::spawn(async move {
                service_clone.validate_tool(&tool_name).await
            });
            handles.push(handle);
        }

        // Wait for all to complete
        let results: Vec<_> = futures::future::join_all(handles).await;
        let total_time = start_time.elapsed();

        // Verify all completed successfully
        assert_eq!(results.len(), 5);
        for result in results {
            assert!(result.is_ok());
            assert!(result.unwrap().is_ok());
        }

        println!("Concurrent validation of 5 tools completed in {:?}", total_time);

        // Concurrent should be faster than sequential
        assert!(total_time < Duration::from_secs(5));

        Ok(())
    }
}

#[cfg(test)]
mod stress_tests {
    use super::*;

    #[tokio::test]
    async fn test_rapid_tool_operations() -> Result<(), Box<dyn std::error::Error>> {
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await?;

        let tools = service.list_tools().await?;
        if tools.is_empty() {
            return Ok(());
        }

        let operation_count = 200;
        let tool_name = &tools[0];

        println!("Performing {} rapid validations on tool '{}'", operation_count, tool_name);

        let start_time = std::time::Instant::now();
        let mut success_count = 0;
        let mut error_count = 0;

        for i in 0..operation_count {
            match service.validate_tool(tool_name).await {
                Ok(validation) => {
                    if validation.valid {
                        success_count += 1;
                    } else {
                        error_count += 1;
                    }
                }
                Err(_) => {
                    error_count += 1;
                }
            }

            // Small delay every 50 operations
            if i % 50 == 0 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }

        let total_time = start_time.elapsed();

        println!("Completed {} operations in {:?} (success: {}, errors: {})",
                 operation_count, total_time, success_count, error_count);

        // Most operations should succeed
        assert!(success_count > operation_count * 80 / 100); // At least 80% success rate

        // Service should still be healthy
        let health = service.service_health().await?;
        println!("Final service health: {:?}", health.status);

        Ok(())
    }

    #[tokio::test]
    async fn test_memory_stability() -> Result<(), Box<dyn std::error::Error>> {
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await?;

        let initial_health = service.service_health().await?;
        println!("Initial service status: {:?}", initial_health.status);

        // Perform many operations
        for cycle in 0..10 {
            // List tools
            let _tools = service.list_tools().await?;

            // Validate first few tools
            let tools = service.list_tools().await?;
            for tool_name in tools.iter().take(3) {
                let _validation = service.validate_tool(tool_name).await?;
            }

            // Get health and metrics
            let _health = service.service_health().await?;
            let _metrics = service.get_metrics().await?;

            println!("Cycle {} completed", cycle + 1);

            // Small delay
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Final health check
        let final_health = service.service_health().await?;
        println!("Final service status: {:?}", final_health.status);

        // Service should still be operational
        assert!(matches!(final_health.status, crucible_tools::ServiceStatus::Healthy | crucible_tools::ServiceStatus::Degraded));

        let final_tools = service.list_tools().await?;
        assert!(!final_tools.is_empty());

        println!("Memory stability test passed - service still operational");

        Ok(())
    }
}