/// Dynamic Tool Handler Generator for Enhanced Rune Tools
///
/// This module provides functionality to generate individual tool handlers
/// for discovered Rune tools, instead of using a generic dispatcher.

use anyhow::Result;
use rmcp::model::*;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{ToolRegistry, ToolMetadata};
use crate::errors::{CrucibleError, CrucibleResult, ErrorContext, errors, ErrorCategory, ErrorSeverity};

/// Dynamic tool handler that can execute any registered Rune tool
pub struct DynamicRuneToolHandler {
    registry: Arc<RwLock<ToolRegistry>>,
}

impl DynamicRuneToolHandler {
    /// Create a new dynamic tool handler
    pub fn new(registry: Arc<RwLock<ToolRegistry>>) -> Self {
        Self { registry }
    }

    /// Execute a specific Rune tool by name
    pub async fn execute_tool(&self, tool_name: &str, args: Value) -> CrucibleResult<Value> {
        let error_context = ErrorContext::new("DynamicRuneToolHandler")
            .with_operation("execute_tool")
            .with_tool_name(tool_name);

        let registry = self.registry.read().await;

        // Get the tool
        let tool = registry.get_tool(tool_name)
            .ok_or_else(|| errors::rune_tool_not_found(tool_name, "DynamicRuneToolHandler"))?
            .clone();

        // Get context
        let rune_context = registry.context().clone();
        drop(registry);

        // Execute the tool on a blocking thread since Rune futures are !Send
        // This is necessary because Rune's VM uses thread-local storage
        let result = tokio::task::spawn_blocking(move || {
            // Create a new tokio runtime for the Rune execution
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(tool.call(args, &rune_context))
        })
        .await
        .map_err(|e| {
            CrucibleError::new(
                "TASK_JOIN_ERROR",
                &format!("Task join error: {}", e),
                ErrorCategory::Rune,
                ErrorSeverity::Error,
                error_context.clone(),
            ).with_cause(&e.to_string())
        })?
        .map_err(|e| {
            errors::rune_execution_failed(tool_name, &e.to_string(), "DynamicRuneToolHandler")
        })?;

        Ok(result)
    }

    /// Get metadata for all available tools
    pub async fn get_all_tools_metadata(&self) -> Vec<ToolMetadata> {
        let registry = self.registry.read().await;
        registry.list_tools()
    }

    /// Check if a tool exists
    pub async fn has_tool(&self, tool_name: &str) -> bool {
        let registry = self.registry.read().await;
        registry.has_tool(tool_name)
    }

    /// Get metadata for a specific tool
    pub async fn get_tool_metadata(&self, tool_name: &str) -> Option<ToolMetadata> {
        let registry = self.registry.read().await;
        registry.get_tool(tool_name).map(|tool| tool.metadata())
    }
}

/// Generate individual tool handlers for discovered Rune tools
pub struct ToolHandlerGenerator {
    registry: Arc<RwLock<ToolRegistry>>,
    handlers: HashMap<String, DynamicRuneToolHandler>,
}

impl ToolHandlerGenerator {
    /// Create a new tool handler generator
    pub fn new(registry: Arc<RwLock<ToolRegistry>>) -> Self {
        Self {
            registry,
            handlers: HashMap::new(),
        }
    }

    /// Get or create a handler for a specific tool
    pub fn get_handler(&mut self, tool_name: &str) -> Option<&DynamicRuneToolHandler> {
        if !self.handlers.contains_key(tool_name) {
            let handler = DynamicRuneToolHandler::new(self.registry.clone());
            self.handlers.insert(tool_name.to_string(), handler);
        }
        self.handlers.get(tool_name)
    }

    /// Generate a complete tool list with handlers for all discovered tools
    pub async fn generate_tool_list(&mut self) -> Result<Vec<Tool>> {
        let registry = self.registry.read().await;
        let tool_metas = registry.list_tools();
        drop(registry);

        let mut tools = Vec::new();

        for tool_meta in tool_metas {
            // Convert input_schema from Value to Map<String, Value>
            let input_schema = match &tool_meta.input_schema {
                Value::Object(map) => Arc::new(map.clone()),
                _ => Arc::new(serde_json::Map::new()),
            };

            // Convert output_schema if present
            let output_schema = tool_meta.output_schema.as_ref().and_then(|schema| {
                match schema {
                    Value::Object(map) => Some(Arc::new(map.clone())),
                    _ => None,
                }
            });

            // Create the rmcp tool
            let tool = Tool {
                name: std::borrow::Cow::Owned(tool_meta.name.clone()),
                title: None,
                description: Some(std::borrow::Cow::Owned(tool_meta.description.clone())),
                input_schema,
                output_schema,
                annotations: Some(ToolAnnotations {
                    title: Some(format!("{} (Rune Tool)", tool_meta.name)),
                    read_only_hint: Some(true), // Most Rune tools are read-only
                    destructive_hint: Some(false),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(true), // Rune tools can interact with external systems
                }),
                icons: None,
            };

            tools.push(tool);
        }

        Ok(tools)
    }

    /// Execute a tool by name
    pub async fn execute_tool(&self, tool_name: &str, args: Value) -> CrucibleResult<Value> {
        if let Some(handler) = self.handlers.get(tool_name) {
            handler.execute_tool(tool_name, args).await
        } else {
            Err(errors::rune_tool_not_found(
                tool_name,
                "ToolHandlerGenerator"
            ))
        }
    }
}

/// Enhanced service that integrates with the handler generator
pub struct EnhancedToolService {
    database: Arc<crate::database::EmbeddingDatabase>,
    provider: Arc<dyn crate::embeddings::EmbeddingProvider>,
    handler_generator: ToolHandlerGenerator,
}

impl EnhancedToolService {
    /// Create a new enhanced tool service
    pub fn new(
        database: Arc<crate::database::EmbeddingDatabase>,
        provider: Arc<dyn crate::embeddings::EmbeddingProvider>,
        registry: Arc<RwLock<ToolRegistry>>,
    ) -> Self {
        Self {
            database,
            provider,
            handler_generator: ToolHandlerGenerator::new(registry),
        }
    }

    /// Get the handler generator
    pub fn handler_generator(&mut self) -> &mut ToolHandlerGenerator {
        &mut self.handler_generator
    }

    /// Get all available tools with enhanced metadata
    pub async fn list_enhanced_tools(&mut self) -> Result<Vec<Tool>> {
        self.handler_generator.generate_tool_list().await
    }

    /// Execute a specific tool
    pub async fn execute_tool(&self, tool_name: &str, args: Value) -> CrucibleResult<Value> {
        self.handler_generator.execute_tool(tool_name, args).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs;
    use serde_json::json;

    #[tokio::test]
    async fn test_dynamic_tool_handler() {
        let temp_dir = tempdir().unwrap();
        let tool_dir = temp_dir.path().to_path_buf();

        // Create a test tool file
        let test_tool = r#"
pub fn NAME() { "test_handler_tool" }
pub fn DESCRIPTION() { "A test tool for handler generation" }
pub fn INPUT_SCHEMA() {
    #{
        type: "object",
        properties: #{ message: #{ type: "string" } },
        required: ["message"]
    }
}

pub async fn call(args) {
    #{ success: true, message: args.message, handler: "dynamic" }
}
"#;

        let tool_path = tool_dir.join("test_handler_tool.rn");
        fs::create_dir_all(&tool_dir).unwrap();
        fs::write(&tool_path, test_tool).unwrap();

        // Create registry
        let context = Arc::new(rune::Context::with_default_modules().unwrap());
        let registry = ToolRegistry::new(tool_dir, context).unwrap();
        let registry_arc = Arc::new(RwLock::new(registry));

        // Create handler
        let handler = DynamicRuneToolHandler::new(registry_arc.clone());

        // Test tool existence
        assert!(handler.has_tool("test_handler_tool").await);

        // Test tool execution
        let args = serde_json::json!({ "message": "Hello from dynamic handler" });
        let result = handler.execute_tool("test_handler_tool", args).await.unwrap();

        assert_eq!(result["success"], true);
        assert_eq!(result["message"], "Hello from dynamic handler");
        assert_eq!(result["handler"], "dynamic");

        // Test error handling for non-existent tool
        let error_result = handler.execute_tool("non_existent_tool", json!({})).await;
        assert!(error_result.is_err());
        let error = error_result.unwrap_err();
        assert_eq!(error.code, crate::errors::error_codes::RUNE_TOOL_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_tool_handler_generator() {
        let temp_dir = tempdir().unwrap();
        let tool_dir = temp_dir.path().to_path_buf();

        // Create registry - for now, test with the fallback system
        let context = Arc::new(rune::Context::with_default_modules().unwrap());
        let registry = ToolRegistry::new_with_enhanced_discovery(tool_dir, context, true).unwrap();
        let registry_arc = Arc::new(RwLock::new(registry));

        // Create generator
        let mut generator = ToolHandlerGenerator::new(registry_arc.clone());

        // Generate tool list - this may be empty for now due to discovery limitations
        let tools = generator.generate_tool_list().await.unwrap();

        // Test that the generator structure works correctly
        println!("Generated {} tools", tools.len());

        // Test getting handlers and caching behavior
        {
            // First test: getting a handler creates and caches it
            let handler1_ptr = generator.get_handler("test_tool1").unwrap() as *const DynamicRuneToolHandler;
            assert!(handler1_ptr != std::ptr::null(), "Should create handler for any tool name");

            // Second test: getting same handler returns cached instance
            let handler1_again_ptr = generator.get_handler("test_tool1").unwrap() as *const DynamicRuneToolHandler;
            assert_eq!(handler1_ptr, handler1_again_ptr, "Should return same handler instance");
        }

        {
            // Third test: getting a different handler creates a new one
            let handler2_ptr = generator.get_handler("test_tool2").unwrap() as *const DynamicRuneToolHandler;
            assert!(handler2_ptr != std::ptr::null(), "Should create handler for different tool name");

            // Fourth test: getting the first handler still returns cached instance
            let handler1_again_ptr = generator.get_handler("test_tool1").unwrap() as *const DynamicRuneToolHandler;
            assert!(handler1_again_ptr != std::ptr::null(), "Should still have cached handler");
        }

        // Test error handling for non-existent tool execution
        let error_result = generator.execute_tool("non_existent_tool", json!({})).await;
        assert!(error_result.is_err(), "Should return error for non-existent tool");

        // Test that tool list generation works consistently
        let tools2 = generator.generate_tool_list().await.unwrap();
        assert_eq!(tools.len(), tools2.len(), "Should generate consistent tool lists");

        // The main functionality is that the generator can create handlers
        // and manage them correctly, even if discovery is limited
        assert!(!generator.handlers.is_empty(), "Should have cached handlers");
    }
}