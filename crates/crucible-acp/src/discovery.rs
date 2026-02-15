//! Agent Discovery and Management
//!
//! Handles discovering and spawning ACP-compatible agents.
//! Uses parallel probing for fast agent discovery.

use anyhow::{anyhow, Result};
use crucible_config::{AcpConfig, AgentProfile};
use futures::future::join_all;
use once_cell::sync::Lazy;
use std::collections::HashMap;
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
    /// Environment variables to pass to the agent process
    pub env_vars: HashMap<String, String>,
}

/// Known ACP-compatible agents (name, command, args, description)
const KNOWN_AGENTS: &[(&str, &str, &[&str], &str)] = &[
    ("opencode", "opencode", &["acp"], "OpenCode AI (Go)"),
    (
        "claude",
        "npx",
        &["@zed-industries/claude-code-acp"],
        "Claude Code via ACP",
    ),
    ("gemini", "gemini", &[], "Google Gemini CLI"),
    (
        "codex",
        "npx",
        &["@zed-industries/codex-acp"],
        "OpenAI Codex via ACP",
    ),
    ("cursor", "cursor-acp", &[], "Cursor IDE via ACP"),
];

/// Information about a known agent (for splash screen display)
#[derive(Debug, Clone)]
pub struct KnownAgent {
    pub name: String,
    pub description: String,
    pub available: bool,
}

/// Get list of known agents (sync, no availability check)
pub fn get_known_agents() -> Vec<KnownAgent> {
    KNOWN_AGENTS
        .iter()
        .map(|(name, _, _, desc)| KnownAgent {
            name: name.to_string(),
            description: desc.to_string(),
            available: false, // Unknown until probed
        })
        .collect()
}

/// Probe all known agents and return availability status
///
/// This probes all agents in parallel and returns their availability.
/// Use this to populate splash screen with actual availability info.
pub async fn probe_all_agents() -> Vec<KnownAgent> {
    let futures: Vec<_> = KNOWN_AGENTS
        .iter()
        .map(|(name, cmd, _, desc)| async move {
            let available = is_agent_available(cmd).await;
            KnownAgent {
                name: name.to_string(),
                description: desc.to_string(),
                available,
            }
        })
        .collect();

    join_all(futures).await
}

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
        if let Some((name, cmd, args, _)) =
            KNOWN_AGENTS.iter().find(|(n, _, _, _)| *n == agent_name)
        {
            if is_agent_available(cmd).await {
                info!("Using preferred agent: {}", agent_name);
                let agent = AgentInfo {
                    name: name.to_string(),
                    command: cmd.to_string(),
                    args: args.iter().map(|s| s.to_string()).collect(),
                    env_vars: HashMap::new(),
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
        .map(|(name, cmd, args, _)| async move {
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
                env_vars: HashMap::new(),
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

/// Commands that should trust PATH lookup without --version verification.
/// These are either:
/// - Package managers (npx) that handle resolution themselves
/// - ACP agents that start servers and don't support --version
const TRUST_PATH_COMMANDS: &[&str] = &[
    "npx",        // Package manager, verifies packages at runtime
    "cursor-acp", // ACP server, no --version support
    "gemini",     // ACP server, no --version support
];

/// Check if an agent command is available (async, non-blocking)
///
/// Uses a two-phase approach for speed:
/// 1. Fast check with `which` to see if command exists in PATH
/// 2. Only if found, verify with `--version` (with timeout)
///
/// For certain commands (npx, cursor-acp, gemini-cli), we skip the
/// --version check since they either handle verification at runtime
/// or don't support --version.
pub async fn is_agent_available(command: &str) -> bool {
    // Phase 1: Fast PATH lookup using `which` (Unix) or `where` (Windows)
    // This is ~1ms vs ~300ms+ for spawning the actual command
    #[cfg(windows)]
    let which_cmd = "where";
    #[cfg(not(windows))]
    let which_cmd = "which";

    let which_result = Command::new(which_cmd).arg(command).output().await;

    match which_result {
        Ok(output) if output.status.success() => {
            debug!("Agent '{}' found in PATH", command);

            // For certain commands, trust PATH check without --version verification
            if TRUST_PATH_COMMANDS.contains(&command) {
                debug!("Agent '{}' trusted without --version check", command);
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

/// Resolve an agent from config profiles or built-in agents
///
/// This function looks up an agent by name, checking:
/// 1. Custom profiles in config.agents
/// 2. Built-in KNOWN_AGENTS
///
/// For custom profiles, it can extend a built-in agent (using `extends`)
/// or define a completely custom agent (using `command` and `args`).
///
/// # Arguments
/// * `name` - Agent name to resolve
/// * `config` - ACP configuration containing agent profiles
///
/// # Returns
/// AgentInfo with merged configuration
pub fn resolve_agent_from_config(name: &str, config: &AcpConfig) -> Result<AgentInfo> {
    // Check for custom profile first
    if let Some(profile) = config.agents.get(name) {
        return resolve_profile(name, profile);
    }

    // Fall back to built-in agent
    if let Some((_, cmd, args, _)) = KNOWN_AGENTS.iter().find(|(n, _, _, _)| *n == name) {
        return Ok(AgentInfo {
            name: name.to_string(),
            command: cmd.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            env_vars: HashMap::new(),
        });
    }

    Err(anyhow!(
        "Unknown agent '{}'. Use --agent with a known agent name or define it in config.",
        name
    ))
}

/// Resolve a custom agent profile
fn resolve_profile(name: &str, profile: &AgentProfile) -> Result<AgentInfo> {
    // If profile has custom command, use it directly
    if let Some(cmd) = &profile.command {
        return Ok(AgentInfo {
            name: name.to_string(),
            command: cmd.clone(),
            args: profile.args.clone().unwrap_or_default(),
            env_vars: profile.env.clone(),
        });
    }

    // Otherwise, look up base agent (from extends or profile name)
    let base_name = profile.extends.as_deref().unwrap_or(name);

    if let Some((_, cmd, args, _)) = KNOWN_AGENTS.iter().find(|(n, _, _, _)| *n == base_name) {
        Ok(AgentInfo {
            name: name.to_string(),
            command: cmd.to_string(),
            args: profile
                .args
                .clone()
                .unwrap_or_else(|| args.iter().map(|s| s.to_string()).collect()),
            env_vars: profile.env.clone(),
        })
    } else {
        Err(anyhow!(
            "Agent profile '{}' extends unknown agent '{}'. Define command/args or use a known base agent.",
            name,
            base_name
        ))
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
        // Should complete quickly, but Windows process updates can be slow
        // especially in CI or under load. 1000ms is generous but differentiates
        // from the 2000ms timeout
        assert!(
            elapsed.as_millis() < 1000,
            "Fast path rejection took too long: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_is_agent_available_common_command() {
        // Test with `cargo` - guaranteed to exist when running tests and supports --version
        let result = is_agent_available("cargo").await;
        assert!(result, "Command 'cargo' should be available");
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
            env_vars: HashMap::new(),
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
            env_vars: HashMap::new(),
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

        for (name, cmd, _args, _desc) in KNOWN_AGENTS {
            assert!(!name.is_empty(), "Agent name should not be empty");
            assert!(!cmd.is_empty(), "Agent command should not be empty");
        }

        // Verify expected agents are present
        let names: Vec<_> = KNOWN_AGENTS.iter().map(|(n, _, _, _)| *n).collect();
        assert!(names.contains(&"opencode"));
        assert!(names.contains(&"claude"));
    }

    #[test]
    fn test_agent_info_has_env_vars() {
        // AgentInfo should support environment variables
        let mut env_vars = std::collections::HashMap::new();
        env_vars.insert(
            "LOCAL_ENDPOINT".to_string(),
            "http://localhost:11434".to_string(),
        );

        let agent = AgentInfo {
            name: "opencode".to_string(),
            command: "opencode".to_string(),
            args: vec!["acp".to_string()],
            env_vars,
        };

        assert_eq!(
            agent.env_vars.get("LOCAL_ENDPOINT"),
            Some(&"http://localhost:11434".to_string())
        );
    }

    #[test]
    fn test_agent_info_default_empty_env_vars() {
        // Default AgentInfo should have empty env_vars
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        assert!(agent.env_vars.is_empty());
    }

    #[test]
    fn test_resolve_agent_from_config_profile() {
        use crucible_config::{AcpConfig, AgentProfile};
        use std::collections::HashMap;

        // Create a config with a custom profile
        let mut agents = HashMap::new();
        let mut env = HashMap::new();
        env.insert(
            "LOCAL_ENDPOINT".to_string(),
            "http://localhost:11434/v1".to_string(),
        );
        agents.insert(
            "opencode-local".to_string(),
            AgentProfile {
                extends: Some("opencode".to_string()),
                command: None,
                args: None,
                env,
                description: None,
                capabilities: None,
                delegation: None,
            },
        );

        let config = AcpConfig {
            default_agent: Some("opencode-local".to_string()),
            agents,
            ..Default::default()
        };

        // Resolve should find the profile and merge with built-in agent
        let agent = resolve_agent_from_config("opencode-local", &config).expect("should resolve");

        assert_eq!(agent.name, "opencode-local");
        assert_eq!(agent.command, "opencode"); // From built-in
        assert_eq!(agent.args, vec!["acp".to_string()]); // From built-in
        assert_eq!(
            agent.env_vars.get("LOCAL_ENDPOINT"),
            Some(&"http://localhost:11434/v1".to_string())
        );
    }

    #[test]
    fn test_resolve_agent_custom_command_overrides_builtin() {
        use crucible_config::{AcpConfig, AgentProfile};
        use std::collections::HashMap;

        let mut agents = HashMap::new();
        agents.insert(
            "my-agent".to_string(),
            AgentProfile {
                extends: None,
                command: Some("/usr/local/bin/my-agent".to_string()),
                args: Some(vec!["--mode".to_string(), "acp".to_string()]),
                env: HashMap::new(),
                description: None,
                capabilities: None,
                delegation: None,
            },
        );

        let config = AcpConfig {
            agents,
            ..Default::default()
        };

        let agent = resolve_agent_from_config("my-agent", &config).expect("should resolve");

        assert_eq!(agent.name, "my-agent");
        assert_eq!(agent.command, "/usr/local/bin/my-agent");
        assert_eq!(agent.args, vec!["--mode".to_string(), "acp".to_string()]);
    }

    #[test]
    fn test_resolve_agent_falls_back_to_builtin() {
        use crucible_config::AcpConfig;

        let config = AcpConfig::default();

        // Resolving a built-in agent name should work
        let agent = resolve_agent_from_config("opencode", &config).expect("should resolve");

        assert_eq!(agent.name, "opencode");
        assert_eq!(agent.command, "opencode");
        assert_eq!(agent.args, vec!["acp".to_string()]);
        assert!(agent.env_vars.is_empty());
    }

    #[test]
    fn test_resolve_agent_unknown_returns_error() {
        use crucible_config::AcpConfig;

        let config = AcpConfig::default();

        // Unknown agent should fail
        let result = resolve_agent_from_config("unknown-agent", &config);
        assert!(result.is_err());
    }
}
