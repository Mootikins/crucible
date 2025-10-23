//! Tool Group Trait for Unified Tool System
//!
//! This module defines the ToolGroup trait that provides a unified interface
//! for different types of tools in the Crucible system:
//! - System tools (crucible-tools)
//! - Rune tools (scripted tools)
//! - MCP server tools (external servers)
//!
//! The trait enables the REPL to work with multiple tool sources while
//! maintaining a consistent interface for discovery and execution.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use super::types::ToolResult;

/// Result type for tool group operations
pub type ToolGroupResult<T> = Result<T, ToolGroupError>;

/// Errors that can occur during tool group operations
#[derive(Debug, thiserror::Error)]
pub enum ToolGroupError {
    #[error("Tool group initialization failed: {0}")]
    InitializationFailed(String),

    #[error("Tool discovery failed: {0}")]
    DiscoveryFailed(String),

    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Parameter conversion failed: {0}")]
    ParameterConversionFailed(String),

    #[error("Result conversion failed: {0}")]
    ResultConversionFailed(String),

    #[error("Tool group error: {0}")]
    Other(String),
}

/// Schema information for a tool
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolSchema {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// JSON schema for parameters
    pub input_schema: Value,
    /// Expected output type description
    pub output_schema: Option<Value>,
}

/// Trait for tool groups that can be discovered and executed through the REPL
///
/// This trait provides a unified interface for different types of tools:
/// - System tools (Rust functions from crucible-tools)
/// - Rune tools (scripted .rn files)
/// - MCP server tools (external tool servers)
#[async_trait]
pub trait ToolGroup: std::fmt::Debug + Send + Sync {
    /// Get the name of this tool group
    fn group_name(&self) -> &str;

    /// Get a description of this tool group
    fn group_description(&self) -> &str;

    /// Discover all available tools in this group
    async fn discover_tools(&mut self) -> ToolGroupResult<Vec<String>>;

    /// List all currently available tools in this group
    fn list_tools(&self) -> Vec<String>;

    /// Get schema information for a specific tool
    async fn get_tool_schema(&self, tool_name: &str) -> ToolGroupResult<Option<ToolSchema>>;

    /// Execute a tool from this group
    async fn execute_tool(
        &self,
        tool_name: &str,
        args: &[String],
    ) -> ToolGroupResult<ToolResult>;

    /// Check if this group has been initialized
    fn is_initialized(&self) -> bool;

    /// Initialize the tool group (if needed)
    async fn initialize(&mut self) -> ToolGroupResult<()>;

    /// Get metadata about this tool group
    fn get_metadata(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

/// Helper trait for tool parameter conversion
pub trait ParameterConverter {
    /// Convert string arguments to the expected parameter format
    fn convert_args_to_params(&self, tool_name: &str, args: &[String]) -> ToolGroupResult<Value>;

    /// Validate that the provided parameters match the tool's schema
    fn validate_params(&self, tool_name: &str, params: &Value) -> ToolGroupResult<()>;
}

/// Helper trait for tool result conversion
pub trait ResultConverter {
    /// Convert tool execution result to the standard ToolResult format
    fn convert_to_tool_result(&self, tool_name: &str, raw_result: Value) -> ToolGroupResult<ToolResult>;
}

/// Registry for managing multiple tool groups
#[derive(Debug)]
pub struct ToolGroupRegistry {
    groups: HashMap<String, Box<dyn ToolGroup>>,
    tool_to_group: HashMap<String, String>, // Maps tool name to group name
}

impl ToolGroupRegistry {
    /// Create a new tool group registry
    pub fn new() -> Self {
        Self {
            groups: HashMap::new(),
            tool_to_group: HashMap::new(),
        }
    }

    /// Register a tool group
    pub async fn register_group(&mut self, group: Box<dyn ToolGroup>) -> ToolGroupResult<()> {
        let group_name = group.group_name().to_string();

        // Initialize the group
        let mut group = group;
        if !group.is_initialized() {
            group.initialize().await?;
        }

        // Discover tools from the group
        let tools = group.discover_tools().await?;

        // Map tools to this group
        for tool_name in &tools {
            self.tool_to_group.insert(tool_name.clone(), group_name.clone());
        }

        // Register the group
        self.groups.insert(group_name.clone(), group);

        tracing::info!("Registered tool group '{}' with {} tools", group_name, tools.len());
        Ok(())
    }

    /// List all tools from all groups
    pub fn list_all_tools(&self) -> Vec<String> {
        self.tool_to_group.keys().cloned().collect()
    }

    /// List tools grouped by source
    pub fn list_tools_by_group(&self) -> HashMap<String, Vec<String>> {
        let mut grouped = HashMap::new();

        for (tool_name, group_name) in &self.tool_to_group {
            grouped
                .entry(group_name.clone())
                .or_insert_with(Vec::new)
                .push(tool_name.clone());
        }

        // Sort tools within each group
        for tools in grouped.values_mut() {
            tools.sort();
        }

        grouped
    }

    /// Get the group that owns a specific tool
    pub fn get_tool_group(&self, tool_name: &str) -> Option<&str> {
        self.tool_to_group.get(tool_name).map(|s| s.as_str())
    }

    /// Get all registered groups
    pub fn list_groups(&self) -> Vec<String> {
        self.groups.keys().cloned().collect()
    }

    /// Execute a tool by finding the appropriate group
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        args: &[String],
    ) -> ToolGroupResult<ToolResult> {
        let group_name = self.get_tool_group(tool_name)
            .ok_or_else(|| ToolGroupError::ToolNotFound(format!(
                "Tool '{}' not found in any group", tool_name
            )))?;

        let group = self.groups.get(group_name)
            .ok_or_else(|| ToolGroupError::ToolNotFound(format!(
                "Group '{}' not found for tool '{}'", group_name, tool_name
            )))?;

        group.execute_tool(tool_name, args).await
    }

    /// Get schema for a tool
    pub async fn get_tool_schema(&self, tool_name: &str) -> ToolGroupResult<Option<ToolSchema>> {
        let group_name = self.get_tool_group(tool_name)
            .ok_or_else(|| ToolGroupError::ToolNotFound(format!(
                "Tool '{}' not found in any group", tool_name
            )))?;

        let group = self.groups.get(group_name)
            .ok_or_else(|| ToolGroupError::ToolNotFound(format!(
                "Group '{}' not found for tool '{}'", group_name, tool_name
            )))?;

        group.get_tool_schema(tool_name).await
    }
}

impl Default for ToolGroupRegistry {
    fn default() -> Self {
        Self::new()
    }
}