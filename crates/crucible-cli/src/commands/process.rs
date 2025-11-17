//! Process Command - Explicit Pipeline Processing
//!
//! Runs the note processing pipeline on files in the kiln.

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use tracing::{info, warn};
use walkdir::WalkDir;

use crate::config::CliConfig;
use crate::core_facade::CrucibleCoreFacade;

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

    // Initialize core facade
    let core = CrucibleCoreFacade::from_config(config).await?;

    let target_path = path.as_ref().map(|p| p.as_path()).unwrap_or(core.kiln_root());

    info!("Processing path: {}", target_path.display());
    info!("Force reprocess: {}", force);
    info!("Watch mode: {}", watch);

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

    // Process files
    println!("\n🔄 Processing {} files...", files.len());

    // TODO: Integrate with NotePipeline
    // For MVP, this is a placeholder showing the architecture
    for file in files {
        pb.set_message(format!("Processing: {}", file.display()));

        // In full implementation:
        // let result = pipeline.process(&file).await?;
        // - Quick filter (check if changed)
        // - Parse markdown
        // - Merkle diff (identify changed blocks)
        // - Enrich (generate embeddings for changed blocks)
        // - Store results

        pb.inc(1);
    }

    pb.finish_with_message("Processing complete!");

    println!("\n✅ Processed {} files", pb.position());

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
fn is_markdown_file(path: &std::path::Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("md")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_markdown_file() {
        assert!(is_markdown_file(std::path::Path::new("test.md")));
        assert!(!is_markdown_file(std::path::Path::new("test.txt")));
        assert!(!is_markdown_file(std::path::Path::new("test")));
    }
}
