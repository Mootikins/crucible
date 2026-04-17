//! Chat Command - ACP-based Natural Language Interface

//!
//! Provides an interactive chat interface using the Agent Client Protocol.
//! Supports toggleable plan (read-only) and act (write-enabled) modes.

use anyhow::Result;
use crucible_daemon::{DaemonClient, LuaInitSessionRequest, LuaShutdownSessionRequest};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::commands::chat_preflight::ensure_valid_kiln;
use crate::config::CliConfig;
use crate::context_enricher::ContextEnricher;
use crate::core_facade::KilnContext;
use crate::factories;
use crate::output;
use crate::progress::{BackgroundProgress, LiveProgress, StatusLine};
use crate::tui::AgentSelection;
use crucible_core::traits::chat::{is_read_only, mode_display_name};

/// Parameters for the execute function
pub struct ExecuteParams {
    pub config: CliConfig,
    pub agent_name: Option<String>,
    pub query: Option<String>,
    pub read_only: bool,
    pub no_context: bool,
    pub context_size: Option<usize>,
    pub provider_key: Option<String>,
    pub max_context_tokens: usize,
    pub env_overrides: Vec<String>,
    pub resume_session_id: Option<String>,
    pub set_overrides: Vec<String>,
    pub record: Option<PathBuf>,
    pub replay: Option<PathBuf>,
    pub replay_speed: f64,
    pub replay_auto_exit: Option<u64>,
}

/// Parameters for the run_interactive_chat function
pub struct RunInteractiveChatParams {
    pub config: CliConfig,
    pub initial_mode: String,
    pub agent_name: Option<String>,
    pub provider_key: Option<String>,
    pub max_context_tokens: usize,
    pub parsed_env: std::collections::HashMap<String, String>,
    pub working_dir: Option<std::path::PathBuf>,
    pub resume_session_id: Option<String>,
    pub set_overrides: Vec<String>,
    pub record: Option<PathBuf>,
    pub replay: Option<PathBuf>,
    pub replay_speed: f64,
    pub replay_auto_exit: Option<u64>,
}

/// Parameters for the run_oneshot_chat function
pub struct RunOneshotChatParams {
    pub config: CliConfig,
    pub initial_mode: String,
    pub agent_name: Option<String>,
    pub provider_key: Option<String>,
    pub max_context_tokens: usize,
    pub parsed_env: std::collections::HashMap<String, String>,
    pub working_dir: Option<std::path::PathBuf>,
    pub resume_session_id: Option<String>,
    pub no_context: bool,
    pub context_size: Option<usize>,
    pub query_text: String,
    pub set_overrides: Vec<String>,
}

/// Determine which kiln to save sessions to.
///
/// Priority:
/// 1. `config.session_kiln` — explicit personal session kiln
/// 2. `config.kiln_path` — workspace kiln (original behavior)
/// 3. None — sessions won't be saved
///
/// When `session_kiln` is set in `~/.config/crucible/config.toml`,
/// sessions are stored there instead of the workspace kiln.
#[allow(dead_code)] // Prepared for future multi-kiln support
fn select_session_kiln(config: &CliConfig) -> Option<PathBuf> {
    // Prefer session_kiln if configured
    if let Some(ref session_kiln) = config.session_kiln {
        if session_kiln.exists() && session_kiln.is_dir() {
            info!(
                "Using session_kiln for session storage: {}",
                session_kiln.display()
            );
            return Some(session_kiln.clone());
        }
        warn!(
            "session_kiln path is invalid (not a directory or missing): {} - falling back to kiln_path",
            session_kiln.display()
        );
    }

    // Fall back to kiln_path
    let kiln_path = &config.kiln_path;

    if !kiln_path.exists() {
        warn!(
            "Kiln path does not exist: {} - sessions will not be saved",
            kiln_path.display()
        );
        return None;
    }

    if !kiln_path.is_dir() {
        warn!(
            "Kiln path is not a directory: {} - sessions will not be saved",
            kiln_path.display()
        );
        return None;
    }

    Some(kiln_path.clone())
}

pub async fn execute(params: ExecuteParams) -> Result<()> {
    let ExecuteParams {
        config,
        agent_name,
        query,
        read_only,
        no_context,
        context_size,
        provider_key,
        max_context_tokens,
        env_overrides,
        resume_session_id,
        set_overrides,
        record,
        replay,
        replay_speed,
        replay_auto_exit,
    } = params;
    let initial_mode = if read_only { "plan" } else { "normal" };

    info!("Starting chat command");
    info!("Initial mode: {}", mode_display_name(initial_mode));

    let parsed_env = parse_env_overrides(&env_overrides);
    let working_dir = std::env::current_dir().ok();

    let mut config = config;

    // If no explicit query but stdin is piped, read query from stdin (oneshot mode)
    let query = match query {
        Some(q) => Some(q),
        None if crate::commands::stdin::stdin_is_piped() => {
            crate::commands::stdin::read_stdin_message().ok()
        }
        None => None,
    };

    if query.is_none() {
        ensure_valid_kiln(&mut config).await?;
    }

    match query {
        None => {
            run_interactive_chat(RunInteractiveChatParams {
                config,
                initial_mode: initial_mode.to_string(),
                agent_name,
                provider_key,
                max_context_tokens,
                parsed_env,
                working_dir,
                resume_session_id,
                set_overrides,
                record,
                replay,
                replay_speed,
                replay_auto_exit,
            })
            .await
        }
        Some(query_text) => {
            run_oneshot_chat(RunOneshotChatParams {
                config,
                initial_mode: initial_mode.to_string(),
                agent_name,
                provider_key,
                max_context_tokens,
                parsed_env,
                working_dir,
                resume_session_id,
                no_context,
                context_size,
                query_text,
                set_overrides,
            })
            .await
        }
    }
}

fn parse_env_overrides(env_overrides: &[String]) -> std::collections::HashMap<String, String> {
    let parsed: std::collections::HashMap<String, String> = env_overrides
        .iter()
        .filter_map(|s| {
            let mut parts = s.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some(key), Some(value)) if !key.is_empty() => {
                    Some((key.to_string(), value.to_string()))
                }
                _ => {
                    warn!("Invalid env format '{}', expected KEY=VALUE", s);
                    None
                }
            }
        })
        .collect();

    if !parsed.is_empty() {
        let keys: Vec<_> = parsed.keys().collect();
        info!("CLI env overrides: {:?}", keys);
    }

    parsed
}

/// If the current working directory matches a registered project, open that
/// project's kilns via daemon RPC. This ensures multi-kiln projects have all
/// their knowledge sources available at session start.
async fn open_project_kilns_if_matched(
    config: &CliConfig,
    existing_client: Option<&DaemonClient>,
) -> Result<()> {
    let cwd = std::env::current_dir()?;

    // Find a project whose expanded path matches cwd
    let matched_project = config.projects.values().find(|project| {
        let expanded = crate::kiln_validate::expand_tilde(&project.path.to_string_lossy());
        expanded == cwd
    });

    let project = match matched_project {
        Some(p) => p,
        None => return Ok(()), // No project matches, nothing to do
    };

    if project.kilns.is_empty() {
        return Ok(());
    }

    let registry = config.resolved_kilns();
    // Reuse existing daemon connection if available, otherwise connect
    let owned_client;
    let client = match existing_client {
        Some(c) => c,
        None => {
            owned_client = crate::common::daemon_client().await?;
            &owned_client
        }
    };

    for kiln_name in &project.kilns {
        if let Some(entry) = registry.get(kiln_name) {
            if entry.lazy() {
                debug!(kiln = %kiln_name, "Skipping lazy kiln");
                continue;
            }
            let path = crate::kiln_validate::expand_tilde(&entry.path().to_string_lossy());
            match client.kiln_open(&path).await {
                Ok(()) => {
                    info!(kiln = %kiln_name, path = %path.display(), "Opened project kiln");
                }
                Err(e) => {
                    warn!(kiln = %kiln_name, error = %e, "Failed to open project kiln");
                }
            }
        } else {
            warn!(kiln = %kiln_name, "Kiln not found in registry");
        }
    }

    Ok(())
}

async fn run_interactive_chat(params: RunInteractiveChatParams) -> Result<()> {
    let RunInteractiveChatParams {
        config,
        initial_mode,
        agent_name,
        provider_key,
        max_context_tokens,
        parsed_env,
        working_dir,
        resume_session_id,
        set_overrides,
        record,
        replay,
        replay_speed,
        replay_auto_exit,
    } = params;
    use crate::chat::bridge::AgentEventBridge;
    use crate::tui::oil::{ChatMode, OilChatRunner};
    use crucible_core::events::EventRing;
    use crucible_core::traits::chat::is_read_only;

    let parsed_set_overrides = {
        use crate::tui::oil::commands::{validate_set_for_cli, SetEffect};

        let mut parsed = Vec::with_capacity(set_overrides.len());
        for input in &set_overrides {
            match validate_set_for_cli(input) {
                Err(e) => {
                    output::error(&format!("invalid --set '{}': {}", input, e));
                    std::process::exit(1);
                }
                Ok(SetEffect::DaemonRpc(_)) if agent_name.is_some() => {
                    output::error(&format!(
                        "invalid --set '{}': cannot set daemon RPC keys on ACP agent sessions",
                        input
                    ));
                    std::process::exit(1);
                }
                Ok(effect) => parsed.push(effect),
            }
        }
        parsed
    };

    let default_agent = config.acp.default_agent.clone();

    let ring = std::sync::Arc::new(EventRing::new(4096));
    let bridge = AgentEventBridge::new(ring);

    let mode = ChatMode::parse(&initial_mode);
    let effective_llm = config.effective_llm_provider().ok();
    let model_name = effective_llm
        .as_ref()
        .map(|p| p.model.clone())
        .unwrap_or_else(|| config.chat_model());

    let display_model = agent_name
        .as_deref()
        .map(|n| n.to_string())
        .unwrap_or_else(|| model_name.clone());

    let recording_mode = record.as_ref().map(|_| "granular".to_string());
    let recording_path = record;

    let mut runner = OilChatRunner::new()?
        .with_mode(mode)
        .with_model(&display_model)
        .with_context_limit(0)
        .with_show_thinking(config.chat.show_thinking)
        .with_agent_name(agent_name)
        .with_initial_sets(parsed_set_overrides)
        .with_recording_mode(recording_mode.clone())
        .with_recording_path(recording_path.clone())
        .with_replay_path(replay)
        .with_replay_speed(replay_speed)
        .with_replay_auto_exit(replay_auto_exit);

    info!(
        "Starting oil chat with model: {} (display: {})",
        model_name, display_model
    );

    if let Some(ref session_id) = resume_session_id {
        info!("Will resume session: {}", session_id);
        runner = runner.with_resume_session(session_id.clone());

        match fetch_resume_history(session_id, &config.kiln_path).await {
            Ok(history) if !history.is_empty() => {
                info!(
                    count = history.len(),
                    "Fetched resume history for viewport hydration"
                );
                runner = runner.with_resume_history(history);
            }
            Ok(_) => {
                info!("No history events found for session {}", session_id);
            }
            Err(e) => {
                warn!(
                    "Failed to fetch resume history, starting with blank viewport: {}",
                    e
                );
            }
        }
    }

    // Daemon owns setup (indexing, plugin discovery, MCP config read,
    // provider detection, context-length fetch). Results arrive as session
    // events from the setup task the daemon spawns on session.create. We
    // still need a daemon client here for two CLI-local concerns:
    //   1. Open project-registered kilns before session.create runs.
    //   2. Initialize the Lua session (RPC the TUI uses for slash commands).
    let kiln_root = config.kiln_path.clone();
    let lua_session_id = resume_session_id
        .clone()
        .unwrap_or_else(|| format!("chat-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S")));

    let lua_client = match crate::common::daemon_client().await {
        Ok(client) => Some(client),
        Err(e) => {
            warn!("Failed to connect to daemon for Lua init: {}", e);
            None
        }
    };

    if let Some(client) = lua_client.as_ref() {
        if let Err(e) = open_project_kilns_if_matched(&config, Some(client)).await {
            debug!("Project kiln auto-open skipped: {}", e);
        }
    }

    let lua_initialized = if let Some(client) = lua_client.as_ref() {
        let init_params = LuaInitSessionRequest {
            session_id: lua_session_id.clone(),
            kiln_path: kiln_root.to_string_lossy().to_string(),
            config: serde_json::Value::Null,
        };
        match client.lua_init_session(init_params).await {
            Ok(response) => {
                debug!(
                    session_id = %response.session_id,
                    commands = response.commands.len(),
                    views = response.views.len(),
                    "Initialized Lua session via daemon RPC"
                );
                true
            }
            Err(e) => {
                warn!("Failed to initialize Lua session via daemon RPC: {}", e);
                false
            }
        }
    } else {
        false
    };

    // MCP config is still passed to the runner because the runner's
    // background MCP gateway task connects to upstream servers to update
    // tool counts / connection status. The initial display list (name,
    // prefix) now arrives from the daemon's `mcp_servers_ready` event.
    if let Some(ref mcp) = config.mcp {
        runner = runner.with_mcp_config(mcp.clone());
    }

    runner = runner.with_slash_commands(known_slash_commands());

    let session_id = format!("chat-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
    let session_dir = config
        .kiln_path
        .join(".crucible")
        .join("sessions")
        .join(&session_id);
    std::fs::create_dir_all(&session_dir).ok();
    runner = runner.with_session_dir(session_dir);

    let config_for_factory = config;
    let initial_mode_str = initial_mode.to_string();
    let resume_id_for_factory = resume_session_id;
    let recording_mode_for_factory = recording_mode.clone();
    let recording_path_for_factory = recording_path.clone();
    let factory = move |selection: AgentSelection| {
        let config = config_for_factory.clone();
        let default_agent = default_agent.clone();
        let provider_key = provider_key.clone();
        let parsed_env = parsed_env.clone();
        let working_dir = working_dir.clone();
        let initial_mode = initial_mode_str.clone();
        let resume_session_id = resume_id_for_factory.clone();
        let recording_mode = recording_mode_for_factory.clone();
        let recording_path = recording_path_for_factory.clone();

        async move {
            // Build common params once
            let mut params = factories::AgentInitParams::new()
                .with_provider_opt(provider_key)
                .with_read_only(is_read_only(&initial_mode))
                .with_max_context_tokens(max_context_tokens)
                .with_env_overrides(parsed_env)
                .with_resume_session_id(resume_session_id)
                .with_recording_mode(recording_mode)
                .with_recording_path(recording_path);

            // Apply ACP-specific fields if needed
            if let AgentSelection::Acp(agent_name) = &selection {
                params = params
                    .with_type(factories::AgentType::Acp)
                    .with_agent_name_opt(Some(agent_name.clone()).or(default_agent));
            }

            // Apply working directory if provided
            if let Some(wd) = working_dir {
                params = params.with_working_dir(wd);
            }

            match selection {
                AgentSelection::Acp(_) | AgentSelection::Internal => {
                    let (handle, _session_id, event_rx) =
                        factories::create_daemon_agent_with_events(&config, &params).await?;
                    Ok((handle, Some(event_rx)))
                }
                AgentSelection::Cancelled => {
                    anyhow::bail!("Agent selection was cancelled")
                }
            }
        }
    };

    // Context length now arrives via the daemon's `context_limit_resolved`
    // setup event (internal-agent sessions only). The runner's
    // SessionEventStream updates its AtomicUsize handle as that event fires.

    let run_result = runner.run_with_factory(&bridge, factory).await;

    if lua_initialized {
        if let Some(client) = lua_client.as_ref() {
            let shutdown_params = LuaShutdownSessionRequest {
                session_id: lua_session_id,
            };
            if let Err(e) = client.lua_shutdown_session(shutdown_params).await {
                warn!("Failed to shutdown Lua session via daemon RPC: {}", e);
            }
        }
    }

    run_result
}

async fn run_oneshot_chat(params: RunOneshotChatParams) -> Result<()> {
    let RunOneshotChatParams {
        config,
        initial_mode,
        agent_name,
        provider_key,
        max_context_tokens,
        parsed_env,
        working_dir,
        resume_session_id,
        no_context,
        context_size,
        query_text,
        set_overrides,
    } = params;
    let mut status = StatusLine::new();
    let default_agent = config.acp.default_agent.clone();

    let mut agent_params = factories::AgentInitParams::new()
        .with_agent_name_opt(agent_name.clone().or(default_agent.clone()))
        .with_provider_opt(provider_key)
        .with_read_only(is_read_only(&initial_mode))
        .with_max_context_tokens(max_context_tokens)
        .with_env_overrides(parsed_env)
        .with_resume_session_id(resume_session_id);

    if let Some(ref wd) = working_dir {
        agent_params = agent_params.with_working_dir(wd.clone());
    }

    status.update("Initializing storage...");
    let storage_handle = factories::get_storage(&config).await?;

    let _storage_client: Option<()> = None;

    status.update("Discovering agent...");
    let mut handle = factories::create_agent(&config, agent_params).await?;

    let bg_progress: Option<BackgroundProgress> = None;
    status.update("Initializing core...");
    let core = Arc::new(KilnContext::from_storage_handle(storage_handle, config));
    status.success("Ready");

    let _autoconfirm_session =
        apply_oneshot_set_overrides(&mut handle, &set_overrides, agent_name.is_some()).await;

    let _live_progress = bg_progress.map(LiveProgress::start);

    let prompt = if no_context {
        query_text
    } else {
        let enricher = ContextEnricher::new(core.clone(), context_size);
        enricher.enrich(&query_text).await?
    };

    {
        use crate::formatting::render_markdown;
        use crucible_core::traits::chat::AgentHandle;
        use futures::StreamExt;

        let mut response_content = String::new();
        let mut stream = handle.send_message_stream(prompt);
        while let Some(result) = stream.next().await {
            match result {
                Ok(chunk) => {
                    if !chunk.done {
                        response_content.push_str(&chunk.delta);
                    }
                }
                Err(e) => {
                    eprintln!();
                    output::error(&format!("{}", e));
                    return Err(e.into());
                }
            }
        }

        println!("{}", render_markdown(&response_content));
    }

    Ok(())
}

async fn apply_oneshot_set_overrides(
    handle: &mut Box<dyn crucible_core::traits::chat::AgentHandle + Send + Sync>,
    set_overrides: &[String],
    is_acp: bool,
) -> bool {
    use crate::tui::oil::commands::{validate_set_for_cli, CliValue, SetEffect};

    let mut autoconfirm = false;

    for input in set_overrides {
        let effect = match validate_set_for_cli(input) {
            Ok(effect) => effect,
            Err(err) => {
                output::error(&format!("invalid --set '{}': {}", input, err));
                std::process::exit(1);
            }
        };

        match effect {
            SetEffect::DaemonRpc(action) => {
                if is_acp {
                    output::error(&format!(
                        "--set '{}' cannot be used with ACP agents (daemon RPC not available)",
                        input
                    ));
                    std::process::exit(1);
                }
                if let Err(e) = apply_rpc_action(handle, action).await {
                    output::error(&format!("--set '{}' failed: {}", input, e));
                    std::process::exit(1);
                }
            }
            SetEffect::TuiLocal { key, value } => {
                if key == "perm.autoconfirm_session" {
                    autoconfirm = match value {
                        CliValue::Disable => false,
                        CliValue::Set(v)
                            if matches!(
                                v.to_ascii_lowercase().as_str(),
                                "false" | "0" | "no" | "off"
                            ) =>
                        {
                            false
                        }
                        _ => true,
                    };
                } else {
                    eprintln!(
                        "warning: --set '{}' is TUI-only and has no effect in oneshot mode",
                        key
                    );
                }
            }
        }
    }

    autoconfirm
}

async fn apply_rpc_action(
    handle: &mut Box<dyn crucible_core::traits::chat::AgentHandle + Send + Sync>,
    action: crate::tui::oil::commands::SetRpcAction,
) -> Result<(), String> {
    use crate::tui::oil::commands::SetRpcAction;

    match action {
        SetRpcAction::SwitchModel(model) => {
            handle.switch_model(&model).await.map_err(|e| e.to_string())
        }
        SetRpcAction::SetThinkingBudget(Some(budget)) => handle
            .set_thinking_budget(budget)
            .await
            .map_err(|e| e.to_string()),
        SetRpcAction::SetThinkingBudget(None) => Ok(()),
        SetRpcAction::SetTemperature(temp) => handle
            .set_temperature(temp)
            .await
            .map_err(|e| e.to_string()),
        SetRpcAction::SetMaxTokens(max) => {
            handle.set_max_tokens(max).await.map_err(|e| e.to_string())
        }
        SetRpcAction::SetMaxIterations(max) => handle
            .set_max_iterations(max)
            .await
            .map_err(|e| e.to_string()),
        SetRpcAction::SetExecutionTimeout(timeout) => handle
            .set_execution_timeout(timeout)
            .await
            .map_err(|e| e.to_string()),
        SetRpcAction::SetContextBudget(budget) => handle
            .set_context_budget(budget)
            .await
            .map_err(|e| e.to_string()),
        SetRpcAction::SetContextStrategy(ref strategy_str) => {
            match strategy_str.parse::<crucible_core::session::ContextStrategy>() {
                Ok(strategy) => handle
                    .set_context_strategy(strategy)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            }
        }
        SetRpcAction::SetContextWindow(window) => handle
            .set_context_window(window)
            .await
            .map_err(|e| e.to_string()),
        SetRpcAction::SetOutputValidation(ref validation_str) => {
            match validation_str.parse::<crucible_core::session::OutputValidation>() {
                Ok(validation) => handle
                    .set_output_validation(validation)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            }
        }
        SetRpcAction::SetValidationRetries(retries) => handle
            .set_validation_retries(retries)
            .await
            .map_err(|e| e.to_string()),
        SetRpcAction::SetPrecognitionResults(count) => handle
            .set_precognition_results(count)
            .await
            .map_err(|e| e.to_string()),
    }
}

async fn fetch_resume_history(
    session_id: &str,
    kiln_path: &std::path::Path,
) -> Result<Vec<serde_json::Value>> {
    let client = crate::common::daemon_client().await?;
    let result = client
        .session_resume_from_storage(session_id, kiln_path, None, None)
        .await?;

    Ok(result
        .get("history")
        .and_then(|h| h.as_array())
        .cloned()
        .unwrap_or_default())
}

pub fn known_slash_commands() -> Vec<(String, String)> {
    vec![
        ("mode".into(), "Cycle chat mode".into()),
        (
            "default".into(),
            "Set default mode (ask permissions)".into(),
        ),
        ("plan".into(), "Set plan mode (read-only)".into()),
        ("auto".into(), "Set auto mode (full access)".into()),
        ("search".into(), "Search the knowledge base".into()),
        ("commit".into(), "Smart git commit workflow".into()),
        ("agent".into(), "Show/list available agents".into()),
        ("new".into(), "Start a new session".into()),
        ("resume".into(), "Resume a recent session".into()),
        ("view".into(), "Open or list Lua-defined views".into()),
        ("models".into(), "List or switch models".into()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_env_overrides_empty() {
        let result = parse_env_overrides(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_env_overrides_single() {
        let result = parse_env_overrides(&["FOO=bar".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result.get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    fn test_parse_env_overrides_multiple() {
        let result = parse_env_overrides(&["FOO=bar".to_string(), "BAZ=qux".to_string()]);
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(result.get("BAZ"), Some(&"qux".to_string()));
    }

    #[test]
    fn test_parse_env_overrides_with_equals_in_value() {
        let result = parse_env_overrides(&["KEY=value=with=equals".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result.get("KEY"), Some(&"value=with=equals".to_string()));
    }

    #[test]
    fn test_parse_env_overrides_empty_key_ignored() {
        let result = parse_env_overrides(&["=value".to_string()]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_env_overrides_no_equals_ignored() {
        let result = parse_env_overrides(&["INVALID".to_string()]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_env_overrides_mixed_valid_invalid() {
        let result = parse_env_overrides(&[
            "VALID=value".to_string(),
            "INVALID".to_string(),
            "=nokey".to_string(),
            "ALSO_VALID=123".to_string(),
        ]);
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("VALID"), Some(&"value".to_string()));
        assert_eq!(result.get("ALSO_VALID"), Some(&"123".to_string()));
    }

    #[test]
    fn test_parse_env_overrides_empty_value() {
        let result = parse_env_overrides(&["KEY=".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result.get("KEY"), Some(&"".to_string()));
    }

    #[test]
    fn test_select_session_kiln_valid_directory() {
        let temp = TempDir::new().unwrap();
        let config = CliConfig {
            kiln_path: temp.path().to_path_buf(),
            ..Default::default()
        };

        let result = select_session_kiln(&config);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), temp.path());
    }

    #[test]
    fn test_select_session_kiln_nonexistent_path() {
        let config = CliConfig {
            kiln_path: std::path::PathBuf::from("/nonexistent/path/that/does/not/exist"),
            ..Default::default()
        };

        let result = select_session_kiln(&config);
        assert!(result.is_none());
    }

    #[test]
    fn test_select_session_kiln_file_not_directory() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("file.txt");
        std::fs::write(&file_path, "content").unwrap();

        let config = CliConfig {
            kiln_path: file_path,
            ..Default::default()
        };

        let result = select_session_kiln(&config);
        assert!(result.is_none());
    }

    #[test]
    fn test_select_session_kiln_prefers_session_kiln_over_kiln_path() {
        let session_dir = TempDir::new().unwrap();
        let workspace_dir = TempDir::new().unwrap();
        let config = CliConfig {
            kiln_path: workspace_dir.path().to_path_buf(),
            session_kiln: Some(session_dir.path().to_path_buf()),
            ..Default::default()
        };

        let result = select_session_kiln(&config);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), session_dir.path());
    }

    #[test]
    fn test_select_session_kiln_falls_back_when_session_kiln_invalid() {
        let workspace_dir = TempDir::new().unwrap();
        let config = CliConfig {
            kiln_path: workspace_dir.path().to_path_buf(),
            session_kiln: Some(std::path::PathBuf::from("/nonexistent/session/kiln")),
            ..Default::default()
        };

        let result = select_session_kiln(&config);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), workspace_dir.path());
    }

    #[test]
    fn test_select_session_kiln_none_when_not_set() {
        let workspace_dir = TempDir::new().unwrap();
        let config = CliConfig {
            kiln_path: workspace_dir.path().to_path_buf(),
            session_kiln: None,
            ..Default::default()
        };

        let result = select_session_kiln(&config);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), workspace_dir.path());
    }
}
