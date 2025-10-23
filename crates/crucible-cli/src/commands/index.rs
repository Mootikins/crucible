//! Simplified vault indexing commands for CLI
//!
//! This module provides simplified CLI commands for vault indexing.
//! Complex database and embedding services have been removed in Phase 1.1 dead code elimination.
//! Now provides basic file discovery and tool-based indexing functionality.

use anyhow::Result;
use crate::common::CrucibleToolManager;
use serde_json::json;
use crate::config::CliConfig;
use colored::Colorize;
use glob::glob;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use chrono;

pub async fn execute(
    config: CliConfig,
    path: Option<String>,
    force: bool,
    glob_pattern: String,
) -> Result<()> {
    let vault_path = if let Some(p) = path {
        PathBuf::from(p)
    } else {
        config.vault.path.clone()
    };

    println!("🔍 Indexing vault: {}", vault_path.display());
    println!("📋 Pattern: {}\n", glob_pattern);

    // Ensure crucible-tools are initialized through centralized manager
    CrucibleToolManager::ensure_initialized_global().await?;

    // Find all files matching pattern
    let pattern_str = format!("{}/{}", vault_path.display(), glob_pattern);
    let files: Vec<PathBuf> = glob(&pattern_str)?
        .filter_map(Result::ok)
        .collect();

    if files.is_empty() {
        println!("❌ No files found matching pattern");
        return Ok(());
    }

    println!("📁 Found {} files\n", files.len());

    // Progress bar
    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );

    let mut indexed = 0;
    let mut skipped = 0;
    let mut errors = 0;

    for file_path in files {
        let file_path_str = file_path.to_string_lossy().to_string();
        let file_name = file_path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        pb.set_message(file_name.clone());

        // Read file content
        match std::fs::read_to_string(&file_path) {
            Ok(content) => {
                // Use simplified indexing with tools
                match index_file_with_tools(&file_path_str, &content, force).await {
                    Ok(success) => {
                        if success {
                            indexed += 1;
                        } else {
                            skipped += 1;
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ Error indexing {}: {}", file_name, e);
                        errors += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ Error reading {}: {}", file_name, e);
                errors += 1;
            }
        }

        pb.inc(1);
    }

    pb.finish_with_message("Indexing complete");

    // Display results
    println!("\n📊 Indexing Results:");
    println!("  ✅ Indexed: {}", indexed.to_string().green());
    println!("  ⏭️  Skipped: {}", skipped.to_string().yellow());
    println!("  ❌ Errors:  {}", errors.to_string().red());

    // Show vault statistics
    if let Ok(stats) = get_vault_statistics().await {
        println!("\n📈 Vault Statistics:");
        if let Some(total_notes) = stats.get("total_notes").and_then(|v| v.as_u64()) {
            println!("  📝 Total notes: {}", total_notes);
        }
        if let Some(total_size) = stats.get("total_size_mb").and_then(|v| v.as_f64()) {
            println!("  💾 Total size: {:.1} MB", total_size);
        }
        if let Some(last_indexed) = stats.get("last_indexed").and_then(|v| v.as_str()) {
            println!("  🕐 Last indexed: {}", last_indexed);
        }
    }

    println!("\n💡 Phase 1.1 Simplification Notice:");
    println!("   Complex database indexing has been simplified.");
    println!("   Advanced embedding features are now disabled.");
    println!("   File discovery and basic indexing preserved.");

    Ok(())
}

/// Index a file using simplified tools
async fn index_file_with_tools(file_path: &str, content: &str, force: bool) -> Result<bool> {
    // Check if already indexed (unless forcing)
    if !force {
        if let Ok(existing) = check_if_indexed(file_path).await {
            if existing {
                return Ok(false); // Skip
            }
        }
    }

    // Use index_document tool through centralized manager
    let result = CrucibleToolManager::execute_tool_global(
        "index_document",
        json!({
            "document": {
                "id": file_path,
                "content": content,
                "title": extract_title_from_path(file_path),
                "folder": extract_folder_from_path(file_path),
                "indexed_at": chrono::Utc::now().to_rfc3339()
            }
        }),
        Some("cli_indexer".to_string()),
        Some("index_session".to_string()),
    ).await?;

    Ok(result.success)
}

/// Check if a file is already indexed
async fn check_if_indexed(file_path: &str) -> Result<bool> {
    let result = CrucibleToolManager::execute_tool_global(
        "search_by_filename",
        json!({
            "pattern": file_path
        }),
        Some("cli_indexer".to_string()),
        Some("index_session".to_string()),
    ).await?;

    if let Some(data) = result.data {
        if let Some(files) = data.get("files").and_then(|f| f.as_array()) {
            return Ok(!files.is_empty());
        }
    }

    Ok(false)
}

/// Get vault statistics using tools
async fn get_vault_statistics() -> Result<serde_json::Value> {
    let result = CrucibleToolManager::execute_tool_global(
        "get_vault_stats",
        json!({}),
        Some("cli_indexer".to_string()),
        Some("index_session".to_string()),
    ).await?;

    Ok(result.data.unwrap_or(json!({})))
}

/// Extract title from file path
fn extract_title_from_path(file_path: &str) -> String {
    std::path::Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

/// Extract folder from file path
fn extract_folder_from_path(file_path: &str) -> String {
    std::path::Path::new(file_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_string()
}