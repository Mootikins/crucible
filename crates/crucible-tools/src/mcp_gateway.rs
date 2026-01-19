//! MCP Gateway Manager - aggregates upstream MCP servers

use crate::mcp_client::{create_stdio_executor_with_env, RmcpExecutor};
use crucible_config::mcp::{McpConfig, TransportType, UpstreamServerConfig};
use crucible_core::traits::mcp::{McpToolExecutor, McpToolInfo, ToolCallResult};
use crucible_core::utils::glob_match;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Error type for gateway operations.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    /// Upstream server not found by name.
    #[error("Upstream '{0}' not found")]
    UpstreamNotFound(String),
    /// Tool not found by prefixed name.
    #[error("Tool '{0}' not found")]
    ToolNotFound(String),
    /// Failed to establish connection to upstream.
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    /// Tool execution failed on upstream.
    #[error("Tool call failed: {0}")]
    ToolCallFailed(String),
    /// Upstream with this name already registered.
    #[error("Upstream '{0}' already exists")]
    DuplicateUpstream(String),
    /// Prefix already in use by another upstream.
    #[error("Prefix '{0}' already in use by upstream '{1}'")]
    DuplicatePrefix(String, String),
    /// Tool call timed out.
    #[error("Tool '{0}' timed out after {1}s")]
    Timeout(String, u64),
    /// Invalid prefix format.
    #[error("Invalid prefix '{0}': {1}")]
    InvalidPrefix(String, String),
}

/// Result type for gateway operations.
pub type GatewayResult<T> = Result<T, GatewayError>;

/// State of an upstream connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Successfully connected and ready.
    Connected,
    /// Not connected (initial state or after disconnect).
    Disconnected,
    /// Connection failed with error.
    Error,
}

/// An upstream MCP client with its configuration and tools.
pub struct UpstreamClient {
    /// Unique name for this upstream.
    pub name: String,
    /// Prefix applied to all tools from this upstream.
    pub prefix: String,
    /// Configuration for this upstream.
    pub config: UpstreamServerConfig,
    executor: Option<RmcpExecutor>,
    tools: Vec<McpToolInfo>,
    state: ConnectionState,
}

impl UpstreamClient {
    /// Create a new upstream client from configuration.
    #[must_use]
    pub fn new(config: UpstreamServerConfig) -> Self {
        Self {
            name: config.name.clone(),
            prefix: config.prefix.clone(),
            config,
            executor: None,
            tools: Vec::new(),
            state: ConnectionState::Disconnected,
        }
    }

    /// Connect to the upstream server and discover available tools.
    ///
    /// # Errors
    /// Returns `GatewayError::ConnectionFailed` if connection fails.
    pub async fn connect(&mut self) -> GatewayResult<()> {
        match &self.config.transport {
            TransportType::Stdio { command, args, env } => {
                let args_refs: Vec<&str> = args.iter().map(String::as_str).collect();
                let env_refs: Vec<(&str, &str)> =
                    env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

                match create_stdio_executor_with_env(command, &args_refs, &env_refs).await {
                    Ok(executor) => {
                        self.tools = executor
                            .tools()
                            .await
                            .into_iter()
                            .filter(|t| self.is_tool_allowed(&t.name))
                            .map(|mut t| {
                                t.prefixed_name = format!("{}{}", self.prefix, t.name);
                                t.upstream.clone_from(&self.name);
                                t
                            })
                            .collect();

                        info!(
                            "Connected to upstream '{}' with {} tools",
                            self.name,
                            self.tools.len()
                        );
                        self.executor = Some(executor);
                        self.state = ConnectionState::Connected;
                        Ok(())
                    }
                    Err(e) => {
                        self.state = ConnectionState::Error;
                        Err(GatewayError::ConnectionFailed(format!(
                            "{}: {}",
                            self.name, e
                        )))
                    }
                }
            }
            TransportType::Sse { url, .. } => {
                self.state = ConnectionState::Error;
                Err(GatewayError::ConnectionFailed(format!(
                    "{}: SSE transport not yet implemented (url: {})",
                    self.name, url
                )))
            }
        }
    }

    fn is_tool_allowed(&self, tool_name: &str) -> bool {
        if let Some(blocked) = &self.config.blocked_tools {
            for pattern in blocked {
                if glob_match(pattern, tool_name) {
                    return false;
                }
            }
        }

        if let Some(allowed) = &self.config.allowed_tools {
            for pattern in allowed {
                if glob_match(pattern, tool_name) {
                    return true;
                }
            }
            return false;
        }

        true
    }

    /// Get the list of tools available from this upstream.
    #[must_use]
    pub fn tools(&self) -> &[McpToolInfo] {
        &self.tools
    }

    /// Get the current connection state.
    #[must_use]
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Call a tool on this upstream.
    ///
    /// # Errors
    /// Returns error if not connected, tool call fails, or times out.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        args: JsonValue,
    ) -> GatewayResult<ToolCallResult> {
        let executor = self.executor.as_ref().ok_or_else(|| {
            GatewayError::ConnectionFailed(format!("{}: not connected", self.name))
        })?;

        let timeout_duration = Duration::from_secs(self.config.timeout_secs);

        tokio::time::timeout(timeout_duration, executor.call_tool(tool_name, args))
            .await
            .map_err(|_| GatewayError::Timeout(tool_name.to_string(), self.config.timeout_secs))?
            .map_err(|e| GatewayError::ToolCallFailed(e.to_string()))
    }

    /// Disconnect from the upstream server.
    pub fn disconnect(&mut self) {
        self.executor = None;
        self.tools.clear();
        self.state = ConnectionState::Disconnected;
    }
}

/// Manages multiple upstream MCP server connections.
pub struct McpGatewayManager {
    upstreams: HashMap<String, UpstreamClient>,
    tool_index: HashMap<String, String>,
}

impl McpGatewayManager {
    /// Create a new empty gateway manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            upstreams: HashMap::new(),
            tool_index: HashMap::new(),
        }
    }

    /// Create and connect to all upstreams from config.
    ///
    /// # Errors
    /// Returns error if any upstream fails to connect (logged but continues).
    pub async fn from_config(config: &McpConfig) -> GatewayResult<Self> {
        let mut manager = Self::new();

        for server_config in &config.servers {
            if let Err(e) = manager.add_upstream(server_config.clone()).await {
                warn!("Failed to add upstream '{}': {}", server_config.name, e);
            }
        }

        Ok(manager)
    }

    /// Add and connect to an upstream server.
    ///
    /// # Errors
    /// Returns error if upstream name/prefix is duplicate or connection fails.
    pub async fn add_upstream(&mut self, config: UpstreamServerConfig) -> GatewayResult<()> {
        let name = config.name.clone();
        let prefix = config.prefix.clone();

        Self::validate_prefix(&prefix)?;

        if self.upstreams.contains_key(&name) {
            return Err(GatewayError::DuplicateUpstream(name));
        }

        for (existing_name, existing_client) in &self.upstreams {
            if existing_client.prefix == prefix {
                return Err(GatewayError::DuplicatePrefix(prefix, existing_name.clone()));
            }
        }

        let mut client = UpstreamClient::new(config);
        client.connect().await?;

        for tool in client.tools() {
            self.tool_index
                .insert(tool.prefixed_name.clone(), name.clone());
        }

        self.upstreams.insert(name, client);
        Ok(())
    }

    fn validate_prefix(prefix: &str) -> GatewayResult<()> {
        if prefix.is_empty() {
            return Err(GatewayError::InvalidPrefix(
                prefix.to_string(),
                "prefix cannot be empty".to_string(),
            ));
        }
        if !prefix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(GatewayError::InvalidPrefix(
                prefix.to_string(),
                "prefix must contain only alphanumeric characters and underscores".to_string(),
            ));
        }
        if !prefix.ends_with('_') {
            return Err(GatewayError::InvalidPrefix(
                prefix.to_string(),
                "prefix should end with underscore for clarity".to_string(),
            ));
        }
        Ok(())
    }

    /// Remove an upstream server.
    ///
    /// # Errors
    /// Returns error if upstream not found.
    pub fn remove_upstream(&mut self, name: &str) -> GatewayResult<()> {
        let mut client = self
            .upstreams
            .remove(name)
            .ok_or_else(|| GatewayError::UpstreamNotFound(name.to_string()))?;

        for tool in client.tools() {
            self.tool_index.remove(&tool.prefixed_name);
        }

        client.disconnect();
        info!("Removed upstream '{}'", name);
        Ok(())
    }

    /// Get all tools from all connected upstreams.
    #[must_use]
    pub fn all_tools(&self) -> Vec<McpToolInfo> {
        self.upstreams
            .values()
            .flat_map(|c| c.tools().iter().cloned())
            .collect()
    }

    /// Find which upstream owns a tool by its prefixed name.
    #[must_use]
    pub fn find_upstream(&self, prefixed_tool_name: &str) -> Option<&str> {
        self.tool_index.get(prefixed_tool_name).map(String::as_str)
    }

    /// Call a tool by its prefixed name.
    ///
    /// # Errors
    /// Returns error if tool or upstream not found, or call fails.
    pub async fn call_tool(
        &self,
        prefixed_name: &str,
        args: JsonValue,
    ) -> GatewayResult<ToolCallResult> {
        let upstream_name = self
            .tool_index
            .get(prefixed_name)
            .ok_or_else(|| GatewayError::ToolNotFound(prefixed_name.to_string()))?;

        let client = self
            .upstreams
            .get(upstream_name)
            .ok_or_else(|| GatewayError::UpstreamNotFound(upstream_name.clone()))?;

        let original_name = prefixed_name
            .strip_prefix(&client.prefix)
            .unwrap_or(prefixed_name);

        debug!(
            "Calling tool '{}' (original: '{}') on upstream '{}'",
            prefixed_name, original_name, upstream_name
        );

        client.call_tool(original_name, args).await
    }

    /// Check if a prefixed tool name belongs to this gateway.
    #[must_use]
    pub fn has_tool(&self, prefixed_name: &str) -> bool {
        self.tool_index.contains_key(prefixed_name)
    }

    /// Get upstream names.
    pub fn upstream_names(&self) -> impl Iterator<Item = &str> {
        self.upstreams.keys().map(String::as_str)
    }

    /// Get upstream count.
    #[must_use]
    pub fn upstream_count(&self) -> usize {
        self.upstreams.len()
    }

    /// Get tool count across all upstreams.
    #[must_use]
    pub fn tool_count(&self) -> usize {
        self.tool_index.len()
    }

    /// Try to reconnect a disconnected upstream.
    ///
    /// # Errors
    /// Returns error if upstream not found or reconnection fails.
    pub async fn reconnect(&mut self, name: &str) -> GatewayResult<()> {
        let client = self
            .upstreams
            .get_mut(name)
            .ok_or_else(|| GatewayError::UpstreamNotFound(name.to_string()))?;

        for tool in client.tools() {
            self.tool_index.remove(&tool.prefixed_name);
        }

        client.connect().await?;

        for tool in client.tools() {
            self.tool_index
                .insert(tool.prefixed_name.clone(), name.to_string());
        }

        Ok(())
    }
}

impl Default for McpGatewayManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_config(name: &str, prefix: &str) -> UpstreamServerConfig {
        UpstreamServerConfig {
            name: name.to_string(),
            prefix: prefix.to_string(),
            transport: TransportType::Stdio {
                command: "echo".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
            allowed_tools: None,
            blocked_tools: None,
            auto_reconnect: true,
            timeout_secs: 30,
        }
    }

    #[test]
    fn test_upstream_client_creation() {
        let config = test_config("test", "t_");
        let client = UpstreamClient::new(config);
        assert_eq!(client.name, "test");
        assert_eq!(client.prefix, "t_");
        assert_eq!(client.state(), ConnectionState::Disconnected);
    }

    #[test]
    fn test_tool_filtering_blocked() {
        let mut config = test_config("test", "t_");
        config.blocked_tools = Some(vec!["dangerous_*".to_string()]);
        let client = UpstreamClient::new(config);

        assert!(client.is_tool_allowed("safe_tool"));
        assert!(!client.is_tool_allowed("dangerous_tool"));
        assert!(!client.is_tool_allowed("dangerous_action"));
    }

    #[test]
    fn test_tool_filtering_allowed() {
        let mut config = test_config("test", "t_");
        config.allowed_tools = Some(vec!["read_*".to_string(), "list_*".to_string()]);
        let client = UpstreamClient::new(config);

        assert!(client.is_tool_allowed("read_file"));
        assert!(client.is_tool_allowed("list_notes"));
        assert!(!client.is_tool_allowed("delete_file"));
    }

    #[test]
    fn test_tool_filtering_combined() {
        let mut config = test_config("test", "t_");
        config.allowed_tools = Some(vec!["*_tool".to_string()]);
        config.blocked_tools = Some(vec!["dangerous_*".to_string()]);
        let client = UpstreamClient::new(config);

        assert!(client.is_tool_allowed("safe_tool"));
        assert!(!client.is_tool_allowed("dangerous_tool"));
    }

    #[test]
    fn test_gateway_manager_creation() {
        let manager = McpGatewayManager::new();
        assert_eq!(manager.upstream_count(), 0);
        assert_eq!(manager.tool_count(), 0);
    }

    #[test]
    fn test_prefix_validation_empty() {
        let result = McpGatewayManager::validate_prefix("");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GatewayError::InvalidPrefix(_, _)
        ));
    }

    #[test]
    fn test_prefix_validation_no_trailing_underscore() {
        let result = McpGatewayManager::validate_prefix("gh");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GatewayError::InvalidPrefix(_, _)
        ));
    }

    #[test]
    fn test_prefix_validation_special_chars() {
        let result = McpGatewayManager::validate_prefix("gh-mcp_");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GatewayError::InvalidPrefix(_, _)
        ));
    }

    #[test]
    fn test_prefix_validation_valid() {
        assert!(McpGatewayManager::validate_prefix("gh_").is_ok());
        assert!(McpGatewayManager::validate_prefix("fs_").is_ok());
        assert!(McpGatewayManager::validate_prefix("my_tool_").is_ok());
        assert!(McpGatewayManager::validate_prefix("MCP123_").is_ok());
    }

    #[test]
    fn test_blocked_tools_take_precedence_over_allowed() {
        let mut config = test_config("test", "t_");
        config.allowed_tools = Some(vec!["*".to_string()]);
        config.blocked_tools = Some(vec!["dangerous_*".to_string()]);
        let client = UpstreamClient::new(config);

        assert!(!client.is_tool_allowed("dangerous_action"));
        assert!(client.is_tool_allowed("safe_action"));
    }
}
