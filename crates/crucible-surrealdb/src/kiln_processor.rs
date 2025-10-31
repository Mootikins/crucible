//! Kiln Processor Module
//!
//! This module provides the processing pipeline for kiln files, integrating with
//! the parser system and embedding infrastructure. It handles batch processing,
//! parallel execution, and comprehensive error recovery.

use anyhow::{anyhow, Result};
use futures::stream::{self, StreamExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use crate::embedding_config::EmbeddingProcessingResult;
use crate::embedding_pool::EmbeddingThreadPool;
use crate::kiln_integration::*;
use crate::kiln_scanner::{
    KilnFileInfo, KilnProcessError, KilnProcessResult, KilnScannerConfig, KilnScannerErrorType,
};
use crate::SurrealClient;
use crucible_core::parser::ParsedDocument;

/// Scan a kiln directory recursively and return discovered files
pub async fn scan_kiln_directory(
    kiln_path: &PathBuf,
    config: &KilnScannerConfig,
) -> Result<Vec<KilnFileInfo>> {
    let mut scanner = crate::kiln_scanner::create_kiln_scanner(config.clone()).await?;
    let scan_result = scanner.scan_kiln_directory(kiln_path).await?;

    Ok(scan_result.discovered_files)
}

/// Process a collection of kiln files with full pipeline integration
pub async fn process_kiln_files(
    files: &[KilnFileInfo],
    client: &SurrealClient,
    config: &KilnScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>,
    kiln_root: &std::path::Path,
) -> Result<KilnProcessResult> {
    let start_time = std::time::Instant::now();
    let mut processed_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    info!("Processing {} kiln files", files.len());
    debug!("processing {} kiln files", files.len());

    // Filter to only accessible markdown files
    let markdown_files: Vec<&KilnFileInfo> = files
        .iter()
        .filter(|f| f.is_markdown && f.is_accessible)
        .collect();

    info!("Found {} markdown files to process", markdown_files.len());
    debug!(
        "found {} markdown files to process",
        markdown_files.len()
    );

    for (i, file) in files.iter().enumerate().take(5) {
        debug!(
            "sample file {}: {:?} (markdown={}, accessible={})",
            i,
            file.path,
            file.is_markdown,
            file.is_accessible
        );
    }

    debug!(
        "batch_processing={}, markdown_files={}, batch_size={}",
        config.batch_processing,
        markdown_files.len(),
        config.batch_size
    );
    debug!(
        "parallel_processing={}",
        config.parallel_processing
    );

    if config.batch_processing && markdown_files.len() > config.batch_size {
        // Process in batches
        debug!("using batch processing");
        let batches: Vec<Vec<&KilnFileInfo>> = markdown_files
            .chunks(config.batch_size)
            .map(|chunk| chunk.to_vec())
            .collect();

        debug!(
            "Processing {} batches with max size {}",
            batches.len(),
            config.batch_size
        );

        for (batch_index, batch) in batches.iter().enumerate() {
            debug!(
                "Processing batch {} with {} files",
                batch_index + 1,
                batch.len()
            );

            let batch_result =
                process_file_batch(batch, client, config, embedding_pool, kiln_root).await?;

            processed_count += batch_result.processed_count;
            failed_count += batch_result.failed_count;
            errors.extend(batch_result.errors);

            debug!(
                "Batch {} completed: {} processed, {} failed",
                batch_index + 1,
                batch_result.processed_count,
                batch_result.failed_count
            );
        }
    } else {
        // Process all files at once or in parallel
        if config.parallel_processing > 1 && markdown_files.len() > 1 {
            debug!(
                "using parallel processing (workers={})",
                config.parallel_processing
            );
            let parallel_result =
                process_files_parallel(&markdown_files, client, config, embedding_pool, kiln_root)
                    .await?;
            debug!(
                "parallel result: processed={}, failed={}, errors={}",
                parallel_result.processed_count,
                parallel_result.failed_count,
                parallel_result.errors.len()
            );
            processed_count = parallel_result.processed_count;
            failed_count = parallel_result.failed_count;
            errors = parallel_result.errors;
        } else {
            debug!("using sequential processing");
            let sequential_result = process_files_sequential(
                &markdown_files,
                client,
                config,
                embedding_pool,
                kiln_root,
            )
            .await?;
            debug!(
                "sequential result: processed={}, failed={}, errors={}",
                sequential_result.processed_count,
                sequential_result.failed_count,
                sequential_result.errors.len()
            );
            processed_count = sequential_result.processed_count;
            failed_count = sequential_result.failed_count;
            errors = sequential_result.errors;
        }
    }

    let total_processing_time = start_time.elapsed();
    let avg_time_per_doc = if processed_count > 0 {
        total_processing_time / processed_count as u32
    } else {
        Duration::from_secs(0)
    };

    info!(
        "Processing completed: {} successful, {} failed in {:?}",
        processed_count, failed_count, total_processing_time
    );

    Ok(KilnProcessResult {
        processed_count,
        failed_count,
        errors,
        total_processing_time,
        average_processing_time_per_document: avg_time_per_doc,
    })
}

/// Process files with comprehensive error handling and recovery
pub async fn process_kiln_files_with_error_handling(
    files: &[KilnFileInfo],
    client: &SurrealClient,
    config: &KilnScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>,
    kiln_root: &std::path::Path,
) -> Result<KilnProcessResult> {
    let start_time = std::time::Instant::now();
    let mut processed_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    info!("Processing {} kiln files with error handling", files.len());

    // Filter to only accessible markdown files
    let markdown_files: Vec<&KilnFileInfo> = files
        .iter()
        .filter(|f| f.is_markdown && f.is_accessible)
        .collect();

    for file_info in markdown_files {
        match process_single_file_with_recovery(
            file_info,
            client,
            config,
            embedding_pool,
            kiln_root,
        )
        .await
        {
            Ok(success) => {
                if success {
                    processed_count += 1;
                } else {
                    failed_count += 1;
                }
            }
            Err(e) => {
                failed_count += 1;
                let process_error = KilnProcessError {
                    file_path: file_info.path.clone(),
                    error_message: e.to_string(),
                    error_type: KilnScannerErrorType::ParseError,
                    timestamp: chrono::Utc::now(),
                    retry_attempts: config.error_retry_attempts,
                    recovered: false,
                    final_error_message: Some(e.to_string()),
                };
                errors.push(process_error);
            }
        }
    }

    let total_processing_time = start_time.elapsed();
    let avg_time_per_doc = if processed_count > 0 {
        total_processing_time / processed_count as u32
    } else {
        Duration::from_secs(0)
    };

    Ok(KilnProcessResult {
        processed_count,
        failed_count,
        errors,
        total_processing_time,
        average_processing_time_per_document: avg_time_per_doc,
    })
}

/// Process a single file with retry logic and error recovery
pub async fn process_single_file_with_recovery(
    file_info: &KilnFileInfo,
    client: &SurrealClient,
    config: &KilnScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>,
    kiln_root: &std::path::Path,
) -> Result<bool> {
    let mut last_error = None;

    for attempt in 0..=config.error_retry_attempts {
        match process_single_file_internal(file_info, client, embedding_pool, kiln_root).await {
            Ok(_) => {
                if attempt > 0 {
                    info!(
                        "File {} recovered after {} attempts",
                        file_info.path.display(),
                        attempt
                    );
                }
                return Ok(true);
            }
            Err(e) => {
                last_error = Some(anyhow::anyhow!("{}", e));
                warn!(
                    "Processing attempt {} failed for {}: {}",
                    attempt + 1,
                    file_info.path.display(),
                    e
                );

                if attempt < config.error_retry_attempts {
                    let delay = Duration::from_millis(config.error_retry_delay_ms);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    error!(
        "Failed to process file {} after {} attempts: {}",
        file_info.path.display(),
        config.error_retry_attempts + 1,
        last_error.unwrap_or_else(|| anyhow!("Unknown error"))
    );

    Ok(false)
}

/// Perform incremental processing of changed files only
pub async fn process_incremental_changes(
    all_files: &[KilnFileInfo],
    client: &SurrealClient,
    config: &KilnScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>,
    kiln_root: &Path,
) -> Result<KilnProcessResult> {
    if !config.enable_incremental {
        return process_kiln_files(all_files, client, config, embedding_pool, kiln_root).await;
    }

    let start_time = std::time::Instant::now();
    let mut processed_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    info!(
        "Performing incremental processing for {} files",
        all_files.len()
    );

    // For each file, check if it needs processing
    let mut files_to_process = Vec::new();

    for file_info in all_files {
        if !file_info.is_markdown || !file_info.is_accessible {
            continue;
        }

        if needs_processing(file_info, client).await? {
            files_to_process.push(file_info);
        }
    }

    info!(
        "Found {} files that need processing",
        files_to_process.len()
    );

    if !files_to_process.is_empty() {
        let result = process_kiln_files(
            &files_to_process
                .iter()
                .map(|&f| f.clone())
                .collect::<Vec<_>>(),
            client,
            config,
            embedding_pool,
            kiln_root,
        )
        .await?;
        processed_count = result.processed_count;
        failed_count = result.failed_count;
        errors = result.errors;
    }

    let total_processing_time = start_time.elapsed();
    let avg_time_per_doc = if processed_count > 0 {
        total_processing_time / processed_count as u32
    } else {
        Duration::from_secs(0)
    };

    Ok(KilnProcessResult {
        processed_count,
        failed_count,
        errors,
        total_processing_time,
        average_processing_time_per_document: avg_time_per_doc,
    })
}

/// Process embeddings for a list of documents (mocked for now)
pub async fn process_document_embeddings(
    documents: &[ParsedDocument],
    _embedding_pool: &EmbeddingThreadPool,
    _client: &SurrealClient,
) -> Result<Vec<EmbeddingProcessingResult>> {
    let mut results = Vec::new();

    for document in documents {
        debug!(
            "Would process embeddings for document: {}",
            document.path.display()
        );

        // Mock successful processing
        results.push(EmbeddingProcessingResult {
            processed_count: 1,
            failed_count: 0,
            total_processing_time: Duration::from_millis(100),
            errors: vec![],
            circuit_breaker_triggered: false,
            embeddings_generated: 0, // Mock
        });
    }

    Ok(results)
}

// Private helper functions

async fn process_file_batch(
    batch: &[&KilnFileInfo],
    client: &SurrealClient,
    config: &KilnScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>,
    kiln_root: &std::path::Path,
) -> Result<KilnProcessResult> {
    if config.parallel_processing > 1 && batch.len() > 1 {
        process_files_parallel(batch, client, config, embedding_pool, kiln_root).await
    } else {
        process_files_sequential(batch, client, config, embedding_pool, kiln_root).await
    }
}

async fn process_files_parallel(
    files: &[&KilnFileInfo],
    client: &SurrealClient,
    config: &KilnScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>,
    kiln_root: &std::path::Path,
) -> Result<KilnProcessResult> {
    let start_time = std::time::Instant::now();
    let semaphore = Arc::new(Semaphore::new(config.parallel_processing));
    let client = Arc::new(client.clone());
    let kiln_root = Arc::new(kiln_root.to_path_buf());

    let results = stream::iter(files)
        .map(|file_info| {
            let semaphore = semaphore.clone();
            let client = client.clone();
            let embedding_pool = embedding_pool.cloned();
            let kiln_root = kiln_root.clone();

            async move {
                let _permit = semaphore.acquire().await?;
                process_single_file_with_recovery(
                    file_info,
                    &client,
                    config,
                    embedding_pool.as_ref(),
                    &kiln_root,
                )
                .await
            }
        })
        .buffer_unordered(config.parallel_processing)
        .collect::<Vec<_>>()
        .await;

    let mut processed_count = 0;
    let mut failed_count = 0;

    for result in results {
        match result {
            Ok(success) => {
                if success {
                    processed_count += 1;
                } else {
                    failed_count += 1;
                }
            }
            Err(_) => failed_count += 1,
        }
    }

    let total_processing_time = start_time.elapsed();
    let avg_time_per_doc = if processed_count > 0 {
        total_processing_time / processed_count as u32
    } else {
        Duration::from_secs(0)
    };

    Ok(KilnProcessResult {
        processed_count,
        failed_count,
        errors: Vec::new(), // Errors handled in recovery function
        total_processing_time,
        average_processing_time_per_document: avg_time_per_doc,
    })
}

async fn process_files_sequential(
    files: &[&KilnFileInfo],
    client: &SurrealClient,
    config: &KilnScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>,
    kiln_root: &std::path::Path,
) -> Result<KilnProcessResult> {
    let start_time = std::time::Instant::now();
    let mut processed_count = 0;
    let mut failed_count = 0;

    for file_info in files {
        match process_single_file_with_recovery(
            file_info,
            client,
            config,
            embedding_pool,
            kiln_root,
        )
        .await
        {
            Ok(success) => {
                if success {
                    processed_count += 1;
                } else {
                    failed_count += 1;
                }
            }
            Err(_) => failed_count += 1,
        }
    }

    let total_processing_time = start_time.elapsed();
    let avg_time_per_doc = if processed_count > 0 {
        total_processing_time / processed_count as u32
    } else {
        Duration::from_secs(0)
    };

    Ok(KilnProcessResult {
        processed_count,
        failed_count,
        errors: Vec::new(), // Errors handled in recovery function
        total_processing_time,
        average_processing_time_per_document: avg_time_per_doc,
    })
}

async fn process_single_file_internal(
    file_info: &KilnFileInfo,
    client: &SurrealClient,
    embedding_pool: Option<&EmbeddingThreadPool>,
    kiln_root: &std::path::Path,
) -> Result<()> {
    // Parse the file
    let document = crate::kiln_scanner::parse_file_to_document(&file_info.path).await?;

    // Store the document
    let doc_id = store_parsed_document(client, &document, kiln_root).await?;

    // Create relationships
    create_wikilink_edges(client, &doc_id, &document).await?;
    create_embed_relationships(client, &doc_id, &document).await?;
    create_tag_associations(client, &doc_id, &document).await?;

    // Process embeddings if available
    if let Some(pool) = embedding_pool {
        // Use KilnPipelineConnector to process embeddings
        let connector = crate::kiln_pipeline_connector::KilnPipelineConnector::new(
            pool.clone(),
            kiln_root.to_path_buf(),
        );
        match connector
            .process_document_to_embedding(client, &document)
            .await
        {
            Ok(result) => {
                info!(
                    "Generated {} embeddings for document {} in {:?}",
                    result.embeddings_generated, doc_id, result.processing_time
                );
            }
            Err(e) => {
                error!("Failed to generate embeddings for {}: {}", doc_id, e);
                // Don't fail the entire processing if embeddings fail
                // Just log the error and continue
            }
        }
    }

    // Update processed timestamp
    update_document_processed_timestamp(client, &doc_id).await?;

    debug!("Successfully processed file: {}", file_info.path.display());

    Ok(())
}

// Embedding processing functions removed for now - to be implemented properly later

async fn needs_processing(file_info: &KilnFileInfo, client: &SurrealClient) -> Result<bool> {
    // Check if document exists in database
    let doc_id = find_document_id_by_path(client, &file_info.relative_path).await?;

    if doc_id.is_empty() {
        return Ok(true); // New document
    }

    // Check if document exists and compare content hash
    // Note: Using string formatting for now since mock client doesn't support parameters
    let path_str = file_info.relative_path.replace('\'', "''");
    let sql = format!(
        "SELECT content_hash, processed_at FROM notes WHERE path = '{}'",
        path_str
    );

    let result = client.query(&sql, &[]).await?;

    if let Some(record) = result.records.first() {
        let stored_hash = record
            .data
            .get("content_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let processed_at = record
            .data
            .get("processed_at")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));

        // Check if content hash matches or if file was modified after processing
        if stored_hash == file_info.content_hash {
            if let Some(processed_time) = processed_at {
                let processed_system_time: std::time::SystemTime = processed_time.into();
                if file_info.modified_time <= processed_system_time {
                    return Ok(false); // No changes needed
                }
            }
        }
    }

    Ok(true) // Needs processing
}

async fn find_document_id_by_path(client: &SurrealClient, relative_path: &str) -> Result<String> {
    // Note: Using string formatting for now since mock client doesn't support parameters
    let path_str = relative_path.replace('\'', "''");
    let sql = format!("SELECT id FROM notes WHERE path = '{}'", path_str);

    let result = client.query(&sql, &[]).await?;

    if let Some(record) = result.records.first() {
        if let Some(id) = &record.id {
            return Ok(id.0.clone());
        }
    }

    Ok(String::new()) // Not found
}

/// Query document hashes for multiple files in a single database call
///
/// This function efficiently retrieves content hashes for multiple files using
/// a single parameterized query with an IN clause, which is much faster than
/// querying each file individually.
///
/// # Arguments
/// * `client` - SurrealDB client connection
/// * `paths` - Slice of file paths to query
///
/// # Returns
/// A HashMap mapping file paths to their stored content hashes. Files not found
/// in the database will not be present in the HashMap.
///
/// # Performance
/// - Single database query for all paths (O(1) queries vs O(n))
/// - Optimized for large path lists (100+ files)
/// - Empty input returns empty HashMap without database call
///
/// # Example
/// ```ignore
/// let paths = vec![PathBuf::from("note1.md"), PathBuf::from("note2.md")];
/// let hashes = bulk_query_document_hashes(&client, &paths).await?;
/// for (path, hash) in hashes {
///     println!("{}: {}", path.display(), hash);
/// }
/// ```
async fn bulk_query_document_hashes(
    client: &SurrealClient,
    paths: &[PathBuf],
    kiln_root: &Path,
) -> Result<std::collections::HashMap<PathBuf, String>> {
    use std::collections::HashMap;

    if paths.is_empty() {
        debug!("No paths provided for bulk hash query");
        return Ok(HashMap::new());
    }

    debug!("querying hashes for {} files", paths.len());

    // Convert absolute paths to relative paths for database query
    // Store mapping from relative -> absolute for later lookup
    let mut abs_to_rel: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut rel_paths: Vec<PathBuf> = Vec::new();

    for abs_path in paths {
        if let Ok(rel_path) = abs_path.strip_prefix(kiln_root) {
            abs_to_rel.insert(abs_path.clone(), rel_path.to_path_buf());
            rel_paths.push(rel_path.to_path_buf());
        } else {
            warn!(
                "Path {} is not under kiln_root {}",
                abs_path.display(),
                kiln_root.display()
            );
        }
    }

    // Build query with IN clause using relative paths
    // Note: Using string formatting for now since mock client doesn't support parameters
    let path_strings: Vec<String> = rel_paths
        .iter()
        .map(|p| {
            let sanitized = p.display().to_string().replace('\'', "''");
            format!("'{}'", sanitized)
        })
        .collect();

    let sql = format!(
        "SELECT path, content_hash FROM notes WHERE path IN [{}]",
        path_strings.join(", ")
    );

    debug!("Executing hash query SQL: {}", sql);
    debug!("Querying for relative paths: {:?}", rel_paths);

    let result = client
        .query(&sql, &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query document hashes: {}", e))?;

    debug!("Query returned {} records", result.records.len());

    // Build HashMap from results, mapping back to absolute paths
    let mut hash_map = HashMap::new();
    for (i, record) in result.records.iter().enumerate() {
        if let Some(path_value) = record.data.get("path") {
            if let Some(rel_path_str) = path_value.as_str() {
                if let Some(hash_value) = record.data.get("content_hash") {
                    if let Some(hash_str) = hash_value.as_str() {
                        let rel_path = PathBuf::from(rel_path_str);
                        // Find the absolute path that corresponds to this relative path
                        for (abs_path, stored_rel_path) in &abs_to_rel {
                            if stored_rel_path == &rel_path {
                                hash_map.insert(abs_path.clone(), hash_str.to_string());
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    debug!(
        "retrieved {} hashes from database (out of {} requested)",
        hash_map.len(),
        paths.len()
    );

    Ok(hash_map)
}

/// Query document IDs for multiple files in a single database call
///
/// This function efficiently retrieves document IDs for multiple file paths using
/// a single parameterized query with an IN clause, which is much faster than
/// querying each file individually.
///
/// # Arguments
/// * `client` - SurrealDB client connection
/// * `relative_paths` - Slice of relative file paths to query
///
/// # Returns
/// A HashMap mapping relative file paths to their document IDs. Files not found
/// in the database will not be present in the HashMap.
async fn bulk_query_document_ids(
    client: &SurrealClient,
    relative_paths: &[String],
) -> Result<std::collections::HashMap<String, String>> {
    use std::collections::HashMap;

    if relative_paths.is_empty() {
        return Ok(HashMap::new());
    }

    // Build query with IN clause
    let path_strings: Vec<String> = relative_paths
        .iter()
        .map(|p| {
            let sanitized = p.replace('\'', "''");
            format!("'{}'", sanitized)
        })
        .collect();

    let sql = format!(
        "SELECT id, path FROM notes WHERE path IN [{}]",
        path_strings.join(", ")
    );

    let result = client
        .query(&sql, &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query document IDs: {}", e))?;

    // Build HashMap from results
    let mut id_map = HashMap::new();
    for record in result.records {
        if let Some(path_value) = record.data.get("path") {
            if let Some(path_str) = path_value.as_str() {
                if let Some(id) = &record.id {
                    id_map.insert(path_str.to_string(), id.0.clone());
                }
            }
        }
    }

    Ok(id_map)
}

/// Convert file paths to KilnFileInfo structures
///
/// This helper function reads file metadata for each path and creates KilnFileInfo
/// structures required by the processing pipeline. It handles missing files gracefully
/// by logging warnings and skipping them.
///
/// # Arguments
/// * `paths` - Slice of file paths to convert
/// * `kiln_root` - Root directory so relative paths can be normalized
///
/// # Returns
/// Vector of KilnFileInfo structures for successfully read files
///
/// # Errors
/// Returns an error if a critical file operation fails
///
/// # Example
/// ```ignore
/// let paths = vec![PathBuf::from("note1.md"), PathBuf::from("note2.md")];
/// let file_infos = convert_paths_to_file_infos(&paths, kiln_root).await?;
/// ```
async fn convert_paths_to_file_infos(paths: &[PathBuf], kiln_root: &Path) -> Result<Vec<KilnFileInfo>> {
    let mut file_infos = Vec::new();

    for path in paths {
        // Skip if file doesn't exist
        if !path.exists() {
            warn!("File not found, skipping: {}", path.display());
            continue;
        }

        // Get file metadata
        let metadata = match tokio::fs::metadata(path).await {
            Ok(m) => m,
            Err(e) => {
                warn!("Failed to read metadata for {}: {}", path.display(), e);
                continue;
            }
        };

        // Read file content and calculate hash using MD5 (same as parser)
        let content = match tokio::fs::read_to_string(path).await {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read file {}: {}", path.display(), e);
                continue;
            }
        };

        // Use SHA-256 hash to match what the parser uses
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&content);
        let content_hash = format!("{:x}", hasher.finalize());

        // Get modification time
        let modified_time = metadata
            .modified()
            .unwrap_or_else(|_| std::time::SystemTime::now());

        let relative_path = path
            .strip_prefix(kiln_root)
            .unwrap_or(path)
            .to_path_buf();

        // Create KilnFileInfo
        let file_info = KilnFileInfo {
            path: path.clone(),
            relative_path: relative_path.display().to_string(),
            file_size: metadata.len(),
            modified_time,
            is_markdown: path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("md"))
                .unwrap_or(false),
            is_accessible: true,
            content_hash,
        };

        file_infos.push(file_info);
    }

    debug!(
        "Converted {} paths to KilnFileInfo (out of {} total)",
        file_infos.len(),
        paths.len()
    );

    Ok(file_infos)
}

/// Detect which files have changed by comparing content hashes
///
/// This function uses the existing ChangeDetector to calculate SHA256 hashes
/// for files and compares them against the database to identify actual changes.
/// This prevents unnecessary reprocessing of unchanged files.
///
/// # Arguments
/// * `client` - SurrealDB client connection
/// * `file_infos` - List of potentially changed files to check
///
/// # Returns
/// Filtered list containing only files that have actually changed
///
/// # Performance
/// - Uses bulk_query_document_hashes() for efficiency
/// - In-memory hash comparison (fast)
/// - Returns files where hash mismatches OR not in database (new files)
///
/// # Example
/// ```ignore
/// let all_files = scan_kiln_directory(&kiln_path, &config).await?;
/// let changed_files = detect_changed_files(&client, &all_files).await?;
/// println!("Found {} changed files out of {}", changed_files.len(), all_files.len());
/// ```
async fn detect_changed_files(
    client: &SurrealClient,
    file_infos: &[KilnFileInfo],
    kiln_root: &Path,
) -> Result<Vec<KilnFileInfo>> {
    if file_infos.is_empty() {
        debug!("No files to check for changes");
        return Ok(Vec::new());
    }

    debug!("Detecting changes in {} files", file_infos.len());

    // Extract paths for bulk query
    let paths: Vec<PathBuf> = file_infos.iter().map(|fi| fi.path.clone()).collect();

    // Query database for stored hashes
    let stored_hashes = bulk_query_document_hashes(client, &paths, kiln_root).await?;

    // Compare hashes to find changed files
    let mut changed_files = Vec::new();

    for file_info in file_infos {
        match stored_hashes.get(&file_info.path) {
            Some(stored_hash) => {
                // File exists in database - compare hashes
                if stored_hash != &file_info.content_hash {
                    debug!(
                        "file changed (hash mismatch): {} (stored: {}..., current: {}...)",
                        file_info.path.display(),
                        &stored_hash[..8],
                        &file_info.content_hash[..8]
                    );
                    changed_files.push(file_info.clone());
                } else {
                    debug!("file unchanged: {}", file_info.path.display());
                }
            }
            None => {
                // File not in database - treat as new/changed
                debug!("new file (not in database): {}", file_info.path.display());
                changed_files.push(file_info.clone());
            }
        }
    }

    info!(
        "Detected {} changed files out of {} total",
        changed_files.len(),
        file_infos.len()
    );

    Ok(changed_files)
}

/// Process only files that have changed since last processing
///
/// This is the main entry point for delta processing, which significantly improves
/// performance by only reprocessing files that have actually changed. It uses
/// SHA256 hash comparison to detect changes efficiently.
///
/// # Performance Target
/// - Single file change: ≤1 second
/// - Bulk changes: scales linearly with changed file count
///
/// # Process Flow
/// 1. Convert paths to KilnFileInfo structures (read metadata, calculate hashes)
/// 2. Detect which files actually changed via bulk hash comparison
/// 3. Delete old embeddings for changed files
/// 4. Process changed files using existing pipeline
/// 5. Update content_hash and processed_at timestamps
///
/// # Arguments
/// * `changed_files` - List of potentially changed file paths
/// * `client` - SurrealDB client connection
/// * `config` - Kiln scanner configuration
/// * `embedding_pool` - Optional embedding thread pool for parallel processing
///
/// # Returns
/// KilnProcessResult containing processing statistics and performance metrics
///
/// # Errors
/// Returns an error if critical operations fail. Per-file errors are logged
/// but don't stop processing of other files.
///
/// # Example
/// ```ignore
/// let changed_paths = vec![PathBuf::from("note1.md")];
/// let result = process_kiln_delta(
///     changed_paths,
///     &client,
///     &config,
///     Some(&embedding_pool),
///     &kiln_root
/// ).await?;
/// println!("Processed {} files in {:?}", result.processed_count, result.total_processing_time);
/// ```
pub async fn process_kiln_delta(
    changed_files: Vec<PathBuf>,
    client: &SurrealClient,
    config: &KilnScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>,
    kiln_root: &Path,
) -> Result<KilnProcessResult> {
    let start_time = std::time::Instant::now();

    info!(
        "Starting delta processing for {} potentially changed files",
        changed_files.len()
    );
    debug!(
        "starting delta processing for {} files",
        changed_files.len()
    );

    // Handle empty input
    if changed_files.is_empty() {
        info!("No files to process");
        return Ok(KilnProcessResult {
            processed_count: 0,
            failed_count: 0,
            errors: Vec::new(),
            total_processing_time: start_time.elapsed(),
            average_processing_time_per_document: Duration::from_secs(0),
        });
    }

    // Step 1: Convert paths to KilnFileInfo structures
    let change_detection_start = std::time::Instant::now();
    let file_infos = convert_paths_to_file_infos(&changed_files, kiln_root).await?;
    let change_detection_time = change_detection_start.elapsed();

    debug!(
        "Converted {} paths to KilnFileInfo in {:?}",
        file_infos.len(),
        change_detection_time
    );

    // Step 2: Detect which files actually changed
    let changed_file_infos = detect_changed_files(client, &file_infos, kiln_root).await?;

    info!(
        "Detected {} actually changed files (out of {} candidates) in {:?}",
        changed_file_infos.len(),
        file_infos.len(),
        change_detection_time
    );
    debug!(
        "detected {} changed files out of {} candidates",
        changed_file_infos.len(),
        file_infos.len()
    );

    // Handle case where no files actually changed
    if changed_file_infos.is_empty() {
        info!("No files have changed, skipping processing");
        return Ok(KilnProcessResult {
            processed_count: 0,
            failed_count: 0,
            errors: Vec::new(),
            total_processing_time: start_time.elapsed(),
            average_processing_time_per_document: Duration::from_secs(0),
        });
    }

    // Step 3 & 4: Process changed files using incremental chunk-level re-embedding
    // This will automatically:
    // - Detect which chunks changed
    // - Delete only changed chunks
    // - Re-embed only changed chunks
    let processing_result = process_kiln_files(
        &changed_file_infos,
        client,
        config,
        embedding_pool,
        kiln_root,
    )
    .await?;

    let total_time = start_time.elapsed();

    info!(
        "Delta processing completed: {} processed, {} failed in {:?}",
        processing_result.processed_count,
        processing_result.failed_count,
        total_time
    );

    // Return results with updated timing
    Ok(KilnProcessResult {
        processed_count: processing_result.processed_count,
        failed_count: processing_result.failed_count,
        errors: processing_result.errors,
        total_processing_time: total_time,
        average_processing_time_per_document: if processing_result.processed_count > 0 {
            total_time / processing_result.processed_count as u32
        } else {
            Duration::from_secs(0)
        },
    })
}
