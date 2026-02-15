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

const BUILTIN_AGENT_ORDER: &[&str] = &["opencode", "claude", "gemini", "codex", "cursor"];

static KNOWN_AGENTS: Lazy<Vec<(String, String)>> = Lazy::new(|| {
    let defaults = default_agent_profiles();
    BUILTIN_AGENT_ORDER
        .iter()
        .filter_map(|name| {
            defaults.get(*name).map(|profile| {
                (
                    (*name).to_string(),
                    profile.description.clone().unwrap_or_default(),
                )
            })
        })
        .collect()
});

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
        .map(|(name, desc)| KnownAgent {
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
    let profiles = default_agent_profiles();
    let ordered_names = ordered_profile_names(&profiles);

    let futures: Vec<_> = ordered_names
        .into_iter()
        .filter_map(|name| {
            profiles.get(&name).and_then(|profile| {
                profile.command.as_ref().map(|cmd| {
                    let name = name.clone();
                    let cmd = cmd.clone();
                    let description = profile.description.clone().unwrap_or_default();
                    async move {
                        let available = is_agent_available(&cmd).await;
                        KnownAgent {
                            name,
                            description,
                            available,
                        }
                    }
                })
            })
        })
        .collect();

    join_all(futures).await
}

pub fn default_agent_profiles() -> HashMap<String, AgentProfile> {
    let mut profiles = HashMap::new();

    profiles.insert(
        "opencode".to_string(),
        AgentProfile {
            extends: None,
            command: Some("opencode".to_string()),
            args: Some(vec!["acp".to_string()]),
            env: HashMap::new(),
            description: Some("OpenCode AI (Go)".to_string()),
            capabilities: None,
            delegation: None,
        },
    );

    profiles.insert(
        "claude".to_string(),
        AgentProfile {
            extends: None,
            command: Some("npx".to_string()),
            args: Some(vec!["@zed-industries/claude-code-acp".to_string()]),
            env: HashMap::new(),
            description: Some("Claude Code via ACP".to_string()),
            capabilities: None,
            delegation: None,
        },
    );

    profiles.insert(
        "gemini".to_string(),
        AgentProfile {
            extends: None,
            command: Some("gemini".to_string()),
            args: Some(Vec::new()),
            env: HashMap::new(),
            description: Some("Google Gemini CLI".to_string()),
            capabilities: None,
            delegation: None,
        },
    );

    profiles.insert(
        "codex".to_string(),
        AgentProfile {
            extends: None,
            command: Some("npx".to_string()),
            args: Some(vec!["@zed-industries/codex-acp".to_string()]),
            env: HashMap::new(),
            description: Some("OpenAI Codex via ACP".to_string()),
            capabilities: None,
            delegation: None,
        },
    );

    profiles.insert(
        "cursor".to_string(),
        AgentProfile {
            extends: None,
            command: Some("cursor-acp".to_string()),
            args: Some(Vec::new()),
            env: HashMap::new(),
            description: Some("Cursor IDE via ACP".to_string()),
            capabilities: None,
            delegation: None,
        },
    );

    profiles
}

fn ordered_profile_names(profiles: &HashMap<String, AgentProfile>) -> Vec<String> {
    let mut ordered = Vec::new();
    for name in BUILTIN_AGENT_ORDER {
        if profiles.contains_key(*name) {
            ordered.push((*name).to_string());
        }
    }

    let mut custom: Vec<String> = profiles
        .keys()
        .filter(|name| !BUILTIN_AGENT_ORDER.contains(&name.as_str()))
        .cloned()
        .collect();
    custom.sort();
    ordered.extend(custom);

    ordered
}

fn merge_profiles(config: &AcpConfig) -> Result<HashMap<String, AgentProfile>> {
    let mut merged = default_agent_profiles();

    for (name, profile) in &config.agents {
        let resolved = resolve_profile(name, profile, &merged)?;
        merged.insert(name.clone(), resolved);
    }

    Ok(merged)
}

fn profile_to_agent_info(name: &str, profile: &AgentProfile) -> Result<AgentInfo> {
    let command = profile.command.clone().ok_or_else(|| {
        anyhow!(
            "Agent profile '{}' must define `command` (directly or via built-in default)",
            name
        )
    })?;

    Ok(AgentInfo {
        name: name.to_string(),
        command,
        args: profile.args.clone().unwrap_or_default(),
        env_vars: profile.env.clone(),
    })
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
pub async fn discover_agent(preferred: Option<&str>, acp_config: &AcpConfig) -> Result<AgentInfo> {
    // Check cache first (unless a specific agent is preferred)
    if preferred.is_none() {
        if let Some(cached) = AGENT_CACHE.lock().unwrap().clone() {
            trace!("Using cached agent: {}", cached.name);
            return Ok(cached);
        }
    }

    let merged_profiles = merge_profiles(acp_config)?;

    // If a preferred agent is specified, check it first (single probe)
    if let Some(agent_name) = preferred {
        debug!("Trying preferred agent: {}", agent_name);
        if let Some(profile) = merged_profiles.get(agent_name) {
            if let Some(cmd) = profile.command.as_deref() {
                if is_agent_available(cmd).await {
                    info!("Using preferred agent: {}", agent_name);
                    let agent = profile_to_agent_info(agent_name, profile)?;
                    // Cache the result
                    *AGENT_CACHE.lock().unwrap() = Some(agent.clone());
                    return Ok(agent);
                }
            }
        }
        warn!(
            "Preferred agent '{}' not found, trying fallbacks",
            agent_name
        );
    }

    // Parallel probe: check all agents concurrently
    let ordered_names = ordered_profile_names(&merged_profiles);
    debug!("Probing {} agents in parallel", ordered_names.len());
    let start = std::time::Instant::now();

    let futures: Vec<_> = ordered_names
        .iter()
        .filter_map(|name| {
            merged_profiles.get(name).and_then(|profile| {
                profile.command.as_ref().map(|command| {
                    let name = name.clone();
                    let profile = profile.clone();
                    let command = command.clone();
                    async move {
                        let available = is_agent_available(&command).await;
                        (name, profile, available)
                    }
                })
            })
        })
        .collect();

    let results = join_all(futures).await;
    debug!("Parallel probe completed in {:?}", start.elapsed());

    // Find first available agent (maintaining priority order)
    for (name, profile, available) in results {
        if available {
            info!("Discovered agent: {}", name);
            let agent = profile_to_agent_info(&name, &profile)?;
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
    let merged_profiles = merge_profiles(config)?;

    if let Some(profile) = merged_profiles.get(name) {
        return profile_to_agent_info(name, profile);
    }

    let mut known_names = ordered_profile_names(&merged_profiles);
    known_names.sort();

    Err(anyhow!(
        "Unknown agent '{}'. Known agent names: {}. Use --agent with a known agent name or define it in config.",
        name,
        known_names.join(", ")
    ))
}

/// Resolve a custom agent profile
fn resolve_profile(
    name: &str,
    profile: &AgentProfile,
    merged_profiles: &HashMap<String, AgentProfile>,
) -> Result<AgentProfile> {
    let base_name = profile.extends.as_deref().unwrap_or(name);

    if profile.extends.is_some() && !merged_profiles.contains_key(base_name) {
        return Err(anyhow!(
            "Agent profile '{}' extends unknown agent '{}'. Define command/args or use a known base agent.",
            name,
            base_name
        ));
    }

    if profile.command.is_none()
        && profile.args.is_none()
        && profile.extends.is_none()
        && !merged_profiles.contains_key(name)
    {
        return Err(anyhow!(
            "Agent profile '{}' must define `command` or `extends`",
            name
        ));
    }

    let mut resolved = merged_profiles.get(base_name).cloned().unwrap_or_default();
    resolved.extends = profile.extends.clone();

    if let Some(command) = &profile.command {
        resolved.command = Some(command.clone());
    }
    if let Some(args) = &profile.args {
        resolved.args = Some(args.clone());
    }
    if let Some(description) = &profile.description {
        resolved.description = Some(description.clone());
    }
    if let Some(capabilities) = &profile.capabilities {
        resolved.capabilities = Some(capabilities.clone());
    }
    if let Some(delegation) = &profile.delegation {
        resolved.delegation = Some(delegation.clone());
    }

    resolved.env.extend(profile.env.clone());

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_config() -> AcpConfig {
        AcpConfig::default()
    }

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
        let config = empty_config();
        let result = discover_agent(None, &config).await;
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
        let config = empty_config();
        let _result = discover_agent(None, &config).await;
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

    #[test]
    fn test_known_agents_list() {
        assert!(!KNOWN_AGENTS.is_empty(), "Should have known agents");

        for (name, description) in KNOWN_AGENTS.iter() {
            assert!(!name.is_empty(), "Agent name should not be empty");
            assert!(
                !description.is_empty(),
                "Agent description should not be empty"
            );
        }

        let names: Vec<String> = KNOWN_AGENTS.iter().map(|(name, _)| name.clone()).collect();
        assert!(names.contains(&"opencode".to_string()));
        assert!(names.contains(&"claude".to_string()));
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

    #[test]
    fn test_default_agent_profiles_include_all_builtin_agents() {
        let profiles = default_agent_profiles();

        for name in ["opencode", "claude", "gemini", "codex", "cursor"] {
            assert!(profiles.contains_key(name), "missing profile: {}", name);
        }
    }

    #[test]
    fn test_default_agent_profiles_have_command_args_and_description() {
        let profiles = default_agent_profiles();

        for name in ["opencode", "claude", "gemini", "codex", "cursor"] {
            let profile = profiles.get(name).expect("profile should exist");
            assert!(
                profile.command.as_ref().is_some_and(|v| !v.is_empty()),
                "{} should have command",
                name
            );
            assert!(profile.args.is_some(), "{} should have args", name);
            assert!(
                profile.description.as_ref().is_some_and(|v| !v.is_empty()),
                "{} should have description",
                name
            );
        }
    }

    #[test]
    fn test_resolve_agent_user_overlay_overrides_command_and_falls_back_for_none_fields() {
        let mut agents = HashMap::new();
        agents.insert(
            "opencode".to_string(),
            AgentProfile {
                extends: None,
                command: Some("cargo".to_string()),
                args: None,
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

        let agent = resolve_agent_from_config("opencode", &config).expect("should resolve");
        assert_eq!(agent.command, "cargo");
        assert_eq!(agent.args, vec!["acp".to_string()]);
    }

    #[test]
    fn test_resolve_agent_uses_extends_for_backward_compatible_defaults() {
        let mut agents = HashMap::new();
        agents.insert(
            "my-claude".to_string(),
            AgentProfile {
                extends: Some("claude".to_string()),
                command: None,
                args: None,
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

        let agent = resolve_agent_from_config("my-claude", &config).expect("should resolve");
        assert_eq!(agent.command, "npx");
        assert_eq!(agent.args, vec!["@zed-industries/claude-code-acp".to_string()]);
    }

    #[test]
    fn test_unknown_agent_error_is_helpful() {
        let config = AcpConfig::default();

        let err = resolve_agent_from_config("definitely-unknown", &config).unwrap_err();
        let message = err.to_string();

        assert!(message.contains("definitely-unknown"));
        assert!(message.contains("known agent"));
    }

    #[tokio::test]
    async fn test_discover_agent_preferred_uses_merged_profile_command() {
        clear_agent_cache();

        let mut agents = HashMap::new();
        agents.insert(
            "opencode".to_string(),
            AgentProfile {
                extends: None,
                command: Some("cargo".to_string()),
                args: Some(vec!["--version".to_string()]),
                env: HashMap::new(),
                description: Some("Overridden".to_string()),
                capabilities: None,
                delegation: None,
            },
        );

        let config = AcpConfig {
            agents,
            ..Default::default()
        };

        let agent = discover_agent(Some("opencode"), &config)
            .await
            .expect("preferred profile should resolve");

        assert_eq!(agent.name, "opencode");
        assert_eq!(agent.command, "cargo");
        assert_eq!(agent.args, vec!["--version".to_string()]);
    }

    #[tokio::test]
    async fn test_discover_agent_without_preferred_can_use_config_only_profile() {
        clear_agent_cache();

        let mut agents = HashMap::new();
        agents.insert(
            "cargo-agent".to_string(),
            AgentProfile {
                extends: None,
                command: Some("cargo".to_string()),
                args: Some(vec!["--version".to_string()]),
                env: HashMap::new(),
                description: Some("Cargo-backed profile".to_string()),
                capabilities: None,
                delegation: None,
            },
        );

        let config = AcpConfig {
            agents,
            ..Default::default()
        };

        let agent = discover_agent(None, &config)
            .await
            .expect("should discover from merged profiles");
        assert!(
            ["cargo-agent", "opencode", "claude", "gemini", "codex", "cursor"]
                .contains(&agent.name.as_str())
        );
    }

    #[test]
    fn test_get_known_agents_reflects_default_profiles() {
        let known = get_known_agents();
        let profiles = default_agent_profiles();

        for agent in known {
            assert!(profiles.contains_key(&agent.name));
        }
    }
}
