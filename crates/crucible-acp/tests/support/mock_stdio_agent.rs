//! Mock stdio-based ACP agent for integration testing
//!
//! This module provides a mock agent that communicates via stdin/stdout,
//! allowing integration tests to simulate real agent behavior without
//! requiring actual agent binaries.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};

// Import ACP protocol types for proper response construction
use agent_client_protocol::{
    AgentCapabilities, AuthMethod, AuthMethodId, Implementation, InitializeResponse,
    NewSessionResponse, PromptResponse, ProtocolVersion, SessionId, StopReason,
};

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
    /// Custom capabilities to advertise
    pub capabilities: Vec<String>,
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
        }
    }

    /// Create configuration for Claude-ACP-compatible agent
    pub fn claude_acp() -> Self {
        Self {
            behavior: AgentBehavior::ClaudeAcp,
            protocol_version: 1,
            requires_auth: true, // Claude typically requires API key
            response_delay_ms: None,
            inject_errors: false,
            capabilities: vec![
                "fs.readTextFile".to_string(),
                "fs.writeTextFile".to_string(),
                "terminal".to_string(),
                "loadSession".to_string(),
            ],
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
}

impl MockStdioAgent {
    /// Create a new mock stdio agent
    pub fn new(config: MockStdioAgentConfig) -> Self {
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

            // Handle the request and generate response
            let response = self.handle_request(&request);

            // Write response to stdout
            let response_json = serde_json::to_string(&response)?;
            writeln!(stdout, "{}", response_json)?;
            stdout.flush()?;
        }

        Ok(())
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
            "authenticate" => self.handle_authenticate(request),
            _ => self.error_response(request, -32601, "Method not found"),
        }
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
            "agentCapabilities": {},
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
        let mut result = serde_json::to_value(&response).unwrap();

        // Add custom MCP capabilities (non-standard extension for testing)
        // This supports the legacy capability format expected by tests
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
        let session_id = format!("mock-session-{}", uuid::Uuid::new_v4());
        self.session_id = Some(session_id.clone());

        // Construct proper NewSessionResponse using ACP types
        let response: NewSessionResponse = serde_json::from_value(json!({
            "sessionId": session_id,
            "modes": null,
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

    /// Handle prompt request (chat message)
    fn handle_prompt(&self, request: &Value) -> Value {
        if self.config.inject_errors {
            return self.error_response(request, -32000, "Simulated prompt error");
        }

        // Construct proper PromptResponse using ACP types
        let response: PromptResponse = serde_json::from_value(json!({
            "stopReason": "end_turn",
            "_meta": null
        }))
        .expect("Failed to create PromptResponse");

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
