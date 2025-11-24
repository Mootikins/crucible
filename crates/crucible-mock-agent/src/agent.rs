//! Mock ACP agent implementation
//!
//! Provides a configurable mock agent that handles the full ACP protocol,
//! including streaming responses via `session/update` notifications.

use std::io::{self, BufRead, Write};
use serde_json::{json, Value};

use agent_client_protocol::{
    InitializeResponse, NewSessionResponse, PromptResponse,
    ProtocolVersion, AgentCapabilities, AuthMethod, AuthMethodId, Implementation,
    SessionId, StopReason,
};

use crate::behaviors::AgentBehavior;
use crate::streaming;

/// Configuration for the mock agent
#[derive(Debug, Clone)]
pub struct MockAgentConfig {
    /// Agent behavior profile
    pub behavior: AgentBehavior,

    /// Protocol version to advertise
    pub protocol_version: u16,

    /// Whether to require authentication
    pub requires_auth: bool,

    /// Delay in milliseconds before responding
    pub response_delay_ms: Option<u64>,

    /// Whether to inject errors
    pub inject_errors: bool,

    /// Custom capabilities to advertise
    pub capabilities: Vec<String>,

    /// Number of chunks to send for streaming responses
    pub streaming_chunk_count: usize,

    /// Delay between streaming chunks in milliseconds
    pub streaming_chunk_delay_ms: Option<u64>,
}

impl Default for MockAgentConfig {
    fn default() -> Self {
        Self {
            behavior: AgentBehavior::default(),
            protocol_version: 1,
            requires_auth: false,
            response_delay_ms: None,
            inject_errors: false,
            capabilities: vec![
                "fs.readTextFile".to_string(),
                "fs.writeTextFile".to_string(),
                "terminal".to_string(),
            ],
            streaming_chunk_count: 4,
            streaming_chunk_delay_ms: Some(10),
        }
    }
}

impl MockAgentConfig {
    /// Create configuration for OpenCode-compatible agent
    pub fn opencode() -> Self {
        Self {
            behavior: AgentBehavior::OpenCode,
            ..Default::default()
        }
    }

    /// Create configuration for Claude-ACP-compatible agent
    pub fn claude_acp() -> Self {
        Self {
            behavior: AgentBehavior::ClaudeAcp,
            requires_auth: true,
            capabilities: vec![
                "fs.readTextFile".to_string(),
                "fs.writeTextFile".to_string(),
                "terminal".to_string(),
                "loadSession".to_string(),
            ],
            ..Default::default()
        }
    }

    /// Create configuration for Gemini-compatible agent
    pub fn gemini() -> Self {
        Self {
            behavior: AgentBehavior::Gemini,
            capabilities: vec![
                "fs.readTextFile".to_string(),
                "fs.writeTextFile".to_string(),
            ],
            ..Default::default()
        }
    }

    /// Create configuration for Codex-compatible agent
    pub fn codex() -> Self {
        Self {
            behavior: AgentBehavior::Codex,
            ..Default::default()
        }
    }

    /// Create configuration for streaming agent
    pub fn streaming() -> Self {
        Self {
            behavior: AgentBehavior::Streaming,
            ..Default::default()
        }
    }

    /// Create configuration for slow streaming (timeout testing)
    pub fn streaming_slow() -> Self {
        Self {
            behavior: AgentBehavior::StreamingSlow,
            streaming_chunk_delay_ms: Some(500),
            ..Default::default()
        }
    }

    /// Create configuration for incomplete streaming (hang detection)
    pub fn streaming_incomplete() -> Self {
        Self {
            behavior: AgentBehavior::StreamingIncomplete,
            ..Default::default()
        }
    }
}

/// Mock ACP agent
///
/// Reads JSON-RPC messages from stdin and writes responses to stdout,
/// simulating a real ACP agent for integration testing.
pub struct MockAgent {
    config: MockAgentConfig,
    session_id: Option<String>,
}

impl MockAgent {
    /// Create a new mock agent
    pub fn new(config: MockAgentConfig) -> Self {
        Self {
            config,
            session_id: None,
        }
    }

    /// Run the mock agent, reading from stdin and writing to stdout
    ///
    /// This is the main entry point for the mock agent process.
    pub fn run(&mut self) -> io::Result<()> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            // Parse the JSON-RPC request
            let request: Value = match serde_json::from_str(&line) {
                Ok(req) => req,
                Err(e) => {
                    eprintln!("Failed to parse request: {}", e);
                    continue;
                }
            };

            // Simulate delay if configured
            if let Some(delay_ms) = self.config.response_delay_ms {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }

            // Handle the request
            // For streaming behaviors on session/prompt, send multiple messages
            let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

            if method == "session/prompt" && self.config.behavior.is_streaming() {
                // Send streaming response
                self.handle_streaming_prompt(&request, &mut stdout)?;
            } else {
                // Normal single-response handling
                let response = self.handle_request(&request);
                writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
                stdout.flush()?;
            }
        }

        Ok(())
    }

    /// Handle a JSON-RPC request and generate appropriate response
    fn handle_request(&mut self, request: &Value) -> Value {
        let method = request.get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown");

        match method {
            "initialize" => self.handle_initialize(request),
            "session/new" => self.handle_new_session(request),
            "session/prompt" => self.handle_prompt(request),
            "authenticate" => self.handle_authenticate(request),
            _ => self.error_response(request, -32601, "Method not found"),
        }
    }

    /// Handle streaming prompt request
    ///
    /// Sends multiple `session/update` notifications followed by final response
    fn handle_streaming_prompt(&self, request: &Value, stdout: &mut io::Stdout) -> io::Result<()> {
        if self.config.inject_errors {
            let error = self.error_response(request, -32000, "Simulated prompt error");
            writeln!(stdout, "{}", serde_json::to_string(&error)?)?;
            stdout.flush()?;
            return Ok(());
        }

        let session_id = self.session_id.as_deref().unwrap_or("unknown-session");

        // Default chunks
        let chunks = vec!["The", " answer", " is", " 4"];

        streaming::send_streaming_response(
            request,
            session_id,
            &chunks,
            self.config.streaming_chunk_delay_ms,
            self.config.behavior.sends_final_response(),
            stdout,
        )
    }

    /// Handle initialize request
    fn handle_initialize(&self, request: &Value) -> Value {
        if self.config.inject_errors {
            return self.error_response(request, -32000, "Simulated initialization error");
        }

        // Build agent info based on behavior profile
        let (name, version) = match self.config.behavior {
            AgentBehavior::OpenCode => ("mock-opencode", "1.0.0"),
            AgentBehavior::ClaudeAcp => ("mock-claude-acp", "1.0.0"),
            AgentBehavior::Gemini => ("mock-gemini", "1.0.0"),
            AgentBehavior::Codex => ("mock-codex", "1.0.0"),
            AgentBehavior::Streaming |
            AgentBehavior::StreamingSlow |
            AgentBehavior::StreamingIncomplete => ("mock-streaming", "1.0.0"),
            AgentBehavior::Custom(_) => ("mock-custom", "1.0.0"),
        };

        // Determine auth methods based on behavior
        let auth_methods = if self.config.requires_auth {
            vec![AuthMethod {
                id: AuthMethodId("api_key".into()),
                name: "API Key".to_string(),
                description: Some("Authenticate using an API key".to_string()),
                meta: None,
            }]
        } else {
            vec![]
        };

        // Construct proper InitializeResponse using ACP types
        let response = InitializeResponse {
            protocol_version: ProtocolVersion::from(self.config.protocol_version),
            agent_capabilities: AgentCapabilities::default(),
            auth_methods,
            agent_info: Some(Implementation {
                name: name.to_string(),
                version: version.to_string(),
                title: None,
            }),
            meta: None,
        };

        // Serialize to JSON value
        let mut result = serde_json::to_value(&response).unwrap();

        // Add custom MCP capabilities (non-standard extension for testing)
        let mut capabilities_map = serde_json::Map::new();
        for cap in &self.config.capabilities {
            capabilities_map.insert(cap.clone(), json!({}));
        }

        if let Some(agent_caps) = result.get_mut("agentCapabilities") {
            agent_caps["mcpCapabilities"] = json!(capabilities_map);
        }

        // Wrap in JSON-RPC 2.0 response format
        json!({
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "result": result
        })
    }

    /// Handle new session request
    fn handle_new_session(&mut self, request: &Value) -> Value {
        if self.config.inject_errors {
            return self.error_response(request, -32000, "Simulated session creation error");
        }

        // Generate a session ID
        use std::sync::atomic::{AtomicU64, Ordering};
        static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);
        let session_num = SESSION_COUNTER.fetch_add(1, Ordering::SeqCst);
        let session_id = format!("mock-session-{}", session_num);
        self.session_id = Some(session_id.clone());

        // Construct proper NewSessionResponse using ACP types
        let response = NewSessionResponse {
            session_id: SessionId::from(session_id),
            modes: None,
            meta: None,
        };

        // Serialize response to JSON value
        let result = serde_json::to_value(&response).unwrap();

        // Wrap in JSON-RPC 2.0 response format
        json!({
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "result": result
        })
    }

    /// Handle prompt request (non-streaming)
    fn handle_prompt(&self, request: &Value) -> Value {
        if self.config.inject_errors {
            return self.error_response(request, -32000, "Simulated prompt error");
        }

        // Construct proper PromptResponse using ACP types
        let response = PromptResponse {
            stop_reason: StopReason::EndTurn,
            meta: None,
        };

        // Wrap in JSON-RPC 2.0 response format
        json!({
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "result": response
        })
    }

    /// Handle authentication request
    fn handle_authenticate(&self, request: &Value) -> Value {
        if self.config.inject_errors {
            return self.error_response(request, -32000, "Simulated authentication error");
        }

        // Mock authentication success
        json!({
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "result": {
                "authenticated": true
            }
        })
    }

    /// Generate an error response
    fn error_response(&self, request: &Value, code: i32, message: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "error": {
                "code": code,
                "message": message
            }
        })
    }
}
