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
use crucible_rpc::DaemonClient;
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

    // Determine target path
    let target_path = path.as_deref().unwrap_or(config.kiln_path.as_path());

    info!("Processing path: {}", target_path.display());
    info!("Force reprocess: {}", force);
    info!("Watch mode: {}", watch);
    info!("Dry-run mode: {}", dry_run);

    // Initialize storage using factory pattern (backend-agnostic)
    output::info("Initializing storage...");
    let storage_handle = factories::get_storage(&config).await?;

    // Check if using daemon mode - if so, use daemon's pipeline
    let use_daemon_pipeline = storage_handle.is_daemon();

    // For non-daemon modes, create local pipeline
    let pipeline: Option<Arc<crucible_pipeline::NotePipeline>> = if !use_daemon_pipeline {
        let note_store = storage_handle
            .note_store()
            .ok_or_else(|| anyhow::anyhow!("Failed to get NoteStore from storage handle"))?;
        output::success("Storage initialized");

        output::info("Creating processing pipeline...");
        let p = factories::create_pipeline(note_store, &config, force).await?;
        output::success("Pipeline ready");
        Some(Arc::new(p))
    } else {
        output::success("Storage initialized (daemon mode - using remote pipeline)");
        None
    };

    // Track if we need to restart daemon later
    let stopped_daemon = false;

    // Initialize Reactor for note lifecycle events
    // This allows Lua handlers to react to note processing
    let reactor = Arc::new(RwLock::new(Reactor::new()));
    load_lua_handlers(&mut *reactor.write().await, &config.kiln_path);
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

    let processed_count: usize;
    let skipped_count: usize;
    let error_count: usize;

    if use_daemon_pipeline {
        // Daemon mode: use process_batch RPC
        println!(
            "\n🔄 Processing {} files through daemon pipeline...",
            files.len()
        );

        if dry_run {
            processed_count = files.len();
            skipped_count = 0;
            error_count = 0;
            pb.finish_with_message("Dry run complete!");
        } else {
            let daemon_storage = storage_handle
                .as_daemon_client()
                .ok_or_else(|| anyhow::anyhow!("Expected daemon client in daemon mode"))?;

            let kiln_path = config.kiln_path.clone();

            // Process in batches to show progress
            let batch_size = 100;
            let mut total_processed = 0;
            let mut total_skipped = 0;
            let mut total_errors = 0;

            for chunk in files.chunks(batch_size) {
                let chunk_paths: Vec<PathBuf> = chunk.to_vec();
                match daemon_storage
                    .daemon_client()
                    .process_batch(&kiln_path, &chunk_paths)
                    .await
                {
                    Ok((proc, skip, errs)) => {
                        total_processed += proc;
                        total_skipped += skip;
                        total_errors += errs.len();
                        for (path, err) in &errs {
                            eprintln!("Error processing {}: {}", path, err);
                        }
                    }
                    Err(e) => {
                        total_errors += chunk.len();
                        eprintln!("Batch processing error: {}", e);
                    }
                }
                pb.inc(chunk.len() as u64);
            }

            processed_count = total_processed;
            skipped_count = total_skipped;
            error_count = total_errors;
            pb.finish_with_message("Processing complete!");
        }
    } else {
        // Local pipeline mode
        println!(
            "\n🔄 Processing {} files through pipeline (with {} workers)...",
            files.len(),
            workers
        );

        let pipeline = pipeline.expect("Pipeline should exist for non-daemon mode");

        // Use atomic counters for thread-safe updates
        let processed = Arc::new(AtomicUsize::new(0));
        let skipped = Arc::new(AtomicUsize::new(0));
        let errors = Arc::new(AtomicUsize::new(0));

        // Bounded concurrency with semaphore
        let semaphore = Arc::new(Semaphore::new(workers));
        let pb = Arc::new(pb);
        let mut handles = Vec::new();

        for file in files {
            let permit = semaphore.clone().acquire_owned().await?;
            let pipeline = pipeline.clone();
            let reactor = reactor.clone();
            let pb = pb.clone();
            let processed = processed.clone();
            let skipped = skipped.clone();
            let errors = errors.clone();

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
                errors.fetch_add(1, Ordering::Relaxed);
                eprintln!("Task panic: {:?}", e);
            }
        }

        pb.finish_with_message("Processing complete!");

        // Extract final counts
        processed_count = processed.load(Ordering::Relaxed);
        skipped_count = skipped.load(Ordering::Relaxed);
        error_count = errors.load(Ordering::Relaxed);
    }

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
        output::info("Restarting daemon...");
        if let Err(e) = DaemonClient::connect_or_start().await {
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

fn load_lua_handlers(reactor: &mut Reactor, kiln_path: &Path) {
    use crucible_core::discovery::DiscoveryPaths;
    use crucible_lua::LuaScriptHandlerRegistry;

    let paths = DiscoveryPaths::new("handlers", Some(kiln_path));
    let existing = paths.existing_paths();
    if existing.is_empty() {
        debug!("No handler directories found, skipping Lua handlers");
        return;
    }

    let registry = match LuaScriptHandlerRegistry::discover(&existing) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to discover Lua handlers: {}", e);
            return;
        }
    };

    let handlers = match registry.to_core_handlers() {
        Ok(h) => h,
        Err(e) => {
            warn!("Failed to create core handlers from Lua: {}", e);
            return;
        }
    };

    let mut loaded_count = 0;
    for handler in handlers {
        let name = handler.name().to_string();
        if let Err(e) = reactor.register(handler) {
            warn!("Failed to register Lua handler {}: {}", name, e);
        } else {
            loaded_count += 1;
            debug!("Loaded Lua handler: {}", name);
        }
    }

    if loaded_count > 0 {
        info!("Loaded {} Lua handlers", loaded_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_is_markdown_file_with_md_extension() {
        assert!(is_markdown_file(Path::new("test.md")));
        assert!(is_markdown_file(Path::new("README.md")));
        assert!(is_markdown_file(Path::new("/path/to/notes/document.md")));
    }

    #[test]
    fn test_is_markdown_file_with_other_extensions() {
        assert!(!is_markdown_file(Path::new("test.txt")));
        assert!(!is_markdown_file(Path::new("test.rs")));
        assert!(!is_markdown_file(Path::new("test.markdown")));
        assert!(!is_markdown_file(Path::new("test.mdx")));
    }

    #[test]
    fn test_is_markdown_file_without_extension() {
        assert!(!is_markdown_file(Path::new("test")));
        assert!(!is_markdown_file(Path::new("README")));
        assert!(!is_markdown_file(Path::new("/path/to/file")));
    }

    #[test]
    fn test_is_markdown_file_with_hidden_files() {
        assert!(is_markdown_file(Path::new(".hidden.md")));
        assert!(!is_markdown_file(Path::new(".hidden")));
    }

    #[test]
    fn test_is_excluded_dir_standard_exclusions() {
        assert!(is_excluded_dir(Path::new(".crucible")));
        assert!(is_excluded_dir(Path::new(".git")));
        assert!(is_excluded_dir(Path::new(".obsidian")));
        assert!(is_excluded_dir(Path::new("node_modules")));
        assert!(is_excluded_dir(Path::new(".trash")));
    }

    #[test]
    fn test_is_excluded_dir_nested_paths() {
        assert!(is_excluded_dir(Path::new("/home/user/kiln/.git")));
        assert!(is_excluded_dir(Path::new("/project/node_modules")));
        assert!(is_excluded_dir(Path::new("some/path/.crucible")));
    }

    #[test]
    fn test_is_excluded_dir_not_excluded() {
        assert!(!is_excluded_dir(Path::new("notes")));
        assert!(!is_excluded_dir(Path::new("documents")));
        assert!(!is_excluded_dir(Path::new("my_folder")));
        assert!(!is_excluded_dir(Path::new(".config")));
    }

    #[test]
    fn test_is_excluded_dir_partial_matches_not_excluded() {
        assert!(!is_excluded_dir(Path::new("git")));
        assert!(!is_excluded_dir(Path::new("crucible")));
        assert!(!is_excluded_dir(Path::new("obsidian")));
    }
}
