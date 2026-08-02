//! The `ToolExecutor` for user-configured MCP gateway servers.
//!
//! Lives next to [`crate::tools::mcp_gateway`] rather than in `tool_dispatch`
//! because it is a provider like any other, and `tool_dispatch.rs` is against
//! the module size budget.

use async_trait::async_trait;
use crucible_core::traits::tools::{
    ExecutionContext, ToolDefinition, ToolError, ToolExecutor, ToolResult, ToolSurface,
};
use std::collections::HashSet;
use std::sync::Arc;

/// Dispatches gateway (user MCP) tools through the shared `McpGatewayManager`,
/// scoped to the session agent's configured upstream servers. Registering this
/// as a dispatcher provider makes deferred gateway tools reachable via the
/// progressive-disclosure bridge (`discover_tools` → `invoke_tool`).
pub struct GatewayToolExecutor {
    gateway: Arc<tokio::sync::RwLock<crate::tools::mcp_gateway::McpGatewayManager>>,
    allowed_servers: HashSet<String>,
}

impl GatewayToolExecutor {
    pub fn new(
        gateway: Arc<tokio::sync::RwLock<crate::tools::mcp_gateway::McpGatewayManager>>,
        allowed_servers: Vec<String>,
    ) -> Self {
        Self {
            gateway,
            allowed_servers: allowed_servers.into_iter().collect(),
        }
    }
}

#[async_trait]
impl ToolExecutor for GatewayToolExecutor {
    async fn execute_tool(
        &self,
        name: &str,
        params: serde_json::Value,
        _context: &ExecutionContext,
    ) -> ToolResult<serde_json::Value> {
        let gateway = self.gateway.read().await;
        // Only dispatch tools belonging to the agent's configured servers;
        // anything else falls through the provider chain as NotFound.
        match gateway.find_upstream(name) {
            Some(upstream) if self.allowed_servers.contains(upstream) => {}
            _ => return Err(ToolError::NotFound(name.to_string())),
        }
        match gateway.call_tool(name, params).await {
            Ok(result) => {
                let text = result
                    .content
                    .iter()
                    .filter_map(|c| c.as_text().map(str::to_string))
                    .collect::<Vec<_>>()
                    .join("\n");
                if result.is_error {
                    Err(ToolError::ExecutionFailed(text))
                } else {
                    Ok(serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)))
                }
            }
            Err(err) => Err(ToolError::ExecutionFailed(err.to_string())),
        }
    }

    async fn list_tools(&self) -> ToolResult<Vec<ToolDefinition>> {
        let gateway = self.gateway.read().await;
        Ok(gateway
            .all_tools()
            .into_iter()
            .filter(|t| self.allowed_servers.contains(&t.upstream))
            .map(|t| ToolDefinition {
                name: t.prefixed_name,
                description: t.description.unwrap_or_default(),
                category: Some("mcp".to_string()),
                parameters: Some(t.input_schema),
                returns: None,
                examples: vec![],
                required_permissions: vec![],
            })
            .collect())
    }

    /// `Unknown`, not `Daemon`, deliberately. Gateway tools run in the daemon
    /// process but are third-party code reached over a pipe: a filesystem MCP
    /// server is host-touching in every way that matters, and the daemon has
    /// no way to tell one from a calculator.
    fn surface(&self) -> ToolSurface {
        ToolSurface::Unknown
    }
}
