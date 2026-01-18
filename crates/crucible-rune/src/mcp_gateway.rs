//! MCP Gateway Client
//!
//! This module provides client functionality for connecting to upstream MCP servers
//! (e.g., GitHub MCP, filesystem MCP) and routing their tools through the unified
//! event system.
//!
//! ## Architecture
//!
//! ```text
//! ┌───────────────────┐    ┌─────────────────┐    ┌───────────────┐
//! │   Crucible MCP    │    │  MCP Gateway    │    │  Upstream MCP │
//! │      Server       │◄───│    Client       │◄───│    Servers    │
//! │  (ExtendedMcp)    │    │                 │    │ (gh, fs, etc) │
//! └───────────────────┘    └─────────────────┘    └───────────────┘
//!          │                       │
//!          │                       ▼
//!          │               ┌───────────────┐
//!          └──────────────►│   EventBus    │
//!                          │ (tool:before, │
//!                          │  tool:after)  │
//!                          └───────────────┘
//! ```
//!
//! ## Supported Transports
//!
//! - **stdio**: Spawn a subprocess and communicate via stdin/stdout
//! - **SSE**: Connect to an HTTP+SSE endpoint
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::mcp_gateway::{UpstreamMcpClient, UpstreamConfig, TransportConfig};
//!
//! // Connect via stdio (spawn process)
//! let config = UpstreamConfig {
//!     name: "github".to_string(),
//!     transport: TransportConfig::Stdio {
//!         command: "npx".to_string(),
//!         args: vec!["-y".to_string(), "@modelcontextprotocol/server-github".to_string()],
//!         env: vec![("GITHUB_TOKEN".to_string(), token)],
//!     },
//!     prefix: Some("gh_".to_string()),
//!     allowed_tools: None,
//!     blocked_tools: None,
//! };
//!
//! let client = UpstreamMcpClient::connect(config).await?;
//! let tools = client.list_tools().await?;
//!
//! // Call a tool
//! let result = client.call_tool("search_repositories", args).await?;
//! ```

#![allow(deprecated)]

use crate::event_bus::{Event, EventBus, EventContext, HandlerError};
use crate::tool_events::ToolSource;
use crucible_core::traits::mcp::McpToolExecutor;
use crucible_core::utils::glob_match;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// Re-export core MCP types with backwards-compatible aliases
pub use crucible_core::traits::mcp::{
    ContentBlock, McpClientConfig as UpstreamConfig, McpServerInfo as UpstreamServerInfo,
    McpToolInfo as UpstreamTool, McpTransportConfig as TransportConfig, ToolCallResult,
};

/// Errors that can occur in MCP gateway operations
#[derive(Error, Debug)]
pub enum GatewayError {
    /// Failed to connect to upstream server
    #[error("Connection failed: {0}")]
    Connection(String),

    /// Transport error
    #[error("Transport error: {0}")]
    Transport(String),

    /// Tool not found
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    /// Tool execution failed
    #[error("Tool execution failed: {0}")]
    Execution(String),

    /// Server returned an error
    #[error("Server error: {0}")]
    ServerError(String),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    Config(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Boxed executor type for dynamic dispatch
pub type BoxedMcpExecutor = Box<dyn McpToolExecutor + Send + Sync>;

/// MCP Gateway client for connecting to upstream MCP servers
///
/// This client manages the connection to an upstream MCP server and provides
/// methods for discovering and calling tools. All tool calls are routed through
/// the event system for hook processing.
///
/// ## Dependency Injection
///
/// The actual tool execution is delegated to an injected `McpToolExecutor`.
/// This keeps the transport implementation (e.g., rmcp) separate from this crate.
///
/// ```rust,ignore
/// // Create executor externally (e.g., in crucible-tools with rmcp)
/// let executor = create_mcp_executor(config).await?;
///
/// // Inject into client
/// let client = UpstreamMcpClient::new(config)
///     .with_executor(executor);
/// ```
pub struct UpstreamMcpClient {
    /// Configuration for this client
    config: UpstreamConfig,
    /// Server information
    server_info: Option<UpstreamServerInfo>,
    /// Discovered tools (original name -> tool info)
    tools: Arc<RwLock<HashMap<String, UpstreamTool>>>,
    /// Event bus for tool events
    event_bus: Option<Arc<RwLock<EventBus>>>,
    /// Connection state
    connected: Arc<RwLock<bool>>,
    /// Injected tool executor (optional - required for actual tool calls)
    executor: Option<Arc<BoxedMcpExecutor>>,
}

impl UpstreamMcpClient {
    /// Create a new client with configuration (does not connect yet)
    pub fn new(config: UpstreamConfig) -> Self {
        Self {
            config,
            server_info: None,
            tools: Arc::new(RwLock::new(HashMap::new())),
            event_bus: None,
            connected: Arc::new(RwLock::new(false)),
            executor: None,
        }
    }

    /// Set the event bus for this client
    pub fn with_event_bus(mut self, bus: Arc<RwLock<EventBus>>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Set the tool executor for this client
    ///
    /// The executor handles actual MCP tool calls. Without an executor,
    /// tool calls will return an error.
    pub fn with_executor(mut self, executor: BoxedMcpExecutor) -> Self {
        self.executor = Some(Arc::new(executor));
        self
    }

    /// Set the tool executor from an Arc (for sharing across instances)
    pub fn with_shared_executor(mut self, executor: Arc<BoxedMcpExecutor>) -> Self {
        self.executor = Some(executor);
        self
    }

    /// Check if an executor is configured
    pub fn has_executor(&self) -> bool {
        self.executor.is_some()
    }

    /// Get the upstream name
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Get the configured prefix
    pub fn prefix(&self) -> Option<&str> {
        self.config.prefix.as_deref()
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }

    /// Get server information (if connected)
    pub fn server_info(&self) -> Option<&UpstreamServerInfo> {
        self.server_info.as_ref()
    }

    /// Get list of discovered tools
    pub async fn tools(&self) -> Vec<UpstreamTool> {
        self.tools.read().await.values().cloned().collect()
    }

    /// Get a tool by its original name
    pub async fn get_tool(&self, name: &str) -> Option<UpstreamTool> {
        self.tools.read().await.get(name).cloned()
    }

    /// Get a tool by its prefixed name
    pub async fn get_tool_by_prefixed_name(&self, prefixed_name: &str) -> Option<UpstreamTool> {
        self.tools
            .read()
            .await
            .values()
            .find(|t| t.prefixed_name == prefixed_name)
            .cloned()
    }

    /// Apply prefix to a tool name
    #[allow(dead_code)]
    fn apply_prefix(&self, name: &str) -> String {
        match &self.config.prefix {
            Some(prefix) => format!("{}{}", prefix, name),
            None => name.to_string(),
        }
    }

    /// Check if a tool is allowed based on whitelist/blacklist
    pub fn is_tool_allowed(&self, name: &str) -> bool {
        // Check blacklist first
        if let Some(blocked) = &self.config.blocked_tools {
            if blocked.iter().any(|b| b == name || glob_match(b, name)) {
                return false;
            }
        }

        // Check whitelist
        if let Some(allowed) = &self.config.allowed_tools {
            return allowed.iter().any(|a| a == name || glob_match(a, name));
        }

        true
    }

    /// Emit an event through the event bus
    async fn emit_event(&self, event: Event) -> (Event, EventContext, Vec<HandlerError>) {
        if let Some(bus) = &self.event_bus {
            bus.read().await.emit(event)
        } else {
            (event, EventContext::new(), vec![])
        }
    }

    /// Emit mcp:attached event
    #[allow(dead_code)]
    async fn emit_attached(&self, server_info: &UpstreamServerInfo) {
        let payload = json!({
            "name": self.config.name,
            "server": {
                "name": server_info.name,
                "version": server_info.version,
                "protocol_version": server_info.protocol_version,
            },
            "transport": match &self.config.transport {
                TransportConfig::Stdio { command, .. } => json!({"type": "stdio", "command": command}),
                TransportConfig::Sse { url, .. } => json!({"type": "sse", "url": url}),
            },
        });

        let event = Event::mcp_attached(&self.config.name, payload)
            .with_source(format!("upstream:{}", self.config.name));

        info!("Emitting mcp:attached for {}", self.config.name);
        self.emit_event(event).await;
    }

    /// Emit tool:discovered event for each discovered tool
    async fn emit_tool_discovered(&self, tool: &UpstreamTool) {
        let payload = json!({
            "name": tool.prefixed_name,
            "original_name": tool.name,
            "description": tool.description,
            "input_schema": tool.input_schema,
            "upstream": tool.upstream,
        });

        let event = Event::tool_discovered(&tool.prefixed_name, payload)
            .with_source(format!("upstream:{}", self.config.name));

        debug!("Emitting tool:discovered for {}", tool.prefixed_name);
        self.emit_event(event).await;
    }

    /// Call a tool with event lifecycle
    ///
    /// This method:
    /// 1. Emits tool:before (allows modification or cancellation)
    /// 2. Calls the actual tool on upstream
    /// 3. Emits tool:after or tool:error
    /// 4. Returns the (possibly transformed) result
    pub async fn call_tool_with_events(
        &self,
        tool_name: &str,
        arguments: JsonValue,
    ) -> Result<ToolCallResult, GatewayError> {
        let start = Instant::now();

        // Find the tool
        let tool = self
            .get_tool_by_prefixed_name(tool_name)
            .await
            .or_else(|| {
                // Try without prefix
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(self.get_tool(tool_name))
                })
            })
            .ok_or_else(|| GatewayError::ToolNotFound(tool_name.to_string()))?;

        let _source = ToolSource::Upstream;

        // 1. Emit tool:before
        let before_event = Event::tool_before(&tool.prefixed_name, arguments.clone())
            .with_source(format!("upstream:{}", self.config.name));

        let (before_result, _ctx, errors) = self.emit_event(before_event).await;

        if !errors.is_empty() {
            for e in &errors {
                warn!("Hook error during tool:before: {}", e);
            }
        }

        // Check if cancelled
        if before_result.is_cancelled() {
            return Err(GatewayError::Execution(
                "Tool execution cancelled by hook".to_string(),
            ));
        }

        // Get potentially modified arguments
        let modified_args = before_result.payload;

        // 2. Call the actual tool (placeholder - actual implementation requires rmcp)
        // In a real implementation, this would use the rmcp client to call the tool
        let result = self.call_tool_internal(&tool.name, modified_args).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(call_result) => {
                // 3. Emit tool:after
                let result_json = serde_json::to_value(&call_result)
                    .unwrap_or_else(|_| json!({"error": "serialization failed"}));

                let payload = json!({
                    "result": result_json,
                    "duration_ms": duration_ms,
                    "upstream": self.config.name,
                });

                let after_event = Event::tool_after(&tool.prefixed_name, payload)
                    .with_source(format!("upstream:{}", self.config.name));

                let (after_result, _ctx, errors) = self.emit_event(after_event).await;

                if !errors.is_empty() {
                    for e in &errors {
                        warn!("Hook error during tool:after: {}", e);
                    }
                }

                // Extract result from event (hooks may have transformed it)
                if let Some(result_obj) = after_result.payload.get("result") {
                    if let Ok(transformed) = serde_json::from_value(result_obj.clone()) {
                        return Ok(transformed);
                    }
                }

                Ok(call_result)
            }
            Err(e) => {
                // 3. Emit tool:error
                let payload = json!({
                    "error": e.to_string(),
                    "duration_ms": duration_ms,
                    "upstream": self.config.name,
                });

                let error_event = Event::tool_error(&tool.prefixed_name, payload)
                    .with_source(format!("upstream:{}", self.config.name));

                self.emit_event(error_event).await;

                Err(e)
            }
        }
    }

    /// Internal tool call using injected executor
    ///
    /// Delegates to the configured `McpToolExecutor`. Returns an error if
    /// no executor is configured.
    async fn call_tool_internal(
        &self,
        tool_name: &str,
        arguments: JsonValue,
    ) -> Result<ToolCallResult, GatewayError> {
        let executor = self.executor.as_ref().ok_or_else(|| {
            GatewayError::Connection(
                "No executor configured - call with_executor() to enable tool calls".to_string(),
            )
        })?;

        executor
            .call_tool(tool_name, arguments)
            .await
            .map_err(|e| GatewayError::Execution(e.to_string()))
    }

    /// Update tools from a list (used when receiving toolListChanged notification)
    pub async fn update_tools(&self, tools: Vec<UpstreamTool>) {
        let mut tool_map = self.tools.write().await;
        tool_map.clear();

        for tool in tools {
            if self.is_tool_allowed(&tool.name) {
                self.emit_tool_discovered(&tool).await;
                tool_map.insert(tool.name.clone(), tool);
            }
        }
    }
}

/// Manager for multiple upstream MCP clients
pub struct McpGatewayManager {
    /// Clients by name
    clients: HashMap<String, Arc<UpstreamMcpClient>>,
    /// Shared event bus
    event_bus: Arc<RwLock<EventBus>>,
}

impl McpGatewayManager {
    /// Create a new manager with an event bus
    pub fn new(event_bus: EventBus) -> Self {
        Self {
            clients: HashMap::new(),
            event_bus: Arc::new(RwLock::new(event_bus)),
        }
    }

    /// Add an upstream client
    pub fn add_client(&mut self, config: UpstreamConfig) -> Arc<UpstreamMcpClient> {
        let name = config.name.clone();
        let client =
            Arc::new(UpstreamMcpClient::new(config).with_event_bus(Arc::clone(&self.event_bus)));
        self.clients.insert(name, Arc::clone(&client));
        client
    }

    /// Get a client by name
    pub fn get_client(&self, name: &str) -> Option<Arc<UpstreamMcpClient>> {
        self.clients.get(name).cloned()
    }

    /// Get all clients
    pub fn clients(&self) -> impl Iterator<Item = &Arc<UpstreamMcpClient>> {
        self.clients.values()
    }

    /// Get all tools from all clients
    pub async fn all_tools(&self) -> Vec<UpstreamTool> {
        let mut all = Vec::new();
        for client in self.clients.values() {
            all.extend(client.tools().await);
        }
        all
    }

    /// Find client that owns a tool by prefixed name
    pub async fn find_client_for_tool(
        &self,
        prefixed_name: &str,
    ) -> Option<Arc<UpstreamMcpClient>> {
        for client in self.clients.values() {
            if client
                .get_tool_by_prefixed_name(prefixed_name)
                .await
                .is_some()
            {
                return Some(Arc::clone(client));
            }
        }
        None
    }

    /// Call a tool, finding the appropriate client automatically
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: JsonValue,
    ) -> Result<ToolCallResult, GatewayError> {
        let client = self
            .find_client_for_tool(tool_name)
            .await
            .ok_or_else(|| GatewayError::ToolNotFound(tool_name.to_string()))?;

        client.call_tool_with_events(tool_name, arguments).await
    }

    /// Get the event bus
    pub fn event_bus(&self) -> &Arc<RwLock<EventBus>> {
        &self.event_bus
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("*", ""));
        assert!(glob_match("foo*", "foobar"));
        assert!(glob_match("foo*", "foo"));
        assert!(!glob_match("foo*", "bar"));
        assert!(glob_match("*bar", "foobar"));
        assert!(glob_match("foo*bar", "fooXXXbar"));
        assert!(glob_match("foo?bar", "fooXbar"));
        assert!(!glob_match("foo?bar", "fooXXbar"));
        assert!(glob_match("search_*", "search_repositories"));
        assert!(glob_match("gh_*", "gh_search_code"));
    }

    #[test]
    fn test_apply_prefix() {
        let config = UpstreamConfig {
            name: "github".to_string(),
            transport: TransportConfig::Stdio {
                command: "test".to_string(),
                args: vec![],
                env: vec![],
            },
            prefix: Some("gh_".to_string()),
            allowed_tools: None,
            blocked_tools: None,
            auto_reconnect: true,
        };

        let client = UpstreamMcpClient::new(config);
        assert_eq!(client.apply_prefix("search"), "gh_search");
    }

    #[test]
    fn test_apply_prefix_none() {
        let config = UpstreamConfig {
            name: "test".to_string(),
            transport: TransportConfig::Sse {
                url: "http://localhost:8080".to_string(),
                auth_header: None,
            },
            prefix: None,
            allowed_tools: None,
            blocked_tools: None,
            auto_reconnect: false,
        };

        let client = UpstreamMcpClient::new(config);
        assert_eq!(client.apply_prefix("search"), "search");
    }

    #[test]
    fn test_is_tool_allowed_no_filters() {
        let config = UpstreamConfig {
            name: "test".to_string(),
            transport: TransportConfig::Stdio {
                command: "test".to_string(),
                args: vec![],
                env: vec![],
            },
            prefix: None,
            allowed_tools: None,
            blocked_tools: None,
            auto_reconnect: true,
        };

        let client = UpstreamMcpClient::new(config);
        assert!(client.is_tool_allowed("any_tool"));
    }

    #[test]
    fn test_is_tool_allowed_whitelist() {
        let config = UpstreamConfig {
            name: "test".to_string(),
            transport: TransportConfig::Stdio {
                command: "test".to_string(),
                args: vec![],
                env: vec![],
            },
            prefix: None,
            allowed_tools: Some(vec!["search_*".to_string(), "get_*".to_string()]),
            blocked_tools: None,
            auto_reconnect: true,
        };

        let client = UpstreamMcpClient::new(config);
        assert!(client.is_tool_allowed("search_code"));
        assert!(client.is_tool_allowed("get_user"));
        assert!(!client.is_tool_allowed("delete_repo"));
    }

    #[test]
    fn test_is_tool_allowed_blacklist() {
        let config = UpstreamConfig {
            name: "test".to_string(),
            transport: TransportConfig::Stdio {
                command: "test".to_string(),
                args: vec![],
                env: vec![],
            },
            prefix: None,
            allowed_tools: None,
            blocked_tools: Some(vec!["delete_*".to_string(), "dangerous".to_string()]),
            auto_reconnect: true,
        };

        let client = UpstreamMcpClient::new(config);
        assert!(client.is_tool_allowed("search_code"));
        assert!(!client.is_tool_allowed("delete_repo"));
        assert!(!client.is_tool_allowed("dangerous"));
    }

    #[test]
    fn test_is_tool_allowed_blacklist_overrides_whitelist() {
        let config = UpstreamConfig {
            name: "test".to_string(),
            transport: TransportConfig::Stdio {
                command: "test".to_string(),
                args: vec![],
                env: vec![],
            },
            prefix: None,
            allowed_tools: Some(vec!["*".to_string()]),
            blocked_tools: Some(vec!["dangerous".to_string()]),
            auto_reconnect: true,
        };

        let client = UpstreamMcpClient::new(config);
        assert!(client.is_tool_allowed("safe_tool"));
        assert!(!client.is_tool_allowed("dangerous"));
    }

    #[test]
    fn test_transport_config_serialization() {
        let stdio = TransportConfig::Stdio {
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@mcp/server".to_string()],
            env: vec![("TOKEN".to_string(), "secret".to_string())],
        };

        let json = serde_json::to_value(&stdio).unwrap();
        assert_eq!(json["type"], "stdio");
        assert_eq!(json["command"], "npx");

        let sse = TransportConfig::Sse {
            url: "http://localhost:8080/sse".to_string(),
            auth_header: Some("Bearer token".to_string()),
        };

        let json = serde_json::to_value(&sse).unwrap();
        assert_eq!(json["type"], "sse");
        assert_eq!(json["url"], "http://localhost:8080/sse");
    }

    #[test]
    fn test_upstream_config_serialization() {
        let config = UpstreamConfig {
            name: "github".to_string(),
            transport: TransportConfig::Stdio {
                command: "npx".to_string(),
                args: vec![],
                env: vec![],
            },
            prefix: Some("gh_".to_string()),
            allowed_tools: Some(vec!["search_*".to_string()]),
            blocked_tools: None,
            auto_reconnect: true,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: UpstreamConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "github");
        assert_eq!(parsed.prefix, Some("gh_".to_string()));
    }

    #[tokio::test]
    async fn test_manager_add_client() {
        let bus = EventBus::new();
        let mut manager = McpGatewayManager::new(bus);

        let config = UpstreamConfig {
            name: "test".to_string(),
            transport: TransportConfig::Stdio {
                command: "test".to_string(),
                args: vec![],
                env: vec![],
            },
            prefix: None,
            allowed_tools: None,
            blocked_tools: None,
            auto_reconnect: false,
        };

        manager.add_client(config);

        assert!(manager.get_client("test").is_some());
        assert!(manager.get_client("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_upstream_tool() {
        let tool = UpstreamTool {
            name: "search_code".to_string(),
            prefixed_name: "gh_search_code".to_string(),
            description: Some("Search code on GitHub".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                }
            }),
            upstream: "github".to_string(),
        };

        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json["name"], "search_code");
        assert_eq!(json["prefixed_name"], "gh_search_code");
    }
}
