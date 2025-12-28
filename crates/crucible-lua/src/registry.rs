//! Lua tool registry
//!
//! Discovers and manages Lua/Fennel tools from configured directories.
//! Uses full_moon to extract type annotations from Luau source for schemas.

use crate::error::LuaError;
use crate::executor::LuaExecutor;
use crate::schema::{self, extract_signatures};
use crate::types::{LuaTool, ToolParam, ToolResult};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, warn};

/// Registry of discovered Lua tools
pub struct LuaToolRegistry {
    /// Discovered tools by name
    tools: HashMap<String, LuaTool>,
    /// Executor for running tools
    executor: LuaExecutor,
}

impl LuaToolRegistry {
    /// Create a new empty registry
    pub fn new() -> Result<Self, LuaError> {
        Ok(Self {
            tools: HashMap::new(),
            executor: LuaExecutor::new()?,
        })
    }

    /// Discover tools from a directory
    ///
    /// Looks for .lua and .fnl files and extracts schemas from type annotations.
    pub async fn discover_from(&mut self, dir: impl AsRef<Path>) -> Result<usize, LuaError> {
        let dir = dir.as_ref();
        if !dir.exists() {
            debug!("Tool directory does not exist: {}", dir.display());
            return Ok(0);
        }

        let mut count = 0;
        let mut entries = tokio::fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // Check for .lua or .fnl extension
            let is_lua = path.extension().map(|e| e == "lua").unwrap_or(false);
            let is_fennel = path.extension().map(|e| e == "fnl").unwrap_or(false);

            if !is_lua && !is_fennel {
                continue;
            }

            match self.discover_tool(&path, is_fennel).await {
                Ok(Some(tool)) => {
                    info!("Discovered Lua tool: {} ({} params)", tool.name, tool.params.len());
                    self.tools.insert(tool.name.clone(), tool);
                    count += 1;
                }
                Ok(None) => {
                    debug!("No tool definition in: {}", path.display());
                }
                Err(e) => {
                    warn!("Failed to discover tool in {}: {}", path.display(), e);
                }
            }
        }

        Ok(count)
    }

    /// Discover a tool from a single file using type extraction
    async fn discover_tool(
        &self,
        path: &Path,
        is_fennel: bool,
    ) -> Result<Option<LuaTool>, LuaError> {
        let source = tokio::fs::read_to_string(path).await?;
        let source_path = path.to_string_lossy().to_string();

        // For Fennel, we'd need to compile first then extract
        // For now, only support Luau type extraction on .lua files
        if is_fennel {
            return self.discover_tool_from_comments(&source, &source_path, true).await;
        }

        // Try to extract from Luau type annotations first
        match extract_signatures(&source) {
            Ok(signatures) => {
                // Look for handler/main function
                let sig = signatures
                    .iter()
                    .find(|s| s.name == "handler" || s.name == "main")
                    .or_else(|| signatures.first());

                if let Some(sig) = sig {
                    let params = sig
                        .params
                        .iter()
                        .map(|p| ToolParam {
                            name: p.name.clone(),
                            param_type: schema::type_to_string(&p.type_info),
                            description: String::new(),
                            required: !p.optional,
                            default: None,
                        })
                        .collect();

                    // Extract description from doc comment if present
                    let description = sig.description.clone().unwrap_or_default();

                    return Ok(Some(LuaTool {
                        name: sig.name.clone(),
                        description,
                        params,
                        source_path,
                        is_fennel: false,
                    }));
                }
            }
            Err(e) => {
                debug!("Type extraction failed, falling back to comments: {}", e);
            }
        }

        // Fall back to comment-based discovery
        self.discover_tool_from_comments(&source, &source_path, false).await
    }

    /// Fallback: discover tool from @tool/@param comments
    async fn discover_tool_from_comments(
        &self,
        source: &str,
        source_path: &str,
        is_fennel: bool,
    ) -> Result<Option<LuaTool>, LuaError> {
        let comment_prefix = if is_fennel { ";;" } else { "--" };

        let mut name: Option<String> = None;
        let mut description = String::new();
        let mut params = Vec::new();

        for line in source.lines() {
            let line = line.trim();

            if !line.starts_with(comment_prefix) {
                if !line.is_empty() {
                    break;
                }
                continue;
            }

            let content = line.trim_start_matches(comment_prefix).trim();

            if let Some(tool_name) = content.strip_prefix("@tool ") {
                name = Some(tool_name.trim().to_string());
            } else if let Some(desc) = content.strip_prefix("@description ") {
                description = desc.trim().to_string();
            } else if let Some(param_def) = content.strip_prefix("@param ") {
                if let Some(param) = parse_param(param_def) {
                    params.push(param);
                }
            }
        }

        let name = match name {
            Some(n) => n,
            None => return Ok(None),
        };

        Ok(Some(LuaTool {
            name,
            description,
            params,
            source_path: source_path.to_string(),
            is_fennel,
        }))
    }

    /// List all registered tools
    pub fn list_tools(&self) -> Vec<&LuaTool> {
        self.tools.values().collect()
    }

    /// Get a tool by name
    pub fn get_tool(&self, name: &str) -> Option<&LuaTool> {
        self.tools.get(name)
    }

    /// Execute a tool by name
    pub async fn execute(
        &self,
        name: &str,
        args: JsonValue,
    ) -> Result<ToolResult, LuaError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| LuaError::NotFound(name.to_string()))?;

        self.executor.execute_tool(tool, args).await
    }

    /// Get JSON Schema for a tool's input
    pub fn get_input_schema(&self, name: &str) -> Option<JsonValue> {
        self.tools.get(name).map(schema::generate_input_schema)
    }

    /// Register a tool manually (e.g., for testing)
    pub fn register(&mut self, tool: LuaTool) {
        self.tools.insert(tool.name.clone(), tool);
    }
}

/// Parse a @param annotation (fallback for untyped code)
fn parse_param(s: &str) -> Option<ToolParam> {
    let parts: Vec<&str> = s.splitn(3, ' ').collect();

    if parts.len() < 2 {
        return None;
    }

    let name = parts[0].to_string();
    let param_type = parts[1].to_string();
    let description = parts.get(2).map(|s| s.to_string()).unwrap_or_default();

    let (description, required) = if description.ends_with("(optional)") {
        (
            description.trim_end_matches("(optional)").trim().to_string(),
            false,
        )
    } else {
        (description, true)
    };

    Some(ToolParam {
        name,
        param_type,
        description,
        required,
        default: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_param() {
        let param = parse_param("query string The search query").unwrap();
        assert_eq!(param.name, "query");
        assert_eq!(param.param_type, "string");
        assert_eq!(param.description, "The search query");
        assert!(param.required);

        let param = parse_param("limit number Max results (optional)").unwrap();
        assert_eq!(param.name, "limit");
        assert!(!param.required);
    }

    #[test]
    fn test_parse_param_minimal() {
        let param = parse_param("x number").unwrap();
        assert_eq!(param.name, "x");
        assert_eq!(param.param_type, "number");
        assert_eq!(param.description, "");
    }
}
