//! Process Command - Explicit Pipeline Processing
//!
//! Runs the note processing pipeline on files in the kiln.

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use tracing::{info, warn};
use walkdir::WalkDir;

use crate::config::CliConfig;
use crate::{factories, output};

/// Execute the process command
///
/// # Arguments
/// * `config` - CLI configuration
/// * `path` - Optional specific file/directory to process (if None, processes entire kiln)
/// * `force` - If true, reprocess all files regardless of changes
/// * `watch` - If true, continue watching for changes after initial processing
pub async fn execute(
    config: CliConfig,
    path: Option<PathBuf>,
    force: bool,
    watch: bool,
) -> Result<()> {
    info!("Starting process command");

    // Determine target path
    let target_path = path.as_ref()
        .map(|p| p.as_path())
        .unwrap_or(config.kiln.path.as_path());

    info!("Processing path: {}", target_path.display());
    info!("Force reprocess: {}", force);
    info!("Watch mode: {}", watch);

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
    println!("\n🔄 Processing {} files through pipeline...", files.len());

    let mut processed_count = 0;
    let mut skipped_count = 0;
    let mut error_count = 0;

    for file in files {
        let file_name = file.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        pb.set_message(format!("Processing: {}", file_name));

        // Run file through the 5-phase pipeline
        match pipeline.process(&file).await {
            Ok(crucible_core::processing::ProcessingResult::Success { .. }) => {
                processed_count += 1;
            }
            Ok(crucible_core::processing::ProcessingResult::Skipped) |
            Ok(crucible_core::processing::ProcessingResult::NoChanges) => {
                skipped_count += 1;
            }
            Err(e) => {
                error_count += 1;
                warn!("Failed to process {}: {}", file.display(), e);
            }
        }

        pb.inc(1);
    }

    pb.finish_with_message("Processing complete!");

    // Print summary
    println!("\n✅ Pipeline processing complete!");
    println!("   Processed: {} files", processed_count);
    println!("   Skipped (unchanged): {} files", skipped_count);
    if error_count > 0 {
        println!("   ⚠️  Errors: {} files", error_count);
    }

    // Watch mode
    if watch {
        println!("\n👀 Watching for changes (Press Ctrl+C to stop)...");
        warn!("Watch mode not yet implemented");
        // TODO: Implement file watching with notify crate
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
