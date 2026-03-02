//! Crucible Tools - MCP-compatible tools for knowledge management
//!
//! This module provides focused tools for the Crucible knowledge management system,
//! designed following SOLID principles and MCP (Model Context Protocol) compatibility.
//!
//! ## Tool Categories
//!
//! - **`NoteTools`** (6): `create_note`, `read_note`, `read_metadata`, `update_note`, `delete_note`, `list_notes`
//! - **`SearchTools`** (3): `semantic_search`, `text_search`, `property_search`
//! - **`KilnTools`** (1): `get_kiln_info`
//! - **`CrucibleMcpServer`** (12): Unified MCP server exposing all tools via stdio transport

#![allow(missing_docs)]

pub mod error_utils;
pub mod extended_mcp_server;
pub mod helpers;
pub mod kiln;
pub mod mcp_client;
pub mod mcp_gateway;
pub mod mcp_server;
pub mod notes;
pub mod output_filter;
pub mod search;
pub mod tool_discovery;
pub mod tool_modes;
pub mod toon_response;
pub mod utils;
pub mod workspace;

// ===== PUBLIC API EXPORTS =====

pub use error_utils::strip_tool_error_prefix;
pub use extended_mcp_server::{ExtendedMcpServer, ExtendedMcpService};
pub use kiln::KilnTools;
pub use mcp_client::{create_stdio_executor, create_stdio_executor_with_env, RmcpExecutor};
pub use mcp_gateway::{GatewayError, GatewayResult, McpGatewayManager, UpstreamClient};
pub use mcp_server::{CrucibleMcpServer, DelegationContext};
pub use notes::NoteTools;
pub use search::SearchTools;
pub use tool_discovery::{
    DiscoverToolsParams, GetToolSchemaParams, ToolDiscovery, ToolInfo, ToolSchema,
};
pub use workspace::WorkspaceTools;
