//! Tool execution abstraction trait
//!
//! This trait defines the interface for executing tools (MCP tools, Rune scripts, etc.)
//! as part of the agent workflow.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result type for tool operations
pub type ToolResult<T> = Result<T, ToolError>;

/// Tool execution errors
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Tool executor abstraction
///
/// This trait defines the interface for executing tools in Crucible's agent system.
/// Tools can be implemented as:
/// - MCP (Model Context Protocol) tools
/// - Rune scripts with tool definitions
/// - Native Rust functions
///
/// ## Design Rationale
///
/// The trait is intentionally minimal to support multiple tool implementations:
/// - `execute_tool()` - Execute a single tool with parameters
/// - `list_tools()` - Discover available tools (for agent planning)
///
/// ## Thread Safety
///
/// Implementations must be Send + Sync to enable concurrent tool execution.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute a tool with given parameters
    ///
    /// # Arguments
    ///
    /// * `name` - The tool name/identifier
    /// * `params` - Tool parameters as a JSON value
    /// * `context` - Execution context (workspace path, user info, etc.)
    ///
    /// # Returns
    ///
    /// Returns the tool execution result as a JSON value, or a `ToolError`.
    ///
    /// See trait implementation for usage.
    async fn execute_tool(
        &self,
        name: &str,
        params: serde_json::Value,
        context: &ExecutionContext,
    ) -> ToolResult<serde_json::Value>;

    /// List all available tools
    ///
    /// Returns metadata about available tools for agent discovery and planning.
    ///
    /// # Returns
    ///
    /// Returns a vector of tool definitions, or a `ToolError`.
    async fn list_tools(&self) -> ToolResult<Vec<ToolDefinition>>;
}

/// Execution context for tool invocations
///
/// Provides environment and user context for tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    /// Current workspace/kiln path
    pub workspace_path: Option<String>,

    /// User identifier
    pub user_id: Option<String>,

    /// Session identifier
    pub session_id: Option<String>,

    /// Additional context metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ExecutionContext {
    /// Create a new execution context
    pub fn new() -> Self {
        Self {
            workspace_path: None,
            user_id: None,
            session_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Set the workspace path
    pub fn with_workspace(mut self, path: impl Into<String>) -> Self {
        self.workspace_path = Some(path.into());
        self
    }

    /// Set the user ID
    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set the session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Tool definition metadata
///
/// Describes a tool's capabilities, parameters, and usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name/identifier
    pub name: String,

    /// Human-readable description
    pub description: String,

    /// Tool category (e.g., "query", "transform", "export")
    pub category: Option<String>,

    /// Parameter schema (JSON Schema format)
    pub parameters: Option<serde_json::Value>,

    /// Return type schema (JSON Schema format)
    pub returns: Option<serde_json::Value>,

    /// Example usage
    pub examples: Vec<ToolExample>,

    /// Required permissions
    pub required_permissions: Vec<String>,
}

impl ToolDefinition {
    /// Create a new tool definition
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            category: None,
            parameters: None,
            returns: None,
            examples: Vec::new(),
            required_permissions: Vec::new(),
        }
    }

    /// Set the category
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Set the parameters schema
    pub fn with_parameters(mut self, schema: serde_json::Value) -> Self {
        self.parameters = Some(schema);
        self
    }

    /// Add an example
    pub fn with_example(mut self, example: ToolExample) -> Self {
        self.examples.push(example);
        self
    }

    /// Add a required permission
    pub fn with_permission(mut self, permission: impl Into<String>) -> Self {
        self.required_permissions.push(permission.into());
        self
    }
}

/// Tool usage example
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExample {
    /// Example description
    pub description: String,

    /// Example parameters
    pub parameters: serde_json::Value,

    /// Expected result (optional)
    pub result: Option<serde_json::Value>,
}

impl ToolExample {
    /// Create a new tool example
    pub fn new(description: impl Into<String>, parameters: serde_json::Value) -> Self {
        Self {
            description: description.into(),
            parameters,
            result: None,
        }
    }

    /// Set the expected result
    pub fn with_result(mut self, result: serde_json::Value) -> Self {
        self.result = Some(result);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_context_builder() {
        let ctx = ExecutionContext::new()
            .with_workspace("/path/to/kiln")
            .with_user("user123")
            .with_session("session456")
            .with_metadata("key", serde_json::json!("value"));

        assert_eq!(ctx.workspace_path, Some("/path/to/kiln".to_string()));
        assert_eq!(ctx.user_id, Some("user123".to_string()));
        assert_eq!(ctx.session_id, Some("session456".to_string()));
        assert_eq!(ctx.metadata.get("key"), Some(&serde_json::json!("value")));
    }

    #[test]
    fn test_tool_definition_builder() {
        let def = ToolDefinition::new("query_notes", "Query notes by criteria")
            .with_category("query")
            .with_parameters(serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                }
            }))
            .with_permission("read:notes");

        assert_eq!(def.name, "query_notes");
        assert_eq!(def.category, Some("query".to_string()));
        assert_eq!(def.required_permissions, vec!["read:notes"]);
    }

    #[test]
    fn test_tool_example() {
        let example = ToolExample::new(
            "Query AI notes",
            serde_json::json!({"query": "SELECT * FROM notes WHERE tags CONTAINS 'ai'"}),
        )
        .with_result(serde_json::json!([{"id": "note:1", "title": "AI Note"}]));

        assert_eq!(example.description, "Query AI notes");
        assert!(example.result.is_some());
    }
}
