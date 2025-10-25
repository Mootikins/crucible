//! Semantic search commands for CLI with real vector search integration
//!
//! This module provides CLI commands for semantic search using real vector similarity
//! search from Phase 2.1. Integrates with vault_integration::semantic_search()
//! instead of using mock tool execution.

use anyhow::Result;
use serde_json::json;
use crate::config::CliConfig;
use crate::interactive::SearchResultWithScore;
use indicatif::{ProgressBar, ProgressStyle};
use crucible_surrealdb::{
    vault_integration::{semantic_search, retrieve_parsed_document},
    SurrealClient,
    SurrealDbConfig,
};

pub async fn execute(
    config: CliConfig,
    query: String,
    top_k: u32,
    format: String,
    show_scores: bool,
) -> Result<()> {
    // Initialize progress bar
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );

    pb.set_message("Initializing database connection...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    // Initialize database connection
    let db_config = SurrealDbConfig {
        namespace: "crucible".to_string(),
        database: "vault".to_string(),
        path: config.database_path_str()?,
        max_connections: Some(10),
        timeout_seconds: Some(30),
    };

    let client = match SurrealClient::new(db_config).await {
        Ok(client) => {
            pb.set_message("Database connected, performing semantic search...");
            client
        }
        Err(e) => {
            pb.finish_with_message("Database connection failed");
            return Err(anyhow::anyhow!("Failed to connect to database: {}. Make sure embeddings have been generated for your vault.", e));
        }
    };

    // Perform real semantic search using vector similarity
    let search_results = match semantic_search(&client, &query, top_k as usize).await {
        Ok(results) => {
            pb.finish_with_message("Search completed");
            results
        }
        Err(e) => {
            pb.finish_with_message("Search failed");
            return Err(anyhow::anyhow!("Semantic search failed: {}. Make sure embeddings exist for your vault documents.", e));
        }
    };

    // Convert search results to CLI format
    let cli_results = convert_vector_search_results(&client, search_results).await?;

    if cli_results.is_empty() {
        println!("❌ No semantic search results found for query: {}", query);
        println!("\n💡 Real Vector Search Integration:");
        println!("   No embeddings found matching your query.");
        println!("   Ensure documents have been processed with embeddings using:");
        println!("   'crucible vault process' to generate embeddings for your vault.");
        return Ok(());
    }

    // Display results
    match format.as_str() {
        "json" => {
            let json_output = json!({
                "query": query,
                "total_results": cli_results.len(),
                "results": cli_results.iter().map(|r| {
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
            println!("🔍 Semantic Search Results (Real Vector Search)");
            println!("📝 Query: {}", query);
            println!("📊 Found {} results\n", cli_results.len());

            for (idx, result) in cli_results.iter().enumerate() {
                println!("{}. {} ({:.4})", idx + 1, result.title, result.score);
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

            println!("💡 Real Vector Search Integration:");
            println!("   Results are based on vector similarity using document embeddings.");
            println!("   Higher scores indicate better semantic matches to your query.");
        }
    }

    Ok(())
}

/// Convert vector search results to CLI format with document content
async fn convert_vector_search_results(
    client: &SurrealClient,
    search_results: Vec<(String, f64)>,
) -> Result<Vec<SearchResultWithScore>> {
    let mut cli_results = Vec::new();

    for (document_id, similarity_score) in search_results {
        // Retrieve document details from database using vault_integration
        match retrieve_parsed_document(client, &document_id).await {
            Ok(parsed_document) => {
                // Extract title and content from the parsed document
                let title = parsed_document.frontmatter
                    .and_then(|fm| fm.get_string("title"))
                    .unwrap_or_else(|| {
                        // Fallback to first line of content
                        parsed_document.content.plain_text
                            .lines()
                            .next()
                            .unwrap_or("Untitled Document")
                            .to_string()
                    });

                cli_results.push(SearchResultWithScore {
                    id: document_id.clone(),
                    title,
                    content: parsed_document.content.plain_text,
                    score: similarity_score,
                });
            }
            Err(_) => {
                // If document not found, create a basic result
                cli_results.push(SearchResultWithScore {
                    id: document_id.clone(),
                    title: format!("Document {}", document_id),
                    content: "Document content not available".to_string(),
                    score: similarity_score,
                });
            }
        }
    }

    // Sort by similarity score (descending)
    cli_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    Ok(cli_results)
}

