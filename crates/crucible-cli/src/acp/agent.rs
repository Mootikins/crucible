//! Agent Discovery and Management
//!
//! Handles discovering and spawning ACP-compatible agents.

use anyhow::{anyhow, Result};
use std::process::Command;
use tracing::{debug, info, warn};

/// Information about a discovered agent
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

/// Known ACP-compatible agents (name, command, args)
const KNOWN_AGENTS: &[(&str, &str, &[&str])] = &[
    ("opencode", "opencode", &["acp"]),
    ("claude-acp", "npx", &["@zed-industries/claude-code-acp"]),
    ("gemini", "gemini-cli", &[]),
    ("codex", "codex", &[]),
];

/// Discover an available ACP agent
///
/// # Arguments
/// * `preferred` - Optional preferred agent name to try first
///
/// # Returns
/// AgentInfo for the discovered agent
///
/// # Errors
/// Returns error if no compatible agent is found
pub async fn discover_agent(preferred: Option<&str>) -> Result<AgentInfo> {
    // Try user's preferred agent first
    if let Some(agent_name) = preferred {
        debug!("Trying preferred agent: {}", agent_name);
        // Try to find in known agents list
        if let Some((name, cmd, args)) = KNOWN_AGENTS.iter().find(|(n, _, _)| *n == agent_name) {
            if is_agent_available(cmd).await? {
                info!("Using preferred agent: {}", agent_name);
                return Ok(AgentInfo {
                    name: name.to_string(),
                    command: cmd.to_string(),
                    args: args.iter().map(|s| s.to_string()).collect(),
                });
            }
        }
        warn!("Preferred agent '{}' not found, trying fallbacks", agent_name);
    }

    // Fallback: try all known agents
    for (name, cmd, args) in KNOWN_AGENTS {
        debug!("Trying agent: {} {} {:?}", name, cmd, args);
        if is_agent_available(cmd).await? {
            info!("Discovered agent: {} ({} {:?})", name, cmd, args);
            return Ok(AgentInfo {
                name: name.to_string(),
                command: cmd.to_string(),
                args: args.iter().map(|s| s.to_string()).collect(),
            });
        }
    }

    // None found - provide helpful error message
    Err(anyhow!(
        "No compatible ACP agent found.\n\
         Compatible agents: opencode, claude-acp, gemini-cli, codex\n\
         Install one with: npm install @zed-industries/claude-code-acp\n\
         Or specify a custom agent with: --agent <command>"
    ))
}

/// Check if an agent command is available
pub async fn is_agent_available(command: &str) -> Result<bool> {
    // Try to run the command with --version to check if it exists
    let result = Command::new(command)
        .arg("--version")
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                debug!("Agent '{}' is available", command);
                Ok(true)
            } else {
                debug!("Agent '{}' exists but --version failed", command);
                Ok(false)
            }
        }
        Err(e) => {
            debug!("Agent '{}' not available: {}", command, e);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_is_agent_available_returns_ok() {
        // Test that the function returns Ok regardless of whether command exists
        // Don't assert on the result value since it depends on system state
        let result = is_agent_available("ls").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_is_agent_available_rejects_nonexistent() {
        // This command should not exist
        let result = is_agent_available("definitely-not-a-real-command-12345").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
