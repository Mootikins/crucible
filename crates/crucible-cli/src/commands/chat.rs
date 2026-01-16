//! Chat Command - ACP-based Natural Language Interface

//!
//! Provides an interactive chat interface using the Agent Client Protocol.
//! Supports toggleable plan (read-only) and act (write-enabled) modes.

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::acp::{ContextEnricher, CrucibleAcpClient};
use crate::chat::DynamicAgent;
use crate::config::CliConfig;
use crate::core_facade::KilnContext;
use crate::factories;
use crate::progress::{BackgroundProgress, LiveProgress, StatusLine};
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

/// Execute the chat command
///
/// # Arguments
/// * `config` - CLI configuration
/// * `agent_name` - Optional preferred ACP agent name
/// * `query` - Optional one-shot query (if None, starts interactive mode)
/// * `read_only` - Initial mode: if true, starts in plan mode; if false, starts in act mode
/// * `no_context` - If true, skip context enrichment
/// * `no_process` - If true, skip auto-processing of files before context enrichment
/// * `context_size` - Number of context results to include
/// * `use_internal` - If true, use internal LLM agent instead of ACP agent
/// * `provider_key` - Optional LLM provider for internal agent
/// * `max_context_tokens` - Maximum context window tokens for internal agent
/// * `env_overrides` - Environment variables to pass to ACP agent (KEY=VALUE pairs)
/// * `resume_session_id` - Optional session ID to resume from
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    config: CliConfig,
    agent_name: Option<String>,
    query: Option<String>,
    read_only: bool,
    no_context: bool,
    no_process: bool,
    context_size: Option<usize>,
    use_internal: bool,
    provider_key: Option<String>,
    max_context_tokens: usize,
    env_overrides: Vec<String>,
    resume_session_id: Option<String>,
    fullscreen: bool,
    use_ink_runner: bool,
) -> Result<()> {
    // Determine initial mode
    let initial_mode = if read_only { "plan" } else { "act" };

    info!("Starting chat command");
    info!("Initial mode: {}", mode_display_name(initial_mode));

    // Note: DB lock detection is now handled in factories::get_storage()
    // with proper detection of orphan daemon processes

    // Single-line status display for clean startup UX
    let mut status = StatusLine::new();

    // Get default agent from config before moving config
    let default_agent_from_config = config.acp.default_agent.clone();

    // Parse env overrides from CLI (KEY=VALUE format) - needed for both paths
    let parsed_env: std::collections::HashMap<String, String> = env_overrides
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

    if !parsed_env.is_empty() {
        let keys: Vec<_> = parsed_env.keys().collect();
        info!("CLI env overrides: {:?}", keys);
    }

    // Capture current working directory for agent initialization
    // This ensures the agent operates in the user's current directory (where they invoked cru)
    // This is distinct from the kiln path, which is where knowledge is stored.
    let working_dir = std::env::current_dir().ok();

    // === INTERACTIVE MODE: Always use factory path for /new restart support ===
    let is_interactive = query.is_none();
    if is_interactive {
        // Compute preselected agent from CLI args and config
        // Priority: CLI flags > config agent_preference > config default_agent
        use crucible_config::AgentPreference;
        let preselected_agent = if use_internal {
            // Explicit CLI flag for internal agent
            Some(AgentSelection::Internal)
        } else if let Some(ref name) = agent_name {
            // Explicit CLI flag for specific ACP agent
            Some(AgentSelection::Acp(name.clone()))
        } else {
            // No CLI flags - use config preference
            match config.chat.agent_preference {
                AgentPreference::Crucible => Some(AgentSelection::Internal),
                AgentPreference::Acp => {
                    // Use configured default agent or discover
                    default_agent_from_config.clone().map(AgentSelection::Acp)
                }
            }
        };

        return run_deferred_chat(
            config,
            default_agent_from_config,
            initial_mode,
            no_context,
            no_process,
            context_size,
            provider_key,
            max_context_tokens,
            parsed_env,
            working_dir.clone(),
            status,
            preselected_agent,
            resume_session_id,
            fullscreen,
            use_ink_runner,
        )
        .await;
    }

    // === ONE-SHOT MODE: Agent created before query execution ===

    // Determine agent type based on selection
    let agent_type = if use_internal {
        factories::AgentType::Internal
    } else {
        factories::AgentType::Acp
    };

    // Create agent initialization params
    let mut agent_params = factories::AgentInitParams::new()
        .with_type(agent_type)
        .with_agent_name_opt(agent_name.clone().or(default_agent_from_config.clone()))
        .with_provider_opt(provider_key)
        .with_read_only(is_read_only(initial_mode))
        .with_max_context_tokens(max_context_tokens)
        .with_env_overrides(parsed_env);

    // Set working directory if available
    if let Some(ref wd) = working_dir {
        agent_params = agent_params.with_working_dir(wd.clone());
    }

    // Fetch available models for OpenCode (unused in one-shot mode, but kept for future use)
    let _available_models = if agent_type == factories::AgentType::Acp {
        let effective_agent = agent_name.clone().or(default_agent_from_config);
        if effective_agent.as_deref() == Some("opencode") {
            status.update("Fetching available models from OpenCode...");
            crate::acp::models::fetch_opencode_models().await.ok()
        } else {
            None
        }
    } else {
        None
    };

    // Initialize storage first (needed for kiln tools in internal agents)
    status.update("Initializing storage...");
    let storage_handle = factories::get_storage(&config).await?;
    let storage_client = storage_handle
        .get_embedded_for_operation(&config, "chat initialization")
        .await?;
    factories::initialize_surrealdb_schema(&storage_client).await?;

    // For internal agents, create kiln context for knowledge tools
    let initialized_agent = if use_internal {
        status.update("Initializing LLM provider with kiln tools...");

        // Create kiln context for knowledge base access
        let embedding_provider = factories::get_or_create_embedding_provider(&config).await?;
        let knowledge_repo = storage_client.as_knowledge_repository();
        let kiln_ctx =
            crucible_rig::KilnContext::new(&config.kiln_path, knowledge_repo, embedding_provider);

        let agent_params = agent_params.with_kiln_context(kiln_ctx);
        factories::create_agent(&config, agent_params).await?
    } else {
        // ACP agents don't need kiln tools directly (they use MCP)
        status.update("Discovering agent...");
        factories::create_agent(&config, agent_params).await?
    };

    // Quick sync check + background processing (unless --no-process or --no-context)
    let bg_progress: Option<BackgroundProgress> = if !no_process && !no_context {
        use crate::sync::quick_sync_check;

        status.update("Checking for file changes...");

        let kiln_path = &config.kiln_path;
        let sync_status = quick_sync_check(&storage_client, kiln_path).await?;

        if sync_status.needs_processing() {
            let pending = sync_status.pending_count();
            status.update(&format!(
                "Starting background indexing ({} files)...",
                pending
            ));

            // Create pipeline for background processing
            let note_store = storage_handle.note_store().ok_or_else(|| {
                anyhow::anyhow!("Storage mode does not support background indexing")
            })?;
            let pipeline = factories::create_pipeline(note_store, &config, false).await?;

            let files_to_process = sync_status.files_to_process();
            let bg_pipeline = Arc::new(pipeline);
            let progress = BackgroundProgress::new(pending);

            // Spawn background processing with progress tracking
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
                tracing::debug!("Background file processing completed");
            });

            // Spawn background watch task
            let watch_config = config.clone();
            let watch_pipeline = bg_pipeline;
            tokio::spawn(async move {
                if let Err(e) = spawn_background_watch(watch_config, watch_pipeline).await {
                    tracing::error!("Background watch failed: {}", e);
                }
            });

            info!("Background processing spawned for {} files", pending);
            Some(progress)
        } else {
            // All files up to date, still spawn watch
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
    } else {
        None
    };

    // Initialize core facade
    status.update("Initializing core...");
    let core = Arc::new(KilnContext::from_storage(storage_client.clone(), config));

    // Get cached embedding provider
    status.update("Initializing embedding provider...");
    let embedding_provider = factories::get_or_create_embedding_provider(core.config()).await?;

    // Get knowledge repository from storage (one-shot mode always has embedded client)
    let knowledge_repo = storage_client.as_knowledge_repository();

    // Handle agent type specific setup and session execution
    match initialized_agent {
        factories::InitializedAgent::Acp(client) => {
            // Configure ACP client with MCP dependencies for in-process tool execution
            let kiln_path = core.config().kiln_path.clone();
            let mut client = client
                .with_kiln_path(kiln_path)
                .with_mcp_dependencies(knowledge_repo, embedding_provider);

            // Spawn agent (tools will be initialized via in-process SSE MCP server)
            status.update("Connecting to agent...");
            client.spawn().await?;

            // Finalize startup status
            status.success("Ready");

            // One-shot mode - query is guaranteed Some here (interactive returned early)
            let query_text = query.expect("query should be Some in one-shot path");
            info!("One-shot query mode");
            let _live_progress = bg_progress.map(LiveProgress::start);

            let prompt = if no_context {
                info!("Context enrichment disabled");
                query_text
            } else {
                // Enrich with context
                info!("Enriching query with context...");
                let enricher = ContextEnricher::new(core.clone(), context_size);
                enricher.enrich(&query_text).await?
            };

            // Start chat with enriched prompt
            client.start_chat(&prompt).await?;

            // Cleanup
            client.shutdown().await?;
        }
        factories::InitializedAgent::Internal(mut handle) => {
            // Internal agent is ready immediately
            status.success("Ready");

            // One-shot mode - query is guaranteed Some here (interactive returned early)
            let query_text = query.expect("query should be Some in one-shot path");
            info!("One-shot query mode (internal agent)");
            let _live_progress = bg_progress.map(LiveProgress::start);

            let prompt = if no_context {
                info!("Context enrichment disabled");
                query_text
            } else {
                // Enrich with context
                info!("Enriching query with context...");
                let enricher = ContextEnricher::new(core.clone(), context_size);
                enricher.enrich(&query_text).await?
            };

            // Send message and stream response, accumulate for markdown rendering
            use crate::tui::MarkdownRenderer;
            use crucible_core::traits::chat::AgentHandle;
            use futures::StreamExt;

            let renderer = MarkdownRenderer::new();
            let mut response_content = String::new();

            let mut stream = handle.send_message_stream(prompt.clone());
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

            // Render and print the complete response with markdown
            let rendered = renderer.render(&response_content);
            println!("{}", rendered);
        }
    }

    Ok(())
}

/// Run chat with deferred agent creation (picker in TUI)
///
/// The agent is created AFTER the user selects it in the TUI picker.
/// This avoids showing a separate picker screen before entering the TUI.
#[allow(clippy::too_many_arguments)]
async fn run_deferred_chat(
    config: CliConfig,
    default_agent: Option<String>,
    initial_mode: &str,
    no_context: bool,
    no_process: bool,
    context_size: Option<usize>,
    provider_key: Option<String>,
    max_context_tokens: usize,
    parsed_env: std::collections::HashMap<String, String>,
    working_dir: Option<std::path::PathBuf>,
    mut status: StatusLine,
    preselected_agent: Option<AgentSelection>,
    resume_session_id: Option<String>,
    fullscreen: bool,
    use_ink_runner: bool,
) -> Result<()> {
    use crate::chat::{ChatSession, ChatSessionConfig};

    info!("Using deferred agent creation (picker in TUI)");

    if use_ink_runner {
        return run_ink_chat(
            config,
            default_agent,
            initial_mode,
            provider_key,
            max_context_tokens,
            parsed_env,
            working_dir,
            status,
        )
        .await;
    }

    status.update("Initializing storage...");
    let storage_handle = factories::get_storage(&config).await?;

    // Background processing only in embedded mode
    // Daemon mode: the db-server already handles schema init and file watching should
    // be a separate daemon responsibility
    let _bg_progress: Option<BackgroundProgress> = if storage_handle.is_embedded()
        && !no_process
        && !no_context
    {
        // Only embedded mode can do schema init and background processing
        let storage_client = storage_handle.as_embedded();
        factories::initialize_surrealdb_schema(storage_client).await?;

        use crate::sync::quick_sync_check;

        status.update("Checking for file changes...");

        let kiln_path = &config.kiln_path;
        let sync_status = quick_sync_check(storage_client, kiln_path).await?;

        if sync_status.needs_processing() {
            let pending = sync_status.pending_count();
            status.update(&format!(
                "Starting background indexing ({} files)...",
                pending
            ));

            let note_store = storage_handle
                .note_store()
                .ok_or_else(|| anyhow::anyhow!("Storage mode does not support NoteStore"))?;
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
                tracing::debug!("Background file processing completed");
            });

            let watch_config = config.clone();
            let watch_pipeline = bg_pipeline;
            tokio::spawn(async move {
                if let Err(e) = spawn_background_watch(watch_config, watch_pipeline).await {
                    tracing::error!("Background watch failed: {}", e);
                }
            });

            info!("Background processing spawned for {} files", pending);
            Some(progress)
        } else {
            let note_store = storage_handle
                .note_store()
                .ok_or_else(|| anyhow::anyhow!("Storage mode does not support NoteStore"))?;
            let pipeline = factories::create_pipeline(note_store, &config, false).await?;
            let watch_config = config.clone();
            let watch_pipeline = Arc::new(pipeline);
            tokio::spawn(async move {
                if let Err(e) = spawn_background_watch(watch_config, watch_pipeline).await {
                    tracing::error!("Background watch failed: {}", e);
                }
            });
            None
        }
    } else {
        if storage_handle.is_daemon() {
            info!("Running in daemon mode - schema init and background processing handled by db-server");
        }
        None
    };

    // Initialize core facade (works with both embedded and daemon modes)
    status.update("Initializing core...");
    let core = Arc::new(KilnContext::from_storage_handle(
        storage_handle.clone(),
        config.clone(),
    ));

    // Get cached embedding provider
    status.update("Initializing embedding provider...");
    let embedding_provider = factories::get_or_create_embedding_provider(core.config()).await?;

    // Get knowledge repository from storage handle (works with both modes)
    let knowledge_repo = storage_handle
        .as_knowledge_repository()
        .ok_or_else(|| anyhow::anyhow!("Knowledge repository not available in lightweight mode"))?;

    // Clear status line - TUI will take over with its own display
    // Note: We don't print "Ready" since TUI's EnterAlternateScreen clears the screen
    status.update("");

    // Create session configuration
    let mut session_config = ChatSessionConfig::new(initial_mode, !no_context, context_size);

    // Set up session logging to appropriate kiln
    if let Some(kiln_path) = select_session_kiln(&config) {
        session_config = session_config.with_session_kiln(kiln_path);
    }

    // Set preselected agent if provided (skips picker first time, allows /new restart)
    if let Some(selection) = preselected_agent {
        session_config = session_config.with_default_selection(selection);
    }

    // Set session to resume from (loads existing conversation history)
    if let Some(session_id) = resume_session_id {
        info!("Will resume session: {}", session_id);
        session_config = session_config.with_resume_session(session_id);
    }

    session_config = session_config
        .with_fullscreen(fullscreen)
        .with_ink_runner(use_ink_runner);

    let mut session = ChatSession::new(session_config, core.clone(), None);

    // Create the agent factory closure
    // This captures all dependencies needed to create the agent after picker selection
    let config_for_factory = config.clone();
    let initial_mode_str = initial_mode.to_string();
    let working_dir_for_factory = working_dir.clone();
    let factory = move |selection: AgentSelection| {
        let config = config_for_factory.clone();
        let default_agent = default_agent.clone();
        let provider_key = provider_key.clone();
        let parsed_env = parsed_env.clone();
        let core = core.clone();
        let embedding_provider = embedding_provider.clone();
        let knowledge_repo = knowledge_repo.clone();
        let initial_mode = initial_mode_str.clone();
        let working_dir = working_dir_for_factory.clone();

        async move {
            match selection {
                AgentSelection::Acp(agent_name) => {
                    info!("Creating ACP agent: {}", agent_name);

                    let mut params = factories::AgentInitParams::new()
                        .with_type(factories::AgentType::Acp)
                        .with_agent_name_opt(Some(agent_name).or(default_agent))
                        .with_provider_opt(provider_key)
                        .with_read_only(is_read_only(&initial_mode))
                        .with_max_context_tokens(max_context_tokens)
                        .with_env_overrides(parsed_env);

                    // Set working directory if available
                    if let Some(wd) = working_dir.clone() {
                        params = params.with_working_dir(wd);
                    }

                    let agent = factories::create_agent(&config, params).await?;

                    match agent {
                        factories::InitializedAgent::Acp(client) => {
                            let kiln_path = core.config().kiln_path.clone();
                            let mut client = client
                                .with_kiln_path(kiln_path)
                                .with_mcp_dependencies(knowledge_repo, embedding_provider);
                            client.spawn().await?;
                            Ok(DynamicAgent::acp(client))
                        }
                        factories::InitializedAgent::Internal(_) => {
                            anyhow::bail!("Expected ACP agent but got Internal")
                        }
                    }
                }
                AgentSelection::Internal => {
                    info!("Creating internal agent");

                    let mut params = factories::AgentInitParams::new()
                        .with_type(factories::AgentType::Internal)
                        .with_provider_opt(provider_key)
                        .with_read_only(is_read_only(&initial_mode))
                        .with_max_context_tokens(max_context_tokens)
                        .with_env_overrides(parsed_env);

                    // Set working directory for internal agents (Rig handles tools internally)
                    if let Some(ref wd) = working_dir {
                        params = params.with_working_dir(wd.clone());
                    }

                    let agent = factories::create_agent(&config, params).await?;

                    match agent {
                        factories::InitializedAgent::Internal(handle) => {
                            Ok(DynamicAgent::local(handle))
                        }
                        factories::InitializedAgent::Acp(_) => {
                            anyhow::bail!("Expected Internal agent but got ACP")
                        }
                    }
                }
                AgentSelection::Cancelled => {
                    // This shouldn't happen - runner handles cancellation before calling factory
                    anyhow::bail!("Agent selection was cancelled")
                }
            }
        }
    };

    // Run deferred session - picker runs in TUI, then factory creates agent
    session.run_deferred(factory).await
}

#[allow(clippy::too_many_arguments)]
async fn run_ink_chat(
    config: CliConfig,
    default_agent: Option<String>,
    initial_mode: &str,
    provider_key: Option<String>,
    max_context_tokens: usize,
    parsed_env: std::collections::HashMap<String, String>,
    working_dir: Option<std::path::PathBuf>,
    status: StatusLine,
) -> Result<()> {
    use crate::chat::bridge::AgentEventBridge;
    use crate::chat::session::{index_kiln_notes, index_workspace_files};
    use crate::tui::ink::{ChatMode, InkChatRunner};
    use crucible_core::traits::chat::is_read_only;
    use crucible_rune::SessionBuilder;

    let _ = status;

    let session = SessionBuilder::with_generated_id("chat").build();
    let ring = session.ring().clone();
    let bridge = AgentEventBridge::new(session.handle(), ring);

    let mode = ChatMode::parse(initial_mode);
    let mut runner = InkChatRunner::new()?.with_mode(mode);

    let workspace_root = working_dir.clone().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    });
    let kiln_root = config.kiln_path.clone();

    let (files, notes) = tokio::join!(
        tokio::task::spawn_blocking({
            let root = workspace_root.clone();
            move || index_workspace_files(&root)
        }),
        tokio::task::spawn_blocking({
            let root = kiln_root.clone();
            move || index_kiln_notes(&root)
        }),
    );

    if let Ok(files) = files {
        runner = runner.with_workspace_files(files);
    }
    if let Ok(notes) = notes {
        runner = runner.with_kiln_notes(notes);
    }

    let session_dir = config
        .kiln_path
        .join(".crucible")
        .join("sessions")
        .join(session.session_id());
    std::fs::create_dir_all(&session_dir).ok();
    runner = runner.with_session_dir(session_dir);

    let config_for_factory = config.clone();
    let initial_mode_str = initial_mode.to_string();
    let factory = move |selection: AgentSelection| {
        let config = config_for_factory.clone();
        let default_agent = default_agent.clone();
        let provider_key = provider_key.clone();
        let parsed_env = parsed_env.clone();
        let working_dir = working_dir.clone();
        let initial_mode = initial_mode_str.clone();

        async move {
            match selection {
                AgentSelection::Acp(agent_name) => {
                    let mut params = factories::AgentInitParams::new()
                        .with_type(factories::AgentType::Acp)
                        .with_agent_name_opt(Some(agent_name).or(default_agent))
                        .with_provider_opt(provider_key)
                        .with_read_only(is_read_only(&initial_mode))
                        .with_max_context_tokens(max_context_tokens)
                        .with_env_overrides(parsed_env);

                    if let Some(wd) = working_dir {
                        params = params.with_working_dir(wd);
                    }

                    let agent = factories::create_agent(&config, params).await?;

                    match agent {
                        factories::InitializedAgent::Acp(mut client) => {
                            client.spawn().await?;
                            Ok(DynamicAgent::acp(client))
                        }
                        factories::InitializedAgent::Internal(_) => {
                            anyhow::bail!("Expected ACP agent but got Internal")
                        }
                    }
                }
                AgentSelection::Internal => {
                    let mut params = factories::AgentInitParams::new()
                        .with_type(factories::AgentType::Internal)
                        .with_provider_opt(provider_key)
                        .with_read_only(is_read_only(&initial_mode))
                        .with_max_context_tokens(max_context_tokens)
                        .with_env_overrides(parsed_env);

                    if let Some(wd) = working_dir {
                        params = params.with_working_dir(wd);
                    }

                    let agent = factories::create_agent(&config, params).await?;

                    match agent {
                        factories::InitializedAgent::Internal(handle) => {
                            Ok(DynamicAgent::local(handle))
                        }
                        factories::InitializedAgent::Acp(_) => {
                            anyhow::bail!("Expected Internal agent but got ACP")
                        }
                    }
                }
                AgentSelection::Cancelled => {
                    anyhow::bail!("Agent selection was cancelled")
                }
            }
        }
    };

    runner.run_with_factory(&bridge, factory).await
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
