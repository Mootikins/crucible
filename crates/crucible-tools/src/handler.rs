//! Dynamic tool handler generation
//!
//! This module provides dynamic handler generation for Rune tools,
//! allowing them to be executed through the service layer.

use crate::errors::{RuneError, ContextualError, ErrorContext};
use crate::tool::RuneTool;
use crate::types::ExecutionConfig;
use anyhow::Result;
use crucible_services::ToolService;
use crucible_services::types::tool::{ToolDefinition, ToolExecutionRequest, ToolExecutionResult, ValidationResult};
use crucible_services::types::tool::ToolExecutionContext;
use uuid::Uuid;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Dynamic tool handler for Rune tools
pub struct DynamicRuneToolHandler {
    /// The Rune tool
    tool: Arc<RuneTool>,
    /// Rune context for execution
    context: Arc<rune::Context>,
    /// Execution configuration
    config: ExecutionConfig,
    /// Execution statistics
    stats: Arc<RwLock<HandlerStats>>,
}

/// Handler statistics
#[derive(Debug, Clone, Default)]
pub struct HandlerStats {
    /// Total executions
    pub total_executions: u64,
    /// Successful executions
    pub successful_executions: u64,
    /// Failed executions
    pub failed_executions: u64,
    /// Average execution time in milliseconds
    pub avg_execution_time_ms: f64,
    /// Last execution timestamp
    pub last_execution: Option<chrono::DateTime<chrono::Utc>>,
    /// Error breakdown
    pub errors_by_type: std::collections::HashMap<String, u64>,
}

impl DynamicRuneToolHandler {
    /// Create a new dynamic tool handler
    pub fn new(
        tool: Arc<RuneTool>,
        context: Arc<rune::Context>,
        config: ExecutionConfig,
    ) -> Self {
        Self {
            tool,
            context,
            config,
            stats: Arc::new(RwLock::new(HandlerStats::default())),
        }
    }

    /// Execute the tool with the given request
    pub async fn execute(&self, request: &ToolExecutionRequest) -> Result<ToolExecutionResult, ContextualError> {
        let start_time = std::time::Instant::now();
        let execution_id = request.execution_id.clone();

        let context = ErrorContext::new()
            .with_operation("execute_tool")
            .with_tool_name(&self.tool.name);

        // Validate tool is enabled
        if !self.tool.enabled {
            return Err(ContextualError::new(
                RuneError::ExecutionError {
                    tool_name: self.tool.name.clone(),
                    execution_id: Some(execution_id),
                    source: anyhow::anyhow!("Tool is disabled"),
                },
                context,
            ));
        }

        // Validate parameters
        if let Err(e) = self.validate_parameters(&request.parameters) {
            self.record_execution_stats(start_time, false, Some(&e.to_string())).await;
            return Err(ContextualError::new(
                RuneError::ValidationError {
                    message: format!("Parameter validation failed: {}", e),
                    field: None,
                    value: Some(request.parameters.clone()),
                },
                context,
            ));
        }

        // Execute with timeout
        let execution_future = self.execute_tool(&request.parameters);
        let result = if let Some(timeout_ms) = self.config.timeout_ms {
            match tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                execution_future,
            )
            .await
            {
                Ok(result) => result,
                Err(_) => {
                    let error = anyhow::anyhow!("Tool execution timed out after {}ms", timeout_ms);
                    self.record_execution_stats(start_time, false, Some(&error.to_string())).await;
                    return Err(ContextualError::new(
                        RuneError::TimeoutError {
                            message: format!("Execution timed out after {}ms", timeout_ms),
                            timeout_ms,
                            elapsed_ms: timeout_ms,
                        },
                        context,
                    ));
                }
            }
        } else {
            execution_future.await
        };

        match result {
            Ok(output) => {
                let execution_time_ms = start_time.elapsed().as_millis() as u64;
                let execution_result = ToolExecutionResult {
                    execution_id: request.execution_id.clone(),
                    tool_name: self.tool.name.clone(),
                    success: true,
                    output,
                    error: None,
                    execution_time_ms,
                    metadata: {
                        let mut meta = std::collections::HashMap::new();
                        meta.insert("tool_id".to_string(), serde_json::Value::String(self.tool.id.clone()));
                        meta.insert("tool_version".to_string(), serde_json::Value::String(self.tool.version.clone()));
                        meta
                    },
                };

                self.record_execution_stats(start_time, true, None).await;
                info!("Tool '{}' executed successfully in {}ms", self.tool.name, execution_time_ms);

                Ok(execution_result)
            }
            Err(e) => {
                let execution_time_ms = start_time.elapsed().as_millis() as u64;
                let error_msg = e.to_string();

                self.record_execution_stats(start_time, false, Some(&error_msg)).await;
                error!("Tool '{}' execution failed: {}", self.tool.name, error_msg);

                Err(ContextualError::new(
                    RuneError::ExecutionError {
                        tool_name: self.tool.name.clone(),
                        execution_id: Some(request.execution_id.clone()),
                        source: e,
                    },
                    context,
                ))
            }
        }
    }

    /// Execute the actual tool logic
    async fn execute_tool(&self, parameters: &serde_json::Value) -> Result<serde_json::Value> {
        // Create execution context
        let execution_context = ToolExecutionContext {
            execution_id: Uuid::new_v4().to_string(),
            tool_name: self.tool.name.clone(),
            user_id: parameters
                .get("user_id")
                .and_then(|v| v.as_str())
                .unwrap_or("anonymous")
                .to_string(),
            session_id: parameters
                .get("session_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            timestamp: chrono::Utc::now(),
            metadata: std::collections::HashMap::new(),
        };

        // Call the tool
        self.tool.call(parameters.clone(), &self.context).await
    }

    /// Validate input parameters
    fn validate_parameters(&self, parameters: &serde_json::Value) -> Result<()> {
        // Basic validation
        if !parameters.is_object() {
            return Err(anyhow::anyhow!("Parameters must be a JSON object"));
        }

        // Validate against tool's input schema if available
        if let Some(schema) = &self.tool.input_schema.as_object() {
            if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
                for required_field in required {
                    if let Some(field_name) = required_field.as_str() {
                        if !parameters.get(field_name).is_some() {
                            return Err(anyhow::anyhow!("Missing required field: {}", field_name));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Record execution statistics
    async fn record_execution_stats(&self, start_time: std::time::Instant, success: bool, error: Option<&str>) {
        let execution_time_ms = start_time.elapsed().as_millis() as u64;
        let mut stats = self.stats.write().await;

        stats.total_executions += 1;
        if success {
            stats.successful_executions += 1;
        } else {
            stats.failed_executions += 1;
            if let Some(error_msg) = error {
                let error_type = self.categorize_error(error_msg);
                *stats.errors_by_type.entry(error_type).or_insert(0) += 1;
            }
        }

        // Update average execution time
        if stats.total_executions > 0 {
            stats.avg_execution_time_ms =
                (stats.avg_execution_time_ms * (stats.total_executions - 1) as f64 + execution_time_ms as f64)
                / stats.total_executions as f64;
        }

        stats.last_execution = Some(chrono::Utc::now());
    }

    /// Categorize error type for statistics
    fn categorize_error(&self, error_msg: &str) -> String {
        if error_msg.contains("timeout") {
            "timeout".to_string()
        } else if error_msg.contains("validation") {
            "validation".to_string()
        } else if error_msg.contains("compilation") {
            "compilation".to_string()
        } else if error_msg.contains("runtime") {
            "runtime".to_string()
        } else {
            "other".to_string()
        }
    }

    /// Get handler statistics
    pub async fn get_stats(&self) -> HandlerStats {
        self.stats.read().await.clone()
    }

    /// Get tool information
    pub fn get_tool_info(&self) -> &RuneTool {
        &self.tool
    }

    /// Update configuration
    pub fn update_config(&mut self, config: ExecutionConfig) {
        self.config = config;
    }

    /// Check if the handler is healthy
    pub async fn health_check(&self) -> Result<bool> {
        // Basic health check - verify tool is still valid
        if !self.tool.enabled {
            return Ok(false);
        }

        // Check if tool needs reloading
        if let Ok(needs_reload) = self.tool.needs_reload().await {
            if needs_reload {
                warn!("Tool '{}' needs reloading", self.tool.name);
                return Ok(false);
            }
        }

        Ok(true)
    }
}

/// Tool handler generator for creating handlers from tools
pub struct ToolHandlerGenerator {
    /// Default execution configuration
    default_config: ExecutionConfig,
}

impl ToolHandlerGenerator {
    /// Create a new handler generator
    pub fn new() -> Self {
        Self {
            default_config: ExecutionConfig::default(),
        }
    }

    /// Create a handler generator with default configuration
    pub fn with_config(config: ExecutionConfig) -> Self {
        Self {
            default_config: config,
        }
    }

    /// Generate a handler for a tool
    pub fn generate_handler(
        &self,
        tool: Arc<RuneTool>,
        context: Arc<rune::Context>,
    ) -> DynamicRuneToolHandler {
        DynamicRuneToolHandler::new(tool, context, self.default_config.clone())
    }

    /// Generate a handler with custom configuration
    pub fn generate_handler_with_config(
        &self,
        tool: Arc<RuneTool>,
        context: Arc<rune::Context>,
        config: ExecutionConfig,
    ) -> DynamicRuneToolHandler {
        DynamicRuneToolHandler::new(tool, context, config)
    }

    /// Generate handlers for multiple tools
    pub fn generate_handlers(
        &self,
        tools: Vec<Arc<RuneTool>>,
        context: Arc<rune::Context>,
    ) -> std::collections::HashMap<String, DynamicRuneToolHandler> {
        let mut handlers = std::collections::HashMap::new();

        for tool in tools {
            let handler = self.generate_handler(tool.clone(), context.clone());
            handlers.insert(tool.name.clone(), handler);
        }

        handlers
    }

    /// Generate handlers with per-tool configuration
    pub fn generate_handlers_with_configs(
        &self,
        tools: Vec<Arc<RuneTool>>,
        context: Arc<rune::Context>,
        config_fn: impl Fn(&RuneTool) -> ExecutionConfig,
    ) -> std::collections::HashMap<String, DynamicRuneToolHandler> {
        let mut handlers = std::collections::HashMap::new();

        for tool in tools {
            let config = config_fn(&tool);
            let handler = self.generate_handler_with_config(tool.clone(), context.clone(), config);
            handlers.insert(tool.name.clone(), handler);
        }

        handlers
    }
}

impl Default for ToolHandlerGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch handler for executing multiple tools
pub struct BatchToolHandler {
    /// Individual handlers
    handlers: std::collections::HashMap<String, DynamicRuneToolHandler>,
    /// Batch execution configuration
    batch_config: BatchConfig,
}

/// Configuration for batch execution
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum concurrent executions
    pub max_concurrent: usize,
    /// Default timeout for batch operations
    pub batch_timeout_ms: Option<u64>,
    /// Whether to continue on individual failures
    pub continue_on_failure: bool,
    /// Whether to collect partial results
    pub collect_partial_results: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 10,
            batch_timeout_ms: Some(60_000), // 1 minute
            continue_on_failure: true,
            collect_partial_results: true,
        }
    }
}

impl BatchToolHandler {
    /// Create a new batch handler
    pub fn new(handlers: std::collections::HashMap<String, DynamicRuneToolHandler>) -> Self {
        Self {
            handlers,
            batch_config: BatchConfig::default(),
        }
    }

    /// Create a batch handler with custom configuration
    pub fn with_config(
        handlers: std::collections::HashMap<String, DynamicRuneToolHandler>,
        batch_config: BatchConfig,
    ) -> Self {
        Self {
            handlers,
            batch_config,
        }
    }

    /// Execute multiple tools in batch
    pub async fn execute_batch(
        &self,
        requests: Vec<ToolExecutionRequest>,
    ) -> Vec<Result<ToolExecutionResult, ContextualError>> {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.batch_config.max_concurrent));
        let mut tasks = Vec::new();

        for request in requests {
            if let Some(handler) = self.handlers.get(&request.tool_name) {
                let semaphore_clone = Arc::clone(&semaphore);
                let handler_clone = handler.clone();
                let continue_on_failure = self.batch_config.continue_on_failure;

                let task = tokio::spawn(async move {
                    let _permit = semaphore_clone.acquire().await.unwrap();

                    match handler_clone.execute(&request).await {
                        Ok(result) => Ok(result),
                        Err(e) => {
                            if continue_on_failure {
                                Err(e)
                            } else {
                                // In a real implementation, you might want to cancel other tasks
                                Err(e)
                            }
                        }
                    }
                });

                tasks.push(task);
            } else {
                // Tool not found
                let error = ContextualError::new(
                    RuneError::RegistryError {
                        message: format!("Tool '{}' not found", request.tool_name),
                        operation: Some("execute".to_string()),
                    },
                    ErrorContext::new().with_tool_name(&request.tool_name),
                );

                if self.batch_config.collect_partial_results {
                    tasks.push(tokio::spawn(async move { Err(error) }));
                }
            }
        }

        // Wait for all tasks to complete
        let mut results = Vec::new();
        for task in tasks {
            match task.await {
                Ok(result) => results.push(result),
                Err(e) => {
                    // Task join error
                    let error = ContextualError::new(
                        RuneError::ExecutionError {
                            tool_name: "unknown".to_string(),
                            execution_id: None,
                            source: anyhow::anyhow!("Task join error: {}", e),
                        },
                        ErrorContext::new().with_operation("batch_execute"),
                    );
                    results.push(Err(error));
                }
            }
        }

        results
    }

    /// Get information about available handlers
    pub fn get_handler_names(&self) -> Vec<String> {
        self.handlers.keys().cloned().collect()
    }

    /// Get a specific handler
    pub fn get_handler(&self, tool_name: &str) -> Option<&DynamicRuneToolHandler> {
        self.handlers.get(tool_name)
    }

    /// Check if all handlers are healthy
    pub async fn health_check_all(&self) -> std::collections::HashMap<String, bool> {
        let mut results = std::collections::HashMap::new();

        for (name, handler) in &self.handlers {
            results.insert(name.clone(), handler.health_check().await.unwrap_or(false));
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::create_safe_context;
    use std::fs;

    #[tokio::test]
    async fn test_handler_generation() -> Result<(), Box<dyn std::error::Error>> {
        let context = create_safe_context()?;

        // Create a test tool
        let tool_source = r#"
            pub fn NAME() { "test_tool" }
            pub fn DESCRIPTION() { "A test tool" }
            pub fn INPUT_SCHEMA() {
                #{ type: "object", properties: #{ name: #{ type: "string" } } }
            }
            pub async fn call(args) {
                #{ success: true, message: `Hello ${args.name}` }
            }
        "#;

        let mut tool = RuneTool::from_source(tool_source, &context, None)?;
        let tool = Arc::new(tool);

        // Generate handler
        let generator = ToolHandlerGenerator::new();
        let handler = generator.generate_handler(tool.clone(), context);

        // Check handler info
        assert_eq!(handler.get_tool_info().name, "test_tool");

        // Test execution
        let request = ToolExecutionRequest {
            execution_id: "test-123".to_string(),
            tool_name: "test_tool".to_string(),
            parameters: serde_json::json!({"name": "World"}),
            context: ToolExecutionContext {
                execution_id: "test-123".to_string(),
                tool_name: "test_tool".to_string(),
                user_id: "test".to_string(),
                session_id: None,
                timestamp: chrono::Utc::now(),
                metadata: std::collections::HashMap::new(),
            },
        };

        let result = handler.execute(&request).await?;
        assert!(result.success);
        assert_eq!(result.output["success"], true);
        assert_eq!(result.output["message"], "Hello World");

        Ok(())
    }

    #[tokio::test]
    async fn test_handler_stats() -> Result<(), Box<dyn std::error::Error>> {
        let context = create_safe_context()?;

        let tool_source = r#"
            pub fn NAME() { "stats_tool" }
            pub fn DESCRIPTION() { "A tool for stats testing" }
            pub fn INPUT_SCHEMA() { #{ type: "object", properties: #{} } }
            pub async fn call(args) {
                #{ success: true }
            }
        "#;

        let tool = Arc::new(RuneTool::from_source(tool_source, &context, None)?);
        let generator = ToolHandlerGenerator::new();
        let handler = generator.generate_handler(tool, context);

        // Check initial stats
        let stats = handler.get_stats().await;
        assert_eq!(stats.total_executions, 0);

        // Execute tool
        let request = ToolExecutionRequest {
            execution_id: "stats-test".to_string(),
            tool_name: "stats_tool".to_string(),
            parameters: serde_json::json!({}),
            context: ToolExecutionContext {
                execution_id: "stats-test".to_string(),
                tool_name: "stats_tool".to_string(),
                user_id: "test".to_string(),
                session_id: None,
                timestamp: chrono::Utc::now(),
                metadata: std::collections::HashMap::new(),
            },
        };

        let _ = handler.execute(&request).await?;

        // Check updated stats
        let stats = handler.get_stats().await;
        assert_eq!(stats.total_executions, 1);
        assert_eq!(stats.successful_executions, 1);
        assert!(stats.last_execution.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_batch_handler() -> Result<(), Box<dyn std::error::Error>> {
        let context = create_safe_context()?;

        // Create multiple tools
        let tool1_source = r#"
            pub fn NAME() { "tool1" }
            pub fn DESCRIPTION() { "Tool 1" }
            pub fn INPUT_SCHEMA() { #{ type: "object", properties: #{} } }
            pub async fn call(args) { #{ tool: 1, success: true } }
        "#;

        let tool2_source = r#"
            pub fn NAME() { "tool2" }
            pub fn DESCRIPTION() { "Tool 2" }
            pub fn INPUT_SCHEMA() { #{ type: "object", properties: #{} } }
            pub async fn call(args) { #{ tool: 2, success: true } }
        "#;

        let tool1 = Arc::new(RuneTool::from_source(tool1_source, &context, None)?);
        let tool2 = Arc::new(RuneTool::from_source(tool2_source, &context, None)?);

        // Generate handlers
        let generator = ToolHandlerGenerator::new();
        let handlers = generator.generate_handlers(vec![tool1, tool2], context);

        // Create batch handler
        let batch_handler = BatchToolHandler::new(handlers);

        // Execute batch
        let requests = vec![
            ToolExecutionRequest {
                execution_id: "batch-1".to_string(),
                tool_name: "tool1".to_string(),
                parameters: serde_json::json!({}),
                context: ToolExecutionContext {
                    execution_id: "batch-1".to_string(),
                    tool_name: "tool1".to_string(),
                    user_id: "test".to_string(),
                    session_id: None,
                    timestamp: chrono::Utc::now(),
                    metadata: std::collections::HashMap::new(),
                },
            },
            ToolExecutionRequest {
                execution_id: "batch-2".to_string(),
                tool_name: "tool2".to_string(),
                parameters: serde_json::json!({}),
                context: ToolExecutionContext {
                    execution_id: "batch-2".to_string(),
                    tool_name: "tool2".to_string(),
                    user_id: "test".to_string(),
                    session_id: None,
                    timestamp: chrono::Utc::now(),
                    metadata: std::collections::HashMap::new(),
                },
            },
        ];

        let results = batch_handler.execute_batch(requests).await;
        assert_eq!(results.len(), 2);

        for result in results {
            assert!(result.is_ok());
            let execution_result = result.unwrap();
            assert!(execution_result.success);
        }

        Ok(())
    }
}