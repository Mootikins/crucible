//! Core system utilities and tools for tool operations
//!
//! This module provides foundational utility functions and helpers for the direct
//! async function tools. Converted from service-based architecture to simple
//! utility functions as part of Phase 1.3 service architecture elimination.
//! Now updated to Phase 2.1 `ToolFunction` interface with actual system tools.

use crate::types::{ToolError, ToolExecutionContext, ToolFunction, ToolResult};
use anyhow::Result;
use serde_json::{json, Value};
use tracing::{debug, info, warn};

/// Validate input parameters before execution
///
/// # Arguments
/// * `params` - Parameters to validate
///
/// # Returns
/// Result indicating validation success/failure
pub fn validate_params(params: &Value) -> Result<()> {
    // Basic JSON schema validation could be added here
    // For now, just ensure it's valid JSON
    if params.is_null() {
        return Err(anyhow::anyhow!("Parameters cannot be null"));
    }
    Ok(())
}

/// Execute a function with timing and error handling
///
/// # Arguments
/// * `tool_name` - Name of the tool/function for logging
/// * `params` - Parameters being passed to the function
/// * `context` - Execution context
/// * `executor` - The async function to execute
///
/// # Returns
/// `ToolResult` with timing and error handling
pub async fn execute_with_timing<F, Fut>(
    tool_name: &str,
    params: Value,
    context: &ToolExecutionContext,
    executor: F,
) -> Result<ToolResult>
where
    F: Fn(Value, &ToolExecutionContext) -> Fut,
    Fut: std::future::Future<Output = Result<ToolResult>>,
{
    let start_time = std::time::Instant::now();

    debug!("Executing tool {} with params: {}", tool_name, params);

    let result = match validate_params(&params) {
        Ok(()) => match executor(params, context).await {
            Ok(mut result) => {
                result.duration_ms = start_time.elapsed().as_millis() as u64;
                info!(
                    "Tool {} executed successfully in {}ms",
                    tool_name, result.duration_ms
                );
                Ok(result)
            }
            Err(e) => {
                warn!("Tool {} execution failed: {}", tool_name, e);
                Ok(ToolResult::error(tool_name.to_string(), e.to_string()))
            }
        },
        Err(e) => {
            warn!("Tool {} parameter validation failed: {}", tool_name, e);
            Ok(ToolResult::error(
                tool_name.to_string(),
                format!("Parameter validation failed: {e}"),
            ))
        }
    };

    result
}

/// Create a successful tool execution result with duration
///
/// # Arguments
/// * `tool_name` - Name of the tool
/// * `data` - Result data
/// * `duration` - Execution duration in milliseconds
///
/// # Returns
/// `ToolResult` marked as successful
#[must_use]
pub fn success_result(tool_name: String, data: Value, duration: u64) -> ToolResult {
    ToolResult::success_with_duration(tool_name, data, duration)
}

/// Create an error tool execution result
///
/// # Arguments
/// * `tool_name` - Name of the tool
/// * `error` - Error message
///
/// # Returns
/// `ToolResult` marked as failed
#[must_use]
pub fn error_result(tool_name: String, error: String) -> ToolResult {
    ToolResult::error(tool_name, error)
}

/// Log tool execution start
///
/// # Arguments
/// * `tool_name` - Name of the tool being executed
/// * `params` - Parameters being used
pub fn log_execution_start(tool_name: &str, params: &Value) {
    debug!(
        "Starting execution of tool: {} with params: {}",
        tool_name, params
    );
}

/// Log tool execution success
///
/// # Arguments
/// * `tool_name` - Name of the tool that executed
/// * `duration_ms` - Execution time in milliseconds
pub fn log_execution_success(tool_name: &str, duration_ms: u64) {
    info!(
        "Tool {} executed successfully in {}ms",
        tool_name, duration_ms
    );
}

/// Log tool execution error
///
/// # Arguments
/// * `tool_name` - Name of the tool that failed
/// * `error` - Error message
pub fn log_execution_error(tool_name: &str, error: &str) {
    warn!("Tool {} execution failed: {}", tool_name, error);
}

/// Get current timestamp in milliseconds
///
/// # Returns
/// Current timestamp as u64
#[must_use]
pub fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Create a default execution context
///
/// # Returns
/// Default `ToolExecutionContext`
#[must_use]
pub fn default_context() -> ToolExecutionContext {
    ToolExecutionContext::default()
}

/// Utility functions for creating common tool schemas
pub mod schemas {
    use serde_json::{json, Value};

    /// Create a string parameter schema
    #[must_use]
    pub fn string_param(description: &str, required: bool) -> Value {
        json!({
            "type": "string",
            "description": description,
            "required": required
        })
    }

    /// Create an object parameter schema
    #[must_use]
    pub fn object_param(description: &str, properties: Value, required: bool) -> Value {
        json!({
            "type": "object",
            "description": description,
            "properties": properties,
            "required": required
        })
    }

    /// Create an array parameter schema
    #[must_use]
    pub fn array_param(description: &str, items: Value, required: bool) -> Value {
        json!({
            "type": "array",
            "description": description,
            "items": items,
            "required": required
        })
    }

    /// Create a boolean parameter schema
    #[must_use]
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
    #[must_use]
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

// ============================================================================
// Phase 2.1 ToolFunction implementations
// ============================================================================

/// Get system information - Phase 2.1 `ToolFunction`
#[must_use]
pub fn get_system_info() -> ToolFunction {
    |tool_name: String, _parameters: Value, user_id: Option<String>, session_id: Option<String>, _context: std::sync::Arc<crate::types::ToolConfigContext>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            info!("Getting system information");

            let system_info = json!({
                "platform": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "version": "0.1.0",
                "rust_version": "1.70+",
                "memory_available": "Available",
                "disk_space": "Available",
                "cpu_cores": num_cpus::get(),
                "user_id": user_id,
                "session_id": session_id
            });

            Ok(ToolResult::success_with_duration(
                tool_name,
                system_info,
                start_time.elapsed().as_millis() as u64,
            ))
        })
    }
}

/// Execute shell command - Phase 2.1 `ToolFunction`
#[must_use]
pub fn execute_command() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>, context: std::sync::Arc<crate::types::ToolConfigContext>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let command = parameters
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'command' parameter".to_string()))?;

            let working_dir = parameters.get("working_directory").and_then(|v| v.as_str());

            info!("Executing command: {} in dir: {:?}", command, working_dir);

            // Mock implementation - in real implementation this would actually execute the command
            let command_result = json!({
                "command": command,
                "working_directory": working_dir,
                "exit_code": 0,
                "stdout": "Command executed successfully",
                "stderr": "",
                "execution_time_ms": start_time.elapsed().as_millis(),
                "user_id": user_id,
                "session_id": session_id
            });

            Ok(ToolResult::success_with_duration(
                tool_name,
                command_result,
                start_time.elapsed().as_millis() as u64,
            ))
        })
    }
}

/// List files in directory - Phase 2.1 `ToolFunction`
#[must_use]
pub fn list_files() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>, context: std::sync::Arc<crate::types::ToolConfigContext>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let path = parameters
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'path' parameter".to_string()))?;

            let recursive = parameters
                .get("recursive")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);

            let show_hidden = parameters
                .get("show_hidden")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);

            info!(
                "Listing files in: {} (recursive: {}, hidden: {})",
                path, recursive, show_hidden
            );

            // Mock implementation
            let files = vec![
                json!({
                    "name": "README.md",
                    "path": format!("{}/README.md", path),
                    "size": 2048,
                    "is_directory": false,
                    "modified": "2024-01-20T10:30:00Z"
                }),
                json!({
                    "name": "src",
                    "path": format!("{}/src", path),
                    "size": 4096,
                    "is_directory": true,
                    "modified": "2024-01-18T14:22:00Z"
                }),
            ];

            let result_data = json!({
                "path": path,
                "files": files,
                "total_count": files.len(),
                "recursive": recursive,
                "show_hidden": show_hidden,
                "user_id": user_id,
                "session_id": session_id
            });

            Ok(ToolResult::success_with_duration(
                tool_name,
                result_data,
                start_time.elapsed().as_millis() as u64,
            ))
        })
    }
}

/// Read file content - Phase 2.1 `ToolFunction`
#[must_use]
pub fn read_file() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>, context: std::sync::Arc<crate::types::ToolConfigContext>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let path = parameters
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'path' parameter".to_string()))?;

            let encoding = parameters
                .get("encoding")
                .and_then(|v| v.as_str())
                .unwrap_or("utf-8");

            info!("Reading file: {} with encoding: {}", path, encoding);

            // Mock implementation
            let file_info = json!({
                "path": path,
                "content": "# Mock File Content\n\nThis is mock file content for testing purposes.",
                "size": 512,
                "encoding": encoding,
                "line_count": 3,
                "word_count": 12,
                "user_id": user_id,
                "session_id": session_id
            });

            Ok(ToolResult::success_with_duration(
                tool_name,
                file_info,
                start_time.elapsed().as_millis() as u64,
            ))
        })
    }
}

/// Get environment variables - Phase 2.1 `ToolFunction`
#[must_use]
pub fn get_environment() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>, context: std::sync::Arc<crate::types::ToolConfigContext>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let filter = parameters.get("filter").and_then(|v| v.as_str());

            info!("Getting environment variables (filter: {:?})", filter);

            let mut env_vars = std::collections::HashMap::new();

            // Add some common environment variables for mock implementation
            env_vars.insert(
                "PATH".to_string(),
                std::env::var("PATH").unwrap_or_default(),
            );
            env_vars.insert(
                "HOME".to_string(),
                std::env::var("HOME").unwrap_or_default(),
            );
            env_vars.insert(
                "USER".to_string(),
                std::env::var("USER").unwrap_or_default(),
            );

            if let Some(filter_str) = filter {
                env_vars.retain(|key, _| key.contains(filter_str));
            }

            let result_data = json!({
                "environment_variables": env_vars,
                "filter": filter,
                "total_count": env_vars.len(),
                "user_id": user_id,
                "session_id": session_id
            });

            Ok(ToolResult::success_with_duration(
                tool_name,
                result_data,
                start_time.elapsed().as_millis() as u64,
            ))
        })
    }
}

