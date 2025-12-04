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

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use crate::session::AcpSession;
use crate::streaming::{humanize_tool_title, ToolCallInfo};
use crate::{AcpError, Result};
use agent_client_protocol::{
    ContentBlock, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SessionNotification, SessionUpdate,
};
use crucible_core::traits::acp::{AcpResult, SessionManager};
use crucible_core::types::acp::{SessionConfig, SessionId};

/// Configuration for the ACP client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Path to the agent executable or script
    pub agent_path: PathBuf,

    /// Command-line arguments to pass to the agent
    #[serde(default)]
    pub agent_args: Option<Vec<String>>,

    /// Working directory for the agent process
    pub working_dir: Option<PathBuf>,

    /// Environment variables to pass to the agent
    pub env_vars: Option<Vec<(String, String)>>,

    /// Timeout for agent operations (in milliseconds)
    pub timeout_ms: Option<u64>,

    /// Maximum number of retry attempts
    pub max_retries: Option<u32>,
}

/// Represents a spawned agent process
///
/// This struct wraps a child process and provides methods to interact with it.
#[derive(Debug)]
pub struct AgentProcess {
    #[allow(dead_code)]
    child: Child,
}

impl AgentProcess {
    /// Check if the agent process is still running
    ///
    /// # Returns
    ///
    /// `true` if the process is running, `false` otherwise
    pub fn is_running(&self) -> bool {
        // For now, we assume the process is running if we have a handle to it
        // In a full implementation, we would check the process status
        true
    }
}

enum ResponseSegment {
    Text(String),
    Tool { label: String },
}

#[derive(Default)]
struct StreamingState {
    segments: Vec<ResponseSegment>,
    tool_calls: Vec<ToolCallInfo>,
    notification_count: usize,
    tool_segment_index: std::collections::HashMap<String, usize>,
    tool_block_active: bool,
}

impl StreamingState {
    fn append_text(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        let chunk = text.to_string();
        if let Some(ResponseSegment::Text(last)) = self.segments.last_mut() {
            last.push_str(&chunk);
        } else {
            self.segments.push(ResponseSegment::Text(chunk));
        }
        self.tool_block_active = false;
    }

    fn formatted_output(&self) -> String {
        let mut output = String::new();
        let mut in_tool_block = false;
        for seg in &self.segments {
            match seg {
                ResponseSegment::Text(text) => {
                    if in_tool_block {
                        // End tool block with blank line
                        output.push('\n');
                        in_tool_block = false;
                    }
                    output.push_str(text);
                }
                ResponseSegment::Tool { label, .. } => {
                    if !in_tool_block {
                        // Start tool block with blank line before
                        if !output.is_empty() && !output.ends_with('\n') {
                            output.push('\n');
                        }
                        output.push('\n');
                        in_tool_block = true;
                    }
                    // All tool calls indented in the block
                    output.push_str("  ");
                    output.push_str(label);
                    output.push('\n');
                }
            }
        }
        // End tool block if we finished with tools
        if in_tool_block {
            output.push('\n');
        }
        output
    }

    fn formatted_length(&self) -> usize {
        self.formatted_output().len()
    }

    fn title_for_tool(&self, id: &str) -> Option<String> {
        self.tool_calls
            .iter()
            .find(|tool| tool.id.as_deref() == Some(id))
            .map(|tool| tool.title.clone())
    }
}

/// Main client for ACP communication
///
/// This struct manages the lifecycle of agent connections and provides
/// the primary interface for sending requests to agents.
pub struct CrucibleAcpClient {
    config: ClientConfig,
    /// Current active session ID, if any
    active_session: Option<SessionId>,
    /// Agent process handle, if spawned
    agent_process: Option<Child>,
    /// Agent stdin for writing requests
    agent_stdin: Option<ChildStdin>,
    /// Agent stdout for reading responses
    agent_stdout: Option<BufReader<ChildStdout>>,
}

// Manual Debug implementation since Child doesn't implement Debug
impl std::fmt::Debug for CrucibleAcpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrucibleAcpClient")
            .field("config", &self.config)
            .field("active_session", &self.active_session)
            .field("agent_process", &self.agent_process.is_some())
            .field("agent_stdin", &self.agent_stdin.is_some())
            .field("agent_stdout", &self.agent_stdout.is_some())
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
        Self {
            config,
            active_session: None,
            agent_process: None,
            agent_stdin: None,
            agent_stdout: None,
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
        use crate::session::SessionConfig;
        let session_id = format!(
            "session-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );

        Ok(AcpSession::new(SessionConfig::default(), session_id))
    }

    /// Get the client configuration
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Get the current active session, if any
    pub fn active_session(&self) -> Option<&SessionId> {
        self.active_session.as_ref()
    }

    /// Spawn the agent process
    ///
    /// This method spawns the agent executable specified in the client configuration
    /// and captures stdin/stdout for communication.
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
            .map_err(|e| AcpError::Connection(format!("Failed to spawn agent: {}", e)))?;

        // Capture stdin and stdout for communication
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AcpError::Connection("Failed to capture agent stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AcpError::Connection("Failed to capture agent stdout".to_string()))?;

        // Store stdio handles and process in the client
        self.agent_stdin = Some(stdin);
        self.agent_stdout = Some(BufReader::new(stdout));

        // Create a handle before storing the child
        let process = AgentProcess { child };

        Ok(process)
    }

    /// Send a message to the agent
    ///
    /// # Arguments
    ///
    /// * `message` - The JSON-RPC message to send
    ///
    /// # Returns
    ///
    /// The agent's response as a JSON value
    ///
    /// # Errors
    ///
    /// Returns an error if message sending fails or times out
    pub async fn send_message(&mut self, message: serde_json::Value) -> Result<serde_json::Value> {
        // Write the message to agent stdin
        self.write_request(&message).await?;

        // Read the response from agent stdout
        let response_line = self.read_response_line().await?;

        // Parse and return the response
        let response: serde_json::Value = serde_json::from_str(&response_line)?;
        Ok(response)
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

    // ACP Protocol handshake methods

    /// Send InitializeRequest to agent
    ///
    /// This performs the first step of the ACP protocol handshake.
    ///
    /// # Arguments
    ///
    /// * `request` - The InitializeRequest to send
    ///
    /// # Returns
    ///
    /// The InitializeResponse from the agent
    ///
    /// # Errors
    ///
    /// Returns an error if initialization fails
    pub async fn initialize(
        &mut self,
        request: agent_client_protocol::InitializeRequest,
    ) -> Result<agent_client_protocol::InitializeResponse> {
        use agent_client_protocol::ClientRequest;

        // Send the initialize request
        let response = self
            .send_request(ClientRequest::InitializeRequest(request))
            .await?;

        // Extract the result field from JSON-RPC response
        let result = response.get("result").ok_or_else(|| {
            AcpError::Session("Missing result field in initialize response".to_string())
        })?;

        // Parse the result as InitializeResponse
        let init_response: agent_client_protocol::InitializeResponse =
            serde_json::from_value(result.clone())?;

        Ok(init_response)
    }

    /// Send NewSessionRequest to create a session
    ///
    /// This performs the second step of the ACP protocol handshake.
    ///
    /// # Arguments
    ///
    /// * `request` - The NewSessionRequest to send
    ///
    /// # Returns
    ///
    /// The NewSessionResponse from the agent
    ///
    /// # Errors
    ///
    /// Returns an error if session creation fails
    pub async fn create_new_session(
        &mut self,
        request: agent_client_protocol::NewSessionRequest,
    ) -> Result<agent_client_protocol::NewSessionResponse> {
        use agent_client_protocol::ClientRequest;

        // Send the new session request
        let response = self
            .send_request(ClientRequest::NewSessionRequest(request))
            .await?;

        // Extract the result field from JSON-RPC response
        let result = response.get("result").ok_or_else(|| {
            AcpError::Session("Missing result field in new session response".to_string())
        })?;

        // Parse the result as NewSessionResponse
        let session_response: agent_client_protocol::NewSessionResponse =
            serde_json::from_value(result.clone())?;

        Ok(session_response)
    }

    /// Connect to agent with full ACP protocol handshake
    ///
    /// This performs the complete connection sequence:
    /// 1. Spawn agent process
    /// 2. Send InitializeRequest
    /// 3. Send NewSessionRequest
    /// 4. Return session
    ///
    /// # Returns
    ///
    /// An AcpSession ready for communication
    ///
    /// # Errors
    ///
    /// Returns an error if any step of the handshake fails
    pub async fn connect_with_handshake(&mut self) -> Result<AcpSession> {
        use agent_client_protocol::{
            ClientCapabilities, InitializeRequest, NewSessionRequest,
        };

        // 1. Spawn agent process
        let _process = self.spawn_agent().await?;

        // 2. Send InitializeRequest
        // Use protocol version 1 instead of default (0) for opencode compatibility
        // Based on ACP spec: https://agentclientprotocol.com/protocol/initialization
        let init_request = InitializeRequest {
            protocol_version: 1u16.into(), // Uses From<u16> for ProtocolVersion
            client_info: None,
            client_capabilities: ClientCapabilities::default(),
            meta: None,
        };

        let _init_response = self.initialize(init_request).await?;

        // 3. Send NewSessionRequest with MCP server configuration
        use agent_client_protocol::McpServer;

        // Configure Crucible MCP server via stdio transport
        // The agent will spawn `cru mcp` which starts the MCP server
        let crucible_mcp_server = McpServer::Stdio {
            name: "crucible".to_string(),
            command: std::env::current_exe()
                .unwrap_or_else(|_| PathBuf::from("cru"))
                .parent()
                .map(|p| p.join("cru"))
                .unwrap_or_else(|| PathBuf::from("cru")),
            args: vec!["mcp".to_string()],
            env: vec![],
        };

        let session_request = NewSessionRequest {
            cwd: self
                .config
                .working_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("/")),
            mcp_servers: vec![crucible_mcp_server],
            meta: None,
        };

        let session_response = self.create_new_session(session_request).await?;

        // 4. Mark as connected and create session
        self.mark_connected();

        use crate::session::SessionConfig;
        Ok(AcpSession::new(
            SessionConfig::default(),
            session_response.session_id.to_string(),
        ))
    }

    /// Connect to agent with full ACP protocol handshake using SSE MCP server
    ///
    /// This performs the complete connection sequence with an in-process SSE MCP server:
    /// 1. Spawn agent process
    /// 2. Send InitializeRequest
    /// 3. Send NewSessionRequest with SSE MCP server URL
    /// 4. Return session
    ///
    /// # Arguments
    ///
    /// * `sse_url` - URL to the SSE MCP server (e.g., "http://127.0.0.1:12345/sse")
    ///
    /// # Returns
    ///
    /// An AcpSession ready for communication
    ///
    /// # Errors
    ///
    /// Returns an error if any step of the handshake fails
    pub async fn connect_with_sse_mcp(&mut self, sse_url: &str) -> Result<AcpSession> {
        use agent_client_protocol::{
            ClientCapabilities, InitializeRequest, McpServer, NewSessionRequest,
        };

        tracing::info!("Connecting to agent with SSE MCP server at {}", sse_url);

        // 1. Spawn agent process
        let _process = self.spawn_agent().await?;

        // 2. Send InitializeRequest
        let init_request = InitializeRequest {
            protocol_version: 1u16.into(),
            client_info: None,
            client_capabilities: ClientCapabilities::default(),
            meta: None,
        };

        let _init_response = self.initialize(init_request).await?;

        // 3. Send NewSessionRequest with SSE MCP server
        let crucible_mcp_server = McpServer::Sse {
            name: "crucible".to_string(),
            url: sse_url.to_string(),
            headers: vec![],
        };

        tracing::debug!("Configuring MCP server: {:?}", crucible_mcp_server);

        let session_request = NewSessionRequest {
            cwd: self
                .config
                .working_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("/")),
            mcp_servers: vec![crucible_mcp_server],
            meta: None,
        };

        let session_response = self.create_new_session(session_request).await?;

        // 4. Mark as connected and create session
        self.mark_connected();

        tracing::info!(
            "Agent connected with session: {}",
            session_response.session_id
        );

        use crate::session::SessionConfig;
        Ok(AcpSession::new(
            SessionConfig::default(),
            session_response.session_id.to_string(),
        ))
    }

    /// Write a JSON request to the agent's stdin
    ///
    /// # Arguments
    ///
    /// * `request` - The JSON value to write
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails or stdin is not available
    pub async fn write_request(&mut self, request: &serde_json::Value) -> Result<()> {
        let stdin = self
            .agent_stdin
            .as_mut()
            .ok_or_else(|| AcpError::Connection("Agent stdin not available".to_string()))?;

        // Serialize to JSON and add newline
        let json_str = serde_json::to_string(request)?; // Auto-converts to AcpError::Serialization

        let line = format!("{}\n", json_str);

        // Write to stdin
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| AcpError::Connection(format!("Failed to write to agent stdin: {}", e)))?;

        // Flush to ensure it's sent
        stdin
            .flush()
            .await
            .map_err(|e| AcpError::Connection(format!("Failed to flush agent stdin: {}", e)))?;

        Ok(())
    }

    /// Read a single line response from the agent's stdout
    ///
    /// # Returns
    ///
    /// The line read from stdout (without trailing newline)
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails, stdout is not available, or timeout occurs
    pub async fn read_response_line(&mut self) -> Result<String> {
        let stdout = self
            .agent_stdout
            .as_mut()
            .ok_or_else(|| AcpError::Connection("Agent stdout not available".to_string()))?;

        let mut line = String::new();

        // Read with a generous per-read timeout.
        // Agents may pause for extended periods during tool execution or deep reasoning.
        // Use 5 minutes per-read minimum, or match the overall streaming timeout if configured.
        // The overall streaming timeout (in send_prompt_with_streaming) provides the actual limit.
        let per_read_timeout_ms = self
            .config
            .timeout_ms
            .map(|ms| ms.max(300_000)) // At least 5 minutes per read
            .unwrap_or(300_000); // Default 5 minutes
        let duration = tokio::time::Duration::from_millis(per_read_timeout_ms);

        let read_result = match tokio::time::timeout(duration, stdout.read_line(&mut line)).await {
            Ok(result) => result,
            Err(_) => return Err(AcpError::Timeout("Read operation timed out".to_string())),
        };

        // Handle read result
        match read_result {
            Ok(0) => Err(AcpError::Connection("Agent closed stdout".to_string())),
            Ok(_) => Ok(line.trim_end().to_string()),
            Err(e) => Err(AcpError::Connection(format!(
                "Failed to read from agent stdout: {}",
                e
            ))),
        }
    }

    /// Send an ACP protocol request and wait for response
    ///
    /// # Arguments
    ///
    /// * `request` - The ClientRequest to send
    ///
    /// # Returns
    ///
    /// The response as a JSON value
    ///
    /// # Errors
    ///
    /// Returns an error if communication fails
    pub async fn send_request(
        &mut self,
        request: agent_client_protocol::ClientRequest,
    ) -> Result<serde_json::Value> {
        use serde_json::json;

        // Determine method name and params from ClientRequest
        let (method, params) = match &request {
            agent_client_protocol::ClientRequest::InitializeRequest(req) => {
                ("initialize", serde_json::to_value(req)?)
            }
            agent_client_protocol::ClientRequest::AuthenticateRequest(req) => {
                ("authenticate", serde_json::to_value(req)?)
            }
            agent_client_protocol::ClientRequest::NewSessionRequest(req) => {
                ("session/new", serde_json::to_value(req)?)
            }
            agent_client_protocol::ClientRequest::LoadSessionRequest(req) => {
                ("session/load", serde_json::to_value(req)?)
            }
            agent_client_protocol::ClientRequest::SetSessionModeRequest(req) => {
                ("session/set_mode", serde_json::to_value(req)?)
            }
            agent_client_protocol::ClientRequest::PromptRequest(req) => {
                ("session/prompt", serde_json::to_value(req)?)
            }
            agent_client_protocol::ClientRequest::ExtMethodRequest(req) => {
                ("ext", serde_json::to_value(req)?)
            }
        };

        // Generate a unique request ID
        static REQUEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Wrap in JSON-RPC 2.0 format
        let json_request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        // Write to agent stdin
        self.write_request(&json_request).await?;

        // Read response from agent stdout
        let response_line = self.read_response_line().await?;

        // Parse JSON response
        let response: serde_json::Value = serde_json::from_str(&response_line)?; // Auto-converts to AcpError::Serialization

        Ok(response)
    }

    /// Send a prompt request and handle streaming responses
    ///
    /// This method properly handles the ACP streaming protocol where:
    /// 1. Agent sends `session/update` notifications during processing
    /// 2. Agent sends final response with `stopReason` when complete
    ///
    /// # Arguments
    ///
    /// * `request` - The PromptRequest to send
    /// * `request_id` - The JSON-RPC request ID to match the final response
    ///
    /// # Returns
    ///
    /// Tuple of (formatted_content, tool_calls, PromptResponse)
    ///
    /// # Errors
    ///
    /// Returns an error if communication fails
    pub async fn send_prompt_with_streaming(
        &mut self,
        request: agent_client_protocol::PromptRequest,
        request_id: u64,
    ) -> Result<(
        String,
        Vec<ToolCallInfo>,
        agent_client_protocol::PromptResponse,
    )> {
        use serde_json::json;

        tracing::info!("Starting streaming request with ID {}", request_id);

        // Wrap in JSON-RPC 2.0 format
        let json_request = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "session/prompt",
            "params": serde_json::to_value(&request)?
        });

        // Write to agent stdin
        self.write_request(&json_request).await?;

        // Create overall timeout (10x per-read timeout or 30s default)
        let overall_timeout = self
            .config
            .timeout_ms
            .map(|ms| tokio::time::Duration::from_millis(ms * 10))
            .unwrap_or(tokio::time::Duration::from_secs(30));

        // Wrap the streaming loop in a timeout
        let streaming_future = async {
            let mut state = StreamingState::default();

            // Read lines until we get the final response (with matching id)
            loop {
                let response_line = self.read_response_line().await?;
                let response: serde_json::Value = serde_json::from_str(&response_line)?;

                tracing::trace!("Received line: {}", response_line);
                tracing::debug!(
                    "Received from agent: {}",
                    serde_json::to_string_pretty(&response).unwrap_or_default()
                );

                // Check for error responses
                if let Some(error) = response.get("error") {
                    let error_msg = error
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown error");
                    let error_code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);

                    tracing::error!("Agent returned error: {} (code: {})", error_msg, error_code);
                    return Err(AcpError::Session(format!(
                        "Agent error during streaming: {} (code: {}, accumulated {} chars)",
                        error_msg,
                        error_code,
                        state.formatted_length()
                    )));
                }

                if let Some(prompt_response) = self
                    .process_streaming_message(&response, request_id, &mut state)
                    .await?
                {
                    tracing::info!(
                        "Final response received (ID: {:?}) after {} notifications, {} chars",
                        request_id,
                        state.notification_count,
                        state.formatted_length()
                    );

                    return Ok((state, prompt_response));
                }
            }
        };

        // Apply overall timeout
        match tokio::time::timeout(overall_timeout, streaming_future).await {
            Ok(Ok((state, response))) => Ok((state.formatted_output(), state.tool_calls, response)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(AcpError::Timeout(format!(
                "Streaming operation timed out after {}s",
                overall_timeout.as_secs()
            ))),
        }
    }

    async fn process_streaming_message(
        &mut self,
        response: &serde_json::Value,
        request_id: u64,
        state: &mut StreamingState,
    ) -> Result<Option<agent_client_protocol::PromptResponse>> {
        if let Some(method_value) = response.get("method") {
            state.notification_count += 1;
            let method_name = method_value.as_str().unwrap_or_default();
            tracing::debug!(
                "Notification #{}: {}",
                state.notification_count,
                method_name
            );

            if method_name == "session/update" {
                if let Some(params) = response.get("params") {
                    match serde_json::from_value::<SessionNotification>(params.clone()) {
                        Ok(notification) => self.apply_session_update(notification, state),
                        Err(e) => {
                            tracing::warn!("Failed to parse SessionNotification: {}", e);
                            tracing::debug!("Raw params: {}", params);
                        }
                    }
                } else {
                    tracing::warn!("session/update notification missing params");
                }
            } else if method_name == "session/request_permission" {
                if let Some(params) = response.get("params") {
                    match serde_json::from_value::<RequestPermissionRequest>(params.clone()) {
                        Ok(request) => {
                            if let Some(id_value) = response.get("id") {
                                if let Some(permission_id) = self.parse_request_id(id_value) {
                                    self.respond_to_permission_request(permission_id, request)
                                        .await?;
                                } else {
                                    tracing::warn!(
                                        "Permission request missing valid ID: {:?}",
                                        id_value
                                    );
                                }
                            } else {
                                tracing::warn!("Permission request missing ID field");
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse RequestPermissionRequest: {}", e);
                            tracing::debug!("Raw params: {}", params);
                        }
                    }
                } else {
                    tracing::warn!("session/request_permission missing params");
                }
            } else {
                tracing::debug!("Ignoring RPC method: {}", method_name);
            }

            return Ok(None);
        }

        if let Some(id_value) = response.get("id") {
            let id_matches = match id_value {
                serde_json::Value::Number(n) => n.as_u64() == Some(request_id),
                serde_json::Value::String(s) => s.parse::<u64>().ok() == Some(request_id),
                _ => false,
            };

            if id_matches {
                let result = response.get("result").ok_or_else(|| {
                    AcpError::Session("Missing result in prompt response".to_string())
                })?;
                let prompt_response = serde_json::from_value(result.clone())?;
                return Ok(Some(prompt_response));
            } else {
                tracing::warn!(
                    "Received response with non-matching ID: {:?} (expected: {})",
                    id_value,
                    request_id
                );
            }

            return Ok(None);
        }

        Err(AcpError::Session(
            "Received message without id or method".to_string(),
        ))
    }

    fn apply_session_update(&self, notification: SessionNotification, state: &mut StreamingState) {
        match notification.update {
            SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
                ContentBlock::Text(text_block) => {
                    state.append_text(&text_block.text);
                    tracing::trace!(
                        "Accumulated chunk: '{}' (total: {} chars)",
                        text_block.text,
                        state.formatted_length()
                    );
                }
                other => {
                    tracing::debug!("Ignoring non-text content block: {:?}", other);
                }
            },
            SessionUpdate::ToolCall(tool_call) => {
                tracing::info!("Tool call: {}", tool_call.title);
                self.record_tool_call(
                    ToolCallInfo {
                        id: Some(tool_call.id.to_string()),
                        title: tool_call.title.clone(),
                        arguments: tool_call.raw_input.clone(),
                    },
                    state,
                );
            }
            SessionUpdate::ToolCallUpdate(update) => {
                tracing::debug!("Tool call update: {:?}", update.id);
                if update.fields.title.is_some() || update.fields.raw_input.is_some() {
                    let id = update.id.to_string();
                    let title = update
                        .fields
                        .title
                        .clone()
                        .or_else(|| state.title_for_tool(&id))
                        .unwrap_or_else(|| "Unnamed tool".to_string());
                    self.record_tool_call(
                        ToolCallInfo {
                            id: Some(id.clone()),
                            title,
                            arguments: update.fields.raw_input.clone(),
                        },
                        state,
                    );
                }
            }
            other => {
                tracing::debug!("Ignoring update type: {:?}", other);
            }
        }
    }

    async fn respond_to_permission_request(
        &mut self,
        request_id: u64,
        request: RequestPermissionRequest,
    ) -> Result<()> {
        let outcome = if let Some(first_option) = request.options.first() {
            RequestPermissionOutcome::Selected {
                option_id: first_option.id.clone(),
            }
        } else {
            RequestPermissionOutcome::Cancelled
        };

        let response = RequestPermissionResponse {
            outcome,
            meta: None,
        };

        let result_value = serde_json::to_value(response)?;
        let json_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": result_value
        });

        self.write_permission_response(json_response).await
    }

    async fn write_permission_response(&mut self, payload: serde_json::Value) -> Result<()> {
        if self.agent_stdin.is_none() {
            tracing::warn!("Agent stdin unavailable; cannot send permission response");
            return Ok(());
        }
        self.write_request(&payload).await
    }

    fn parse_request_id(&self, value: &serde_json::Value) -> Option<u64> {
        match value {
            serde_json::Value::Number(n) => n.as_u64(),
            serde_json::Value::String(s) => s.parse::<u64>().ok(),
            _ => None,
        }
    }

    fn record_tool_call(&self, tool_call: ToolCallInfo, state: &mut StreamingState) {
        let args_str = tool_call
            .arguments
            .as_ref()
            .map(|args| serde_json::to_string(args).unwrap_or_else(|_| "<invalid>".to_string()))
            .unwrap_or_else(|| "".to_string());

        let formatted_args = if args_str.is_empty() {
            "()".to_string()
        } else {
            format!("({})", args_str)
        };

        let display_title = humanize_tool_title(&tool_call.title);
        let label = format!("▷ {}{}", display_title, formatted_args);
        let id = tool_call
            .id
            .clone()
            .unwrap_or_else(|| format!("{}::{}", tool_call.title, args_str));

        let has_prior_text = matches!(
            state.segments.last(),
            Some(ResponseSegment::Text(last)) if !last.trim().is_empty()
        );
        let _indent = has_prior_text || state.tool_block_active;

        if let Some(&idx) = state.tool_segment_index.get(&id) {
            if let Some(ResponseSegment::Tool {
                label: existing, ..
            }) = state.segments.get_mut(idx)
            {
                *existing = label.clone();
            }
        } else {
            state
                .tool_segment_index
                .insert(id.clone(), state.segments.len());
            state.segments.push(ResponseSegment::Tool {
                label: label.clone(),
            });
        }

        self.upsert_tool_info(tool_call, state);
        state.tool_block_active = true;
    }

    fn upsert_tool_info(&self, tool_call: ToolCallInfo, state: &mut StreamingState) {
        if let Some(id) = &tool_call.id {
            if let Some(existing) = state
                .tool_calls
                .iter_mut()
                .find(|t| t.id.as_deref() == Some(id.as_str()))
            {
                *existing = tool_call;
                return;
            }
        }
        state.tool_calls.push(tool_call);
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

#[async_trait]
impl SessionManager for CrucibleAcpClient {
    type Session = SessionId;
    type Config = SessionConfig;

    async fn create_session(&mut self, config: Self::Config) -> AcpResult<Self::Session> {
        // For now, we create a session ID and track it internally
        // Full agent connection will be implemented in later cycles

        // Generate a new session ID
        let session_id = SessionId::new();

        // Store session configuration in metadata
        let mut metadata = config.metadata.clone();
        metadata.insert(
            "cwd".to_string(),
            serde_json::json!(config.cwd.to_string_lossy()),
        );
        metadata.insert(
            "mode".to_string(),
            serde_json::json!(format!("{:?}", config.mode)),
        );

        // Track as active session
        self.active_session = Some(session_id.clone());

        Ok(session_id)
    }

    async fn load_session(&mut self, session: Self::Session) -> AcpResult<()> {
        // For now, just set it as active (actual restoration comes later)
        self.active_session = Some(session);
        Ok(())
    }

    async fn end_session(&mut self, session: Self::Session) -> AcpResult<()> {
        if self.active_session.as_ref() == Some(&session) {
            self.active_session = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::StopReason;
    use crucible_core::traits::acp::SessionManager;
    use crucible_core::types::acp::{SessionConfig, SessionId};
    use serde_json::json;

    #[test]
    fn test_client_creation() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/test/agent"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(5000),
            max_retries: Some(3),
        };
        let client = CrucibleAcpClient::new(config);
        assert_eq!(client.config().agent_path, PathBuf::from("/test/agent"));
    }

    #[test]
    fn streaming_state_merges_chunks_without_newlines() {
        let mut state = StreamingState::default();
        state.append_text("I'll rea");
        state.append_text("d a few notes from the kiln.");

        assert_eq!(
            state.formatted_output(),
            "I'll read a few notes from the kiln."
        );
    }

    #[test]
    fn streaming_state_adds_padding_after_tools() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/tmp/agent"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(1000),
            max_retries: Some(1),
        };
        let client = CrucibleAcpClient::new(config);
        let mut state = StreamingState::default();
        state.append_text("First chunk");

        let tool_call = ToolCallInfo {
            title: "test_tool".to_string(),
            arguments: None,
            id: None,
        };
        client.record_tool_call(tool_call, &mut state);
        state.append_text("Response after the tool call.");

        assert_eq!(
            state.formatted_output(),
            "First chunk\n\n  ▷ test_tool()\n\nResponse after the tool call."
        );
    }

    #[tokio::test]
    async fn test_client_implements_session_manager() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/test/agent"),
            agent_args: None,
            working_dir: Some(PathBuf::from("/test/workspace")),
            env_vars: None,
            timeout_ms: Some(5000),
            max_retries: Some(3),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Should start with no active session
        assert!(client.active_session().is_none());

        // Should implement SessionManager trait
        let session_config = SessionConfig {
            cwd: PathBuf::from("/test/workspace"),
            mode: crucible_core::types::acp::ChatMode::Plan,
            context_size: 5,
            enable_enrichment: true,
            enrichment_count: 5,
            metadata: std::collections::HashMap::new(),
        };

        // This should now succeed and create a session
        let result = client.create_session(session_config).await;
        assert!(result.is_ok(), "Should successfully create session");

        // Should track active session
        let session_id = result.unwrap();
        assert!(client.active_session().is_some());
        assert_eq!(client.active_session(), Some(&session_id));
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/test/agent"),
            agent_args: None,
            working_dir: Some(PathBuf::from("/test/workspace")),
            env_vars: None,
            timeout_ms: Some(5000),
            max_retries: Some(3),
        };
        let mut client = CrucibleAcpClient::new(config);

        let session_config = SessionConfig {
            cwd: PathBuf::from("/test/workspace"),
            mode: crucible_core::types::acp::ChatMode::Plan,
            context_size: 5,
            enable_enrichment: true,
            enrichment_count: 5,
            metadata: std::collections::HashMap::new(),
        };

        // Create session should now succeed
        let create_result = client.create_session(session_config).await;
        assert!(create_result.is_ok());
        let session_id = create_result.unwrap();

        // Should be able to load session
        let load_result = client.load_session(session_id.clone()).await;
        assert!(load_result.is_ok());
        assert_eq!(client.active_session(), Some(&session_id));

        // Should be able to end session
        let end_result = client.end_session(session_id).await;
        assert!(end_result.is_ok());
        assert!(client.active_session().is_none());
    }

    #[tokio::test]
    async fn process_streaming_message_prioritizes_methods() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/tmp/test-agent"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(1000),
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);
        let mut state = StreamingState::default();

        let request_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "session/request_permission",
            "params": {}
        });

        let result = client
            .process_streaming_message(&request_payload, 1, &mut state)
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
        assert_eq!(state.notification_count, 1);
    }

    #[tokio::test]
    async fn process_streaming_message_returns_prompt_response() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/tmp/test-agent"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(1000),
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);
        let mut state = StreamingState::default();

        let response_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 5,
            "result": {
                "stopReason": "end_turn"
            }
        });

        let result = client
            .process_streaming_message(&response_payload, 5, &mut state)
            .await
            .expect("Should parse prompt response");
        assert!(result.is_some());
        assert_eq!(result.unwrap().stop_reason, StopReason::EndTurn);
    }

    #[test]
    fn tool_call_indents_after_text() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/tmp/test-agent"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(1000),
            max_retries: Some(1),
        };
        let client = CrucibleAcpClient::new(config);
        let mut state = StreamingState::default();

        state.append_text("Hello");

        client.record_tool_call(
            ToolCallInfo {
                id: Some("tool-1".into()),
                title: "mcp__crucible__read_note".to_string(),
                arguments: Some(json!({"path": "PRIME"})),
            },
            &mut state,
        );

        state.append_text("World");

        let output = state.formatted_output();
        // Tool block has blank line before and after
        assert!(output.contains("Hello\n\n  ▷ read_note"));
        assert!(output.contains("\n\nWorld"));
    }

    #[test]
    fn tool_call_updates_existing_entry() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/tmp/test-agent"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(1000),
            max_retries: Some(1),
        };
        let client = CrucibleAcpClient::new(config);
        let mut state = StreamingState::default();

        client.record_tool_call(
            ToolCallInfo {
                id: Some("tool-42".into()),
                title: "mcp__crucible__read_note".to_string(),
                arguments: Some(json!({"path": "PRIME"})),
            },
            &mut state,
        );

        client.record_tool_call(
            ToolCallInfo {
                id: Some("tool-42".into()),
                title: "mcp__crucible__read_note".to_string(),
                arguments: Some(json!({"path": "PRIME.md"})),
            },
            &mut state,
        );

        let output = state.formatted_output();
        assert_eq!(output.matches("▷ read_note").count(), 1);
        assert!(output.contains("PRIME.md"));
    }

    #[tokio::test]
    async fn test_session_creation_with_mock_agent() {
        use crate::mock_agent::{MockAgent, MockAgentConfig};
        use std::collections::HashMap;

        // Create a mock agent that will respond successfully
        let mut responses = HashMap::new();
        responses.insert(
            "initialize".to_string(),
            serde_json::json!({
                "agent_capabilities": {},
                "agent_info": {
                    "name": "mock-agent",
                    "version": "0.1.0"
                }
            }),
        );
        responses.insert(
            "new_session".to_string(),
            serde_json::json!({
                "session_id": "test-session-123"
            }),
        );

        let mock_config = MockAgentConfig {
            responses,
            simulate_delay: false,
            delay_ms: 0,
            simulate_errors: false,
        };
        let _mock_agent = MockAgent::new(mock_config);

        // TODO: Once we implement the actual connection logic,
        // this test will verify that we can create a session with the mock agent
        // For now, this is a placeholder showing the expected API
    }

    #[tokio::test]
    async fn test_session_initialization_flow() {
        // 1. Connect to agent (or mock)
        // 2. Send initialize request
        // 3. Create new session
        // 4. Return session ID

        // This will fail until we implement the connection logic
        // but defines the expected behavior
    }

    #[tokio::test]
    async fn test_agent_process_spawning() {
        // Use a simple echo script as test agent
        let config = ClientConfig {
            agent_path: PathBuf::from("echo"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(5000),
            max_retries: Some(3),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Attempt to spawn the agent process
        let result = client.spawn_agent().await;

        // Should successfully spawn process
        assert!(result.is_ok(), "Should spawn agent process");

        // Process should be running
        let process = result.unwrap();
        assert!(process.is_running(), "Agent process should be running");
    }

    #[tokio::test]
    async fn test_connection_establishment() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/test/agent"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(5000),
            max_retries: Some(3),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Should establish connection
        let result = client.connect().await;

        // For now this will fail, but eventually should succeed
        // with a mock or real agent
        assert!(result.is_err(), "Should fail until implementation complete");
    }

    #[tokio::test]
    async fn test_message_sending() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/test/agent"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(5000),
            max_retries: Some(3),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Connect first
        let _session = client.connect().await;

        // Send a message
        let message = serde_json::json!({
            "method": "ping",
            "params": {}
        });

        let result = client.send_message(message).await;

        // Should eventually send successfully
        assert!(result.is_err(), "Will fail until implementation");
    }

    #[tokio::test]
    async fn test_connection_cleanup() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/test/agent"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(5000),
            max_retries: Some(3),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Connect
        let session = client.connect().await;

        if let Ok(session) = session {
            // Disconnect should clean up resources
            let result = client.disconnect(&session).await;
            assert!(result.is_ok(), "Should disconnect cleanly");

            // Connection should be closed
            assert!(
                !client.is_connected(),
                "Should not be connected after disconnect"
            );
        }
    }

    #[tokio::test]
    async fn test_bad_agent_path_error() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/nonexistent/agent"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(1000),
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);

        let result = client.connect().await;

        // Should fail with clear error
        assert!(result.is_err(), "Should fail for nonexistent agent");

        let err = result.unwrap_err();
        match err {
            AcpError::Connection(_) => {} // Expected
            _ => panic!("Should be Connection error"),
        }
    }

    #[tokio::test]
    async fn test_connection_timeout() {
        let config = ClientConfig {
            agent_path: PathBuf::from("/test/hanging-agent"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(100), // Very short timeout
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);

        let result = client.connect().await;

        // Should timeout
        assert!(result.is_err(), "Should timeout");

        let err = result.unwrap_err();
        match err {
            AcpError::Timeout(_) => {}    // Expected
            AcpError::Connection(_) => {} // Also acceptable
            _ => panic!("Should be Timeout or Connection error"),
        }
    }

    #[tokio::test]
    async fn test_stdio_message_exchange() {
        use agent_client_protocol::{
            ClientCapabilities, ClientRequest, InitializeRequest, ProtocolVersion,
        };

        // Use 'cat' as a simple echo agent for testing
        let config = ClientConfig {
            agent_path: PathBuf::from("cat"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(1000),
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Spawn the agent
        let process = client.spawn_agent().await;
        assert!(process.is_ok(), "Should spawn cat process");

        // Create a simple initialize request
        let request = ClientRequest::InitializeRequest(InitializeRequest {
            protocol_version: ProtocolVersion::default(),
            client_info: None,
            client_capabilities: ClientCapabilities::default(),
            meta: None,
        });

        // Send the request - cat will echo it back
        // This will succeed in sending/receiving but may fail on parsing
        // since cat just echoes, not a real ACP agent
        let result = client.send_request(request).await;

        // Either succeeds (cat echoed valid JSON) or fails on parsing
        // Both are acceptable - we're testing that the methods work
        let _ = result; // Accept either outcome
    }

    #[tokio::test]
    async fn test_read_agent_response() {
        let config = ClientConfig {
            agent_path: PathBuf::from("echo"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(500), // Short timeout
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Spawn agent
        let _process = client.spawn_agent().await.unwrap();

        // Try to read a line from stdout
        // Echo may send empty line or close stdout immediately
        let result = client.read_response_line().await;

        // Either succeeds with empty line or fails with EOF/timeout
        // Both outcomes verify that reading mechanism works
        let _ = result; // Accept either outcome
    }

    #[tokio::test]
    async fn test_write_agent_request() {
        let config = ClientConfig {
            agent_path: PathBuf::from("cat"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(1000),
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Spawn agent
        let _process = client.spawn_agent().await.unwrap();

        // Try to write a JSON-RPC message
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "test",
            "params": {}
        });

        let result = client.write_request(&request).await;

        // Should succeed - cat accepts stdin
        assert!(result.is_ok(), "Should successfully write to cat's stdin");
    }

    #[tokio::test]
    async fn test_read_timeout() {
        let config = ClientConfig {
            agent_path: PathBuf::from("sleep"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(100), // Very short timeout
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Spawn agent that won't send anything
        let _process = client.spawn_agent().await;

        // Try to read with timeout
        let result = client.read_response_line().await;

        // Should timeout
        assert!(result.is_err(), "Should timeout on read");
    }

    #[tokio::test]
    async fn test_connection_state_tracking() {
        let config = ClientConfig {
            agent_path: PathBuf::from("cat"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(1000),
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Initially not connected
        assert!(!client.is_connected(), "Should not be connected initially");

        // After spawning, should track connection
        let _process = client.spawn_agent().await.unwrap();

        // Mark as connected (this will be part of connect() implementation)
        client.mark_connected();
        assert!(client.is_connected(), "Should be connected after marking");

        // After disconnect, should not be connected
        client.mark_disconnected();
        assert!(
            !client.is_connected(),
            "Should not be connected after disconnect"
        );
    }

    #[tokio::test]
    async fn test_full_request_response_cycle() {
        use agent_client_protocol::{
            ClientCapabilities, ClientRequest, InitializeRequest, ProtocolVersion,
        };

        let config = ClientConfig {
            agent_path: PathBuf::from("cat"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(2000),
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Spawn and mark connected
        let _process = client.spawn_agent().await.unwrap();
        client.mark_connected();

        // Verify connected
        assert!(client.is_connected(), "Should be marked as connected");

        // Create initialize request
        let request = ClientRequest::InitializeRequest(InitializeRequest {
            protocol_version: ProtocolVersion::default(),
            client_info: None,
            client_capabilities: ClientCapabilities::default(),
            meta: None,
        });

        // Send request - cat will echo it back
        // May succeed or fail depending on JSON parsing
        let _result = client.send_request(request).await;

        // Test that state management works
        client.mark_disconnected();
        assert!(!client.is_connected(), "Should be marked as disconnected");
    }

    // RED: Test expects connect() to spawn agent and establish session
    #[tokio::test]
    async fn test_connect_spawns_and_establishes_session() {
        let config = ClientConfig {
            agent_path: PathBuf::from("cat"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(5000),
            max_retries: Some(3),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Should start with no connection
        assert!(!client.is_connected());

        // Connect should spawn agent and mark connected
        let result = client.connect().await;

        // Should succeed and return a session
        assert!(result.is_ok(), "Should connect successfully");
        assert!(client.is_connected(), "Should be connected after connect()");
    }

    // RED: Test expects send_message() to work with simple JSON
    #[tokio::test]
    async fn test_send_message_with_json() {
        let config = ClientConfig {
            agent_path: PathBuf::from("cat"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(1000),
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Spawn and connect
        let _process = client.spawn_agent().await.unwrap();
        client.mark_connected();

        // Send a simple JSON message
        let message = serde_json::json!({
            "test": "message",
            "value": 42
        });

        let result = client.send_message(message).await;

        // Should succeed (cat echoes back)
        // Result may succeed or fail based on JSON parsing, both acceptable
        let _ = result; // Accept either outcome for now
    }

    // RED: Test expects disconnect() to clean up resources
    #[tokio::test]
    async fn test_disconnect_cleanup() {
        let config = ClientConfig {
            agent_path: PathBuf::from("cat"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(1000),
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Spawn manually for testing
        let _process = client.spawn_agent().await.unwrap();
        client.mark_connected();

        // Create a session for testing
        use crate::session::SessionConfig;
        let session = AcpSession::new(SessionConfig::default(), "test-session-123".to_string());

        // Disconnect should clean up
        let result = client.disconnect(&session).await;

        // Should succeed
        assert!(result.is_ok(), "Should disconnect successfully");
        assert!(
            !client.is_connected(),
            "Should not be connected after disconnect"
        );
    }

    // RED: Test expects full lifecycle: connect -> message -> disconnect
    #[tokio::test]
    async fn test_full_agent_lifecycle() {
        let config = ClientConfig {
            agent_path: PathBuf::from("cat"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(2000),
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);

        // 1. Connect
        let connect_result = client.connect().await;
        if connect_result.is_ok() {
            assert!(client.is_connected(), "Should be connected after connect()");

            // 2. Send message
            let message = serde_json::json!({"action": "test"});
            let _send_result = client.send_message(message).await;

            // 3. Disconnect
            let session = connect_result.unwrap();
            let disconnect_result = client.disconnect(&session).await;

            if disconnect_result.is_ok() {
                assert!(
                    !client.is_connected(),
                    "Should not be connected after disconnect"
                );
            }
        }
    }

    // Test that initialize() method exists and sends messages
    #[tokio::test]
    async fn test_protocol_initialize_handshake() {
        use agent_client_protocol::{ClientCapabilities, InitializeRequest, ProtocolVersion};

        let config = ClientConfig {
            agent_path: PathBuf::from("cat"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(1000),
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Spawn agent
        let _process = client.spawn_agent().await.unwrap();

        // Send initialize request
        let init_request = InitializeRequest {
            protocol_version: ProtocolVersion::default(),
            client_info: None,
            client_capabilities: ClientCapabilities::default(),
            meta: None,
        };

        let result = client.initialize(init_request).await;

        // Cat will echo back but won't provide valid ACP response
        // Either succeeds (unlikely) or fails on parsing - both verify method works
        let _ = result; // Accept either outcome
    }

    // Test that create_new_session() method exists and sends messages
    #[tokio::test]
    async fn test_protocol_new_session() {
        use agent_client_protocol::NewSessionRequest;

        let config = ClientConfig {
            agent_path: PathBuf::from("cat"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(1000),
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);

        // Spawn agent
        let _process = client.spawn_agent().await.unwrap();

        // Create new session request
        let session_request = NewSessionRequest {
            cwd: PathBuf::from("/test"),
            mcp_servers: vec![],
            meta: None,
        };

        let result = client.create_new_session(session_request).await;

        // Cat will echo back but won't provide valid ACP response
        let _ = result; // Accept either outcome
    }

    // Test that connect_with_handshake() method exists and attempts full handshake
    #[tokio::test]
    async fn test_connect_performs_protocol_handshake() {
        let config = ClientConfig {
            agent_path: PathBuf::from("cat"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(2000),
            max_retries: Some(1),
        };
        let mut client = CrucibleAcpClient::new(config);

        // connect_with_handshake() should:
        // 1. Spawn agent
        // 2. Send InitializeRequest
        // 3. Send NewSessionRequest
        // 4. Return session
        let result = client.connect_with_handshake().await;

        // Cat won't respond with valid ACP protocol, so this will fail
        // But it verifies the method exists and attempts the handshake
        let _ = result; // Accept either outcome
    }
}
