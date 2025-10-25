//! Simplified fuzzy search commands for CLI
//!
//! This module provides basic fuzzy search functionality using file system operations.

use anyhow::Result;
use crate::config::CliConfig;
use crate::interactive::SearchResultWithScore;
use crate::commands::search::{get_markdown_files};

pub async fn execute(
    config: CliConfig,
    query: String,
    _search_content: bool,
    _search_tags: bool,
    _search_paths: bool,
    limit: u32,
) -> Result<()> {
    let kiln_path = &config.kiln.path;

    // Check if kiln path exists
    if !kiln_path.exists() {
        eprintln!("Error: kiln path does not exist: {}", kiln_path.display());
        eprintln!("Please set OBSIDIAN_KILN_PATH to a valid kiln directory.");
        return Err(anyhow::anyhow!("kiln path does not exist"));
    }

    println!("🔍 Fuzzy search: {}", query);

    // Use the same search functionality as regular search
    let results = if !query.is_empty() {
        // Direct search with query using file system
        crate::commands::search::search_files_in_kiln(kiln_path, &query, limit, true)?
    } else {
        // Get all files if no query
        let files = get_markdown_files(kiln_path)?;
        let mut results = Vec::new();

        for file_path in files.into_iter().take(limit as usize) {
            let title = file_path.split('/').next_back().unwrap_or(&file_path).to_string();
            results.push(SearchResultWithScore {
                id: file_path,
                title,
                content: String::new(),
                score: 1.0,
            });
        }
        results
    };

    // Display results
    if results.is_empty() {
        println!("❌ No results found for query: {}", query);
        return Ok(());
    }

    println!("\n🎯 Found {} results:", results.len());
    println!("{}", "-".repeat(60));

    for (idx, result) in results.iter().enumerate() {
        println!("\n{}. {}", idx + 1, result.title);
        println!("   📁 {}", result.id);

        // Show preview of content (first 100 characters)
        if !result.content.is_empty() {
            let preview = if result.content.len() > 100 {
                format!("{}...", &result.content[..100])
            } else {
                result.content.clone()
            };
            println!("   📄 {}", preview);
        }
    }

    Ok(())
}