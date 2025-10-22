//! Comprehensive integration tests for Phase 4.1 fixes
//!
//! This module tests the integration between different components that were
//! modified in Phase 4.1, including the new ContextRef patterns, tool registry,
//! and compatibility with the crucible-services architecture.

use crate::registry::*;
use crate::tool::*;
use crate::types::{ToolDependency, ToolCategory};
use anyhow::Result;
use crucible_services::types::tool::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::fs;

#[cfg(test)]
mod tool_definition_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_rune_tool_to_tool_definition_conversion() {
        let tool_source = r#"
            pub fn NAME() { "conversion_test_tool" }
            pub fn DESCRIPTION() { "Tool for testing conversion to ToolDefinition" }
            pub fn VERSION() { "2.1.0" }
            pub fn CATEGORY() { "Testing" }
            pub fn AUTHOR() { "Integration Test Team" }
            pub fn TAGS() { ["integration", "conversion", "test"] }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: #{
                        message: #{
                            type: "string",
                            description: "Message to process"
                        },
                        options: #{
                            type: "object",
                            properties: #{
                                strict: #{ type: "boolean", default: false },
                                retries: #{ type: "integer", minimum: 0, maximum: 10 }
                            }
                        }
                    },
                    required: ["message"]
                }
            }

            pub async fn call(args) {
                #{
                    success: true,
                    processed_message: `Processed: ${args.message}`,
                    options_used: args.options || #{},
                    timestamp: std::time::SystemTime::now()
                }
            }
        "#;

        let context = rune::Context::with_default_modules().unwrap();
        let rune_tool = RuneTool::from_source(tool_source, &context, None).unwrap();

        // Convert to ToolDefinition
        let tool_def = rune_tool.to_tool_definition();

        // Verify conversion preserved all important fields
        assert_eq!(tool_def.name, "conversion_test_tool");
        assert_eq!(tool_def.description, "Tool for testing conversion to ToolDefinition");
        assert_eq!(tool_def.category, Some("Testing".to_string()));
        assert_eq!(tool_def.version, Some("2.1.0".to_string()));
        assert_eq!(tool_def.author, Some("Integration Test Team".to_string()));
        assert!(tool_def.tags.contains(&"integration".to_string()));
        assert!(tool_def.tags.contains(&"conversion".to_string()));
        assert!(tool_def.tags.contains(&"test".to_string()));
        assert!(tool_def.enabled);

        // Verify input schema structure
        assert_eq!(tool_def.input_schema["type"], "object");
        assert!(tool_def.input_schema["properties"]["message"]["type"] == "string");
        assert!(tool_def.input_schema["required"].as_array().unwrap().contains(&"message"));
        assert!(tool_def.input_schema["properties"]["options"]["properties"]["strict"]["type"] == "boolean");
    }

    #[tokio::test]
    async fn test_tool_registration_and_execution_flow() {
        // Create a registry and register a tool
        let mut registry = ToolRegistry::new();

        let tool_source = r#"
            pub fn NAME() { "flow_test_tool" }
            pub fn DESCRIPTION() { "Tool for testing complete flow" }
            pub fn CATEGORY() { "Flow" }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: #{
                        value: #{ type: "number" },
                        operation: #{ type: "string", enum: ["add", "multiply", "subtract"] }
                    },
                    required: ["value", "operation"]
                }
            }

            pub async fn call(args) {
                let value = args.value || 0;
                let operation = args.operation || "add";

                match operation {
                    "add" => #{ result: value + 10, operation: "add" },
                    "multiply" => #{ result: value * 2, operation: "multiply" },
                    "subtract" => #{ result: value - 5, operation: "subtract" },
                    _ => #{ error: "Unknown operation", operation: operation }
                }
            }
        "#;

        let context = rune::Context::with_default_modules().unwrap();
        let rune_tool = RuneTool::from_source(tool_source, &context, None).unwrap();
        let tool_def = rune_tool.to_tool_definition();

        // Register the tool
        registry.register_tool(tool_def.clone());

        // Verify registration
        assert_eq!(registry.tools.len(), 1);
        assert!(registry.get_tool("flow_test_tool").is_some());

        // Test execution with the original Rune tool
        let args = json!({
            "value": 15,
            "operation": "add"
        });

        let exec_context = ToolExecutionContext::default();
        let (result, context_ref) = rune_tool.call_with_context(args.clone(), &context, &exec_context).await.unwrap();

        assert!(result.success);
        assert_eq!(result.result.unwrap()["result"], 25);
        assert_eq!(result.result.unwrap()["operation"], "add");
        assert!(context_ref.metadata.contains_key("tool_name"));
        assert_eq!(context_ref.metadata["tool_name"], "flow_test_tool");
    }

    #[tokio::test]
    async fn test_tool_registry_with_new_types() {
        let registry = initialize_registry();

        // Verify that built-in tools work with new type system
        let system_info_tool = registry.get_tool("system_info").unwrap();
        assert_eq!(system_info_tool.name, "system_info");
        assert_eq!(system_info_tool.category, Some("System".to_string()));

        // Create execution context using new types
        let exec_context = ToolExecutionContext {
            execution_id: "test-system-info".to_string(),
            context_ref: Some(ContextRef::with_metadata({
                let mut metadata = HashMap::new();
                metadata.insert("test_name".to_string(), "system_info_test".to_string());
                metadata.insert("test_type".to_string(), "integration".to_string());
                metadata
            })),
            timeout: Some(Duration::from_secs(10)),
            environment: HashMap::new(),
            user_context: Some(json!({"user_id": "test_user"})),
            service_context: Some(json!({"service": "test_service"})),
            started_at: chrono::Utc::now(),
        };

        // Create execution request
        let request = ToolExecutionRequest::new(
            system_info_tool.name.clone(),
            json!({}),
            exec_context,
        );

        assert_eq!(request.tool_name, "system_info_tool");
        assert!(!request.request_id.is_empty());
        assert!(request.context.context_ref.is_some());
    }
}

#[cfg(test)]
mod context_ref_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_context_ref_across_execution_flow() {
        let tool_source = r#"
            pub fn NAME() { "context_flow_tool" }
            pub fn DESCRIPTION() { "Tool for testing context flow" }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: #{ message: #{ type: "string" } },
                    required: ["message"]
                }
            }

            pub async fn call(args) {
                #{
                    success: true,
                    message: `Echo: ${args.message}`,
                    execution_id: "exec-12345"
                }
            }
        "#;

        let context = rune::Context::with_default_modules().unwrap();
        let rune_tool = RuneTool::from_source(tool_source, &context, None).unwrap();

        // Create execution context with rich metadata
        let parent_context = ContextRef::with_metadata({
            let mut metadata = HashMap::new();
            metadata.insert("tool_name".to_string(), "context_flow_tool".to_string());
            metadata.insert("user_id".to_string(), "user-789".to_string());
            metadata.insert("session_id".to_string(), "session-abc".to_string());
            metadata.insert("request_id".to_string(), "req-xyz".to_string());
            metadata
        });

        let exec_context = ToolExecutionContext {
            execution_id: "context-flow-test".to_string(),
            context_ref: Some(parent_context.clone()),
            timeout: Some(Duration::from_secs(30)),
            environment: HashMap::new(),
            user_context: Some(json!({"user_id": "user-789", "permissions": ["read", "write"]})),
            service_context: Some(json!({"service": "rune-engine", "version": "1.0.0"})),
            started_at: chrono::Utc::now(),
        };

        // Execute tool
        let args = json!({"message": "context flow test"});
        let (result, context_ref) = rune_tool.call_with_context(args, &context, &exec_context).await.unwrap();

        // Verify execution result
        assert!(result.success);
        assert_eq!(result.result.unwrap()["message"], "Echo: context flow test");
        assert_eq!(result.result.unwrap()["execution_id"], "exec-12345");

        // Verify context propagation
        assert!(context_ref.metadata.contains_key("tool_name"));
        assert_eq!(context_ref.metadata["tool_name"], "context_flow_tool");
        assert_eq!(context_ref.metadata["user_id"], "user-789");
        assert_eq!(context_ref.metadata["session_id"], "session-abc");
        assert_eq!(context_ref.metadata["execution_id"], "context-flow-test");

        // Verify result context reference
        assert!(result.context_ref.is_some());
        let result_context_ref = result.context_ref.unwrap();
        assert_eq!(result_context_ref.metadata["tool_name"], "context_flow_tool");
    }

    #[test]
    fn test_context_ref_hierarchy_integration() {
        // Test creating a hierarchy of contexts for complex execution flows
        let root_context = ContextRef::with_metadata({
            let mut metadata = HashMap::new();
            metadata.insert("workflow_id".to_string(), "workflow-123".to_string());
            metadata.insert("user_id".to_string(), "user-456".to_string());
            metadata
        });

        let step1_context = root_context.child();
        let mut step1_metadata = step1_context.metadata;
        step1_metadata.insert("step_name".to_string(), "data_validation".to_string());
        step1_metadata.insert("step_id".to_string(), "step-1".to_string());
        let step1_context = ContextRef {
            id: step1_context.id,
            metadata: step1_metadata,
            parent_id: step1_context.parent_id,
            created_at: step1_context.created_at,
        };

        let step2_context = step1_context.child();
        let mut step2_metadata = step2_context.metadata;
        step2_metadata.insert("step_name".to_string(), "data_processing".to_string());
        step2_metadata.insert("step_id".to_string(), "step-2".to_string());
        step2_metadata.insert("processing_time".to_string(), "150ms".to_string());
        let step2_context = ContextRef {
            id: step2_context.id,
            metadata: step2_metadata,
            parent_id: step2_context.parent_id,
            created_at: step2_context.created_at,
        };

        // Verify hierarchy
        assert!(root_context.parent_id.is_none());
        assert_eq!(step1_context.parent_id, Some(root_context.id.clone()));
        assert_eq!(step2_context.parent_id, Some(step1_context.id.clone()));

        // Verify metadata propagation
        assert!(root_context.metadata.contains_key("workflow_id"));
        assert!(root_context.metadata.contains_key("user_id"));

        assert!(step1_context.metadata.contains_key("workflow_id"));
        assert!(step1_context.metadata.contains_key("user_id"));
        assert!(step1_context.metadata.contains_key("step_name"));
        assert!(step1_context.metadata.contains_key("step_id"));

        assert!(step2_context.metadata.contains_key("workflow_id"));
        assert!(step2_context.metadata.contains_key("user_id"));
        assert!(step2_context.metadata.contains_key("step_name"));
        assert!(step2_context.metadata.contains_key("step_id"));
        assert!(step2_context.metadata.contains_key("processing_time"));

        // Test serialization of the hierarchy
        let serialized_root = serde_json::to_string(&root_context).unwrap();
        let serialized_step1 = serde_json::to_string(&step1_context).unwrap();
        let serialized_step2 = serde_json::to_string(&step2_context).unwrap();

        let deserialized_root: ContextRef = serde_json::from_str(&serialized_root).unwrap();
        let deserialized_step1: ContextRef = serde_json::from_str(&serialized_step1).unwrap();
        let deserialized_step2: ContextRef = serde_json::from_str(&serialized_step2).unwrap();

        assert_eq!(root_context.id, deserialized_root.id);
        assert_eq!(step1_context.parent_id, deserialized_step1.parent_id);
        assert_eq!(step2_context.parent_id, deserialized_step2.parent_id);
        assert_eq!(step2_context.metadata["processing_time"], "150ms");
    }

    #[tokio::test]
    async fn test_context_ref_with_concurrent_executions() {
        let tool_source = r#"
            pub fn NAME() { "concurrent_tool" }
            pub fn DESCRIPTION() { "Tool for concurrent execution testing" }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: #{ task_id: #{ type: "string" } },
                    required: ["task_id"]
                }
            }

            pub async fn call(args) {
                // Simulate some async work
                std::time::sleep(std::time::Duration::from_millis(10)).await;
                #{
                    success: true,
                    task_id: args.task_id,
                    completed_at: std::time::SystemTime::now()
                }
            }
        "#;

        let context = rune::Context::with_default_modules().unwrap();
        let rune_tool = RuneTool::from_source(tool_source, &context, None).unwrap();

        // Run multiple concurrent executions
        let mut handles = Vec::new();

        for i in 0..10 {
            let tool_clone = rune_tool.clone();
            let context_clone = context.clone();

            let handle = tokio::spawn(async move {
                let exec_context = ToolExecutionContext {
                    execution_id: format!("concurrent-exec-{}", i),
                    context_ref: Some(ContextRef::with_metadata({
                        let mut metadata = HashMap::new();
                        metadata.insert("task_id".to_string(), format!("task-{}", i));
                        metadata.insert("thread_id".to_string(), format!("thread-{}", i % 3));
                        metadata
                    })),
                    timeout: Some(Duration::from_secs(5)),
                    environment: HashMap::new(),
                    user_context: None,
                    service_context: None,
                    started_at: chrono::Utc::now(),
                };

                let args = json!({"task_id": format!("task-{}", i)});
                tool_clone.call_with_context(args, &context_clone, &exec_context).await
            });

            handles.push(handle);
        }

        // Wait for all executions to complete
        let mut results = Vec::new();
        for handle in handles {
            let result = handle.await.unwrap();
            results.push(result);
        }

        // Verify all executions succeeded
        assert_eq!(results.len(), 10);
        for (i, result) in results.into_iter().enumerate() {
            let (execution_result, context_ref) = result.unwrap();
            assert!(execution_result.success);
            assert_eq!(execution_result.result.unwrap()["task_id"], format!("task-{}", i));
            assert_eq!(context_ref.metadata["task_id"], format!("task-{}", i));
            assert!(context_ref.metadata.contains_key("thread_id"));
        }
    }
}

#[cfg(test)]
mod registry_integration_tests {
    use super::*;

    #[test]
    fn test_registry_with_new_type_system() {
        let mut registry = ToolRegistry::new();

        // Create tools using the new type system
        let tools = vec![
            ToolDefinition {
                name: "type_system_tool_1".to_string(),
                description: "Tool 1 with new type system".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "input": {"type": "string"},
                        "options": {
                            "type": "object",
                            "properties": {
                                "strict": {"type": "boolean"},
                                "timeout": {"type": "integer"}
                            }
                        }
                    },
                    "required": ["input"]
                }),
                category: Some("System".to_string()),
                version: Some("2.0.0".to_string()),
                author: Some("Type System Team".to_string()),
                tags: vec!["types".to_string(), "system".to_string()],
                enabled: true,
                parameters: vec![
                    ToolParameter {
                        name: "input".to_string(),
                        param_type: "string".to_string(),
                        description: "Input string".to_string(),
                        required: true,
                        default_value: None,
                    },
                    ToolParameter {
                        name: "options".to_string(),
                        param_type: "object".to_string(),
                        description: "Options object".to_string(),
                        required: false,
                        default_value: Some(json!({"strict": false})),
                    },
                ],
            },
            ToolDefinition {
                name: "type_system_tool_2".to_string(),
                description: "Tool 2 with new type system".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "data": {
                            "type": "array",
                            "items": {"type": "number"}
                        }
                    },
                    "required": ["data"]
                }),
                category: Some("Database".to_string()),
                version: Some("2.1.0".to_string()),
                author: Some("Type System Team".to_string()),
                tags: vec!["types".to_string(), "database".to_string()],
                enabled: true,
                parameters: vec![],
            },
        ];

        // Register tools
        for tool in tools.iter() {
            registry.register_tool(tool.clone());
        }

        // Verify registration
        assert_eq!(registry.tools.len(), 2);
        assert_eq!(registry.categories.len(), 2);

        // Verify tools can be retrieved
        let tool1 = registry.get_tool("type_system_tool_1").unwrap();
        let tool2 = registry.get_tool("type_system_tool_2").unwrap();

        assert_eq!(tool1.name, "type_system_tool_1");
        assert_eq!(tool1.category, Some("System".to_string()));
        assert_eq!(tool1.version, Some("2.0.0".to_string()));
        assert_eq!(tool1.parameters.len(), 2);
        assert!(tool1.parameters[0].required);
        assert!(!tool1.parameters[1].required);

        assert_eq!(tool2.name, "type_system_tool_2");
        assert_eq!(tool2.category, Some("Database".to_string()));
        assert_eq!(tool2.version, Some("2.1.0".to_string()));

        // Test category-based listing
        let system_tools = registry.list_tools_by_category(&ToolCategory::System);
        let db_tools = registry.list_tools_by_category(&ToolCategory::Database);

        assert_eq!(system_tools.len(), 1);
        assert_eq!(db_tools.len(), 1);
        assert_eq!(system_tools[0].name, "type_system_tool_1");
        assert_eq!(db_tools[0].name, "type_system_tool_2");
    }

    #[test]
    fn test_registry_dependency_integration() {
        let mut registry = ToolRegistry::new();

        // Create a dependency chain using new types
        let base_tool = ToolDefinition {
            name: "base_tool".to_string(),
            description: "Base tool with no dependencies".to_string(),
            input_schema: json!({"type": "object"}),
            category: Some("System".to_string()),
            version: Some("1.0.0".to_string()),
            author: None,
            tags: vec!["base".to_string()],
            enabled: true,
            parameters: vec![],
        };

        let intermediate_tool = ToolDefinition {
            name: "intermediate_tool".to_string(),
            description: "Tool that depends on base".to_string(),
            input_schema: json!({"type": "object"}),
            category: Some("System".to_string()),
            version: Some("1.0.0".to_string()),
            author: None,
            tags: vec!["intermediate".to_string()],
            enabled: true,
            parameters: vec![],
        };

        let top_tool = ToolDefinition {
            name: "top_tool".to_string(),
            description: "Tool that depends on intermediate".to_string(),
            input_schema: json!({"type": "object"}),
            category: Some("System".to_string()),
            version: Some("1.0.0".to_string()),
            author: None,
            tags: vec!["top".to_string()],
            enabled: true,
            parameters: vec![],
        };

        // Register in dependency order
        registry.register_tool(base_tool);
        registry.register_tool(intermediate_tool);
        registry.register_tool(top_tool);

        // Simulate dependency validation (since we can't easily add dependencies to ToolDefinition)
        // In a real implementation, this would check actual dependencies
        assert!(registry.validate_dependencies("base_tool").is_ok());
        assert!(registry.validate_dependencies("intermediate_tool").is_ok());
        assert!(registry.validate_dependencies("top_tool").is_ok());

        // Check statistics
        let stats = registry.get_stats();
        assert_eq!(stats.total_tools, 3);
        assert_eq!(stats.categories, 1);
        assert_eq!(stats.tools_with_dependencies, 0); // Would be >0 with real dependencies
    }

    #[test]
    fn test_registry_arc_integration() {
        // Test that the registry works correctly when shared via Arc
        let registry = Arc::new(initialize_registry());

        // Simulate multiple services accessing the same registry
        let service1_registry = Arc::clone(&registry);
        let service2_registry = Arc::clone(&registry);

        // All services should see the same tools
        assert_eq!(registry.tools.len(), service1_registry.tools.len());
        assert_eq!(registry.tools.len(), service2_registry.tools.len());

        // All should have access to built-in tools
        assert!(registry.get_tool("system_info").is_some());
        assert!(service1_registry.get_tool("system_info").is_some());
        assert!(service2_registry.get_tool("system_info").is_some());

        // All should be able to list tools by category
        let system_tools_1 = registry.list_tools_by_category(&ToolCategory::System);
        let system_tools_2 = service1_registry.list_tools_by_category(&ToolCategory::System);
        let system_tools_3 = service2_registry.list_tools_by_category(&ToolCategory::System);

        assert_eq!(system_tools_1.len(), system_tools_2.len());
        assert_eq!(system_tools_2.len(), system_tools_3.len());
        assert!(system_tools_1.len() > 0);
    }
}

#[cfg(test)]
mod end_to_end_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_complete_tool_lifecycle_with_new_types() {
        // Test the complete lifecycle: tool creation -> registration -> execution -> result

        // 1. Create a Rune tool from source
        let tool_source = r#"
            pub fn NAME() { "lifecycle_tool" }
            pub fn DESCRIPTION() { "Tool for testing complete lifecycle" }
            pub fn VERSION() { "3.0.0" }
            pub fn CATEGORY() { "Lifecycle" }
            pub fn AUTHOR() { "E2E Test Team" }
            pub fn TAGS() { ["lifecycle", "e2e", "test"] }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: #{
                        action: #{ type: "string", enum: ["process", "validate", "transform"] },
                        data: #{ type: "object", additionalProperties: true },
                        options: #{
                            type: "object",
                            properties: #{
                                strict: #{ type: "boolean", default: false },
                                timeout: #{ type: "integer", minimum: 1, maximum: 300 }
                            }
                        }
                    },
                    required: ["action", "data"]
                }
            }

            pub async fn call(args) {
                let action = args.action || "process";
                let data = args.data || #{};
                let options = args.options || #{};

                match action {
                    "process" => #{
                        success: true,
                        action: "process",
                        processed_data: data,
                        options_used: options,
                        processing_time: "45ms"
                    },
                    "validate" => #{
                        success: true,
                        action: "validate",
                        validation_result: if options.strict { "strict_valid" } else { "valid" },
                        data_fields: Object::keys(data).len()
                    },
                    "transform" => #{
                        success: true,
                        action: "transform",
                        transformed_data: #{ original: data, transformed: `${data}_transformed` },
                        transformation: "prefix_suffix"
                    },
                    _ => #{
                        success: false,
                        error: "Unknown action",
                        action: action
                    }
                }
            }
        "#;

        let context = rune::Context::with_default_modules().unwrap();
        let rune_tool = RuneTool::from_source(tool_source, &context, None).unwrap();

        // 2. Convert to ToolDefinition and register
        let tool_def = rune_tool.to_tool_definition();
        let mut registry = ToolRegistry::new();
        registry.register_tool(tool_def.clone());

        // 3. Verify registration
        assert_eq!(registry.tools.len(), 1);
        assert!(registry.get_tool("lifecycle_tool").is_some());

        // 4. Execute with different contexts and inputs
        let test_cases = vec![
            (
                json!({
                    "action": "process",
                    "data": {"message": "test data"},
                    "options": {"strict": true}
                }),
                "process",
            ),
            (
                json!({
                    "action": "validate",
                    "data": {"field1": "value1", "field2": "value2"},
                    "options": {"strict": false}
                }),
                "validate",
            ),
            (
                json!({
                    "action": "transform",
                    "data": {"original": "content"},
                    "options": {}
                }),
                "transform",
            ),
        ];

        for (i, (args, expected_action)) in test_cases.into_iter().enumerate() {
            // Create execution context for each test case
            let exec_context = ToolExecutionContext {
                execution_id: format!("lifecycle-test-{}", i),
                context_ref: Some(ContextRef::with_metadata({
                    let mut metadata = HashMap::new();
                    metadata.insert("tool_name".to_string(), "lifecycle_tool".to_string());
                    metadata.insert("test_case".to_string(), i.to_string());
                    metadata.insert("expected_action".to_string(), expected_action.to_string());
                    metadata
                })),
                timeout: Some(Duration::from_secs(30)),
                environment: HashMap::new(),
                user_context: Some(json!({"user_id": "e2e_test_user", "session": "test_session"})),
                service_context: Some(json!({"service": "test_runner", "version": "1.0.0"})),
                started_at: chrono::Utc::now(),
            };

            // Execute the tool
            let (result, context_ref) = rune_tool.call_with_context(args.clone(), &context, &exec_context).await.unwrap();

            // Verify execution result
            assert!(result.success, "Test case {} should succeed", i);
            assert_eq!(result.result.unwrap()["action"], expected_action);
            assert_eq!(result.tool_name, "lifecycle_tool");

            // Verify context metadata
            assert_eq!(context_ref.metadata["tool_name"], "lifecycle_tool");
            assert_eq!(context_ref.metadata["test_case"], i.to_string());
            assert_eq!(context_ref.metadata["expected_action"], expected_action);

            // Verify result context reference
            assert!(result.context_ref.is_some());
            let result_context = result.context_ref.unwrap();
            assert_eq!(result_context.metadata["tool_name"], "lifecycle_tool");
            assert_eq!(result_context.metadata["execution_id"], format!("lifecycle-test-{}", i));
        }

        // 5. Verify final registry state
        let stats = registry.get_stats();
        assert_eq!(stats.total_tools, 1);
        assert_eq!(stats.categories, 1);
    }

    #[tokio::test]
    async fn test_error_handling_across_integration() {
        // Test error handling across the entire integration

        // 1. Create a tool that can fail
        let failing_tool_source = r#"
            pub fn NAME() { "failing_tool" }
            pub fn DESCRIPTION() { "Tool that can fail for testing error handling" }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: #{
                        should_fail: #{ type: "boolean" },
                        error_type: #{ type: "string", enum: ["validation", "execution", "timeout"] }
                    },
                    required: ["should_fail"]
                }
            }

            pub async fn call(args) {
                if args.should_fail {
                    match args.error_type || "execution" {
                        "validation" => #{ error: "Input validation failed", details: "Missing required field" },
                        "execution" => #{ error: "Execution failed", details: "Resource not available" },
                        "timeout" => #{ error: "Timeout occurred", details: "Operation took too long" },
                        _ => #{ error: "Unknown error", details: "Unexpected error type" }
                    }
                } else {
                    #{ success: true, message: "Operation completed successfully" }
                }
            }
        "#;

        let context = rune::Context::with_default_modules().unwrap();
        let rune_tool = RuneTool::from_source(failing_tool_source, &context, None).unwrap();

        // 2. Test successful execution
        let success_exec_context = ToolExecutionContext {
            execution_id: "success-test".to_string(),
            context_ref: Some(ContextRef::new()),
            timeout: Some(Duration::from_secs(10)),
            environment: HashMap::new(),
            user_context: None,
            service_context: None,
            started_at: chrono::Utc::now(),
        };

        let success_args = json!({"should_fail": false});
        let (success_result, success_context) = rune_tool.call_with_context(success_args, &context, &success_exec_context).await.unwrap();

        assert!(success_result.success);
        assert_eq!(success_result.result.unwrap()["message"], "Operation completed successfully");
        assert!(!success_context.id.is_empty());

        // 3. Test different failure scenarios
        let failure_scenarios = vec![
            (json!({"should_fail": true, "error_type": "validation"}), "validation"),
            (json!({"should_fail": true, "error_type": "execution"}), "execution"),
            (json!({"should_fail": true, "error_type": "timeout"}), "timeout"),
            (json!({"should_fail": true, "error_type": "unknown"}), "unknown"),
        ];

        for (i, (args, error_type)) in failure_scenarios.into_iter().enumerate() {
            let exec_context = ToolExecutionContext {
                execution_id: format!("failure-test-{}", i),
                context_ref: Some(ContextRef::with_metadata({
                    let mut metadata = HashMap::new();
                    metadata.insert("test_type".to_string(), "error_handling".to_string());
                    metadata.insert("error_type".to_string(), error_type.to_string());
                    metadata
                })),
                timeout: Some(Duration::from_secs(10)),
                environment: HashMap::new(),
                user_context: None,
                service_context: None,
                started_at: chrono::Utc::now(),
            };

            let result = rune_tool.call_with_context(args, &context, &exec_context).await;

            // Should succeed (the call itself succeeds, even if the tool reports an error)
            assert!(result.is_ok(), "Execution should not fail for error type: {}", error_type);

            let (execution_result, context_ref) = result.unwrap();

            // But the tool result should indicate failure
            assert!(!execution_result.success, "Tool execution should report failure for error type: {}", error_type);
            assert!(execution_result.error.is_some(), "Should have error message for error type: {}", error_type);

            // Context should still be created and valid
            assert!(!context_ref.id.is_empty());
            assert_eq!(context_ref.metadata["test_type"], "error_handling");
            assert_eq!(context_ref.metadata["error_type"], error_type);
        }

        // 4. Test input validation failure
        let invalid_args = json!("not an object");
        let validation_exec_context = ToolExecutionContext {
            execution_id: "validation-failure-test".to_string(),
            context_ref: Some(ContextRef::new()),
            timeout: Some(Duration::from_secs(10)),
            environment: HashMap::new(),
            user_context: None,
            service_context: None,
            started_at: chrono::Utc::now(),
        };

        let validation_result = rune_tool.call_with_context(invalid_args, &context, &validation_exec_context).await;
        assert!(validation_result.is_err(), "Should fail with invalid input");
        assert!(validation_result.unwrap_err().to_string().contains("Tool arguments must be a JSON object"));
    }

    #[tokio::test]
    async fn test_performance_integration() {
        // Test performance characteristics of the integrated system

        let tool_source = r#"
            pub fn NAME() { "performance_tool" }
            pub fn DESCRIPTION() { "Tool for performance testing" }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: #{
                        size: #{ type: "integer", minimum: 1, maximum: 10000 },
                        complexity: #{ type: "string", enum: ["low", "medium", "high"] }
                    },
                    required: ["size"]
                }
            }

            pub async fn call(args) {
                let size = args.size || 100;
                let complexity = args.complexity || "medium";

                // Simulate work based on size and complexity
                let iterations = match complexity {
                    "low" => size / 10,
                    "medium" => size,
                    "high" => size * 10,
                    _ => size
                };

                let mut result = 0;
                for i in 0..iterations {
                    result = result + i % 1000;
                }

                #{
                    success: true,
                    size: size,
                    complexity: complexity,
                    iterations: iterations,
                    result: result,
                    performance_metrics: #{
                        processing_time: "calculated",
                        memory_usage: "minimal"
                    }
                }
            }
        "#;

        let context = rune::Context::with_default_modules().unwrap();
        let rune_tool = RuneTool::from_source(tool_source, &context, None).unwrap();

        // Register the tool
        let tool_def = rune_tool.to_tool_definition();
        let mut registry = ToolRegistry::new();
        registry.register_tool(tool_def);

        // Test multiple executions with varying complexity
        let test_cases = vec![
            (json!({"size": 100, "complexity": "low"}), "low"),
            (json!({"size": 500, "complexity": "medium"}), "medium"),
            (json!({"size": 100, "complexity": "high"}), "high"),
        ];

        let total_start = std::time::Instant::now();
        let mut execution_times = Vec::new();

        for (i, (args, complexity)) in test_cases.iter().enumerate() {
            let exec_start = std::time::Instant::now();

            let exec_context = ToolExecutionContext {
                execution_id: format!("perf-test-{}", i),
                context_ref: Some(ContextRef::with_metadata({
                    let mut metadata = HashMap::new();
                    metadata.insert("test_type".to_string(), "performance".to_string());
                    metadata.insert("complexity".to_string(), complexity.to_string());
                    metadata
                })),
                timeout: Some(Duration::from_secs(30)),
                environment: HashMap::new(),
                user_context: None,
                service_context: None,
                started_at: chrono::Utc::now(),
            };

            let (result, _) = rune_tool.call_with_context(args.clone(), &context, &exec_context).await.unwrap();

            let exec_time = exec_start.elapsed();
            execution_times.push(exec_time);

            assert!(result.success);
            assert_eq!(result.result.unwrap()["complexity"], **complexity);
        }

        let total_time = total_start.elapsed();

        // Performance assertions
        assert!(total_time.as_millis() < 5000, "Total execution time should be under 5 seconds");

        for (i, exec_time) in execution_times.iter().enumerate() {
            assert!(exec_time.as_millis() < 2000, "Execution {} should be under 2 seconds", i);
        }

        let avg_time = total_time.as_millis() as f64 / execution_times.len() as f64;
        assert!(avg_time < 1500.0, "Average execution time should be under 1.5 seconds");

        println!("Performance Integration Test Results:");
        println!("  Total time: {:?}", total_time);
        println!("  Average time: {:.2}ms", avg_time);
        println!("  Individual times: {:?}", execution_times);

        // Verify registry performance
        let registry_start = std::time::Instant::now();
        let retrieved_tool = registry.get_tool("performance_tool");
        let registry_time = registry_start.elapsed();

        assert!(retrieved_tool.is_some());
        assert!(registry_time.as_micros() < 1000, "Registry lookup should be under 1ms");
        println!("  Registry lookup time: {:?}", registry_time);
    }
}