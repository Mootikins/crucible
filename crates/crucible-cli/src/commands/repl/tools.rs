// Unified tool registry using crucible-tools
//
// This module integrates the crucible-tools crate into the CLI REPL.

use anyhow::Result;
use crucible_tools::types::{ToolConfigContext, ToolError, ToolRegistry};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

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

/// Unified tool registry wrapping crucible-tools
pub struct UnifiedToolRegistry {
    _tool_dir: PathBuf,
    registry: ToolRegistry,
}

impl UnifiedToolRegistry {
    /// Create a new tool registry and initialize crucible-tools
    pub async fn new(tool_dir: PathBuf, context: ToolConfigContext) -> Result<Self> {
        // Load all tools into a new registry
        let registry = crucible_tools::load_all_tools(Arc::new(context))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to load tools: {}", e))?;

        Ok(Self {
            _tool_dir: tool_dir,
            registry,
        })
    }

    /// List all available tools
    pub fn list_tools(&self) -> Vec<String> {
        self.registry.list_tools()
    }

    /// List tools grouped by source
    pub fn list_tools_by_group(&self) -> HashMap<String, Vec<String>> {
        let mut groups = HashMap::new();
        let tools = self.list_tools();
        
        // For now, group everything under "system" as we don't have external tools yet
        groups.insert("system".to_string(), tools);
        
        groups
    }

    /// Execute a tool by name
    pub async fn execute_tool(&self, tool_name: &str, args: &[String]) -> Result<ToolResult> {
        // Parse arguments into JSON parameters
        // This is a simplified assumption that args are key=value pairs or a single JSON string
        // For a robust REPL, we might want better parsing
        
        let parameters = if args.len() == 1 && args[0].starts_with('{') {
            // Try parsing as JSON
            serde_json::from_str(&args[0]).unwrap_or_else(|_| {
                // Fallback to treating as a single string argument named "arg"
                serde_json::json!({ "arg": args[0] })
            })
        } else {
            // Parse key=value pairs
            let mut params = serde_json::Map::new();
            for arg in args {
                if let Some((key, value)) = arg.split_once('=') {
                    params.insert(key.to_string(), serde_json::Value::String(value.to_string()));
                } else {
                    // Positional args not fully supported in this simple mapper, 
                    // but we can try to map them if we knew the schema.
                    // For now, treat as error or handle specific tools specially if needed.
                    // Or just put them in a list?
                }
            }
            
            // Special handling for common tools to make CLI usage easier
            if tool_name == "read_note" && args.len() == 1 && !args[0].contains('=') {
                 params.insert("name".to_string(), serde_json::Value::String(args[0].to_string()));
            } else if tool_name == "search_notes" && args.len() >= 1 && !args[0].contains('=') {
                 params.insert("query".to_string(), serde_json::Value::String(args[0].to_string()));
            }
            
            serde_json::Value::Object(params)
        };

        // Execute the tool
        let result = self.registry.execute_tool(
            tool_name.to_string(),
            parameters,
            Some("cli_user".to_string()), // TODO: Get real user
            Some("cli_session".to_string()), // TODO: Get real session
        ).await;

        match result {
            Ok(res) => {
                let output = if let Some(data) = res.data {
                    serde_json::to_string_pretty(&data).unwrap_or_else(|_| "Result not serializable".to_string())
                } else {
                    "Success".to_string()
                };
                
                Ok(ToolResult {
                    status: ToolStatus::Success,
                    output,
                })
            },
            Err(e) => {
                Ok(ToolResult {
                    status: ToolStatus::Error(e.to_string()),
                    output: String::new(),
                })
            }
        }
    }

    /// Get tool schema
    pub fn get_tool_schema(&self, tool_name: &str) -> Result<Option<ToolSchema>> {
        match self.registry.get_definition(tool_name) {
            Some(def) => Ok(Some(ToolSchema {
                name: def.name.clone(),
                description: def.description.clone(),
                input_schema: def.input_schema.clone(),
            })),
            None => Ok(None),
        }
    }
}
