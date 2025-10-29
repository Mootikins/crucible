//! Semantic search commands for CLI with real vector search integration
//!
//! This module provides CLI commands for semantic search using real vector similarity
//! search from Phase 2.1. Integrates with kiln_integration::semantic_search()
//! instead of using mock tool execution.

use crate::config::CliConfig;
use crate::interactive::SearchResultWithScore;
use anyhow::Result;
use crucible_config::{ApiConfig, EmbeddingProviderConfig, EmbeddingProviderType, ModelConfig};
use crucible_surrealdb::{
    embedding_pool::{create_embedding_thread_pool_with_crucible_config, EmbeddingThreadPool},
    kiln_integration::{get_database_stats, retrieve_parsed_document},
    kiln_processor::{process_kiln_delta, process_kiln_files, scan_kiln_directory},
    kiln_scanner::{create_kiln_scanner, KilnProcessResult, KilnScannerConfig},
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
        database: "kiln".to_string(),
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
            let error_msg = format!(
                "Failed to connect to kiln database: {}. Make sure your kiln has been processed.",
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

    // Check if embeddings exist, process kiln if needed
    let embeddings_exist = check_embeddings_exist(&client).await?;

    eprintln!("DEBUG SEMANTIC: embeddings_exist = {}", embeddings_exist);

    if !embeddings_exist {
        eprintln!("DEBUG SEMANTIC: Taking FULL processing path (no embeddings)");
        pb.finish_with_message("No embeddings found, starting processing...");
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg:.cyan}")
                .unwrap(),
        );

        if format != "json" {
            println!("❌ No embeddings found in kiln database");
            println!("🚀 Starting kiln processing...\n");
        }

        // Process kiln synchronously (daemon handles background processing)
        match process_kiln_integrated(&client, &config.kiln.path, &pb, &config).await {
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
                    let error_msg = "Processing completed but no embeddings were created. \
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
        eprintln!("DEBUG SEMANTIC: Taking DELTA processing path (embeddings exist)");
        // Embeddings exist - use delta processing for any changed files
        pb.set_message("Checking for kiln changes...");

        eprintln!("DEBUG SEMANTIC: About to call process_kiln_delta_if_needed");
        match process_kiln_delta_if_needed(&client, &config.kiln.path, &pb, &config).await {
            Ok(delta_result) => {
                if delta_result.processed_count > 0 {
                    if format != "json" {
                        println!(
                            "🔄 Detected {} changed files, updated embeddings",
                            delta_result.processed_count
                        );
                    }
                    pb.set_message("Embeddings updated, performing semantic search...");
                } else {
                    pb.set_message("No changes detected, performing semantic search...");
                }
            }
            Err(e) => {
                // Delta processing failed - log but continue with search
                // The embeddings that exist should still be valid
                eprintln!(
                    "⚠️  Delta processing check failed (continuing with existing data): {}",
                    e
                );
                pb.set_message("Performing semantic search with existing data...");
            }
        }
    }

    // Create embedding provider for query embeddings
    eprintln!("DEBUG: Creating embedding provider for query embeddings");
    let embedding_provider = match create_embedding_provider_from_cli_config(&config).await {
        Ok(provider) => {
            eprintln!(
                "DEBUG: Created embedding provider: {}",
                provider.provider_name()
            );
            provider
        }
        Err(e) => {
            eprintln!("ERROR: Failed to create embedding provider: {}", e);
            return Err(e);
        }
    };

    // Create reranker if enabled in config
    eprintln!("DEBUG: Attempting to create reranker from config");
    let reranker = match create_reranker_from_config(&config).await {
        Ok(r) => {
            if r.is_some() {
                eprintln!("DEBUG: Reranker created successfully");
            } else {
                eprintln!("DEBUG: Reranker is None (disabled in config)");
            }
            r
        }
        Err(e) => {
            eprintln!("ERROR: Failed to create reranker: {}", e);
            return Err(e);
        }
    };

    // Determine search parameters based on reranking
    let (initial_limit, final_limit) = if reranker.is_some() {
        let initial = config
            .embedding
            .as_ref()
            .and_then(|e| e.reranking.initial_candidates)
            .unwrap_or(50);
        (initial, top_k as usize)
    } else {
        (top_k as usize, top_k as usize)
    };

    if reranker.is_some() {
        pb.set_message("Performing semantic search with reranking...");
        eprintln!("DEBUG: About to call semantic_search_with_reranking with initial_limit={}, final_limit={}", initial_limit, final_limit);
    } else {
        eprintln!(
            "DEBUG: Reranker is None, calling with both limits={}",
            initial_limit
        );
    }

    // Perform semantic search with optional reranking
    eprintln!("DEBUG: Calling semantic_search_with_reranking NOW");
    let search_results = match crucible_surrealdb::kiln_integration::semantic_search_with_reranking(
        &client,
        &query,
        initial_limit,
        reranker,
        final_limit,
        embedding_provider,
    )
    .await
    {
        Ok(results) => {
            pb.finish_with_message("Search completed");
            results
        }
        Err(e) => {
            pb.finish_with_message("Search failed");
            let error_msg = format!(
                "Semantic search failed: {}. Make sure your kiln has been processed.",
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
            println!("   No results found matching your query.");
            println!("   This could mean:");
            println!("   • Your kiln hasn't been processed yet");
            println!("   • No documents match your semantic query");
            println!("   • There was an issue during processing");
            println!("\n💡 If you believe there should be results, try:");
            println!("   • Running semantic search again to trigger re-processing");
            println!("   • Checking that OBSIDIAN_KILN_PATH points to the correct kiln");
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
            println!("   Results are based on semantic similarity across your kiln.");
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
        // Retrieve document details from database using kiln_integration
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
async fn process_kiln_integrated(
    client: &SurrealClient,
    kiln_path: &std::path::Path,
    pb: &ProgressBar,
    config: &CliConfig,
) -> Result<crucible_surrealdb::kiln_scanner::KilnProcessResult> {
    // Validate kiln path exists
    if !kiln_path.exists() {
        return Err(anyhow::anyhow!(
            "Kiln path '{}' does not exist or is not accessible",
            kiln_path.display()
        ));
    }

    // Initialize database schema (tables, indexes, etc.)
    pb.set_message("Initializing database schema...");
    crucible_surrealdb::kiln_integration::initialize_kiln_schema(client)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize database schema: {}", e))?;

    pb.set_message("Scanning kiln directory...");

    // Create kiln scanner configuration
    let scanner_config = KilnScannerConfig {
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
            crucible_surrealdb::kiln_scanner::ChangeDetectionMethod::ContentHash,
        error_handling_mode: crucible_surrealdb::kiln_scanner::ErrorHandlingMode::ContinueOnError,
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
    let mut scanner = create_kiln_scanner(scanner_config.clone())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create kiln scanner: {}", e))?;

    pb.set_message("Discovering files to process...");

    let kiln_path_buf = PathBuf::from(kiln_path);

    // DEBUG: Log the kiln path being scanned
    eprintln!("DEBUG: Scanning kiln path: {:?}", kiln_path_buf);
    eprintln!("DEBUG: Path exists: {}", kiln_path_buf.exists());
    eprintln!("DEBUG: Is directory: {}", kiln_path_buf.is_dir());

    let scan_result = scanner
        .scan_kiln_directory(&kiln_path_buf)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to scan kiln directory: {}", e))?;

    // DEBUG: Log scan results
    eprintln!(
        "DEBUG: Total files found: {}",
        scan_result.total_files_found
    );
    eprintln!(
        "DEBUG: Markdown files found: {}",
        scan_result.markdown_files_found
    );
    eprintln!(
        "DEBUG: Discovered files count: {}",
        scan_result.discovered_files.len()
    );
    eprintln!("DEBUG: Scan errors: {}", scan_result.scan_errors.len());

    for (i, file) in scan_result.discovered_files.iter().enumerate().take(5) {
        eprintln!(
            "DEBUG: File {}: {:?} (markdown={}, accessible={})",
            i, file.path, file.is_markdown, file.is_accessible
        );
    }

    if scan_result.discovered_files.is_empty() {
        return Err(anyhow::anyhow!(
            "No markdown files found in kiln directory: {}",
            kiln_path.display()
        ));
    }

    pb.set_message("Found files to process, starting embedding generation...");

    // Create embedding thread pool for parallel processing using CLI configuration
    let embedding_pool = create_embedding_pool_from_cli_config(&config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create embedding thread pool: {}", e))?;

    // Process files with integrated pipeline
    pb.set_message("Processing files and generating embeddings...");

    let process_result = process_kiln_files(
        &scan_result.discovered_files,
        client,
        &scanner_config,
        Some(&embedding_pool),
        kiln_path,
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
    // Use the unified config conversion method that handles both new [embedding] section
    // and legacy kiln.embedding_* format
    // Note: to_embedding_config() already returns EmbeddingProviderConfig (re-exported as EmbeddingConfig)
    config.to_embedding_config()
}

/// Create embedding provider directly from CLI configuration
async fn create_embedding_provider_from_cli_config(
    config: &CliConfig,
) -> Result<std::sync::Arc<dyn crucible_llm::embeddings::EmbeddingProvider>> {
    // Convert CLI config to embedding provider config
    let provider_config = create_provider_config_from_cli(config)?;

    // Create the provider using crucible-llm
    // Note: crucible_llm::embeddings::EmbeddingConfig is a re-export of crucible_config::EmbeddingProviderConfig
    crucible_llm::embeddings::create_provider(provider_config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create embedding provider: {}", e))
}

/// Process kiln using delta processing to only update changed files
///
/// This function scans the kiln directory and uses hash comparison to identify
/// which files have changed since last processing. Only changed files are reprocessed,
/// which significantly improves performance for subsequent searches.
///
/// # Performance Target
/// - Single file change: ≤1 second
///
/// # Process Flow
/// 1. Scan kiln directory to discover all files
/// 2. Use process_kiln_delta to detect changes via hash comparison
/// 3. Only reprocess files that have actually changed
/// 4. Return processing statistics
///
/// # Arguments
/// * `client` - SurrealDB client connection
/// * `kiln_path` - Path to the kiln directory
/// * `pb` - Progress bar for user feedback
/// * `config` - CLI configuration
///
/// # Returns
/// KilnProcessResult with processing statistics (0 processed if no changes)
async fn process_kiln_delta_if_needed(
    client: &SurrealClient,
    kiln_path: &std::path::Path,
    pb: &ProgressBar,
    config: &CliConfig,
) -> Result<KilnProcessResult> {
    eprintln!("DEBUG DELTA: Entered process_kiln_delta_if_needed");

    // Validate kiln path exists
    if !kiln_path.exists() {
        return Err(anyhow::anyhow!(
            "Kiln path '{}' does not exist or is not accessible",
            kiln_path.display()
        ));
    }

    pb.set_message("Scanning kiln for changes...");

    // Create kiln scanner configuration
    let scanner_config = KilnScannerConfig {
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
        enable_incremental: true, // Enable incremental for delta processing
        track_file_changes: true,
        change_detection_method:
            crucible_surrealdb::kiln_scanner::ChangeDetectionMethod::ContentHash,
        error_handling_mode: crucible_surrealdb::kiln_scanner::ErrorHandlingMode::ContinueOnError,
        max_error_count: 100,
        error_retry_attempts: 3,
        error_retry_delay_ms: 500,
        skip_problematic_files: true,
        log_errors_detailed: true,
        error_threshold_circuit_breaker: 10,
        circuit_breaker_timeout_ms: 30000,
        processing_timeout_ms: 30000,
    };

    // Scan kiln directory to discover all files
    let kiln_path_buf = PathBuf::from(kiln_path);
    let mut discovered_files = scan_kiln_directory(&kiln_path_buf, &scanner_config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to scan kiln directory: {}", e))?;

    // Filter out .crucible directory files (database, internal files)
    discovered_files.retain(|f| !f.path.components().any(|c| c.as_os_str() == ".crucible"));

    if discovered_files.is_empty() {
        pb.set_message("No markdown files found in kiln");
        return Ok(KilnProcessResult {
            processed_count: 0,
            failed_count: 0,
            errors: Vec::new(),
            total_processing_time: Duration::from_secs(0),
            average_processing_time_per_document: Duration::from_secs(0),
        });
    }

    pb.set_message("Detecting changed files...");

    // Extract file paths for delta processing
    let file_paths: Vec<PathBuf> = discovered_files.iter().map(|f| f.path.clone()).collect();

    // Create embedding thread pool for parallel processing
    let embedding_pool = create_embedding_pool_from_cli_config(&config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create embedding thread pool: {}", e))?;

    pb.set_message("Processing changed files...");

    // Use delta processing - will detect changes and only process what changed
    let process_result = process_kiln_delta(
        file_paths,
        client,
        &scanner_config,
        Some(&embedding_pool),
        kiln_path,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to process kiln delta: {}", e))?;

    pb.set_message("Delta processing completed");

    Ok(process_result)
}

/// Create a reranker from CLI configuration
async fn create_reranker_from_config(
    config: &CliConfig,
) -> Result<Option<std::sync::Arc<dyn crucible_llm::Reranker>>> {
    // Check if reranking is enabled
    let reranking_config = match &config.embedding {
        Some(embedding) => &embedding.reranking,
        None => return Ok(None),
    };

    if !reranking_config.enabled.unwrap_or(false) {
        return Ok(None);
    }

    let provider = reranking_config.provider.as_deref().unwrap_or("fastembed");

    match provider {
        "fastembed" => {
            use crucible_llm::reranking::fastembed::{
                FastEmbedReranker, FastEmbedRerankerConfig, RerankerModel,
            };

            let model_name = reranking_config
                .model
                .as_deref()
                .unwrap_or("bge-reranker-base");

            let model = match model_name {
                "bge-reranker-base" => RerankerModel::BGERerankerBase,
                "bge-reranker-v2-m3" => RerankerModel::BGERerankerV2M3,
                "jina-reranker-v1-turbo-en" => RerankerModel::JINARerankerV1TurboEn,
                "jina-reranker-v2-base-multilingual" => {
                    RerankerModel::JINARerankerV2BaseMultiligual
                }
                _ => {
                    eprintln!(
                        "⚠️  Unknown reranker model '{}', using default bge-reranker-base",
                        model_name
                    );
                    RerankerModel::BGERerankerBase
                }
            };

            let mut reranker_config = FastEmbedRerankerConfig::default();
            reranker_config.model = model;

            if let Some(cache_dir) = &reranking_config.fastembed.cache_dir {
                reranker_config.cache_dir = Some(cache_dir.clone());
            }

            if let Some(batch_size) = reranking_config.fastembed.batch_size {
                reranker_config.batch_size = Some(batch_size);
            }

            if let Some(show_download) = reranking_config.fastembed.show_download {
                reranker_config.show_download = show_download;
            }

            let reranker = FastEmbedReranker::new(reranker_config)?;
            Ok(Some(
                std::sync::Arc::new(reranker) as std::sync::Arc<dyn crucible_llm::Reranker>
            ))
        }
        _ => {
            eprintln!(
                "⚠️  Unsupported reranking provider '{}', reranking disabled",
                provider
            );
            Ok(None)
        }
    }
}
