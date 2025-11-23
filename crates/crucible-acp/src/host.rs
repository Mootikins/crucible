//! ACP Host implementation for serving agents
//!
//! This module provides the host/server side of the Agent Client Protocol.
//! Crucible acts as an ACP host that spawns agent processes and serves them
//! with tools, context, and capabilities.
//!
//! ## Architecture
//!
//! ```text
//! User runs: cru chat "message"
//!   ↓
//! Crucible spawns agent (claude-code/codex) as child process
//!   ↓
//! Crucible listens on stdio as ACP HOST
//!   ↓
//! Agent (child) connects to Crucible via stdio
//!   ↓
//! Agent → Crucible: InitializeRequest
//! Crucible → Agent: InitializeResponse (with server info, tools)
//!   ↓
//! Agent → Crucible: NewSessionRequest
//! Crucible → Agent: NewSessionResponse (with session ID)
//!   ↓
//! Agent → Crucible: sampling/create_message
//! Crucible → Agent: response with tool results
//! ```
//!
//! ## Responsibilities
//!
//! - Agent process lifecycle management (spawn, monitor, cleanup)
//! - Protocol request handling (initialize, new_session, sampling)
//! - Tool discovery and execution
//! - Context enrichment and retrieval
//! - Session state management
//!
//! ## Design Principles
//!
//! - **Single Responsibility**: Focused on ACP protocol hosting
//! - **Dependency Inversion**: Uses tool registry abstraction
//! - **Open/Closed**: New request types can be added without modifying core

use std::path::PathBuf;
use std::process::Stdio;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::process::{Command, Child, ChildStdin, ChildStdout};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use agent_client_protocol::{
    ClientRequest, InitializeRequest, InitializeResponse, NewSessionRequest,
    NewSessionResponse, ProtocolVersion, ServerInfo, ServerCapabilities,
    Tool as AcpTool, ToolDescription
};

use crate::{AcpError, Result};
use crate::tools::{ToolRegistry, ToolExecutor, get_crucible_system_prompt};

/// Configuration for the ACP host
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostConfig {
    /// Path to the agent executable or script
    pub agent_path: PathBuf,

    /// Working directory for the agent process
    pub working_dir: Option<PathBuf>,

    /// Environment variables to pass to the agent
    pub env_vars: Option<Vec<(String, String)>>,

    /// Timeout for agent operations (in milliseconds)
    pub timeout_ms: Option<u64>,

    /// Path to the kiln directory (for tool initialization)
    pub kiln_path: PathBuf,

    /// Enable read-only mode (no write operations)
    pub read_only: bool,
}

/// ACP Host state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostState {
    /// Not yet initialized
    Uninitialized,
    /// Agent process spawned, waiting for initialize request
    Spawned,
    /// Initialized, waiting for session request
    Initialized,
    /// Session active
    Active,
    /// Disconnected
    Disconnected,
}

/// Main host for ACP communication
///
/// This struct manages the lifecycle of agent processes and provides
/// the server-side ACP protocol implementation.
pub struct CrucibleAcpHost {
    config: HostConfig,
    state: HostState,
    /// Agent process handle
    agent_process: Option<Child>,
    /// Agent stdin for sending responses
    agent_stdin: Option<ChildStdin>,
    /// Agent stdout for reading requests
    agent_stdout: Option<BufReader<ChildStdout>>,
    /// Tool registry
    tool_registry: ToolRegistry,
    /// Tool executor
    tool_executor: Option<ToolExecutor>,
    /// Current session ID
    session_id: Option<String>,
    /// Protocol version negotiated
    protocol_version: Option<ProtocolVersion>,
}

// Manual Debug implementation since Child doesn't implement Debug
impl std::fmt::Debug for CrucibleAcpHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrucibleAcpHost")
            .field("config", &self.config)
            .field("state", &self.state)
            .field("agent_process", &self.agent_process.is_some())
            .field("agent_stdin", &self.agent_stdin.is_some())
            .field("agent_stdout", &self.agent_stdout.is_some())
            .field("session_id", &self.session_id)
            .field("protocol_version", &self.protocol_version)
            .finish()
    }
}

impl CrucibleAcpHost {
    /// Create a new ACP host with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Host configuration
    pub fn new(config: HostConfig) -> Result<Self> {
        let tool_registry = ToolRegistry::new();

        Ok(Self {
            config,
            state: HostState::Uninitialized,
            agent_process: None,
            agent_stdin: None,
            agent_stdout: None,
            tool_registry,
            tool_executor: None,
            session_id: None,
            protocol_version: None,
        })
    }

    /// Initialize tools from the kiln
    pub fn initialize_tools(&mut self) -> Result<()> {
        let kiln_path = self.config.kiln_path.to_str()
            .ok_or_else(|| AcpError::Config("Invalid kiln path".to_string()))?;

        let count = crate::tools::discover_crucible_tools(&mut self.tool_registry, kiln_path)?;
        tracing::info!("Registered {} tools for ACP host", count);

        self.tool_executor = Some(ToolExecutor::new(self.config.kiln_path.clone()));
        Ok(())
    }

    /// Spawn the agent process
    ///
    /// # Returns
    ///
    /// The spawned child process handle
    ///
    /// # Errors
    ///
    /// Returns an error if the agent process cannot be started
    pub async fn spawn_agent(&mut self) -> Result<()> {
        tracing::info!("Spawning agent process: {}", self.config.agent_path.display());

        let mut command = Command::new(&self.config.agent_path);

        // Set working directory if specified
        if let Some(working_dir) = &self.config.working_dir {
            command.current_dir(working_dir);
        }

        // Set environment variables if specified
        if let Some(env_vars) = &self.config.env_vars {
            for (key, value) in env_vars {
                command.env(key, value);
            }
        }

        // Configure stdio for bidirectional communication
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::inherit()); // Let agent errors show in our stderr

        // Spawn the process
        let mut child = command.spawn()
            .map_err(|e| AcpError::Connection(format!("Failed to spawn agent: {}", e)))?;

        // Capture stdin/stdout handles
        let stdin = child.stdin.take()
            .ok_or_else(|| AcpError::Connection("Failed to capture agent stdin".to_string()))?;
        let stdout = child.stdout.take()
            .ok_or_else(|| AcpError::Connection("Failed to capture agent stdout".to_string()))?;

        self.agent_stdin = Some(stdin);
        self.agent_stdout = Some(BufReader::new(stdout));
        self.agent_process = Some(child);
        self.state = HostState::Spawned;

        tracing::info!("Agent process spawned successfully");
        Ok(())
    }

    /// Read a request from the agent
    ///
    /// # Returns
    ///
    /// The parsed JSON-RPC request from the agent
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails or JSON is invalid
    pub async fn read_request(&mut self) -> Result<Value> {
        let stdout = self.agent_stdout.as_mut()
            .ok_or_else(|| AcpError::Connection("Agent stdout not available".to_string()))?;

        let mut line = String::new();
        let bytes_read = stdout.read_line(&mut line).await
            .map_err(|e| AcpError::Connection(format!("Failed to read from agent: {}", e)))?;

        if bytes_read == 0 {
            return Err(AcpError::Connection("Agent disconnected (EOF)".to_string()));
        }

        tracing::debug!("Received from agent: {}", line.trim());

        serde_json::from_str(&line)
            .map_err(|e| AcpError::Protocol(format!("Invalid JSON from agent: {}", e)))
    }

    /// Send a response to the agent
    ///
    /// # Arguments
    ///
    /// * `response` - The JSON-RPC response to send
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails
    pub async fn send_response(&mut self, response: Value) -> Result<()> {
        let stdin = self.agent_stdin.as_mut()
            .ok_or_else(|| AcpError::Connection("Agent stdin not available".to_string()))?;

        let mut response_str = serde_json::to_string(&response)
            .map_err(|e| AcpError::Protocol(format!("Failed to serialize response: {}", e)))?;
        response_str.push('\n');

        tracing::debug!("Sending to agent: {}", response_str.trim());

        stdin.write_all(response_str.as_bytes()).await
            .map_err(|e| AcpError::Connection(format!("Failed to write to agent: {}", e)))?;

        stdin.flush().await
            .map_err(|e| AcpError::Connection(format!("Failed to flush agent stdin: {}", e)))?;

        Ok(())
    }

    /// Handle an initialize request from the agent
    ///
    /// # Arguments
    ///
    /// * `request` - The initialize request
    /// * `id` - The JSON-RPC request ID
    ///
    /// # Returns
    ///
    /// The JSON-RPC response to send
    pub fn handle_initialize(&mut self, request: InitializeRequest, id: Value) -> Result<Value> {
        tracing::info!("Handling initialize request from agent");
        tracing::debug!("Agent protocol version: {:?}", request.protocol_version);
        tracing::debug!("Agent client info: {:?}", request.client_info);

        // Store negotiated protocol version
        self.protocol_version = Some(request.protocol_version.clone());

        // Build server capabilities
        let capabilities = ServerCapabilities {
            tools: Some(true),
            prompts: Some(false),
            resources: Some(false),
        };

        // Convert tools from registry to ACP tool format
        let tools: Vec<AcpTool> = self.tool_registry.tools()
            .iter()
            .map(|tool| AcpTool {
                name: tool.name.clone(),
                description: ToolDescription::String(tool.description.clone()),
                input_schema: tool.input_schema.clone(),
            })
            .collect();

        // Build server info
        let server_info = ServerInfo {
            name: "Crucible".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        // Build initialize response
        let response = InitializeResponse {
            protocol_version: request.protocol_version,
            server_info,
            capabilities,
            tools: Some(tools),
            prompts: None,
            resources: None,
            instructions: Some(get_crucible_system_prompt()),
            meta: None,
        };

        self.state = HostState::Initialized;
        tracing::info!("Agent initialized successfully");

        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": response
        }))
    }

    /// Handle a new session request from the agent
    ///
    /// # Arguments
    ///
    /// * `request` - The new session request
    /// * `id` - The JSON-RPC request ID
    ///
    /// # Returns
    ///
    /// The JSON-RPC response to send
    pub fn handle_new_session(&mut self, request: NewSessionRequest, id: Value) -> Result<Value> {
        tracing::info!("Handling new session request from agent");
        tracing::debug!("Working directory: {}", request.cwd.display());
        tracing::debug!("MCP servers: {:?}", request.mcp_servers);

        // Generate session ID
        let session_id = format!("session-{}-{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            rand::random::<u32>()
        );

        self.session_id = Some(session_id.clone());
        self.state = HostState::Active;

        let response = NewSessionResponse {
            session_id,
            meta: None,
        };

        tracing::info!("Session created successfully");

        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": response
        }))
    }

    /// Main event loop for handling agent requests
    ///
    /// This runs continuously, reading requests from the agent and sending responses.
    ///
    /// # Errors
    ///
    /// Returns an error if communication fails or agent disconnects
    pub async fn run(&mut self) -> Result<()> {
        tracing::info!("Starting ACP host event loop");

        loop {
            // Read request from agent
            let request_json = self.read_request().await?;

            // Parse JSON-RPC request
            let method = request_json.get("method")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcpError::Protocol("Missing method in request".to_string()))?;

            let params = request_json.get("params")
                .ok_or_else(|| AcpError::Protocol("Missing params in request".to_string()))?;

            let id = request_json.get("id")
                .ok_or_else(|| AcpError::Protocol("Missing id in request".to_string()))?
                .clone();

            tracing::info!("Received request: method={}", method);

            // Route to appropriate handler
            let response = match method {
                "initialize" => {
                    let req: InitializeRequest = serde_json::from_value(params.clone())
                        .map_err(|e| AcpError::Protocol(format!("Invalid initialize params: {}", e)))?;
                    self.handle_initialize(req, id)?
                }
                "new_session" => {
                    let req: NewSessionRequest = serde_json::from_value(params.clone())
                        .map_err(|e| AcpError::Protocol(format!("Invalid new_session params: {}", e)))?;
                    self.handle_new_session(req, id)?
                }
                "sampling/create_message" => {
                    // TODO: Implement sampling request handling
                    tracing::warn!("sampling/create_message not yet implemented");
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32601,
                            "message": "Method not implemented"
                        }
                    })
                }
                "tool_call" => {
                    // TODO: Implement tool call handling
                    tracing::warn!("tool_call not yet implemented");
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32601,
                            "message": "Method not implemented"
                        }
                    })
                }
                "ping" => {
                    // Simple ping/pong for keepalive
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": "pong"
                    })
                }
                _ => {
                    tracing::warn!("Unknown method: {}", method);
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32601,
                            "message": format!("Unknown method: {}", method)
                        }
                    })
                }
            };

            // Send response
            self.send_response(response).await?;
        }
    }

    /// Start the host: spawn agent, initialize tools, and enter event loop
    ///
    /// This is the main entry point for running the ACP host.
    ///
    /// # Errors
    ///
    /// Returns an error if spawning or initialization fails
    pub async fn start(&mut self) -> Result<()> {
        // Initialize tools first
        self.initialize_tools()?;

        // Spawn agent process
        self.spawn_agent().await?;

        // Enter event loop
        self.run().await
    }

    /// Get the current state
    pub fn state(&self) -> &HostState {
        &self.state
    }

    /// Get the current session ID
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Shutdown the host gracefully
    pub async fn shutdown(&mut self) -> Result<()> {
        tracing::info!("Shutting down ACP host");

        if let Some(mut process) = self.agent_process.take() {
            // Try graceful shutdown first
            if let Err(e) = process.kill().await {
                tracing::warn!("Failed to kill agent process: {}", e);
            }
        }

        self.state = HostState::Disconnected;
        self.session_id = None;
        self.agent_stdin = None;
        self.agent_stdout = None;

        Ok(())
    }
}

impl Drop for CrucibleAcpHost {
    fn drop(&mut self) {
        if let Some(mut process) = self.agent_process.take() {
            // Try to kill the process on drop
            let _ = process.start_kill();
        }
    }
}
