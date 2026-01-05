//! Process Command - Explicit Pipeline Processing
//!
//! Runs the note processing pipeline on files in the kiln.
//!
//! When `--watch` is enabled, this command uses the full event system to process
//! file changes through the event cascade:
//! ```text
//! FileChanged -> NoteParsed -> EntityStored -> BlocksUpdated -> EmbeddingGenerated
//! ```
//!
//! Note lifecycle events (`NoteParsed`, `NoteModified`) are emitted through the
//! Reactor during processing, allowing Rune handlers to react to note changes.

use anyhow::Result;
use crucible_core::events::{NoteChangeType, Reactor, SessionEvent};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use tracing::{debug, info, warn};
use walkdir::WalkDir;

use crate::config::CliConfig;
use crate::event_system::initialize_event_system;
use crate::{factories, output};
use crucible_daemon_client::{lifecycle, DaemonClient};
use crucible_watch::traits::{DebounceConfig, HandlerConfig, WatchConfig};
use crucible_watch::{EventFilter, WatchMode};

/// Execute the process command
///
/// # Arguments
/// * `config` - CLI configuration
/// * `path` - Optional specific file/directory to process (if None, processes entire kiln)
/// * `force` - If true, reprocess all files regardless of changes
/// * `watch` - If true, continue watching for changes after initial processing
/// * `verbose` - If true, show detailed progress and timing information
/// * `dry_run` - If true, preview changes without writing to database
/// * `parallel` - Optional number of parallel workers (default: num_cpus / 2)
pub async fn execute(
    config: CliConfig,
    path: Option<PathBuf>,
    force: bool,
    watch: bool,
    _verbose: bool,
    dry_run: bool,
    parallel: Option<usize>,
) -> Result<()> {
    info!("Starting process command");

    // Process command needs direct database access, so we must stop any running daemon
    let socket = lifecycle::default_socket_path();
    let stopped_daemon = if lifecycle::is_daemon_running(&socket) {
        info!("Stopping daemon for direct database access");
        output::info("Stopping daemon for direct database access...");
        if let Ok(client) = DaemonClient::connect().await {
            let _ = client.shutdown().await;
        }

        // Wait for daemon to fully exit and release the lock
        let db_path = config.database_path();
        for _ in 0..20 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if !lifecycle::is_daemon_running(&socket) && !lifecycle::is_db_locked(&db_path) {
                info!("Daemon stopped and released database lock");
                break;
            }
        }
        true
    } else {
        false
    };

    // Force embedded mode for process command since we need direct DB access
    let mut config = config;
    config.storage = Some(crucible_config::StorageConfig {
        mode: crucible_config::StorageMode::Embedded,
        ..config.storage.clone().unwrap_or_default()
    });

    // Determine target path
    let target_path = path.as_deref().unwrap_or(config.kiln_path.as_path());

    info!("Processing path: {}", target_path.display());
    info!("Force reprocess: {}", force);
    info!("Watch mode: {}", watch);
    info!("Dry-run mode: {}", dry_run);

    // Initialize storage using factory pattern
    output::info("Initializing storage...");
    let storage_handle = factories::get_storage(&config).await?;
    let storage_client = storage_handle
        .get_embedded_for_operation(&config, "file processing")
        .await?;
    factories::initialize_surrealdb_schema(&storage_client).await?;
    output::success("Storage initialized");

    // Create pipeline (wrapped in Arc for sharing across tasks)
    output::info("Creating processing pipeline...");
    let pipeline =
        Arc::new(factories::create_pipeline(storage_client.clone(), &config, force).await?);
    output::success("Pipeline ready");

    // Initialize Reactor for note lifecycle events
    // This allows Rune handlers to react to note processing
    let reactor = Arc::new(RwLock::new(Reactor::new()));
    load_rune_handlers(&mut *reactor.write().await, &config.kiln_path).await;
    let handler_count = reactor.read().await.handler_count();
    if handler_count > 0 {
        info!("Loaded {} Rune handlers for note events", handler_count);
    }

    // Discover files to process
    let files = discover_markdown_files(target_path)?;
    info!("Found {} markdown files", files.len());

    if files.is_empty() {
        println!("No markdown files found to process");
        return Ok(());
    }

    // Create progress bar
    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
            )?
            .progress_chars("#>-"),
    );

    // Determine number of parallel workers for embedding generation
    // Priority: CLI flag > config file > embedding provider default
    //
    // Provider-specific defaults prevent rate limiting:
    // - Ollama: 1 (single GPU, sequential to avoid OOM)
    // - FastEmbed: num_cpus/2 (CPU-bound, parallel OK)
    // - Remote APIs: 8 (rate-limited, moderate concurrency)
    let embedding_max_concurrent = config.embedding.get_max_concurrent();
    let workers = parallel
        .or(config.parallel_workers())
        .unwrap_or(embedding_max_concurrent)
        .max(1); // At least 1 worker

    info!(
        "Using {} parallel workers (embedding provider default: {})",
        workers, embedding_max_concurrent
    );

    // Process files through pipeline
    if dry_run {
        println!("\n🔍 DRY RUN MODE - No changes will be made");
    }
    println!(
        "\n🔄 Processing {} files through pipeline (with {} workers)...",
        files.len(),
        workers
    );

    // Use atomic counters for thread-safe updates
    let processed_count = Arc::new(AtomicUsize::new(0));
    let skipped_count = Arc::new(AtomicUsize::new(0));
    let error_count = Arc::new(AtomicUsize::new(0));

    // Bounded concurrency with semaphore
    let semaphore = Arc::new(Semaphore::new(workers));
    let pb = Arc::new(pb);
    let mut handles = Vec::new();

    for file in files {
        let permit = semaphore.clone().acquire_owned().await?;
        let pipeline = pipeline.clone();
        let reactor = reactor.clone();
        let pb = pb.clone();
        let processed = processed_count.clone();
        let skipped = skipped_count.clone();
        let errors = error_count.clone();

        let handle = tokio::spawn(async move {
            let _permit = permit; // Release on drop

            let file_name = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            // Run file through the 5-phase pipeline
            if dry_run {
                processed.fetch_add(1, Ordering::Relaxed);
            } else {
                match pipeline.process(&file).await {
                    Ok(crucible_core::processing::ProcessingResult::Success {
                        changed_blocks,
                        ..
                    }) => {
                        processed.fetch_add(1, Ordering::Relaxed);

                        // Emit note lifecycle event through Reactor
                        emit_note_event(&reactor, &file, changed_blocks).await;
                    }
                    Ok(crucible_core::processing::ProcessingResult::Skipped)
                    | Ok(crucible_core::processing::ProcessingResult::NoChanges) => {
                        skipped.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        eprintln!("Error processing {}: {:?}", file.display(), e);
                        warn!("Failed to process {}: {}", file.display(), e);
                    }
                }
            }

            pb.inc(1);
            pb.set_message(format!("Processing: {}", file_name));
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        if let Err(e) = handle.await {
            error_count.fetch_add(1, Ordering::Relaxed);
            eprintln!("Task panic: {:?}", e);
        }
    }

    pb.finish_with_message("Processing complete!");

    // Extract final counts
    let processed_count = processed_count.load(Ordering::Relaxed);
    let skipped_count = skipped_count.load(Ordering::Relaxed);
    let error_count = error_count.load(Ordering::Relaxed);

    // Print summary
    if dry_run {
        println!("\n✅ Dry-run complete!");
        println!("   Would have processed: {} files", processed_count);
    } else {
        println!("\n✅ Pipeline processing complete!");
        println!("   Processed: {} files", processed_count);
        println!("   Skipped (unchanged): {} files", skipped_count);
        if error_count > 0 {
            println!("   ⚠️  Errors: {} files", error_count);
        }
    }

    // Watch mode - uses full event system for event-driven processing
    if watch {
        println!("\n👀 Watching for changes (Press Ctrl+C to stop)...");
        info!("Starting watch mode with event system");

        // Initialize the full event system
        output::info("Initializing event system...");
        let event_handle = initialize_event_system(&config).await?;
        info!(
            "Event system ready with {} handlers",
            event_handle.handler_count().await
        );
        output::success("Event system initialized");

        // Add watch for the target path
        {
            let mut watch = event_handle.watch_manager().write().await;

            // Configure watch with markdown file filter and debouncing
            let crucible_dir = target_path.join(".crucible");
            let filter = EventFilter::new()
                .with_extension("md")
                .exclude_dir(crucible_dir);
            let watch_config = WatchConfig {
                id: "process-watch".to_string(),
                recursive: true,
                filter: Some(filter),
                debounce: DebounceConfig::new(500), // 500ms debounce
                handler_config: HandlerConfig::default(),
                mode: WatchMode::Standard,
                backend_options: Default::default(),
            };

            watch
                .add_watch(target_path.to_path_buf(), watch_config)
                .await?;
        }
        info!("Watch started on: {}", target_path.display());

        println!("📡 Event-driven processing active. File changes will trigger the event cascade:");
        println!(
            "   FileChanged -> NoteParsed -> EntityStored -> BlocksUpdated -> EmbeddingGenerated"
        );

        // Wait for Ctrl+C
        tokio::signal::ctrl_c().await?;
        println!("\n👋 Stopping watch mode...");
        info!("Watch mode stopped by user");

        // Graceful shutdown
        event_handle.shutdown().await?;
        println!("✅ Watch mode stopped");
    }

    // Restart daemon if we stopped it
    if stopped_daemon && !watch {
        let storage_config = config.storage.clone().unwrap_or_default();
        output::info("Restarting daemon...");
        if let Err(e) = lifecycle::ensure_daemon(&socket, storage_config.idle_timeout_secs).await {
            warn!("Failed to restart daemon: {}", e);
        }
    }

    Ok(())
}

/// Discover markdown files in a directory
///
/// Excludes common system directories: .crucible, .git, .obsidian, node_modules
fn discover_markdown_files(path: &std::path::Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    if path.is_file() {
        if is_markdown_file(path) {
            files.push(path.to_path_buf());
        }
    } else if path.is_dir() {
        for entry in WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_excluded_dir(e.path()))
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && is_markdown_file(path) {
                files.push(path.to_path_buf());
            }
        }
    }

    Ok(files)
}

/// Check if a directory should be excluded from file discovery
fn is_excluded_dir(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            name == ".crucible"
                || name == ".git"
                || name == ".obsidian"
                || name == "node_modules"
                || name == ".trash"
        })
        .unwrap_or(false)
}

/// Check if a path is a markdown file
pub fn is_markdown_file(path: &std::path::Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("md")
}

/// Emit note lifecycle event through the Reactor.
///
/// Emits a `NoteModified` event for successfully processed notes, allowing
/// Rune handlers registered with `note:*` patterns to react.
async fn emit_note_event(reactor: &Arc<RwLock<Reactor>>, path: &Path, block_count: usize) {
    // Emit NoteModified for processed notes (we use NoteModified since the pipeline
    // handles both creates and updates - distinguishing would require tracking prior state)
    let event = SessionEvent::NoteModified {
        path: path.to_path_buf(),
        change_type: NoteChangeType::Content,
    };

    let mut reactor_guard = reactor.write().await;
    match reactor_guard.emit(event).await {
        Ok(result) => {
            if result.is_completed() {
                debug!(
                    "Note event dispatched for {}: {} handlers ran",
                    path.display(),
                    result.handlers_run().len()
                );
            }
        }
        Err(e) => {
            warn!("Failed to emit note event for {}: {}", path.display(), e);
        }
    }

    // Also emit NoteParsed with block count info for handlers that want parse details
    let parsed_event = SessionEvent::NoteParsed {
        path: path.to_path_buf(),
        block_count,
        payload: None,
    };

    if let Err(e) = reactor_guard.emit(parsed_event).await {
        warn!(
            "Failed to emit NoteParsed event for {}: {}",
            path.display(),
            e
        );
    }
}

/// Load Rune handlers from the kiln's `.crucible/handlers/` directory.
///
/// Handlers matching `note:*` patterns will receive note lifecycle events.
async fn load_rune_handlers(reactor: &mut Reactor, kiln_path: &Path) {
    let handlers_dir = kiln_path.join(".crucible").join("handlers");

    if !handlers_dir.exists() {
        debug!(
            "No handlers directory at {}, skipping Rune handlers",
            handlers_dir.display()
        );
        return;
    }

    // Scan for .rn files
    let entries = match std::fs::read_dir(&handlers_dir) {
        Ok(entries) => entries,
        Err(e) => {
            warn!("Failed to read handlers directory: {}", e);
            return;
        }
    };

    let mut loaded_count = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "rn") {
            match load_single_rune_handler(&path).await {
                Ok(handler) => {
                    if let Err(e) = reactor.register(handler) {
                        warn!("Failed to register Rune handler {}: {}", path.display(), e);
                    } else {
                        loaded_count += 1;
                        debug!("Loaded Rune handler from {}", path.display());
                    }
                }
                Err(e) => {
                    warn!("Failed to load Rune handler from {}: {}", path.display(), e);
                }
            }
        }
    }

    if loaded_count > 0 {
        info!(
            "Loaded {} Rune handlers from {}",
            loaded_count,
            handlers_dir.display()
        );
    }
}

/// Load a single Rune handler from a file.
async fn load_single_rune_handler(path: &Path) -> Result<Box<dyn crucible_core::events::Handler>> {
    use anyhow::Context;
    use crucible_rune::core_handler::{RuneHandler, RuneHandlerMeta};
    use crucible_rune::RuneExecutor;

    // Create executor for this handler
    let executor = Arc::new(RuneExecutor::new().with_context(|| "Failed to create Rune executor")?);

    // Create handler metadata
    // Priority 500+ for user scripts (after built-in handlers)
    let meta = RuneHandlerMeta::new(path.to_path_buf(), "handle")
        .with_priority(500)
        .with_event_pattern("*");

    // Create RuneHandler - this will compile the script
    let handler = RuneHandler::new(meta, executor)
        .with_context(|| format!("Failed to create Rune handler from {}", path.display()))?;

    Ok(Box::new(handler))
}
