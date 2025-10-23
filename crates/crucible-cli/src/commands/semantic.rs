//! Simplified semantic search commands for CLI
//!
//! This module provides simplified CLI commands for semantic search.
//! Complex embedding services have been removed in Phase 1.1 dead code elimination.
//! Now provides basic semantic search with tool-based functionality.

use anyhow::Result;
use crucible_tools::execute_tool;
use serde_json::json;
use crate::config::CliConfig;
use crate::output;
use crate::interactive::SearchResultWithScore;
use indicatif::{ProgressBar, ProgressStyle};

pub async fn execute(
    config: CliConfig,
    query: String,
    top_k: u32,
    format: String,
    show_scores: bool,
) -> Result<()> {
    // Initialize crucible-tools for simplified semantic search
    crucible_tools::init();

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );

    pb.set_message("Performing semantic search...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    // Use semantic search tool
    let result = execute_tool(
        "semantic_search".to_string(),
        json!({
            "query": query,
            "top_k": top_k
        }),
        Some("cli_user".to_string()),
        Some("semantic_search".to_string()),
    ).await?;

    pb.finish_with_message("Search completed");

    if let Some(data) = result.data {
        if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
            let search_results: Vec<SearchResultWithScore> = results
                .iter()
                .enumerate()
                .filter_map(|(idx, item)| {
                    if let (Some(file_path), Some(title), Some(score)) = (
                        item.get("file_path").and_then(|p| p.as_str()),
                        item.get("title").and_then(|t| t.as_str()),
                        item.get("score").and_then(|s| s.as_f64())
                    ) {
                        Some(SearchResultWithScore {
                            id: file_path.to_string(),
                            title: title.to_string(),
                            content: item.get("content")
                                .and_then(|c| c.as_str())
                                .unwrap_or("")
                                .to_string(),
                            score,
                        })
                    } else {
                        None
                    }
                })
                .collect();

            if search_results.is_empty() {
                println!("❌ No semantic search results found for query: {}", query);
                println!("\n💡 Phase 1.1 Simplification:");
                println!("   Advanced semantic search algorithms have been simplified.");
                println!("   Consider using broader terms or different keywords.");
                return Ok(());
            }

            // Display results
            match format.as_str() {
                "json" => {
                    let json_output = json!({
                        "query": query,
                        "total_results": search_results.len(),
                        "results": search_results.iter().map(|r| {
                            json!({
                                "id": r.id,
                                "title": r.title,
                                "content_preview": if r.content.len() > 200 {
                                    format!("{}...", &r.content[..200])
                                } else {
                                    r.content.clone()
                                },
                                "score": r.score
                            })
                        }).collect::<Vec<_>>()
                    });
                    println!("{}", serde_json::to_string_pretty(&json_output)?);
                }
                _ => {
                    println!("🔍 Semantic Search Results");
                    println!("📝 Query: {}", query);
                    println!("📊 Found {} results\n", search_results.len());

                    for (idx, result) in search_results.iter().enumerate() {
                        println!("{}. {} ({:.2})", idx + 1, result.title, result.score);
                        println!("   📁 {}", result.id);

                        // Show content preview
                        let preview = if result.content.len() > 300 {
                            format!("{}...", &result.content[..300])
                        } else {
                            result.content.clone()
                        };

                        if !preview.is_empty() {
                            println!("   📄 {}", preview);
                        }

                        if show_scores {
                            println!("   🎯 Similarity Score: {:.4}", result.score);
                        }
                        println!();
                    }

                    println!("💡 Phase 1.1 Simplification Notice:");
                    println!("   Complex embedding and vector search have been simplified.");
                    println!("   Semantic similarity scores are now approximated.");
                }
            }

            return Ok(());
        }
    }

    // If no results from semantic search, show message
    println!("❌ Semantic search failed or returned no results");
    println!("💡 This may be due to Phase 1.1 simplification of semantic search features.");
    println!("   Consider using regular search with 'crucible search <query>'");

    Ok(())
}