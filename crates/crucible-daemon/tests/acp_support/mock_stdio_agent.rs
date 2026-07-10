#![allow(unused)]

//! Mock stdio-based ACP agent for integration testing
//!
//! This module provides a mock agent that communicates via stdin/stdout,
//! allowing integration tests to simulate real agent behavior without
//! requiring actual agent binaries.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};

// Import ACP protocol types for proper response construction
use agent_client_protocol::{AuthMethod, InitializeResponse, NewSessionResponse, PromptResponse};

/// Defines the behavior profile of a mock agent
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentBehavior {
    /// OpenCode-compatible agent
    OpenCode,
    /// Claude-ACP-compatible agent (requires auth)
    ClaudeAcp,
    /// Gemini-compatible agent
    Gemini,
    /// Codex-compatible agent
    Codex,
    /// Custom behavior with specific responses
    Custom(HashMap<String, Value>),
}

/// Configuration for the mock stdio agent
#[derive(Debug, Clone)]
pub struct MockStdioAgentConfig {
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
    /// Custom capabilities to advertise (legacy, kept for backward compat)
    pub capabilities: Vec<String>,
    /// Whether agent advertises HTTP MCP support (ACP spec: McpCapabilities.http)
    pub mcp_http: bool,
    /// Whether agent advertises SSE MCP support (ACP spec: McpCapabilities.sse)
    pub mcp_sse: bool,
    /// Text chunks streamed as `session/update` `agent_message_chunk`
    /// notifications before the final PromptResponse. Empty = respond
    /// with a bare `end_turn` and no notifications (legacy behavior).
    pub stream_chunks: Vec<String>,
    /// Emit a `tool_call` + completed `tool_call_update` notification pair
    /// (after the text chunks) before the final PromptResponse.
    pub stream_tool_call: bool,
    /// After emitting the notifications, hold the turn open until a
    /// `session/cancel` notification arrives, then finish with
    /// `stopReason: cancelled` (per ACP, cancel MUST end the turn that way).
    /// Models a long-running turn so cancellation propagation is testable.
    /// Honored by `ThreadedMockAgent`; the stdio binary ignores it.
    pub hold_turn_until_cancel: bool,
}

impl Default for MockStdioAgentConfig {
    fn default() -> Self {
        Self {
            behavior: AgentBehavior::OpenCode,
            protocol_version: 1,
            requires_auth: false,
            response_delay_ms: None,
            inject_errors: false,
            capabilities: vec![
                "fs.readTextFile".to_string(),
                "fs.writeTextFile".to_string(),
                "terminal".to_string(),
            ],
            mcp_http: false,
            mcp_sse: false,
            stream_chunks: Vec::new(),
            stream_tool_call: false,
            hold_turn_until_cancel: false,
        }
    }
}

impl MockStdioAgentConfig {
    /// Create configuration for OpenCode-compatible agent
    pub fn opencode() -> Self {
        Self {
            behavior: AgentBehavior::OpenCode,
            protocol_version: 1,
            requires_auth: false,
            response_delay_ms: None,
            inject_errors: false,
            capabilities: vec![
                "fs.readTextFile".to_string(),
                "fs.writeTextFile".to_string(),
                "terminal".to_string(),
            ],
            mcp_http: true,
            mcp_sse: false,
            stream_chunks: Vec::new(),
            stream_tool_call: false,
            hold_turn_until_cancel: false,
        }
    }

    /// Create configuration for Claude-ACP-compatible agent
    pub fn claude_acp() -> Self {
        Self {
            behavior: AgentBehavior::ClaudeAcp,
            protocol_version: 1,
            requires_auth: true,
            response_delay_ms: None,
            inject_errors: false,
            capabilities: vec![
                "fs.readTextFile".to_string(),
                "fs.writeTextFile".to_string(),
                "terminal".to_string(),
                "loadSession".to_string(),
            ],
            mcp_http: true,
            mcp_sse: true,
            stream_chunks: Vec::new(),
            stream_tool_call: false,
            hold_turn_until_cancel: false,
        }
    }

    /// Create configuration for Gemini-compatible agent
    pub fn gemini() -> Self {
        Self {
            behavior: AgentBehavior::Gemini,
            protocol_version: 1,
            requires_auth: false,
            response_delay_ms: None,
            inject_errors: false,
            capabilities: vec![
                "fs.readTextFile".to_string(),
                "fs.writeTextFile".to_string(),
            ],
            mcp_http: false,
            mcp_sse: false,
            stream_chunks: Vec::new(),
            stream_tool_call: false,
            hold_turn_until_cancel: false,
        }
    }

    /// Create configuration for Codex-compatible agent
    pub fn codex() -> Self {
        Self {
            behavior: AgentBehavior::Codex,
            protocol_version: 1,
            requires_auth: false,
            response_delay_ms: None,
            inject_errors: false,
            capabilities: vec![
                "fs.readTextFile".to_string(),
                "fs.writeTextFile".to_string(),
                "terminal".to_string(),
            ],
            mcp_http: true,
            mcp_sse: false,
            stream_chunks: Vec::new(),
            stream_tool_call: false,
            hold_turn_until_cancel: false,
        }
    }
}

/// Mock stdio-based ACP agent
///
/// This agent reads JSON-RPC messages from stdin and writes responses to stdout,
/// simulating a real ACP agent for integration testing.
pub struct MockStdioAgent {
    /// Agent configuration (public for threaded mock access)
    pub config: MockStdioAgentConfig,
    pub session_id: Option<String>,
    /// Whether a `session/cancel` notification has been received.
    pub cancel_received: bool,
}

/// The ordered wire messages for one `session/prompt` turn.
pub struct PromptTurn {
    /// `session/update` notifications to emit before the final response.
    pub notifications: Vec<Value>,
    /// Final response ending the turn normally.
    pub end_turn: Value,
    /// Final response after a `session/cancel` (ACP: MUST be `cancelled`).
    pub cancelled: Value,
}

impl MockStdioAgent {
    /// Create a new mock stdio agent
    pub fn new(config: MockStdioAgentConfig) -> Self {
        Self {
            config,
            session_id: None,
            cancel_received: false,
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

            let method = request.get("method").and_then(|m| m.as_str());

            // `session/cancel` is a notification: record it, send nothing.
            if method == Some("session/cancel") {
                self.note_cancel(&request);
                continue;
            }

            // A streaming prompt turn emits notifications before the final
            // response. (`hold_turn_until_cancel` needs interleaved IO and is
            // only honored by ThreadedMockAgent.)
            if method == Some("session/prompt") && !self.config.inject_errors {
                let turn = self.handle_prompt_turn(&request);
                for message in turn.notifications.iter().chain([&turn.end_turn]) {
                    writeln!(stdout, "{}", serde_json::to_string(message)?)?;
                }
                stdout.flush()?;
                continue;
            }

            // Handle the request and generate response
            let response = self.handle_request(&request);

            // Write response to stdout
            let response_json = serde_json::to_string(&response)?;
            writeln!(stdout, "{}", response_json)?;
            stdout.flush()?;
        }

        Ok(())
    }

    /// Record receipt of a `session/cancel` notification. Also writes the
    /// cancelled session id to the file named by `CRU_MOCK_CANCEL_CAPTURE`
    /// (when set) so subprocess-based tests can assert propagation.
    pub fn note_cancel(&mut self, request: &Value) {
        self.cancel_received = true;
        if let Ok(path) = env::var("CRU_MOCK_CANCEL_CAPTURE") {
            let session_id = request
                .get("params")
                .and_then(|p| p.get("sessionId"))
                .and_then(|s| s.as_str())
                .unwrap_or_default();
            let _ = fs::write(path, session_id);
        }
    }

    /// Handle a JSON-RPC request and generate appropriate response
    pub fn handle_request(&mut self, request: &Value) -> Value {
        // Extract method from request
        let method = request
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown");

        match method {
            "initialize" => self.handle_initialize(request),
            "session/new" => self.handle_new_session(request),
            "session/prompt" => self.handle_prompt(request),
            "session/set_model" => self.handle_set_model(request),
            "authenticate" => self.handle_authenticate(request),
            _ => self.error_response(request, -32601, "Method not found"),
        }
    }

    /// Handle `session/set_model`. Captures the requested model id to the file
    /// named by `CRU_MOCK_MODEL_CAPTURE` (when set) so tests can assert the
    /// switch reached the agent over the wire.
    fn handle_set_model(&self, request: &Value) -> Value {
        if let Ok(path) = env::var("CRU_MOCK_MODEL_CAPTURE") {
            let model_id = request
                .get("params")
                .and_then(|p| p.get("modelId"))
                .and_then(|m| m.as_str())
                .unwrap_or_default();
            let _ = fs::write(path, model_id);
        }
        json!({
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "result": {}
        })
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
            AgentBehavior::Custom(_) => ("mock-custom", "1.0.0"),
        };

        // Determine auth methods based on behavior
        let auth_methods = if self.config.requires_auth {
            let auth_method: AuthMethod = serde_json::from_value(json!({
                "id": "api_key",
                "name": "API Key",
                "description": "Authenticate using an API key",
                "_meta": null
            }))
            .expect("Failed to create AuthMethod");
            vec![auth_method]
        } else {
            vec![]
        };

        // Construct proper InitializeResponse using ACP types
        // Serialize auth_methods first for JSON construction
        let auth_methods_json = serde_json::to_value(&auth_methods).unwrap();

        let response: InitializeResponse = serde_json::from_value(json!({
            "protocolVersion": self.config.protocol_version,
            "agentCapabilities": {
                "mcpCapabilities": {
                    "http": self.config.mcp_http,
                    "sse": self.config.mcp_sse
                }
            },
            "authMethods": auth_methods_json,
            "agentInfo": {
                "name": name,
                "version": version,
                "title": null,
                "_meta": null
            },
            "_meta": null
        }))
        .expect("Failed to create InitializeResponse");

        // Serialize to JSON value
        let result = serde_json::to_value(&response).unwrap();

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
        let session_id = format!("mock-session-{}", uuid::Uuid::new_v4());
        self.session_id = Some(session_id.clone());

        // Advertise a model list when asked, mirroring claude-agent-acp's
        // `unstable_session_model` support (availableModels + currentModelId).
        let models = if env::var("CRU_MOCK_ADVERTISE_MODELS").is_ok() {
            json!({
                "currentModelId": "mock-sonnet",
                "availableModels": [
                    {"modelId": "mock-sonnet", "name": "Mock Sonnet"},
                    {"modelId": "mock-opus", "name": "Mock Opus"}
                ]
            })
        } else {
            Value::Null
        };

        // Construct proper NewSessionResponse using ACP types
        let response: NewSessionResponse = serde_json::from_value(json!({
            "sessionId": session_id,
            "modes": null,
            "models": models,
            "_meta": null
        }))
        .expect("Failed to create NewSessionResponse");

        // Serialize response to JSON value (respects serde attributes)
        let result = serde_json::to_value(&response).unwrap();

        // Wrap in JSON-RPC 2.0 response format
        json!({
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "result": result
        })
    }

    /// Handle prompt request (chat message) as a single final response.
    ///
    /// Kept for callers that only need the turn-ending message; the run
    /// loops use `handle_prompt_turn` so configured streaming notifications
    /// reach the wire first.
    fn handle_prompt(&self, request: &Value) -> Value {
        if self.config.inject_errors {
            return self.error_response(request, -32000, "Simulated prompt error");
        }
        self.handle_prompt_turn(request).end_turn
    }

    /// Build the full wire script for a `session/prompt` turn: streaming
    /// `session/update` notifications (per config) followed by the final
    /// response, in the exact shapes `CrucibleAcpClient` parses
    /// (`SessionNotification` / `PromptResponse`).
    pub fn handle_prompt_turn(&self, request: &Value) -> PromptTurn {
        // Test hook: capture the concatenated prompt text to a file so tests
        // can assert exactly what reached the agent over the ACP wire (e.g.
        // daemon-injected Precognition context). Gated on an env var set via
        // the session agent's `env_overrides`, so it's inert by default.
        if let Ok(path) = env::var("CRU_MOCK_PROMPT_CAPTURE") {
            let text = request
                .get("params")
                .and_then(|p| p.get("prompt"))
                .and_then(|p| p.as_array())
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            let _ = fs::write(path, text);
        }

        let session_id = request
            .get("params")
            .and_then(|p| p.get("sessionId"))
            .and_then(|s| s.as_str())
            .map(str::to_string)
            .or_else(|| self.session_id.clone())
            .unwrap_or_else(|| "mock-session".to_string());

        let update = |update: Value| {
            json!({
                "jsonrpc": "2.0",
                "method": "session/update",
                "params": {
                    "sessionId": session_id,
                    "update": update
                }
            })
        };

        let mut notifications: Vec<Value> = self
            .config
            .stream_chunks
            .iter()
            .map(|chunk| {
                update(json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": { "type": "text", "text": chunk }
                }))
            })
            .collect();

        if self.config.stream_tool_call {
            notifications.push(update(json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "mock-tool-call-1",
                "title": "mock_tool",
                "status": "pending",
                "rawInput": { "query": "2+2" }
            })));
            notifications.push(update(json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": "mock-tool-call-1",
                "status": "completed",
                "rawOutput": { "result": "4" }
            })));
        }

        let final_response = |stop_reason: &str| {
            let response: PromptResponse = serde_json::from_value(json!({
                "stopReason": stop_reason,
                "_meta": null
            }))
            .expect("Failed to create PromptResponse");
            json!({
                "jsonrpc": "2.0",
                "id": request.get("id"),
                "result": response
            })
        };

        PromptTurn {
            notifications,
            end_turn: final_response("end_turn"),
            cancelled: final_response("cancelled"),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = MockStdioAgentConfig::default();
        assert_eq!(config.protocol_version, 1);
        assert!(!config.requires_auth);
    }

    #[test]
    fn test_opencode_config() {
        let config = MockStdioAgentConfig::opencode();
        assert_eq!(config.behavior, AgentBehavior::OpenCode);
        assert_eq!(config.protocol_version, 1);
        assert!(!config.requires_auth);
        assert!(config.capabilities.contains(&"terminal".to_string()));
    }

    #[test]
    fn test_claude_acp_config() {
        let config = MockStdioAgentConfig::claude_acp();
        assert_eq!(config.behavior, AgentBehavior::ClaudeAcp);
        assert_eq!(config.protocol_version, 1);
        assert!(config.requires_auth);
        assert!(config.capabilities.contains(&"loadSession".to_string()));
    }

    #[test]
    fn test_gemini_config() {
        let config = MockStdioAgentConfig::gemini();
        assert_eq!(config.behavior, AgentBehavior::Gemini);
        assert_eq!(config.protocol_version, 1);
    }

    #[test]
    fn test_codex_config() {
        let config = MockStdioAgentConfig::codex();
        assert_eq!(config.behavior, AgentBehavior::Codex);
        assert_eq!(config.protocol_version, 1);
    }

    #[test]
    fn test_agent_creation() {
        let config = MockStdioAgentConfig::default();
        let agent = MockStdioAgent::new(config);
        assert!(agent.session_id.is_none());
    }

    #[test]
    fn test_handle_initialize_request() {
        let config = MockStdioAgentConfig::opencode();
        let mut agent = MockStdioAgent::new(config);

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientInfo": null,
                "clientCapabilities": {},
                "meta": null
            }
        });

        let response = agent.handle_request(&request);
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 1);
        assert!(response.get("result").is_some());

        // Verify proper ACP response structure (camelCase in JSON)
        let result = &response["result"];
        assert!(result.get("protocolVersion").is_some());
        assert!(result.get("agentCapabilities").is_some());
        assert!(result.get("authMethods").is_some());
        assert!(result.get("agentInfo").is_some());

        // Verify agent info structure
        let agent_info = &result["agentInfo"];
        assert_eq!(agent_info["name"], "mock-opencode");
        assert_eq!(agent_info["version"], "1.0.0");
    }

    #[test]
    fn test_handle_new_session_request() {
        let config = MockStdioAgentConfig::opencode();
        let mut agent = MockStdioAgent::new(config);

        let request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "session/new",
            "params": {
                "cwd": "/test",
                "mcpServers": [],
                "meta": null
            }
        });

        let response = agent.handle_request(&request);
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 2);
        assert!(response.get("result").is_some());

        // Verify proper ACP response structure (camelCase in JSON)
        let result = &response["result"];
        assert!(result.get("sessionId").is_some());

        // Verify session ID was stored in agent
        assert!(agent.session_id.is_some());

        // Verify session ID format
        let session_id = result["sessionId"].as_str().unwrap();
        assert!(session_id.starts_with("mock-session-"));
    }

    #[test]
    fn test_error_injection() {
        let mut config = MockStdioAgentConfig::opencode();
        config.inject_errors = true;
        let mut agent = MockStdioAgent::new(config);

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });

        let response = agent.handle_request(&request);
        assert!(response.get("error").is_some());
        assert_eq!(response["error"]["code"], -32000);
    }

    /// The mock's streamed notifications must parse with the exact types the
    /// real client uses (`SessionNotification`) — otherwise streaming tests
    /// would exercise a wire shape no production code accepts.
    #[test]
    fn streamed_notifications_parse_as_session_notifications() {
        use agent_client_protocol::SessionNotification;

        let mut config = MockStdioAgentConfig::opencode();
        config.stream_chunks = vec!["a".into(), "b".into()];
        config.stream_tool_call = true;
        let agent = MockStdioAgent::new(config);

        let request = json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "session/prompt",
            "params": {
                "sessionId": "sess-1",
                "prompt": [{"type": "text", "text": "hi"}]
            }
        });

        let turn = agent.handle_prompt_turn(&request);
        assert_eq!(
            turn.notifications.len(),
            4,
            "2 chunks + tool_call + tool_call_update"
        );
        for notification in &turn.notifications {
            assert_eq!(notification["method"], "session/update");
            let params = notification["params"].clone();
            serde_json::from_value::<SessionNotification>(params.clone()).unwrap_or_else(|e| {
                panic!("notification params must parse as SessionNotification: {e}\n{params}")
            });
        }

        // Final responses must parse as PromptResponse with the right reasons.
        let end: PromptResponse =
            serde_json::from_value(turn.end_turn["result"].clone()).expect("end_turn parses");
        assert_eq!(end.stop_reason, agent_client_protocol::StopReason::EndTurn);
        let cancelled: PromptResponse =
            serde_json::from_value(turn.cancelled["result"].clone()).expect("cancelled parses");
        assert_eq!(
            cancelled.stop_reason,
            agent_client_protocol::StopReason::Cancelled
        );
    }

    #[test]
    fn test_unknown_method() {
        let config = MockStdioAgentConfig::opencode();
        let mut agent = MockStdioAgent::new(config);

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "unknown/method",
            "params": {}
        });

        let response = agent.handle_request(&request);
        assert!(response.get("error").is_some());
        assert_eq!(response["error"]["code"], -32601);
    }
}
