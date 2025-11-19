//! Semantic search commands for CLI with real vector search integration.
//!
//! Phase 3 refactors introduce a dependency-injected `SemanticSearchService`
//! so the CLI can orchestrate semantic search without hard-coding SurrealDB or
//! embedding provider wiring. This keeps the command thin and testable while
//! preserving the original behaviour and messaging.

use crate::config::CliConfig;
use crate::interactive::SearchResultWithScore;
use anyhow::Result;
use async_trait::async_trait;
use crucible_config::EmbeddingProviderConfig;
use crucible_llm::embeddings::create_provider as create_embedding_provider;
use crucible_surrealdb::{
    embedding_pool::{create_embedding_thread_pool_with_crucible_config, EmbeddingThreadPool},
    kiln_integration::{
        clear_all_embeddings, get_embedding_index_metadata, retrieve_parsed_document,
        semantic_search_with_reranking,
    },
    EmbeddingConfig, SurrealClient, SurrealDbConfig,
};
use crucible_tools::kiln_scanner::{KilnProcessResult, KilnScannerConfig};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Primary interface for executing semantic search from the CLI.
#[async_trait(?Send)]
pub trait SemanticSearchService: Send + Sync {
    async fn search(&self, request: SemanticSearchRequest<'_>) -> Result<SemanticSearchResponse>;
}

/// Minimal progress reporting abstraction so services stay decoupled from
/// the concrete progress bar implementation.
pub trait SemanticProgress: Send + Sync {
    fn start(&self, message: &str);
    fn set_message(&self, message: &str);
    fn finish_with_message(&self, message: &str);
    fn fail_with_message(&self, message: &str);
}

/// Command input passed down to the service layer.
pub struct SemanticSearchRequest<'a> {
    pub config: &'a CliConfig,
    pub query: &'a str,
    pub top_k: u32,
    pub json_output: bool,
    pub progress: Arc<dyn SemanticProgress>,
}

/// Structured response returned by the service before the CLI renders output.
pub struct SemanticSearchResponse {
    pub results: Vec<SearchResultWithScore>,
    pub info_messages: Vec<String>,
}

/// Default production implementation that wires SurrealDB and embedding logic.
#[derive(Default)]
pub struct DefaultSemanticSearchService;

#[async_trait(?Send)]
impl SemanticSearchService for DefaultSemanticSearchService {
    async fn search(&self, request: SemanticSearchRequest<'_>) -> Result<SemanticSearchResponse> {
        let SemanticSearchRequest {
            config,
            query,
            top_k,
            json_output: _,
            progress,
        } = request;

        let mut info_messages = Vec::new();

        progress.set_message("Resolving embedding provider configuration...");
        let provider_config = create_provider_config_from_cli(config)?;
        let expected_model_name = provider_config.model.name.clone();
        let expected_dimensions = provider_config.model.dimensions;

        progress.set_message("Connecting to kiln database...");
        let db_config = SurrealDbConfig {
            namespace: "crucible".to_string(),
            database: "kiln".to_string(),
            path: config.database_path_str()?,
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };

        let client = SurrealClient::new(db_config).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to connect to kiln database: {}. Make sure your kiln has been processed.",
                e
            )
        })?;

        progress.set_message("Database connected, checking embeddings...");

        let metadata = get_embedding_index_metadata(&client).await?;
        let mut embeddings_exist = metadata.is_some();

        if let Some(meta) = &metadata {
            let model_matches = meta
                .model
                .as_ref()
                .map(|m| m.eq_ignore_ascii_case(&expected_model_name))
                .unwrap_or(false);

            let dimensions_match = match (meta.dimensions, expected_dimensions) {
                (Some(actual), Some(expected)) => actual == expected as usize,
                _ => true,
            };

            if !(model_matches && dimensions_match) {
                debug!(
                    "Existing embeddings generated with model {:?} ({:?} dims), expected model '{}' ({:?} dims)",
                    meta.model,
                    meta.dimensions,
                    expected_model_name,
                    expected_dimensions
                );

                info_messages.push("⚠️  Embedding model/dimension mismatch detected".to_string());
                info_messages.push(format!(
                    "    Stored: {} ({:?} dimensions)",
                    meta.model.as_deref().unwrap_or("unknown"),
                    meta.dimensions
                ));
                info_messages.push(format!(
                    "    Requested: {} ({:?} dimensions)",
                    expected_model_name, expected_dimensions
                ));
                info_messages
                    .push("    Clearing existing embeddings and rebuilding index...".to_string());
                info_messages.push(String::new());

                clear_all_embeddings(&client).await?;
                embeddings_exist = false;
            }
        }

        if !embeddings_exist {
            debug!("taking full processing path (no embeddings cached)");
            info_messages.push("❌ No embeddings found in kiln database".to_string());
            info_messages.push("🚀 Starting kiln processing...".to_string());
            info_messages.push(String::new());

            progress.set_message("Scanning kiln and generating embeddings...");
            let process_result = process_kiln_integrated(
                &client,
                &config.kiln.path,
                progress.as_ref(),
                &provider_config,
            )
            .await?;

            info_messages.push("✅ Processing completed successfully".to_string());
            info_messages.push(format!(
                "📊 Processed {} documents in {:.1}s",
                process_result.processed_count,
                process_result.total_processing_time.as_secs_f64()
            ));
            info_messages.push(String::new());

            let embeddings_check = check_embeddings_exist(&client).await?;
            if !embeddings_check {
                return Err(anyhow::anyhow!(
                    "Processing completed but no embeddings were created. Check for processing errors above."
                ));
            }

            progress.set_message("Embeddings ready, performing semantic search...");
        } else {
            debug!("taking delta processing path (embeddings already indexed)");
            progress.set_message("Checking kiln for changes...");

            match process_kiln_delta_if_needed(
                &client,
                &config.kiln.path,
                progress.as_ref(),
                &provider_config,
            )
            .await
            {
                Ok(delta_result) => {
                    if delta_result.processed_count > 0 {
                        info_messages.push(format!(
                            "🔄 Detected {} changed files, updated embeddings",
                            delta_result.processed_count
                        ));
                        progress.set_message("Embeddings updated, performing semantic search...");
                    } else {
                        progress.set_message("No changes detected, performing semantic search...");
                    }
                }
                Err(e) => {
                    warn!(
                        "delta processing check failed (continuing with existing data): {}",
                        e
                    );
                    progress.set_message("Performing semantic search with existing data...");
                }
            }
        }

        debug!("creating embedding provider for query embeddings");
        let embedding_provider = create_embedding_provider(provider_config.clone())
            .await
            .map_err(|e| {
                error!("failed to create embedding provider: {}", e);
                e
            })?;

        debug!("attempting to create reranker from config");
        let reranker = create_reranker_from_config(config).await?;
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
            progress.set_message("Performing semantic search with reranking...");
            debug!(
                "about to call semantic_search_with_reranking with initial_limit={}, final_limit={}",
                initial_limit, final_limit
            );
        } else {
            progress.set_message("Performing semantic search...");
            debug!("reranker disabled, using limit {}", initial_limit);
        }

        let search_results = semantic_search_with_reranking(
            &client,
            query,
            initial_limit,
            reranker,
            final_limit,
            embedding_provider,
        )
        .await
        .map_err(|e| {
            error!("semantic search failed: {}", e);
            anyhow::anyhow!(
                "Semantic search failed: {}. Make sure your kiln has been processed.",
                e
            )
        })?;

        let cli_results = convert_vector_search_results(&client, search_results).await?;

        Ok(SemanticSearchResponse {
            results: cli_results,
            info_messages,
        })
    }
}

/// Progress handle used by the CLI; hidden entirely for JSON output.
#[derive(Clone)]
struct CliProgress {
    inner: Option<ProgressBar>,
}

impl CliProgress {
    fn new(visible: bool) -> Self {
        let inner = if visible {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap(),
            );
            pb.enable_steady_tick(Duration::from_millis(100));
            Some(pb)
        } else {
            None
        };

        Self { inner }
    }
}

impl SemanticProgress for CliProgress {
    fn start(&self, message: &str) {
        if let Some(pb) = &self.inner {
            pb.set_message(message.to_string());
        }
    }

    fn set_message(&self, message: &str) {
        if let Some(pb) = &self.inner {
            pb.set_message(message.to_string());
        }
    }

    fn finish_with_message(&self, message: &str) {
        if let Some(pb) = &self.inner {
            pb.finish_with_message(message.to_string());
        }
    }

    fn fail_with_message(&self, message: &str) {
        if let Some(pb) = &self.inner {
            pb.abandon_with_message(message.to_string());
        }
    }
}

pub async fn execute(
    config: CliConfig,
    query: String,
    top_k: u32,
    format: String,
    show_scores: bool,
) -> Result<()> {
    let service = Arc::new(DefaultSemanticSearchService);
    execute_with_service(service, config, query, top_k, format, show_scores).await
}

pub async fn execute_with_service(
    service: Arc<dyn SemanticSearchService>,
    config: CliConfig,
    query: String,
    top_k: u32,
    format: String,
    show_scores: bool,
) -> Result<()> {
    let is_json = format.eq_ignore_ascii_case("json");
    let progress = Arc::new(CliProgress::new(!is_json));
    progress.start("Initializing database connection...");

    let request = SemanticSearchRequest {
        config: &config,
        query: &query,
        top_k,
        json_output: is_json,
        progress: progress.clone(),
    };

    match service.search(request).await {
        Ok(response) => {
            progress.finish_with_message("Search completed");
            let SemanticSearchResponse {
                results,
                info_messages,
            } = response;

            if !is_json {
                for line in info_messages {
                    if line.is_empty() {
                        println!();
                    } else {
                        println!("{}", line);
                    }
                }
            }

            if results.is_empty() {
                if is_json {
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
                    println!("   • Checking your kiln.path configuration (see: cru config show)");
                }
                return Ok(());
            }

            if is_json {
                let json_output = json!({
                    "query": query,
                    "total_results": results.len(),
                    "results": results.iter().map(|r| {
                        json!({
                            "id": r.id,
                            "title": r.title,
                            "content_preview": if r.content.len() > 200 {
                                let mut truncate_pos = 200.min(r.content.len());
                                while truncate_pos > 0 && !r.content.is_char_boundary(truncate_pos) {
                                    truncate_pos -= 1;
                                }
                                format!("{}...", &r.content[..truncate_pos])
                            } else {
                                r.content.clone()
                            },
                            "score": r.score
                        })
                    }).collect::<Vec<_>>()
                });
                println!("{}", serde_json::to_string_pretty(&json_output)?);
                return Ok(());
            }

            println!("🔍 Semantic Search Results (Real Vector Search)");
            println!("📝 Query: {}", query);
            println!("📊 Found {} results\n", results.len());

            for (idx, result) in results.iter().enumerate() {
                println!("{}. {} ({:.4})", idx + 1, result.title, result.score);
                println!("   📁 {}", result.id);

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

            Ok(())
        }
        Err(err) => {
            progress.fail_with_message("Search failed");
            if is_json {
                let json_error = json!({
                    "error": true,
                    "message": err.to_string(),
                    "query": query,
                    "total_results": 0,
                    "results": []
                });
                println!("{}", serde_json::to_string_pretty(&json_error)?);
                Ok(())
            } else {
                Err(err)
            }
        }
    }
}

async fn process_kiln_integrated(
    client: &SurrealClient,
    kiln_path: &Path,
    progress: &dyn SemanticProgress,
    provider_config: &EmbeddingProviderConfig,
) -> Result<KilnProcessResult> {
    if !kiln_path.exists() {
        return Err(anyhow::anyhow!(
            "Kiln path '{}' does not exist or is not accessible",
            kiln_path.display()
        ));
    }

    progress.set_message("Initializing database schema...");
    crucible_surrealdb::kiln_integration::initialize_kiln_schema(client)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize database schema: {}", e))?;

    // Clear the notes table to ensure fresh processing
    // This prevents issues where files might be marked as already processed
    debug!("Clearing existing notes to ensure fresh processing");
    client
        .query("DELETE notes", &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to clear notes table: {}", e))?;

    progress.set_message("Scanning kiln directory...");

    // Give filesystem time to complete any pending write operations
    // This is especially important in test scenarios where files are created
    // immediately before processing begins. Also ensures file watcher has
    // time to initialize and won't interfere with the initial scan.
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    let scanner_config = KilnScannerConfig {
        max_file_size_bytes: 50 * 1024 * 1024,
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
        enable_incremental: false,
        track_file_changes: true,
        change_detection_method:
            crucible_tools::kiln_scanner::ChangeDetectionMethod::ContentHash,
        error_handling_mode: crucible_tools::kiln_scanner::ErrorHandlingMode::ContinueOnError,
        max_error_count: 100,
        error_retry_attempts: 3,
        error_retry_delay_ms: 500,
        skip_problematic_files: true,
        log_errors_detailed: true,
        error_threshold_circuit_breaker: 10,
        circuit_breaker_timeout_ms: 30000,
        processing_timeout_ms: 30000,
    };

    let kiln_path_buf = kiln_path.to_path_buf();

    // Retry scanning to ensure all files are discovered
    // Filesystem operations may be delayed, especially in test scenarios
    // We keep rescanning if we find MORE files on subsequent attempts
    let mut discovered_files = Vec::new();
    let mut best_markdown_count = 0;

    for attempt in 0..3 {
        let files = scan_kiln_directory(&kiln_path_buf, &scanner_config)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to scan kiln directory: {}", e))?;

        let markdown_count = files
            .iter()
            .filter(|f| f.is_markdown && f.is_accessible)
            .count();

        info!(
            "Scan attempt {}: discovered {} total files, {} markdown files",
            attempt + 1,
            files.len(),
            markdown_count
        );

        // Keep the scan with the most markdown files
        if markdown_count > best_markdown_count {
            best_markdown_count = markdown_count;
            discovered_files = files;
        }

        // If we found the same count as last time and it's > 0, we're stable
        if attempt > 0 && markdown_count == best_markdown_count && markdown_count > 0 {
            break;
        }

        // Wait between retries to allow filesystem operations to complete
        if attempt < 2 {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }
    }

    let markdown_count = best_markdown_count;

    for (i, file) in discovered_files.iter().enumerate().take(10) {
        debug!(
            "Discovered file {}: {} (markdown={}, accessible={})",
            i + 1,
            file.path.display(),
            file.is_markdown,
            file.is_accessible
        );
    }

    if discovered_files.is_empty() || markdown_count == 0 {
        return Err(anyhow::anyhow!(
            "No markdown files found in kiln directory: {}",
            kiln_path.display()
        ));
    }

    progress.set_message("Found files to process, starting embedding generation...");
    let embedding_pool = create_embedding_pool_from_config(provider_config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create embedding thread pool: {}", e))?;

    progress.set_message("Processing files and generating embeddings...");
    let process_result = process_kiln_files(
        &discovered_files,
        client,
        &scanner_config,
        Some(&embedding_pool),
        kiln_path,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to process kiln files: {}", e))?;

    progress.set_message("Processing completed successfully");

    Ok(process_result)
}

async fn process_kiln_delta_if_needed(
    client: &SurrealClient,
    kiln_path: &Path,
    progress: &dyn SemanticProgress,
    provider_config: &EmbeddingProviderConfig,
) -> Result<KilnProcessResult> {
    if !kiln_path.exists() {
        return Err(anyhow::anyhow!(
            "Kiln path '{}' does not exist or is not accessible",
            kiln_path.display()
        ));
    }

    progress.set_message("Scanning kiln for changes...");
    let scanner_config = KilnScannerConfig {
        max_file_size_bytes: 50 * 1024 * 1024,
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
        enable_incremental: true,
        track_file_changes: true,
        change_detection_method:
            crucible_tools::kiln_scanner::ChangeDetectionMethod::ContentHash,
        error_handling_mode: crucible_tools::kiln_scanner::ErrorHandlingMode::ContinueOnError,
        max_error_count: 100,
        error_retry_attempts: 3,
        error_retry_delay_ms: 500,
        skip_problematic_files: true,
        log_errors_detailed: true,
        error_threshold_circuit_breaker: 10,
        circuit_breaker_timeout_ms: 30000,
        processing_timeout_ms: 30000,
    };

    let kiln_path_buf = PathBuf::from(kiln_path);
    let mut discovered_files = scan_kiln_directory(&kiln_path_buf, &scanner_config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to scan kiln directory: {}", e))?;

    discovered_files.retain(|f| !f.path.components().any(|c| c.as_os_str() == ".crucible"));

    if discovered_files.is_empty() {
        progress.set_message("No markdown files found in kiln");
        return Ok(KilnProcessResult {
            processed_count: 0,
            failed_count: 0,
            errors: Vec::new(),
            total_processing_time: Duration::from_secs(0),
            average_processing_time_per_document: Duration::from_secs(0),
        });
    }

    progress.set_message("Detecting changed files...");
    let file_paths: Vec<PathBuf> = discovered_files.iter().map(|f| f.path.clone()).collect();

    let embedding_pool = create_embedding_pool_from_config(provider_config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create embedding thread pool: {}", e))?;

    progress.set_message("Processing changed files...");
    let process_result = process_kiln_delta(
        file_paths,
        client,
        &scanner_config,
        Some(&embedding_pool),
        kiln_path,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to process kiln delta: {}", e))?;

    progress.set_message("Delta processing completed");

    Ok(process_result)
}

async fn create_embedding_pool_from_config(
    provider_config: &EmbeddingProviderConfig,
) -> Result<EmbeddingThreadPool> {
    let pool_config = EmbeddingConfig::default();

    create_embedding_thread_pool_with_crucible_config(pool_config, provider_config.clone())
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to create embedding thread pool with provider config: {}",
                e
            )
        })
}

fn create_provider_config_from_cli(config: &CliConfig) -> Result<EmbeddingProviderConfig> {
    config.to_embedding_config()
}

async fn check_embeddings_exist(client: &SurrealClient) -> Result<bool> {
    Ok(get_embedding_index_metadata(client).await?.is_some())
}

async fn convert_vector_search_results(
    client: &SurrealClient,
    search_results: Vec<(String, f64)>,
) -> Result<Vec<SearchResultWithScore>> {
    let mut cli_results = Vec::new();

    for (document_id, similarity_score) in search_results {
        match retrieve_parsed_document(client, &document_id).await {
            Ok(parsed_document) => {
                let display_path = parsed_document.path.display().to_string();
                let title = parsed_document.title();

                cli_results.push(SearchResultWithScore {
                    id: display_path,
                    title,
                    content: parsed_document.content.plain_text,
                    score: similarity_score,
                });
            }
            Err(err) => {
                warn!(
                    "Failed to load parsed note {} from database: {}",
                    document_id, err
                );

                cli_results.push(SearchResultWithScore {
                    id: document_id.clone(),
                    title: format!("Note {}", document_id),
                    content: "Note content not available".to_string(),
                    score: similarity_score,
                });
            }
        }
    }

    cli_results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(cli_results)
}

async fn create_reranker_from_config(
    config: &CliConfig,
) -> Result<Option<std::sync::Arc<dyn crucible_llm::Reranker>>> {
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
                    warn!(
                        "unknown reranker model '{}', using default bge-reranker-base",
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
            warn!(
                "unsupported reranking provider '{}', reranking disabled",
                provider
            );
            Ok(None)
        }
    }
}
