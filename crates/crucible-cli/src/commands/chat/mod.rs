//! Chat Command - ACP-based Natural Language Interface

//!
//! Provides an interactive chat interface using the Agent Client Protocol.
//! Supports toggleable plan (read-only) and act (write-enabled) modes.

use anyhow::Result;
use crucible_daemon::{DaemonClient, LuaInitSessionRequest, LuaShutdownSessionRequest};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::commands::chat_preflight::{ensure_valid_kiln, fill_default_model_if_missing};
use crate::config::CliConfig;
use crate::factories;
use crate::output;
use crate::status_line::StatusLine;
use crate::tui::AgentSelection;

/// The flags that every chat run shares, plus the mode that selects the run.
pub struct ChatParams {
    pub config: CliConfig,
    pub agent_name: Option<String>,
    /// The user's `--plan` intent, threaded rather than re-derived.
    ///
    /// Read-only-ness is a property of a mode's tools and permissions, which
    /// the daemon owns — the CLI cannot compute it for a user-defined mode.
    /// Recovering it from the mode NAME (`mode_id == "plan"`) was a lie that
    /// only happened to hold while the mode set was fixed.
    pub read_only: bool,
    pub no_context: bool,
    pub context_size: Option<usize>,
    pub provider_key: Option<String>,
    pub max_context_tokens: usize,
    pub env_overrides: Vec<String>,
    pub resume_session_id: Option<String>,
    pub set_overrides: Vec<String>,
    pub mode: ChatMode,
}

impl ChatParams {
    /// The flags of a plain `cru chat` with no arguments.
    pub fn new(config: CliConfig) -> Self {
        Self {
            config,
            agent_name: None,
            read_only: false,
            no_context: false,
            // No override: the daemon's session default stands.
            context_size: None,
            provider_key: None,
            max_context_tokens: 16384,
            env_overrides: vec![],
            resume_session_id: None,
            set_overrides: vec![],
            mode: ChatMode::Interactive { record: None },
        }
    }
}

/// Which kind of chat run the flags select.
#[derive(Debug)]
pub enum ChatMode {
    /// The TUI. `record` writes a granular transcript to that path.
    Interactive { record: Option<PathBuf> },
    /// One query, one answer, then exit.
    Oneshot { query: String },
    /// Play a recorded transcript back. No session is created.
    Replay {
        path: PathBuf,
        speed: f64,
        auto_exit: Option<u64>,
    },
}

impl ChatMode {
    /// Build the mode from the `cru chat` flags.
    ///
    /// `--replay` excludes a query and `--record`; the flags that need a
    /// session (`--resume`, `--agent`) are checked in `execute`.
    pub fn from_flags(
        query: Option<String>,
        record: Option<PathBuf>,
        replay: Option<PathBuf>,
        replay_speed: f64,
        replay_auto_exit: Option<u64>,
    ) -> Result<Self> {
        match (replay, query) {
            (Some(path), query) => {
                if query.is_some() {
                    anyhow::bail!("--replay cannot be combined with a query argument");
                }
                if record.is_some() {
                    anyhow::bail!("--replay cannot be combined with --record");
                }
                Ok(Self::Replay {
                    path,
                    speed: replay_speed,
                    auto_exit: replay_auto_exit,
                })
            }
            (None, Some(_)) if record.is_some() => {
                // A query, given here or piped, runs one turn and exits.
                // There is no session for a recording to cover.
                anyhow::bail!("--record needs an interactive terminal; a query runs oneshot")
            }
            (None, Some(query)) => Ok(Self::Oneshot { query }),
            (None, None) => Ok(Self::Interactive { record }),
        }
    }
}

pub async fn execute(mut params: ChatParams) -> Result<()> {
    // Seed the render-time highlighting state (theme + enabled) from config
    // before any frame renders; `:set theme` updates it later.
    crate::formatting::syntax::seed_from_config(&params.config.cli.highlighting);

    if let ChatMode::Replay {
        path,
        speed,
        auto_exit,
    } = &params.mode
    {
        if !path.exists() {
            anyhow::bail!("replay file not found: {}", path.display());
        }
        if params.resume_session_id.is_some() {
            anyhow::bail!("--replay cannot be combined with --resume");
        }
        if params.agent_name.is_some() {
            anyhow::bail!("--replay cannot be combined with --agent");
        }
        return run_replay(path.clone(), *speed, *auto_exit, &params.config).await;
    }

    info!("Starting chat command");

    // A piped stdin is a query: the TUI becomes a oneshot run.
    params.mode = apply_piped_query(params.mode, || {
        crate::commands::stdin::stdin_is_piped()
            .then(crate::commands::stdin::read_stdin_message)
            .and_then(Result::ok)
    })?;

    if let ChatMode::Interactive { .. } = params.mode {
        ensure_valid_kiln(&mut params.config).await?;
    }
    fill_default_model_if_missing(&mut params.config);

    // A session on the internal agent can do nothing without a provider.
    // Fail here, with remedies, rather than mid-conversation with a raw
    // transport error. ACP agents bring their own provider — and "is this
    // ACP?" must be `resolve_is_acp`, not `agent_name.is_some()`: a user
    // with `[chat] agent_preference = "acp"` runs plain `cru chat` with no
    // LLM provider configured, legitimately. Replay never reaches this
    // point (it returns above), and a resumed session's agent type is
    // stored daemon-side (it may be ACP even without `-a`), so resume
    // relies on the TUI's empty-list warning instead.
    let is_acp = crate::factories::agent::resolve_is_acp(
        None,
        params.agent_name.as_deref(),
        &params.config.chat.agent_preference,
    );
    if !is_acp && params.resume_session_id.is_none() {
        let client = crucible_daemon::DaemonClient::connect_or_start().await?;
        crate::commands::chat_preflight::ensure_providers_available(
            &client,
            &params.config.kiln_path,
        )
        .await?;
    }

    match std::mem::replace(&mut params.mode, ChatMode::Interactive { record: None }) {
        ChatMode::Interactive { record } => run_interactive_chat(params, record).await,
        ChatMode::Oneshot { query } => run_oneshot_chat(params, query).await,
        ChatMode::Replay { .. } => unreachable!("replay returns above"),
    }
}

/// Fold a piped stdin query into the mode.
///
/// Only the TUI reads stdin as a query. An explicit query never calls
/// `read_piped`: the read would drain a pipe that a shell loop shares,
/// or block on a pipe that a supervisor holds open. A oneshot run has
/// no TUI to record, so `--record` with a piped query is an error;
/// before this check the recording path vanished without a word.
fn apply_piped_query(
    mode: ChatMode,
    read_piped: impl FnOnce() -> Option<String>,
) -> Result<ChatMode> {
    let ChatMode::Interactive { record } = mode else {
        return Ok(mode);
    };
    match (record, read_piped()) {
        (Some(_), Some(_)) => {
            anyhow::bail!("--record needs an interactive terminal; a query runs oneshot")
        }
        (None, Some(query)) => Ok(ChatMode::Oneshot { query }),
        (record, None) => Ok(ChatMode::Interactive { record }),
    }
}

/// The mode name `--plan` selects. The daemon owns what the name means.
fn initial_mode(read_only: bool) -> &'static str {
    if read_only {
        "plan"
    } else {
        "ask"
    }
}

/// The mode `cru chat -q` must apply before its turn.
///
/// Without `--plan` nothing is sent: a resumed session keeps the mode it
/// has, and a new session starts in the daemon's default.
fn oneshot_mode_override(read_only: bool) -> Option<&'static str> {
    read_only.then(|| initial_mode(true))
}

async fn run_replay(
    path: PathBuf,
    speed: f64,
    auto_exit: Option<u64>,
    config: &CliConfig,
) -> Result<()> {
    use crate::chat::bridge::AgentEventBridge;
    use crate::tui::oil::OilChatRunner;
    use crucible_core::events::EventRing;

    // The replay entry short-circuits inside `run_with_factory` before the
    // factory is invoked, so the supplied closure is a stub that never runs.
    // The bridge is still required by the entry-point signature; it is
    // backed by a throwaway ring because nothing consumes it in replay.
    let ring = Arc::new(EventRing::new(4096));
    let bridge = AgentEventBridge::new(ring);

    let mut runner = OilChatRunner::new()?
        .with_mode(crate::tui::oil::DEFAULT_MODE)
        .with_model("replay")
        .with_context_limit(0)
        .with_show_thinking(config.chat.show_thinking)
        .with_show_diffs(config.chat.show_diffs)
        .with_replay_path(Some(path))
        .with_replay_speed(speed)
        .with_replay_auto_exit(auto_exit);

    let factory = |_selection: crate::tui::AgentSelection| async move {
        // Unreachable: replay short-circuits before the factory is called.
        Err::<
            (
                Box<dyn crucible_core::traits::chat::AgentHandle + Send + Sync>,
                Option<tokio::sync::mpsc::UnboundedReceiver<crucible_daemon::SessionEvent>>,
            ),
            anyhow::Error,
        >(anyhow::anyhow!(
            "factory called during replay (should be unreachable)"
        ))
    };

    runner.run_with_factory(&bridge, factory).await
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

/// The session-state RPCs `--no-context` / `--context-size` stand for.
///
/// `--no-context` → `session.set_precognition(false)`, `--context-size N` →
/// `session.set_precognition_results(N)`. Both paths (`cru chat` and
/// `cru chat -q`) go through here so they cannot drift apart.
///
/// The asymmetry is deliberate. An *absent* flag emits nothing:
///   - no `set_precognition(true)`, which would silently undo a
///     `:set noprecognition` the user made in a session now being `--resume`d;
///   - no default result count, which would override the daemon's own.
///
/// `--no-context` also suppresses `--context-size`: there is no result count
/// to set on a searcher that will not run.
fn precognition_flag_actions(
    no_context: bool,
    context_size: Option<usize>,
) -> Vec<crate::tui::oil::commands::SetRpcAction> {
    use crate::tui::oil::commands::SetRpcAction;

    if no_context {
        vec![SetRpcAction::SetPrecognition(false)]
    } else {
        context_size
            .map(SetRpcAction::SetPrecognitionResults)
            .into_iter()
            .collect()
    }
}

/// Startup overrides for interactive chat: `--set` first, then the
/// `--no-context` / `--context-size` flags.
///
/// Flags land last so that, as in `run_oneshot_chat`, an explicit
/// `--no-context` beats a `--set precognition=on` on the same command line.
/// The `Err` is a rendered message for the caller to report and exit on —
/// this returns rather than exits so it stays testable.
fn build_initial_sets(
    set_overrides: &[String],
    no_context: bool,
    context_size: Option<usize>,
) -> Result<Vec<crate::tui::oil::commands::SetEffect>, String> {
    use crate::tui::oil::commands::{validate_set_for_cli, SetEffect};

    let mut sets = Vec::with_capacity(set_overrides.len() + 1);
    for input in set_overrides {
        match validate_set_for_cli(input) {
            Ok(effect) => sets.push(effect),
            Err(e) => return Err(format!("invalid --set '{}': {}", input, e)),
        }
    }
    sets.extend(
        precognition_flag_actions(no_context, context_size)
            .into_iter()
            .map(SetEffect::DaemonRpc),
    );
    Ok(sets)
}

/// Ask the daemon to open the kilns of the project rooted at the working
/// directory, if any.
///
/// This used to read `[projects.*]` out of the user's config, match it against
/// the working directory, and call `kiln.open` for each named kiln. Every part
/// of that is business logic, and it lived here — in a render layer a web
/// frontend cannot share, because a browser cannot read the user's config file.
/// The daemon holds both the project registry and the kiln registry, so it is
/// the only layer that can answer "which kilns does this directory imply"
/// without duplicating one of them.
///
/// A directory that matches no project is the ordinary case and not an error.
/// The kilns the daemon has open, as the startup banner names them.
///
/// The daemon owns the set. A listing failure is not an error here: the banner
/// is information, and a session with no banner is better than a session that
/// refuses to start over one.
async fn attached_kilns(client: &DaemonClient) -> Vec<crate::tui::oil::KilnSummary> {
    let rows = match client.kiln_list().await {
        Ok(rows) => rows,
        Err(e) => {
            debug!("kiln.list failed; the startup banner is skipped: {e}");
            return Vec::new();
        }
    };

    rows.iter()
        .filter_map(|row| {
            let path = row["path"].as_str()?;
            let name = match row["name"].as_str().unwrap_or_default() {
                "" => std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string()),
                name => name.to_string(),
            };
            Some(crate::tui::oil::KilnSummary {
                name,
                path: path.to_string(),
            })
        })
        .collect()
}

/// How many proposals wait in the kiln the CLI reads, for the startup banner.
///
/// The banner names `cru proposals list`, and that command reads one
/// directory: the staging area of `config.kiln_path`. The count comes from
/// the same directory, so the two numbers agree. A directory the CLI cannot
/// read counts zero: the banner is information, and it must not fail the
/// session.
fn pending_proposals(config: &CliConfig) -> usize {
    crate::commands::proposals::collect_proposals(&crate::commands::proposals::proposals_dir(
        config,
    ))
    .map(|files| files.len())
    .unwrap_or(0)
}

async fn open_project_kilns_if_matched(existing_client: Option<&DaemonClient>) -> Result<()> {
    let cwd = std::env::current_dir()?;

    let owned_client;
    let client = match existing_client {
        Some(c) => c,
        None => {
            owned_client = crate::common::daemon_client().await?;
            &owned_client
        }
    };

    let reply = client.project_open_kilns(&cwd).await?;
    if !reply["matched"].as_bool().unwrap_or(false) {
        return Ok(());
    }
    for opened in reply["opened"].as_array().into_iter().flatten() {
        info!(
            kiln = %opened["kiln"].as_str().unwrap_or_default(),
            path = %opened["path"].as_str().unwrap_or_default(),
            "Opened project kiln"
        );
    }
    for skipped in reply["skipped"].as_array().into_iter().flatten() {
        debug!(
            kiln = %skipped["kiln"].as_str().unwrap_or_default(),
            reason = %skipped["reason"].as_str().unwrap_or_default(),
            "Skipped project kiln"
        );
    }
    for failed in reply["errors"].as_array().into_iter().flatten() {
        warn!(
            kiln = %failed["kiln"].as_str().unwrap_or_default(),
            error = %failed["error"].as_str().unwrap_or_default(),
            "Failed to open project kiln"
        );
    }
    Ok(())
}

async fn run_interactive_chat(params: ChatParams, record: Option<PathBuf>) -> Result<()> {
    let ChatParams {
        config,
        agent_name,
        read_only,
        no_context,
        context_size,
        provider_key,
        // `--max-context` never reached the daemon; the flag stays until a
        // session knob carries it.
        max_context_tokens: _,
        env_overrides,
        resume_session_id,
        set_overrides,
        mode: _,
    } = params;
    let initial_mode = initial_mode(read_only);
    info!("Initial mode: {}", initial_mode);
    let parsed_env = parse_env_overrides(&env_overrides);
    let working_dir = std::env::current_dir().ok();
    use crate::chat::bridge::AgentEventBridge;
    use crate::tui::oil::OilChatRunner;
    use crucible_core::events::EventRing;

    // `--set` plus the `--no-context` / `--context-size` flags. All of these
    // are session state the daemon owns, so they ride the same
    // `initial_sets` → `process_action` path, which applies them after the
    // session exists — including a session reattached by `--resume`, matching
    // `cru chat -q --resume`.
    let parsed_set_overrides = match build_initial_sets(&set_overrides, no_context, context_size) {
        Ok(sets) => sets,
        Err(message) => {
            output::error(&message);
            std::process::exit(1);
        }
    };

    let default_agent = config.acp.default_agent.clone();

    let ring = std::sync::Arc::new(EventRing::new(4096));
    let bridge = AgentEventBridge::new(ring);

    let mode: std::sync::Arc<str> = initial_mode.into();
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
        .with_show_diffs(config.chat.show_diffs)
        .with_agent_name(agent_name)
        .with_initial_sets(parsed_set_overrides);

    info!(
        "Starting oil chat with model: {} (display: {})",
        model_name, display_model
    );

    if let Some(ref session_id) = resume_session_id {
        info!("Will resume session: {}", session_id);
        runner = runner.with_resume_session(session_id.clone());

        match fetch_resume_history(session_id).await {
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
        if let Err(e) = open_project_kilns_if_matched(Some(client)).await {
            debug!("Project kiln auto-open skipped: {}", e);
        }
    }

    // After the project kilns open, so the banner names them too.
    if let Some(client) = lua_client.as_ref() {
        runner = runner
            .with_connected_kilns(attached_kilns(client).await)
            .with_pending_proposals(pending_proposals(&config));
    }

    // Pull the Lua-defined theme before the first frame. Strictly an upgrade:
    // the TUI already holds a complete compiled-in default, so a missing daemon,
    // an RPC error, or an older daemon that does not know `ui.config` all leave a
    // correct screen. Never make this a precondition for rendering.
    if let Some(client) = lua_client.as_ref() {
        let ui_params = serde_json::json!({ "session_id": lua_session_id });
        match client.call("ui.config", ui_params).await {
            Ok(payload) => {
                crate::tui::oil::theme::apply_ui_config(&payload);
            }
            Err(e) => debug!("ui.config unavailable, using the built-in theme: {e}"),
        }
    }

    let lua_initialized = if let Some(client) = lua_client.as_ref() {
        let init_params = LuaInitSessionRequest {
            session_id: lua_session_id.clone(),
            kiln_path: Some(kiln_root.to_string_lossy().to_string()),
        };
        match client.lua_init_session(init_params).await {
            Ok(response) => {
                debug!(
                    session_id = %response.session_id,
                    commands = response.commands.len(),
                    "Initialized Lua session via daemon RPC"
                );
                // Plugin-declared slash commands: `/name` dispatches to the
                // daemon's `plugin.run_command` and autocompletes alongside
                // the built-ins. This response is where every client is meant
                // to learn the set — until here, nothing consumed it.
                let plugin_commands: Vec<(String, String)> = response
                    .commands
                    .iter()
                    .filter_map(|c| {
                        let name = c.get("name")?.as_str()?.to_string();
                        let description = c
                            .get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("")
                            .to_string();
                        Some((name, description))
                    })
                    .collect();
                if !plugin_commands.is_empty() {
                    runner = runner.with_plugin_commands(plugin_commands);
                }
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

    runner = runner.with_slash_commands(known_slash_commands());

    // Scratch home for TUI-side artifacts (saved shell output). Under the
    // daemon's sessions root, not inside a kiln: a kiln holds knowledge, and
    // shipping a kiln should not ship somebody's captured shell output.
    let session_id = format!("chat-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
    let session_dir = crate::commands::session::io::sessions_dir(&config).join(&session_id);
    std::fs::create_dir_all(&session_dir).ok();
    runner = runner.with_session_dir(session_dir);

    let config_for_factory = config;
    let resume_id_for_factory = resume_session_id;
    let recording_mode_for_factory = recording_mode.clone();
    let recording_path_for_factory = recording_path.clone();
    let factory = move |selection: AgentSelection| {
        let config = config_for_factory.clone();
        let default_agent = default_agent.clone();
        let provider_key = provider_key.clone();
        let parsed_env = parsed_env.clone();
        let working_dir = working_dir.clone();
        let resume_session_id = resume_id_for_factory.clone();
        let recording_mode = recording_mode_for_factory.clone();
        let recording_path = recording_path_for_factory.clone();

        async move {
            // Build common params once
            let mut params = factories::AgentInitParams::new()
                .with_provider_opt(provider_key)
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

            let (handle, _session_id, event_rx) =
                factories::create_daemon_agent_with_events(&config, &params).await?;
            Ok((handle, Some(event_rx)))
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

/// Await `work`, re-drawing `status` each second with the elapsed time.
///
/// The status line only ever repainted between steps, so a step that took
/// minutes was indistinguishable from a hang: no spinner, no counter, and —
/// when stdout is not a terminal — no output at all. `StatusLine::update`
/// still suppresses itself when piped; this only fixes the interactive case,
/// which is the one a person is watching.
async fn await_with_elapsed<T, F>(status: &mut StatusLine, label: &str, work: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    use std::time::{Duration, Instant};

    let started = Instant::now();
    status.update(&format!("{label}..."));

    let mut work = std::pin::pin!(work);
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    ticker.tick().await; // fires immediately; the label is already drawn

    loop {
        tokio::select! {
            result = &mut work => return result,
            _ = ticker.tick() => {
                status.update(&format!("{label}... {}s", started.elapsed().as_secs()));
            }
        }
    }
}

/// `cru chat -q` applies `--plan` through `set_mode_str`, the same RPC the
/// TUI's `/plan` uses. `max_context_tokens` does not reach the agent.
async fn run_oneshot_chat(params: ChatParams, query_text: String) -> Result<()> {
    let ChatParams {
        config,
        agent_name,
        read_only,
        no_context,
        context_size,
        provider_key,
        max_context_tokens: _,
        env_overrides,
        resume_session_id,
        set_overrides,
        mode: _,
    } = params;
    let parsed_env = parse_env_overrides(&env_overrides);
    let working_dir = std::env::current_dir().ok();
    let mut status = StatusLine::new();
    let default_agent = config.acp.default_agent.clone();

    let mut agent_params = factories::AgentInitParams::new()
        .with_agent_name_opt(agent_name.clone().or(default_agent.clone()))
        .with_provider_opt(provider_key)
        .with_env_overrides(parsed_env)
        .with_resume_session_id(resume_session_id);

    if let Some(ref wd) = working_dir {
        agent_params = agent_params.with_working_dir(wd.clone());
    }

    // Kept for its side effect, not its value: `get_storage` opens the kiln
    // with `process = true`, which indexes pending files. Without it the
    // daemon's Precognition would search a possibly-unindexed kiln.
    //
    // The work is unbounded — a kiln the daemon has never seen is parsed and
    // embedded note by note first — so the wait is timed on screen rather than
    // spent behind a status line that never moves. A frozen line is how this
    // read as a crash.
    let (_storage_handle, kiln) = await_with_elapsed(
        &mut status,
        "Opening kiln",
        factories::get_storage_with_summary(&config),
    )
    .await?;
    if let Some(line) = kiln.describe() {
        status.update(&line);
    }

    status.update("Discovering agent...");
    let mut handle = factories::create_agent(&config, agent_params).await?;

    status.success("Ready");

    let _autoconfirm_session = apply_oneshot_set_overrides(&mut handle, &set_overrides).await;

    if let Some(mode_id) = oneshot_mode_override(read_only) {
        handle
            .set_mode_str(mode_id)
            .await
            .map_err(|e| anyhow::anyhow!("failed to apply --plan: {e}"))?;
    }

    // `--no-context` / `--context-size` are session state, not a local
    // transform: the daemon owns Precognition, and it is already enabled by
    // `SessionAgent::internal_from_config`. Setting it here is what makes
    // `cru chat -q` and the TUI ground identically — and why the prompt below
    // is the user's text verbatim. Enriching it client-side made the daemon's
    // own search run against the CLI's context block instead of the question.
    for action in precognition_flag_actions(no_context, context_size) {
        if let Err(e) = apply_rpc_action(&mut handle, action).await {
            anyhow::bail!("failed to apply knowledge-base context flags: {e}");
        }
    }
    let prompt = query_text;

    {
        use crate::formatting::render_markdown;
        use crucible_core::turn::{Agent, TurnContext, TurnEvent};
        use futures::StreamExt;

        let mut response_content = String::new();
        let mut stream = handle.turn(TurnContext::new(prompt)).await?;
        while let Some(event) = stream.next().await {
            match event {
                TurnEvent::TextDelta(text) => response_content.push_str(&text),
                TurnEvent::Error(err) => {
                    eprintln!();
                    output::error(&format!("{}", err));
                    return Err(anyhow::anyhow!("{err}"));
                }
                _ => {}
            }
        }

        println!("{}", render_markdown(&response_content));
    }

    Ok(())
}

async fn apply_oneshot_set_overrides(
    handle: &mut Box<dyn crucible_core::traits::chat::AgentHandle + Send + Sync>,
    set_overrides: &[String],
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
            crucible_core::traits::chat::SessionKnobs::switch_model(handle, &model)
                .await
                .map_err(|e| e.to_string())
        }
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
        SetRpcAction::SetPrecognition(enabled) => handle
            .set_precognition(enabled)
            .await
            .map_err(|e| e.to_string()),
        SetRpcAction::SetPrecognitionResults(count) => handle
            .set_precognition_results(count)
            .await
            .map_err(|e| e.to_string()),
        SetRpcAction::SetAutocompactThreshold(threshold) => handle
            .set_autocompact_threshold(threshold)
            .await
            .map_err(|e| e.to_string()),
    }
}

async fn fetch_resume_history(session_id: &str) -> Result<Vec<serde_json::Value>> {
    let client = crate::common::daemon_client().await?;
    let result = client
        .session_resume_from_storage(session_id, None, None)
        .await?;

    Ok(result
        .get("history")
        .and_then(|h| h.as_array())
        .cloned()
        .unwrap_or_default())
}

/// Slash commands advertised in the TUI completion popup.
///
/// Only list commands `handle_slash_command` actually handles — anything
/// else falls through to `ExecuteSlashCommand`, which delivers the raw
/// text to the LLM as a user message. Advertising unhandled commands
/// (/search, /new, /resume, ...) promised features that silently became
/// prompt text.
pub fn known_slash_commands() -> Vec<(String, String)> {
    vec![
        ("mode".into(), "Cycle chat mode".into()),
        (
            "default".into(),
            "Set default mode (ask permissions)".into(),
        ),
        ("plan".into(), "Set plan mode (read-only)".into()),
        ("auto".into(), "Set auto mode (full access)".into()),
        ("undo".into(), "Undo last exchange(s)".into()),
        ("help".into(), "Show help".into()),
    ]
}

#[cfg(test)]
mod tests;
