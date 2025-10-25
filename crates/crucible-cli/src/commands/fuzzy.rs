//! Simplified fuzzy search commands for CLI
//!
//! This module provides simplified CLI commands for fuzzy search.
//! Complex database operations have been removed in Phase 1.1 dead code elimination.
//! Now provides basic semantic search with tool-based functionality.

use anyhow::Result;
use crucible_tools::execute_tool;
use serde_json::json;
use crate::config::CliConfig;
use crate::interactive::SearchResultWithScore;

pub async fn execute(
    config: CliConfig,
    query: String,
    search_content: bool,
    search_tags: bool,
    search_paths: bool,
    limit: u32,
) -> Result<()> {
    // Initialize crucible-tools for simplified fuzzy search
    crucible_tools::init();

    let mut all_results = Vec::new();

    println!("🔍 Fuzzy search: {}", query);
    println!("📋 Content: {}, Tags: {}, Paths: {}", search_content, search_tags, search_paths);

    // Use semantic search with the query
    if search_content {
        let result = execute_tool(
            "search_documents".to_string(),
            json!({
                "query": query,
                "top_k": limit
            }),
            Some("cli_user".to_string()),
            Some("fuzzy_search".to_string()),
        ).await?;

        if let Some(data) = result.data {
            if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
                for item in results {
                    if let (Some(file_path), Some(title), Some(score)) = (
                        item.get("file_path").and_then(|p| p.as_str()),
                        item.get("title").and_then(|t| t.as_str()),
                        item.get("score").and_then(|s| s.as_f64())
                    ) {
                        all_results.push(SearchResultWithScore {
                            id: file_path.to_string(),
                            title: title.to_string(),
                            content: item.get("content")
                                .and_then(|c| c.as_str())
                                .unwrap_or("")
                                .to_string(),
                            score,
                        });
                    }
                }
            }
        }
    }

    // Search by tags if requested
    if search_tags {
        let result = execute_tool(
            "search_by_tags".to_string(),
            json!({
                "tags": [query]
            }),
            Some("cli_user".to_string()),
            Some("fuzzy_search".to_string()),
        ).await?;

        if let Some(data) = result.data {
            if let Some(files) = data.get("matching_files").and_then(|f| f.as_array()) {
                for item in files {
                    if let (Some(path), Some(name)) = (
                        item.get("path").and_then(|p| p.as_str()),
                        item.get("name").and_then(|n| n.as_str())
                    ) {
                        all_results.push(SearchResultWithScore {
                            id: path.to_string(),
                            title: name.to_string(),
                            content: format!("Tag match: {}", query),
                            score: 0.8, // Fixed score for tag matches
                        });
                    }
                }
            }
        }
    }

    // Search by filename/path if requested
    if search_paths {
        let result = execute_tool(
            "search_by_filename".to_string(),
            json!({
                "pattern": query
            }),
            Some("cli_user".to_string()),
            Some("fuzzy_search".to_string()),
        ).await?;

        if let Some(data) = result.data {
            if let Some(files) = data.get("files").and_then(|f| f.as_array()) {
                for file_name in files {
                    if let Some(path) = file_name.as_str() {
                        all_results.push(SearchResultWithScore {
                            id: path.to_string(),
                            title: path.split('/').next_back().unwrap_or(path).to_string(),
                            content: format!("Filename match: {}", query),
                            score: 0.7, // Fixed score for filename matches
                        });
                    }
                }
            }
        }
    }

    // If no results from any search type, show message
    if all_results.is_empty() {
        println!("❌ No results found for query: {}", query);
        println!("\n💡 Phase 1.1 Simplification:");
        println!("   Advanced fuzzy search algorithms have been simplified.");
        println!("   Try using different search options or broader terms.");
        return Ok(());
    }

    // Sort by score (descending) and limit results
    all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    all_results.truncate(limit as usize);

    // Display results
    println!("\n🎯 Found {} results:", all_results.len());
    println!("{}", "-".repeat(60));

    for (idx, result) in all_results.iter().enumerate() {
        println!("\n{}. {} (Score: {:.2})", idx + 1, result.title, result.score);
        println!("   📁 {}", result.id);

        // Show preview of content (first 100 characters)
        let preview = if result.content.len() > 100 {
            format!("{}...", &result.content[..100])
        } else {
            result.content.clone()
        };

        if !preview.is_empty() {
            println!("   📄 {}", preview);
        }
    }

    println!("\n💡 Phase 1.1 Simplification Notice:");
    println!("   Complex fuzzy search algorithms have been replaced with tool-based search.");
    println!("   Advanced scoring and ranking features are now simplified.");

    Ok(())
}