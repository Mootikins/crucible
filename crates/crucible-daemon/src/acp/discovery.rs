//! Agent Discovery and Management
//!
//! Handles discovering and spawning ACP-compatible agents.
//! Uses parallel probing for fast agent discovery.

use anyhow::{anyhow, Result};
use crucible_core::config::{AcpConfig, AgentProfile};
use futures::future::join_all;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::process::Command;
use tracing::{debug, info, trace, warn};

/// Timeout for agent availability checks (ms)
const PROBE_TIMEOUT_MS: u64 = 2000;

/// Cache for discovered agent to avoid repeated probing on subsequent calls
// KNOWN LIMITATION: AGENT_CACHE global still exists as a process-wide cache.
// Config-driven discovery (via AcpConfig) is an overlay on top of this cache, not a replacement.
// The cache is cleared between tests but persists across discovery calls in production.
// Future versions could make caching configurable or use session-scoped discovery caches.
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

/// A built-in agent: everything Crucible knows about it, in one place.
///
/// Single source of truth for the default profiles, the discovery priority
/// order (array order), and the install instructions rendered by the "no agent
/// found" error. It used to be three parallel tables,
/// which is how `opencode` spent several releases pointing at an unrelated
/// project and how two different descriptions of the same agent drifted apart.
struct BuiltinAgent {
    name: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    description: &'static str,
    /// Base CLI this agent bridges to, or `None` if it is standalone.
    requires: Option<&'static str>,
    install: &'static str,
}

/// The Antigravity ACP server executable, per platform.
///
/// The names come from the ACP registry entry `antigravity-acp`
/// (github.com/agentclientprotocol/registry). Windows ships a `.exe`; Linux
/// and macOS ship the same `.par` file.
#[cfg(windows)]
const ANTIGRAVITY_COMMAND: &str = "agy_acp_server.exe";
#[cfg(not(windows))]
const ANTIGRAVITY_COMMAND: &str = "agy_acp_server.par";

/// The arguments of the Antigravity ACP server, per platform.
///
/// The empty value of `--uid=` is deliberate. Without the argument the
/// binary's InitGoogle start-up code defaults to `--uid=nobody` and aborts on
/// Debian and Ubuntu (registry issue #607). The registry declares the argument
/// for Linux only. Zed and acpx pass the list unchanged.
#[cfg(target_os = "linux")]
const ANTIGRAVITY_ARGS: &[&str] = &["--uid="];
#[cfg(not(target_os = "linux"))]
const ANTIGRAVITY_ARGS: &[&str] = &[];

const BUILTIN_AGENTS: &[BuiltinAgent] = &[
    BuiltinAgent {
        name: "opencode",
        command: "opencode",
        args: &["acp"],
        description: "Standalone ACP agent — https://opencode.ai",
        requires: None,
        install: "npm install -g opencode-ai@latest",
    },
    BuiltinAgent {
        name: "claude",
        command: "npx",
        args: &["@agentclientprotocol/claude-agent-acp"],
        description: "Bridge to Claude Code",
        requires: Some("Claude Code CLI"),
        install: "npm install -g @agentclientprotocol/claude-agent-acp",
    },
    BuiltinAgent {
        name: "gemini",
        command: "gemini",
        // Without `--acp`, Gemini CLI starts its interactive UI and never
        // answers `initialize`. `--experimental-acp` is the deprecated name.
        args: &["--acp"],
        description: "Google's Gemini CLI, speaks ACP directly",
        requires: None,
        install: "npm install -g @google/gemini-cli",
    },
    BuiltinAgent {
        name: "codex",
        command: "npx",
        args: &["@agentclientprotocol/codex-acp"],
        description: "Bridge to OpenAI Codex",
        requires: Some("OpenAI Codex CLI"),
        install: "npm install -g @agentclientprotocol/codex-acp",
    },
    BuiltinAgent {
        name: "cursor",
        command: "cursor-agent",
        args: &["acp"],
        description: "Cursor's CLI, speaks ACP directly",
        // `cursor-agent acp` is a subcommand of the Cursor CLI, so the agent
        // is standalone. The npm package `cursor-acp` this used to name is an
        // unrelated third-party bridge, abandoned at 0.1.0.
        requires: None,
        install: "curl https://cursor.com/install -fsS | bash",
    },
    BuiltinAgent {
        name: "hermes",
        command: "hermes",
        args: &["acp"],
        description: "Nous Research Hermes agent, speaks ACP directly",
        // The `acp` subcommand ships inside the Hermes CLI, so the agent
        // is standalone. The install line comes from the Hermes README at
        // github.com/NousResearch/hermes-agent.
        requires: None,
        install: "curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash",
    },
    BuiltinAgent {
        name: "antigravity",
        command: ANTIGRAVITY_COMMAND,
        args: ANTIGRAVITY_ARGS,
        description: "Google Antigravity ACP server, speaks ACP directly",
        // The server ships inside the Antigravity extension archive, so the
        // agent is standalone.
        requires: None,
        install: "download the archive for your platform from \
                  https://dl.google.com/agy-extensions/releases/ , then put \
                  the server on PATH (ACP registry entry: antigravity-acp)",
    },
];

/// Is `name` one of the agents Crucible ships a profile for?
pub fn is_builtin(name: &str) -> bool {
    BUILTIN_AGENTS.iter().any(|agent| agent.name == name)
}

/// The profile Crucible ships for the built-in `name`.
fn builtin_profile(name: &str) -> Option<AgentProfile> {
    BUILTIN_AGENTS
        .iter()
        .find(|agent| agent.name == name)
        .map(|agent| AgentProfile {
            command: Some(agent.command.to_string()),
            args: Some(agent.args.iter().map(|s| s.to_string()).collect()),
            description: Some(agent.description.to_string()),
            ..Default::default()
        })
}

/// The built-in names, in discovery order, as one comma-separated list.
fn builtin_names() -> String {
    BUILTIN_AGENTS
        .iter()
        .map(|agent| agent.name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The error for a name that neither a built-in nor `[acp.agents]` defines.
pub(crate) fn unknown_agent(name: &str, config: &AcpConfig) -> anyhow::Error {
    anyhow!(
        "Unknown ACP agent '{name}'. Known agents: {}. Name a built-in, or define \
         `[acp.agents.{name}]` with a `command`.",
        agent_names(config).join(", ")
    )
}

/// The profile of the agent `name`, or `None` when nothing defines it.
///
/// This is the one place that decides what a profile means, and every caller
/// goes through it: the launcher, discovery, the agent list and the permission
/// lookup. There are exactly two kinds of profile.
///
/// 1. `name` is a built-in. The built-in supplies command, arguments and
///    description; an `[acp.agents.<name>]` entry lays its own fields over it.
/// 2. `name` is anything else. The `[acp.agents.<name>]` entry must define
///    `command`, because Crucible has nothing else to run.
///
/// A profile never inherits from another profile. The removed `extends` key
/// produces an error that names it.
///
/// Error policy: a caller that names a profile gets an error for that profile.
/// [`profiles`] sweeps every name and skips a failing one with a warning, so a
/// single broken entry cannot hide every other agent.
pub fn profile(name: &str, config: &AcpConfig) -> Result<Option<AgentProfile>> {
    let builtin = builtin_profile(name);
    let Some(configured) = config.agents.get(name) else {
        return Ok(builtin);
    };

    if let Some(base) = &configured.removed_extends {
        return Err(anyhow!(
            "Agent profile '{name}' sets `extends = \"{base}\"`, which Crucible removed. \
             A profile no longer inherits. To adjust the built-in '{base}', move these \
             fields into `[acp.agents.{base}]`; to keep the name '{name}', give it its \
             own `command` and `args`."
        ));
    }

    let mut resolved = builtin.unwrap_or_default();
    if let Some(command) = &configured.command {
        resolved.command = Some(command.clone());
    }
    if let Some(args) = &configured.args {
        resolved.args = Some(args.clone());
    }
    if let Some(description) = &configured.description {
        resolved.description = Some(description.clone());
    }
    if let Some(delegation) = &configured.delegation {
        resolved.delegation = Some(delegation.clone());
    }
    if let Some(permissions) = &configured.permissions {
        resolved.permissions = Some(permissions.clone());
    }
    resolved.env.extend(configured.env.clone());

    if resolved.command.is_none() {
        return Err(anyhow!(
            "Agent profile '{name}' must define `command`. Only a built-in name ({}) \
             takes its command from Crucible.",
            builtin_names()
        ));
    }

    Ok(Some(resolved))
}

/// Every agent name Crucible knows: the built-ins in discovery order, then
/// the configured names that are not built-in, sorted.
pub fn agent_names(config: &AcpConfig) -> Vec<String> {
    let mut names: Vec<String> = BUILTIN_AGENTS
        .iter()
        .map(|agent| agent.name.to_string())
        .collect();

    let mut custom: Vec<String> = config
        .agents
        .keys()
        .filter(|name| !is_builtin(name))
        .cloned()
        .collect();
    custom.sort();
    names.extend(custom);

    names
}

/// Every profile that resolves, in discovery order.
///
/// A profile that does not resolve is skipped with a warning. One broken
/// `[acp.agents]` entry must not hide every other agent from the picker or
/// from discovery; a caller that asks for that entry by name still gets the
/// error, from [`profile`].
pub fn profiles(config: &AcpConfig) -> Vec<(String, AgentProfile)> {
    agent_names(config)
        .into_iter()
        .filter_map(|name| match profile(&name, config) {
            Ok(resolved) => resolved.map(|resolved| (name, resolved)),
            Err(error) => {
                warn!(agent = %name, %error, "skipping unusable ACP agent profile");
                None
            }
        })
        .collect()
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
        env_vars: profile.env.clone().into_iter().collect(),
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
        if let Some(cached) = AGENT_CACHE
            .lock()
            .expect("AGENT_CACHE: poisoned while reading cached agent")
            .clone()
        {
            trace!("Using cached agent: {}", cached.name);
            return Ok(cached);
        }
    }

    let agent = discover_agent_uncached(preferred, acp_config).await?;
    *AGENT_CACHE
        .lock()
        .expect("AGENT_CACHE: poisoned while caching discovered agent") = Some(agent.clone());
    Ok(agent)
}

/// Cache-free discovery core: resolves profiles, probes availability, and returns
/// the first available agent in priority order. Unlike [`discover_agent`] it does
/// NOT read or write the process-global `AGENT_CACHE`, so parallel tests can call
/// it without cross-test state bleeding.
pub async fn discover_agent_uncached(
    preferred: Option<&str>,
    acp_config: &AcpConfig,
) -> Result<AgentInfo> {
    let resolved = profiles(acp_config);

    // If a preferred agent is specified, check it first (single probe)
    if let Some(agent_name) = preferred {
        debug!("Trying preferred agent: {}", agent_name);
        if let Some((_, profile)) = resolved.iter().find(|(name, _)| name == agent_name) {
            if let Some(cmd) = profile.command.as_deref() {
                if is_agent_available(cmd).await {
                    info!("Using preferred agent: {}", agent_name);
                    return profile_to_agent_info(agent_name, profile);
                }
            }
        }
        warn!(
            "Preferred agent '{}' not found, trying fallbacks",
            agent_name
        );
    }

    // Parallel probe: check all agents concurrently
    debug!("Probing {} agents in parallel", resolved.len());
    let start = std::time::Instant::now();

    let futures: Vec<_> = resolved
        .iter()
        .filter_map(|(name, profile)| {
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
        .collect();

    let results = join_all(futures).await;
    debug!("Parallel probe completed in {:?}", start.elapsed());

    // Find first available agent (maintaining priority order)
    for (name, profile, available) in results {
        if available {
            info!("Discovered agent: {}", name);
            return profile_to_agent_info(&name, &profile);
        }
    }

    // None found - provide helpful error message
    Err(anyhow!(
        "No compatible ACP agent found.\n\
         \n\
         Standalone agents:\n\
         {}\
         \n\
         Bridge agents (require the base CLI as well):\n\
         {}\
         \n\
         After installation, use with:\n\
         cru chat --agent <agent> \"your message\"\n\
         \n\
         For another agent, define `[acp.agents.<name>]` with a `command`, \
         then pass that name.",
        install_lines(|agent| agent.requires.is_none()),
        install_lines(|agent| agent.requires.is_some()),
    ))
}

/// Render `• <name>: <install>` lines for the agents matching `keep`.
fn install_lines(keep: impl Fn(&BuiltinAgent) -> bool) -> String {
    BUILTIN_AGENTS
        .iter()
        .filter(|agent| keep(agent))
        .map(|agent| format!("• {}: {}\n", agent.name, agent.install))
        .collect()
}

/// Clear the agent cache to prevent state bleeding across tests.
/// Call this in test teardown to reset the global AGENT_CACHE.
#[cfg(any(test, feature = "test-utils"))]
pub fn reset_agent_cache() {
    *AGENT_CACHE
        .lock()
        .expect("AGENT_CACHE: poisoned while clearing agent cache") = None;
}

/// Commands that should trust PATH lookup without --version verification.
/// These are either:
/// - Package managers (npx) that handle resolution themselves
/// - ACP agents that start servers and don't support --version
const TRUST_PATH_COMMANDS: &[&str] = &[
    "npx",    // Package manager, verifies packages at runtime
    "gemini", // ACP server, no --version support
];

/// Check if an agent command is available (async, non-blocking)
///
/// Uses a two-phase approach for speed:
/// 1. Fast check with `which` to see if command exists in PATH
/// 2. Only if found, verify with `--version` (with timeout)
///
/// For certain commands (npx, gemini-cli), we skip the --version check
/// since they either handle verification at runtime or don't support
/// --version.
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

/// The command, arguments and environment of the agent `name`.
///
/// A thin wrapper over [`profile`] for the callers that want the launch shape
/// rather than the profile. An unknown name is an error, never a command.
pub fn resolve_agent_from_config(name: &str, config: &AcpConfig) -> Result<AgentInfo> {
    match profile(name, config)? {
        Some(resolved) => profile_to_agent_info(name, &resolved),
        None => Err(unknown_agent(name, config)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

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

    // Touches the process-global AGENT_CACHE; serialized so it can't race the
    // other cache test.
    #[tokio::test]
    #[serial(agent_cache)]
    async fn test_agent_cache_operations() {
        // Clear cache first
        reset_agent_cache();
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
        reset_agent_cache();
        assert!(AGENT_CACHE.lock().unwrap().is_none());
    }

    // Touches the process-global AGENT_CACHE; serialized so the cached value
    // can't be cleared by a concurrent cache test.
    #[tokio::test]
    #[serial(agent_cache)]
    async fn test_discover_agent_uses_cache() {
        // Pre-populate cache with a fake agent
        reset_agent_cache();
        *AGENT_CACHE.lock().unwrap() = Some(AgentInfo {
            name: "cached-agent".to_string(),
            command: "cached-cmd".to_string(),
            args: vec![],
            env_vars: HashMap::new(),
        });

        // Discovery with no preference must return the cached agent without probing.
        let start = std::time::Instant::now();
        let config = empty_config();
        let agent = discover_agent(None, &config)
            .await
            .expect("cached agent should be returned");
        let elapsed = start.elapsed();

        assert_eq!(agent.name, "cached-agent", "should return the cached agent");
        assert!(
            elapsed.as_millis() < 50,
            "cache lookup took too long: {:?}",
            elapsed
        );

        // Clean up
        reset_agent_cache();
    }

    #[tokio::test]
    async fn test_discover_agent_parallel_probe_is_fast() {
        // Parallel probe should complete quickly even with multiple agents
        // because non-existent commands fail fast via `which`. Uses the
        // cache-free core so it neither reads nor writes the global cache.
        let start = std::time::Instant::now();
        let config = empty_config();
        let _result = discover_agent_uncached(None, &config).await;
        let elapsed = start.elapsed();

        // Should complete within timeout + some margin
        // Even in worst case (all agents timeout), should be < PROBE_TIMEOUT_MS + overhead
        // since probes run in parallel
        assert!(
            elapsed.as_millis() < (PROBE_TIMEOUT_MS as u128) + 500,
            "Parallel probe took too long: {:?}",
            elapsed
        );
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

    /// A profile named after a built-in lays its own fields over that
    /// built-in. This is the only way to reach a built-in's command.
    #[test]
    fn a_profile_named_after_a_builtin_overlays_it() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "LOCAL_ENDPOINT".to_string(),
            "http://localhost:11434/v1".to_string(),
        );
        let mut agents = std::collections::BTreeMap::new();
        agents.insert(
            "opencode".to_string(),
            AgentProfile {
                env,
                ..Default::default()
            },
        );

        let config = AcpConfig {
            agents,
            ..Default::default()
        };

        let agent = resolve_agent_from_config("opencode", &config).expect("should resolve");

        assert_eq!(agent.name, "opencode");
        assert_eq!(agent.command, "opencode");
        assert_eq!(agent.args, vec!["acp".to_string()]);
        assert_eq!(
            agent.env_vars.get("LOCAL_ENDPOINT"),
            Some(&"http://localhost:11434/v1".to_string())
        );
    }

    /// A profile whose name is not a built-in must define `command`.
    #[test]
    fn a_profile_that_is_not_a_builtin_must_define_a_command() {
        let mut agents = std::collections::BTreeMap::new();
        agents.insert("my-agent".to_string(), AgentProfile::default());

        let config = AcpConfig {
            agents,
            ..Default::default()
        };

        let message = profile("my-agent", &config)
            .expect_err("a profile with nothing to run is an error")
            .to_string();

        assert!(message.contains("'my-agent'"), "{message}");
        assert!(message.contains("command"), "{message}");
    }

    /// The removed `extends` key gets an error that names it. Serde would
    /// otherwise drop the key and the operator would read "must define
    /// `command`", which says nothing about what changed.
    #[test]
    fn the_removed_extends_key_is_an_error_that_names_it() {
        let mut agents = std::collections::BTreeMap::new();
        agents.insert(
            "my-claude".to_string(),
            AgentProfile {
                removed_extends: Some("claude".to_string()),
                ..Default::default()
            },
        );

        let config = AcpConfig {
            agents,
            ..Default::default()
        };

        let message = profile("my-claude", &config)
            .expect_err("inheritance is removed")
            .to_string();

        assert!(message.contains("extends"), "{message}");
        assert!(message.contains("my-claude"), "{message}");
        assert!(message.contains("claude"), "{message}");
    }

    /// One unusable entry must not hide every other agent. The sweep skips it;
    /// a caller that names it still gets the error from `profile`.
    #[test]
    fn one_broken_profile_does_not_hide_the_others() {
        let mut agents = std::collections::BTreeMap::new();
        agents.insert("broken".to_string(), AgentProfile::default());

        let config = AcpConfig {
            agents,
            ..Default::default()
        };

        let names: Vec<String> = profiles(&config).into_iter().map(|(n, _)| n).collect();

        assert!(names.contains(&"opencode".to_string()), "{names:?}");
        assert!(!names.contains(&"broken".to_string()), "{names:?}");
        assert!(profile("broken", &config).is_err());
    }

    #[test]
    fn test_resolve_agent_custom_command_overrides_builtin() {
        use crucible_core::config::{AcpConfig, AgentProfile};

        let mut agents = std::collections::BTreeMap::new();
        agents.insert(
            "my-agent".to_string(),
            AgentProfile {
                command: Some("/usr/local/bin/my-agent".to_string()),
                args: Some(vec!["--mode".to_string(), "acp".to_string()]),
                ..Default::default()
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
        use crucible_core::config::AcpConfig;

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
        use crucible_core::config::AcpConfig;

        let config = AcpConfig::default();

        // Unknown agent should fail
        let result = resolve_agent_from_config("unknown-agent", &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_default_agent_profiles_include_all_builtin_agents() {
        let names = agent_names(&AcpConfig::default());

        for name in [
            "opencode",
            "claude",
            "gemini",
            "codex",
            "cursor",
            "hermes",
            "antigravity",
        ] {
            assert!(names.iter().any(|n| n == name), "missing profile: {}", name);
        }
    }

    #[test]
    fn test_default_agent_profiles_have_command_args_and_description() {
        let profiles: HashMap<String, AgentProfile> =
            profiles(&AcpConfig::default()).into_iter().collect();

        for name in [
            "opencode",
            "claude",
            "gemini",
            "codex",
            "cursor",
            "hermes",
            "antigravity",
        ] {
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
        let mut agents = std::collections::BTreeMap::new();
        agents.insert(
            "opencode".to_string(),
            AgentProfile {
                command: Some("cargo".to_string()),
                args: None,
                ..Default::default()
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
    fn test_unknown_agent_error_is_helpful() {
        let config = AcpConfig::default();

        let err = resolve_agent_from_config("definitely-unknown", &config).unwrap_err();
        let message = err.to_string();

        assert!(message.contains("definitely-unknown"));
        assert!(message.contains("Known agents"));
    }

    /// The `antigravity` built-in must launch what the ACP registry entry
    /// `antigravity-acp` declares for this platform, argument for argument.
    /// Zed and acpx pass the same list.
    #[test]
    fn the_antigravity_builtin_matches_the_registry_entry() {
        let agent = resolve_agent_from_config("antigravity", &AcpConfig::default())
            .expect("antigravity is a built-in");

        #[cfg(windows)]
        assert_eq!(agent.command, "agy_acp_server.exe");
        #[cfg(not(windows))]
        assert_eq!(agent.command, "agy_acp_server.par");

        // On Linux the value is empty ON PURPOSE. Without the argument the
        // binary's InitGoogle start-up defaults to `--uid=nobody` and aborts
        // on Debian and Ubuntu (registry issue #607).
        #[cfg(target_os = "linux")]
        assert_eq!(agent.args, vec!["--uid=".to_string()]);
        #[cfg(not(target_os = "linux"))]
        assert!(agent.args.is_empty());
    }

    #[tokio::test]
    async fn test_discover_agent_preferred_uses_merged_profile_command() {
        let mut agents = std::collections::BTreeMap::new();
        agents.insert(
            "opencode".to_string(),
            AgentProfile {
                command: Some("cargo".to_string()),
                args: Some(vec!["--version".to_string()]),
                description: Some("Overridden".to_string()),
                ..Default::default()
            },
        );

        let config = AcpConfig {
            agents,
            ..Default::default()
        };

        let agent = discover_agent_uncached(Some("opencode"), &config)
            .await
            .expect("preferred profile should resolve");

        assert_eq!(agent.name, "opencode");
        assert_eq!(agent.command, "cargo");
        assert_eq!(agent.args, vec!["--version".to_string()]);
    }

    #[tokio::test]
    async fn test_discover_agent_without_preferred_can_use_config_only_profile() {
        let mut agents = std::collections::BTreeMap::new();
        agents.insert(
            "cargo-agent".to_string(),
            AgentProfile {
                command: Some("cargo".to_string()),
                args: Some(vec!["--version".to_string()]),
                description: Some("Cargo-backed profile".to_string()),
                ..Default::default()
            },
        );

        let config = AcpConfig {
            agents,
            ..Default::default()
        };

        let agent = discover_agent_uncached(None, &config)
            .await
            .expect("should discover from merged profiles");
        assert!([
            "cargo-agent",
            "opencode",
            "claude",
            "gemini",
            "codex",
            "cursor"
        ]
        .contains(&agent.name.as_str()));
    }
}
