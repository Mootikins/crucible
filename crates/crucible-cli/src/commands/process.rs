//! Process Command - Explicit Pipeline Processing
//!
//! Runs the note processing pipeline on files in the kiln via daemon RPC.
//! The daemon handles all file discovery and pipeline logic (parsing,
//! enrichment, embedding) through `kiln.open` with `process: true`.
//!
//! When `--watch` is enabled, this command watches for file changes and
//! sends changed files to the daemon for processing.

use anyhow::Result;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

use crate::config::CliConfig;
use crate::{factories, output};
use crucible_core::EXCLUDED_DIRS;

#[derive(Serialize)]
struct ProcessError {
    path: String,
    error: String,
}

#[derive(Serialize)]
struct ProcessSummary {
    target: String,
    mode: &'static str,
    dry_run: bool,
    discovered: u64,
    processed: u64,
    skipped: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<ProcessError>,
}

/// Execute the process command
///
/// # Arguments
/// * `config` - CLI configuration
/// * `path` - Optional specific file/directory to process (if None, processes entire kiln)
/// * `force` - If true, reprocess all files regardless of changes
/// * `watch` - If true, continue watching for changes after initial processing
/// * `verbose` - If true, show detailed progress and timing information
/// * `dry_run` - If true, preview changes without writing to database
/// * `parallel` - Optional number of parallel workers (unused -- daemon handles parallelism)
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    config: CliConfig,
    path: Option<PathBuf>,
    force: bool,
    watch: bool,
    verbose: bool,
    dry_run: bool,
    _parallel: Option<usize>,
    json: bool,
) -> Result<()> {
    info!("Starting process command");

    let target_path = path.as_deref().unwrap_or(config.kiln_path.as_path());
    info!("Processing path: {}", target_path.display());
    info!("Watch mode: {}", watch);
    info!("Dry-run mode: {}", dry_run);

    // Initialize storage -- always daemon-backed
    if !json {
        output::info("Initializing storage...");
    }
    let storage_handle = factories::get_storage(&config).await?;
    if !json {
        output::success("Storage initialized (daemon mode)");
    }

    let client = storage_handle.as_daemon_client().daemon_client();

    // Handle specific single file
    if let Some(ref target) = path {
        if target.is_file() {
            info!("Processing single file: {}", target.display());
            if dry_run {
                if json {
                    let summary = ProcessSummary {
                        target: target.display().to_string(),
                        mode: "single-file",
                        dry_run: true,
                        discovered: 1,
                        processed: 0,
                        skipped: 0,
                        errors: Vec::new(),
                    };
                    println!("{}", serde_json::to_string_pretty(&summary)?);
                } else {
                    println!("Dry-run: would process {}", target.display());
                }
                return Ok(());
            }

            // Use process_batch with a single file
            let paths = vec![target.clone()];
            match client.process_batch(&config.kiln_path, &paths).await {
                Ok((processed, skipped, errors)) => {
                    if json {
                        let summary = ProcessSummary {
                            target: target.display().to_string(),
                            mode: "single-file",
                            dry_run: false,
                            discovered: 1,
                            processed: processed as u64,
                            skipped: skipped as u64,
                            errors: errors
                                .iter()
                                .map(|(p, e)| ProcessError {
                                    path: p.clone(),
                                    error: e.clone(),
                                })
                                .collect(),
                        };
                        println!("{}", serde_json::to_string_pretty(&summary)?);
                    } else {
                        println!("Pipeline processing complete!");
                        println!("  Processed: {} files", processed);
                        println!("  Skipped (unchanged): {} files", skipped);
                        if !errors.is_empty() {
                            println!("  Errors: {} files", errors.len());
                            for (path, err) in &errors {
                                eprintln!("  Error: {} - {}", path, err);
                            }
                        }
                    }
                }
                Err(e) => {
                    if !json {
                        eprintln!("Error processing file: {}", e);
                    }
                    return Err(e);
                }
            }

            // Enter watch mode if requested (watching only this file's parent)
            if watch {
                run_watch_mode(&config, target_path, verbose).await?;
            }

            return Ok(());
        }
    }

    // Full kiln (or subdirectory) processing -- daemon handles file discovery
    if dry_run {
        if json {
            let summary = ProcessSummary {
                target: target_path.display().to_string(),
                mode: "kiln",
                dry_run: true,
                discovered: 0,
                processed: 0,
                skipped: 0,
                errors: Vec::new(),
            };
            println!("{}", serde_json::to_string_pretty(&summary)?);
        } else {
            println!("Dry-run: daemon would discover and process all markdown files in kiln");
        }
        return Ok(());
    }

    if !json {
        println!("Processing kiln via daemon...");
    }
    let result = client
        .kiln_open_with_options(&config.kiln_path, true, force)
        .await?;

    // Parse response
    let discovered = result
        .get("discovered")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let processed = result
        .get("processed")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let skipped = result.get("skipped").and_then(|v| v.as_u64()).unwrap_or(0);
    let err_entries: Vec<ProcessError> = result
        .get("errors")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|err| ProcessError {
                    path: err
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("<unknown>")
                        .to_string(),
                    error: err
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown error")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    if json {
        let summary = ProcessSummary {
            target: target_path.display().to_string(),
            mode: "kiln",
            dry_run: false,
            discovered,
            processed,
            skipped,
            errors: err_entries,
        };
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!("Pipeline processing complete!");
        println!("  Discovered: {} markdown files", discovered);
        println!("  Processed: {} files", processed);
        println!("  Skipped (unchanged): {} files", skipped);
        if !err_entries.is_empty() {
            println!("  Errors: {} files", err_entries.len());
            for err in &err_entries {
                eprintln!("  Error: {} - {}", err.path, err.error);
            }
        }
    }

    if verbose {
        // Log additional detail from the response when available
        if let Some(status) = result.get("status").and_then(|v| v.as_str()) {
            info!("Daemon response status: {}", status);
        }
    }

    // Watch mode
    if watch {
        run_watch_mode(&config, target_path, verbose).await?;
    }

    Ok(())
}

/// Run the watch mode polling loop.
///
/// Polls for file changes every 2 seconds and sends changed files to the
/// daemon for processing. This is a temporary approach until watch mode
/// is fully moved into the daemon.
async fn run_watch_mode(
    config: &CliConfig,
    target: &std::path::Path,
    _verbose: bool,
) -> Result<()> {
    println!("\nWatching for changes (Press Ctrl+C to stop)...");
    info!("Starting watch mode");

    let daemon_client = Arc::new(crate::common::daemon_client().await?);
    let kiln_path = config.kiln_path.clone();
    let target = target.to_path_buf();

    // Simple polling watch: check for changed files every 2 seconds
    // and send them to the daemon for processing
    let mut last_seen: std::collections::HashMap<PathBuf, std::time::SystemTime> =
        std::collections::HashMap::new();

    // Seed with current state
    let initial_files = discover_markdown_files_for_watch(&target);
    for f in &initial_files {
        if let Ok(meta) = std::fs::metadata(f) {
            if let Ok(modified) = meta.modified() {
                last_seen.insert(f.clone(), modified);
            }
        }
    }

    println!("Watching for markdown file changes. Press Ctrl+C to stop.");

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\nStopping watch mode...");
                break;
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(2)) => {
                // Check for changed files
                let current_files = discover_markdown_files_for_watch(&target);
                let mut changed = Vec::new();

                for f in &current_files {
                    if let Ok(meta) = std::fs::metadata(f) {
                        if let Ok(modified) = meta.modified() {
                            let is_new = !last_seen.contains_key(f);
                            let is_changed = last_seen.get(f).map(|&t| t != modified).unwrap_or(false);
                            if is_new || is_changed {
                                changed.push(f.clone());
                                last_seen.insert(f.clone(), modified);
                            }
                        }
                    }
                }

                if !changed.is_empty() {
                    info!("Watch: {} files changed, sending to daemon", changed.len());
                    println!("Processing {} changed file(s)...", changed.len());
                    match daemon_client.process_batch(&kiln_path, &changed).await {
                        Ok((proc, skip, errs)) => {
                            println!("  Processed: {}, Skipped: {}", proc, skip);
                            for (path, err) in &errs {
                                eprintln!("  Error processing {}: {}", path, err);
                            }
                        }
                        Err(e) => {
                            warn!("Watch batch processing error: {}", e);
                            eprintln!("  Batch error: {}", e);
                        }
                    }
                }
            }
        }
    }

    println!("Watch mode stopped");
    Ok(())
}

/// Discover markdown files for watch mode polling seed.
///
/// This is a temporary local helper until watch mode moves to the daemon.
/// Walks the directory tree, excluding common system directories, and
/// returns all `.md` files found.
fn discover_markdown_files_for_watch(path: &std::path::Path) -> Vec<PathBuf> {
    use walkdir::WalkDir;

    let is_excluded = |entry_path: &std::path::Path| -> bool {
        entry_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| EXCLUDED_DIRS.contains(&name))
            .unwrap_or(false)
    };

    if path.is_file() {
        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            return vec![path.to_path_buf()];
        }
        return vec![];
    }

    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_excluded(e.path()))
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().is_file() && e.path().extension().and_then(|s| s.to_str()) == Some("md")
        })
        .map(|e| e.path().to_path_buf())
        .collect()
}
