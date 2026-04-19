use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::BufReader;
use tokio::process::Command;

use super::types::{AgentProcess, ClientConfig};
use super::{BoxedReader, BoxedWriter, CrucibleAcpClient};
use crate::session::AcpSession;
use crate::{ClientError, Result};
use crucible_core::types::acp::SessionId;

impl CrucibleAcpClient {
    /// Create a client with a pre-connected in-process transport
    ///
    /// This allows using the client with a mock agent or other in-process
    /// transport without spawning a subprocess. Used primarily for testing.
    ///
    /// # Arguments
    ///
    /// * `config` - Client configuration (agent_path is ignored)
    /// * `writer` - Async writer for sending requests to the agent
    /// * `reader` - Async buffered reader for receiving responses
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Create duplex streams for in-process communication
    /// let (client_to_agent, agent_to_client) = tokio::io::duplex(8192);
    /// let (read_half, write_half) = tokio::io::split(client_to_agent);
    ///
    /// let client = CrucibleAcpClient::with_transport(
    ///     config,
    ///     Box::pin(write_half),
    ///     Box::pin(BufReader::new(read_half)),
    /// );
    /// ```
    pub fn with_transport(config: ClientConfig, writer: BoxedWriter, reader: BoxedReader) -> Self {
        Self {
            config,
            agent_name: "mock".to_string(),
            active_session: None,
            agent_process: None,
            agent_stdin: None,
            agent_stdout: None,
            boxed_writer: Some(writer),
            boxed_reader: Some(reader),
            available_commands: Vec::new(),
            permission_handler: None,
            agent_mcp_capabilities: None,
        }
    }

    /// Connect to an agent and establish a session
    ///
    /// This will start the agent process if needed and perform protocol
    /// negotiation to establish a communication session.
    ///
    /// # Returns
    ///
    /// An active `AcpSession` that can be used to send requests to the agent
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The agent process cannot be started
    /// - Protocol negotiation fails
    /// - Connection times out
    pub async fn connect(&mut self) -> Result<AcpSession> {
        // Spawn the agent process
        let _process = self.spawn_agent().await?;

        // Mark as connected
        self.mark_connected();

        // Create and return a session
        use crate::session::TransportConfig;
        let session_id = format!(
            "session-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );

        Ok(AcpSession::new(TransportConfig::default(), session_id))
    }

    /// Check if a transport is available (either process-based or in-process)
    ///
    /// Returns true if there's a reader/writer available for communication.
    pub fn has_transport(&self) -> bool {
        (self.boxed_reader.is_some() && self.boxed_writer.is_some())
            || (self.agent_stdin.is_some() && self.agent_stdout.is_some())
    }

    /// Spawn the agent process
    ///
    /// This method spawns the agent executable specified in the client configuration
    /// and captures stdin/stdout for communication.
    ///
    /// If a transport is already available (e.g., via `with_transport`), this method
    /// returns immediately without spawning a process.
    ///
    /// # Returns
    ///
    /// An `AgentProcess` handle that can be used to interact with the spawned process
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The agent executable does not exist
    /// - The process cannot be spawned
    /// - Permissions are insufficient
    pub async fn spawn_agent(&mut self) -> Result<AgentProcess> {
        // If we already have a transport (e.g., from with_transport), skip spawning
        if self.has_transport() {
            tracing::debug!(agent = %self.agent_name, "Using pre-configured transport, skipping spawn");
            return Ok(AgentProcess {
                child: {
                    #[cfg(target_os = "windows")]
                    let mut cmd = Command::new("cmd");
                    #[cfg(target_os = "windows")]
                    cmd.args(["/C", "exit 0"]);

                    #[cfg(not(target_os = "windows"))]
                    let mut cmd = Command::new("true");

                    cmd.spawn().unwrap()
                }, // Dummy process
            });
        }

        tracing::info!(agent = %self.agent_name, path = %self.config.agent_path.display(), "Spawning ACP agent process");

        let mut cmd = Command::new(&self.config.agent_path);

        // Add command-line arguments if specified
        if let Some(ref args) = self.config.agent_args {
            cmd.args(args);
        }

        // Set working directory if specified
        if let Some(ref working_dir) = self.config.working_dir {
            cmd.current_dir(working_dir);
        }

        // Set environment variables if specified
        if let Some(ref env_vars) = self.config.env_vars {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        // Set up stdio for communication
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Spawn the process
        let mut child = cmd
            .spawn()
            .map_err(|e| ClientError::Connection(format!("Failed to spawn agent: {}", e)))?;

        tracing::debug!(agent = %self.agent_name, "ACP agent process spawned successfully");

        // Capture stdin and stdout for communication
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ClientError::Connection("Failed to capture agent stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ClientError::Connection("Failed to capture agent stdout".to_string()))?;

        // Forward stderr to tracing for debugging
        if let Some(stderr) = child.stderr.take() {
            let agent_name = self.agent_name.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let mut reader = tokio::io::BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                tracing::debug!(agent = %agent_name, "[agent stderr] {}", trimmed);
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        // Store stdio handles and process in the client
        self.agent_stdin = Some(stdin);
        self.agent_stdout = Some(BufReader::new(stdout));

        // Create a handle before storing the child
        let process = AgentProcess { child };

        Ok(process)
    }

    /// Disconnect from the agent and clean up resources
    ///
    /// # Arguments
    ///
    /// * `session` - The session to disconnect
    ///
    /// # Errors
    ///
    /// Returns an error if cleanup fails
    pub async fn disconnect(&mut self, _session: &AcpSession) -> Result<()> {
        // Mark as disconnected
        self.mark_disconnected();

        // Clean up stdio handles
        self.agent_stdin = None;
        self.agent_stdout = None;

        // Note: agent_process will be dropped, which will terminate the process
        // In a full implementation, we would send a shutdown message first

        Ok(())
    }

    /// Check if currently connected to an agent
    ///
    /// # Returns
    ///
    /// `true` if there is an active connection, `false` otherwise
    pub fn is_connected(&self) -> bool {
        self.active_session.is_some()
    }

    /// Connect to agent with capability-aware MCP transport negotiation.
    ///
    /// This performs the complete connection sequence and picks the best MCP
    /// transport based on the agent's reported capabilities:
    ///
    /// 1. Spawn agent process (or use pre-connected transport)
    /// 2. Send InitializeRequest — reads agent capabilities
    /// 3. Choose transport (priority order):
    ///    - HTTP (Streamable HTTP) if `mcp_url` is provided AND agent reports `mcp_capabilities.http == true`
    ///    - Stdio otherwise (all agents MUST support stdio per ACP spec)
    /// 4. Create session with chosen transport
    ///
    /// Note: `McpServer::Sse` means legacy SSE transport (GET /sse → endpoint event),
    /// which our `StreamableHttpService` does not speak. We skip it entirely.
    ///
    /// # Arguments
    ///
    /// * `mcp_url` - Optional URL to an in-process MCP server. If `None` or if
    ///   the agent doesn't support HTTP, falls back to stdio transport.
    pub async fn connect_with_best_mcp(&mut self, mcp_url: Option<&str>) -> Result<AcpSession> {
        use agent_client_protocol::{
            InitializeRequest, McpServer, McpServerHttp, NewSessionRequest,
        };

        tracing::debug!(agent = %self.agent_name, mcp_url = ?mcp_url, "Starting capability-aware ACP handshake");

        // 1. Spawn agent process (no-op if transport already connected)
        let _process = self.spawn_agent().await?;

        // 2. Initialize — this stores agent capabilities on self
        let init_request = InitializeRequest::new(1u16.into());
        let _init_response = self.initialize(init_request).await?;

        // 3. Choose transport based on agent capabilities
        // Priority: HTTP (Streamable HTTP) > Stdio (all agents MUST support stdio per ACP spec)
        tracing::debug!(
            agent = %self.agent_name,
            supports_http = self.agent_supports_http_mcp(),
            mcp_url_provided = mcp_url.is_some(),
            "MCP transport decision"
        );
        let crucible_mcp_server = if let Some(url) = mcp_url {
            if self.agent_supports_http_mcp() {
                tracing::info!(
                    agent = %self.agent_name,
                    url = %url,
                    "Agent supports HTTP MCP — using Streamable HTTP transport"
                );
                McpServer::Http(McpServerHttp::new("crucible", url))
            } else {
                // Agent doesn't support Streamable HTTP (may only support legacy SSE
                // or nothing). Our server uses StreamableHttpService which only speaks
                // Streamable HTTP, not legacy SSE. Fall back to stdio.
                tracing::info!(
                    agent = %self.agent_name,
                    "Agent lacks Streamable HTTP support, falling back to stdio transport"
                );
                Self::build_stdio_mcp_server()
            }
        } else {
            tracing::debug!(agent = %self.agent_name, "No MCP URL provided, using stdio transport");
            Self::build_stdio_mcp_server()
        };

        // 4. Create session with chosen transport
        // Must be absolute — the agent process runs in working_dir, so a relative
        // cwd would resolve to working_dir/cwd (double-nesting).
        let cwd = self
            .config
            .working_dir
            .as_ref()
            .and_then(|p| std::fs::canonicalize(p).ok())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/"));

        let session_request = NewSessionRequest::new(cwd).mcp_servers(vec![crucible_mcp_server]);
        let session_response = self.create_new_session(session_request).await?;

        self.mark_connected();

        tracing::info!(
            agent = %self.agent_name,
            session_id = %session_response.session_id,
            "ACP agent connected with session"
        );

        use crate::session::TransportConfig;
        Ok(AcpSession::new(
            TransportConfig::default(),
            session_response.session_id.to_string(),
        ))
    }

    /// Mark the client as connected
    ///
    /// This sets an active session to indicate a connection is established
    pub fn mark_connected(&mut self) {
        // Generate a temporary session ID
        let session_id = SessionId::new();
        self.active_session = Some(session_id);
    }

    /// Mark the client as disconnected
    ///
    /// This clears the active session
    pub fn mark_disconnected(&mut self) {
        self.active_session = None;
    }
}
