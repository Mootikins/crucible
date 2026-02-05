//! MCP Client executor implementation using rmcp
//!
//! Provides an implementation of `McpToolExecutor` that connects to upstream
//! MCP servers via stdio or SSE transport.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_tools::mcp_client::{RmcpExecutor, create_stdio_executor};
//!
//! // Create executor for a stdio-based MCP server
//! let executor = create_stdio_executor("uvx", &["mcp-server-git"]).await?;
//!
//! // Call tools directly
//! let result = executor.call_tool("git_status", args).await?;
//! ```

#![allow(
    clippy::missing_errors_doc,
    clippy::default_trait_access,
    clippy::unnecessary_wraps
)]

use async_trait::async_trait;
use crucible_core::traits::mcp::{
    ContentBlock, McpError, McpServerInfo, McpToolExecutor, McpToolInfo, ToolCallResult,
};
use rmcp::model::{
    CallToolRequestParam, Content, InitializeResult, ListToolsResult, RawContent, Tool as RmcpTool,
};
use rmcp::service::{RunningService, ServiceExt};
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::RoleClient;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// MCP client executor backed by rmcp
///
/// This wraps an rmcp client connection and provides `McpToolExecutor` implementation.
pub struct RmcpExecutor {
    /// The underlying rmcp service
    service: Arc<RunningService<RoleClient, ()>>,
    /// Cached server info
    server_info: Option<McpServerInfo>,
    /// Cached tools by name
    tools: Arc<RwLock<HashMap<String, McpToolInfo>>>,
}

impl RmcpExecutor {
    /// Create from an already-initialized rmcp service
    pub async fn from_service(service: RunningService<RoleClient, ()>) -> Result<Self, McpError> {
        let service = Arc::new(service);

        // Get server info
        let init_result = service.peer_info();
        let server_info = init_result.and_then(convert_server_info);

        // Discover tools
        let tools_result = service
            .list_tools(Default::default())
            .await
            .map_err(|e| McpError::Transport(e.to_string()))?;

        let tools = convert_tools_list(tools_result);
        let tools_map: HashMap<_, _> = tools.into_iter().map(|t| (t.name.clone(), t)).collect();

        info!("RmcpExecutor initialized with {} tools", tools_map.len());

        Ok(Self {
            service,
            server_info,
            tools: Arc::new(RwLock::new(tools_map)),
        })
    }

    /// Get server information
    #[must_use]
    pub fn server_info(&self) -> Option<&McpServerInfo> {
        self.server_info.as_ref()
    }

    /// Get discovered tools
    pub async fn tools(&self) -> Vec<McpToolInfo> {
        self.tools.read().await.values().cloned().collect()
    }

    /// Get a tool by name
    pub async fn get_tool(&self, name: &str) -> Option<McpToolInfo> {
        self.tools.read().await.get(name).cloned()
    }

    /// Refresh the tool list from server
    pub async fn refresh_tools(&self) -> Result<(), McpError> {
        let tools_result = self
            .service
            .list_tools(Default::default())
            .await
            .map_err(|e| McpError::Transport(e.to_string()))?;

        let tools = convert_tools_list(tools_result);
        let mut tools_map = self.tools.write().await;
        tools_map.clear();
        for tool in tools {
            tools_map.insert(tool.name.clone(), tool);
        }

        Ok(())
    }
}

#[async_trait]
impl McpToolExecutor for RmcpExecutor {
    async fn call_tool(
        &self,
        tool_name: &str,
        arguments: JsonValue,
    ) -> Result<ToolCallResult, McpError> {
        debug!("Calling MCP tool: {} with args: {:?}", tool_name, arguments);

        // Convert to owned String to satisfy lifetime requirements
        let tool_name_owned = tool_name.to_string();

        let result = self
            .service
            .call_tool(CallToolRequestParam {
                name: tool_name_owned.into(),
                arguments: arguments.as_object().cloned(),
                task: None,
            })
            .await
            .map_err(|e| McpError::Execution(e.to_string()))?;

        let content = result.content.into_iter().map(convert_content).collect();

        Ok(ToolCallResult {
            content,
            is_error: result.is_error.unwrap_or(false),
        })
    }
}

/// Create an executor for a stdio-based MCP server
///
/// Spawns the specified command and connects via stdin/stdout.
pub async fn create_stdio_executor(command: &str, args: &[&str]) -> Result<RmcpExecutor, McpError> {
    create_stdio_executor_with_env(command, args, &[]).await
}

/// Create an executor for a stdio-based MCP server with environment variables
pub async fn create_stdio_executor_with_env(
    command: &str,
    args: &[&str],
    env: &[(&str, &str)],
) -> Result<RmcpExecutor, McpError> {
    info!("Creating stdio executor: {} {:?}", command, args);

    let args_owned: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    let env_owned: Vec<(String, String)> = env
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();

    let transport = TokioChildProcess::new(Command::new(command).configure(move |cmd| {
        for arg in &args_owned {
            cmd.arg(arg);
        }
        for (key, value) in &env_owned {
            cmd.env(key, value);
        }
    }))
    .map_err(|e| McpError::Connection(format!("Failed to spawn process: {e}")))?;

    let client = ().serve(transport).await.map_err(|e| McpError::Connection(e.to_string()))?;

    RmcpExecutor::from_service(client).await
}

// =============================================================================
// Conversion Helpers
// =============================================================================

fn convert_server_info(init: &InitializeResult) -> Option<McpServerInfo> {
    Some(McpServerInfo {
        name: init.server_info.name.clone(),
        version: Some(init.server_info.version.clone()),
        protocol_version: init.protocol_version.to_string(),
        capabilities: serde_json::to_value(&init.capabilities).unwrap_or_default(),
    })
}

#[allow(dead_code)]
fn convert_server_info_opt(init: Option<&InitializeResult>) -> Option<McpServerInfo> {
    init.and_then(convert_server_info)
}

fn convert_tools_list(result: ListToolsResult) -> Vec<McpToolInfo> {
    result
        .tools
        .into_iter()
        .map(|t| convert_tool(t, "upstream"))
        .collect()
}

fn convert_tool(tool: RmcpTool, upstream: &str) -> McpToolInfo {
    McpToolInfo {
        name: tool.name.to_string(),
        prefixed_name: tool.name.to_string(), // No prefix by default
        description: tool.description.map(|d| d.to_string()),
        input_schema: serde_json::to_value(&tool.input_schema).unwrap_or_default(),
        upstream: upstream.to_string(),
    }
}

fn convert_content(content: Content) -> ContentBlock {
    // Content is Annotated<RawContent>, access via .raw field
    match content.raw {
        RawContent::Text(t) => ContentBlock::Text { text: t.text },
        RawContent::Image(i) => ContentBlock::Image {
            data: i.data,
            mime_type: i.mime_type,
        },
        RawContent::Resource(r) => {
            // EmbeddedResource contains ResourceContents
            ContentBlock::Resource {
                uri: String::new(),
                text: Some(embedded_resource_to_text(&r)),
            }
        }
        RawContent::Audio(_) => ContentBlock::Text {
            text: "[Audio content]".to_string(),
        },
        RawContent::ResourceLink(r) => ContentBlock::Resource {
            uri: r.uri,
            text: Some(r.name),
        },
    }
}

fn embedded_resource_to_text(resource: &rmcp::model::RawEmbeddedResource) -> String {
    // ResourceContents is an enum with TextResourceContents or BlobResourceContents
    match &resource.resource {
        rmcp::model::ResourceContents::TextResourceContents { text, .. } => text.clone(),
        rmcp::model::ResourceContents::BlobResourceContents { blob, .. } => {
            format!("[Binary blob: {} bytes]", blob.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::JsonObject;
    use std::sync::Arc;

    #[test]
    fn test_convert_tool() {
        let schema: JsonObject = serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "arg1": {"type": "string"}
            }
        }))
        .unwrap();

        let tool = RmcpTool::new("test_tool", "A test tool", Arc::new(schema));

        let converted = convert_tool(tool, "test_upstream");
        assert_eq!(converted.name, "test_tool");
        assert_eq!(converted.description, Some("A test tool".to_string()));
        assert_eq!(converted.upstream, "test_upstream");
    }
}
