//! Crucible Tools - MCP-compatible tools for knowledge management
//!
//! This module provides focused tools for the Crucible knowledge management system,
//! designed following SOLID principles and MCP (Model Context Protocol) compatibility.
//!
//! ## Tool Categories
//!
//! - **`NoteTools`** (6): `create_note`, `read_note`, `read_metadata`, `update_note`, `delete_note`, `list_notes`
//! - **`SearchTools`** (3): `semantic_search`, `grep_notes`, `property_search`
//! - **`KilnTools`** (1): `get_kiln_info`
//! - **`CrucibleMcpServer`** (12): Unified MCP server exposing all tools via stdio transport

#![allow(missing_docs)]

pub mod active_tools;
pub mod autolink;
pub mod containment;
pub mod diff_synth;
pub mod error_utils;
pub mod extended_mcp_server;
pub(crate) mod fs_scope;
pub mod gateway_executor;
pub mod grep_engine;
pub mod helpers;
pub mod kiln;
pub mod mcp_client;
pub mod mcp_gateway;
pub mod mcp_server;
pub mod notes;
pub(crate) mod path_resolution;
pub(crate) mod protected;
pub mod search;
pub mod surface;
pub mod tool_discovery;
pub mod tool_modes;
pub mod toon_response;
pub mod utils;
pub mod workspace;

/// One [`ToolDefinition`] shape for every executor that lists tools.
pub(crate) fn tool_definition(
    name: String,
    description: Option<String>,
    parameters: serde_json::Value,
    category: &str,
) -> crucible_core::traits::tools::ToolDefinition {
    crucible_core::traits::tools::ToolDefinition {
        name,
        description: description.unwrap_or_default(),
        category: Some(category.to_string()),
        parameters: Some(parameters),
        returns: None,
        required_permissions: vec![],
        examples: vec![],
    }
}

/// The [`ToolDefinition`] for a tool an rmcp server advertises.
pub(crate) fn tool_definition_from_rmcp(
    tool: rmcp::model::Tool,
    category: &str,
) -> crucible_core::traits::tools::ToolDefinition {
    tool_definition(
        tool.name.to_string(),
        tool.description.map(|d| d.to_string()),
        serde_json::Value::Object((*tool.input_schema).clone()),
        category,
    )
}
pub(crate) mod workspace_defs;

// ===== PUBLIC API EXPORTS =====

pub use error_utils::strip_tool_error_prefix;
pub use extended_mcp_server::{ExtendedMcpServer, ExtendedMcpService};
pub use kiln::KilnTools;
pub use mcp_client::{create_stdio_executor, create_stdio_executor_with_env, RmcpExecutor};
pub use mcp_gateway::{
    GatewayError, GatewayResult, McpGatewayManager, ReconnectSchedule, UpstreamClient,
};
pub use mcp_server::{CrucibleMcpServer, DelegationContext};
pub use notes::NoteTools;
pub use search::SearchTools;
pub use tool_discovery::{
    DiscoverToolsParams, GetToolSchemaParams, ToolDiscovery, ToolInfo, ToolSchema, ToolSourceFilter,
};
pub use workspace::WorkspaceTools;
