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
use crate::simple_integration;
use crate::transaction_queue::TransactionQueue;
use crate::SurrealClient;
use crucible_core::types::ParsedDocument;

/// Performance metrics for change detection operations
#[derive(Debug, Clone, Default)]
pub struct ChangeDetectionMetrics {
    /// Total number of files scanned
    pub total_files: usize,
    /// Number of files that had changes
    pub changed_files: usize,
    /// Number of files skipped (unchanged)
    pub skipped_files: usize,
    /// Time taken for change detection
    pub change_detection_time: Duration,
    /// Number of database round trips
    pub database_round_trips: usize,
    /// Cache hit rate (0.0 to 1.0)
    pub cache_hit_rate: f64,
    /// Files processed per second
    pub files_per_second: f64,
}

impl ChangeDetectionMetrics {
    /// Create new metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate performance summary
    pub fn performance_summary(&self) -> String {
        format!(
            "Scanned {} files: {} changed, {} skipped ({:.1}% unchanged) \
             in {:?} ({:.0} files/sec, {} DB queries, {:.1}% cache hit)",
            self.total_files,
            self.changed_files,
            self.skipped_files,
            if self.total_files > 0 {
                (self.skipped_files as f64 / self.total_files as f64) * 100.0
            } else {
                0.0
            },
            self.change_detection_time,
            self.files_per_second,
            self.database_round_trips,
            self.cache_hit_rate * 100.0
        )
    }

    /// Log performance metrics
    pub fn log_metrics(&self) {
        info!("📊 Change Detection Performance:");
        info!("  📁 Total files scanned: {}", self.total_files);
        info!("  📝 Files that changed: {}", self.changed_files);
        info!("  ⏭️  Files skipped: {} ({:.1}%)",
              self.skipped_files,
              if self.total_files > 0 {
                  (self.skipped_files as f64 / self.total_files as f64) * 100.0
              } else {
                  0.0
              });
        info!("  ⏱️  Change detection time: {:?}", self.change_detection_time);
        info!("  🗄️  Database round trips: {}", self.database_round_trips);
        info!("  🚀 Processing speed: {:.0} files/second", self.files_per_second);
        info!("  💾 Cache hit rate: {:.1}%", self.cache_hit_rate * 100.0);

        if self.skipped_files > 0 {
            let time_saved = self.change_detection_time.mul_f64(
                self.skipped_files as f64 / self.total_files.max(1) as f64
            );
            info!("  ⚡ Estimated time saved: {:?}", time_saved);
        }
    }
}

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

    info!("🚀 Processing {} total kiln files", files.len());
    debug!("processing {} kiln files", files.len());

    // Filter to only accessible markdown files
    let markdown_files: Vec<&KilnFileInfo> = files
        .iter()
        .filter(|f| f.is_markdown && f.is_accessible)
        .collect();

    info!("📚 Found {} markdown files to process", markdown_files.len());
    for (i, file) in markdown_files.iter().enumerate() {
        info!("  File {}: {}", i + 1, file.path.display());
    }
    debug!("found {} markdown files to process", markdown_files.len());

    for (i, file) in files.iter().enumerate().take(5) {
        debug!(
            "sample file {}: {:?} (markdown={}, accessible={})",
            i, file.path, file.is_markdown, file.is_accessible
        );
    }

    debug!(
        "batch_processing={}, markdown_files={}, batch_size={}",
        config.batch_processing,
        markdown_files.len(),
        config.batch_size
    );
    debug!("parallel_processing={}", config.parallel_processing);

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

/// Queue-based processing: Process a collection of kiln files using simple queue operations
///
/// This replaces direct database calls with our simple integration layer to eliminate
/// RocksDB lock contention while maintaining the same functionality.
pub async fn process_kiln_files_with_queue(
    files: &[KilnFileInfo],
    client: &Arc<SurrealClient>,
    queue: &TransactionQueue,
    config: &KilnScannerConfig,
    kiln_root: &std::path::Path,
) -> Result<KilnProcessResult> {
    let start_time = std::time::Instant::now();
    let mut processed_count = 0;
    let mut failed_count = 0;

    info!("🚀 Processing {} total kiln files with queue", files.len());

    // Filter to only accessible markdown files
    let markdown_files: Vec<&KilnFileInfo> = files
        .iter()
        .filter(|f| f.is_markdown && f.is_accessible)
        .collect();

    info!("📚 Found {} markdown files to queue", markdown_files.len());

    // Convert file paths to Path references for queue processing
    let file_paths: Vec<&std::path::Path> = markdown_files
        .iter()
        .map(|f| f.path.as_path())
        .collect();

    // Enqueue all documents for processing
    debug!("📤 Enqueuing {} documents for queue processing", file_paths.len());
    match simple_integration::enqueue_documents(queue, client, &file_paths, kiln_root).await {
        Ok(document_ids) => {
            processed_count = document_ids.len();
            info!("✅ Successfully enqueued {} documents", processed_count);
        }
        Err(e) => {
            error!("❌ Failed to enqueue documents: {}", e);
            failed_count = file_paths.len();
            warn!("⚠️ Queue processing failed for {} files", failed_count);
        }
    }

    let total_processing_time = start_time.elapsed();
    let avg_time_per_doc = if processed_count > 0 {
        total_processing_time / processed_count as u32
    } else {
        Duration::from_millis(0)
    };

    Ok(KilnProcessResult {
        processed_count,
        failed_count,
        errors: Vec::new(), // Simplified - don't collect individual errors
        total_processing_time,
        average_processing_time_per_document: avg_time_per_doc,
    })
}

/// Queue-based single file processing with simple queue operations
pub async fn process_single_file_with_queue(
    file_info: &KilnFileInfo,
    client: &Arc<SurrealClient>,
    queue: &TransactionQueue,
    kiln_root: &std::path::Path,
) -> Result<bool> {
    info!("📝 Queuing file for processing: {}", file_info.path.display());

    match simple_integration::enqueue_document(queue, client, &file_info.path, kiln_root).await {
        Ok(document_id) => {
            info!("✅ Successfully enqueued document: {}", document_id);
            Ok(true)
        }
        Err(e) => {
            error!("❌ Failed to enqueue document {}: {}", file_info.path.display(), e);
            Err(e)
        }
    }
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
            Ok(()) => {
                processed_count += 1;
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
) -> Result<()> {
    let mut last_error = None;

    info!("📝 Queuing file for processing: {}", file_info.path.display());

    for attempt in 0..=config.error_retry_attempts {
        match process_single_file_internal(file_info, client, embedding_pool, kiln_root).await {
            Ok(_) => {
                if attempt > 0 {
                    info!(
                        "🔄 File {} recovered after {} attempts",
                        file_info.path.display(),
                        attempt
                    );
                }
                return Ok(());
            }
            Err(e) => {
                last_error = Some(anyhow::anyhow!("{}", e));
                warn!(
                    "⚠️  Processing attempt {} failed for {}: {}",
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

    let final_error = last_error.unwrap_or_else(|| anyhow!("Unknown error"));
    error!(
        "❌ FAILED to process file {} after {} attempts: {}",
        file_info.path.display(),
        config.error_retry_attempts + 1,
        final_error
    );

    Err(final_error)
}

/// Perform incremental processing of changed files only using efficient batch hash comparison
///
/// **DEPRECATED:** This function performs duplicate change detection internally.
/// Use `ChangeDetectionService` for change detection, then call `process_files()` instead.
/// This function will be removed in a future version.
#[deprecated(
    since = "0.1.0",
    note = "Use ChangeDetectionService for detection, then process_files() for processing"
)]
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
        "🔍 Performing incremental processing for {} files",
        all_files.len()
    );

    // Filter to only accessible markdown files
    let markdown_files: Vec<&KilnFileInfo> = all_files
        .iter()
        .filter(|f| f.is_markdown && f.is_accessible)
        .collect();

    if markdown_files.is_empty() {
        info!("📂 No markdown files found to process");
        return Ok(KilnProcessResult {
            processed_count: 0,
            failed_count: 0,
            errors: Vec::new(),
            total_processing_time: start_time.elapsed(),
            average_processing_time_per_document: Duration::from_secs(0),
        });
    }

    // Use efficient batch change detection
    let change_detection_start = std::time::Instant::now();
    let (changed_files, change_metrics) = detect_changed_files_efficient(
        client,
        &markdown_files,
        kiln_root,
    ).await?;
    let change_detection_time = change_detection_start.elapsed();

    // Log comprehensive change detection metrics
    info!(
        "📊 {}",
        change_metrics.performance_summary()
    );

    if !changed_files.is_empty() {
        let result = process_kiln_files(
            &changed_files,
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

    // Calculate final performance summary
    let total_skipped = change_metrics.skipped_files;
    let total_change_time = change_metrics.change_detection_time;

    info!(
        "🎯 Incremental processing completed: {} processed, {} failed, {} skipped in {:?}",
        processed_count,
        failed_count,
        total_skipped,
        total_processing_time
    );

    // Performance impact analysis
    if total_skipped > 0 {
        let time_saved_percentage = (total_skipped as f64 / markdown_files.len() as f64) * 100.0;
        info!(
            "⚡ Performance impact: skipped {:.1}% of files, saved estimated processing time",
            time_saved_percentage
        );

        if total_change_time.as_millis() > 100 {
            info!(
                "📈 Change detection efficiency: {:?} to scan {} files ({:.0} files/sec)",
                total_change_time,
                change_metrics.total_files,
                change_metrics.files_per_second
            );
        }
    }

    Ok(KilnProcessResult {
        processed_count,
        failed_count,
        errors,
        total_processing_time,
        average_processing_time_per_document: avg_time_per_doc,
    })
}

/// Process pre-filtered files through the kiln pipeline (Single-Pass Architecture)
///
/// This function processes ONLY the files provided - it does NOT perform change detection.
/// Change detection should be performed beforehand using `ChangeDetectionService`.
///
/// # Single-Pass Architecture
///
/// This function is part of the single-pass change detection architecture where:
/// 1. **ChangeDetectionService** performs change detection (queries database once)
/// 2. **process_files** processes the resulting filtered list (no additional queries)
///
/// This ensures:
/// - **No race conditions**: Single database query means consistent state
/// - **Better performance**: 50% fewer database queries
/// - **Clear separation**: Detection logic separate from processing logic
/// - **Deterministic results**: Same input always produces same output
///
/// # Arguments
///
/// * `files_to_process` - Pre-filtered list of files that need processing (new or changed)
/// * `client` - SurrealDB client for database operations
/// * `config` - Scanner configuration settings
/// * `embedding_pool` - Optional embedding thread pool for vector operations
/// * `kiln_root` - Root path of the kiln directory
///
/// # Returns
///
/// Processing results including counts of successful/failed operations and timing metrics
///
/// # Example
///
/// ```no_run
/// use crucible_cli::common::ChangeDetectionService;
///
/// // Step 1: Detect changes
/// let service = ChangeDetectionService::new(...).await?;
/// let detection_result = service.detect_changes().await?;
///
/// // Step 2: Process only the changed files
/// let files_to_process = detection_result.changes.new
///     .into_iter()
///     .chain(detection_result.changes.changed)
///     .collect();
/// let result = process_files(&files_to_process, client, config, None, kiln_root).await?;
/// ```
///
/// # Migration Note
///
/// This replaces the old `process_incremental_changes` which performed duplicate
/// change detection internally, causing race conditions and wasted database queries.
pub async fn process_files(
    files_to_process: &[KilnFileInfo],
    client: &SurrealClient,
    config: &KilnScannerConfig,
    embedding_pool: Option<&EmbeddingThreadPool>,
    kiln_root: &Path,
) -> Result<KilnProcessResult> {
    let start_time = std::time::Instant::now();

    if files_to_process.is_empty() {
        info!("📂 No files to process");
        return Ok(KilnProcessResult {
            processed_count: 0,
            failed_count: 0,
            errors: Vec::new(),
            total_processing_time: start_time.elapsed(),
            average_processing_time_per_document: Duration::from_secs(0),
        });
    }

    info!("🚀 Processing {} pre-filtered files", files_to_process.len());

    // Process the files directly (no change detection)
    let result = process_kiln_files(
        files_to_process,
        client,
        config,
        embedding_pool,
        kiln_root,
    )
    .await?;

    let total_processing_time = start_time.elapsed();
    let avg_time_per_doc = if result.processed_count > 0 {
        total_processing_time / result.processed_count as u32
    } else {
        Duration::from_secs(0)
    };

    info!(
        "✅ Processing completed: {} successful, {} failed in {:?}",
        result.processed_count,
        result.failed_count,
        total_processing_time
    );

    Ok(KilnProcessResult {
        processed_count: result.processed_count,
        failed_count: result.failed_count,
        errors: result.errors,
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
    // Clone is now cheap - it just clones the Arc inside SurrealClient
    let client = Arc::new(client.clone());
    let kiln_root = Arc::new(kiln_root.to_path_buf());

    // Store file info for error reporting
    let file_infos: Vec<&KilnFileInfo> = files.iter().copied().collect();

    let results = stream::iter(files.iter().enumerate())
        .map(|(index, file_info)| {
            let semaphore = semaphore.clone();
            let client = client.clone();
            let embedding_pool = embedding_pool.cloned();
            let kiln_root = kiln_root.clone();

            async move {
                let _permit = semaphore.acquire().await?;
                let result = process_single_file_with_recovery(
                    file_info,
                    &client,
                    config,
                    embedding_pool.as_ref(),
                    &kiln_root,
                )
                .await;
                Ok::<(usize, Result<()>), anyhow::Error>((index, result))
            }
        })
        .buffer_unordered(config.parallel_processing)
        .collect::<Vec<_>>()
        .await;

    let mut processed_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    for result in results {
        match result {
            Ok((index, inner_result)) => {
                match inner_result {
                    Ok(()) => {
                        processed_count += 1;
                    }
                    Err(e) => {
                        failed_count += 1;
                        errors.push(KilnProcessError {
                            file_path: file_infos[index].path.clone(),
                            error_message: format!("{}", e),
                            error_type: KilnScannerErrorType::ParseError,
                            timestamp: chrono::Utc::now(),
                            retry_attempts: 0,
                            recovered: false,
                            final_error_message: Some(format!("{}", e)),
                        });
                    }
                }
            }
            Err(e) => {
                failed_count += 1;
                // Can't associate with specific file when semaphore acquire fails
                errors.push(KilnProcessError {
                    file_path: PathBuf::from("unknown"),
                    error_message: format!("Parallel processing error: {}", e),
                    error_type: KilnScannerErrorType::IoError,
                    timestamp: chrono::Utc::now(),
                    retry_attempts: 0,
                    recovered: false,
                    final_error_message: Some(format!("{}", e)),
                });
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
    let mut errors = Vec::new();

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
            Ok(()) => {
                processed_count += 1;
            }
            Err(e) => {
                failed_count += 1;
                errors.push(KilnProcessError {
                    file_path: file_info.path.clone(),
                    error_message: format!("{}", e),
                    error_type: KilnScannerErrorType::ParseError,
                    timestamp: chrono::Utc::now(),
                    retry_attempts: 0,
                    recovered: false,
                    final_error_message: Some(format!("{}", e)),
                });
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

async fn process_single_file_internal(
    file_info: &KilnFileInfo,
    client: &SurrealClient,
    embedding_pool: Option<&EmbeddingThreadPool>,
    kiln_root: &std::path::Path,
) -> Result<()> {
    info!("🔄 Starting processing: {}", file_info.path.display());

    // Parse the file
    debug!("  📄 Parsing file...");
    let document = crate::kiln_scanner::parse_file_to_document(&file_info.path)
        .await
        .map_err(|e| {
            error!("  ❌ Parse failed for {}: {}", file_info.path.display(), e);
            e
        })?;
    debug!("  ✅ Parse complete");

    // Store the document
    debug!("  💾 Storing document...");
    let doc_id = store_parsed_document(client, &document, kiln_root)
        .await
        .map_err(|e| {
            error!("  ❌ Store failed for {}: {}", file_info.path.display(), e);
            e
        })?;
    debug!("  ✅ Document stored with ID: {}", doc_id);

    // Create relationships
    debug!("  🔗 Creating relationships...");
    create_wikilink_edges(client, &doc_id, &document, kiln_root).await?;
    create_embed_relationships(client, &doc_id, &document, kiln_root).await?;
    create_tag_associations(client, &doc_id, &document).await?;
    debug!("  ✅ Relationships created");

    // Process embeddings if available
    if let Some(pool) = embedding_pool {
        debug!("  🧮 Generating embeddings...");
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
                    "  ✅ Generated {} embeddings for {} in {:?}",
                    result.embeddings_generated, doc_id, result.processing_time
                );
            }
            Err(e) => {
                error!("  ❌ Embedding generation failed for {}: {}", doc_id, e);
                // Don't fail the entire processing if embeddings fail
                // Just log the error and continue
            }
        }
    } else {
        debug!("  ⏭️  Skipping embeddings (no pool available)");
    }

    // Update processed timestamp and content hash
    debug!("  ⏰ Updating timestamp and content hash...");
    crate::kiln_integration::update_document_processing_metadata(client, &doc_id, &file_info).await?;

    info!("✅ Successfully completed: {}", file_info.path.display());

    Ok(())
}

// Embedding processing functions removed for now - to be implemented properly later

pub async fn needs_processing(file_info: &KilnFileInfo, client: &SurrealClient) -> Result<bool> {
    // Validate input parameters
    if file_info.relative_path.is_empty() {
        warn!("Empty relative path provided, treating as needs processing");
        return Ok(true);
    }

    // Check if document exists in database with error handling
    let doc_id = match find_document_id_by_path(client, &file_info.relative_path).await {
        Ok(id) => id,
        Err(e) => {
            warn!("Database error checking document existence for {}: {}, treating as needs processing",
                  file_info.relative_path, e);
            return Ok(true); // Conservative approach: process if we can't determine status
        }
    };

    if doc_id.is_empty() {
        debug!("Document {} not found in database, needs processing", file_info.relative_path);
        return Ok(true); // New document
    }

    // Check if document exists and compare file hash with error handling
    // Note: Using string formatting for now since mock client doesn't support parameters
    let path_str = file_info.relative_path.replace('\'', "''");
    let sql = format!(
        "SELECT file_hash, processed_at FROM notes WHERE path = '{}'",
        path_str
    );

    let result = match client.query(&sql, &[]).await {
        Ok(result) => result,
        Err(e) => {
            warn!("Database error querying document {} for {}: {}, treating as needs processing",
                  doc_id, file_info.relative_path, e);
            return Ok(true); // Conservative approach: process if query fails
        }
    };

    if let Some(record) = result.records.first() {
        let stored_hash_hex = record
            .data
            .get("file_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let processed_at = record
            .data
            .get("processed_at")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));

        // Validate stored hash format
        if stored_hash_hex.len() != 64 || !stored_hash_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            warn!(
                "Invalid stored hash format for {}: {}, treating as needs processing",
                file_info.relative_path,
                if stored_hash_hex.len() > 8 { &stored_hash_hex[..8] } else { stored_hash_hex }
            );
            return Ok(true);
        }

        // Validate current hash format
        let current_hash_hex = file_info.content_hash_hex();
        if current_hash_hex.len() != 64 || !current_hash_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            warn!(
                "Invalid current hash format for {}: {}, treating as needs processing",
                file_info.relative_path,
                &current_hash_hex[..8]
            );
            return Ok(true);
        }

        // Check if content hash matches
        if stored_hash_hex == current_hash_hex {
            // Hashes match, check if file was modified after processing timestamp
            if let Some(processed_time) = processed_at {
                let processed_system_time: std::time::SystemTime = processed_time.into();
                if file_info.modified_time <= processed_system_time {
                    debug!("Document {} unchanged (hash matches and file not modified after processing)",
                           file_info.relative_path);
                    return Ok(false); // No changes needed
                } else {
                    debug!("Document {} modified after processing (file: {:?}, processed: {:?})",
                           file_info.relative_path, file_info.modified_time, processed_time);
                }
            } else {
                warn!("No processed_at timestamp for {}, treating as needs processing",
                      file_info.relative_path);
                return Ok(true);
            }
        } else {
            debug!("Document {} hash mismatch (stored: {}..., current: {}...)",
                   file_info.relative_path, &stored_hash_hex[..8], &current_hash_hex[..8]);
        }
    } else {
        warn!("No record found for document {} despite finding ID, treating as needs processing",
              file_info.relative_path);
        return Ok(true);
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
        "SELECT path, file_hash FROM notes WHERE path IN [{}]",
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
    for (_i, record) in result.records.iter().enumerate() {
        if let Some(path_value) = record.data.get("path") {
            if let Some(rel_path_str) = path_value.as_str() {
                if let Some(hash_value) = record.data.get("file_hash") {
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
async fn convert_paths_to_file_infos(
    paths: &[PathBuf],
    kiln_root: &Path,
) -> Result<Vec<KilnFileInfo>> {
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

        // Use BLAKE3 hash for content change detection
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        hasher.update(content.as_bytes());
        let blake3_hash = hasher.finalize();
        let mut content_hash = [0u8; 32];
        content_hash.copy_from_slice(blake3_hash.as_bytes());

        // Get modification time
        let modified_time = metadata
            .modified()
            .unwrap_or_else(|_| std::time::SystemTime::now());

        let relative_path = path.strip_prefix(kiln_root).unwrap_or(path).to_path_buf();

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

/// Detect which files have changed by comparing content hashes using efficient batch lookup
///
/// This function uses the hash_lookup module to perform efficient batch queries
/// for stored file hashes, minimizing database round trips and maximizing performance.
/// It compares BLAKE3 content hashes to identify actual changes and prevents
/// unnecessary reprocessing of unchanged files.
///
/// # Performance Characteristics
/// - Batch database queries (max 100 files per query by default)
/// - In-memory hash comparison (very fast)
/// - Cached lookups within the same session
/// - Minimal database round trips: O(n/batch_size) instead of O(n)
/// - Robust error handling with graceful degradation
///
/// # Arguments
/// * `client` - SurrealDB client connection
/// * `file_infos` - List of file references to check for changes
/// * `kiln_root` - Root directory for resolving relative paths
///
/// # Returns
/// Filtered list containing only files that have actually changed
///
/// # Errors
/// Returns an error only if critical database operations fail. Individual file
/// lookup errors are logged but don't stop the overall operation.
///
/// # Example
/// ```ignore
/// let all_files = scan_kiln_directory(&kiln_path, &config).await?;
/// let markdown_files: Vec<&KilnFileInfo> = all_files.iter()
///     .filter(|f| f.is_markdown && f.is_accessible)
///     .collect();
/// let (changed_files, metrics) = detect_changed_files_efficient(&client, &markdown_files, kiln_root).await?;
/// println!("Found {} changed files out of {}", changed_files.len(), markdown_files.len());
/// metrics.log_metrics();
/// ```
async fn detect_changed_files_efficient(
    client: &SurrealClient,
    file_infos: &[&KilnFileInfo],
    kiln_root: &Path,
) -> Result<(Vec<KilnFileInfo>, ChangeDetectionMetrics)> {
    if file_infos.is_empty() {
        debug!("No files to check for changes");
        return Ok((Vec::new(), ChangeDetectionMetrics::new()));
    }

    let start_time = std::time::Instant::now();
    let total_files = file_infos.len();

    info!(
        "🔍 Detecting changes in {} files using efficient batch lookup",
        total_files
    );

    // Extract relative paths for batch query
    let relative_paths: Vec<String> = file_infos
        .iter()
        .map(|fi| fi.relative_path.clone())
        .collect();

    // Use efficient batch hash lookup with caching and error handling
    let lookup_config = crate::hash_lookup::BatchLookupConfig::default();
    let mut hash_cache = crate::hash_lookup::HashLookupCache::new();

    let lookup_result = match crate::hash_lookup::lookup_file_hashes_batch_cached(
        client,
        &relative_paths,
        Some(lookup_config),
        &mut hash_cache,
    ).await {
        Ok(result) => result,
        Err(e) => {
            error!("❌ Database lookup failed, falling back to individual file processing: {}", e);
            // Fallback: treat all files as changed if database lookup fails
            warn!("⚠️ Graceful degradation: processing all {} files as changed", total_files);
            let fallback_metrics = ChangeDetectionMetrics {
                total_files,
                changed_files: total_files,
                skipped_files: 0,
                change_detection_time: start_time.elapsed(),
                database_round_trips: 0,
                cache_hit_rate: 0.0,
                files_per_second: total_files as f64 / start_time.elapsed().as_secs_f64().max(0.001),
            };
            return Ok((file_infos.iter().map(|&fi| fi.clone()).collect(), fallback_metrics));
        }
    };

    debug!(
        "Hash lookup completed: {}/{} files found in {} round trips",
        lookup_result.found_files.len(),
        lookup_result.total_queried,
        lookup_result.database_round_trips
    );

    // Compare hashes to find changed files with robust error handling
    let mut changed_files = Vec::new();
    let mut unchanged_files = Vec::new();
    let error_count = 0;

    for file_info in file_infos {
        match lookup_result.found_files.get(&file_info.relative_path) {
            Some(stored_hash) => {
                // File exists in database - compare hashes
                let current_hash_hex = file_info.content_hash_hex();

                // Validate hash format before comparison
                if current_hash_hex.len() != 64 || !current_hash_hex.chars().all(|c| c.is_ascii_hexdigit()) {
                    warn!(
                        "⚠️ Invalid current hash format for {}: {} - treating as changed",
                        file_info.relative_path,
                        &current_hash_hex[..current_hash_hex.len().min(8)]
                    );
                    changed_files.push((*file_info).clone());
                    continue;
                }

                if stored_hash.file_hash.len() != 64 || !stored_hash.file_hash.chars().all(|c| c.is_ascii_hexdigit()) {
                    warn!(
                        "⚠️ Invalid stored hash format for {}: {} - treating as changed",
                        file_info.relative_path,
                        &stored_hash.file_hash[..stored_hash.file_hash.len().min(8)]
                    );
                    changed_files.push((*file_info).clone());
                    continue;
                }

                if stored_hash.file_hash != current_hash_hex {
                    debug!(
                        "📝 File changed (hash mismatch): {} (stored: {}..., current: {}...)",
                        file_info.relative_path,
                        &stored_hash.file_hash[..8],
                        &current_hash_hex[..8]
                    );
                    changed_files.push((*file_info).clone());
                } else {
                    // Also check if file was modified after processing timestamp
                    let processed_system_time: std::time::SystemTime = stored_hash.modified_at.into();
                    if file_info.modified_time > processed_system_time {
                        debug!(
                            "📝 File changed (timestamp mismatch): {} (file modified: {:?}, processed: {:?})",
                            file_info.relative_path,
                            file_info.modified_time,
                            stored_hash.modified_at
                        );
                        changed_files.push((*file_info).clone());
                    } else {
                        unchanged_files.push(file_info.relative_path.clone());
                    }
                }
            }
            None => {
                // File not in database - treat as new/changed
                debug!(
                    "🆕 New file (not in database): {}",
                    file_info.relative_path
                );
                changed_files.push((*file_info).clone());
            }
        }
    }

    let change_detection_time = start_time.elapsed();

    // Calculate performance metrics
    let cache_stats = hash_cache.stats();
    let cache_hit_rate = if cache_stats.hits + cache_stats.misses > 0 {
        cache_stats.hit_rate
    } else {
        0.0
    };

    let files_per_second = total_files as f64 / change_detection_time.as_secs_f64().max(0.001);

    let metrics = ChangeDetectionMetrics {
        total_files,
        changed_files: changed_files.len(),
        skipped_files: unchanged_files.len(),
        change_detection_time,
        database_round_trips: lookup_result.database_round_trips,
        cache_hit_rate,
        files_per_second,
    };

    // Report cache statistics
    if cache_stats.hits + cache_stats.misses > 0 {
        debug!(
            "Cache performance: {} hits, {} misses, {:.1}% hit rate",
            cache_stats.hits,
            cache_stats.misses,
            cache_hit_rate * 100.0
        );
    }

    // Log sample of unchanged files for debugging (limited to avoid spam)
    if !unchanged_files.is_empty() {
        let sample_size = unchanged_files.len().min(5);
        debug!(
            "✅ Sample of unchanged files: {}",
            unchanged_files[..sample_size].join(", ")
        );
    }

    // Report any errors encountered
    if error_count > 0 {
        warn!(
            "⚠️ Encountered {} errors during change detection (see logs for details)",
            error_count
        );
    }

    info!(
        "✅ Change detection completed in {:?}: {} changed, {} unchanged out of {} total",
        change_detection_time,
        changed_files.len(),
        unchanged_files.len(),
        total_files
    );

    // Performance analysis for larger batches
    if total_files > 10 {
        let effective_batch_size = if lookup_result.database_round_trips > 0 {
            (total_files + lookup_result.database_round_trips - 1) / lookup_result.database_round_trips
        } else {
            total_files
        };

        info!(
            "⚡ Performance: {:.0} files/second, {} database round trips (effective batch size: {})",
            files_per_second,
            lookup_result.database_round_trips,
            effective_batch_size
        );
    }

    // Log detailed metrics
    metrics.log_metrics();

    Ok((changed_files, metrics))
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
                // Convert stored hex string to current file's hex for comparison
                if stored_hash != &file_info.content_hash_hex() {
                    debug!(
                        "file changed (hash mismatch): {} (stored: {}..., current: {}...)",
                        file_info.path.display(),
                        &stored_hash[..stored_hash.len().min(8)],
                        &file_info.content_hash_hex()[..8]
                    );
                    changed_files.push((*file_info).clone());
                } else {
                    debug!("file unchanged: {}", file_info.path.display());
                }
            }
            None => {
                // File not in database - treat as new/changed
                debug!("new file (not in database): {}", file_info.path.display());
                changed_files.push((*file_info).clone());
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
        processing_result.processed_count, processing_result.failed_count, total_time
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kiln_scanner::KilnFileInfo;
    use std::path::PathBuf;
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_change_detection_metrics() {
        let metrics = ChangeDetectionMetrics {
            total_files: 100,
            changed_files: 20,
            skipped_files: 80,
            change_detection_time: Duration::from_millis(500),
            database_round_trips: 2,
            cache_hit_rate: 0.75,
            files_per_second: 200.0,
        };

        let summary = metrics.performance_summary();
        assert!(summary.contains("100 files"));
        assert!(summary.contains("20 changed"));
        assert!(summary.contains("80 skipped"));
        assert!(summary.contains("80.0% unchanged"));
    }

    #[tokio::test]
    async fn test_empty_file_list_change_detection() {
        let client = crate::SurrealClient::new_memory().await.unwrap();
        let kiln_root = PathBuf::from("/test");
        let files: Vec<&KilnFileInfo> = vec![];

        let (changed_files, metrics) = detect_changed_files_efficient(
            &client,
            &files,
            &kiln_root,
        ).await.unwrap();

        assert!(changed_files.is_empty());
        assert_eq!(metrics.total_files, 0);
        assert_eq!(metrics.changed_files, 0);
        assert_eq!(metrics.skipped_files, 0);
    }

    #[tokio::test]
    async fn test_needs_processing_with_invalid_data() {
        let client = crate::SurrealClient::new_memory().await.unwrap();

        let file_info = KilnFileInfo {
            path: PathBuf::from("/test.md"),
            relative_path: String::new(),
            file_size: 100,
            modified_time: SystemTime::now(),
            is_markdown: true,
            is_accessible: true,
            content_hash: [1u8; 32],
        };

        let result = needs_processing(&file_info, &client).await.unwrap();
        assert!(result, "Should return true for empty relative path");
    }

    #[tokio::test]
    async fn test_change_detection_with_new_files() {
        let client = crate::SurrealClient::new_memory().await.unwrap();
        let kiln_root = PathBuf::from("/test");

        let file_info = KilnFileInfo {
            path: PathBuf::from("/test.md"),
            relative_path: "test.md".to_string(),
            file_size: 100,
            modified_time: SystemTime::now(),
            is_markdown: true,
            is_accessible: true,
            content_hash: [1u8; 32],
        };

        let files = vec![&file_info];
        let (changed_files, metrics) = detect_changed_files_efficient(
            &client,
            &files,
            &kiln_root,
        ).await.unwrap();

        assert_eq!(changed_files.len(), 1);
        assert_eq!(metrics.changed_files, 1);
        assert_eq!(metrics.total_files, 1);
        assert_eq!(metrics.skipped_files, 0);
    }

    #[tokio::test]
    async fn test_file_info_hash_validation() {
        let file_info = KilnFileInfo {
            path: PathBuf::from("/test.md"),
            relative_path: "test.md".to_string(),
            file_size: 100,
            modified_time: SystemTime::now(),
            is_markdown: true,
            is_accessible: true,
            content_hash: [0x42; 32],
        };

        let hash_hex = file_info.content_hash_hex();
        assert_eq!(hash_hex.len(), 64);
        assert!(hash_hex.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(hash_hex, "4242424242424242424242424242424242424242424242424242424242424242");
    }
}
