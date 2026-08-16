//! Locating and configuring the spawned `mock-acp-agent` binary.
//!
//! Shared by every test that drives a real `AcpAgentHandle` (which spawns a
//! process and speaks ACP over its stdio) rather than a `CrucibleAcpClient`
//! over an in-process duplex pipe.
//!
//! `acp_support` is `#[path]`-included by several test binaries and each uses
//! only part of it, so every helper here is dead code in at least one of them.
//! The allows are per-item rather than a module-level `#![allow(unused)]` so
//! that an unused import or variable added later is still a warning.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crucible_core::config::BackendType;
use crucible_core::session::{OutputValidation, SessionAgent};
use crucible_daemon::acp_handle::AcpAgentHandleParams;

/// Returns the path to the mock-acp-agent binary.
///
/// Prefers `CARGO_BIN_EXE_mock-acp-agent` (set by cargo when the bin target is
/// built, i.e. when the test-utils feature is enabled — honors any custom
/// target-dir). Falls back to the default workspace-root target path.
#[allow(dead_code)]
pub fn mock_agent_path() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_mock-acp-agent") {
        return PathBuf::from(path);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug/mock-acp-agent")
        .canonicalize()
        .expect(
            "mock-acp-agent binary not found. Build it with:\n\
             cargo build -p crucible-daemon --features test-utils --bin mock-acp-agent",
        )
}

/// Creates a SessionAgent configured for ACP with the given agent path.
///
/// This helper constructs a minimal SessionAgent with:
/// - agent_type: "acp"
/// - agent_name: the provided agent_path
/// - provider: Mock (for testing)
/// - All other fields set to sensible defaults
#[allow(dead_code)]
pub fn mock_session_agent(agent_path: &str) -> SessionAgent {
    SessionAgent {
        mode: None,
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
        precognition_enabled: false,
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: OutputValidation::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        tool_policy: None,
    }
}

/// `AcpAgentHandleParams` with everything optional switched off.
///
/// Every test that drives a spawned mock agent needs the same twelve-field
/// literal with only the two required references filled in; the struct cannot
/// `derive(Default)` because those two are borrows. Override a field with
/// functional-update syntax:
///
/// ```ignore
/// AcpAgentHandle::new(AcpAgentHandleParams {
///     acp_config: Some(&acp_config),
///     ..mock_handle_params(&agent_config, workspace.path())
/// })
/// ```
#[allow(dead_code)]
pub fn mock_handle_params<'a>(
    agent_config: &'a SessionAgent,
    workspace: &'a Path,
) -> AcpAgentHandleParams<'a> {
    AcpAgentHandleParams {
        agent_config,
        workspace,
        kiln_path: None,
        knowledge_repo: None,
        embedding_provider: None,
        background_spawner: None,
        delegation_spawner: None,
        parent_session_id: None,
        delegation_config: None,
        acp_config: None,
        permission_handler: None,
        sandbox_exec: None,
        containment: crucible_daemon::tools::containment::RootSet::Ambient,
    }
}
