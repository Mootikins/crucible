//! Process Command - Explicit Pipeline Processing
//!
//! Runs the note processing pipeline on files in the kiln via daemon RPC.
//! The daemon handles all pipeline logic (parsing, enrichment, embedding).
//!
//! When `--watch` is enabled, this command watches for file changes and
//! sends changed files to the daemon for processing.

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};
use walkdir::WalkDir;

use crate::config::CliConfig;
use crate::{factories, output};
use crucible_rpc::DaemonClient;

/// Execute the process command
///
/// # Arguments
/// * `config` - CLI configuration
/// * `path` - Optional specific file/directory to process (if None, processes entire kiln)
/// * `force` - If true, reprocess all files regardless of changes
/// * `watch` - If true, continue watching for changes after initial processing
/// * `verbose` - If true, show detailed progress and timing information
/// * `dry_run` - If true, preview changes without writing to database
/// * `parallel` - Optional number of parallel workers (unused — daemon handles parallelism)
pub async fn execute(
    config: CliConfig,
    path: Option<PathBuf>,
    force: bool,
    watch: bool,
    _verbose: bool,
    dry_run: bool,
    _parallel: Option<usize>,
) -> Result<()> {
    info!("Starting process command");

    // Determine target path
    let target_path = path.as_deref().unwrap_or(config.kiln_path.as_path());

    info!("Processing path: {}", target_path.display());
    info!("Force reprocess: {}", force);
    info!("Watch mode: {}", watch);
    info!("Dry-run mode: {}", dry_run);

    // Initialize storage — always daemon-backed
    output::info("Initializing storage...");
    let storage_handle = factories::get_storage(&config).await?;
    output::success("Storage initialized (daemon mode)");

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

    // Process files through daemon pipeline
    if dry_run {
        println!("\n🔍 DRY RUN MODE - No changes will be made");
    }

    let processed_count: usize;
    let skipped_count: usize;
    let error_count: usize;

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
        let daemon_storage = storage_handle.as_daemon_client();

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

    // Watch mode — poll for file changes and send to daemon
    if watch {
        println!("\n👀 Watching for changes (Press Ctrl+C to stop)...");
        info!("Starting watch mode");

        let daemon_client = Arc::new(DaemonClient::connect_or_start().await?);
        let kiln_path = config.kiln_path.clone();
        let target = target_path.to_path_buf();

        // Simple polling watch: check for changed files every 2 seconds
        // and send them to the daemon for processing
        let mut last_seen: std::collections::HashMap<PathBuf, std::time::SystemTime> =
            std::collections::HashMap::new();

        // Seed with current state
        if let Ok(initial_files) = discover_markdown_files(&target) {
            for f in &initial_files {
                if let Ok(meta) = std::fs::metadata(f) {
                    if let Ok(modified) = meta.modified() {
                        last_seen.insert(f.clone(), modified);
                    }
                }
            }
        }

        println!("📡 Watching for markdown file changes. Press Ctrl+C to stop.");

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    println!("\n👋 Stopping watch mode...");
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(2)) => {
                    // Check for changed files
                    let current_files = discover_markdown_files(&target).unwrap_or_default();
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
                        println!("🔄 Processing {} changed file(s)...", changed.len());
                        match daemon_client.process_batch(&kiln_path, &changed).await {
                            Ok((proc, skip, errs)) => {
                                println!("   ✅ Processed: {}, Skipped: {}", proc, skip);
                                for (path, err) in &errs {
                                    eprintln!("   ⚠️  Error processing {}: {}", path, err);
                                }
                            }
                            Err(e) => {
                                warn!("Watch batch processing error: {}", e);
                                eprintln!("   ⚠️  Batch error: {}", e);
                            }
                        }
                    }
                }
            }
        }

        println!("✅ Watch mode stopped");
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
