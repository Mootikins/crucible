//! Core system tool implementations
//!
//! This module provides the foundational tool framework and core system tools
//! that provide basic functionality for file operations, database interactions,
//! and system utilities.

use crate::types::*;
use crate::registry::ToolRegistry;
use crate::types::{ToolDefinition, ToolExecutionContext, ToolExecutionResult, ContextRef};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Trait for tool implementations
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool definition
    fn definition(&self) -> &ToolDefinition;

    /// Execute the tool with given parameters
    async fn execute(
        &self,
        params: Value,
        context: &ToolExecutionContext,
    ) -> Result<ToolExecutionResult>;

    /// Validate input parameters before execution
    fn validate_params(&self, params: &Value) -> Result<()> {
        // Basic JSON schema validation could be added here
        // For now, just ensure it's valid JSON
        if params.is_null() {
            return Err(anyhow::anyhow!("Parameters cannot be null"));
        }
        Ok(())
    }

    /// Get the tool category
    fn category(&self) -> Option<String> {
        self.definition().category.clone()
    }
}

/// Base tool implementation with common functionality
pub struct BaseTool {
    definition: ToolDefinition,
    executor: Arc<dyn Fn(Value, &ToolExecutionContext) -> Result<ToolExecutionResult> + Send + Sync>,
}

impl BaseTool {
    pub fn new<F>(definition: ToolDefinition, executor: F) -> Self
    where
        F: Fn(Value, &ToolExecutionContext) -> Result<ToolExecutionResult> + Send + Sync + 'static,
    {
        Self {
            definition,
            executor: Arc::new(executor),
        }
    }
}

#[async_trait]
impl Tool for BaseTool {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    async fn execute(
        &self,
        params: Value,
        context: &ToolExecutionContext,
    ) -> Result<ToolExecutionResult> {
        let start_time = std::time::Instant::now();

        debug!("Executing tool {} with params: {}", self.definition.name, params);

        let result = match self.validate_params(&params) {
            Ok(()) => {
                match (self.executor)(params, context) {
                    Ok(mut result) => {
                        result.execution_time = start_time.elapsed();
                        info!("Tool {} executed successfully in {}ms",
                              self.definition.name, result.execution_time.as_millis());
                        Ok(result)
                    }
                    Err(e) => {
                        error!("Tool {} execution failed: {}", self.definition.name, e);
                        Ok(ToolExecutionResult {
                            success: false,
                            result: None,
                            error: Some(e.to_string()),
                            execution_time: start_time.elapsed(),
                            tool_name: self.definition.name.clone(),
                            context_ref: Some(ContextRef::new()),
                        })
                    }
                }
            }
            Err(e) => {
                warn!("Tool {} parameter validation failed: {}", self.definition.name, e);
                Ok(ToolExecutionResult {
                    success: false,
                    result: None,
                    error: Some(format!("Parameter validation failed: {}", e)),
                    execution_time: start_time.elapsed(),
                    tool_name: self.definition.name.clone(),
                    context_ref: Some(ContextRef::new()),
                })
            }
        };

        result
    }
}

/// Tool manager for organizing and executing tools
pub struct ToolManager {
    tools: HashMap<String, Arc<dyn Tool>>,
    registry: ToolRegistry,
}

impl ToolManager {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            registry: ToolRegistry::new(),
        }
    }

    /// Register a tool
    pub fn register_tool<T: Tool + 'static>(&mut self, tool: T) {
        let tool = Arc::new(tool);
        let name = tool.definition().name.clone();
        self.registry.register_tool(tool.definition().clone());
        self.tools.insert(name, tool);
    }

    /// Execute a tool by name
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        params: Value,
        context: ToolExecutionContext,
    ) -> Result<ToolExecutionResult> {
        match self.tools.get(tool_name) {
            Some(tool) => {
                // Note: deprecated field removed from simplified ToolDefinition
                tool.execute(params, &context).await
            }
            None => Ok(ToolExecutionResult {
                success: false,
                result: None,
                error: Some(format!("Tool not found: {}", tool_name)),
                execution_time: std::time::Duration::from_millis(0),
                duration: std::time::Duration::from_millis(0),
                completed_at: chrono::Utc::now(),
                tool_name: tool_name.to_string(),
                context: None,
                context_ref: Some(ContextRef::new()),
                metadata: std::collections::HashMap::new(),
            })
        }
    }

    /// Get all registered tools
    pub fn list_tools(&self) -> Vec<&ToolDefinition> {
        self.registry.list_tools()
    }

    /// Get tools by category
    pub fn list_tools_by_category(&self, category: &ToolCategory) -> Vec<&ToolDefinition> {
        self.registry.list_tools_by_category(category)
    }

    /// Get a specific tool definition
    pub fn get_tool_definition(&self, name: &str) -> Option<&ToolDefinition> {
        self.registry.get_tool(name)
    }

    /// Search tools by name or description
    pub fn search_tools(&self, query: &str) -> Vec<&ToolDefinition> {
        let query = query.to_lowercase();
        self.tools
            .values()
            .filter(|tool| {
                tool.definition().name.to_lowercase().contains(&query)
                    || tool.definition().description.to_lowercase().contains(&query)
            })
            .map(|tool| tool.definition())
            .collect()
    }
}

impl Default for ToolManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility functions for creating common tool schemas
pub mod schemas {
    use serde_json::{json, Value};

    /// Create a string parameter schema
    pub fn string_param(description: &str, required: bool) -> Value {
        json!({
            "type": "string",
            "description": description,
            "required": required
        })
    }

    /// Create an object parameter schema
    pub fn object_param(description: &str, properties: Value, required: bool) -> Value {
        json!({
            "type": "object",
            "description": description,
            "properties": properties,
            "required": required
        })
    }

    /// Create an array parameter schema
    pub fn array_param(description: &str, items: Value, required: bool) -> Value {
        json!({
            "type": "array",
            "description": description,
            "items": items,
            "required": required
        })
    }

    /// Create a boolean parameter schema
    pub fn boolean_param(description: &str, default: Option<bool>) -> Value {
        let mut schema = json!({
            "type": "boolean",
            "description": description
        });
        if let Some(default_val) = default {
            schema["default"] = json!(default_val);
        }
        schema
    }

    /// Create a success response schema
    pub fn success_response(data_schema: Option<Value>) -> Value {
        let mut response = json!({
            "type": "object",
            "properties": {
                "success": {"type": "boolean"},
                "error": {"type": "string"}
            },
            "required": ["success"]
        });

        if let Some(schema) = data_schema {
            response["properties"]["data"] = schema;
        } else {
            response["properties"]["data"] = json!({});
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_registry() {
        let mut registry = ToolRegistry::new();

        let tool = ToolDefinition {
            name: "test_tool".to_string(),
            description: "Test tool".to_string(),
            input_schema: json!({}),
            category: Some("System".to_string()),
            version: Some("1.0.0".to_string()),
            author: None,
            tags: vec![],
            enabled: true,
            parameters: vec![],
        };

        registry.register_tool(tool);

        assert_eq!(registry.tools.len(), 1);
        assert!(registry.get_tool("test_tool").is_some());
        assert!(registry.list_tools_by_category(&ToolCategory::System).len() == 1);
    }

    #[tokio::test]
    async fn test_tool_manager() {
        let mut manager = ToolManager::new();

        let tool = BaseTool::new(
            ToolDefinition {
                name: "echo_tool".to_string(),
                description: "Echo tool for testing".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "message": {"type": "string"}
                    },
                    "required": ["message"]
                }),
                category: Some("System".to_string()),
                version: Some("1.0.0".to_string()),
                author: None,
                tags: vec![],
                enabled: true,
                parameters: vec![],
            },
            |params, _context| {
                let message = params["message"].as_str().unwrap_or("no message");
                Ok(ToolExecutionResult {
                    success: true,
                    result: Some(json!({"echo": message})),
                    error: None,
                    execution_time: std::time::Duration::from_millis(0),
                    tool_name: "echo_tool".to_string(),
                    context_ref: Some(ContextRef::new()),
                })
            },
        );

        manager.register_tool(tool);

        let context = ToolExecutionContext::default();

        let result = manager.execute_tool(
            "echo_tool",
            json!({"message": "hello world"}),
            context,
        ).await.unwrap();

        assert!(result.success);
        assert_eq!(result.result.unwrap()["echo"], "hello world");
    }

    #[test]
    fn test_tool_validation() {
        let tool = BaseTool::new(
            ToolDefinition {
                name: "test_tool".to_string(),
                description: "Test tool".to_string(),
                input_schema: json!({}),
                category: Some("System".to_string()),
                version: Some("1.0.0".to_string()),
                author: None,
                tags: vec![],
                enabled: true,
                parameters: vec![],
            },
            |_params, _context| {
                Ok(ToolExecutionResult {
                    success: true,
                    result: None,
                    error: None,
                    execution_time: std::time::Duration::from_millis(0),
                    tool_name: "test_tool".to_string(),
                    context_ref: Some(ContextRef::new()),
                })
            },
        );

        // Valid params should pass
        assert!(tool.validate_params(&json!({})).is_ok());

        // Null params should fail
        assert!(tool.validate_params(&Value::Null).is_err());
    }
}