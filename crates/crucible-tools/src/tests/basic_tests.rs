//! Basic tests to verify Phase 4.1 fixes work correctly
//!
//! This module contains simple tests that verify the core functionality
//! of the Phase 4.1 fixes without complex dependencies.

#[cfg(test)]
mod basic_type_tests {
    use crucible_services::types::tool::*;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::time::Duration;

    #[test]
    fn test_context_ref_basic_functionality() {
        let context = ContextRef::new();

        assert!(!context.id.is_empty());
        assert!(context.metadata.is_empty());
        assert!(context.parent_id.is_none());
    }

    #[test]
    fn test_context_ref_with_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("tool_name".to_string(), "test_tool".to_string());

        let context = ContextRef::with_metadata(metadata);

        assert!(!context.id.is_empty());
        assert_eq!(context.metadata.len(), 1);
        assert_eq!(context.metadata.get("tool_name"), Some(&"test_tool".to_string()));
    }

    #[test]
    fn test_tool_execution_context_default() {
        let context = ToolExecutionContext::default();

        assert!(!context.execution_id.is_empty());
        assert!(context.context_ref.is_some());
        assert_eq!(context.timeout, Some(Duration::from_secs(30)));
        assert!(context.environment.is_empty());
    }

    #[test]
    fn test_tool_definition_creation() {
        let tool = ToolDefinition {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message": {"type": "string"}
                }
            }),
            category: Some("Test".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Test Author".to_string()),
            tags: vec!["test".to_string()],
            enabled: true,
            parameters: vec![],
        };

        assert_eq!(tool.name, "test_tool");
        assert_eq!(tool.description, "A test tool");
        assert_eq!(tool.category, Some("Test".to_string()));
        assert_eq!(tool.version, Some("1.0.0".to_string()));
        assert_eq!(tool.author, Some("Test Author".to_string()));
        assert!(tool.enabled);
    }

    #[test]
    fn test_tool_execution_result_success() {
        let result = ToolExecutionResult::success(
            json!({"output": "success"}),
            Duration::from_millis(100),
            "test_tool".to_string(),
            Some(ContextRef::new()),
        );

        assert!(result.success);
        assert_eq!(result.result, Some(json!({"output": "success"})));
        assert!(result.error.is_none());
        assert_eq!(result.tool_name, "test_tool");
    }

    #[test]
    fn test_tool_execution_result_error() {
        let result = ToolExecutionResult::error(
            "Something went wrong".to_string(),
            Duration::from_millis(50),
            "error_tool".to_string(),
            Some(ContextRef::new()),
        );

        assert!(!result.success);
        assert!(result.result.is_none());
        assert_eq!(result.error, Some("Something went wrong".to_string()));
        assert_eq!(result.tool_name, "error_tool");
    }

    #[test]
    fn test_validation_result() {
        let valid_result = ValidationResult::valid();
        assert!(valid_result.valid);
        assert!(valid_result.errors.is_empty());

        let invalid_result = ValidationResult::invalid(vec!["Error message".to_string()]);
        assert!(!invalid_result.valid);
        assert_eq!(invalid_result.errors.len(), 1);
        assert_eq!(invalid_result.errors[0], "Error message");
    }

    #[test]
    fn test_tool_category_parsing() {
        assert!("system".parse::<ToolCategory>().unwrap() == ToolCategory::System);
        assert!("database".parse::<ToolCategory>().unwrap() == ToolCategory::Database);
        assert!("ai".parse::<ToolCategory>().unwrap() == ToolCategory::AI);

        let result: Result<ToolCategory, _> = "invalid".parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_execution_stats() {
        let mut stats = ToolExecutionStats::new("test_tool".to_string());

        assert_eq!(stats.tool_name, "test_tool");
        assert_eq!(stats.total_executions, 0);
        assert_eq!(stats.successful_executions, 0);
        assert_eq!(stats.failed_executions, 0);

        // Record a successful execution
        stats.record_execution(100, true);
        assert_eq!(stats.total_executions, 1);
        assert_eq!(stats.successful_executions, 1);
        assert_eq!(stats.failed_executions, 0);
        assert_eq!(stats.avg_execution_time_ms, 100.0);

        // Record a failed execution
        stats.record_execution(200, false);
        assert_eq!(stats.total_executions, 2);
        assert_eq!(stats.successful_executions, 1);
        assert_eq!(stats.failed_executions, 1);
        assert_eq!(stats.avg_execution_time_ms, 150.0);
    }
}

#[cfg(test)]
mod basic_registry_tests {
    use crucible_tools::registry::*;
    use crucible_services::types::tool::*;
    use serde_json::json;

    fn create_simple_tool(name: &str, category: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: format!("Test tool: {}", name),
            input_schema: json!({"type": "object"}),
            category: Some(category.to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Test Suite".to_string()),
            tags: vec!["test".to_string()],
            enabled: true,
            parameters: vec![],
        }
    }

    #[test]
    fn test_registry_creation_and_basic_operations() {
        let mut registry = ToolRegistry::new();

        assert_eq!(registry.tools.len(), 0);
        assert_eq!(registry.categories.len(), 0);

        let tool = create_simple_tool("test_tool", "System");
        registry.register_tool(tool);

        assert_eq!(registry.tools.len(), 1);
        assert!(registry.get_tool("test_tool").is_some());
    }

    #[test]
    fn test_registry_multiple_tools() {
        let mut registry = ToolRegistry::new();

        let tools = vec![
            create_simple_tool("tool1", "System"),
            create_simple_tool("tool2", "Database"),
            create_simple_tool("tool3", "System"),
        ];

        for tool in tools {
            registry.register_tool(tool);
        }

        assert_eq!(registry.tools.len(), 3);
        assert!(registry.get_tool("tool1").is_some());
        assert!(registry.get_tool("tool2").is_some());
        assert!(registry.get_tool("tool3").is_some());
    }

    #[test]
    fn test_registry_stats() {
        let mut registry = ToolRegistry::new();

        let tool = create_simple_tool("stats_tool", "Test");
        registry.register_tool(tool);

        let stats = registry.get_stats();
        assert_eq!(stats.total_tools, 1);
        assert_eq!(stats.tools_with_dependencies, 0); // Simplified for basic test
    }

    #[test]
    fn test_registry_initialization() {
        let registry = super::super::registry::initialize_registry();

        // Should have built-in tools
        assert!(registry.tools.len() > 0);

        // Should have specific expected tools
        assert!(registry.get_tool("system_info").is_some());
        assert!(registry.get_tool("vault_search").is_some());
        assert!(registry.get_tool("database_query").is_some());
        assert!(registry.get_tool("semantic_search").is_some());
    }
}

#[cfg(test)]
mod integration_basic_tests {
    use super::*;

    #[test]
    fn test_end_to_end_tool_lifecycle() {
        // Create a tool definition
        let tool = ToolDefinition {
            name: "lifecycle_test_tool".to_string(),
            description: "Tool for testing lifecycle".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message": {"type": "string"}
                }
            }),
            category: Some("Test".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Integration Test".to_string()),
            tags: vec!["integration".to_string(), "test".to_string()],
            enabled: true,
            parameters: vec![],
        };

        // Register the tool
        let mut registry = ToolRegistry::new();
        registry.register_tool(tool.clone());

        // Verify registration
        assert_eq!(registry.tools.len(), 1);
        let retrieved_tool = registry.get_tool("lifecycle_test_tool").unwrap();
        assert_eq!(retrieved_tool.name, "lifecycle_test_tool");
        assert_eq!(retrieved_tool.description, "Tool for testing lifecycle");
        assert_eq!(retrieved_tool.category, Some("Test".to_string()));

        // Create execution context
        let context = ContextRef::with_metadata({
            let mut metadata = std::collections::HashMap::new();
            metadata.insert("test_name".to_string(), "lifecycle_test".to_string());
            metadata
        });

        let exec_context = ToolExecutionContext {
            execution_id: "test-execution".to_string(),
            context_ref: Some(context),
            timeout: Some(Duration::from_secs(30)),
            environment: std::collections::HashMap::new(),
            user_context: Some(json!({"user_id": "test_user"})),
            service_context: Some(json!({"service": "test_service"})),
            started_at: chrono::Utc::now(),
        };

        // Verify execution context
        assert_eq!(exec_context.execution_id, "test-execution");
        assert!(exec_context.context_ref.is_some());
        assert!(exec_context.user_context.is_some());
        assert!(exec_context.service_context.is_some());

        // Create execution request
        let request = ToolExecutionRequest::new(
            tool.name.clone(),
            json!({"message": "test message"}),
            exec_context,
        );

        // Verify request
        assert_eq!(request.tool_name, "lifecycle_test_tool");
        assert_eq!(request.parameters, json!({"message": "test message"}));
        assert!(!request.request_id.is_empty());

        // Mock execution result
        let result = ToolExecutionResult::success(
            json!({"processed": "test message", "status": "ok"}),
            Duration::from_millis(75),
            tool.name.clone(),
            Some(request.context.context_ref.unwrap_or_default()),
        );

        // Verify result
        assert!(result.success);
        assert_eq!(result.result.unwrap()["processed"], "test message");
        assert_eq!(result.tool_name, "lifecycle_test_tool");
        assert!(result.context_ref.is_some());
    }

    #[test]
    fn test_type_compatibility() {
        // Test that all types work together properly

        let context = ContextRef::new();
        let exec_context = ToolExecutionContext {
            execution_id: "compatibility-test".to_string(),
            context_ref: Some(context.clone()),
            timeout: Some(Duration::from_secs(10)),
            environment: std::collections::HashMap::new(),
            user_context: None,
            service_context: None,
            started_at: chrono::Utc::now(),
        };

        let request = ToolExecutionRequest::new(
            "compatibility_tool".to_string(),
            json!({"test": true}),
            exec_context,
        );

        let result = ToolExecutionResult::success(
            json!({"success": true}),
            Duration::from_millis(25),
            "compatibility_tool".to_string(),
            Some(context),
        );

        // Verify all types are compatible
        assert!(request.context.context_ref.is_some());
        assert!(result.context_ref.is_some());
        assert_eq!(request.context.context_ref.as_ref().unwrap().id,
                  result.context_ref.unwrap().id);
    }

    #[test]
    fn test_serialization_roundtrip() {
        // Test that types can be serialized and deserialized correctly

        let original_context = ContextRef::with_metadata({
            let mut metadata = std::collections::HashMap::new();
            metadata.insert("tool_name".to_string(), "serialization_test".to_string());
            metadata.insert("version".to_string(), "1.0.0".to_string());
            metadata
        });

        // Serialize
        let serialized = serde_json::to_string(&original_context).expect("Failed to serialize");

        // Deserialize
        let deserialized: ContextRef = serde_json::from_str(&serialized).expect("Failed to deserialize");

        // Verify round-trip preservation
        assert_eq!(original_context.id, deserialized.id);
        assert_eq!(original_context.metadata, deserialized.metadata);
        assert_eq!(original_context.parent_id, deserialized.parent_id);
    }
}