//! Turning a `SessionAgent` into the process to launch.
//!
//! Split from `acp_handle.rs` for the 1000-line budget, along a real seam:
//! everything here answers "what command, with what arguments, where" — and
//! nothing here knows about the ACP protocol, the handle, or a live session.
//!
//! It is also where a sandboxed session's agent gets relocated into its
//! container, which is the one place that decision can be made once for every
//! agent rather than per known agent name.

use crate::acp::client::ClientConfig;
use crate::acp_handle::AcpHandleError;
use crucible_core::config::components::acp::AcpConfig;
use crucible_core::session::SessionAgent;
use crucible_lua::SandboxExec;
use std::path::{Path, PathBuf};

/// Build a `ClientConfig` from `SessionAgent` fields.
///
/// Maps agent_name to command/args via discovery, merges env_overrides,
/// and applies ACP config timeouts.
pub(crate) fn build_client_config(
    agent_config: &SessionAgent,
    workspace: &Path,
    acp_config: Option<&AcpConfig>,
    sandbox_exec: Option<&SandboxExec>,
) -> Result<ClientConfig, AcpHandleError> {
    let agent_name = agent_config.agent_name.as_deref().unwrap_or("acp");

    let (command, args, mut env_vars) =
        resolve_agent_command(agent_name, agent_config, acp_config)?;

    // A sandboxed session runs the agent INSIDE its container rather than
    // beside it. That is what makes an external agent's isolation enforceable
    // at all: the daemon cannot intercept tools an ACP agent runs in its own
    // process, but it does not have to when that process is already confined.
    //
    // Prepending argv rather than building a shell string keeps the agent's own
    // arguments unquoted and unsplit — `npx @zed-industries/claude-agent-acp`
    // survives as two argv entries, not as something a shell re-parses.
    let (command, args) = match sandbox_exec {
        Some(exec) if !exec.is_empty() => {
            let mut argv: Vec<String> = exec.prefix[1..].to_vec();
            // Environment is delivered on the launcher's own argv, because the
            // spawner's `cmd.env` would set it on the launcher process and the
            // container boundary drops it there — an API key configured for the
            // agent would never reach the agent. The flags go between prefix and
            // suffix because a launcher's flags must precede its operand.
            argv.extend(sandbox_env_argv(exec, &env_vars, agent_name)?);
            // Delivered inside; leaving a copy on the launcher process would be
            // a second, meaningless delivery of the same secret.
            env_vars.clear();
            argv.extend(exec.suffix.iter().cloned());
            argv.push(command);
            argv.extend(args);
            (exec.prefix[0].clone(), argv)
        }
        _ => (command, args),
    };

    // Protocol layer multiplies timeout_ms by 10, so: minutes * 60_000 / 10 = minutes * 6000
    let timeout_ms = acp_config
        .map(|c| c.streaming_timeout_minutes * 6000)
        .unwrap_or(90_000);

    Ok(ClientConfig {
        agent_path: PathBuf::from(command),
        agent_args: if args.is_empty() { None } else { Some(args) },
        working_dir: Some(workspace.to_path_buf()),
        env_vars: if env_vars.is_empty() {
            None
        } else {
            Some(env_vars)
        },
        timeout_ms: Some(timeout_ms),
        max_retries: None,
    })
}

/// The argv that carries `env_vars` across the sandbox boundary.
///
/// `NAME=VALUE`, with or without a preceding flag depending on what the
/// launcher takes: the one form podman, docker and nerdctl all accept for
/// `exec`, and the one `env(1)` takes for `ssh`. It does put the value on the
/// launcher's command line, where `ps` can read it — the alternative, a bare
/// `-e NAME` inherited from the launcher's own environment, hides it but is
/// only reliably supported by podman.
///
/// A plugin that described no way in cannot deliver environment at all, and the
/// launch is refused naming the variables: an agent started without its API
/// key fails later, somewhere else, with nothing pointing back to the sandbox.
fn sandbox_env_argv(
    exec: &SandboxExec,
    env_vars: &[(String, String)],
    agent_name: &str,
) -> Result<Vec<String>, AcpHandleError> {
    if env_vars.is_empty() {
        return Ok(Vec::new());
    }
    let mut argv = Vec::with_capacity(env_vars.len() * 2);
    for (name, value) in env_vars {
        let Some(pair) = exec.env.argv_for(name, value) else {
            let names: Vec<&str> = env_vars.iter().map(|(k, _)| k.as_str()).collect();
            return Err(AcpHandleError::Config(format!(
                "agent '{agent_name}' is configured with environment ({}) but this session's \
                 sandbox offers no way to pass it in, so the agent would start without it; \
                 the claiming plugin must supply `exec_env_flag` or `exec_env_inline`",
                names.join(", ")
            )));
        };
        argv.extend(pair);
    }
    Ok(argv)
}

/// Resolved command, arguments, and environment variables for an ACP agent.
type ResolvedCommand = (String, Vec<String>, Vec<(String, String)>);

/// Resolve agent name to (command, args, env_vars) using known agents list
/// and any profile overrides from AcpConfig.
fn resolve_agent_command(
    agent_name: &str,
    agent_config: &SessionAgent,
    acp_config: Option<&AcpConfig>,
) -> Result<ResolvedCommand, AcpHandleError> {
    let known: &[(&str, &str, &[&str])] = &[
        ("opencode", "opencode", &["acp"]),
        ("claude", "npx", &["@zed-industries/claude-agent-acp"]),
        ("gemini", "gemini", &[]),
        ("codex", "npx", &["@zed-industries/codex-acp"]),
        ("cursor", "cursor-acp", &[]),
    ];

    let (mut command, mut args) = known
        .iter()
        .find(|(name, _, _)| *name == agent_name)
        .map(|(_, cmd, ag)| (cmd.to_string(), ag.iter().map(|s| s.to_string()).collect()))
        .unwrap_or_else(|| (agent_name.to_string(), Vec::new()));

    if let Some(config) = acp_config {
        if let Some(profile) = config.agents.get(agent_name) {
            if let Some(ref cmd) = profile.command {
                command = cmd.clone();
            }
            if let Some(ref profile_args) = profile.args {
                args = profile_args.clone();
            }
        }
    }

    let mut env_vars: Vec<(String, String)> = agent_config
        .env_overrides
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    if let Some(config) = acp_config {
        if let Some(profile) = config.agents.get(agent_name) {
            for (k, v) in &profile.env {
                if let Some(existing) = env_vars.iter_mut().find(|(ek, _)| ek == k) {
                    existing.1 = v.clone();
                } else {
                    env_vars.push((k.clone(), v.clone()));
                }
            }
        }
    }

    Ok((command, args, env_vars))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::config::BackendType;
    use std::collections::HashMap;

    /// A sandbox the way `oci` describes one: launcher flags, the hole the env
    /// flags fill, then the container name.
    fn sandbox(env_flag: Option<&str>) -> SandboxExec {
        SandboxExec {
            prefix: ["podman", "exec", "-i", "-w", "/workspace"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            env: match env_flag {
                Some(flag) => crucible_lua::SandboxEnv::Flag(flag.to_string()),
                None => crucible_lua::SandboxEnv::Unsupported,
            },
            suffix: vec!["crucible-s1".to_string()],
        }
    }

    /// A sandbox the way `ssh` describes one: no per-variable flag, `env(1)`
    /// as the last word of the prefix, and no operand after the variables.
    fn ssh_sandbox() -> SandboxExec {
        SandboxExec {
            prefix: ["ssh", "-T", "build-box", "env"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            env: crucible_lua::SandboxEnv::Inline,
            suffix: vec![],
        }
    }

    fn test_session_agent(agent_name: &str) -> SessionAgent {
        SessionAgent {
            mode: None,
            agent_type: "acp".to_string(),
            agent_name: Some(agent_name.to_string()),
            provider_key: None,
            provider: BackendType::Custom,
            model: "acp".to_string(),
            system_prompt: String::new(),
            temperature: None,
            max_tokens: None,
            max_context_tokens: None,
            thinking_budget: None,
            endpoint: None,
            env_overrides: HashMap::new(),
            mcp_servers: Vec::new(),
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
            output_validation: Default::default(),
            validation_retries: 3,
            autocompact_threshold: None,
            tool_policy: None,
        }
    }

    #[test]
    fn test_resolve_known_agent() {
        let config = test_session_agent("opencode");
        let (cmd, args, env) = resolve_agent_command("opencode", &config, None).unwrap();
        assert_eq!(cmd, "opencode");
        assert_eq!(args, vec!["acp"]);
        assert!(env.is_empty());
    }
    #[test]
    fn test_resolve_claude_agent() {
        let config = test_session_agent("claude");
        let (cmd, args, _) = resolve_agent_command("claude", &config, None).unwrap();
        assert_eq!(cmd, "npx");
        assert_eq!(args, vec!["@zed-industries/claude-agent-acp"]);
    }
    /// Why `session.create`/`session.configure_agent` must keep `agent_name`
    /// set on an ACP agent: it is the only input to profile resolution, and its
    /// absence is not an error — the fallback at the top of
    /// `build_client_config` execs the literal `acp`, which is nothing. The RPC
    /// round trips that keep the name set are pinned in
    /// `tests/rpc_session_create_agent_e2e.rs`.
    #[test]
    fn an_acp_agent_without_a_name_resolves_no_profile() {
        let mut agent = test_session_agent("claude");
        agent.agent_name = None;

        let config = build_client_config(&agent, Path::new("/nonexistent"), None, None).unwrap();
        assert_eq!(config.agent_path, PathBuf::from("acp"));
        assert_eq!(config.agent_args, None);
    }

    #[test]
    fn test_resolve_unknown_agent_uses_name_as_command() {
        let config = test_session_agent("my-custom-agent");
        let (cmd, args, _) = resolve_agent_command("my-custom-agent", &config, None).unwrap();
        assert_eq!(cmd, "my-custom-agent");
        assert!(args.is_empty());
    }
    #[test]
    fn test_env_overrides_from_session_agent() {
        let mut config = test_session_agent("opencode");
        config
            .env_overrides
            .insert("OPENCODE_MODEL".to_string(), "ollama/llama3.2".to_string());

        let (_, _, env) = resolve_agent_command("opencode", &config, None).unwrap();
        assert_eq!(env.len(), 1);
        assert_eq!(
            env[0],
            ("OPENCODE_MODEL".to_string(), "ollama/llama3.2".to_string())
        );
    }
    #[test]
    fn test_profile_overrides_command() {
        let config = test_session_agent("opencode");
        let mut acp_config = AcpConfig::default();

        let profile = crucible_core::config::AgentProfile {
            command: Some("/usr/local/bin/opencode".to_string()),
            ..Default::default()
        };
        acp_config.agents.insert("opencode".to_string(), profile);

        let (cmd, _, _) = resolve_agent_command("opencode", &config, Some(&acp_config)).unwrap();
        assert_eq!(cmd, "/usr/local/bin/opencode");
    }
    #[test]
    fn test_build_client_config() {
        let agent = test_session_agent("opencode");
        let config = build_client_config(&agent, Path::new("/tmp/workspace"), None, None).unwrap();

        assert_eq!(config.agent_path, PathBuf::from("opencode"));
        assert_eq!(config.agent_args, Some(vec!["acp".to_string()]));
        assert_eq!(config.working_dir, Some(PathBuf::from("/tmp/workspace")));
    }
    #[test]
    fn test_build_client_config_with_timeout() {
        let agent = test_session_agent("opencode");
        let acp_config = AcpConfig {
            streaming_timeout_minutes: 30,
            ..Default::default()
        };

        let config =
            build_client_config(&agent, Path::new("/tmp"), Some(&acp_config), None).unwrap();
        assert_eq!(config.timeout_ms, Some(180_000));
    }
    /// A sandboxed session launches the agent INSIDE its container.
    ///
    /// This is what makes an external agent's isolation enforceable: the
    /// daemon cannot intercept tools an ACP agent runs in its own process, but
    /// it does not need to when that process is already confined. Without the
    /// relocation the session is refused outright.
    #[test]
    fn a_sandbox_prefix_relocates_the_agent_into_the_container() {
        let agent = test_session_agent("opencode");

        let config = build_client_config(
            &agent,
            Path::new("/tmp/workspace"),
            None,
            Some(&sandbox(Some("-e"))),
        )
        .unwrap();

        assert_eq!(config.agent_path, PathBuf::from("podman"));
        assert_eq!(
            config.agent_args.unwrap(),
            vec![
                "exec",
                "-i",
                "-w",
                "/workspace",
                "crucible-s1",
                "opencode",
                "acp"
            ],
            "the agent's own argv must survive after the prefix, unsplit"
        );
    }
    /// A multi-word agent command is argv, not a shell string.
    ///
    /// `claude` resolves to `npx @zed-industries/claude-agent-acp`. Joining
    /// that into one string and letting a shell re-split it is how an argument
    /// containing a space silently becomes two.
    #[test]
    fn a_relocated_multi_word_agent_keeps_its_arguments_separate() {
        let agent = test_session_agent("claude");

        let config =
            build_client_config(&agent, Path::new("/tmp"), None, Some(&sandbox(Some("-e"))))
                .unwrap();

        assert_eq!(config.agent_path, PathBuf::from("podman"));
        let args = config.agent_args.unwrap();
        assert_eq!(args[args.len() - 2], "npx");
        assert_eq!(args[args.len() - 1], "@zed-industries/claude-agent-acp");
    }
    /// Configured environment must arrive INSIDE the container.
    ///
    /// `env_vars` is applied by the spawner to the process the daemon starts —
    /// which, once the agent is relocated, is the container runtime and not
    /// the agent. An API key configured for `claude` would then be read by
    /// podman and dropped at the container boundary.
    #[test]
    fn a_relocated_agents_environment_reaches_the_container_side() {
        let mut agent = test_session_agent("opencode");
        agent
            .env_overrides
            .insert("ANTHROPIC_API_KEY".to_string(), "sk-test".to_string());

        let config = build_client_config(
            &agent,
            Path::new("/tmp/workspace"),
            None,
            Some(&sandbox(Some("-e"))),
        )
        .unwrap();

        assert_eq!(
            config.agent_args.unwrap(),
            vec![
                "exec",
                "-i",
                "-w",
                "/workspace",
                "-e",
                "ANTHROPIC_API_KEY=sk-test",
                "crucible-s1",
                "opencode",
                "acp"
            ],
            "the variable must cross the boundary, and its flag must precede the container"
        );
        assert!(
            config.env_vars.is_none(),
            "it is delivered inside; a copy on the launcher process would mean nothing"
        );
    }

    /// The same delivery, for a launcher with no per-variable flag.
    ///
    /// `ssh` takes `env K=V … cmd`, so the pairs go in bare and the relocated
    /// command follows immediately — there is no operand to keep them in front
    /// of. Reading `Unsupported` for "no flag" would refuse every ACP launch
    /// over ssh, since an agent essentially always carries a key.
    #[test]
    fn an_inline_env_launcher_passes_variables_with_no_flag() {
        let mut agent = test_session_agent("opencode");
        agent
            .env_overrides
            .insert("ANTHROPIC_API_KEY".to_string(), "sk-test".to_string());

        let config = build_client_config(
            &agent,
            Path::new("/tmp/workspace"),
            None,
            Some(&ssh_sandbox()),
        )
        .unwrap();

        assert_eq!(config.agent_path, PathBuf::from("ssh"));
        assert_eq!(
            config.agent_args.unwrap(),
            vec![
                "-T",
                "build-box",
                "env",
                "ANTHROPIC_API_KEY=sk-test",
                "opencode",
                "acp"
            ],
            "no flag, and the agent's argv follows the variables directly"
        );
        assert!(
            config.env_vars.is_none(),
            "it is delivered on the remote side; a copy on the ssh process would mean nothing"
        );
    }

    /// A sandbox that cannot carry environment must say so at launch.
    ///
    /// Starting the agent anyway strips its credentials, and the failure then
    /// surfaces as an unauthenticated API call with nothing pointing back at
    /// the container.
    #[test]
    fn a_sandbox_that_cannot_pass_env_refuses_the_launch_by_name() {
        let mut agent = test_session_agent("opencode");
        agent
            .env_overrides
            .insert("ANTHROPIC_API_KEY".to_string(), "sk-test".to_string());

        let err = build_client_config(&agent, Path::new("/tmp"), None, Some(&sandbox(None)))
            .expect_err("a launch that would silently drop configured env must be refused");

        let msg = err.to_string();
        assert!(
            msg.contains("ANTHROPIC_API_KEY"),
            "the refusal must name the variable that could not be delivered, got: {msg}"
        );
    }

    /// ...and a sandbox with nothing to carry is unaffected by that rule.
    #[test]
    fn a_sandbox_without_configured_env_launches_unchanged() {
        let agent = test_session_agent("opencode");
        let config =
            build_client_config(&agent, Path::new("/tmp"), None, Some(&sandbox(None))).unwrap();
        let args = config.agent_args.unwrap();
        assert_eq!(args[args.len() - 3], "crucible-s1");
        assert!(!args.iter().any(|a| a == "-e"));
    }

    /// The profile's `[acp.agents.*] env` is the other half of the same
    /// configuration, and reaches the container by the same route.
    #[test]
    fn profile_env_reaches_the_container_too() {
        let agent = test_session_agent("opencode");
        let mut acp_config = AcpConfig::default();
        let mut profile = crucible_core::config::AgentProfile::default();
        profile
            .env
            .insert("OPENCODE_MODEL".to_string(), "ollama/llama3.2".to_string());
        acp_config.agents.insert("opencode".to_string(), profile);

        let config = build_client_config(
            &agent,
            Path::new("/tmp"),
            Some(&acp_config),
            Some(&sandbox(Some("-e"))),
        )
        .unwrap();

        let args = config.agent_args.unwrap();
        assert!(
            args.windows(2)
                .any(|w| w[0] == "-e" && w[1] == "OPENCODE_MODEL=ollama/llama3.2"),
            "got argv {args:?}"
        );
    }

    /// An empty prefix is not a prefix — it must not turn the agent's own
    /// command into an argument of nothing.
    #[test]
    fn an_empty_sandbox_prefix_leaves_the_launch_alone() {
        let agent = test_session_agent("opencode");
        let config = build_client_config(
            &agent,
            Path::new("/tmp"),
            None,
            Some(&SandboxExec::default()),
        )
        .unwrap();
        assert_eq!(config.agent_path, PathBuf::from("opencode"));
        assert_eq!(config.agent_args, Some(vec!["acp".to_string()]));
    }
}
