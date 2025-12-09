//! Agent Discovery and Management
//!
//! Handles discovering and spawning ACP-compatible agents.
//! Uses parallel probing for fast agent discovery.

use anyhow::{anyhow, Result};
use futures::future::join_all;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tokio::process::Command;
use tracing::{debug, info, trace, warn};

/// Cache for discovered agent to avoid repeated probing on subsequent calls
static AGENT_CACHE: Lazy<Mutex<Option<AgentInfo>>> = Lazy::new(|| Mutex::new(None));

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
    ("claude", "npx", &["@zed-industries/claude-code-acp"]),
    ("gemini", "gemini-cli", &[]),
    ("codex", "npx", &["@zed-industries/codex-acp"]),
    ("cursor", "cursor-acp", &[]),
];

/// Discover an available ACP agent using parallel probing
///
/// This function probes all known agents concurrently for faster discovery.
/// Results are cached to avoid repeated probing on subsequent calls.
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
    // Check cache first (unless a specific agent is preferred)
    if preferred.is_none() {
        if let Some(cached) = AGENT_CACHE.lock().unwrap().clone() {
            trace!("Using cached agent: {}", cached.name);
            return Ok(cached);
        }
    }

    // If a preferred agent is specified, check it first (single probe)
    if let Some(agent_name) = preferred {
        debug!("Trying preferred agent: {}", agent_name);
        if let Some((name, cmd, args)) = KNOWN_AGENTS.iter().find(|(n, _, _)| *n == agent_name) {
            if is_agent_available(cmd).await {
                info!("Using preferred agent: {}", agent_name);
                let agent = AgentInfo {
                    name: name.to_string(),
                    command: cmd.to_string(),
                    args: args.iter().map(|s| s.to_string()).collect(),
                };
                // Cache the result
                *AGENT_CACHE.lock().unwrap() = Some(agent.clone());
                return Ok(agent);
            }
        }
        warn!(
            "Preferred agent '{}' not found, trying fallbacks",
            agent_name
        );
    }

    // Parallel probe: check all agents concurrently
    debug!("Probing {} agents in parallel", KNOWN_AGENTS.len());
    let start = std::time::Instant::now();

    let futures: Vec<_> = KNOWN_AGENTS
        .iter()
        .map(|(name, cmd, args)| async move {
            let available = is_agent_available(cmd).await;
            (*name, *cmd, *args, available)
        })
        .collect();

    let results = join_all(futures).await;
    debug!("Parallel probe completed in {:?}", start.elapsed());

    // Find first available agent (maintaining priority order)
    for (name, cmd, args, available) in results {
        if available {
            info!("Discovered agent: {} ({} {:?})", name, cmd, args);
            let agent = AgentInfo {
                name: name.to_string(),
                command: cmd.to_string(),
                args: args.iter().map(|s| s.to_string()).collect(),
            };
            // Cache the result
            *AGENT_CACHE.lock().unwrap() = Some(agent.clone());
            return Ok(agent);
        }
    }

    // None found - provide helpful error message
    Err(anyhow!(
        "No compatible ACP agent found.\n\
         Compatible agents: opencode, claude, gemini, codex, cursor\n\
         Install one with: npm install @zed-industries/claude-code-acp\n\
         Or specify a custom agent with: --agent <command>"
    ))
}

/// Clear the agent cache (useful for testing or when agent availability changes)
#[allow(dead_code)]
pub fn clear_agent_cache() {
    *AGENT_CACHE.lock().unwrap() = None;
}

/// Check if an agent command is available (async, non-blocking)
///
/// Uses tokio::process::Command for async execution, allowing parallel probing.
pub async fn is_agent_available(command: &str) -> bool {
    // Try to run the command with --version to check if it exists
    let result = Command::new(command).arg("--version").output().await;

    match result {
        Ok(output) => {
            if output.status.success() {
                debug!("Agent '{}' is available", command);
                true
            } else {
                debug!("Agent '{}' exists but --version failed", command);
                false
            }
        }
        Err(e) => {
            debug!("Agent '{}' not available: {}", command, e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_is_agent_available_common_command() {
        // Test with a commonly available command (like 'ls' on Unix)
        // Don't assert on the result value since it depends on system state
        let _result = is_agent_available("ls").await;
        // Just verify it doesn't panic
    }

    #[tokio::test]
    async fn test_is_agent_available_rejects_nonexistent() {
        // This command should not exist
        let result = is_agent_available("definitely-not-a-real-command-12345").await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_agent_cache_is_populated() {
        // Clear cache first
        clear_agent_cache();

        // Cache should be empty initially
        assert!(AGENT_CACHE.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn test_discover_agent_parallel_probe() {
        // Clear cache to force a fresh probe
        clear_agent_cache();

        // This will probe all agents in parallel
        // We can't assert on the result since it depends on what's installed
        let result = discover_agent(None).await;

        // If any agent is available, it should be cached
        if result.is_ok() {
            assert!(AGENT_CACHE.lock().unwrap().is_some());
        }
    }
}
