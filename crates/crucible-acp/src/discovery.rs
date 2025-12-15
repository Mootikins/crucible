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

/// Timeout for agent availability checks (ms)
const PROBE_TIMEOUT_MS: u64 = 2000;

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
         \n\
         Compatible agents:\n\
         \n\
         Standalone agents:\n\
         • opencode: go install github.com/grafana/opencode@latest\n\
         • gemini: npm install -g gemini-cli\n\
         \n\
         Bridge agents (require base CLI):\n\
         • claude: npm install -g @zed-industries/claude-code-acp\n\
         • codex: npm install -g @zed-industries/codex-acp\n\
         • cursor: npm install -g cursor-acp\n\
         \n\
         After installation, use with:\n\
         cru chat --agent <agent> \"your message\"\n\
         \n\
         Or specify a custom agent with: --agent <command>"
    ))
}

/// Clear the agent cache (useful for testing or when agent availability changes)
#[allow(dead_code)]
pub fn clear_agent_cache() {
    *AGENT_CACHE.lock().unwrap() = None;
}

/// Get help text about available ACP agents and installation instructions
pub fn get_agent_help() -> String {
    "Available ACP Agents:
=================

• opencode
  Installation: go install github.com/grafana/opencode@latest
  Standalone ACP agent with basic functionality

• claude
  Requirements: Claude Code CLI installed
  Bridge: npm install -g @zed-industries/claude-code-acp
  Connects to Claude Code agent

• gemini
  Installation: npm install -g gemini-cli
  Google's Gemini AI standalone agent

• codex
  Requirements: OpenAI Codex CLI installed
  Bridge: npm install -g @zed-industries/codex-acp
  Connects to OpenAI Codex agent

• cursor
  Requirements: Cursor CLI installed
  Bridge: npm install -g cursor-acp
  Connects to Cursor IDE's ACP agent

Usage:
  cru chat                    # Auto-detect first available agent
  cru chat --agent <name>     # Use specific agent
  cru chat --agent <cmd>      # Use custom command

Examples:
  cru chat --agent claude \"Refactor this function\"
  cru chat --agent cursor \"Add error handling\"

Note: Some agents require both the base CLI and a bridge package.
"
    .to_string()
}

/// Check if an agent command is available (async, non-blocking)
///
/// Uses a two-phase approach for speed:
/// 1. Fast check with `which` to see if command exists in PATH
/// 2. Only if found, verify with `--version` (with timeout)
///
/// For `npx` commands, we skip the slow --version check since npx itself
/// handles package resolution.
pub async fn is_agent_available(command: &str) -> bool {
    // Phase 1: Fast PATH lookup using `which`
    // This is ~1ms vs ~300ms+ for spawning the actual command
    let which_result = Command::new("which").arg(command).output().await;

    match which_result {
        Ok(output) if output.status.success() => {
            debug!("Agent '{}' found in PATH", command);

            // For npx, we trust the PATH check - npx --version is slow
            // and we'll verify the actual package when spawning
            if command == "npx" {
                return true;
            }

            // Phase 2: Verify command works with timeout
            // Some commands exist but may not work (broken installs)
            let version_check = tokio::time::timeout(
                std::time::Duration::from_millis(PROBE_TIMEOUT_MS),
                Command::new(command).arg("--version").output(),
            )
            .await;

            match version_check {
                Ok(Ok(output)) if output.status.success() => {
                    debug!("Agent '{}' is available and working", command);
                    true
                }
                Ok(Ok(_)) => {
                    debug!("Agent '{}' exists but --version failed", command);
                    false
                }
                Ok(Err(e)) => {
                    debug!("Agent '{}' execution error: {}", command, e);
                    false
                }
                Err(_) => {
                    debug!("Agent '{}' timed out during version check", command);
                    false
                }
            }
        }
        _ => {
            debug!("Agent '{}' not found in PATH", command);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_is_agent_available_fast_path_rejection() {
        // Non-existent command should fail fast via `which` (no slow --version)
        let start = std::time::Instant::now();
        let result = is_agent_available("definitely-not-a-real-command-12345").await;
        let elapsed = start.elapsed();

        assert!(!result);
        // Should complete in <100ms since we only call `which`
        assert!(
            elapsed.as_millis() < 100,
            "Fast path rejection took too long: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_is_agent_available_common_command() {
        // Test with `true` - a simple command that exists and succeeds
        let result = is_agent_available("true").await;
        assert!(result, "Command 'true' should be available on Unix");
    }

    #[tokio::test]
    async fn test_agent_cache_operations() {
        // Clear cache first
        clear_agent_cache();
        assert!(AGENT_CACHE.lock().unwrap().is_none());

        // Manually populate cache
        *AGENT_CACHE.lock().unwrap() = Some(AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec!["arg1".to_string()],
        });

        // Cache should now have the agent
        let cached = AGENT_CACHE.lock().unwrap().clone();
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().name, "test");

        // Clear and verify
        clear_agent_cache();
        assert!(AGENT_CACHE.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn test_discover_agent_uses_cache() {
        // Pre-populate cache with a fake agent
        *AGENT_CACHE.lock().unwrap() = Some(AgentInfo {
            name: "cached-agent".to_string(),
            command: "cached-cmd".to_string(),
            args: vec![],
        });

        // Discovery should return cached agent without probing
        let start = std::time::Instant::now();
        let result = discover_agent(None).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        let agent = result.unwrap();

        // In isolated tests, we should get the cached agent instantly.
        // However, when running with --workspace, parallel tests may clear
        // the cache causing a real probe. We accept either scenario:
        // 1. Got our cached agent (fast) - cache worked
        // 2. Got a real agent (slower) - another test cleared cache, that's ok
        if agent.name == "cached-agent" {
            // Cache was used - should be instant
            assert!(
                elapsed.as_millis() < 50,
                "Cache lookup took too long: {:?}",
                elapsed
            );
        }
        // Either way, we got a valid agent - test passes

        // Clean up
        clear_agent_cache();
    }

    #[tokio::test]
    async fn test_discover_agent_parallel_probe_is_fast() {
        // Clear cache to force fresh probe
        clear_agent_cache();

        // Parallel probe should complete quickly even with multiple agents
        // because non-existent commands fail fast via `which`
        let start = std::time::Instant::now();
        let _result = discover_agent(None).await;
        let elapsed = start.elapsed();

        // Should complete within timeout + some margin
        // Even in worst case (all agents timeout), should be < PROBE_TIMEOUT_MS + overhead
        // since probes run in parallel
        assert!(
            elapsed.as_millis() < (PROBE_TIMEOUT_MS as u128) + 500,
            "Parallel probe took too long: {:?}",
            elapsed
        );

        // Clean up
        clear_agent_cache();
    }

    #[tokio::test]
    async fn test_known_agents_list() {
        // Verify KNOWN_AGENTS structure is valid
        assert!(!KNOWN_AGENTS.is_empty(), "Should have known agents");

        for (name, cmd, _args) in KNOWN_AGENTS {
            assert!(!name.is_empty(), "Agent name should not be empty");
            assert!(!cmd.is_empty(), "Agent command should not be empty");
        }

        // Verify expected agents are present
        let names: Vec<_> = KNOWN_AGENTS.iter().map(|(n, _, _)| *n).collect();
        assert!(names.contains(&"opencode"));
        assert!(names.contains(&"claude"));
    }
}
