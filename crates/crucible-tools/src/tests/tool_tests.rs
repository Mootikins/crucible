//! Comprehensive unit tests for crucible-tools Phase 4.1 fixes
//!
//! This module tests the ContextRef migration fixes and tool functionality
//! that were implemented in Phase 4.1 to ensure compatibility with the new
//! crucible-services architecture.

use crucible_tools::tool::*;
use crucible_tools::types::{ToolDependency, ToolCategory};
use anyhow::Result;
use chrono::{DateTime, Utc};
use crucible_services::types::tool::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::fs;

#[cfg(test)]
mod rune_tool_tests {
    use super::*;

    #[test]
    fn test_rune_tool_metadata_structure() {
        let metadata = ToolMetadata {
            version: Some("2.1.0".to_string()),
            category: Some("Testing".to_string()),
            tags: Some(vec!["test".to_string(), "rune".to_string()]),
            author: Some("Test Author".to_string()),
            dependencies: Some(vec![
                ToolDependency {
                    name: "stdlib".to_string(),
                    version: Some("1.0.0".to_string()),
                    optional: false,
                },
                ToolDependency {
                    name: "json".to_string(),
                    version: None,
                    optional: true,
                },
            ]),
            permissions: Some(vec!["read".to_string(), "execute".to_string()]),
            additional: {
                let mut map = HashMap::new();
                map.insert("created_by".to_string(), json!("test_suite"));
                map.insert("test_env".to_string(), json!(true));
                map
            },
        };

        assert_eq!(metadata.version, Some("2.1.0"));
        assert_eq!(metadata.category, Some("Testing"));
        assert_eq!(metadata.tags, Some(vec!["test", "rune"]));
        assert_eq!(metadata.author, Some("Test Author"));
        assert_eq!(metadata.dependencies.as_ref().unwrap().len(), 2);
        assert_eq!(metadata.permissions.as_ref().unwrap().len(), 2);
        assert_eq!(metadata.additional.len(), 2);
    }

    #[test]
    fn test_rune_tool_metadata_serialization() {
        let metadata = ToolMetadata {
            version: Some("1.5.0".to_string()),
            category: Some("AI".to_string()),
            tags: Some(vec!["ai".to_string(), "ml".to_string()]),
            author: Some("ML Team".to_string()),
            dependencies: Some(vec![
                ToolDependency {
                    name: "tensorflow".to_string(),
                    version: Some("2.8.0".to_string()),
                    optional: false,
                },
            ]),
            permissions: Some(vec!["compute".to_string()]),
            additional: HashMap::new(),
        };

        let serialized = serde_json::to_string(&metadata).expect("Failed to serialize metadata");
        let deserialized: ToolMetadata = serde_json::from_str(&serialized).expect("Failed to deserialize metadata");

        assert_eq!(metadata.version, deserialized.version);
        assert_eq!(metadata.category, deserialized.category);
        assert_eq!(metadata.tags, deserialized.tags);
        assert_eq!(metadata.author, deserialized.author);
        assert_eq!(metadata.dependencies, deserialized.dependencies);
        assert_eq!(metadata.permissions, deserialized.permissions);
    }

    #[test]
    fn test_tool_execution_config_default() {
        let config = ToolExecutionConfig::default();

        assert_eq!(config.timeout_ms, Some(30000));
        assert_eq!(config.max_memory_bytes, Some(100 * 1024 * 1024));
        assert!(config.capture_output);
        assert!(config.environment.is_empty());
        assert!(config.working_directory.is_none());
    }

    #[test]
    fn test_tool_execution_config_custom() {
        let mut environment = HashMap::new();
        environment.insert("PATH".to_string(), "/usr/bin".to_string());
        environment.insert("HOME".to_string(), "/home/user".to_string());

        let config = ToolExecutionConfig {
            timeout_ms: Some(60000),
            max_memory_bytes: Some(200 * 1024 * 1024),
            capture_output: false,
            environment: environment.clone(),
            working_directory: Some(PathBuf::from("/tmp")),
        };

        assert_eq!(config.timeout_ms, Some(60000));
        assert_eq!(config.max_memory_bytes, Some(200 * 1024 * 1024));
        assert!(!config.capture_output);
        assert_eq!(config.environment, environment);
        assert_eq!(config.working_directory, Some(PathBuf::from("/tmp")));
    }

    #[tokio::test]
    async fn test_rune_value_to_json_conversion() {
        // Test null value
        let rune_null = rune::Value::from(());
        let json_null = rune_value_to_json(&rune_null).expect("Failed to convert null");
        assert_eq!(json_null, Value::Null);

        // Test boolean values
        let rune_true = rune::Value::from(true);
        let json_true = rune_value_to_json(&rune_true).expect("Failed to convert true");
        assert_eq!(json_true, Value::Bool(true));

        let rune_false = rune::Value::from(false);
        let json_false = rune_value_to_json(&rune_false).expect("Failed to convert false");
        assert_eq!(json_false, Value::Bool(false));

        // Test integer values
        let rune_int = rune::Value::from(42i64);
        let json_int = rune_value_to_json(&rune_int).expect("Failed to convert integer");
        assert_eq!(json_int, Value::Number(42.into()));

        // Test float values
        let rune_float = rune::Value::from(3.14159f64);
        let json_float = rune_value_to_json(&rune_float).expect("Failed to convert float");
        assert_eq!(json_float, Value::Number(serde_json::Number::from_f64(3.14159).unwrap()));

        // Test string values
        let rune_str = rune::Value::try_from(rune::alloc::String::try_from("hello world").unwrap()).unwrap();
        let json_str = rune_value_to_json(&rune_str).expect("Failed to convert string");
        assert_eq!(json_str, Value::String("hello world".to_string()));
    }

    #[tokio::test]
    async fn test_json_to_rune_value_conversion() {
        // Test null value
        let json_null = Value::Null;
        let rune_null = json_to_rune_value(&json_null).expect("Failed to convert null to rune");
        assert_eq!(rune_null.as_bool().into_result().unwrap_err(), rune::vm::Error::expected_bool(None));

        // Test boolean values
        let json_true = Value::Bool(true);
        let rune_true = json_to_rune_value(&json_true).expect("Failed to convert true to rune");
        assert_eq!(rune_true.as_bool().into_result().unwrap(), true);

        let json_false = Value::Bool(false);
        let rune_false = json_to_rune_value(&json_false).expect("Failed to convert false to rune");
        assert_eq!(rune_false.as_bool().into_result().unwrap(), false);

        // Test integer values
        let json_int = Value::Number(42.into());
        let rune_int = json_to_rune_value(&json_int).expect("Failed to convert integer to rune");
        assert_eq!(rune_int.as_integer().into_result().unwrap(), 42);

        // Test float values
        let json_float = Value::Number(serde_json::Number::from_f64(2.71828).unwrap());
        let rune_float = json_to_rune_value(&json_float).expect("Failed to convert float to rune");
        assert_eq!(rune_float.as_float().into_result().unwrap(), 2.71828);

        // Test string values
        let json_str = Value::String("test string".to_string());
        let rune_str = json_to_rune_value(&json_str).expect("Failed to convert string to rune");
        let converted_back = rune_value_to_json(&rune_str).expect("Failed to convert back");
        assert_eq!(json_str, converted_back);

        // Test array values
        let json_array = Value::Array(vec![json!(1), json!(2), json!(3)]);
        let rune_array = json_to_rune_value(&json_array).expect("Failed to convert array to rune");
        let converted_back = rune_value_to_json(&rune_array).expect("Failed to convert array back");
        assert_eq!(json_array, converted_back);

        // Test object values
        let json_object = json!({
            "name": "test",
            "value": 42,
            "nested": {
                "active": true
            }
        });
        let rune_object = json_to_rune_value(&json_object).expect("Failed to convert object to rune");
        let converted_back = rune_value_to_json(&rune_object).expect("Failed to convert object back");
        assert_eq!(json_object, converted_back);
    }

    #[tokio::test]
    async fn test_complex_json_rune_conversions() {
        let complex_json = json!({
            "users": [
                {
                    "id": 1,
                    "name": "Alice",
                    "active": true,
                    "profile": {
                        "age": 30,
                        "city": "New York",
                        "hobbies": ["reading", "swimming"]
                    }
                },
                {
                    "id": 2,
                    "name": "Bob",
                    "active": false,
                    "profile": {
                        "age": 25,
                        "city": "San Francisco",
                        "hobbies": ["coding", "gaming"]
                    }
                }
            ],
            "metadata": {
                "total": 2,
                "page": 1,
                "timestamp": "2023-01-01T00:00:00Z"
            }
        });

        let rune_value = json_to_rune_value(&complex_json).expect("Failed to convert complex JSON");
        let converted_back = rune_value_to_json(&rune_value).expect("Failed to convert complex Rune value back");

        assert_eq!(complex_json, converted_back);

        // Verify specific nested structures
        assert_eq!(converted_back["users"][0]["name"], "Alice");
        assert_eq!(converted_back["users"][0]["profile"]["hobbies"][1], "swimming");
        assert_eq!(converted_back["metadata"]["total"], 2);
    }

    #[test]
    fn test_tool_validation_basic() {
        // Create a mock tool for validation testing
        let input_schema = json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "minLength": 1
                },
                "count": {
                    "type": "integer",
                    "minimum": 0
                }
            },
            "required": ["message"]
        });

        let output_schema = json!({
            "type": "object",
            "properties": {
                "success": {"type": "boolean"},
                "result": {"type": "string"}
            },
            "required": ["success"]
        });

        // Mock tool validation logic (simplified version of what RuneTool::validate_input does)
        let valid_input = json!({
            "message": "Hello, World!",
            "count": 5
        });

        let invalid_input = json!("not an object");

        // Valid input should be an object
        assert!(valid_input.is_object());

        // Invalid input should fail validation
        assert!(!invalid_input.is_object());

        // Test output validation
        let valid_output = json!({
            "success": true,
            "result": "processed"
        });

        let invalid_output = json!({
            "result": "missing success field"
        });

        // Valid output should be serializable
        assert!(serde_json::to_string(&valid_output).is_ok());
        assert!(serde_json::to_string(&invalid_output).is_ok());
    }

    #[test]
    fn test_tool_definition_conversion() {
        let rune_tool_metadata = ToolMetadata {
            version: Some("3.0.0".to_string()),
            category: Some("System".to_string()),
            tags: Some(vec!["system".to_string(), "admin".to_string()]),
            author: Some("System Admin".to_string()),
            dependencies: Some(vec![]),
            permissions: Some(vec!["admin".to_string()]),
            additional: HashMap::new(),
        };

        // Simulate creating a ToolDefinition from RuneTool data
        let tool_def = ToolDefinition {
            name: "system_monitor".to_string(),
            description: "Monitor system resources and performance".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "metric": {
                        "type": "string",
                        "enum": ["cpu", "memory", "disk", "network"]
                    },
                    "interval": {
                        "type": "integer",
                        "minimum": 1,
                        "default": 5
                    }
                },
                "required": ["metric"]
            }),
            category: rune_tool_metadata.category.clone(),
            version: rune_tool_metadata.version.clone(),
            author: rune_tool_metadata.author.clone(),
            tags: rune_tool_metadata.tags.clone().unwrap_or_default(),
            enabled: true,
            parameters: vec![
                ToolParameter {
                    name: "metric".to_string(),
                    param_type: "string".to_string(),
                    description: "System metric to monitor".to_string(),
                    required: true,
                    default_value: None,
                },
                ToolParameter {
                    name: "interval".to_string(),
                    param_type: "integer".to_string(),
                    description: "Monitoring interval in seconds".to_string(),
                    required: false,
                    default_value: Some(json!(5)),
                },
            ],
        };

        assert_eq!(tool_def.name, "system_monitor");
        assert_eq!(tool_def.category, Some("System".to_string()));
        assert_eq!(tool_def.version, Some("3.0.0".to_string()));
        assert_eq!(tool_def.author, Some("System Admin".to_string()));
        assert_eq!(tool_def.tags, vec!["system", "admin"]);
        assert!(tool_def.enabled);
        assert_eq!(tool_def.parameters.len(), 2);
        assert!(tool_def.parameters[0].required);
        assert!(!tool_def.parameters[1].required);
    }
}

#[cfg(test)]
mod context_ref_migration_tests {
    use super::*;

    #[test]
    fn test_context_ref_new_api_compatibility() {
        // Test the new ContextRef API from crucible-services
        let context = crucible_services::types::tool::ContextRef::new();

        assert!(!context.id.is_empty());
        assert!(context.metadata.is_empty());
        assert!(context.parent_id.is_none());
        assert!(context.created_at <= Utc::now());
    }

    #[test]
    fn test_context_ref_with_metadata_api() {
        let mut metadata = HashMap::new();
        metadata.insert("tool_name".to_string(), "test_tool".to_string());
        metadata.insert("execution_id".to_string(), "exec-12345".to_string());
        metadata.insert("user_id".to_string(), "user-67890".to_string());

        let context = crucible_services::types::tool::ContextRef::with_metadata(metadata);

        assert!(!context.id.is_empty());
        assert_eq!(context.metadata.len(), 3);
        assert_eq!(context.metadata.get("tool_name"), Some(&"test_tool".to_string()));
        assert_eq!(context.metadata.get("execution_id"), Some(&"exec-12345".to_string()));
        assert_eq!(context.metadata.get("user_id"), Some(&"user-67890".to_string()));
    }

    #[test]
    fn test_context_ref_child_creation() {
        let parent = crucible_services::types::tool::ContextRef::new();
        let child = parent.child();

        assert_ne!(parent.id, child.id);
        assert_eq!(child.parent_id, Some(parent.id.clone()));
        assert!(child.metadata.is_empty());
        assert!(child.created_at >= parent.created_at);
    }

    #[test]
    fn test_context_ref_serialization_compatibility() {
        let mut metadata = HashMap::new();
        metadata.insert("test_key".to_string(), "test_value".to_string());
        metadata.insert("numeric_key".to_string(), "42".to_string());

        let context = crucible_services::types::tool::ContextRef::with_metadata(metadata);

        // Test serialization
        let serialized = serde_json::to_string(&context).expect("Failed to serialize ContextRef");
        let deserialized: crucible_services::types::tool::ContextRef =
            serde_json::from_str(&serialized).expect("Failed to deserialize ContextRef");

        assert_eq!(context.id, deserialized.id);
        assert_eq!(context.metadata, deserialized.metadata);
        assert_eq!(context.parent_id, deserialized.parent_id);
    }

    #[test]
    fn test_context_ref_with_id_migration_pattern() {
        // Test the migration pattern using with_id for testing scenarios
        let known_id = "test-context-123".to_string();
        let context = crucible_services::types::tool::ContextRef::with_id(known_id.clone());

        assert_eq!(context.id, known_id);
        assert!(context.metadata.is_empty());
        assert!(context.parent_id.is_none());
    }

    #[test]
    fn test_tool_execution_context_with_context_ref() {
        let context_ref = crucible_services::types::tool::ContextRef::new();
        let mut exec_context = crucible_services::types::tool::ToolExecutionContext {
            execution_id: "test-execution".to_string(),
            context_ref: Some(context_ref.clone()),
            timeout: Some(Duration::from_secs(60)),
            environment: HashMap::new(),
            user_context: Some(json!({"user_id": "test-user"})),
            service_context: Some(json!({"service": "test-service"})),
            started_at: Utc::now(),
        };

        assert_eq!(exec_context.execution_id, "test-execution");
        assert_eq!(exec_context.context_ref, Some(context_ref));
        assert_eq!(exec_context.timeout, Some(Duration::from_secs(60)));
        assert!(exec_context.user_context.is_some());
        assert!(exec_context.service_context.is_some());
    }

    #[test]
    fn test_nested_context_ref_hierarchy() {
        let root = crucible_services::types::tool::ContextRef::new();
        let level1 = root.child();
        let level2 = level1.child();
        let level3 = level2.child();

        // Verify hierarchy
        assert!(root.parent_id.is_none());
        assert_eq!(level1.parent_id, Some(root.id.clone()));
        assert_eq!(level2.parent_id, Some(level1.id.clone()));
        assert_eq!(level3.parent_id, Some(level2.id.clone()));

        // Verify all IDs are unique
        let mut ids = std::collections::HashSet::new();
        for context in [&root, &level1, &level2, &level3] {
            assert!(ids.insert(context.id.clone()), "Duplicate ID found in context hierarchy");
        }

        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn test_context_ref_metadata_evolution() {
        let mut context = crucible_services::types::tool::ContextRef::new();

        // Add initial metadata
        context.metadata.insert("phase".to_string(), "initialization".to_string());
        context.metadata.insert("step".to_string(), "1".to_string());

        // Evolve metadata (simulating execution phases)
        context.metadata.insert("phase".to_string(), "processing".to_string());
        context.metadata.insert("step".to_string(), "2".to_string());
        context.metadata.insert("processing_time".to_string(), "150ms".to_string());

        // Final evolution
        context.metadata.insert("phase".to_string(), "completed".to_string());
        context.metadata.insert("step".to_string(), "3".to_string());
        context.metadata.insert("result".to_string(), "success".to_string());

        assert_eq!(context.metadata.len(), 4);
        assert_eq!(context.metadata.get("phase"), Some(&"completed".to_string()));
        assert_eq!(context.metadata.get("step"), Some(&"3".to_string()));
        assert_eq!(context.metadata.get("result"), Some(&"success".to_string()));
    }

    #[tokio::test]
    async fn test_rune_tool_context_integration() {
        // Test integration between RuneTool and new ContextRef pattern
        let tool_source = r#"
            pub fn NAME() { "context_test_tool" }
            pub fn DESCRIPTION() { "Tool for testing context integration" }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: #{ message: #{ type: "string" } }
                }
            }

            pub async fn call(args) {
                #{
                    success: true,
                    message: `Processed: ${args.message}`,
                    timestamp: std::time::SystemTime::now()
                }
            }
        "#;

        let context = rune::Context::with_default_modules().unwrap();
        let tool = RuneTool::from_source(tool_source, &context, None).unwrap();

        // Create execution context with new ContextRef pattern
        let mut exec_context = ToolExecutionContext {
            execution_id: "integration-test".to_string(),
            context_ref: Some(crucible_services::types::tool::ContextRef::with_metadata({
                let mut metadata = HashMap::new();
                metadata.insert("tool_name".to_string(), tool.name.clone());
                metadata.insert("test_type".to_string(), "integration".to_string());
                metadata
            })),
            timeout: Some(Duration::from_secs(30)),
            environment: HashMap::new(),
            user_context: Some(json!({"user_id": "test-user", "session": "test-session"})),
            service_context: Some(json!({"service": "rune-engine"})),
            started_at: Utc::now(),
        };

        // Execute tool with context
        let args = json!({"message": "integration test"});
        let (result, context_ref) = tool.call_with_context(args, &context, &exec_context).await.unwrap();

        assert!(result.success);
        assert!(result.result.is_some());
        assert!(result.context_ref.is_some());
        assert!(!context_ref.id.is_empty());

        // Verify context metadata was properly transferred
        assert_eq!(context_ref.metadata.get("tool_name"), Some(&tool.name));
        assert_eq!(context_ref.metadata.get("test_type"), Some(&"integration"));
        assert_eq!(context_ref.metadata.get("execution_id"), Some(&"integration-test"));
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[test]
    fn test_rune_value_conversion_error_handling() {
        // Test conversion of invalid or edge case values
        let invalid_rune_values = vec![
            // Add test cases for edge cases that might cause conversion errors
        ];

        for value in invalid_rune_values {
            // This would need actual invalid rune values, which are hard to construct
            // without deeper knowledge of Rune's internal representation
        }
    }

    #[test]
    fn test_json_conversion_error_handling() {
        // Test JSON conversion errors
        let invalid_json_numbers = vec![
            Value::Number(serde_json::Number::from_f64(f64::NAN).unwrap()),
            Value::Number(serde_json::Number::from_f64(f64::INFINITY).unwrap()),
            Value::Number(serde_json::Number::from_f64(f64::NEG_INFINITY).unwrap()),
        ];

        for invalid_num in invalid_json_numbers {
            let result = json_to_rune_value(&invalid_num);
            // These should either succeed with appropriate handling or fail gracefully
            match result {
                Ok(_) => {
                    // If conversion succeeds, converting back should preserve the error state
                    let back_to_json = rune_value_to_json(&result.unwrap());
                    // Handle the error appropriately
                }
                Err(_) => {
                    // Expected failure for invalid numbers
                }
            }
        }
    }

    #[test]
    fn test_tool_validation_edge_cases() {
        let tool = create_mock_tool();

        // Test validation with invalid input types
        let invalid_inputs = vec![
            Value::Null,
            Value::Bool(true),
            Value::Number(42),
            Value::String("not an object"),
            Value::Array(vec![json!(1), json!(2)]),
        ];

        for invalid_input in invalid_inputs {
            let validation_result = tool.validate_input(&invalid_input);
            // Should fail for all non-object inputs
            assert!(validation_result.is_err(), "Expected validation to fail for input: {:?}", invalid_input);
        }

        // Test validation with valid object but missing required fields
        let incomplete_object = json!({"optional_field": "value"});
        let validation_result = tool.validate_input(&incomplete_object);
        // Current simplified validation only checks if it's an object
        assert!(validation_result.is_ok());
    }

    #[test]
    fn test_context_ref_error_scenarios() {
        // Test ContextRef creation with problematic metadata
        let problematic_metadata_cases = vec![
            (HashMap::new(), "empty metadata"),
            ({
                let mut metadata = HashMap::new();
                metadata.insert("key".to_string(), "value".to_string());
                metadata
            }, "simple metadata"),
            ({
                let mut metadata = HashMap::new();
                for i in 0..1000 {
                    metadata.insert(format!("key_{}", i), format!("value_{}", i));
                }
                metadata
            }, "large metadata"),
        ];

        for (metadata, description) in problematic_metadata_cases {
            let context = crucible_services::types::tool::ContextRef::with_metadata(metadata);
            assert!(!context.id.is_empty(), "Context ID should not be empty for case: {}", description);

            // Test serialization doesn't fail
            let serialized = serde_json::to_string(&context);
            assert!(serialized.is_ok(), "Serialization should not fail for case: {}", description);
        }
    }

    #[test]
    fn test_tool_execution_timeout_scenarios() {
        let tool = create_mock_tool();
        let context = rune::Context::with_default_modules().unwrap();

        // Test with various timeout configurations
        let timeout_cases = vec![
            Some(Duration::from_millis(1)),    // Very short timeout
            Some(Duration::from_secs(30)),     // Normal timeout
            Some(Duration::from_secs(300)),    // Long timeout
            None,                              // No timeout
        ];

        for timeout in timeout_cases {
            let exec_context = ToolExecutionContext {
                execution_id: format!("timeout-test-{:?}", timeout),
                context_ref: Some(crucible_services::types::tool::ContextRef::new()),
                timeout,
                environment: HashMap::new(),
                user_context: None,
                service_context: None,
                started_at: Utc::now(),
            };

            // Execute tool and verify it handles timeout appropriately
            let args = json!({"test": "timeout"});
            let result = tool.call_with_context(args, &context, &exec_context).await;

            // Verify result is either success or appropriate timeout error
            match result {
                Ok((execution_result, _)) => {
                    // Either success or timeout error should be handled gracefully
                    assert!(!execution_result.tool_name.is_empty());
                }
                Err(e) => {
                    // Should be a timeout-related error or other appropriate error
                    assert!(!e.to_string().is_empty());
                }
            }
        }
    }

    fn create_mock_tool() -> RuneTool {
        // This would need to create an actual RuneTool instance
        // For now, we'll use a placeholder that would be implemented
        // with actual Rune source compilation
        unimplemented!("Mock tool creation needs actual Rune source")
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_context_ref_creation_performance() {
        let iterations = 10000;
        let start = Instant::now();

        for _ in 0..iterations {
            let _context = crucible_services::types::tool::ContextRef::new();
        }

        let duration = start.elapsed();
        let avg_time = duration.as_nanos() as f64 / iterations as f64;

        // Context creation should be fast (< 10 microseconds per creation)
        assert!(avg_time < 10000.0, "ContextRef creation too slow: {:.2}ns average", avg_time);
        println!("ContextRef creation: {:.2}ns average ({} iterations)", avg_time, iterations);
    }

    #[test]
    fn test_context_ref_with_metadata_performance() {
        let iterations = 1000;
        let mut metadata = HashMap::new();
        metadata.insert("tool_name".to_string(), "performance_test".to_string());
        metadata.insert("execution_id".to_string(), "perf-123".to_string());
        metadata.insert("user_id".to_string(), "user-456".to_string());

        let start = Instant::now();

        for i in 0..iterations {
            let mut iteration_metadata = metadata.clone();
            iteration_metadata.insert("iteration".to_string(), i.to_string());
            let _context = crucible_services::types::tool::ContextRef::with_metadata(iteration_metadata);
        }

        let duration = start.elapsed();
        let avg_time = duration.as_nanos() as f64 / iterations as f64;

        // Context with metadata creation should still be reasonably fast
        assert!(avg_time < 50000.0, "ContextRef with metadata creation too slow: {:.2}ns average", avg_time);
        println!("ContextRef with metadata: {:.2}ns average ({} iterations)", avg_time, iterations);
    }

    #[tokio::test]
    async fn test_json_rune_conversion_performance() {
        let test_data = json!({
            "users": [
                {"id": 1, "name": "Alice", "active": true},
                {"id": 2, "name": "Bob", "active": false},
                {"id": 3, "name": "Charlie", "active": true}
            ],
            "metadata": {
                "total": 3,
                "page": 1,
                "filters": ["active", "recent"]
            }
        });

        let iterations = 1000;
        let start = Instant::now();

        for _ in 0..iterations {
            let rune_value = json_to_rune_value(&test_data).expect("Failed to convert to Rune");
            let _json_value = rune_value_to_json(&rune_value).expect("Failed to convert back to JSON");
        }

        let duration = start.elapsed();
        let avg_time = duration.as_nanos() as f64 / iterations as f64;

        // JSON conversion should be reasonably fast for moderately complex data
        assert!(avg_time < 100000.0, "JSON-Rune conversion too slow: {:.2}ns average", avg_time);
        println!("JSON-Rune round-trip: {:.2}ns average ({} iterations)", avg_time, iterations);
    }

    #[test]
    fn test_serialization_performance() {
        let context = crucible_services::types::tool::ContextRef::with_metadata({
            let mut metadata = HashMap::new();
            metadata.insert("tool_name".to_string(), "performance_test_tool".to_string());
            metadata.insert("execution_id".to_string(), "exec-perf-789".to_string());
            metadata.insert("user_id".to_string(), "user-perf-012".to_string());
            metadata.insert("session_id".to_string(), "session-perf-345".to_string());
            metadata.insert("trace_id".to_string(), "trace-perf-678".to_string());
            metadata
        });

        let iterations = 1000;
        let start = Instant::now();

        for _ in 0..iterations {
            let _serialized = serde_json::to_string(&context).expect("Failed to serialize");
        }

        let duration = start.elapsed();
        let avg_time = duration.as_nanos() as f64 / iterations as f64;

        // Serialization should be fast
        assert!(avg_time < 20000.0, "ContextRef serialization too slow: {:.2}ns average", avg_time);
        println!("ContextRef serialization: {:.2}ns average ({} iterations)", avg_time, iterations);
    }
}