// Minimal tool registry stub (Rune tools removed from MVP)
//
// This is a temporary stub to keep the REPL compiling after Rune removal.
// TODO: Re-implement tool system in Phase 4+ if needed

use anyhow::Result;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;

/// Status of tool execution
#[derive(Debug, Clone)]
pub enum ToolStatus {
    Success,
    Error(String),
}

/// Result of tool execution
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub status: ToolStatus,
    pub output: String,
}

/// Tool schema for documentation
#[derive(Debug, Clone)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: JsonValue,
}

/// Unified tool registry (stub - tools removed from MVP)
pub struct UnifiedToolRegistry {
    _tool_dir: PathBuf,
}

impl UnifiedToolRegistry {
    /// Create a new tool registry
    pub async fn new(_tool_dir: PathBuf) -> Result<Self> {
        Ok(Self { _tool_dir })
    }

    /// List all available tools (returns empty - no tools in MVP)
    pub async fn list_tools(&self) -> Vec<String> {
        Vec::new()
    }

    /// List tools grouped by source (returns empty - no tools in MVP)
    pub async fn list_tools_by_group(&self) -> HashMap<String, Vec<String>> {
        HashMap::new()
    }

    /// Execute a tool by name (always fails - no tools in MVP)
    pub async fn execute_tool(&self, tool_name: &str, _args: &[String]) -> Result<ToolResult> {
        Err(anyhow::anyhow!(
            "Tool '{}' not found or execution failed in all registries",
            tool_name
        ))
    }

    /// Get tool schema (always None - no tools in MVP)
    pub async fn get_tool_schema(&self, _tool_name: &str) -> Result<Option<ToolSchema>> {
        Ok(None)
    }
}
