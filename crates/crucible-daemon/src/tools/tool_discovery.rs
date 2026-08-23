//! Tool discovery for agents to search available tools at runtime
//!
//! This module provides tools for agents to discover and inspect available tools
//! at runtime, enabling proactive tool search ("what tools can help with X?") and
//! progressive disclosure (don't dump all tools in context).

use super::helpers::text_success;
use rmcp::model::{CallToolResult, Tool};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Where a tool comes from, as `discover_tools` classifies it by name.
///
/// The `source` filter in `DiscoverToolsParams`, the `source` field the
/// discovery results report, and the `enum` the advertised schema lists all
/// project this one type. The schema used to carry a hand list and the
/// handler accepted any string, so a typo matched nothing and said so to
/// nobody.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(strum::EnumIter))]
#[serde(rename_all = "lowercase")]
pub enum ToolSourceFilter {
    /// A tool the daemon or one of its executors defines.
    Builtin,
    /// A `just_*` recipe.
    Just,
    /// A tool an upstream MCP server serves.
    Upstream,
}

impl ToolSourceFilter {
    /// Every variant, in declaration order. `all_is_complete` proves it
    /// against `EnumIter`.
    pub const ALL: [Self; 3] = [Self::Builtin, Self::Just, Self::Upstream];

    /// The name serde reads and writes for this variant.
    #[must_use]
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Just => "just",
            Self::Upstream => "upstream",
        }
    }

    /// The wire names, in declaration order, for the advertised schema.
    #[must_use]
    pub fn wire_names() -> Vec<&'static str> {
        Self::ALL.iter().copied().map(Self::wire_name).collect()
    }
}

/// Parameters for discovering available tools
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverToolsParams {
    /// Optional search query to filter tools by name or description
    #[serde(default)]
    pub query: Option<String>,
    /// Optional source filter.
    #[serde(default)]
    pub source: Option<ToolSourceFilter>,
    /// Maximum number of results to return (default: 50)
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

/// The same defaults serde applies to `{}`. The derived `Default` gave
/// `limit: 0`, so `discover_tools` with no arguments returned nothing.
impl Default for DiscoverToolsParams {
    fn default() -> Self {
        Self {
            query: None,
            source: None,
            limit: default_limit(),
        }
    }
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
    /// Source classification.
    pub source: ToolSourceFilter,
}

/// Full schema information for a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    /// The tool's unique name
    pub name: String,
    /// Human-readable description of what the tool does
    pub description: String,
    /// Source classification.
    pub source: ToolSourceFilter,
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

    fn classify_source(name: &str) -> ToolSourceFilter {
        if name.starts_with("just_") {
            ToolSourceFilter::Just
        } else if name.contains("::") || name.starts_with("gh_") || name.starts_with("mcp_") {
            ToolSourceFilter::Upstream
        } else {
            ToolSourceFilter::Builtin
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
        let source_filter = params.source;

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

                let matches_source = source_filter.is_none_or(|wanted| wanted == source);

                matches_query && matches_source
            })
            .take(params.limit)
            .map(|t| ToolInfo {
                name: t.name.to_string(),
                description: t.description.as_deref().unwrap_or("").to_string(),
                source: Self::classify_source(t.name.as_ref()),
            })
            .collect();

        matches.sort_by(|a, b| a.name.cmp(&b.name));

        let output = json!({
            "count": matches.len(),
            "tools": matches
        });

        Ok(text_success(
            serde_json::to_string_pretty(&output).unwrap_or_else(|_| output.to_string()),
        ))
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
                    source: Self::classify_source(t.name.as_ref()),
                    input_schema: serde_json::Value::Object((*t.input_schema).clone()),
                };

                Ok(text_success(
                    serde_json::to_string_pretty(&schema)
                        .unwrap_or_else(|_| json!(schema).to_string()),
                ))
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
    use std::sync::Arc;

    fn make_tool(name: &str, desc: &str) -> Tool {
        let mut schema = Map::new();
        schema.insert("type".to_string(), json!("object"));
        schema.insert("properties".to_string(), json!({}));

        Tool::new(name.to_string(), desc.to_string(), Arc::new(schema))
    }

    #[test]
    fn test_classify_source() {
        assert_eq!(
            ToolDiscovery::classify_source("read_note"),
            ToolSourceFilter::Builtin
        );
        assert_eq!(
            ToolDiscovery::classify_source("just_build"),
            ToolSourceFilter::Just
        );
        assert_eq!(
            ToolDiscovery::classify_source("gh_search_repos"),
            ToolSourceFilter::Upstream
        );
    }

    #[test]
    fn all_is_complete() {
        use strum::IntoEnumIterator;
        let every: Vec<ToolSourceFilter> = ToolSourceFilter::iter().collect();
        assert_eq!(every, ToolSourceFilter::ALL.to_vec());
    }

    /// The wire names are what the schema advertises and what the results
    /// report. Change them only with the intent to change what the model sees.
    #[test]
    fn source_filter_keeps_its_wire_names() {
        assert_eq!(
            ToolSourceFilter::wire_names(),
            vec!["builtin", "just", "upstream"]
        );
        for variant in ToolSourceFilter::ALL {
            let name = variant.wire_name();
            assert_eq!(serde_json::to_value(variant).unwrap(), json!(name));
            let parsed: ToolSourceFilter = serde_json::from_value(json!(name)).unwrap();
            assert_eq!(parsed, variant);
        }
    }

    /// A `source` the schema does not list is an error, not a silent empty
    /// result.
    #[test]
    fn discover_tools_rejects_a_source_outside_the_schema() {
        let err = serde_json::from_value::<DiscoverToolsParams>(json!({ "source": "plugin" }))
            .unwrap_err()
            .to_string();
        assert!(err.contains("plugin"), "{err}");
    }

    /// `Default` and `{}` are the same request: the schema's `limit: 50`.
    #[test]
    fn default_params_match_the_schema_defaults() {
        let from_json: DiscoverToolsParams = serde_json::from_value(json!({})).unwrap();
        assert_eq!(DiscoverToolsParams::default().limit, from_json.limit);
        assert_eq!(from_json.limit, 50);
    }

    #[test]
    fn discover_tools_filters_by_source() {
        let tools = vec![
            make_tool("read_note", "Read a note"),
            make_tool("just_build", "Build project"),
            make_tool("gh_search_repos", "Search repos"),
        ];

        let discovery = ToolDiscovery::new(tools);
        let result = discovery
            .discover_tools(&DiscoverToolsParams {
                source: Some(ToolSourceFilter::Just),
                ..Default::default()
            })
            .unwrap();
        let text = result.content[0].as_text().expect("text content");
        let output: serde_json::Value = serde_json::from_str(&text.text).unwrap();
        assert_eq!(
            output,
            json!({
                "count": 1,
                "tools": [{
                    "name": "just_build",
                    "description": "Build project",
                    "source": "just"
                }]
            })
        );
    }

    #[test]
    fn test_discover_tools_no_filter() {
        let tools = vec![
            make_tool("read_note", "Read a note"),
            make_tool("just_build", "Build project"),
        ];

        let discovery = ToolDiscovery::new(tools);
        let result = discovery
            .discover_tools(&DiscoverToolsParams::default())
            .unwrap();
        assert!(!result
            .is_error
            .expect("is_error field should be present in tool result"));
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
        assert!(!result
            .is_error
            .expect("is_error field should be present in tool result"));
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
        assert!(!result
            .is_error
            .expect("is_error field should be present in tool result"));
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
