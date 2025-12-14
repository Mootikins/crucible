//! MCP (Model Context Protocol) abstractions
//!
//! Core types and traits for MCP client/server interactions.
//! Implementations (rmcp, mock, etc.) depend on these abstractions.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────┐
//! │  crucible-core  │  ← Defines traits and types
//! │   traits/mcp    │
//! └────────┬────────┘
//!          │
//!    ┌─────┴─────┐
//!    ▼           ▼
//! ┌──────┐   ┌──────────┐
//! │ rmcp │   │ mock/test│  ← Implementations
//! └──────┘   └──────────┘
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// =============================================================================
// MCP Content Types
// =============================================================================

/// Content block types (matching MCP specification)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ContentBlock {
    /// Text content
    Text { text: String },
    /// Image content (base64 encoded)
    Image { data: String, mime_type: String },
    /// Resource reference
    Resource { uri: String, text: Option<String> },
}

/// Result of a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    /// Content blocks returned by the tool
    pub content: Vec<ContentBlock>,
    /// Whether the tool execution resulted in an error
    pub is_error: bool,
}

impl Default for ToolCallResult {
    fn default() -> Self {
        Self {
            content: Vec::new(),
            is_error: false,
        }
    }
}

impl ToolCallResult {
    /// Create a successful text result
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::Text { text: text.into() }],
            is_error: false,
        }
    }

    /// Create an error result
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::Text {
                text: message.into(),
            }],
            is_error: true,
        }
    }

    /// Get first text content
    pub fn first_text(&self) -> Option<&str> {
        self.content.iter().find_map(|c| match c {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
    }
}

// =============================================================================
// MCP Tool Types
// =============================================================================

/// Information about a discovered tool from upstream MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    /// Original tool name from upstream
    pub name: String,
    /// Prefixed name for namespacing (e.g., "gh_search_repos")
    pub prefixed_name: String,
    /// Tool description
    pub description: Option<String>,
    /// JSON schema for tool parameters
    pub input_schema: JsonValue,
    /// Source upstream/server name
    pub upstream: String,
}

/// Server information from upstream MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    /// Server name
    pub name: String,
    /// Server version
    pub version: Option<String>,
    /// Protocol version
    pub protocol_version: String,
    /// Server capabilities
    pub capabilities: JsonValue,
}

// =============================================================================
// MCP Configuration Types
// =============================================================================

/// Transport configuration for connecting to upstream MCP servers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpTransportConfig {
    /// Stdio transport (spawn subprocess)
    Stdio {
        /// Command to execute
        command: String,
        /// Command arguments
        #[serde(default)]
        args: Vec<String>,
        /// Environment variables to set
        #[serde(default)]
        env: Vec<(String, String)>,
    },

    /// SSE transport (HTTP+Server-Sent Events)
    Sse {
        /// URL to connect to
        url: String,
        /// Optional authorization header
        #[serde(default)]
        auth_header: Option<String>,
    },
}

/// Configuration for an upstream MCP server connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpClientConfig {
    /// Unique name for this upstream (e.g., "github", "filesystem")
    pub name: String,

    /// Transport configuration
    pub transport: McpTransportConfig,

    /// Prefix to add to tool names (e.g., "gh_" -> "gh_search_repositories")
    #[serde(default)]
    pub prefix: Option<String>,

    /// Whitelist of allowed tools (glob patterns supported)
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,

    /// Blacklist of blocked tools (glob patterns supported)
    #[serde(default)]
    pub blocked_tools: Option<Vec<String>>,

    /// Whether to auto-reconnect on disconnection
    #[serde(default = "default_auto_reconnect")]
    pub auto_reconnect: bool,
}

fn default_auto_reconnect() -> bool {
    true
}

// =============================================================================
// MCP Traits - Interface Segregation
// =============================================================================

/// Trait for discovering tools from an MCP server
///
/// This trait is dyn-compatible for use with trait objects.
#[async_trait]
pub trait McpToolDiscovery: Send + Sync {
    /// List all available tools
    async fn list_tools(&self) -> Result<Vec<McpToolInfo>, McpError>;

    /// Get a specific tool by name
    async fn get_tool(&self, name: &str) -> Result<Option<McpToolInfo>, McpError>;
}

/// Trait for executing tools on an MCP server
///
/// This trait is dyn-compatible for use with trait objects.
#[async_trait]
pub trait McpToolExecutor: Send + Sync {
    /// Call a tool with JSON arguments
    async fn call_tool(
        &self,
        tool_name: &str,
        arguments: JsonValue,
    ) -> Result<ToolCallResult, McpError>;
}

/// Trait for managing MCP connection state
///
/// This trait is dyn-compatible for use with trait objects.
#[async_trait]
pub trait McpConnection: Send + Sync {
    /// Connect to the MCP server
    async fn connect(&mut self) -> Result<(), McpError>;

    /// Disconnect from the MCP server
    async fn disconnect(&mut self) -> Result<(), McpError>;

    /// Check if connected
    fn is_connected(&self) -> bool;

    /// Get server info (if connected)
    fn server_info(&self) -> Option<&McpServerInfo>;
}

/// Combined trait for a full MCP client
///
/// This is a convenience trait that combines all MCP client capabilities.
/// Prefer using the individual traits when only specific functionality is needed.
pub trait McpClient: McpToolDiscovery + McpToolExecutor + McpConnection {}

// Blanket implementation
impl<T> McpClient for T where T: McpToolDiscovery + McpToolExecutor + McpConnection {}

// =============================================================================
// MCP Errors
// =============================================================================

/// Errors that can occur in MCP operations
#[derive(Debug, Clone)]
pub enum McpError {
    /// Failed to connect to server
    Connection(String),
    /// Transport error during communication
    Transport(String),
    /// Tool not found
    ToolNotFound(String),
    /// Tool execution failed
    Execution(String),
    /// Server returned an error
    ServerError(String),
    /// Invalid configuration
    Config(String),
    /// Not connected
    NotConnected,
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpError::Connection(msg) => write!(f, "Connection error: {}", msg),
            McpError::Transport(msg) => write!(f, "Transport error: {}", msg),
            McpError::ToolNotFound(name) => write!(f, "Tool not found: {}", name),
            McpError::Execution(msg) => write!(f, "Execution error: {}", msg),
            McpError::ServerError(msg) => write!(f, "Server error: {}", msg),
            McpError::Config(msg) => write!(f, "Config error: {}", msg),
            McpError::NotConnected => write!(f, "Not connected to MCP server"),
        }
    }
}

impl std::error::Error for McpError {}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_call_result_text() {
        let result = ToolCallResult::text("hello");
        assert!(!result.is_error);
        assert_eq!(result.first_text(), Some("hello"));
    }

    #[test]
    fn test_tool_call_result_error() {
        let result = ToolCallResult::error("failed");
        assert!(result.is_error);
        assert_eq!(result.first_text(), Some("failed"));
    }

    #[test]
    fn test_content_block_serialization() {
        let block = ContentBlock::Text {
            text: "test".to_string(),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("\"type\":\"text\""));
    }

    #[test]
    fn test_transport_config_stdio() {
        let config = McpTransportConfig::Stdio {
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "server".to_string()],
            env: vec![],
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"type\":\"stdio\""));
    }

    #[test]
    fn test_mcp_error_display() {
        let err = McpError::ToolNotFound("my_tool".to_string());
        assert_eq!(format!("{}", err), "Tool not found: my_tool");
    }
}
