//! Tests for Phase 4 system tools fixes
//!
//! This test module validates the Phase 4 fixes specifically for system tools,
//! ensuring that all components work correctly after the compilation fixes.

use crucible_tools::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use serde_json::{json, Value};
use tempfile::TempDir;
use std::fs;

#[cfg(test)]
mod system_tools_phase4_tests {
    use super::*;

    /// Test that system tools can be created and executed with Phase 4 fixes
    #[tokio::test]
    async fn test_system_tools_creation_and_execution() {
        let mut manager = crucible_tools::system_tools::ToolManager::new();

        // Create a simple tool using the BaseTool pattern
        let tool = crucible_tools::system_tools::BaseTool::new(
            ToolDefinition {
                name: "phase4_test_tool".to_string(),
                description: "Tool for testing Phase 4 fixes".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Message to echo"
                        }
                    },
                    "required": ["message"]
                }),
                category: Some("Test".to_string()),
                version: Some("1.0.0".to_string()),
                author: Some("Phase 4 Test".to_string()),
                tags: vec!["test".to_string(), "phase4".to_string()],
                enabled: true,
                parameters: vec![],
            },
            |params, _context| {
                let message = params["message"].as_str().unwrap_or("no message");
                Ok(ToolExecutionResult {
                    success: true,
                    result: Some(json!({
                        "echo": message,
                        "tool_name": "phase4_test_tool",
                        "context_ref": "phase4-test-context"
                    })),
                    error: None,
                    duration: Duration::from_millis(10),
                    completed_at: chrono::Utc::now(),
                    context_ref: Some(ContextRef::new()),
                    metadata: HashMap::new(),
                })
            },
        );

        manager.register_tool(tool);

        // Test that tool is registered
        let tools = manager.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "phase4_test_tool");

        // Test tool execution
        let context = ToolExecutionContext::default();
        let result = manager.execute_tool(
            "phase4_test_tool",
            json!({"message": "Phase 4 test message"}),
            context,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.result.unwrap()["echo"], "Phase 4 test message");
    }

    /// Test that tool categories work correctly after Phase 4 fixes
    #[test]
    fn test_tool_categories_phase4_fixes() {
        let mut manager = crucible_tools::system_tools::ToolManager::new();

        // Create tools in different categories
        let system_tool = crucible_tools::system_tools::BaseTool::new(
            ToolDefinition {
                name: "system_test_tool".to_string(),
                description: "System category tool".to_string(),
                input_schema: json!({"type": "object"}),
                category: Some("System".to_string()),
                version: None,
                author: None,
                tags: vec![],
                enabled: true,
                parameters: vec![],
            },
            |_params, _context| {
                Ok(ToolExecutionResult::success(json!({"system": true})))
            },
        );

        let database_tool = crucible_tools::system_tools::BaseTool::new(
            ToolDefinition {
                name: "database_test_tool".to_string(),
                description: "Database category tool".to_string(),
                input_schema: json!({"type": "object"}),
                category: Some("Database".to_string()),
                version: None,
                author: None,
                tags: vec![],
                enabled: true,
                parameters: vec![],
            },
            |_params, _context| {
                Ok(ToolExecutionResult::success(json!({"database": true})))
            },
        );

        manager.register_tool(system_tool);
        manager.register_tool(database_tool);

        // Test listing tools by category
        let system_tools = manager.list_tools_by_category(&ToolCategory::System);
        let database_tools = manager.list_tools_by_category(&ToolCategory::Database);

        assert_eq!(system_tools.len(), 1);
        assert_eq!(database_tools.len(), 1);
        assert_eq!(system_tools[0].name, "system_test_tool");
        assert_eq!(database_tools[0].name, "database_test_tool");
    }

    /// Test tool search functionality after Phase 4 fixes
    #[test]
    fn test_tool_search_phase4_fixes() {
        let mut manager = crucible_tools::system_tools::ToolManager::new();

        // Create multiple tools for testing search
        let search_tools = vec![
            ("file_search_tool", "Tool for searching files", "File"),
            ("text_search_tool", "Tool for searching text", "Search"),
            ("database_query_tool", "Tool for database operations", "Database"),
        ];

        for (name, description, category) in search_tools {
            let tool = crucible_tools::system_tools::BaseTool::new(
                ToolDefinition {
                    name: name.to_string(),
                    description: description.to_string(),
                    input_schema: json!({"type": "object"}),
                    category: Some(category.to_string()),
                    version: None,
                    author: None,
                    tags: vec!["search".to_string()],
                    enabled: true,
                    parameters: vec![],
                },
                |_params, _context| {
                    Ok(ToolExecutionResult::success(json!({"found": true})))
                },
            );
            manager.register_tool(tool);
        }

        // Test search functionality
        let search_results = manager.search_tools("search");
        assert_eq!(search_results.len(), 2); // file_search_tool and text_search_tool

        let file_results = manager.search_tools("file");
        assert_eq!(file_results.len(), 1); // file_search_tool only

        let database_results = manager.search_tools("database");
        assert_eq!(database_results.len(), 1); // database_query_tool only

        let no_results = manager.search_tools("nonexistent");
        assert!(no_results.is_empty());
    }

    /// Test tool validation after Phase 4 fixes
    #[test]
    fn test_tool_validation_phase4_fixes() {
        let tool = crucible_tools::system_tools::BaseTool::new(
            ToolDefinition {
                name: "validation_test_tool".to_string(),
                description: "Tool for testing validation".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "required_field": {
                            "type": "string",
                            "description": "A required field"
                        },
                        "optional_field": {
                            "type": "integer",
                            "description": "An optional field",
                            "default": 42
                        }
                    },
                    "required": ["required_field"]
                }),
                category: Some("Test".to_string()),
                version: None,
                author: None,
                tags: vec![],
                enabled: true,
                parameters: vec![],
            },
            |_params, _context| {
                Ok(ToolExecutionResult::success(json!({"validated": true})))
            },
        );

        // Test validation with valid parameters
        let valid_params = json!({
            "required_field": "test value",
            "optional_field": 123
        });
        assert!(tool.validate_params(&valid_params).is_ok());

        // Test validation with only required parameters
        let minimal_params = json!({
            "required_field": "test value"
        });
        assert!(tool.validate_params(&minimal_params).is_ok());

        // Test validation with null parameters (should fail)
        assert!(tool.validate_params(&Value::Null).is_err());
    }
}

#[cfg(test)]
mod service_integration_phase4_tests {
    use super::*;

    /// Test RuneService integration with Phase 4 fixes
    #[tokio::test]
    async fn test_rune_service_phase4_integration() {
        // Create temporary directory with test tools
        let temp_dir = TempDir::new().unwrap();

        // Create multiple test tools to test discovery
        let tools = vec![
            ("phase4_tool_1.rn", "phase4_tool_1", "First Phase 4 test tool"),
            ("phase4_tool_2.rn", "phase4_tool_2", "Second Phase 4 test tool"),
        ];

        for (filename, tool_name, description) in tools {
            let tool_path = temp_dir.path().join(filename);
            let tool_source = format!(r#"
                pub fn NAME() {{ "{}" }}
                pub fn DESCRIPTION() {{ "{}" }}
                pub fn INPUT_SCHEMA() {{
                    #{{
                        type: "object",
                        properties: {{
                            message: {{ type: "string" }}
                        }},
                        required: ["message"]
                    }}
                }}
                pub async fn call(args) {{
                    #{{
                        success: true,
                        result: {{
                            tool_name: "{}",
                            message: args.message,
                            timestamp: time::now()
                        }}
                    }}
                }}
            "#, tool_name, description, tool_name);

            fs::write(&tool_path, tool_source).unwrap();
        }

        // Configure service
        let mut config = RuneServiceConfig::default();
        config.service_name = "phase4-integration-test".to_string();
        config.discovery.tool_directories.clear();
        config.discovery.tool_directories.push(temp_dir.path().to_path_buf());

        // Create service (tests constructor fixes)
        let service = RuneService::new(config).await.unwrap();

        // Discover tools (tests discovery fixes)
        let discovered_count = service.discover_tools_from_directory(temp_dir.path()).await.unwrap();
        assert_eq!(discovered_count, 2);

        // List tools (tests tool definition fixes)
        let tools = service.list_tools().await.unwrap();
        assert_eq!(tools.len(), 2);

        let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
        assert!(tool_names.contains(&"phase4_tool_1".to_string()));
        assert!(tool_names.contains(&"phase4_tool_2".to_string()));

        // Test individual tool execution
        for tool_name in &tool_names {
            let context = ToolExecutionContext::default();
            let request = ToolExecutionRequest::new(
                tool_name.clone(),
                json!({"message": format!("Test message for {}", tool_name)}),
                context,
            );

            let result = service.execute_tool(request).await.unwrap();
            assert!(result.success);

            if let Some(output) = result.result {
                assert_eq!(output["result"]["tool_name"], tool_name.as_str());
                assert!(output["result"]["message"].as_str().unwrap().contains("Test message for"));
            }
        }

        // Test service health (tests method call fixes)
        let health = service.service_health().await.unwrap();
        assert_eq!(health.status, ServiceStatus::Healthy);
        assert!(health.message.unwrap().contains("2 tools"));

        // Test metrics
        let metrics = service.get_metrics().await.unwrap();
        assert!(metrics.total_requests >= 0); // May include validation calls

        // Test system info
        let info = service.system_info();
        assert!(!info.version.is_empty());
        assert!(info.supported_extensions.contains(&"rn".to_string()));
    }

    /// Test error scenarios with Phase 4 fixes
    #[tokio::test]
    async fn test_error_scenarios_phase4_fixes() {
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await.unwrap();

        // Test execution of nonexistent tool
        let request = ToolExecutionRequest::simple(
            "nonexistent_phase4_tool".to_string(),
            json!({"test": true})
        );

        let result = service.execute_tool(request).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(matches!(error, ServiceError::ToolNotFound(_)));
        assert!(error.to_string().contains("nonexistent_phase4_tool"));

        // Test validation of nonexistent tool
        let validation = service.validate_tool("nonexistent_phase4_tool").await.unwrap();
        assert!(!validation.valid);
        assert_eq!(validation.tool_name, "nonexistent_phase4_tool");
        assert!(validation.errors[0].contains("not found"));

        // Test getting nonexistent tool
        let tool_result = service.get_tool("nonexistent_phase4_tool").await.unwrap();
        assert!(tool_result.is_none());

        // Test that error objects are properly formatted (Debug trait)
        let error_debug = format!("{:?}", error);
        assert!(error_debug.contains("ToolNotFound"));
        assert!(error_debug.contains("nonexistent_phase4_tool"));
    }
}

#[cfg(test)]
mod context_and_execution_phase4_tests {
    use super::*;

    /// Test ContextRef functionality with Phase 4 fixes
    #[test]
    fn test_context_ref_comprehensive_phase4() {
        // Test basic creation
        let context1 = ContextRef::new();
        assert!(!context1.id.is_empty());
        assert!(context1.metadata.is_empty());
        assert!(context1.parent_id.is_none());

        // Test with metadata
        let mut metadata = HashMap::new();
        metadata.insert("phase4_test".to_string(), json!("test_value"));
        metadata.insert("test_number".to_string(), json!(42));

        let context2 = ContextRef::with_metadata(metadata.clone());
        assert_eq!(context2.metadata.get("phase4_test"), Some(&json!("test_value")));
        assert_eq!(context2.metadata.get("test_number"), Some(&json!(42)));

        // Test child context creation
        let child_context = context1.child();
        assert_ne!(child_context.id, context1.id);
        assert_eq!(child_context.parent_id, Some(context1.id));
        assert!(child_context.metadata.is_empty());

        // Test metadata operations
        let mut context3 = ContextRef::new();
        context3.add_metadata("added_key".to_string(), json!("added_value"));
        assert_eq!(context3.get_metadata("added_key"), Some(&json!("added_value")));

        // Test overwriting metadata
        context3.add_metadata("added_key".to_string(), json!("new_value"));
        assert_eq!(context3.get_metadata("added_key"), Some(&json!("new_value")));

        // Test getting non-existent key
        assert_eq!(context3.get_metadata("nonexistent"), None);
    }

    /// Test ToolExecutionContext with Phase 4 fixes
    #[test]
    fn test_tool_execution_context_phase4() {
        // Test default creation
        let default_context = ToolExecutionContext::default();
        assert!(!default_context.execution_id.is_empty());
        assert!(default_context.context_ref.is_some());
        assert_eq!(default_context.timeout, Some(Duration::from_secs(30)));
        assert!(default_context.environment.is_empty());

        // Test custom context creation
        let custom_context_ref = ContextRef::new();
        let mut environment = HashMap::new();
        environment.insert("TEST_VAR".to_string(), "test_value".to_string());
        environment.insert("PATH".to_string(), "/test/path".to_string());

        let custom_context = ToolExecutionContext {
            execution_id: "phase4-test-execution".to_string(),
            context_ref: Some(custom_context_ref.clone()),
            timeout: Some(Duration::from_secs(120)),
            environment,
            user_context: Some(json!({"user_id": "phase4_user"})),
            service_context: Some(json!({"service": "phase4_test"})),
            started_at: chrono::Utc::now(),
        };

        assert_eq!(custom_context.execution_id, "phase4-test-execution");
        assert_eq!(custom_context.context_ref, Some(custom_context_ref));
        assert_eq!(custom_context.timeout, Some(Duration::from_secs(120)));
        assert_eq!(custom_context.environment.len(), 2);
        assert_eq!(custom_context.environment.get("TEST_VAR"), Some(&"test_value".to_string()));
    }

    /// Test ToolExecutionResult with Phase 4 fixes
    #[test]
    fn test_tool_execution_result_phase4() {
        // Test success result
        let success_result = ToolExecutionResult::success(json!({
            "output": "Phase 4 success",
            "data": {"key": "value"}
        }));

        assert!(success_result.success);
        assert!(success_result.result.is_some());
        assert!(success_result.error.is_none());
        assert_eq!(success_result.duration, Duration::from_millis(0));
        assert!(success_result.context_ref.is_none());

        // Test error result
        let error_result = ToolExecutionResult::error("Phase 4 test error".to_string());

        assert!(!error_result.success);
        assert!(error_result.result.is_none());
        assert!(error_result.error.is_some());
        assert_eq!(error_result.error.unwrap(), "Phase 4 test error");
        assert_eq!(error_result.duration, Duration::from_millis(0));
        assert!(error_result.context_ref.is_none());

        // Test custom result creation
        let custom_context = ContextRef::new();
        let mut metadata = HashMap::new();
        metadata.insert("execution_type".to_string(), json!("phase4_test"));

        let custom_result = ToolExecutionResult {
            success: true,
            result: Some(json!({"custom": "result"})),
            error: None,
            duration: Duration::from_millis(250),
            completed_at: chrono::Utc::now(),
            context_ref: Some(custom_context),
            metadata,
        };

        assert!(custom_result.success);
        assert_eq!(custom_result.duration, Duration::from_millis(250));
        assert!(custom_result.context_ref.is_some());
        assert_eq!(custom_result.metadata.get("execution_type"), Some(&json!("phase4_test")));
    }

    /// Test ToolExecutionRequest with Phase 4 fixes
    #[test]
    fn test_tool_execution_request_phase4() {
        let context = ToolExecutionContext::default();

        // Test full constructor
        let request1 = ToolExecutionRequest::new(
            "phase4_test_tool".to_string(),
            json!({
                "param1": "value1",
                "param2": 42,
                "param3": true
            }),
            context.clone()
        );

        assert_eq!(request1.tool_name, "phase4_test_tool");
        assert_eq!(request1.parameters["param1"], "value1");
        assert_eq!(request1.parameters["param2"], 42);
        assert_eq!(request1.parameters["param3"], true);
        assert!(!request1.request_id.is_empty());
        assert_eq!(request1.context.execution_id, context.execution_id);

        // Test simple constructor
        let request2 = ToolExecutionRequest::simple(
            "simple_phase4_tool".to_string(),
            json!({"simple": true})
        );

        assert_eq!(request2.tool_name, "simple_phase4_tool");
        assert_eq!(request2.parameters["simple"], true);
        assert!(!request2.request_id.is_empty());
        assert!(request2.context.context_ref.is_some());
    }
}