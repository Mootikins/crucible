//! ACP Client implementation for agent communication
//!
//! This module provides the main client interface for communicating with
//! AI agents via the Agent Client Protocol.
//!
//! ## Responsibilities
//!
//! - Agent process lifecycle management (start, stop, restart)
//! - Connection establishment and maintenance
//! - Protocol version negotiation
//! - Message routing to appropriate handlers
//!
//! ## Design Principles
//!
//! - **Single Responsibility**: Focused on agent connection and lifecycle
//! - **Dependency Inversion**: Uses traits from crucible-core for extensibility
//! - **Open/Closed**: New agent types can be added without modifying this code

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::io::{AsyncBufRead, AsyncWrite, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

/// Global request ID counter for JSON-RPC requests.
/// Shared between send_request and send_prompt_with_streaming to ensure unique IDs.
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

use agent_client_protocol::{AvailableCommand, RequestPermissionOutcome, RequestPermissionRequest};
use crucible_core::types::acp::SessionId;

mod connection;
mod io;
mod permission;
mod protocol;
mod session_manager;
mod streaming;
mod tools;
mod types;

#[cfg(test)]
mod tests;

pub use types::{AgentProcess, ClientConfig};

/// Type-erased async writer for agent communication
pub type BoxedWriter = Pin<Box<dyn AsyncWrite + Send + Sync + Unpin>>;
/// Type-erased async reader for agent communication
pub type BoxedReader = Pin<Box<dyn AsyncBufRead + Send + Sync + Unpin>>;
pub type PermissionOutcomeFuture = Pin<Box<dyn Future<Output = RequestPermissionOutcome> + Send>>;
pub type PermissionRequestHandler =
    Arc<dyn Fn(RequestPermissionRequest) -> PermissionOutcomeFuture + Send + Sync>;

/// Main client for ACP communication
///
/// This struct manages the lifecycle of agent connections and provides
/// the primary interface for sending requests to agents.
pub struct CrucibleAcpClient {
    pub(super) config: ClientConfig,
    /// Agent name (e.g., "opencode", "claude") for display
    pub(super) agent_name: String,
    /// Current active session ID, if any
    pub(super) active_session: Option<SessionId>,
    /// Agent process handle, if spawned (None for in-process transports)
    pub(super) agent_process: Option<Child>,
    /// Agent stdin for writing requests (concrete type from process)
    pub(super) agent_stdin: Option<ChildStdin>,
    /// Agent stdout for reading responses (concrete type from process)
    pub(super) agent_stdout: Option<BufReader<ChildStdout>>,
    /// Type-erased writer for in-process transports (e.g., ThreadedMockAgent)
    pub(super) boxed_writer: Option<BoxedWriter>,
    /// Type-erased reader for in-process transports (e.g., ThreadedMockAgent)
    pub(super) boxed_reader: Option<BoxedReader>,
    /// Latest available slash commands advertised by the agent
    pub(super) available_commands: Vec<AvailableCommand>,
    pub(super) permission_handler: Option<PermissionRequestHandler>,
    /// Agent's MCP transport capabilities, populated after initialize()
    pub(super) agent_mcp_capabilities: Option<agent_client_protocol::McpCapabilities>,
}

// Manual Debug implementation since Child doesn't implement Debug
impl std::fmt::Debug for CrucibleAcpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrucibleAcpClient")
            .field("config", &self.config)
            .field("agent_name", &self.agent_name)
            .field("active_session", &self.active_session)
            .field("agent_process", &self.agent_process.is_some())
            .field("agent_stdin", &self.agent_stdin.is_some())
            .field("agent_stdout", &self.agent_stdout.is_some())
            .field("boxed_writer", &self.boxed_writer.is_some())
            .field("boxed_reader", &self.boxed_reader.is_some())
            .field("available_commands", &self.available_commands.len())
            .field("permission_handler", &self.permission_handler.is_some())
            .field("agent_mcp_capabilities", &self.agent_mcp_capabilities)
            .finish()
    }
}

impl CrucibleAcpClient {
    /// Create a new ACP client with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Client configuration
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let config = ClientConfig {
    ///     agent_path: PathBuf::from("/path/to/agent"),
    ///     working_dir: None,
    ///     env_vars: None,
    ///     timeout_ms: Some(5000),
    ///     max_retries: Some(3),
    /// };
    /// let client = CrucibleAcpClient::new(config);
    /// ```
    pub fn new(config: ClientConfig) -> Self {
        Self::with_name(config, "acp".to_string())
    }

    /// Create a new ACP client with a specific agent name
    pub fn with_name(config: ClientConfig, agent_name: String) -> Self {
        Self {
            config,
            agent_name,
            active_session: None,
            agent_process: None,
            agent_stdin: None,
            agent_stdout: None,
            boxed_writer: None,
            boxed_reader: None,
            available_commands: Vec::new(),
            permission_handler: None,
            agent_mcp_capabilities: None,
        }
    }

    pub fn with_permission_handler(mut self, handler: PermissionRequestHandler) -> Self {
        self.permission_handler = Some(handler);
        self
    }

    /// Get the agent name for display
    pub fn agent_name(&self) -> &str {
        &self.agent_name
    }

    /// Get the latest slash commands advertised by the agent
    pub fn available_commands(&self) -> &[AvailableCommand] {
        &self.available_commands
    }

    /// Whether the agent reported HTTP MCP transport support during initialization.
    ///
    /// Returns `false` if `initialize()` has not been called yet.
    pub fn agent_supports_http_mcp(&self) -> bool {
        self.agent_mcp_capabilities
            .as_ref()
            .map(|c| c.http)
            .unwrap_or(false)
    }

    /// Whether the agent reported SSE MCP transport support during initialization.
    ///
    /// Returns `false` if `initialize()` has not been called yet.
    pub fn agent_supports_sse_mcp(&self) -> bool {
        self.agent_mcp_capabilities
            .as_ref()
            .map(|c| c.sse)
            .unwrap_or(false)
    }

    /// Get the client configuration
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Get the current active session, if any
    pub fn active_session(&self) -> Option<&SessionId> {
        self.active_session.as_ref()
    }
}
