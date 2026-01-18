//! Tool discovery for agents to search available tools at runtime
//!
//! This module provides tools for agents to discover and inspect available tools
//! at runtime, enabling proactive tool search ("what tools can help with X?") and
//! progressive disclosure (don't dump all tools in context).

use rmcp::model::{CallToolResult, Content, Tool};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Parameters for discovering available tools
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscoverToolsParams {
    /// Optional search query to filter tools by name or description
    #[serde(default)]
    pub query: Option<String>,
    /// Optional source filter: "builtin", "rune", "just", "upstream"
    #[serde(default)]
    pub source: Option<String>,
    /// Maximum number of results to return (default: 50)
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

/// Parameters for getting a specific tool's schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetToolSchemaParams {
    /// The name of the tool to get the schema for
    pub name: String,
}

/// Summary information about a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    /// The tool's unique name
    pub name: String,
    /// Human-readable description of what the tool does
    pub description: String,
    /// Source classification: "builtin", "rune", "just", or "upstream"
    pub source: String,
}

/// Full schema information for a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    /// The tool's unique name
    pub name: String,
    /// Human-readable description of what the tool does
    pub description: String,
    /// Source classification: "builtin", "rune", "just", or "upstream"
    pub source: String,
    /// JSON Schema describing the tool's input parameters
    pub input_schema: serde_json::Value,
}

/// Runtime tool discovery service
///
/// Enables agents to search for tools by name/description and retrieve
/// full schemas before invoking tools programmatically.
pub struct ToolDiscovery {
    tools: Vec<Tool>,
}

impl ToolDiscovery {
    /// Create a new tool discovery instance with the given tool list
    #[must_use]
    pub fn new(tools: Vec<Tool>) -> Self {
        Self { tools }
    }

    fn classify_source(name: &str) -> &'static str {
        if name.starts_with("rune_") {
            "rune"
        } else if name.starts_with("just_") {
            "just"
        } else if name.contains("::") || name.starts_with("gh_") || name.starts_with("mcp_") {
            "upstream"
        } else {
            "builtin"
        }
    }

    /// Search for tools matching the given query and filters.
    ///
    /// # Errors
    /// This function is infallible - always returns Ok.
    pub fn discover_tools(
        &self,
        params: &DiscoverToolsParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let query = params.query.as_deref().unwrap_or("").to_lowercase();
        let source_filter = params.source.as_deref();

        let mut matches: Vec<ToolInfo> = self
            .tools
            .iter()
            .filter(|t| {
                let name = t.name.as_ref();
                let desc = t.description.as_deref().unwrap_or("");
                let source = Self::classify_source(name);

                let matches_query = query.is_empty()
                    || name.to_lowercase().contains(&query)
                    || desc.to_lowercase().contains(&query);

                let matches_source = source_filter.is_none() || source_filter == Some(source);

                matches_query && matches_source
            })
            .take(params.limit)
            .map(|t| ToolInfo {
                name: t.name.to_string(),
                description: t.description.as_deref().unwrap_or("").to_string(),
                source: Self::classify_source(t.name.as_ref()).to_string(),
            })
            .collect();

        matches.sort_by(|a, b| a.name.cmp(&b.name));

        let output = json!({
            "count": matches.len(),
            "tools": matches
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output).unwrap_or_else(|_| output.to_string()),
        )]))
    }

    /// Get the full schema for a specific tool by name.
    ///
    /// # Errors
    /// Returns `ErrorData::invalid_params` if the tool name is not found.
    pub fn get_tool_schema(
        &self,
        params: &GetToolSchemaParams,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tool = self.tools.iter().find(|t| t.name.as_ref() == params.name);

        match tool {
            Some(t) => {
                let schema = ToolSchema {
                    name: t.name.to_string(),
                    description: t.description.as_deref().unwrap_or("").to_string(),
                    source: Self::classify_source(t.name.as_ref()).to_string(),
                    input_schema: serde_json::Value::Object((*t.input_schema).clone()),
                };

                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&schema)
                        .unwrap_or_else(|_| json!(schema).to_string()),
                )]))
            }
            None => Err(rmcp::ErrorData::invalid_params(
                format!("Tool '{}' not found", params.name),
                None,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;
    use std::borrow::Cow;
    use std::sync::Arc;

    fn make_tool(name: &str, desc: &str) -> Tool {
        let mut schema = Map::new();
        schema.insert("type".to_string(), json!("object"));
        schema.insert("properties".to_string(), json!({}));

        Tool {
            name: Cow::Owned(name.to_string()),
            description: Some(Cow::Owned(desc.to_string())),
            input_schema: Arc::new(schema),
            annotations: None,
            title: None,
            output_schema: None,
            icons: None,
            meta: None,
        }
    }

    #[test]
    fn test_classify_source() {
        assert_eq!(ToolDiscovery::classify_source("read_note"), "builtin");
        assert_eq!(ToolDiscovery::classify_source("rune_my_tool"), "rune");
        assert_eq!(ToolDiscovery::classify_source("just_build"), "just");
        assert_eq!(
            ToolDiscovery::classify_source("gh_search_repos"),
            "upstream"
        );
    }

    #[test]
    fn test_discover_tools_no_filter() {
        let tools = vec![
            make_tool("read_note", "Read a note"),
            make_tool("rune_search", "Custom search"),
            make_tool("just_build", "Build project"),
        ];

        let discovery = ToolDiscovery::new(tools);
        let result = discovery
            .discover_tools(&DiscoverToolsParams::default())
            .unwrap();
        assert!(!result.is_error.unwrap_or(false));
    }

    #[test]
    fn test_discover_tools_with_query() {
        let tools = vec![
            make_tool("read_note", "Read a note"),
            make_tool("create_note", "Create a note"),
            make_tool("semantic_search", "Search semantically"),
        ];

        let discovery = ToolDiscovery::new(tools);
        let result = discovery
            .discover_tools(&DiscoverToolsParams {
                query: Some("note".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert!(!result.is_error.unwrap_or(false));
    }

    #[test]
    fn test_get_tool_schema_found() {
        let tools = vec![make_tool("read_note", "Read a note")];

        let discovery = ToolDiscovery::new(tools);
        let result = discovery
            .get_tool_schema(&GetToolSchemaParams {
                name: "read_note".to_string(),
            })
            .unwrap();
        assert!(!result.is_error.unwrap_or(false));
    }

    #[test]
    fn test_get_tool_schema_not_found() {
        let tools = vec![make_tool("read_note", "Read a note")];

        let discovery = ToolDiscovery::new(tools);
        let result = discovery.get_tool_schema(&GetToolSchemaParams {
            name: "nonexistent".to_string(),
        });
        assert!(result.is_err());
    }
}
