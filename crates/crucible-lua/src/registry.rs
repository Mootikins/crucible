//! Lua tool registry
//!
//! Discovers and manages Lua/Fennel tools from configured directories.
//! Uses LDoc-style annotations (@tool, @param) for schema extraction.

use crate::error::LuaError;
use crate::executor::LuaExecutor;
use crate::schema;
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
    /// Looks for .lua and .fnl files and extracts schemas from LDoc annotations.
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
                    info!(
                        "Discovered Lua tool: {} ({} params)",
                        tool.name,
                        tool.params.len()
                    );
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

    /// Discover a tool from a single file using LDoc annotations
    async fn discover_tool(
        &self,
        path: &Path,
        is_fennel: bool,
    ) -> Result<Option<LuaTool>, LuaError> {
        let source = tokio::fs::read_to_string(path).await?;
        let source_path = path.to_string_lossy().to_string();

        self.discover_tool_from_annotations(&source, &source_path, is_fennel)
            .await
    }

    /// Discover tool from @tool/@param annotations
    async fn discover_tool_from_annotations(
        &self,
        source: &str,
        source_path: &str,
        is_fennel: bool,
    ) -> Result<Option<LuaTool>, LuaError> {
        // For Fennel, both ;; and ;;; are doc comments
        // For Lua, -- is a comment, --- is a doc comment (LDoc style)
        let comment_prefix = if is_fennel { ";;" } else { "--" };

        let mut name: Option<String> = None;
        let mut description = String::new();
        let mut params = Vec::new();
        let mut in_header = true;

        for line in source.lines() {
            let line = line.trim();

            // Check if this is a comment line
            if !line.starts_with(comment_prefix) {
                // Non-empty, non-comment line ends the header
                if !line.is_empty() {
                    in_header = false;
                }
                continue;
            }

            if !in_header {
                continue;
            }

            // Strip comment prefix and any extra dashes/semicolons
            let content = line
                .trim_start_matches(comment_prefix)
                .trim_start_matches('-')
                .trim_start_matches(';')
                .trim();

            // Parse @tool annotation
            if content.starts_with("@tool") {
                let rest = content.strip_prefix("@tool").unwrap().trim();
                // Handle @tool name="foo" or just @tool foo
                if let Some(tool_name) = parse_annotation_value(rest, "name") {
                    name = Some(tool_name);
                } else if !rest.is_empty() && !rest.starts_with("desc") {
                    name = Some(rest.split_whitespace().next().unwrap_or("").to_string());
                }
                // If no name given, we'll derive from filename later
                if name.is_none() || name.as_ref().map(|n| n.is_empty()).unwrap_or(false) {
                    name = Some(derive_name_from_path(source_path));
                }

                // Also check for inline description
                if let Some(desc) = parse_annotation_value(rest, "desc") {
                    description = desc;
                }
            }
            // Parse @description annotation
            else if let Some(desc) = content.strip_prefix("@description ") {
                description = desc.trim().to_string();
            }
            // Parse @param annotation
            else if let Some(param_def) = content.strip_prefix("@param ") {
                if let Some(param) = parse_param(param_def) {
                    params.push(param);
                }
            }
            // Plain doc comment before @tool becomes description
            else if name.is_none() && !content.starts_with('@') && !content.is_empty() {
                if !description.is_empty() {
                    description.push(' ');
                }
                description.push_str(content);
            }
        }

        // No @tool annotation found
        let name = match name {
            Some(n) if !n.is_empty() => n,
            _ => return Ok(None),
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
    pub async fn execute(&self, name: &str, args: JsonValue) -> Result<ToolResult, LuaError> {
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

/// Parse a @param annotation
/// Format: @param name type Description text
/// Optional types end with ? (e.g., number?)
fn parse_param(s: &str) -> Option<ToolParam> {
    let parts: Vec<&str> = s.splitn(3, ' ').collect();

    if parts.len() < 2 {
        return None;
    }

    let name = parts[0].to_string();
    let mut param_type = parts[1].to_string();
    let description = parts.get(2).map(|s| s.to_string()).unwrap_or_default();

    // Check for optional type (T? syntax or "(optional)" in description)
    let required = if param_type.ends_with('?') {
        param_type = param_type.trim_end_matches('?').to_string();
        false
    } else {
        // Backwards compat: description containing "(optional)" also makes it optional
        !description.contains("(optional)")
    };

    let description = description
        .trim_end_matches("(optional)")
        .trim()
        .to_string();

    Some(ToolParam {
        name,
        param_type,
        description,
        required,
        default: None,
    })
}

/// Parse key="value" from annotation string
fn parse_annotation_value(s: &str, key: &str) -> Option<String> {
    let pattern = format!("{}=\"", key);
    if let Some(start) = s.find(&pattern) {
        let rest = &s[start + pattern.len()..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }

    // Also try key='value' with single quotes
    let pattern = format!("{}='", key);
    if let Some(start) = s.find(&pattern) {
        let rest = &s[start + pattern.len()..];
        if let Some(end) = rest.find('\'') {
            return Some(rest[..end].to_string());
        }
    }

    None
}

/// Derive tool name from file path
fn derive_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
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
        assert!(!param.required); // (optional) in desc makes it optional
    }

    #[test]
    fn test_parse_param_optional_type() {
        let param = parse_param("limit number? Max results").unwrap();
        assert_eq!(param.name, "limit");
        assert_eq!(param.param_type, "number");
        assert_eq!(param.description, "Max results");
        assert!(!param.required);
    }

    #[test]
    fn test_parse_param_minimal() {
        let param = parse_param("x number").unwrap();
        assert_eq!(param.name, "x");
        assert_eq!(param.param_type, "number");
        assert_eq!(param.description, "");
    }

    #[test]
    fn test_parse_annotation_value() {
        assert_eq!(
            parse_annotation_value("desc=\"Search notes\"", "desc"),
            Some("Search notes".to_string())
        );
        assert_eq!(
            parse_annotation_value("name='search'", "name"),
            Some("search".to_string())
        );
        assert_eq!(parse_annotation_value("desc=\"test\"", "name"), None);
    }

    #[test]
    fn test_derive_name_from_path() {
        assert_eq!(derive_name_from_path("/tools/search.lua"), "search");
        assert_eq!(derive_name_from_path("foo/bar.fnl"), "bar");
    }
}
