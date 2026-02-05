//! MCP Tool integration for Rig agents
//!
//! This module provides utilities for attaching Crucible MCP tools to Rig agents
//! via the rmcp protocol. Tools are discovered from the MCP server and attached
//! to the agent builder.

use thiserror::Error;

#[cfg(feature = "rmcp-full")]
use rig::agent::AgentBuilder;

/// Errors from MCP tool attachment operations
///
/// This is distinct from `crucible_core::traits::tools::ToolError` which defines
/// abstract errors for the tool executor traits.
#[derive(Debug, Error)]
pub enum McpToolError {
    /// MCP protocol error
    #[error("MCP protocol error: {0}")]
    Protocol(String),

    /// Tool discovery failed
    #[error("Failed to discover tools: {0}")]
    Discovery(String),

    /// Transport error
    #[error("Transport error: {0}")]
    Transport(String),
}

/// Result type for MCP tool operations
pub type McpToolResult<T> = Result<T, McpToolError>;

/// Attach MCP tools from a crucible-tools server to a Rig agent builder
///
/// This function connects to an MCP server via rmcp, discovers available tools,
/// and attaches them to the agent builder.
///
/// # Arguments
///
/// * `builder` - The Rig agent builder to attach tools to
/// * `tools` - Vector of MCP tools from the server
/// * `server_sink` - The rmcp ServerSink for executing tool calls
///
/// # Returns
///
/// The agent builder with tools attached (returns `AgentBuilderSimple`)
///
/// # Example
///
/// ```rust,ignore
/// use crucible_rig::tools::attach_mcp_tools;
/// use rig::providers::openai;
///
/// let client = openai::Client::from_env();
/// let builder = client.agent(openai::GPT_4O)
///     .preamble("You are a helpful assistant with knowledge management tools.");
///
/// // Get tools from MCP server
/// let (tools, server) = discover_crucible_tools().await?;
///
/// // Attach to builder
/// let agent = attach_mcp_tools(builder, tools, server).build();
/// ```
#[cfg(feature = "rmcp-full")]
pub fn attach_mcp_tools<M>(
    builder: AgentBuilder<M>,
    tools: Vec<rmcp::model::Tool>,
    server_sink: rmcp::service::ServerSink,
) -> rig::agent::AgentBuilderSimple<M>
where
    M: rig::completion::CompletionModel,
{
    // Use Rig's built-in rmcp_tools method
    builder.rmcp_tools(tools, server_sink)
}

/// Connect to a Crucible MCP server and get available tools
///
/// This function starts a Crucible MCP server as a child process and
/// discovers its tools via stdio transport.
///
/// # Arguments
///
/// * `kiln_path` - Path to the Crucible kiln
///
/// # Returns
///
/// A tuple of (tools, server_sink) ready for attachment to an agent
///
/// # Errors
///
/// Returns `McpToolError` if:
/// - Cannot spawn the MCP server process
/// - Cannot connect via stdio transport
/// - Tool discovery fails
///
/// # Example
///
/// ```rust,ignore
/// use crucible_rig::tools::discover_crucible_tools;
///
/// let (tools, server) = discover_crucible_tools("/path/to/kiln").await?;
/// println!("Discovered {} tools", tools.len());
/// ```
#[cfg(feature = "rmcp-full")]
pub async fn discover_crucible_tools(
    _kiln_path: &str,
) -> McpToolResult<(Vec<rmcp::model::Tool>, rmcp::service::ServerSink)> {
    // TODO: This will require spawning the crucible-tools MCP server
    // and connecting via stdio. For now, return an error.
    Err(McpToolError::Discovery(
        "Not implemented: requires crucible-tools binary integration".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires MCP server infrastructure"]
    async fn test_tool_list_from_mcp_server() {
        // This test demonstrates how tools would be attached to an agent
        // In practice, this requires:
        // 1. A running MCP server with Crucible tools
        // 2. An rmcp client connection
        // 3. Tool list discovery
        // 4. Attachment to Rig agent builder

        // Example flow (commented out until infrastructure is ready):
        // let server = start_test_mcp_server().await;
        // let tools = discover_tools(&server).await.unwrap();
        // assert!(!tools.is_empty());
        // assert!(tools.iter().any(|t| t.name.contains("semantic_search")));
    }

    #[test]
    fn test_error_types() {
        // Verify error types are properly constructed
        let err = McpToolError::Protocol("test".to_string());
        assert!(err.to_string().contains("MCP protocol error"));

        let err = McpToolError::Discovery("not found".to_string());
        assert!(err.to_string().contains("Failed to discover tools"));

        let err = McpToolError::Transport("connection refused".to_string());
        assert!(err.to_string().contains("Transport error"));
    }

    #[tokio::test]
    async fn test_discover_crucible_tools_not_implemented() {
        // Verify that discover_crucible_tools returns appropriate error
        #[cfg(feature = "rmcp-full")]
        {
            let result = discover_crucible_tools("/tmp/test-kiln").await;
            assert!(result.is_err());
            let err = result.unwrap_err();
            assert!(matches!(err, McpToolError::Discovery(_)));
            assert!(err.to_string().contains("Not implemented"));
        }
    }

    #[test]
    #[cfg(feature = "rmcp-full")]
    fn test_rmcp_feature_enabled() {
        // This test verifies that when the rmcp-full feature is enabled,
        // the attach_mcp_tools function is available.
        // We can't easily test the function signature directly since it's generic.
        // The fact that this compiles is enough.
    }
}
