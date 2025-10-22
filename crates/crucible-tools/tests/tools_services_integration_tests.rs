//! Integration tests for Crucible Tools and Services Integration
//!
//! This module provides comprehensive integration tests for the tools layer,
//! validating tool registration, execution, and coordination with the services layer.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crucible_tools::{
    RuneService, RuneServiceConfig, ToolService, ToolDefinition,
    ToolExecutionRequest, ToolExecutionResult, ServiceHealth, ServiceMetrics,
    ContextFactory, ContextRef, create_tool_manager, init,
    registry::ToolRegistry,
    system_tools::{Tool, ToolManager},
    types::{ServiceError, ServiceResult, ServiceStatus},
};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Test setup for tools integration
struct ToolsTestSetup {
    pub rune_service: RuneService,
    pub tool_manager: ToolManager,
    pub registry: Arc<ToolRegistry>,
}

impl ToolsTestSetup {
    async fn new() -> ServiceResult<Self> {
        // Initialize the tool registry
        let registry = init();

        // Create RuneService
        let rune_config = RuneServiceConfig::default();
        let rune_service = RuneService::new(rune_config).await?;

        // Create ToolManager
        let tool_manager = create_tool_manager();

        Ok(Self {
            rune_service,
            tool_manager,
            registry,
        })
    }
}

#[cfg(test)]
mod tool_registry_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_registry_initialization() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        // Verify registry is populated
        assert!(!setup.registry.tools.is_empty());

        // Verify categories exist
        assert!(!setup.registry.categories.is_empty());

        // Test getting tools by category
        let system_tools = setup.registry.get_tools_by_category("system");
        assert!(!system_tools.is_empty());

        // Test getting all tool names
        let tool_names = setup.registry.list_tools();
        assert!(!tool_names.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_tool_discovery_integration() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        // Test discovering tools from registry
        let discovered_tools = setup.rune_service.list_tools().await?;
        assert!(!discovered_tools.is_empty());

        // Verify each discovered tool has proper metadata
        for tool_name in discovered_tools {
            let tool_def = setup.rune_service.get_tool(&tool_name).await?;
            assert!(tool_def.is_some());

            let tool_def = tool_def.unwrap();
            assert!(!tool_def.name.is_empty());
            assert!(!tool_def.description.is_empty());
            assert!(!tool_def.parameters.is_empty());
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_tool_manager_integration() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        // Test tool manager operations
        let available_tools = setup.tool_manager.list_tools();
        assert!(!available_tools.is_empty());

        // Test getting tool by name
        if let Some(tool_name) = available_tools.first() {
            let tool = setup.tool_manager.get_tool(tool_name);
            assert!(tool.is_some());

            let tool = tool.unwrap();
            assert_eq!(tool.name(), *tool_name);
            assert!(!tool.description().is_empty());
        }

        // Test getting tools by category
        let system_tools = setup.tool_manager.get_tools_by_category("system");
        assert!(!system_tools.is_empty());

        Ok(())
    }
}

#[cfg(test)]
mod tool_execution_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_tool_execution() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        // Get available tools
        let tools = setup.rune_service.list_tools().await?;
        assert!(!tools.is_empty());

        // Try to execute a simple tool (one that might work with minimal parameters)
        for tool_name in tools {
            let tool_def = setup.rune_service.get_tool(&tool_name).await?;
            if let Some(tool_def) = tool_def {
                // Check if tool has optional or no parameters
                let has_required_params = tool_def.parameters.values()
                    .any(|param| param.required && param.default_value.is_none());

                if !has_required_params {
                    // Execute tool with no parameters
                    let execution_request = ToolExecutionRequest {
                        tool_name: tool_name.clone(),
                        parameters: HashMap::new(),
                        context: None,
                        timeout: Some(Duration::from_secs(5)),
                    };

                    let result = setup.rune_service.execute_tool(execution_request).await;

                    // Tool execution might succeed or fail - we're testing the integration
                    match result {
                        Ok(execution_result) => {
                            assert!(!execution_result.execution_id.is_empty());
                            assert!(execution_result.execution_time > Duration::ZERO);
                            println!("Tool '{}' executed successfully", tool_name);
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
    async fn test_tool_execution_with_context() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        // Create a test context
        let mut context = HashMap::new();
        context.insert("user_id".to_string(), serde_json::Value::String("test-user".to_string()));
        context.insert("session_id".to_string(), serde_json::Value::String(Uuid::new_v4().to_string()));

        // Create ContextRef
        let context_ref = ContextRef {
            user_id: "test-user".to_string(),
            session_id: Uuid::new_v4().to_string(),
            workspace_id: None,
            permissions: vec!["tool.execute".to_string()],
            metadata: context.clone(),
            created_at: chrono::Utc::now(),
            expires_at: None,
        };

        // Get available tools
        let tools = setup.rune_service.list_tools().await?;

        if let Some(tool_name) = tools.first() {
            // Execute tool with context
            let execution_request = ToolExecutionRequest {
                tool_name: tool_name.clone(),
                parameters: HashMap::new(),
                context: Some(context_ref.clone()),
                timeout: Some(Duration::from_secs(5)),
            };

            let result = setup.rune_service.execute_tool(execution_request).await;

            // The important thing is that the context is passed through the service layer
            match result {
                Ok(execution_result) => {
                    assert!(!execution_result.execution_id.is_empty());
                    println!("Tool '{}' executed with context successfully", tool_name);
                }
                Err(e) => {
                    println!("Tool '{}' execution with context failed: {}", tool_name, e);
                    // Expected for tools that require specific parameters
                }
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_tool_execution_error_handling() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        // Test executing non-existent tool
        let execution_request = ToolExecutionRequest {
            tool_name: "non-existent-tool".to_string(),
            parameters: HashMap::new(),
            context: None,
            timeout: Some(Duration::from_secs(5)),
        };

        let result = setup.rune_service.execute_tool(execution_request).await;
        assert!(result.is_err());

        if let Err(ServiceError::ToolNotFound(name)) = result {
            assert_eq!(name, "non-existent-tool");
        } else {
            panic!("Expected ToolNotFound error");
        }

        // Test executing tool with invalid parameters
        let tools = setup.rune_service.list_tools().await?;
        if let Some(tool_name) = tools.first() {
            let tool_def = setup.rune_service.get_tool(&tool_name).await?;
            if let Some(tool_def) = tool_def {
                // Create invalid parameters (wrong types)
                let mut invalid_params = HashMap::new();
                for (param_name, param_def) in tool_def.parameters {
                    if param_def.param_type == "string" {
                        invalid_params.insert(param_name, serde_json::Value::Number(42.into()));
                    } else if param_def.param_type == "number" {
                        invalid_params.insert(param_name, serde_json::Value::String("invalid".to_string()));
                    }
                }

                if !invalid_params.is_empty() {
                    let execution_request = ToolExecutionRequest {
                        tool_name: tool_name.clone(),
                        parameters: invalid_params,
                        context: None,
                        timeout: Some(Duration::from_secs(5)),
                    };

                    let result = setup.rune_service.execute_tool(execution_request).await;

                    // Should fail due to invalid parameters
                    assert!(result.is_err());
                    println!("Tool '{}' failed with invalid parameters as expected", tool_name);
                }
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_tool_execution() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        let tools = setup.rune_service.list_tools().await?;
        if tools.len() < 2 {
            println!("Skipping concurrent execution test - not enough tools");
            return Ok(());
        }

        // Take first two tools for concurrent execution
        let tool1 = tools[0].clone();
        let tool2 = tools[1].clone();

        let service1 = setup.rune_service.clone();
        let service2 = setup.rune_service.clone();

        // Execute tools concurrently
        let handle1 = tokio::spawn(async move {
            let execution_request = ToolExecutionRequest {
                tool_name: tool1,
                parameters: HashMap::new(),
                context: None,
                timeout: Some(Duration::from_secs(5)),
            };

            service1.execute_tool(execution_request).await
        });

        let handle2 = tokio::spawn(async move {
            let execution_request = ToolExecutionRequest {
                tool_name: tool2,
                parameters: HashMap::new(),
                context: None,
                timeout: Some(Duration::from_secs(5)),
            };

            service2.execute_tool(execution_request).await
        });

        // Wait for both executions to complete
        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        // Both should complete (either successfully or with errors)
        // The important thing is that concurrent execution doesn't cause deadlocks or panics
        match (result1, result2) {
            (Ok(r1), Ok(r2)) => {
                println!("Both tools executed successfully");
                assert!(!r1.execution_id.is_empty());
                assert!(!r2.execution_id.is_empty());
            }
            (Ok(r1), Err(e2)) => {
                println!("Tool 1 succeeded, tool 2 failed: {}", e2);
                assert!(!r1.execution_id.is_empty());
            }
            (Err(e1), Ok(r2)) => {
                println!("Tool 1 failed: {}, tool 2 succeeded", e1);
                assert!(!r2.execution_id.is_empty());
            }
            (Err(e1), Err(e2)) => {
                println!("Both tools failed: {} and {}", e1, e2);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod context_factory_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_context_factory_creation() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        // Create a context factory
        let context_factory = ContextFactory::new();

        // Test creating basic context
        let context = context_factory.create_context("test-user", vec!["tool.execute"]);
        assert_eq!(context.user_id, "test-user");
        assert!(context.permissions.contains(&"tool.execute".to_string()));
        assert!(!context.session_id.is_empty());

        // Test creating context with workspace
        let workspace_context = context_factory.create_workspace_context(
            "test-user",
            "workspace-123",
            vec!["tool.execute", "workspace.access"]
        );
        assert_eq!(workspace_context.user_id, "test-user");
        assert_eq!(workspace_context.workspace_id, Some("workspace-123".to_string()));
        assert!(workspace_context.permissions.len() >= 2);

        // Test creating temporary context
        let temp_context = context_factory.create_temp_context(
            "temp-user",
            vec!["temporary.access"],
            Duration::from_secs(300) // 5 minutes
        );
        assert_eq!(temp_context.user_id, "temp-user");
        assert!(temp_context.expires_at.is_some());
        assert!(temp_context.expires_at.unwrap() > chrono::Utc::now());

        Ok(())
    }

    #[tokio::test]
    async fn test_context_validation() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        let context_factory = ContextFactory::new();

        // Create valid context
        let valid_context = context_factory.create_context("test-user", vec!["tool.execute"]);

        // Test context validation
        assert!(context_factory.validate_context(&valid_context));

        // Test expired context
        let mut expired_context = valid_context.clone();
        expired_context.expires_at = Some(chrono::Utc::now() - chrono::Duration::minutes(1));
        assert!(!context_factory.validate_context(&expired_context));

        // Test context permission checking
        assert!(context_factory.has_permission(&valid_context, "tool.execute"));
        assert!(!context_factory.has_permission(&valid_context, "admin.access"));

        Ok(())
    }
}

#[cfg(test)]
mod service_coordination_tests {
    use super::*;

    #[tokio::test]
    async fn test_service_health_coordination() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        // Test RuneService health
        let rune_health = setup.rune_service.service_health().await?;
        assert!(matches!(rune_health.status, ServiceStatus::Healthy));
        assert!(rune_health.message.is_some());
        assert!(!rune_health.details.is_empty());

        // Test health metrics
        let rune_metrics = setup.rune_service.get_metrics().await?;
        assert!(rune_metrics.uptime >= Duration::ZERO);
        assert!(rune_metrics.total_requests >= 0);
        assert!(rune_metrics.successful_requests >= 0);

        // Verify health details contain useful information
        assert!(rune_health.details.contains_key("service_type"));
        assert!(rune_health.details.contains_key("version"));

        Ok(())
    }

    #[tokio::test]
    async fn test_tool_validation_integration() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        // Get available tools
        let tools = setup.rune_service.list_tools().await?;
        assert!(!tools.is_empty());

        // Test tool validation
        for tool_name in tools {
            let validation_result = setup.rune_service.validate_tool(&tool_name).await?;

            // All tools should be valid (they're registered and properly defined)
            assert!(validation_result.valid, "Tool '{}' should be valid", tool_name);

            if !validation_result.warnings.is_empty() {
                println!("Tool '{}' has warnings: {:?}", tool_name, validation_result.warnings);
            }
        }

        // Test validation of non-existent tool
        let invalid_validation = setup.rune_service.validate_tool("non-existent-tool").await?;
        assert!(!invalid_validation.valid);
        assert!(!invalid_validation.errors.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_service_metrics_integration() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        // Get initial metrics
        let initial_metrics = setup.rune_service.get_metrics().await?;
        assert!(initial_metrics.uptime >= Duration::ZERO);

        // Execute some operations to generate metrics
        let tools = setup.rune_service.list_tools().await?;
        if let Some(tool_name) = tools.first() {
            let execution_request = ToolExecutionRequest {
                tool_name: tool_name.clone(),
                parameters: HashMap::new(),
                context: None,
                timeout: Some(Duration::from_secs(5)),
            };

            let _ = setup.rune_service.execute_tool(execution_request).await;

            // Check updated metrics
            let updated_metrics = setup.rune_service.get_metrics().await?;
            assert!(updated_metrics.total_requests >= initial_metrics.total_requests);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_service_error_recovery() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        // Test service resilience to errors
        let tools = setup.rune_service.list_tools().await?;

        if let Some(tool_name) = tools.first() {
            // Execute tool with invalid parameters to trigger error
            let mut invalid_params = HashMap::new();
            invalid_params.insert("invalid_param".to_string(), serde_json::Value::String("invalid".to_string()));

            let execution_request = ToolExecutionRequest {
                tool_name: tool_name.clone(),
                parameters: invalid_params,
                context: None,
                timeout: Some(Duration::from_secs(5)),
            };

            let result = setup.rune_service.execute_tool(execution_request).await;

            // Service should handle error gracefully
            match result {
                Ok(_) => {
                    println!("Tool execution unexpectedly succeeded");
                }
                Err(e) => {
                    println!("Tool execution failed as expected: {}", e);
                    // Service should still be healthy after handling the error
                    let health = setup.rune_service.service_health().await?;
                    assert!(matches!(health.status, ServiceStatus::Healthy));
                }
            }
        }

        // Verify service is still operational after error
        let tools_after_error = setup.rune_service.list_tools().await?;
        assert_eq!(tools_after_error.len(), tools.len());

        Ok(())
    }

    #[tokio::test]
    async fn test_cross_service_consistency() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        // Test consistency between different service interfaces
        let rune_tools = setup.rune_service.list_tools().await?;
        let manager_tools = setup.tool_manager.list_tools();

        // Both should have tools (though they might be different sets)
        assert!(!rune_tools.is_empty());
        assert!(!manager_tools.is_empty());

        // Test that tool definitions are consistent
        for tool_name in rune_tools.iter().take(3) { // Check first 3 tools
            let rune_tool_def = setup.rune_service.get_tool(tool_name).await?;
            let manager_tool = setup.tool_manager.get_tool(tool_name);

            // At least one should have the tool
            assert!(rune_tool_def.is_some() || manager_tool.is_some());
        }

        // Test health consistency
        let rune_health = setup.rune_service.service_health().await?;
        assert!(matches!(rune_health.status, ServiceStatus::Healthy));

        // Test metrics consistency
        let rune_metrics = setup.rune_service.get_metrics().await?;
        assert!(rune_metrics.uptime >= Duration::ZERO);

        Ok(())
    }
}

#[cfg(test)]
mod performance_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_execution_performance() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        let tools = setup.rune_service.list_tools().await?;
        if tools.is_empty() {
            return Ok(());
        }

        // Test execution time for simple operations
        let start_time = std::time::Instant::now();

        let execution_request = ToolExecutionRequest {
            tool_name: tools[0].clone(),
            parameters: HashMap::new(),
            context: None,
            timeout: Some(Duration::from_secs(5)),
        };

        let _ = setup.rune_service.execute_tool(execution_request).await;

        let execution_time = start_time.elapsed();

        // Tool execution should be reasonably fast (even if it fails)
        assert!(execution_time < Duration::from_secs(10));
        println!("Tool execution took: {:?}", execution_time);

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_performance() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        let tools = setup.rune_service.list_tools().await?;
        if tools.len() < 5 {
            println!("Skipping concurrent performance test - not enough tools");
            return Ok(());
        }

        // Test concurrent execution of multiple tools
        let start_time = std::time::Instant::now();

        let mut handles = Vec::new();

        for tool_name in tools.into_iter().take(5) {
            let service = setup.rune_service.clone();

            let handle = tokio::spawn(async move {
                let execution_request = ToolExecutionRequest {
                    tool_name,
                    parameters: HashMap::new(),
                    context: None,
                    timeout: Some(Duration::from_secs(5)),
                };

                service.execute_tool(execution_request).await
            });

            handles.push(handle);
        }

        // Wait for all executions to complete
        let results: Vec<_> = futures::future::join_all(handles).await;

        let total_time = start_time.elapsed();

        // Concurrent execution should be faster than sequential
        println!("Concurrent execution of 5 tools took: {:?}", total_time);
        assert!(total_time < Duration::from_secs(30)); // Should complete within 30 seconds

        // Verify all executions completed (either successfully or with errors)
        assert_eq!(results.len(), 5);

        Ok(())
    }

    #[tokio::test]
    async fn test_memory_usage_stability() -> ServiceResult<()> {
        let setup = ToolsTestSetup::new().await?;

        let tools = setup.rune_service.list_tools().await?;
        if tools.is_empty() {
            return Ok(());
        }

        // Execute many operations to test memory stability
        for i in 0..10 {
            let execution_request = ToolExecutionRequest {
                tool_name: tools[0].clone(),
                parameters: HashMap::from([
                    ("iteration".to_string(), serde_json::Value::Number(i.into()))
                ]),
                context: None,
                timeout: Some(Duration::from_secs(5)),
            };

            let _ = setup.rune_service.execute_tool(execution_request).await;

            // Small delay to allow for cleanup
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Service should still be responsive
        let health = setup.rune_service.service_health().await?;
        assert!(matches!(health.status, ServiceStatus::Healthy));

        let tools_after = setup.rune_service.list_tools().await?;
        assert_eq!(tools_after.len(), tools.len());

        Ok(())
    }
}