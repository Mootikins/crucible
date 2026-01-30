//! MCP Proxy Tool — wraps upstream MCP gateway tools as Rig `Tool` instances.
//!
//! This module provides [`McpProxyTool`], which implements [`rig::tool::Tool`] by proxying
//! tool calls through the [`McpGatewayManager`]. This enables dynamically-discovered upstream
//! MCP tools to be used by Rig agents without requiring direct `rmcp` dependencies.
//!
//! # Example
//!
//! ```rust,ignore
//! use crucible_rig::mcp_proxy_tool::{McpProxyTool, mcp_tools_from_gateway};
//! use std::sync::Arc;
//! use tokio::sync::RwLock;
//!
//! let gateway = Arc::new(RwLock::new(McpGatewayManager::new()));
//! let tools = {
//!     let gw = gateway.read().await;
//!     let all = gw.all_tools();
//!     mcp_tools_from_gateway(&gateway, &["github".into()], &all)
//! };
//! ```

use std::sync::Arc;

use crucible_core::traits::mcp::McpToolInfo;
use crucible_tools::mcp_gateway::McpGatewayManager;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use tokio::sync::RwLock;

/// Error type for MCP proxy tool operations.
#[derive(Debug, thiserror::Error)]
pub enum McpProxyError {
    /// Error communicating with the MCP gateway.
    #[error("MCP gateway error: {0}")]
    GatewayError(String),
    /// The upstream tool returned an error result.
    #[error("MCP tool error: {0}")]
    ToolError(String),
}

/// A Rig [`Tool`] that proxies calls to an upstream MCP tool via the gateway.
///
/// Each instance wraps a single tool discovered from an upstream MCP server.
/// The tool's name, description, and input schema come from [`McpToolInfo`],
/// and calls are forwarded through [`McpGatewayManager::call_tool`].
pub struct McpProxyTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
    gateway: Arc<RwLock<McpGatewayManager>>,
}

impl McpProxyTool {
    /// Create a new proxy tool from MCP tool info and a shared gateway reference.
    pub fn new(info: &McpToolInfo, gateway: Arc<RwLock<McpGatewayManager>>) -> Self {
        Self {
            name: info.prefixed_name.clone(),
            description: info.description.clone().unwrap_or_default(),
            input_schema: info.input_schema.clone(),
            gateway,
        }
    }
}

impl Tool for McpProxyTool {
    const NAME: &'static str = "__mcp_proxy";

    type Error = McpProxyError;
    type Args = serde_json::Value;
    type Output = String;

    fn name(&self) -> String {
        self.name.clone()
    }

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: self.input_schema.clone(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let gateway = self.gateway.read().await;
        let result = gateway
            .call_tool(&self.name, args)
            .await
            .map_err(|e| McpProxyError::GatewayError(e.to_string()))?;

        if result.is_error {
            return Err(McpProxyError::ToolError(
                result
                    .first_text()
                    .unwrap_or("Unknown error")
                    .to_string(),
            ));
        }

        Ok(result.first_text().unwrap_or("").to_string())
    }
}

/// Create [`McpProxyTool`] instances for tools from specific upstream servers.
///
/// Filters `all_tools` to only those whose `upstream` field matches one of the
/// given `server_names`, then wraps each as a proxy tool.
///
/// The caller must pre-fetch `all_tools` (e.g. via `gateway.read().await.all_tools()`)
/// so this function does not need to acquire the gateway lock itself.
pub fn mcp_tools_from_gateway(
    gateway: &Arc<RwLock<McpGatewayManager>>,
    server_names: &[String],
    all_tools: &[McpToolInfo],
) -> Vec<McpProxyTool> {
    all_tools
        .iter()
        .filter(|tool| server_names.contains(&tool.upstream))
        .map(|tool| McpProxyTool::new(tool, Arc::clone(gateway)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_gateway() -> Arc<RwLock<McpGatewayManager>> {
        Arc::new(RwLock::new(McpGatewayManager::new()))
    }

    fn make_tool_info(name: &str, upstream: &str) -> McpToolInfo {
        McpToolInfo {
            name: name.to_string(),
            prefixed_name: format!("{upstream}_{name}"),
            description: Some(format!("Description for {name}")),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
            upstream: upstream.to_string(),
        }
    }

    #[test]
    fn mcp_proxy_tool_name_returns_prefixed_name() {
        let gateway = make_gateway();
        let info = make_tool_info("search_repos", "gh");
        let tool = McpProxyTool::new(&info, gateway);

        // name() should return the dynamic prefixed name, not "__mcp_proxy"
        assert_eq!(tool.name(), "gh_search_repos");
        assert_ne!(tool.name(), McpProxyTool::NAME);
    }

    #[tokio::test]
    async fn mcp_proxy_tool_definition_matches_info() {
        let gateway = make_gateway();
        let info = make_tool_info("search_repos", "gh");
        let tool = McpProxyTool::new(&info, gateway);

        let def = tool.definition(String::new()).await;

        assert_eq!(def.name, "gh_search_repos");
        assert_eq!(def.description, "Description for search_repos");
        assert_eq!(
            def.parameters,
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            })
        );
    }

    #[test]
    fn mcp_proxy_tool_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<McpProxyTool>();
    }

    #[test]
    fn mcp_tools_from_gateway_filters_by_server_name() {
        let gateway = make_gateway();
        let tools = vec![
            make_tool_info("search_repos", "gh"),
            make_tool_info("create_issue", "gh"),
            make_tool_info("web_search", "brave"),
            make_tool_info("read_file", "fs"),
        ];

        // Filter to only "gh" tools
        let gh_tools =
            mcp_tools_from_gateway(&gateway, &["gh".to_string()], &tools);
        assert_eq!(gh_tools.len(), 2);
        assert_eq!(gh_tools[0].name(), "gh_search_repos");
        assert_eq!(gh_tools[1].name(), "gh_create_issue");

        // Filter to multiple servers
        let multi = mcp_tools_from_gateway(
            &gateway,
            &["gh".to_string(), "brave".to_string()],
            &tools,
        );
        assert_eq!(multi.len(), 3);

        // Filter to nonexistent server
        let empty =
            mcp_tools_from_gateway(&gateway, &["nonexistent".to_string()], &tools);
        assert!(empty.is_empty());
    }

    #[test]
    fn mcp_proxy_tool_default_description_when_none() {
        let gateway = make_gateway();
        let info = McpToolInfo {
            name: "test".to_string(),
            prefixed_name: "srv_test".to_string(),
            description: None,
            input_schema: json!({}),
            upstream: "srv".to_string(),
        };
        let tool = McpProxyTool::new(&info, gateway);
        assert_eq!(tool.description, "");
    }
}
