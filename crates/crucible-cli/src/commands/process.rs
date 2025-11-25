//! Process Command - Explicit Pipeline Processing
//!
//! Runs the note processing pipeline on files in the kiln.

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};
use walkdir::WalkDir;

use crate::config::CliConfig;
use crate::{factories, output};
use crucible_watch::{EventFilter, FileEvent, FileEventKind, WatchMode};
use crucible_watch::traits::{DebounceConfig, HandlerConfig, WatchConfig};

/// Execute the process command
///
/// # Arguments
/// * `config` - CLI configuration
/// * `path` - Optional specific file/directory to process (if None, processes entire kiln)
/// * `force` - If true, reprocess all files regardless of changes
/// * `watch` - If true, continue watching for changes after initial processing
/// * `verbose` - If true, show detailed progress and timing information
/// * `dry_run` - If true, preview changes without writing to database
pub async fn execute(
    config: CliConfig,
    path: Option<PathBuf>,
    force: bool,
    watch: bool,
    verbose: bool,
    dry_run: bool,
) -> Result<()> {
    info!("Starting process command");

    // Determine target path
    let target_path = path.as_ref()
        .map(|p| p.as_path())
        .unwrap_or(config.kiln.path.as_path());

    info!("Processing path: {}", target_path.display());
    info!("Force reprocess: {}", force);
    info!("Watch mode: {}", watch);
    info!("Dry-run mode: {}", dry_run);

    // Initialize storage using factory pattern
    output::info("Initializing storage...");
    let storage_client = factories::create_surrealdb_storage(&config).await?;
    factories::initialize_surrealdb_schema(&storage_client).await?;
    output::success("Storage initialized");

    // Create pipeline
    output::info("Creating processing pipeline...");
    let pipeline = factories::create_pipeline(
        storage_client.clone(),
        &config,
        force,
    ).await?;
    output::success("Pipeline ready");

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
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")?
            .progress_chars("#>-"),
    );

    // Process files through pipeline
    if dry_run {
        println!("\n🔍 DRY RUN MODE - No changes will be made");
    }
    println!("\n🔄 Processing {} files through pipeline...", files.len());

    let mut processed_count = 0;
    let mut skipped_count = 0;
    let mut error_count = 0;

    for file in files {
        let file_name = file.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        if verbose {
            println!("📄 Processing: {}", file_name);
        }
        pb.set_message(format!("Processing: {}", file_name));

        // Run file through the 5-phase pipeline
        if dry_run {
            if verbose {
                println!("   ⏭ Would process (dry-run)");
            } else {
                println!("  Would process: {}", file_name);
            }
            processed_count += 1;
        } else {
            match pipeline.process(&file).await {
                Ok(crucible_core::processing::ProcessingResult::Success { .. }) => {
                    processed_count += 1;
                    if verbose {
                        println!("   ✓ Success");
                    }
                }
                Ok(crucible_core::processing::ProcessingResult::Skipped) |
                Ok(crucible_core::processing::ProcessingResult::NoChanges) => {
                    skipped_count += 1;
                    if verbose {
                        println!("   ⏭ Skipped (unchanged)");
                    }
                }
                Err(e) => {
                    error_count += 1;
                    let error_msg = format!("{}", e);
                    if verbose {
                        println!("   ⚠ Error: {}", error_msg);
                    }
                    eprintln!("Error processing {}: {:?}", file.display(), e);
                    warn!("Failed to process {}: {}", file.display(), e);
                }
            }
        }

        pb.inc(1);
    }

    pb.finish_with_message("Processing complete!");

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

    // Watch mode
    if watch {
        println!("\n👀 Watching for changes (Press Ctrl+C to stop)...");
        info!("Starting watch mode");

        // Create watcher via factory (DIP pattern - depends only on FileWatcher trait)
        let mut watcher_arc = factories::create_file_watcher(&config)?;

        // Get mutable access to configure the watcher
        let watcher = Arc::get_mut(&mut watcher_arc)
            .ok_or_else(|| anyhow::anyhow!("Failed to get mutable watcher reference"))?;

        // Create event channel
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<FileEvent>();
        watcher.set_event_sender(tx);

        // Configure watch with markdown file filter and debouncing
        let filter = EventFilter::new().with_extension("md");
        let watch_config = WatchConfig {
            id: "process-watch".to_string(),
            recursive: true,
            filter: Some(filter),
            debounce: DebounceConfig::new(500), // 500ms debounce
            handler_config: HandlerConfig::default(),
            mode: WatchMode::Standard,
            backend_options: Default::default(),
        };

        // Start watching the target path
        // IMPORTANT: Keep handle alive for duration of watch to prevent premature cleanup
        let watch_handle = watcher.watch(target_path.to_path_buf(), watch_config).await?;
        info!("Watch started on: {}", target_path.display());

        // Event processing loop with Ctrl+C handling
        loop {
            tokio::select! {
                Some(event) = rx.recv() => {
                    // Process file change events
                    match &event.kind {
                        FileEventKind::Modified | FileEventKind::Created => {
                            if verbose {
                                println!("📝 Change detected: {}", event.path.display());
                            }

                            // Reprocess changed file through pipeline
                            if dry_run {
                                println!("   Would reprocess: {}", event.path.display());
                            } else {
                                match pipeline.process(&event.path).await {
                                    Ok(crucible_core::processing::ProcessingResult::Success { .. }) => {
                                        if verbose {
                                            println!("   ✓ Reprocessed successfully");
                                        }
                                    }
                                    Ok(crucible_core::processing::ProcessingResult::Skipped) |
                                    Ok(crucible_core::processing::ProcessingResult::NoChanges) => {
                                        if verbose {
                                            println!("   ⏭ Skipped (unchanged)");
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("   ⚠ Error reprocessing: {}", e);
                                        warn!("Failed to reprocess {}: {}", event.path.display(), e);
                                    }
                                }
                            }
                        }
                        FileEventKind::Deleted => {
                            if verbose {
                                println!("🗑 Deleted: {}", event.path.display());
                                // Could optionally mark as deleted in DB in the future
                            }
                        }
                        _ => {
                            // Ignore other event types (Moved, Batch, Unknown)
                            if verbose {
                                println!("   ℹ Ignoring event type: {:?}", event.kind);
                            }
                        }
                    }
                }

                _ = tokio::signal::ctrl_c() => {
                    println!("\n👋 Stopping watch mode...");
                    info!("Watch mode stopped by user");
                    break;
                }
            }
        }

        // Cleanup - explicitly drop handle to ensure clean shutdown
        drop(watch_handle);
        println!("✅ Watch mode stopped");
    }

    Ok(())
}

/// Discover markdown files in a directory
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

/// Check if a path is a markdown file
pub fn is_markdown_file(path: &std::path::Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("md")
}
