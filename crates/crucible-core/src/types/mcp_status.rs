//! MCP server status entry emitted by the daemon after reading MCP config.
//!
//! Surfaced via the `mcp_servers_ready` session setup event so the TUI MCP
//! panel can render configured MCP servers with their connection status and
//! the tools they expose.
//!
//! NOTE: distinct from [`crate::traits::mcp::McpServerInfo`], which describes
//! MCP protocol-level identity (name/version/protocol_version/capabilities)
//! returned by an upstream server's `initialize` response. This struct is for
//! display and event-stream consumption, not protocol handshakes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub prefix: String,
    pub tools: Vec<String>,
    pub connected: bool,
}
