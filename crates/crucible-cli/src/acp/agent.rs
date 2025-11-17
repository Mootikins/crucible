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
}

/// Known ACP-compatible agents
const KNOWN_AGENTS: &[(&str, &str)] = &[
    ("claude-code", "claude-code"),
    ("gemini", "gemini-cli"),
    ("codex", "codex"),
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
        if is_agent_available(agent_name).await? {
            info!("Using preferred agent: {}", agent_name);
            return Ok(AgentInfo {
                name: agent_name.to_string(),
                command: agent_name.to_string(),
            });
        }
        warn!("Preferred agent '{}' not found, trying fallbacks", agent_name);
    }

    // Fallback: try all known agents
    for (name, cmd) in KNOWN_AGENTS {
        debug!("Trying agent: {} ({})", name, cmd);
        if is_agent_available(cmd).await? {
            info!("Discovered agent: {} ({})", name, cmd);
            return Ok(AgentInfo {
                name: name.to_string(),
                command: cmd.to_string(),
            });
        }
    }

    // None found - provide helpful error message
    Err(anyhow!(
        "No compatible ACP agent found.\n\
         Compatible agents: claude-code, gemini-cli, codex\n\
         Install one with: npm install -g @anthropic/claude-code\n\
         Or specify a custom agent with: --agent <command>"
    ))
}

/// Check if an agent command is available
async fn is_agent_available(command: &str) -> Result<bool> {
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
    async fn test_is_agent_available_finds_sh() {
        // sh should be available on any Unix system
        let result = is_agent_available("sh").await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_is_agent_available_rejects_nonexistent() {
        // This command should not exist
        let result = is_agent_available("definitely-not-a-real-command-12345").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
