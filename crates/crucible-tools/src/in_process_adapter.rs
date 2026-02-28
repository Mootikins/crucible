use std::collections::HashSet;
use std::sync::Arc;

use rmcp::model::{CallToolResult, Tool as McpTool};

use crate::mcp_server::CrucibleMcpServer;
use crate::tool_modes::PLAN_TOOL_NAMES;
use crucible_core::error_utils::strip_tool_error_prefix;

#[derive(Clone)]
/// Adapter for running MCP tools in-process without stdio transport.
/// Wraps a `CrucibleMcpServer` and exposes its tools as Rig-compatible tools.
pub struct InProcessMcpAdapter {
    server: Arc<CrucibleMcpServer>,
}

impl InProcessMcpAdapter {
    /// Creates a new adapter wrapping the given MCP server.
    #[must_use]
    pub fn new(server: Arc<CrucibleMcpServer>) -> Self {
        Self { server }
    }

    /// Lists all available tool names from the MCP server.
    #[must_use]
    pub fn list_tool_names(&self) -> Vec<String> {
        self.server
            .list_tools()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect()
    }
}

#[allow(dead_code)]
fn filter_plan_tools(all_tools: Vec<McpTool>) -> Vec<McpTool> {
    let plan_names: HashSet<&str> = PLAN_TOOL_NAMES.iter().copied().collect();
    all_tools
        .into_iter()
        .filter(|tool| plan_names.contains(tool.name.as_ref()))
        .collect()
}

#[allow(dead_code)]
fn normalize_tool_error_message(message: &str) -> String {
    let unquoted = serde_json::from_str::<String>(message).unwrap_or_else(|_| message.to_string());
    strip_tool_error_prefix(&unquoted)
}

#[allow(dead_code)]
fn first_text(result: &CallToolResult) -> Option<&str> {
    result
        .content
        .iter()
        .find_map(|content| content.as_text().map(|text| text.text.as_str()))
}

#[allow(dead_code)]
fn into_object(
    value: serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>, rmcp::ErrorData> {
    match value {
        serde_json::Value::Object(map) => Ok(map),
        serde_json::Value::Null => Ok(serde_json::Map::new()),
        _ => Err(rmcp::ErrorData::invalid_params(
            "tool arguments must be a JSON object",
            None,
        )),
    }
}

#[allow(dead_code)]
fn parse_params<T: serde::de::DeserializeOwned>(
    map: serde_json::Map<String, serde_json::Value>,
) -> Result<T, rmcp::ErrorData> {
    serde_json::from_value(serde_json::Value::Object(map))
        .map_err(|err| rmcp::ErrorData::invalid_params(err.to_string(), None))
}
