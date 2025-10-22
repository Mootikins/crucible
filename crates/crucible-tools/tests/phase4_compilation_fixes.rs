//! Comprehensive tests for Phase 4 compilation fixes
//!
//! This test module validates all the compilation fixes completed in Phase 4:
//! - Phase 4.1: Critical compilation errors using Option A approach
//! - Phase 4.2: Unit tests for crucible-tools compilation fixes
//! - Phase 4.3: ToolDefinition struct initialization with missing required fields
//! - Phase 4.4: ContextRef migration (context -> context_ref field renames)
//! - Phase 4.5: Missing trait implementations (Debug, Clone for Rune types)
//! - Phase 4.6: String vs &str type mismatches throughout codebase
//! - Phase 4.7: Method call issues (.ok_or_else() vs .or_else(), missing .await)
//! - Phase 4.8: Removed or fixed missing module references (registry module)
//! - Phase 4.9: Updated constructor signatures to match actual API requirements

use crucible_tools::*;
use std::collections::HashMap;
use std::time::Duration;
use serde_json::{json, Value};
use tempfile::TempDir;
use std::fs;

#[cfg(test)]
mod phase4_3_tool_definition_fixes {
    use super::*;

    /// Test that ToolDefinition can be properly initialized with all required fields
    /// This validates Phase 4.3 fixes for missing required fields
    #[test]
    fn test_tool_definition_complete_initialization() {
        let tool = ToolDefinition {
            name: "complete_test_tool".to_string(),
            description: "A complete test tool with all fields".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "Message to process"
                    },
                    "count": {
                        "type": "integer",
                        "description": "Number of repetitions",
                        "default": 1
                    }
                },
                "required": ["message"]
            }),
            category: Some("Test".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Test Author".to_string()),
            tags: vec!["test".to_string(), "example".to_string()],
            enabled: true,
            parameters: vec![
                ToolParameter {
                    name: "message".to_string(),
                    param_type: "string".to_string(),
                    description: "Message to process".to_string(),
                    required: true,
                    default_value: None,
                },
                ToolParameter {
                    name: "count".to_string(),
                    param_type: "integer".to_string(),
                    description: "Number of repetitions".to_string(),
                    required: false,
                    default_value: Some(json!(1)),
                },
            ],
        };

        // Verify all fields are correctly set
        assert_eq!(tool.name, "complete_test_tool");
        assert_eq!(tool.description, "A complete test tool with all fields");
        assert!(tool.input_schema.is_object());
        assert_eq!(tool.category, Some("Test".to_string()));
        assert_eq!(tool.version, Some("1.0.0".to_string()));
        assert_eq!(tool.author, Some("Test Author".to_string()));
        assert_eq!(tool.tags, vec!["test".to_string(), "example".to_string()]);
        assert!(tool.enabled);
        assert_eq!(tool.parameters.len(), 2);
    }

    /// Test ToolDefinition with minimal required fields
    #[test]
    fn test_tool_definition_minimal_initialization() {
        let tool = ToolDefinition {
            name: "minimal_tool".to_string(),
            description: "Minimal tool definition".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            category: None,
            version: None,
            author: None,
            tags: vec![],
            enabled: true,
            parameters: vec![],
        };

        assert_eq!(tool.name, "minimal_tool");
        assert_eq!(tool.description, "Minimal tool definition");
        assert!(tool.category.is_none());
        assert!(tool.version.is_none());
        assert!(tool.author.is_none());
        assert!(tool.tags.is_empty());
        assert!(tool.enabled);
        assert!(tool.parameters.is_empty());
    }

    /// Test ToolParameter creation and validation
    #[test]
    fn test_tool_parameter_creation() {
        let required_param = ToolParameter {
            name: "required_field".to_string(),
            param_type: "string".to_string(),
            description: "A required field".to_string(),
            required: true,
            default_value: None,
        };

        let optional_param = ToolParameter {
            name: "optional_field".to_string(),
            param_type: "integer".to_string(),
            description: "An optional field".to_string(),
            required: false,
            default_value: Some(json!(42)),
        };

        assert_eq!(required_param.name, "required_field");
        assert_eq!(required_param.param_type, "string");
        assert!(required_param.required);
        assert!(required_param.default_value.is_none());

        assert_eq!(optional_param.name, "optional_field");
        assert_eq!(optional_param.param_type, "integer");
        assert!(!optional_param.required);
        assert_eq!(optional_param.default_value, Some(json!(42)));
    }
}

#[cfg(test)]
mod phase4_4_context_ref_migration {
    use super::*;
    use chrono::Utc;

    /// Test that ContextRef works correctly after context -> context_ref migration
    /// This validates Phase 4.4 fixes for ContextRef field migration
    #[test]
    fn test_context_ref_creation_and_methods() {
        let context = ContextRef::new();

        // Test basic properties
        assert!(!context.id.is_empty());
        assert!(context.metadata.is_empty());
        assert!(context.parent_id.is_none());
        assert!(context.created_at.timestamp() > 0);

        // Test child context creation
        let child_context = context.child();
        assert_ne!(child_context.id, context.id);
        assert_eq!(child_context.parent_id, Some(context.id));
        assert!(child_context.metadata.is_empty());
    }

    /// Test ContextRef with metadata
    #[test]
    fn test_context_ref_with_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("tool_name".to_string(), json!("test_tool"));
        metadata.insert("execution_id".to_string(), json!("exec-123"));

        let context = ContextRef::with_metadata(metadata.clone());

        assert_eq!(context.metadata.get("tool_name"), Some(&json!("test_tool")));
        assert_eq!(context.metadata.get("execution_id"), Some(&json!("exec-123")));
    }

    /// Test ContextRef metadata operations
    #[test]
    fn test_context_ref_metadata_operations() {
        let mut context = ContextRef::new();

        // Test adding metadata
        context.add_metadata("test_key".to_string(), json!("test_value"));
        assert_eq!(context.get_metadata("test_key"), Some(&json!("test_value")));

        // Test getting non-existent metadata
        assert_eq!(context.get_metadata("non_existent"), None);

        // Test overwriting metadata
        context.add_metadata("test_key".to_string(), json!("new_value"));
        assert_eq!(context.get_metadata("test_key"), Some(&json!("new_value")));
    }

    /// Test ToolExecutionContext with ContextRef integration
    #[test]
    fn test_tool_execution_context_with_context_ref() {
        let context_ref = ContextRef::new();
        let mut environment = HashMap::new();
        environment.insert("PATH".to_string(), "/usr/bin".to_string());

        let execution_context = ToolExecutionContext {
            execution_id: "test-exec-123".to_string(),
            context_ref: Some(context_ref.clone()),
            timeout: Some(Duration::from_secs(60)),
            environment,
            user_context: Some(json!({"user_id": "user-123"})),
            service_context: Some(json!({"service": "test"})),
            started_at: Utc::now(),
        };

        assert_eq!(execution_context.execution_id, "test-exec-123");
        assert_eq!(execution_context.context_ref, Some(context_ref));
        assert_eq!(execution_context.timeout, Some(Duration::from_secs(60)));
        assert_eq!(execution_context.environment.get("PATH"), Some(&"/usr/bin".to_string()));
    }

    /// Test ToolExecutionResult with ContextRef
    #[test]
    fn test_tool_execution_result_with_context_ref() {
        let context_ref = ContextRef::new();
        let mut metadata = HashMap::new();
        metadata.insert("execution_time_ms".to_string(), json!(150));

        let result = ToolExecutionResult {
            success: true,
            result: Some(json!({"output": "success"})),
            error: None,
            duration: Duration::from_millis(150),
            completed_at: Utc::now(),
            context_ref: Some(context_ref),
            metadata,
        };

        assert!(result.success);
        assert!(result.context_ref.is_some());
        assert_eq!(result.metadata.get("execution_time_ms"), Some(&json!(150)));
    }
}

#[cfg(test)]
mod phase4_5_trait_implementations {
    use super::*;

    /// Test that ContextRef implements Debug and Clone traits
    /// This validates Phase 4.5 fixes for missing trait implementations
    #[test]
    fn test_context_ref_debug_and_clone() {
        let mut metadata = HashMap::new();
        metadata.insert("test".to_string(), json!("value"));

        let context = ContextRef {
            id: "test-123".to_string(),
            metadata,
            parent_id: None,
            created_at: Utc::now(),
        };

        // Test Debug trait
        let debug_output = format!("{:?}", context);
        assert!(debug_output.contains("ContextRef"));
        assert!(debug_output.contains("test-123"));

        // Test Clone trait
        let cloned_context = context.clone();
        assert_eq!(context.id, cloned_context.id);
        assert_eq!(context.metadata, cloned_context.metadata);
        assert_eq!(context.parent_id, cloned_context.parent_id);
    }

    /// Test that ToolParameter implements Debug and Clone
    #[test]
    fn test_tool_parameter_debug_and_clone() {
        let param = ToolParameter {
            name: "test_param".to_string(),
            param_type: "string".to_string(),
            description: "Test parameter".to_string(),
            required: true,
            default_value: Some(json!("default")),
        };

        // Test Debug trait
        let debug_output = format!("{:?}", param);
        assert!(debug_output.contains("ToolParameter"));
        assert!(debug_output.contains("test_param"));

        // Test Clone trait
        let cloned_param = param.clone();
        assert_eq!(param.name, cloned_param.name);
        assert_eq!(param.param_type, cloned_param.param_type);
        assert_eq!(param.description, cloned_param.description);
        assert_eq!(param.required, cloned_param.required);
        assert_eq!(param.default_value, cloned_param.default_value);
    }

    /// Test that ToolDefinition implements Debug and Clone
    #[test]
    fn test_tool_definition_debug_and_clone() {
        let tool = ToolDefinition {
            name: "debug_test_tool".to_string(),
            description: "Tool for testing debug and clone".to_string(),
            input_schema: json!({"type": "object"}),
            category: Some("Test".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Test Author".to_string()),
            tags: vec!["test".to_string(), "debug".to_string()],
            enabled: true,
            parameters: vec![],
        };

        // Test Debug trait
        let debug_output = format!("{:?}", tool);
        assert!(debug_output.contains("ToolDefinition"));
        assert!(debug_output.contains("debug_test_tool"));

        // Test Clone trait
        let cloned_tool = tool.clone();
        assert_eq!(tool.name, cloned_tool.name);
        assert_eq!(tool.description, cloned_tool.description);
        assert_eq!(tool.category, cloned_tool.category);
        assert_eq!(tool.tags, cloned_tool.tags);
        assert_eq!(tool.enabled, cloned_tool.enabled);
    }

    /// Test that ServiceError implements Debug
    #[test]
    fn test_service_error_debug() {
        let error = ServiceError::ToolNotFound("missing_tool".to_string());
        let debug_output = format!("{:?}", error);
        assert!(debug_output.contains("ToolNotFound"));
        assert!(debug_output.contains("missing_tool"));

        let exec_error = ServiceError::ExecutionError("execution failed".to_string());
        let debug_output2 = format!("{:?}", exec_error);
        assert!(debug_output2.contains("ExecutionError"));
        assert!(debug_output2.contains("execution failed"));
    }
}

#[cfg(test)]
mod phase4_6_string_str_conversions {
    use super::*;

    /// Test String vs &str type handling in various contexts
    /// This validates Phase 4.6 fixes for String vs &str type mismatches
    #[test]
    fn test_string_str_conversions_in_context_ref() {
        let context_ref = ContextRef::new();

        // Test that get_metadata works with &str
        let mut context = ContextRef::new();
        context.add_metadata("key_str".to_string(), json!("value"));

        // Should work with &str
        assert!(context.get_metadata("key_str").is_some());

        // Should also work with String
        let key_string = "key_str".to_string();
        assert!(context.get_metadata(&key_string).is_some());
    }

    /// Test string handling in ToolExecutionRequest
    #[test]
    fn test_string_str_handling_in_execution_request() {
        let tool_name = "test_tool".to_string();
        let tool_name_str = "test_tool";

        // Test that both String and &str work for tool names
        let request1 = ToolExecutionRequest::simple(
            tool_name.clone(),
            json!({"test": true})
        );

        let request2 = ToolExecutionRequest::simple(
            tool_name_str.to_string(),
            json!({"test": true})
        );

        assert_eq!(request1.tool_name, request2.tool_name);
    }

    /// Test string handling in ValidationResult
    #[test]
    fn test_string_str_handling_in_validation_result() {
        let tool_name_str = "test_tool";
        let tool_name_string = "test_tool".to_string();

        let result1 = ValidationResult::valid(tool_name_string.clone());
        let result2 = ValidationResult::valid(tool_name_str.to_string());

        assert_eq!(result1.tool_name, result2.tool_name);
        assert!(result1.valid);
        assert!(result2.valid);

        // Test with warnings
        let result3 = result1.with_warning("test warning".to_string());
        assert_eq!(result3.warnings.len(), 1);
        assert_eq!(result3.warnings[0], "test warning");
    }
}

#[cfg(test)]
mod phase4_7_method_call_fixes {
    use super::*;

    /// Test method call patterns and error handling
    /// This validates Phase 4.7 fixes for method call issues
    #[tokio::test]
    async fn test_async_method_calls_with_await() {
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await.unwrap();

        // Test that async methods work properly with .await
        let tools = service.list_tools().await.unwrap();
        assert!(tools.is_empty()); // No tools discovered yet

        // Test service health check
        let health = service.service_health().await.unwrap();
        assert_eq!(health.status, ServiceStatus::Healthy);

        // Test metrics
        let metrics = service.get_metrics().await.unwrap();
        assert_eq!(metrics.total_requests, 0);
    }

    /// Test error handling with proper method chaining
    #[test]
    fn test_error_handling_method_chaining() {
        // Test ServiceError creation and handling
        let tool_not_found = ServiceError::ToolNotFound("missing".to_string());
        assert!(matches!(tool_not_found, ServiceError::ToolNotFound(_)));

        // Test Result type handling
        let result: Result<Value, ServiceError> = Err(ServiceError::ValidationError("test error".to_string()));

        // Test that or_else and ok_or_else patterns work
        let mapped_result = result.or_else(|e| {
            match e {
                ServiceError::ValidationError(msg) => Ok(json!({"error": msg})),
                other => Err(other),
            }
        });

        assert!(mapped_result.is_ok());
        assert_eq!(mapped_result.unwrap()["error"], "test error");
    }

    /// Test method calls on Option types
    #[test]
    fn test_option_method_calls() {
        let context_ref = Some(ContextRef::new());

        // Test that we can call methods on Option properly
        let id = context_ref.as_ref().map(|c| c.id.clone());
        assert!(id.is_some());

        let no_context: Option<ContextRef> = None;
        let id_or_default = no_context.as_ref()
            .map(|c| c.id.clone())
            .unwrap_or_else(|| "default-id".to_string());
        assert_eq!(id_or_default, "default-id");
    }
}

#[cfg(test)]
mod phase4_8_module_independence {
    use super::*;

    /// Test that crucible-tools works as a self-contained module
    /// This validates Phase 4.8 fixes for module independence
    #[test]
    fn test_registry_module_independence() {
        // Test that we can create and use registry without external dependencies
        let mut registry = crucible_tools::registry::ToolRegistry::new();

        let tool = ToolDefinition {
            name: "independent_tool".to_string(),
            description: "Tool created independently".to_string(),
            input_schema: json!({"type": "object"}),
            category: Some("Test".to_string()),
            version: Some("1.0.0".to_string()),
            author: None,
            tags: vec![],
            enabled: true,
            parameters: vec![],
        };

        registry.register_tool(tool);

        // Test registry operations work independently
        assert_eq!(registry.list_tools().len(), 1);
        assert!(registry.get_tool("independent_tool").is_some());

        let stats = registry.get_stats();
        assert_eq!(stats.total_tools, 1);
        assert_eq!(stats.categories, 1);
    }

    /// Test tool manager creation without external registry
    #[test]
    fn test_tool_manager_independence() {
        let manager = crucible_tools::system_tools::ToolManager::new();

        // Should work without external dependencies
        let tools = manager.list_tools();
        assert!(tools.is_empty()); // No tools registered yet

        // Search should work without errors
        let search_results = manager.search_tools("nonexistent");
        assert!(search_results.is_empty());
    }

    /// Test context factory independence
    #[test]
    fn test_context_factory_independence() {
        let factory = crucible_tools::ContextFactory::new().unwrap();

        // Should be able to create contexts independently
        let context = factory.create_fresh_context("test_tool").unwrap();
        assert!(!context.id.is_empty());
    }
}

#[cfg(test)]
mod phase4_9_constructor_signatures {
    use super::*;

    /// Test updated constructor signatures work correctly
    /// This validates Phase 4.9 fixes for constructor signature updates
    #[test]
    fn test_rune_service_config_constructor() {
        let config = RuneServiceConfig::default();

        // Test default constructor works
        assert_eq!(config.service_name, "crucible-rune");
        assert_eq!(config.version, "1.0.0");
        assert!(!config.discovery.tool_directories.is_empty());
        assert!(config.hot_reload.enabled);
        assert_eq!(config.execution.default_timeout_ms, 30000);
    }

    /// Test custom constructor parameters
    #[test]
    fn test_rune_service_config_custom_constructor() {
        let mut config = RuneServiceConfig::default();
        config.service_name = "custom-service".to_string();
        config.version = "2.0.0".to_string();
        config.execution.default_timeout_ms = 60000;

        assert_eq!(config.service_name, "custom-service");
        assert_eq!(config.version, "2.0.0");
        assert_eq!(config.execution.default_timeout_ms, 60000);
    }

    /// Test ToolExecutionRequest constructors
    #[test]
    fn test_tool_execution_request_constructors() {
        let context = ToolExecutionContext::default();

        // Test full constructor
        let request1 = ToolExecutionRequest::new(
            "test_tool".to_string(),
            json!({"param": "value"}),
            context.clone()
        );

        assert_eq!(request1.tool_name, "test_tool");
        assert_eq!(request1.parameters["param"], "value");
        assert!(!request1.request_id.is_empty());

        // Test simple constructor
        let request2 = ToolExecutionRequest::simple(
            "simple_tool".to_string(),
            json!({"simple": true})
        );

        assert_eq!(request2.tool_name, "simple_tool");
        assert_eq!(request2.parameters["simple"], true);
        assert!(!request2.request_id.is_empty());
        assert_eq!(request2.context.execution_id, request2.context.context_ref.as_ref().unwrap().id);
    }

    /// Test ContextRef constructors
    #[test]
    fn test_context_ref_constructors() {
        // Test default constructor
        let context1 = ContextRef::new();
        assert!(!context1.id.is_empty());
        assert!(context1.metadata.is_empty());
        assert!(context1.parent_id.is_none());

        // Test with metadata constructor
        let mut metadata = HashMap::new();
        metadata.insert("test".to_string(), json!("value"));
        let context2 = ContextRef::with_metadata(metadata);
        assert_eq!(context2.metadata.get("test"), Some(&json!("value")));

        // Test child constructor
        let child = context1.child();
        assert_ne!(child.id, context1.id);
        assert_eq!(child.parent_id, Some(context1.id));
    }
}

#[cfg(test)]
mod integration_tests_phase4 {
    use super::*;

    /// Test integration scenarios with multiple Phase 4 fixes
    #[tokio::test]
    async fn test_end_to_end_rune_service_with_phase4_fixes() {
        // Create temporary directory with test tool
        let temp_dir = TempDir::new().unwrap();
        let tool_path = temp_dir.path().join("integration_test_tool.rn");

        let tool_source = r#"
            pub fn NAME() { "integration_test_tool" }
            pub fn DESCRIPTION() { "Tool for testing Phase 4 integration fixes" }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: {
                        message: { type: "string" },
                        count: { type: "integer", default: 1 }
                    },
                    required: ["message"]
                }
            }
            pub async fn call(args) {
                #{
                    success: true,
                    result: {
                        echoed_message: args.message,
                        repeat_count: args.count.unwrap_or(1),
                        timestamp: time::now()
                    }
                }
            }
        "#;

        fs::write(&tool_path, tool_source).unwrap();

        // Configure service with temp directory
        let mut config = RuneServiceConfig::default();
        config.service_name = "phase4-integration-test".to_string();
        config.discovery.tool_directories.clear();
        config.discovery.tool_directories.push(temp_dir.path().to_path_buf());

        // Create service (tests constructor fixes)
        let service = RuneService::new(config).await.unwrap();

        // Discover tools (tests discovery fixes)
        let discovered_count = service.discover_tools_from_directory(temp_dir.path()).await.unwrap();
        assert_eq!(discovered_count, 1);

        // List tools (tests tool definition fixes)
        let tools = service.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "integration_test_tool");
        assert!(tools[0].description.contains("Phase 4"));

        // Create execution request (tests constructor fixes)
        let context = ToolExecutionContext::default();
        let request = ToolExecutionRequest::new(
            "integration_test_tool".to_string(),
            json!({
                "message": "Phase 4 integration test",
                "count": 3
            }),
            context
        );

        // Execute tool (tests async/await fixes)
        let result = service.execute_tool(request).await.unwrap();
        assert!(result.success);

        if let Some(output) = result.result {
            assert_eq!(output["result"]["echoed_message"], "Phase 4 integration test");
            assert_eq!(output["result"]["repeat_count"], 3);
        }

        // Test service health (tests method call fixes)
        let health = service.service_health().await.unwrap();
        assert_eq!(health.status, ServiceStatus::Healthy);

        // Test tool validation (tests validation fixes)
        let validation = service.validate_tool("integration_test_tool").await.unwrap();
        assert!(validation.valid);
        assert!(validation.errors.is_empty());
    }

    /// Test tool registry with all Phase 4 fixes applied
    #[test]
    fn test_registry_with_all_phase4_fixes() {
        let mut registry = crucible_tools::registry::ToolRegistry::new();

        // Create tool with all required fields (Phase 4.3 fix)
        let tool = ToolDefinition {
            name: "registry_test_tool".to_string(),
            description: "Tool testing all Phase 4 registry fixes".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string" }
                },
                "required": ["input"]
            }),
            category: Some("RegistryTest".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Phase 4 Test".to_string()),
            tags: vec!["registry".to_string(), "phase4".to_string(), "test".to_string()],
            enabled: true,
            parameters: vec![
                ToolParameter {
                    name: "input".to_string(),
                    param_type: "string".to_string(),
                    description: "Input parameter".to_string(),
                    required: true,
                    default_value: None,
                }
            ],
        };

        // Register tool (tests registration without external dependencies - Phase 4.8)
        registry.register_tool(tool);

        // Test registry operations
        let registered_tools = registry.list_tools();
        assert_eq!(registered_tools.len(), 1);

        let retrieved_tool = registry.get_tool("registry_test_tool");
        assert!(retrieved_tool.is_some());

        let tool = retrieved_tool.unwrap();
        assert_eq!(tool.name, "registry_test_tool");
        assert_eq!(tool.tags.len(), 3);
        assert!(tool.tags.contains(&"phase4".to_string()));

        // Test category operations
        let category_tools = registry.list_tools_by_category(&ToolCategory::System); // Should be empty
        assert!(category_tools.is_empty());

        // Test stats (tests struct field access - Phase 4.5 Debug traits)
        let stats = registry.get_stats();
        assert_eq!(stats.total_tools, 1);
        assert!(format!("{:?}", stats).contains("RegistryStats"));
    }

    /// Test error handling scenarios with Phase 4 fixes
    #[tokio::test]
    async fn test_error_handling_with_phase4_fixes() {
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await.unwrap();

        // Test tool not found error (tests error creation and handling)
        let request = ToolExecutionRequest::simple(
            "nonexistent_tool".to_string(),
            json!({})
        );

        let result = service.execute_tool(request).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(matches!(error, ServiceError::ToolNotFound(_)));
        assert!(error.to_string().contains("nonexistent_tool"));

        // Test validation of nonexistent tool
        let validation = service.validate_tool("nonexistent_tool").await.unwrap();
        assert!(!validation.valid);
        assert_eq!(validation.errors.len(), 1);
        assert!(validation.errors[0].contains("not found"));

        // Test that error objects are properly formatted (Debug trait - Phase 4.5)
        let error_debug = format!("{:?}", error);
        assert!(error_debug.contains("ToolNotFound"));
    }
}