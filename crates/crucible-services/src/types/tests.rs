//! Comprehensive unit tests for tool-related type definitions
//!
//! This module tests all the tool-related types defined in the crucible-services
//! crate, ensuring they work correctly with the new architecture and handle
//! edge cases appropriately.

use super::*;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

#[cfg(test)]
mod tool_tests {
    use super::*;

    #[test]
    fn test_tool_definition_creation() {
        let tool = ToolDefinition {
            name: "test_tool".to_string(),
            description: "A test tool for unit testing".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "Message to process"
                    }
                },
                "required": ["message"]
            }),
            category: Some("Testing".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Test Author".to_string()),
            tags: vec!["test".to_string(), "demo".to_string()],
            enabled: true,
            parameters: vec![
                ToolParameter {
                    name: "message".to_string(),
                    param_type: "string".to_string(),
                    description: "Message to process".to_string(),
                    required: true,
                    default_value: None,
                },
            ],
        };

        assert_eq!(tool.name, "test_tool");
        assert_eq!(tool.description, "A test tool for unit testing");
        assert_eq!(tool.category, Some("Testing".to_string()));
        assert_eq!(tool.version, Some("1.0.0".to_string()));
        assert_eq!(tool.author, Some("Test Author".to_string()));
        assert_eq!(tool.tags, vec!["test", "demo"]);
        assert!(tool.enabled);
        assert_eq!(tool.parameters.len(), 1);
        assert!(tool.parameters[0].required);
    }

    #[test]
    fn test_tool_definition_serialization() {
        let tool = ToolDefinition {
            name: "serializer_test".to_string(),
            description: "Test serialization".to_string(),
            input_schema: json!({"type": "object"}),
            category: Some("Test".to_string()),
            version: Some("2.0.0".to_string()),
            author: Some("Serializer".to_string()),
            tags: vec!["serialization".to_string()],
            enabled: false,
            parameters: vec![],
        };

        let serialized = serde_json::to_string(&tool).expect("Failed to serialize");
        let deserialized: ToolDefinition = serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(tool.name, deserialized.name);
        assert_eq!(tool.description, deserialized.description);
        assert_eq!(tool.enabled, deserialized.enabled);
        assert_eq!(tool.tags, deserialized.tags);
    }

    #[test]
    fn test_tool_parameter_validation() {
        let required_param = ToolParameter {
            name: "required_field".to_string(),
            param_type: "string".to_string(),
            description: "Required parameter".to_string(),
            required: true,
            default_value: None,
        };

        let optional_param = ToolParameter {
            name: "optional_field".to_string(),
            param_type: "number".to_string(),
            description: "Optional parameter".to_string(),
            required: false,
            default_value: Some(json!(42)),
        };

        assert!(required_param.required);
        assert!(required_param.default_value.is_none());
        assert!(!optional_param.required);
        assert!(optional_param.default_value.is_some());
    }

    #[test]
    fn test_context_ref_creation() {
        let context = ContextRef::new();

        assert!(!context.id.is_empty());
        assert_eq!(context.metadata.len(), 0);
        assert!(context.parent_id.is_none());
        assert!(context.created_at <= Utc::now());
    }

    #[test]
    fn test_context_ref_with_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("tool_name".to_string(), json!("test_tool"));
        metadata.insert("execution_id".to_string(), json!("exec-123"));

        let context = ContextRef::with_metadata(metadata);

        assert!(!context.id.is_empty());
        assert_eq!(context.metadata.len(), 2);
        assert_eq!(context.metadata.get("tool_name"), Some(&json!("test_tool")));
        assert_eq!(context.metadata.get("execution_id"), Some(&json!("exec-123")));
    }

    #[test]
    fn test_context_ref_child() {
        let parent = ContextRef::new();
        let child = parent.child();

        assert_ne!(parent.id, child.id);
        assert_eq!(child.parent_id, Some(parent.id));
        assert!(child.metadata.is_empty());
        assert!(child.created_at >= parent.created_at);
    }

    #[test]
    fn test_context_ref_metadata_operations() {
        let mut context = ContextRef::new();

        // Test adding metadata
        context.add_metadata("key1".to_string(), json!("value1"));
        context.add_metadata("key2".to_string(), json!(42));

        assert_eq!(context.metadata.len(), 2);
        assert_eq!(context.get_metadata("key1"), Some(&json!("value1")));
        assert_eq!(context.get_metadata("key2"), Some(&json!(42)));
        assert_eq!(context.get_metadata("nonexistent"), None);

        // Test overwriting metadata
        context.add_metadata("key1".to_string(), json!("new_value"));
        assert_eq!(context.get_metadata("key1"), Some(&json!("new_value")));
    }

    #[test]
    fn test_tool_execution_context_default() {
        let context = ToolExecutionContext::default();

        assert!(!context.execution_id.is_empty());
        assert!(context.context_ref.is_some());
        assert_eq!(context.timeout, Some(Duration::from_secs(30)));
        assert!(context.environment.is_empty());
        assert!(context.user_context.is_none());
        assert!(context.service_context.is_none());
    }

    #[test]
    fn test_tool_execution_request_creation() {
        let exec_context = ToolExecutionContext::default();
        let request = ToolExecutionRequest::new(
            "test_tool".to_string(),
            json!({"input": "value"}),
            exec_context.clone(),
        );

        assert_eq!(request.tool_name, "test_tool");
        assert_eq!(request.parameters, json!({"input": "value"}));
        assert_eq!(request.context.execution_id, exec_context.execution_id);
        assert!(!request.request_id.is_empty());
    }

    #[test]
    fn test_tool_execution_result_success() {
        let context = ContextRef::new();
        let result = ToolExecutionResult::success(
            json!({"output": "success"}),
            Duration::from_millis(100),
            "test_tool".to_string(),
            Some(context.clone()),
        );

        assert!(result.success);
        assert_eq!(result.result, Some(json!({"output": "success"})));
        assert!(result.error.is_none());
        assert_eq!(result.execution_time, Duration::from_millis(100));
        assert_eq!(result.tool_name, "test_tool");
        assert_eq!(result.context_ref, Some(context));
    }

    #[test]
    fn test_tool_execution_result_error() {
        let context = ContextRef::new();
        let result = ToolExecutionResult::error(
            "Something went wrong".to_string(),
            Duration::from_millis(50),
            "failing_tool".to_string(),
            Some(context.clone()),
        );

        assert!(!result.success);
        assert!(result.result.is_none());
        assert_eq!(result.error, Some("Something went wrong".to_string()));
        assert_eq!(result.execution_time, Duration::from_millis(50));
        assert_eq!(result.tool_name, "failing_tool");
        assert_eq!(result.context_ref, Some(context));
    }

    #[test]
    fn test_tool_execution_result_metadata() {
        let mut result = ToolExecutionResult::success(
            json!({"data": "test"}),
            Duration::from_millis(75),
            "metadata_tool".to_string(),
            None,
        );

        result.add_metadata("trace_id".to_string(), json!("trace-123"));
        result.add_metadata("version".to_string(), json!("1.2.3"));

        assert_eq!(result.metadata.len(), 2);
        assert_eq!(result.metadata.get("trace_id"), Some(&json!("trace-123")));
        assert_eq!(result.metadata.get("version"), Some(&json!("1.2.3")));
    }

    #[test]
    fn test_validation_result_valid() {
        let result = ValidationResult::valid();

        assert!(result.valid);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
        assert!(result.metadata.is_empty());
    }

    #[test]
    fn test_validation_result_invalid() {
        let errors = vec![
            "Missing required field".to_string(),
            "Invalid type".to_string(),
        ];
        let result = ValidationResult::invalid(errors.clone());

        assert!(!result.valid);
        assert_eq!(result.errors, errors);
        assert!(result.warnings.is_empty());
        assert!(result.metadata.is_empty());
    }

    #[test]
    fn test_validation_result_warnings_and_metadata() {
        let mut result = ValidationResult::invalid(vec!["Error message".to_string()]);

        result.add_warning("This is a warning".to_string());
        result.add_warning("Another warning".to_string());
        result.add_metadata("validation_time".to_string(), json!(123));

        assert!(!result.valid);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.warnings.len(), 2);
        assert_eq!(result.metadata.len(), 1);
    }

    #[test]
    fn test_tool_execution_stats_initialization() {
        let stats = ToolExecutionStats::new("test_tool".to_string());

        assert_eq!(stats.tool_name, "test_tool");
        assert_eq!(stats.total_executions, 0);
        assert_eq!(stats.successful_executions, 0);
        assert_eq!(stats.failed_executions, 0);
        assert_eq!(stats.avg_execution_time_ms, 0.0);
        assert_eq!(stats.min_execution_time_ms, u64::MAX);
        assert_eq!(stats.max_execution_time_ms, 0);
        assert!(stats.last_execution.is_none());
    }

    #[test]
    fn test_tool_execution_stats_recording() {
        let mut stats = ToolExecutionStats::new("stats_tool".to_string());

        // Record first successful execution
        stats.record_execution(100, true);
        assert_eq!(stats.total_executions, 1);
        assert_eq!(stats.successful_executions, 1);
        assert_eq!(stats.failed_executions, 0);
        assert_eq!(stats.avg_execution_time_ms, 100.0);
        assert_eq!(stats.min_execution_time_ms, 100);
        assert_eq!(stats.max_execution_time_ms, 100);
        assert!(stats.last_execution.is_some());

        // Record second execution (faster)
        stats.record_execution(50, true);
        assert_eq!(stats.total_executions, 2);
        assert_eq!(stats.successful_executions, 2);
        assert_eq!(stats.avg_execution_time_ms, 75.0); // (100 + 50) / 2
        assert_eq!(stats.min_execution_time_ms, 50);
        assert_eq!(stats.max_execution_time_ms, 100);

        // Record failed execution
        stats.record_execution(200, false);
        assert_eq!(stats.total_executions, 3);
        assert_eq!(stats.successful_executions, 2);
        assert_eq!(stats.failed_executions, 1);
        assert_eq!(stats.avg_execution_time_ms, 116.67); // (100 + 50 + 200) / 3
        assert_eq!(stats.max_execution_time_ms, 200);
    }

    #[test]
    fn test_tool_status_equality() {
        assert_eq!(ToolStatus::Available, ToolStatus::Available);
        assert_ne!(ToolStatus::Available, ToolStatus::Disabled);
        assert_ne!(ToolStatus::Error("test".to_string()), ToolStatus::Error("other".to_string()));
    }

    #[test]
    fn test_tool_category_display() {
        assert_eq!(format!("{}", ToolCategory::System), "System");
        assert_eq!(format!("{}", ToolCategory::Database), "Database");
        assert_eq!(format!("{}", ToolCategory::AI), "AI");
        assert_eq!(format!("{}", ToolCategory::General), "General");
    }

    #[test]
    fn test_tool_category_parsing() {
        assert_eq!("system".parse::<ToolCategory>().unwrap(), ToolCategory::System);
        assert_eq!("database".parse::<ToolCategory>().unwrap(), ToolCategory::Database);
        assert_eq!("ai".parse::<ToolCategory>().unwrap(), ToolCategory::AI);
        assert_eq!("GENERAL".parse::<ToolCategory>().unwrap(), ToolCategory::General);

        // Test case insensitivity
        assert_eq!("System".parse::<ToolCategory>().unwrap(), ToolCategory::System);
        assert_eq!("NETWORK".parse::<ToolCategory>().unwrap(), ToolCategory::Network);

        // Test invalid category
        let result: Result<ToolCategory, _> = "invalid_category".parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown tool category"));
    }

    #[test]
    fn test_tool_category_serialization() {
        let categories = vec![
            ToolCategory::System,
            ToolCategory::Database,
            ToolCategory::Network,
            ToolCategory::Vault,
            ToolCategory::Search,
            ToolCategory::AI,
            ToolCategory::General,
        ];

        for category in categories {
            let serialized = serde_json::to_string(&category).expect("Failed to serialize");
            let deserialized: ToolCategory = serde_json::from_str(&serialized).expect("Failed to deserialize");
            assert_eq!(category, deserialized);
        }
    }

    #[test]
    fn test_complex_tool_definition() {
        let complex_tool = ToolDefinition {
            name: "complex_analyzer".to_string(),
            description: "A complex tool with multiple parameters and metadata".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "input_data": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Array of strings to analyze"
                    },
                    "options": {
                        "type": "object",
                        "properties": {
                            "strict": {"type": "boolean", "default": false},
                            "threshold": {"type": "number", "minimum": 0, "maximum": 100}
                        }
                    },
                    "metadata": {
                        "type": "object",
                        "additionalProperties": true
                    }
                },
                "required": ["input_data"]
            }),
            category: Some("Analytics".to_string()),
            version: Some("3.2.1".to_string()),
            author: Some("Data Science Team".to_string()),
            tags: vec![
                "analytics".to_string(),
                "data-processing".to_string(),
                "ml".to_string(),
            ],
            enabled: true,
            parameters: vec![
                ToolParameter {
                    name: "input_data".to_string(),
                    param_type: "array".to_string(),
                    description: "Array of strings to analyze".to_string(),
                    required: true,
                    default_value: None,
                },
                ToolParameter {
                    name: "options".to_string(),
                    param_type: "object".to_string(),
                    description: "Analysis options".to_string(),
                    required: false,
                    default_value: Some(json!({"strict": false})),
                },
            ],
        };

        // Verify serialization preserves all data
        let serialized = serde_json::to_value(&complex_tool).expect("Failed to serialize to JSON");
        assert_eq!(serialized["name"], "complex_analyzer");
        assert_eq!(serialized["category"], "Analytics");
        assert_eq!(serialized["version"], "3.2.1");
        assert_eq!(serialized["tags"].as_array().unwrap().len(), 3);
        assert!(serialized["input_schema"]["properties"]["input_data"]["items"]["type"] == "string");
    }
}

#[cfg(test)]
mod service_tests {
    use super::*;

    #[test]
    fn test_service_status_equality() {
        assert_eq!(ServiceStatus::Healthy, ServiceStatus::Healthy);
        assert_ne!(ServiceStatus::Healthy, ServiceStatus::Degraded);
        assert_ne!(ServiceStatus::Unhealthy, ServiceStatus::Unknown);
    }

    #[test]
    fn test_service_health_creation() {
        let health = ServiceHealth {
            name: "test_service".to_string(),
            status: ServiceStatus::Healthy,
            message: Some("All systems operational".to_string()),
            last_check: Utc::now(),
            uptime: Duration::from_hours(24),
            metrics: {
                let mut metrics = HashMap::new();
                metrics.insert("cpu_usage".to_string(), 45.2);
                metrics.insert("memory_usage".to_string(), 67.8);
                metrics
            },
        };

        assert_eq!(health.name, "test_service");
        assert_eq!(health.status, ServiceStatus::Healthy);
        assert_eq!(health.message, Some("All systems operational".to_string()));
        assert_eq!(health.uptime, Duration::from_hours(24));
        assert_eq!(health.metrics.len(), 2);
        assert_eq!(health.metrics["cpu_usage"], 45.2);
    }

    #[test]
    fn test_service_health_serialization() {
        let health = ServiceHealth {
            name: "serializable_service".to_string(),
            status: ServiceStatus::Degraded,
            message: Some("Performance issues detected".to_string()),
            last_check: Utc::now(),
            uptime: Duration::from_minutes(30),
            metrics: HashMap::new(),
        };

        let serialized = serde_json::to_string(&health).expect("Failed to serialize");
        let deserialized: ServiceHealth = serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(health.name, deserialized.name);
        assert_eq!(health.status, deserialized.status);
        assert_eq!(health.message, deserialized.message);
        assert_eq!(health.uptime, deserialized.uptime);
    }

    #[test]
    fn test_service_metrics_creation() {
        let metrics = ServiceMetrics {
            name: "metric_service".to_string(),
            request_count: 1000,
            error_count: 50,
            avg_response_time_ms: 125.5,
            memory_usage_bytes: 512 * 1024 * 1024, // 512MB
            cpu_usage_percent: 35.2,
            timestamp: Utc::now(),
        };

        assert_eq!(metrics.name, "metric_service");
        assert_eq!(metrics.request_count, 1000);
        assert_eq!(metrics.error_count, 50);
        assert_eq!(metrics.avg_response_time_ms, 125.5);
        assert_eq!(metrics.memory_usage_bytes, 512 * 1024 * 1024);
        assert_eq!(metrics.cpu_usage_percent, 35.2);
    }

    #[test]
    fn test_service_metrics_calculation() {
        let metrics = ServiceMetrics {
            name: "calc_service".to_string(),
            request_count: 100,
            error_count: 10,
            avg_response_time_ms: 200.0,
            memory_usage_bytes: 256 * 1024 * 1024,
            cpu_usage_percent: 50.0,
            timestamp: Utc::now(),
        };

        let success_rate = (metrics.request_count - metrics.error_count) as f64 / metrics.request_count as f64;
        assert_eq!(success_rate, 0.9); // 90% success rate

        let error_rate = metrics.error_count as f64 / metrics.request_count as f64;
        assert_eq!(error_rate, 0.1); // 10% error rate
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_tool_to_context_integration() {
        let tool = ToolDefinition {
            name: "integration_tool".to_string(),
            description: "Tool for integration testing".to_string(),
            input_schema: json!({"type": "object"}),
            category: Some("Integration".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Test Team".to_string()),
            tags: vec!["integration".to_string()],
            enabled: true,
            parameters: vec![],
        };

        let context = ToolExecutionContext::default();
        let request = ToolExecutionRequest::new(
            tool.name.clone(),
            json!({"test": "data"}),
            context,
        );

        assert_eq!(request.tool_name, tool.name);
        assert!(request.request_id.len() > 0);
    }

    #[test]
    fn test_execution_flow_simulation() {
        // Simulate a complete execution flow
        let tool_name = "flow_test_tool".to_string();
        let input_data = json!({"message": "test flow"});

        // 1. Create execution context
        let mut exec_context = ToolExecutionContext::default();
        exec_context.timeout = Some(Duration::from_secs(10));

        // 2. Create request
        let request = ToolExecutionRequest::new(
            tool_name.clone(),
            input_data.clone(),
            exec_context,
        );

        // 3. Simulate execution
        let start_time = std::time::Instant::now();
        let execution_time = start_time.elapsed();

        // 4. Create result
        let result = ToolExecutionResult::success(
            json!({"processed": true, "original": input_data}),
            execution_time,
            tool_name.clone(),
            Some(request.context.context_ref.unwrap_or_default()),
        );

        // Verify flow
        assert!(result.success);
        assert_eq!(result.tool_name, tool_name);
        assert!(result.result.is_some());
        assert!(result.context_ref.is_some());
    }

    #[test]
    fn test_error_handling_flow() {
        let tool_name = "error_prone_tool".to_string();
        let error_message = "Simulated execution error".to_string();

        let exec_context = ToolExecutionContext::default();
        let request = ToolExecutionRequest::new(
            tool_name.clone(),
            json!({"trigger_error": true}),
            exec_context,
        );

        // Simulate failed execution
        let result = ToolExecutionResult::error(
            error_message.clone(),
            Duration::from_millis(25),
            tool_name.clone(),
            Some(request.context.context_ref.unwrap_or_default()),
        );

        assert!(!result.success);
        assert_eq!(result.error, Some(error_message));
        assert_eq!(result.tool_name, tool_name);
        assert!(result.result.is_none());
    }

    #[test]
    fn test_validation_with_complex_schemas() {
        // Test validation with complex nested schemas
        let complex_schema = json!({
            "type": "object",
            "properties": {
                "nested": {
                    "type": "object",
                    "properties": {
                        "array_field": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": {"type": "string"},
                                    "value": {"type": "number"}
                                },
                                "required": ["id", "value"]
                            }
                        }
                    },
                    "required": ["array_field"]
                }
            },
            "required": ["nested"]
        });

        let tool = ToolDefinition {
            name: "complex_validation_tool".to_string(),
            description: "Tool with complex validation schema".to_string(),
            input_schema: complex_schema,
            category: Some("Validation".to_string()),
            version: Some("2.0.0".to_string()),
            author: None,
            tags: vec!["validation".to_string(), "complex".to_string()],
            enabled: true,
            parameters: vec![],
        };

        // Verify schema is preserved
        assert!(tool.input_schema["properties"]["nested"]["properties"]["array_field"]["items"]["properties"]["id"]["type"] == "string");
        assert_eq!(tool.tags, vec!["validation", "complex"]);
    }

    #[test]
    fn test_concurrent_context_creation() {
        // Test that multiple contexts can be created without conflicts
        let mut contexts = Vec::new();

        for i in 0..10 {
            let mut metadata = HashMap::new();
            metadata.insert("index".to_string(), json!(i));
            metadata.insert("thread".to_string(), json!("test"));

            let context = ContextRef::with_metadata(metadata);
            contexts.push(context);
        }

        // Verify all contexts are unique
        let mut ids = std::collections::HashSet::new();
        for context in &contexts {
            assert!(ids.insert(context.id.clone()), "Duplicate context ID found");
            assert_eq!(context.metadata.get("index"), Some(&json!(contexts.iter().position(|c| c.id == context.id).unwrap())));
        }

        assert_eq!(contexts.len(), 10);
    }

    #[test]
    fn test_large_metadata_handling() {
        // Test handling of large metadata payloads
        let mut large_metadata = HashMap::new();

        // Add many metadata entries
        for i in 0..1000 {
            large_metadata.insert(format!("key_{}", i), json!(format!("value_{}", i)));
        }

        let context = ContextRef::with_metadata(large_metadata.clone());

        // Verify all metadata is preserved
        assert_eq!(context.metadata.len(), 1000);
        for (key, value) in large_metadata {
            assert_eq!(context.metadata.get(&key), Some(&value));
        }

        // Test serialization of large metadata
        let serialized = serde_json::to_string(&context).expect("Failed to serialize large context");
        let deserialized: ContextRef = serde_json::from_str(&serialized).expect("Failed to deserialize large context");

        assert_eq!(context.id, deserialized.id);
        assert_eq!(context.metadata.len(), deserialized.metadata.len());
    }
}