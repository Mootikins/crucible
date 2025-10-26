//! Semantic search commands for CLI with real vector search integration
//!
//! This module provides CLI commands for semantic search using real vector similarity
//! search from Phase 2.1. Integrates with vault_integration::semantic_search()
//! instead of using mock tool execution.

use crate::config::CliConfig;
use crate::interactive::SearchResultWithScore;
use anyhow::Result;
use crucible_config::{ApiConfig, EmbeddingProviderConfig, EmbeddingProviderType, ModelConfig};
use crucible_surrealdb::{
    embedding_pool::{create_embedding_thread_pool_with_crucible_config, EmbeddingThreadPool},
    vault_integration::{get_database_stats, retrieve_parsed_document, semantic_search},
    vault_processor::process_vault_files,
    vault_scanner::{create_vault_scanner, VaultScannerConfig},
    EmbeddingConfig, SurrealClient, SurrealDbConfig,
};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

pub async fn execute(
    config: CliConfig,
    query: String,
    top_k: u32,
    format: String,
    show_scores: bool,
) -> Result<()> {
    // Initialize progress bar - only show for non-JSON output
    let pb = if format == "json" {
        // For JSON output, create a dummy progress bar that doesn't display
        ProgressBar::hidden()
    } else {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        pb.set_message("Initializing database connection...");
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        pb
    };

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
            pb.set_message("Database connected, checking embeddings...");
            client
        }
        Err(e) => {
            pb.finish_with_message("Database connection failed");
            let error_msg = format!("Failed to connect to database: {}. Make sure embeddings have been generated for your kiln.", e);
            if format == "json" {
                let json_error = json!({
                    "error": true,
                    "message": error_msg,
                    "query": query,
                    "total_results": 0,
                    "results": []
                });
                println!("{}", serde_json::to_string_pretty(&json_error)?);
                return Ok(());
            } else {
                return Err(anyhow::anyhow!(error_msg));
            }
        }
    };

    // Check if embeddings exist, process kiln if needed
    let embeddings_exist = check_embeddings_exist(&client).await?;

    if !embeddings_exist {
        pb.finish_with_message("No embeddings found, starting processing...");
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg:.cyan}")
                .unwrap(),
        );

        if format != "json" {
            println!("❌ No embeddings found in database");
            println!("🚀 Starting kiln processing to generate embeddings...\n");
        }

        // Process kiln using integrated functionality
        match process_vault_integrated(&client, &config.kiln.path, &pb, &config).await {
            Ok(process_result) => {
                if format != "json" {
                    println!("✅ Processing completed successfully");
                    println!(
                        "📊 Processed {} documents in {:.1}s",
                        process_result.processed_count,
                        process_result.total_processing_time.as_secs_f64()
                    );
                    println!();
                }

                // Verify embeddings were created
                let embeddings_check = check_embeddings_exist(&client).await?;
                if !embeddings_check {
                    let error_msg = "Processing completed but no embeddings were found. \
                        Check for processing errors above.";
                    if format == "json" {
                        let json_error = json!({
                            "error": true,
                            "message": error_msg,
                            "query": query,
                            "total_results": 0,
                            "results": []
                        });
                        println!("{}", serde_json::to_string_pretty(&json_error)?);
                        return Ok(());
                    } else {
                        return Err(anyhow::anyhow!(error_msg));
                    }
                }

                // Update progress bar for search
                pb.set_message("Embeddings ready, performing semantic search...");
                pb.enable_steady_tick(Duration::from_millis(100));
            }
            Err(e) => {
                let error_msg = format!(
                    "Failed to process kiln: {}. \
                    Please check that OBSIDIAN_KILN_PATH is set correctly and try again.",
                    e
                );
                if format == "json" {
                    let json_error = json!({
                        "error": true,
                        "message": error_msg,
                        "query": query,
                        "total_results": 0,
                        "results": []
                    });
                    println!("{}", serde_json::to_string_pretty(&json_error)?);
                    return Ok(());
                } else {
                    return Err(anyhow::anyhow!(error_msg));
                }
            }
        }
    } else {
        pb.set_message("Embeddings found, performing semantic search...");
    }

    // Perform real semantic search using vector similarity
    let search_results = match semantic_search(&client, &query, top_k as usize).await {
        Ok(results) => {
            pb.finish_with_message("Search completed");
            results
        }
        Err(e) => {
            pb.finish_with_message("Search failed");
            let error_msg = format!(
                "Semantic search failed: {}. Make sure embeddings exist for your kiln documents.",
                e
            );
            if format == "json" {
                let json_error = json!({
                    "error": true,
                    "message": error_msg,
                    "query": query,
                    "total_results": 0,
                    "results": []
                });
                println!("{}", serde_json::to_string_pretty(&json_error)?);
                return Ok(());
            } else {
                return Err(anyhow::anyhow!(error_msg));
            }
        }
    };

    // Convert search results to CLI format
    let cli_results = convert_vector_search_results(&client, search_results).await?;

    if cli_results.is_empty() {
        if format == "json" {
            let json_result = json!({
                "query": query,
                "total_results": 0,
                "results": [],
                "message": "No semantic search results found for query"
            });
            println!("{}", serde_json::to_string_pretty(&json_result)?);
        } else {
            println!("❌ No semantic search results found for query: {}", query);
            println!("\n💡 Semantic Search Integration:");
            println!("   No embeddings found matching your query.");
            println!("   This could mean:");
            println!("   • Your kiln hasn't been processed yet");
            println!("   • No documents match your semantic query");
            println!("   • There was an issue during processing");
            println!("\n💡 If you believe there should be results, try:");
            println!("   • Running semantic search again to trigger re-processing");
            println!("   • Checking that OBSIDIAN_VAULT_PATH points to the correct kiln");
        }
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

            println!("💡 Semantic Search Integration:");
            println!("   Results are based on vector similarity using document embeddings.");
            println!("   Higher scores indicate better semantic matches to your query.");
            println!("   Embeddings are auto-generated when needed using integrated processing.");
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
                let title = parsed_document
                    .frontmatter
                    .and_then(|fm| fm.get_string("title"))
                    .unwrap_or_else(|| {
                        // Fallback to first line of content
                        parsed_document
                            .content
                            .plain_text
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
    cli_results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(cli_results)
}

/// Check if embeddings exist in the database
async fn check_embeddings_exist(client: &SurrealClient) -> Result<bool> {
    match get_database_stats(client).await {
        Ok(stats) => Ok(stats.total_embeddings > 0),
        Err(_e) => {
            // Fallback to direct query if stats function fails
            let embeddings_sql = "SELECT count() as total FROM embeddings LIMIT 1";
            let result = client
                .query(embeddings_sql, &[])
                .await
                .map_err(|e| anyhow::anyhow!("Failed to query embeddings: {}", e))?;

            let embeddings_count = result
                .records
                .first()
                .and_then(|r| r.data.get("total"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            Ok(embeddings_count > 0)
        }
    }
}

/// Process kiln using integrated functionality (no external daemon)
async fn process_vault_integrated(
    client: &SurrealClient,
    vault_path: &std::path::Path,
    pb: &ProgressBar,
    config: &CliConfig,
) -> Result<crucible_surrealdb::vault_scanner::VaultProcessResult> {
    // Validate kiln path exists
    if !vault_path.exists() {
        return Err(anyhow::anyhow!(
            "Kiln path '{}' does not exist or is not accessible",
            vault_path.display()
        ));
    }

    pb.set_message("Scanning kiln directory...");

    // Create kiln scanner configuration
    let scanner_config = VaultScannerConfig {
        max_file_size_bytes: 50 * 1024 * 1024, // 50MB
        max_recursion_depth: 10,
        recursive_scan: true,
        include_hidden_files: false,
        file_extensions: vec!["md".to_string(), "markdown".to_string()],
        parallel_processing: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4),
        batch_processing: true,
        batch_size: 16,
        enable_embeddings: true,
        process_embeds: true,
        process_wikilinks: true,
        enable_incremental: false, // Process all files for simplicity
        track_file_changes: true,
        change_detection_method:
            crucible_surrealdb::vault_scanner::ChangeDetectionMethod::ContentHash,
        error_handling_mode: crucible_surrealdb::vault_scanner::ErrorHandlingMode::ContinueOnError,
        max_error_count: 100,
        error_retry_attempts: 3,
        error_retry_delay_ms: 500,
        skip_problematic_files: true,
        log_errors_detailed: true,
        error_threshold_circuit_breaker: 10,
        circuit_breaker_timeout_ms: 30000,
        processing_timeout_ms: 30000,
    };

    // Create kiln scanner and scan directory
    let mut scanner = create_vault_scanner(scanner_config.clone())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create kiln scanner: {}", e))?;

    pb.set_message("Discovering files to process...");

    let vault_path_buf = PathBuf::from(vault_path);
    let scan_result = scanner
        .scan_vault_directory(&vault_path_buf)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to scan kiln directory: {}", e))?;

    if scan_result.discovered_files.is_empty() {
        return Err(anyhow::anyhow!(
            "No markdown files found in kiln directory: {}",
            vault_path.display()
        ));
    }

    pb.set_message("Found files to process, starting embedding generation...");

    // Create embedding thread pool for parallel processing using CLI configuration
    let embedding_pool = create_embedding_pool_from_cli_config(&config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create embedding thread pool: {}", e))?;

    // Process files with integrated pipeline
    pb.set_message("Processing files and generating embeddings...");

    let process_result = process_vault_files(
        &scan_result.discovered_files,
        client,
        &scanner_config,
        Some(&embedding_pool),
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to process kiln files: {}", e))?;

    pb.set_message("Processing completed successfully");

    Ok(process_result)
}

/// Create embedding thread pool from CLI configuration
async fn create_embedding_pool_from_cli_config(config: &CliConfig) -> Result<EmbeddingThreadPool> {
    // Convert CLI embedding config to crucible-config provider config
    let provider_config = create_provider_config_from_cli(config)?;

    // Create thread pool with real provider configuration
    let pool_config = EmbeddingConfig::default(); // Use default pool config

    create_embedding_thread_pool_with_crucible_config(pool_config, provider_config)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to create embedding thread pool with provider config: {}",
                e
            )
        })
}

/// Convert CLI configuration to crucible-config provider configuration
fn create_provider_config_from_cli(config: &CliConfig) -> Result<EmbeddingProviderConfig> {
    // Extract model name from CLI config
    let model_name = config.kiln.embedding_model.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "Embedding model is not configured. Please set it via:\n\
            - Environment variable: EMBEDDING_MODEL\n\
            - CLI argument: --embedding-model <model>\n\
            - Config file: embedding_model = \"<model>\""
        )
    })?;

    // Create provider config based on embedding URL
    // For now, we default to Ollama provider for local/embedded models
    // and OpenAI for cloud endpoints
    let provider_type = if config.kiln.embedding_url.contains("api.openai.com") {
        EmbeddingProviderType::OpenAI
    } else if config.kiln.embedding_url.contains("localhost")
        || config.kiln.embedding_url.contains("127.0.0.1")
        || config.kiln.embedding_url.contains("11434")
    {
        EmbeddingProviderType::Ollama
    } else {
        // Default to Ollama for custom endpoints
        EmbeddingProviderType::Ollama
    };

    // Create API config
    let mut api_config = ApiConfig {
        key: None, // API keys not needed for Ollama
        base_url: Some(config.kiln.embedding_url.clone()),
        timeout_seconds: Some(config.network.timeout_secs.unwrap_or(60)),
        retry_attempts: Some(config.network.max_retries.unwrap_or(3)),
        headers: std::collections::HashMap::new(),
    };

    // For OpenAI, try to get API key from environment
    if provider_type == EmbeddingProviderType::OpenAI {
        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            api_config.key = Some(api_key);
        }
    }

    // Create model config
    let model_config = ModelConfig {
        name: model_name.clone(),
        dimensions: None, // Let provider determine dimensions
        max_tokens: Some(8192),
    };

    // Create provider config
    let provider_config = EmbeddingProviderConfig {
        provider_type,
        api: api_config,
        model: model_config,
        options: std::collections::HashMap::new(),
    };

    // Validate the configuration
    provider_config
        .validate()
        .map_err(|e| anyhow::anyhow!("Invalid embedding provider configuration: {}", e))?;

    Ok(provider_config)
}
