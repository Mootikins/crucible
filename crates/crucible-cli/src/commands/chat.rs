//! Chat Command - ACP-based Natural Language Interface
//!
//! Provides an interactive chat interface using the Agent Client Protocol.
//! Supports toggleable plan (read-only) and act (write-enabled) modes.

use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, info, trace, warn};

use crate::acp::{discover_agent, ContextEnricher, CrucibleAcpClient};
use crate::chat::{ChatMode, ChatModeDisplay};
use crate::config::CliConfig;
use crate::core_facade::CrucibleCoreFacade;
use crate::factories;
use crate::progress::{BackgroundProgress, LiveProgress, StatusLine};
use crucible_pipeline::NotePipeline;
use crucible_watch::traits::{DebounceConfig, HandlerConfig, WatchConfig};
use crucible_watch::{EventFilter, FileEvent, FileEventKind, WatchMode};
use tokio::sync::mpsc;

/// Execute the chat command
///
/// # Arguments
/// * `config` - CLI configuration
/// * `agent_name` - Optional preferred agent name
/// * `query` - Optional one-shot query (if None, starts interactive mode)
/// * `read_only` - Initial mode: if true, starts in plan mode; if false, starts in act mode
/// * `no_context` - If true, skip context enrichment
/// * `no_process` - If true, skip auto-processing of files before context enrichment
/// * `context_size` - Number of context results to include
pub async fn execute(
    config: CliConfig,
    agent_name: Option<String>,
    query: Option<String>,
    read_only: bool,
    no_context: bool,
    no_process: bool,
    context_size: Option<usize>,
) -> Result<()> {
    // Determine initial mode
    let initial_mode = if read_only {
        ChatMode::Plan
    } else {
        ChatMode::Act
    };

    

    info!("Starting chat command");
    info!("Initial mode: {}", initial_mode.display_name());

    // Single-line status display for clean startup UX
    let mut status = StatusLine::new();

    // Get default agent from config before moving config
    let default_agent_from_config = config.acp.default_agent.clone();

    // PARALLEL INITIALIZATION: Run storage init and agent discovery concurrently
    status.update("Initializing storage and discovering agent...");

    let preferred_agent = agent_name.or(default_agent_from_config);
    let config_for_storage = config.clone();

    let (storage_result, agent_result) = tokio::join!(
        async {
            let client = factories::create_surrealdb_storage(&config_for_storage).await?;
            factories::initialize_surrealdb_schema(&client).await?;
            Ok::<_, anyhow::Error>(client)
        },
        discover_agent(preferred_agent.as_deref())
    );

    let storage_client = storage_result?;
    let agent = agent_result?;

    // Quick sync check + background processing (unless --no-process or --no-context)
    let bg_progress: Option<BackgroundProgress> = if !no_process && !no_context {
        use crate::sync::quick_sync_check;

        status.update("Checking for file changes...");

        let kiln_path = &config.kiln_path;
        let sync_status = quick_sync_check(&storage_client, kiln_path).await?;

        if sync_status.needs_processing() {
            let pending = sync_status.pending_count();
            status.update(&format!("Starting background indexing ({} files)...", pending));

            // Create pipeline for background processing
            let pipeline = factories::create_pipeline(
                storage_client.clone(),
                &config,
                false,
            )
            .await?;

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
                            tracing::warn!("Background process failed for {}: {}", file.display(), e);
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
            let pipeline = factories::create_pipeline(
                storage_client.clone(),
                &config,
                false,
            )
            .await?;
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
    let core = Arc::new(CrucibleCoreFacade::from_storage(
        storage_client.clone(),
        config,
    ));

    // Get cached embedding provider
    status.update("Initializing embedding provider...");
    let embedding_provider = factories::get_or_create_embedding_provider(core.config()).await?;

    // Get knowledge repository from storage
    let knowledge_repo = core.storage().as_knowledge_repository();

    // Create ACP client with kiln path and MCP dependencies for in-process tool execution
    let kiln_path = core.config().kiln_path.clone();
    let acp_config = core.config().acp.clone();
    let mut client =
        CrucibleAcpClient::with_acp_config(agent, initial_mode.is_read_only(), acp_config)
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
        // Interactive mode
        info!("Interactive chat mode");
        run_interactive_session(core, &mut client, initial_mode, no_context, context_size, live_progress).await?;

        // Cleanup
        client.shutdown().await?;
    }

    Ok(())
}

/// Run an interactive chat session with mode toggling support
async fn run_interactive_session(
    core: Arc<CrucibleCoreFacade>,
    client: &mut CrucibleAcpClient,
    initial_mode: ChatMode,
    no_context: bool,
    context_size: Option<usize>,
    _live_progress: Option<LiveProgress>,
) -> Result<()> {
    use crate::chat::{ChatSession, SessionConfig};

    // Create session configuration
    let session_config = SessionConfig::new(
        initial_mode,
        !no_context,  // context_enabled = !no_context
        context_size,
    );

    // Create session orchestrator
    let mut session = ChatSession::new(session_config, core);

    // Run interactive session
    session.run(client).await
}

/// Spawn background watch task for chat mode
///
/// This function runs silently in the background, watching for file changes
/// and reprocessing them automatically. All output goes through tracing
/// (logged to file) to avoid polluting stdio used for JSON-RPC.
///
/// The background task will be automatically cancelled when the chat
/// command exits (tokio runtime cleanup).
async fn spawn_background_watch(config: CliConfig, pipeline: Arc<NotePipeline>) -> Result<()> {
    let kiln_path = config.kiln_path.clone();

    // Create watcher via factory (DIP pattern - depends only on FileWatcher trait)
    let mut watcher_arc = factories::create_file_watcher(&config)?;

    // Get mutable access to configure the watcher
    let watcher = Arc::get_mut(&mut watcher_arc)
        .ok_or_else(|| anyhow::anyhow!("Failed to get mutable watcher reference"))?;

    // Create event channel
    let (tx, mut rx) = mpsc::unbounded_channel::<FileEvent>();
    watcher.set_event_sender(tx);

    // Configure watch with markdown file filter and debouncing
    // Exclude .crucible directory (contains SurrealDB database files)
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
        mode: WatchMode::Standard, // Standard mode for immediate event processing
        backend_options: Default::default(),
    };

    // Start watching the kiln directory
    let _handle = watcher.watch(kiln_path.clone(), watch_config).await?;
    info!(
        "Background watch started for chat mode on: {}",
        kiln_path.display()
    );

    // Event processing loop (runs until chat exits)
    while let Some(event) = rx.recv().await {
        match event.kind {
            FileEventKind::Created | FileEventKind::Modified => {
                debug!("Background watch detected change: {}", event.path.display());

                // Silently reprocess changed file
                match pipeline.process(&event.path).await {
                    Ok(crucible_core::processing::ProcessingResult::Success { .. }) => {
                        debug!("Background reprocessed: {}", event.path.display());
                    }
                    Ok(crucible_core::processing::ProcessingResult::Skipped)
                    | Ok(crucible_core::processing::ProcessingResult::NoChanges) => {
                        trace!("Background skipped (unchanged): {}", event.path.display());
                    }
                    Err(e) => {
                        warn!(
                            "Background reprocess failed for {}: {}",
                            event.path.display(),
                            e
                        );
                    }
                }
            }
            FileEventKind::Deleted => {
                debug!("File deleted: {}", event.path.display());
                // Could mark as deleted in DB in future
            }
            _ => {
                trace!(
                    "Ignoring event: {:?} for {}",
                    event.kind,
                    event.path.display()
                );
            }
        }
    }

    info!("Background watch stopped");
    Ok(())
}
