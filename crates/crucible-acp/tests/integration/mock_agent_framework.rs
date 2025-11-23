//! Tests for the mock agent framework itself
//!
//! These tests verify that our mock agents behave correctly and can simulate
//! various agent behaviors for testing purposes.

use crate::support::{MockStdioAgent, MockStdioAgentConfig, AgentBehavior};
use serde_json::json;

#[test]
fn test_mock_agent_config_creation() {
    let config = MockStdioAgentConfig::default();
    assert_eq!(config.protocol_version, 1);
    assert!(!config.requires_auth);
    assert!(!config.inject_errors);
}

#[test]
fn test_opencode_mock_config() {
    let config = MockStdioAgentConfig::opencode();
    assert_eq!(config.behavior, AgentBehavior::OpenCode);
    assert_eq!(config.protocol_version, 1);
    assert!(!config.requires_auth);
    assert!(config.capabilities.contains(&"terminal".to_string()));
    assert!(config.capabilities.contains(&"fs.readTextFile".to_string()));
    assert!(config.capabilities.contains(&"fs.writeTextFile".to_string()));
}

#[test]
fn test_claude_acp_mock_config() {
    let config = MockStdioAgentConfig::claude_acp();
    assert_eq!(config.behavior, AgentBehavior::ClaudeAcp);
    assert_eq!(config.protocol_version, 1);
    assert!(config.requires_auth, "Claude should require authentication");
    assert!(config.capabilities.contains(&"loadSession".to_string()));
}

#[test]
fn test_gemini_mock_config() {
    let config = MockStdioAgentConfig::gemini();
    assert_eq!(config.behavior, AgentBehavior::Gemini);
    assert_eq!(config.protocol_version, 1);
    assert!(!config.requires_auth);
}

#[test]
fn test_codex_mock_config() {
    let config = MockStdioAgentConfig::codex();
    assert_eq!(config.behavior, AgentBehavior::Codex);
    assert_eq!(config.protocol_version, 1);
    assert!(!config.requires_auth);
}

#[test]
fn test_mock_agent_handles_initialize() {
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

    // Verify JSON-RPC structure
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert!(response.get("result").is_some(), "Response should have result");
    assert!(response.get("error").is_none(), "Response should not have error");

    // Verify response structure
    let result = &response["result"];
    assert!(result.get("protocolVersion").is_some(), "Should have protocol version");
    assert!(result.get("agentInfo").is_some(), "Should have agent info");
    assert!(result.get("agentCapabilities").is_some(), "Should have agent capabilities");

    // Verify agent info
    let agent_info = &result["agentInfo"];
    assert_eq!(agent_info["name"], "mock-opencode");
    assert_eq!(agent_info["version"], "1.0.0");

    // Verify MCP capabilities
    let mcp_capabilities = &result["agentCapabilities"]["mcpCapabilities"];
    assert!(mcp_capabilities.get("terminal").is_some(), "Should have terminal capability");
    assert!(mcp_capabilities.get("fs.readTextFile").is_some(), "Should have fs.readTextFile capability");
}

#[test]
fn test_mock_agent_handles_new_session() {
    let config = MockStdioAgentConfig::opencode();
    let mut agent = MockStdioAgent::new(config);

    let request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "session/new",
        "params": {
            "cwd": "/test/workspace",
            "mcpServers": [],
            "meta": null
        }
    });

    let response = agent.handle_request(&request);

    // Verify JSON-RPC structure
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 2);
    assert!(response.get("result").is_some(), "Response should have result");
    assert!(response.get("error").is_none(), "Response should not have error");

    // Verify session ID was generated
    let result = &response["result"];
    assert!(result.get("sessionId").is_some(), "Should have session ID");
    let session_id = result["sessionId"].as_str().unwrap();
    assert!(session_id.starts_with("mock-session-"), "Session ID should have expected prefix");

    // Verify agent stored the session ID
    assert!(agent.session_id.is_some(), "Agent should store session ID");
}

#[test]
fn test_mock_agent_error_injection() {
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

    // Verify error response
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert!(response.get("error").is_some(), "Response should have error");
    assert!(response.get("result").is_none(), "Response should not have result");

    // Verify error structure
    let error = &response["error"];
    assert_eq!(error["code"], -32000);
    assert!(error["message"].as_str().unwrap().contains("Simulated"));
}

#[test]
fn test_mock_agent_unknown_method() {
    let config = MockStdioAgentConfig::opencode();
    let mut agent = MockStdioAgent::new(config);

    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "unknown/method",
        "params": {}
    });

    let response = agent.handle_request(&request);

    // Verify error response
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert!(response.get("error").is_some(), "Response should have error");

    // Verify error code for "Method not found"
    let error = &response["error"];
    assert_eq!(error["code"], -32601, "Should return 'Method not found' error code");
}

#[test]
fn test_mock_agent_complete_handshake_sequence() {
    let config = MockStdioAgentConfig::opencode();
    let mut agent = MockStdioAgent::new(config);

    // Step 1: Initialize
    let init_request = json!({
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

    let init_response = agent.handle_request(&init_request);
    assert!(init_response.get("result").is_some(), "Initialize should succeed");
    assert_eq!(init_response["result"]["protocolVersion"], 1, "Should return protocol version 1");

    // Step 2: New Session
    let session_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "session/new",
        "params": {
            "cwd": "/test",
            "mcpServers": [],
            "meta": null
        }
    });

    let session_response = agent.handle_request(&session_request);
    assert!(session_response.get("result").is_some(), "Session creation should succeed");
    assert!(session_response["result"].get("sessionId").is_some(), "Should return session ID");

    // Step 3: Send a prompt (chat message)
    let prompt_request = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "session/prompt",
        "params": {
            "message": "Hello, agent!"
        }
    });

    let prompt_response = agent.handle_request(&prompt_request);
    assert!(prompt_response.get("result").is_some(), "Prompt should succeed");
}

#[test]
fn test_different_agent_behaviors() {
    // Test OpenCode
    let mut opencode_agent = MockStdioAgent::new(MockStdioAgentConfig::opencode());
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    });
    let response = opencode_agent.handle_request(&request);
    assert_eq!(response["result"]["agentInfo"]["name"], "mock-opencode");

    // Test Claude
    let mut claude_agent = MockStdioAgent::new(MockStdioAgentConfig::claude_acp());
    let response = claude_agent.handle_request(&request);
    assert_eq!(response["result"]["agentInfo"]["name"], "mock-claude-acp");

    // Test Gemini
    let mut gemini_agent = MockStdioAgent::new(MockStdioAgentConfig::gemini());
    let response = gemini_agent.handle_request(&request);
    assert_eq!(response["result"]["agentInfo"]["name"], "mock-gemini");

    // Test Codex
    let mut codex_agent = MockStdioAgent::new(MockStdioAgentConfig::codex());
    let response = codex_agent.handle_request(&request);
    assert_eq!(response["result"]["agentInfo"]["name"], "mock-codex");
}
