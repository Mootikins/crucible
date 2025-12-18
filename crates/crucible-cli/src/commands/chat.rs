//! Chat Command - ACP-based Natural Language Interface

//!
//! Provides an interactive chat interface using the Agent Client Protocol.
//! Supports toggleable plan (read-only) and act (write-enabled) modes.

use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::acp::{ContextEnricher, CrucibleAcpClient};
use crate::config::CliConfig;
use crate::core_facade::KilnContext;
use crate::factories;
use crate::progress::{BackgroundProgress, LiveProgress, StatusLine};
use crucible_core::traits::chat::{is_read_only, mode_display_name};
use crucible_pipeline::NotePipeline;
use crucible_watch::traits::{DebounceConfig, HandlerConfig, WatchConfig};
use crucible_watch::{EventFilter, WatchMode};

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
) -> Result<()> {
    // Determine initial mode
    let initial_mode = if read_only { "plan" } else { "act" };

    info!("Starting chat command");
    info!("Initial mode: {}", mode_display_name(initial_mode));

    // Single-line status display for clean startup UX
    let mut status = StatusLine::new();

    // Get default agent from config before moving config
    let default_agent_from_config = config.acp.default_agent.clone();

    // Determine agent type
    let agent_type = if use_internal {
        factories::AgentType::Internal
    } else {
        factories::AgentType::Acp
    };

    // Create agent initialization params
    let agent_params = factories::AgentInitParams::new()
        .with_type(agent_type)
        .with_agent_name_opt(agent_name.clone().or(default_agent_from_config))
        .with_provider_opt(provider_key)
        .with_read_only(is_read_only(initial_mode))
        .with_max_context_tokens(max_context_tokens);

    // PARALLEL INITIALIZATION: Run storage init and agent creation concurrently
    let init_msg = if use_internal {
        "Initializing storage and LLM provider..."
    } else {
        "Initializing storage and discovering agent..."
    };
    status.update(init_msg);

    let config_for_storage = config.clone();
    let config_for_agent = config.clone();

    let (storage_result, agent_result) = tokio::join!(
        async {
            let client = factories::create_surrealdb_storage(&config_for_storage).await?;
            factories::initialize_surrealdb_schema(&client).await?;
            Ok::<_, anyhow::Error>(client)
        },
        factories::create_agent(&config_for_agent, agent_params)
    );

    let storage_client = storage_result?;
    let initialized_agent = agent_result?;

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
            let pipeline =
                factories::create_pipeline(storage_client.clone(), &config, false).await?;

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
            let pipeline =
                factories::create_pipeline(storage_client.clone(), &config, false).await?;
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
        None
    };

    // Initialize core facade
    status.update("Initializing core...");
    let core = Arc::new(KilnContext::from_storage(storage_client.clone(), config));

    // Get cached embedding provider
    status.update("Initializing embedding provider...");
    let embedding_provider = factories::get_or_create_embedding_provider(core.config()).await?;

    // Get knowledge repository from storage
    let knowledge_repo = core.storage().as_knowledge_repository();

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

            // Start live progress display if we have background processing
            let live_progress = bg_progress.map(LiveProgress::start);

            // Handle query
            if let Some(query_text) = query {
                // One-shot mode
                info!("One-shot query mode");

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
            } else {
                // Interactive mode with TUI
                info!("Interactive chat mode");

                run_interactive_session(
                    core,
                    &mut client,
                    initial_mode,
                    no_context,
                    context_size,
                    live_progress,
                )
                .await?;

                // Cleanup
                client.shutdown().await?;
            }
        }
        factories::InitializedAgent::Internal(mut handle) => {
            // Internal agent is ready immediately
            status.success("Ready");

            // Start live progress display if we have background processing
            let live_progress = bg_progress.map(LiveProgress::start);

            // Handle query
            if let Some(query_text) = query {
                // One-shot mode
                info!("One-shot query mode (internal agent)");

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

                let mut stream = handle.send_message_stream(&prompt);
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
            } else {
                // Interactive mode with TUI
                info!("Interactive chat mode (internal agent)");

                run_interactive_session_internal(
                    core,
                    &mut handle,
                    initial_mode,
                    no_context,
                    context_size,
                    live_progress,
                )
                .await?;
            }
        }
    }

    Ok(())
}

/// Run an interactive chat session with mode toggling support (ACP agent)
async fn run_interactive_session(
    core: Arc<KilnContext>,
    client: &mut CrucibleAcpClient,
    initial_mode: &str,
    no_context: bool,
    context_size: Option<usize>,
    _live_progress: Option<LiveProgress>,
) -> Result<()> {
    use crate::chat::{ChatSession, SessionConfig};

    // Create session configuration
    let session_config = SessionConfig::new(
        initial_mode,
        !no_context, // context_enabled = !no_context
        context_size,
    );

    // Create session orchestrator
    let mut session = ChatSession::new(session_config, core);

    // Run interactive session
    session.run(client).await
}

/// Run an interactive chat session with mode toggling support (internal agent)
async fn run_interactive_session_internal(
    core: Arc<KilnContext>,
    handle: &mut crucible_agents::InternalAgentHandle,
    initial_mode: &str,
    no_context: bool,
    context_size: Option<usize>,
    _live_progress: Option<LiveProgress>,
) -> Result<()> {
    use crate::chat::{ChatSession, SessionConfig};

    // Create session configuration
    let session_config = SessionConfig::new(
        initial_mode,
        !no_context, // context_enabled = !no_context
        context_size,
    );

    // Create session orchestrator
    let mut session = ChatSession::new(session_config, core);

    // Run interactive session with internal agent
    session.run(handle).await
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
