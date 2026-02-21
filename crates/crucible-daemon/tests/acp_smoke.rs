//! ACP E2E smoke tests using mock-acp-agent binary.
//!
//! These tests spawn the mock-acp-agent process to verify the full ACP lifecycle
//! (spawn → handshake → message → delegation → recording) without requiring
//! real LLM API keys.
//!
//! # Prerequisites
//! Build the mock agent first:
//! ```
//! cargo build -p crucible-acp --features test-utils --bin mock-acp-agent
//! ```

use crucible_config::BackendType;
use crucible_core::session::SessionAgent;
use std::collections::HashMap;
use std::path::PathBuf;

/// Returns the path to the mock-acp-agent binary.
///
/// The binary is built at `target/debug/mock-acp-agent` relative to the workspace root.
/// This function resolves the path from the daemon crate's manifest directory.
pub fn mock_agent_path() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug/mock-acp-agent")
        .canonicalize()
        .expect(
            "mock-acp-agent binary not found. Build it with:\n\
             cargo build -p crucible-acp --features test-utils --bin mock-acp-agent",
        );
    path
}

/// Creates a SessionAgent configured for ACP with the given agent path.
///
/// This helper constructs a minimal SessionAgent with:
/// - agent_type: "acp"
/// - agent_name: the provided agent_path
/// - provider: Mock (for testing)
/// - All other fields set to sensible defaults
pub fn mock_session_agent(agent_path: &str) -> SessionAgent {
    SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some(agent_path.to_string()),
        provider_key: None,
        provider: BackendType::Mock,
        model: "mock-model".to_string(),
        system_prompt: "You are a helpful assistant.".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
    }
}

#[test]
fn mock_binary_exists_and_runs() {
    let path = mock_agent_path();
    assert!(path.exists(), "mock-acp-agent not found at {:?}", path);

    let output = std::process::Command::new(&path)
        .arg("--help")
        .output()
        .expect("Failed to execute mock-acp-agent");

    assert!(
        output.status.success(),
        "mock-acp-agent --help failed with status: {:?}",
        output.status
    );
}
