//! Vault Processor Module
//!
//! This module provides the processing pipeline for vault files, integrating with
//! the parser system and embedding infrastructure. It handles batch processing,
//! parallel execution, and comprehensive error recovery.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use anyhow::{Result, anyhow};
use tokio::sync::Semaphore;
use futures::stream::{self, StreamExt};
use tracing::{debug, info, warn, error};

use crate::SurrealClient;
use crate::embedding_pool::EmbeddingThreadPool;
use crate::embedding_config::{DocumentEmbedding, EmbeddingProcessingResult, EmbeddingError, RetryProcessingResult};
use crate::vault_scanner::{VaultFileInfo, VaultProcessResult, VaultProcessError, VaultScannerErrorType, VaultScannerConfig};
use crate::vault_integration::*;
use crucible_core::parser::ParsedDocument;

/// Scan a vault directory recursively and return discovered files
pub async fn scan_vault_directory(
    vault_path: &PathBuf,
    config: &VaultScannerConfig
) -> Result<Vec<VaultFileInfo>> {
    use crate::vault_scanner::VaultScanner;

    let mut scanner = crate::vault_scanner::create_vault_scanner(config.clone()).await?;
    let scan_result = scanner.scan_vault_directory(vault_path).await?;

    Ok(scan_result.discovered_files)
}

/// Process a collection of vault files with full pipeline integration
pub async fn process_vault_files(
    files: &[VaultFileInfo],
    client: &SurrealClient,
    config: &VaultScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>
) -> Result<VaultProcessResult> {
    let start_time = std::time::Instant::now();
    let mut processed_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    info!("Processing {} vault files", files.len());

    // Filter to only accessible markdown files
    let markdown_files: Vec<&VaultFileInfo> = files.iter()
        .filter(|f| f.is_markdown && f.is_accessible)
        .collect();

    info!("Found {} markdown files to process", markdown_files.len());

    if config.batch_processing && markdown_files.len() > config.batch_size {
        // Process in batches
        let batches: Vec<Vec<&VaultFileInfo>> = markdown_files
            .chunks(config.batch_size)
            .map(|chunk| chunk.to_vec())
            .collect();

        debug!("Processing {} batches with max size {}", batches.len(), config.batch_size);

        for (batch_index, batch) in batches.iter().enumerate() {
            debug!("Processing batch {} with {} files", batch_index + 1, batch.len());

            let batch_result = process_file_batch(batch, client, config, embedding_pool).await?;

            processed_count += batch_result.processed_count;
            failed_count += batch_result.failed_count;
            errors.extend(batch_result.errors);

            debug!("Batch {} completed: {} processed, {} failed",
                   batch_index + 1, batch_result.processed_count, batch_result.failed_count);
        }
    } else {
        // Process all files at once or in parallel
        if config.parallel_processing > 1 && markdown_files.len() > 1 {
            let parallel_result = process_files_parallel(&markdown_files, client, config, embedding_pool).await?;
            processed_count = parallel_result.processed_count;
            failed_count = parallel_result.failed_count;
            errors = parallel_result.errors;
        } else {
            let sequential_result = process_files_sequential(&markdown_files, client, config, embedding_pool).await?;
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

    info!("Processing completed: {} successful, {} failed in {:?}",
          processed_count, failed_count, total_processing_time);

    Ok(VaultProcessResult {
        processed_count,
        failed_count,
        errors,
        total_processing_time,
        average_processing_time_per_document: avg_time_per_doc,
    })
}

/// Process files with comprehensive error handling and recovery
pub async fn process_vault_files_with_error_handling(
    files: &[VaultFileInfo],
    client: &SurrealClient,
    config: &VaultScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>
) -> Result<VaultProcessResult> {
    let start_time = std::time::Instant::now();
    let mut processed_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    info!("Processing {} vault files with error handling", files.len());

    // Filter to only accessible markdown files
    let markdown_files: Vec<&VaultFileInfo> = files.iter()
        .filter(|f| f.is_markdown && f.is_accessible)
        .collect();

    for file_info in markdown_files {
        match process_single_file_with_recovery(file_info, client, config, embedding_pool).await {
            Ok(success) => {
                if success {
                    processed_count += 1;
                } else {
                    failed_count += 1;
                }
            }
            Err(e) => {
                failed_count += 1;
                let process_error = VaultProcessError {
                    file_path: file_info.path.clone(),
                    error_message: e.to_string(),
                    error_type: VaultScannerErrorType::ParseError,
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

    Ok(VaultProcessResult {
        processed_count,
        failed_count,
        errors,
        total_processing_time,
        average_processing_time_per_document: avg_time_per_doc,
    })
}

/// Process a single file with retry logic and error recovery
pub async fn process_single_file_with_recovery(
    file_info: &VaultFileInfo,
    client: &SurrealClient,
    config: &VaultScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>
) -> Result<bool> {
    let mut last_error = None;

    for attempt in 0..=config.error_retry_attempts {
        match process_single_file_internal(file_info, client, embedding_pool).await {
            Ok(_) => {
                if attempt > 0 {
                    info!("File {} recovered after {} attempts",
                          file_info.path.display(), attempt);
                }
                return Ok(true);
            }
            Err(e) => {
                last_error = Some(anyhow::anyhow!("{}", e));
                warn!("Processing attempt {} failed for {}: {}",
                      attempt + 1, file_info.path.display(), e);

                if attempt < config.error_retry_attempts {
                    let delay = Duration::from_millis(config.error_retry_delay_ms);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    error!("Failed to process file {} after {} attempts: {}",
           file_info.path.display(), config.error_retry_attempts + 1,
           last_error.unwrap_or_else(|| anyhow!("Unknown error")));

    Ok(false)
}

/// Perform incremental processing of changed files only
pub async fn process_incremental_changes(
    all_files: &[VaultFileInfo],
    client: &SurrealClient,
    config: &VaultScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>
) -> Result<VaultProcessResult> {
    if !config.enable_incremental {
        return process_vault_files(all_files, client, config, embedding_pool).await;
    }

    let start_time = std::time::Instant::now();
    let mut processed_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    info!("Performing incremental processing for {} files", all_files.len());

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

    info!("Found {} files that need processing", files_to_process.len());

    if !files_to_process.is_empty() {
        let result = process_vault_files(&files_to_process.iter().map(|&f| f.clone()).collect::<Vec<_>>(),
                                       client, config, embedding_pool).await?;
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

    Ok(VaultProcessResult {
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
    _client: &SurrealClient
) -> Result<Vec<EmbeddingProcessingResult>> {
    let mut results = Vec::new();

    for document in documents {
        debug!("Would process embeddings for document: {}", document.path.display());

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
    batch: &[&VaultFileInfo],
    client: &SurrealClient,
    config: &VaultScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>
) -> Result<VaultProcessResult> {
    if config.parallel_processing > 1 && batch.len() > 1 {
        process_files_parallel(batch, client, config, embedding_pool).await
    } else {
        process_files_sequential(batch, client, config, embedding_pool).await
    }
}

async fn process_files_parallel(
    files: &[&VaultFileInfo],
    client: &SurrealClient,
    config: &VaultScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>
) -> Result<VaultProcessResult> {
    let start_time = std::time::Instant::now();
    let semaphore = Arc::new(Semaphore::new(config.parallel_processing));
    let client = Arc::new(client.clone());

    let results = stream::iter(files)
        .map(|file_info| {
            let semaphore = semaphore.clone();
            let client = client.clone();
            let embedding_pool = embedding_pool.cloned();

            async move {
                let _permit = semaphore.acquire().await?;
                process_single_file_with_recovery(file_info, &client, config, embedding_pool.as_ref()).await
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

    Ok(VaultProcessResult {
        processed_count,
        failed_count,
        errors: Vec::new(), // Errors handled in recovery function
        total_processing_time,
        average_processing_time_per_document: avg_time_per_doc,
    })
}

async fn process_files_sequential(
    files: &[&VaultFileInfo],
    client: &SurrealClient,
    config: &VaultScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>
) -> Result<VaultProcessResult> {
    let start_time = std::time::Instant::now();
    let mut processed_count = 0;
    let mut failed_count = 0;

    for file_info in files {
        match process_single_file_with_recovery(file_info, client, config, embedding_pool).await {
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

    Ok(VaultProcessResult {
        processed_count,
        failed_count,
        errors: Vec::new(), // Errors handled in recovery function
        total_processing_time,
        average_processing_time_per_document: avg_time_per_doc,
    })
}

async fn process_single_file_internal(
    file_info: &VaultFileInfo,
    client: &SurrealClient,
    embedding_pool: Option<&EmbeddingThreadPool>
) -> Result<()> {
    // Parse the file
    let document = crate::vault_scanner::parse_file_to_document(&file_info.path).await?;

    // Store the document
    let doc_id = store_parsed_document(client, &document).await?;

    // Create relationships
    create_wikilink_edges(client, &doc_id, &document).await?;
    create_embed_relationships(client, &doc_id, &document).await?;
    create_tag_associations(client, &doc_id, &document).await?;

    // Process embeddings if available (mocked for now)
    if let Some(_pool) = embedding_pool {
        debug!("Would process embeddings for document: {}", doc_id);
        // TODO: Implement actual embedding processing
    }

    // Update processed timestamp
    update_document_processed_timestamp(client, &doc_id).await?;

    debug!("Successfully processed file: {}", file_info.path.display());

    Ok(())
}

// Embedding processing functions removed for now - to be implemented properly later

async fn needs_processing(file_info: &VaultFileInfo, client: &SurrealClient) -> Result<bool> {
    // Check if document exists in database
    let doc_id = find_document_id_by_path(client, &file_info.path).await?;

    if doc_id.is_empty() {
        return Ok(true); // New document
    }

    // Check if document exists and compare content hash
    let sql = format!("SELECT content_hash, processed_at FROM notes WHERE path = '{}'",
                     file_info.path.display());
    let result = client.query(&sql, &[]).await?;

    if let Some(record) = result.records.first() {
        let stored_hash = record.data.get("content_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let processed_at = record.data.get("processed_at")
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

async fn find_document_id_by_path(client: &SurrealClient, path: &PathBuf) -> Result<String> {
    let sql = format!("SELECT id FROM notes WHERE path = '{}'", path.display());
    let result = client.query(&sql, &[]).await?;

    if let Some(record) = result.records.first() {
        if let Some(id) = &record.id {
            return Ok(id.0.clone());
        }
    }

    Ok(String::new()) // Not found
}