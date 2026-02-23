//! Chat Command - ACP-based Natural Language Interface

//!
//! Provides an interactive chat interface using the Agent Client Protocol.
//! Supports toggleable plan (read-only) and act (write-enabled) modes.

use anyhow::Result;
use crucible_lua::{ChannelSessionRpc, LuaExecutor, Session, SessionCommand};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

use colored::Colorize;

use crate::config::CliConfig;
use crate::context_enricher::ContextEnricher;
use crate::core_facade::KilnContext;
use crate::factories;
use crate::kiln_discover::{discover_kiln, DiscoverySource};
use crate::progress::{BackgroundProgress, LiveProgress, StatusLine};
use crate::provider_detect::{
    detect_providers, fetch_all_provider_models, fetch_model_context_length, fetch_provider_models,
};
use crate::tui::oil::{McpServerDisplay, PluginStatusEntry};
use crate::tui::AgentSelection;
use crucible_core::traits::chat::{is_read_only, mode_display_name};
use crucible_pipeline::NotePipeline;
use crucible_watch::traits::{DebounceConfig, HandlerConfig, WatchConfig};
use crucible_watch::{EventFilter, WatchMode};

/// Determine which kiln to save sessions to.
///
/// - Single kiln: use it automatically
/// - Multiple kilns: use primary (first) kiln (future: config.kilns support)
/// - No kilns/invalid path: return None (sessions won't be saved)
///
/// Currently Crucible supports a single kiln_path. This function is designed
/// to support future multi-kiln configurations where multiple kilns can be
/// attached to a workspace.
fn select_session_kiln(config: &CliConfig) -> Option<PathBuf> {
    let kiln_path = &config.kiln_path;

    // Check if the kiln path exists and is a directory
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

    // Future: when config.kilns is available, use first kiln as primary
    // if config.kilns.is_empty() {
    //     warn!("No kilns configured - sessions will not be saved");
    //     return None;
    // }
    // Some(config.kilns[0].path.clone())

    Some(kiln_path.clone())
}

#[allow(clippy::too_many_arguments)]
pub async fn execute(
    config: CliConfig,
    agent_name: Option<String>,
    query: Option<String>,
    read_only: bool,
    no_context: bool,
    no_process: bool,
    context_size: Option<usize>,
    provider_key: Option<String>,
    max_context_tokens: usize,
    env_overrides: Vec<String>,
    resume_session_id: Option<String>,
    set_overrides: Vec<String>,
    record: Option<PathBuf>,
    replay: Option<PathBuf>,
    replay_speed: f64,
    replay_auto_exit: Option<u64>,
) -> Result<()> {
    let initial_mode = if read_only { "plan" } else { "normal" };

    info!("Starting chat command");
    info!("Initial mode: {}", mode_display_name(initial_mode));

    let parsed_env = parse_env_overrides(&env_overrides);
    let working_dir = std::env::current_dir().ok();

    let mut config = config;

    if query.is_none() {
        run_preflight_checks(&mut config).await?;
    }

    match query {
        None => {
            run_interactive_chat(
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
            )
            .await
        }
        Some(query_text) => {
            run_oneshot_chat(
                config,
                initial_mode,
                agent_name,
                provider_key,
                max_context_tokens,
                parsed_env,
                working_dir,
                resume_session_id,
                no_context,
                no_process,
                context_size,
                query_text,
                set_overrides,
            )
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

async fn run_preflight_checks(config: &mut CliConfig) -> Result<()> {
    let config_kiln_valid = config.kiln_path.join(".crucible").is_dir();
    let providers = detect_providers(&config.chat);

    if !config_kiln_valid {
        // Config kiln not valid, try discovery
        let discovered = discover_kiln(None, None);
        match discovered {
            Some(found) => {
                info!(
                    "Discovered kiln at {} (via {:?})",
                    found.path.display(),
                    found.source
                );
                if found.source != DiscoverySource::CliFlag {
                    config.kiln_path = found.path;
                }
            }
            None => {
                info!("No kiln found, prompting for path");
                println!(
                    "{} No kiln found. A kiln is a folder where Crucible stores your notes and sessions.",
                    "Setup:".cyan().bold()
                );
                println!("  {} A kiln is like a vault — it holds all your markdown notes, embeddings, and chat history.", "What is a kiln?".dimmed());
                println!("  {} A good default is a folder in your home directory or Documents (e.g., ~/crucible).", "Tip:".dimmed());

                let path_input: String = dialoguer::Input::new()
                    .with_prompt("Kiln path")
                    .default("~/crucible".to_string())
                    .interact_text()?;

                let expanded = crate::kiln_validate::expand_tilde(path_input.trim());

                if !expanded.exists() {
                    std::fs::create_dir_all(&expanded)?;
                }

                let crucible_dir = expanded.join(".crucible");
                if !crucible_dir.join("config.toml").exists() {
                    let (provider, model) = if let Some(p) = providers.first() {
                        let m = p
                            .default_model
                            .clone()
                            .unwrap_or_else(|| "llama3.2".to_string());
                        (p.provider_type.clone(), m)
                    } else {
                        ("ollama".to_string(), "llama3.2".to_string())
                    };

                    let config_content =
                        crate::commands::init::generate_config_with_provider(&provider, &model);
                    crate::commands::init::create_kiln_with_config(
                        &crucible_dir,
                        &config_content,
                        false,
                    )?;

                    println!("{} Kiln initialized at {}", "✓".green(), expanded.display());
                }

                config.kiln_path = expanded;
            }
        }
    } else {
        info!("Using kiln from config: {}", config.kiln_path.display());
    }

    if providers.is_empty() {
        warn!("No LLM providers detected");
        println!("{} No LLM provider configured.", "Error:".red().bold());
        println!();
        println!("  Crucible needs an LLM provider to chat. Choose one of these:");
        println!();
        println!(
            "  {} Set an API key environment variable:",
            "Option 1:".bold()
        );
        println!(
            "    {} (for OpenAI)",
            "export OPENAI_API_KEY=sk-...".dimmed()
        );
        println!(
            "    {} (for Anthropic)",
            "export ANTHROPIC_API_KEY=sk-ant-...".dimmed()
        );
        println!(
            "    {} (for other providers)",
            "export OPENROUTER_API_KEY=sk-...".dimmed()
        );
        println!();
        println!(
            "  {} Use the interactive credential manager:",
            "Option 2:".bold()
        );
        println!("    {}", "cru auth login".dimmed());
        println!();
        println!(
            "  {} Configure a provider in your config file:",
            "Option 3:".bold()
        );
        println!(
            "    {} (see docs/Guides/Getting Started.md)",
            "~/.config/crucible/config.toml".dimmed()
        );
        println!();
        println!(
            "  {}",
            "Chat will not work without a provider. Please set up one of the above.".dimmed()
        );
    } else {
        let has_cloud_provider = providers
            .iter()
            .any(|p| p.provider_type == "openai" || p.provider_type == "anthropic");

        if !has_cloud_provider {
            if let Some(ollama) = providers.iter().find(|p| p.provider_type == "ollama") {
                info!("Auto-detected Ollama: {}", ollama.reason);
                if config.chat.model.is_none() {
                    if let Some(ref model) = ollama.default_model {
                        config.chat.model = Some(model.clone());
                        info!("Set default model to {}", model);
                    }
                }
            }
        }

        debug!(
            "Detected {} provider(s): {:?}",
            providers.len(),
            providers.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_interactive_chat(
    config: CliConfig,
    initial_mode: &str,
    agent_name: Option<String>,
    provider_key: Option<String>,
    max_context_tokens: usize,
    parsed_env: std::collections::HashMap<String, String>,
    working_dir: Option<std::path::PathBuf>,
    resume_session_id: Option<String>,
    set_overrides: Vec<String>,
    record: Option<PathBuf>,
    replay: Option<PathBuf>,
    replay_speed: f64,
    replay_auto_exit: Option<u64>,
) -> Result<()> {
    use crate::chat::bridge::AgentEventBridge;
    use crate::chat::session::{index_kiln_notes, index_workspace_files};
    use crate::tui::oil::{ChatMode, OilChatRunner};
    use crucible_core::events::EventRing;
    use crucible_core::traits::chat::is_read_only;

    let parsed_set_overrides = {
        use crate::tui::oil::commands::{validate_set_for_cli, SetEffect};

        let mut parsed = Vec::with_capacity(set_overrides.len());
        for input in &set_overrides {
            match validate_set_for_cli(input) {
                Err(e) => {
                    eprintln!("error: invalid --set '{}': {}", input, e);
                    std::process::exit(1);
                }
                Ok(SetEffect::DaemonRpc(_)) if agent_name.is_some() => {
                    eprintln!(
                        "error: invalid --set '{}': cannot set daemon RPC keys on ACP agent sessions",
                        input
                    );
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

    let mode = ChatMode::parse(initial_mode);
    let effective_llm = config.effective_llm_provider().ok();
    let model_name = effective_llm
        .as_ref()
        .map(|p| p.model.clone())
        .unwrap_or_else(|| config.chat_model());
    let endpoint = effective_llm
        .as_ref()
        .map(|p| p.endpoint.clone())
        .or_else(|| config.chat.endpoint.clone())
        .unwrap_or_else(|| crucible_config::DEFAULT_OLLAMA_ENDPOINT.to_string());

    let context_limit = fetch_model_context_length(&endpoint, &model_name)
        .await
        .unwrap_or(0);
    if context_limit > 0 {
        info!(
            "Model {} context length: {} tokens",
            model_name, context_limit
        );
    }

    let display_model = agent_name
        .as_deref()
        .map(|n| n.to_string())
        .unwrap_or_else(|| model_name.clone());

    let recording_mode = record.as_ref().map(|_| "granular".to_string());
    let recording_path = record;

    let mut runner = OilChatRunner::new()?
        .with_mode(mode)
        .with_model(&display_model)
        .with_context_limit(context_limit)
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

    let workspace_root = working_dir.clone().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    });
    let kiln_root = config.kiln_path.clone();

    let (session_cmd_tx, session_cmd_rx) = tokio::sync::mpsc::unbounded_channel::<SessionCommand>();

    let _lua_executor = if let Ok(mut executor) = LuaExecutor::new() {
        if let Err(e) = executor.load_config(Some(&kiln_root)) {
            warn!("Failed to load Lua config: {}", e);
        } else {
            debug!("Lua configuration loaded");
        }

        let session = Session::new("chat".to_string());
        session.bind(Box::new(ChannelSessionRpc::new(session_cmd_tx)));
        executor.session_manager().set_current(session.clone());

        // Sync hooks from Lua and fire session_start hooks
        if let Err(e) = executor.sync_session_start_hooks() {
            warn!("Failed to sync session_start hooks: {}", e);
        } else {
            let hook_count = executor.session_start_hooks().len();
            if let Err(e) = executor.fire_session_start_hooks(&session) {
                warn!("Error firing session_start hooks: {}", e);
            } else {
                debug!("Fired {} session_start hooks", hook_count);
            }
        }

        Some(executor)
    } else {
        None
    };

    runner = runner.with_session_command_receiver(session_cmd_rx);

    let provider = effective_llm
        .as_ref()
        .map(|p| p.provider_type)
        .unwrap_or(crucible_config::BackendType::Ollama);
    let model_endpoint = endpoint.clone();

    let (files, notes, available_models) = tokio::join!(
        tokio::task::spawn_blocking({
            let root = workspace_root.clone();
            move || index_workspace_files(&root)
        }),
        tokio::task::spawn_blocking({
            let root = kiln_root.clone();
            move || index_kiln_notes(&root)
        }),
        async {
            if !config.llm.providers.is_empty() {
                fetch_all_provider_models(&config.llm).await
            } else {
                fetch_provider_models(&provider, &model_endpoint).await
            }
        },
    );

    if let Ok(files) = files {
        runner = runner.with_workspace_files(files);
    }
    if let Ok(notes) = notes {
        runner = runner.with_kiln_notes(notes);
    }
    if !available_models.is_empty() {
        debug!(
            count = available_models.len(),
            "Discovered models for popup"
        );
        runner = runner.with_available_models(available_models);
    }

    let mcp_servers: Vec<McpServerDisplay> = config
        .mcp
        .as_ref()
        .map(|mcp| {
            mcp.servers
                .iter()
                .map(|s| McpServerDisplay {
                    name: s.name.clone(),
                    prefix: s.prefix.trim_end_matches('_').to_string(),
                    tool_count: 0,
                    connected: false,
                })
                .collect()
        })
        .unwrap_or_default();
    if !mcp_servers.is_empty() {
        runner = runner.with_mcp_servers(mcp_servers);
    }
    if let Some(ref mcp) = config.mcp {
        runner = runner.with_mcp_config(mcp.clone());
    }

    runner = runner.with_slash_commands(known_slash_commands());

    let plugin_entries = discover_plugin_status(Some(&kiln_root));
    if !plugin_entries.is_empty() {
        runner = runner.with_plugin_status(plugin_entries);
    }

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
                    let agent = factories::create_agent(&config, params).await?;
                    Ok(agent.into_handle())
                }
                AgentSelection::Cancelled => {
                    anyhow::bail!("Agent selection was cancelled")
                }
            }
        }
    };

    runner.run_with_factory(&bridge, factory).await
}

#[allow(clippy::too_many_arguments)]
async fn run_oneshot_chat(
    config: CliConfig,
    initial_mode: &str,
    agent_name: Option<String>,
    provider_key: Option<String>,
    max_context_tokens: usize,
    parsed_env: std::collections::HashMap<String, String>,
    working_dir: Option<std::path::PathBuf>,
    resume_session_id: Option<String>,
    no_context: bool,
    no_process: bool,
    context_size: Option<usize>,
    query_text: String,
    set_overrides: Vec<String>,
) -> Result<()> {
    let mut status = StatusLine::new();
    let default_agent = config.acp.default_agent.clone();

    let mut agent_params = factories::AgentInitParams::new()
        .with_agent_name_opt(agent_name.clone().or(default_agent.clone()))
        .with_provider_opt(provider_key)
        .with_read_only(is_read_only(initial_mode))
        .with_max_context_tokens(max_context_tokens)
        .with_env_overrides(parsed_env)
        .with_resume_session_id(resume_session_id);

    if let Some(ref wd) = working_dir {
        agent_params = agent_params.with_working_dir(wd.clone());
    }

    status.update("Initializing storage...");
    let storage_handle = factories::get_storage(&config).await?;

    #[cfg(feature = "storage-surrealdb")]
    let storage_client = {
        let client = storage_handle
            .get_embedded_for_operation(&config, "chat initialization")
            .await?;
        factories::initialize_surrealdb_schema(&client).await?;
        Some(client)
    };
    #[cfg(not(feature = "storage-surrealdb"))]
    let storage_client: Option<()> = None;

    status.update("Discovering agent...");
    let initialized_agent = factories::create_agent(&config, agent_params).await?;

    let bg_progress: Option<BackgroundProgress> = if !no_process && !no_context {
        #[cfg(feature = "storage-surrealdb")]
        {
            use crate::sync::quick_sync_check;

            let storage_client = storage_client.as_ref().expect("storage required for sync");
            status.update("Checking for file changes...");
            let sync_status = quick_sync_check(storage_client, &config.kiln_path).await?;

            if sync_status.needs_processing() {
                let pending = sync_status.pending_count();
                status.update(&format!(
                    "Starting background indexing ({pending} files)..."
                ));

                let note_store = storage_handle.note_store().ok_or_else(|| {
                    anyhow::anyhow!("Storage mode does not support background indexing")
                })?;
                let pipeline = factories::create_pipeline(note_store, &config, false).await?;
                let files_to_process = sync_status.files_to_process();
                let bg_pipeline = Arc::new(pipeline);
                let progress = BackgroundProgress::new(pending);

                let bg_pipeline_clone = bg_pipeline.clone();
                let progress_clone = progress.clone();
                tokio::spawn(async move {
                    for file in files_to_process {
                        match bg_pipeline_clone.process(&file).await {
                            Ok(_) => progress_clone.inc_completed(),
                            Err(e) => {
                                tracing::warn!(
                                    "Background process failed for {}: {}",
                                    file.display(),
                                    e
                                );
                                progress_clone.inc_failed();
                            }
                        }
                    }
                });

                let watch_config = config.clone();
                let watch_pipeline = bg_pipeline;
                tokio::spawn(async move {
                    if let Err(e) = spawn_background_watch(watch_config, watch_pipeline).await {
                        tracing::error!("Background watch failed: {}", e);
                    }
                });

                Some(progress)
            } else {
                if let Some(note_store) = storage_handle.note_store() {
                    let pipeline = factories::create_pipeline(note_store, &config, false).await?;
                    let watch_config = config.clone();
                    let watch_pipeline = Arc::new(pipeline);
                    tokio::spawn(async move {
                        if let Err(e) = spawn_background_watch(watch_config, watch_pipeline).await {
                            tracing::error!("Background watch failed: {}", e);
                        }
                    });
                }
                None
            }
        }
        #[cfg(not(feature = "storage-surrealdb"))]
        {
            None
        }
    } else {
        None
    };

    status.update("Initializing core...");
    let core = Arc::new(KilnContext::from_storage_handle(storage_handle, config));

    status.update("Connecting to agent...");
    let mut handle = initialized_agent.into_handle();
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
                    eprintln!("\nError: {}", e);
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
    use crate::tui::oil::commands::{validate_set_for_cli, CliValue, SetEffect, SetRpcAction};

    let mut autoconfirm = false;

    for input in set_overrides {
        let effect = match validate_set_for_cli(input) {
            Ok(effect) => effect,
            Err(err) => {
                eprintln!("error: invalid --set '{}': {}", input, err);
                std::process::exit(1);
            }
        };

        match effect {
            SetEffect::DaemonRpc(action) => {
                if is_acp {
                    eprintln!(
                        "error: --set '{}' cannot be used with ACP agents (daemon RPC not available)",
                        input
                    );
                    std::process::exit(1);
                }
                if let Err(e) = apply_rpc_action(handle, action).await {
                    eprintln!("error: --set '{}' failed: {}", input, e);
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
    }
}

/// Spawn background watch task for chat mode using the event system
///
/// This function runs silently in the background, using the full event system
/// to handle file changes. The event cascade triggers all handlers:
/// FileChanged -> NoteParsed -> EntityStored -> BlocksUpdated -> EmbeddingGenerated
///
/// The background task will be automatically cancelled when the chat
/// command exits (tokio runtime cleanup).
async fn spawn_background_watch(config: CliConfig, _pipeline: Arc<NotePipeline>) -> Result<()> {
    use crate::event_system::initialize_event_system;

    let kiln_path = config.kiln_path.clone();

    // Initialize the full event system
    let event_handle = initialize_event_system(&config).await?;
    info!(
        "Background event system initialized with {} handlers",
        event_handle.handler_count().await
    );

    // Add watch for the kiln directory
    {
        let mut watch = event_handle.watch_manager().write().await;

        // Configure watch with markdown file filter and debouncing
        let crucible_dir = kiln_path.join(".crucible");
        let filter = EventFilter::new()
            .with_extension("md")
            .exclude_dir(crucible_dir);
        let watch_config = WatchConfig {
            id: "chat-background-watch".to_string(),
            recursive: true,
            filter: Some(filter),
            debounce: DebounceConfig::default(),
            handler_config: HandlerConfig::default(),
            mode: WatchMode::Standard,
            backend_options: Default::default(),
        };

        watch.add_watch(kiln_path.clone(), watch_config).await?;
    }

    info!(
        "Background watch started for chat mode on: {}",
        kiln_path.display()
    );

    // The event system handles everything automatically via registered handlers
    // Just wait until shutdown is requested (channel close or task cancellation)
    // The event system runs in the background processing events
    loop {
        // Sleep and check periodically - this allows cancellation
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

        // Check if watch is still running
        if !event_handle.is_watching().await {
            debug!("Watch manager stopped, exiting background watch loop");
            break;
        }
    }

    // Graceful shutdown
    event_handle.shutdown().await?;
    info!("Background watch stopped");
    Ok(())
}

use crate::tui::oil::chat_app::{ChatItem, Role};

async fn fetch_resume_history(
    session_id: &str,
    kiln_path: &std::path::Path,
) -> Result<Vec<ChatItem>> {
    use crucible_rpc::DaemonClient;

    let client = DaemonClient::connect().await?;
    let result = client
        .session_resume_from_storage(session_id, kiln_path, None, None)
        .await?;

    let history = result
        .get("history")
        .and_then(|h| h.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(events_to_chat_items(&history))
}

fn events_to_chat_items(events: &[serde_json::Value]) -> Vec<ChatItem> {
    let mut items = Vec::new();
    let mut counter = 0usize;

    for event in events {
        let event_type = event.get("event").and_then(|e| e.as_str()).unwrap_or("");
        let data = event.get("data").cloned().unwrap_or_default();

        match event_type {
            "message_complete" => {
                let full_response = data
                    .get("full_response")
                    .and_then(|r| r.as_str())
                    .unwrap_or_default();
                if !full_response.is_empty() {
                    items.push(ChatItem::Message {
                        id: format!("resume-{counter}"),
                        role: Role::Assistant,
                        content: full_response.to_string(),
                    });
                    counter += 1;
                }
            }
            "tool_call" => {
                let call_id = data
                    .get("call_id")
                    .and_then(|c| c.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let tool_name = data
                    .get("tool")
                    .and_then(|t| t.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let args = data.get("args").map(|a| a.to_string()).unwrap_or_default();

                items.push(ChatItem::ToolCall {
                    id: call_id,
                    name: tool_name,
                    args,
                    result: String::new(),
                    complete: false,
                });
                counter += 1;
            }
            "tool_result" => {
                let call_id = data.get("call_id").and_then(|c| c.as_str()).unwrap_or("");
                let result_val = data
                    .get("result")
                    .map(|r| r.to_string())
                    .unwrap_or_default();

                if let Some(ChatItem::ToolCall {
                    result, complete, ..
                }) = items
                    .iter_mut()
                    .rev()
                    .find(|item| matches!(item, ChatItem::ToolCall { id, .. } if id == call_id))
                {
                    *result = result_val;
                    *complete = true;
                }
            }
            _ => {}
        }
    }

    items
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

fn discover_plugin_status(kiln_path: Option<&std::path::Path>) -> Vec<PluginStatusEntry> {
    use crucible_lua::PluginManager;

    let manager = match PluginManager::initialize(kiln_path) {
        Ok(m) => m,
        Err(e) => {
            warn!("Plugin discovery failed: {}", e);
            return Vec::new();
        }
    };

    manager
        .list()
        .map(|p| PluginStatusEntry {
            name: p.name().to_string(),
            version: p.version().to_string(),
            state: p.state.to_string(),
            error: p.last_error.clone(),
        })
        .collect()
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
}
