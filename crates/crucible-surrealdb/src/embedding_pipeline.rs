//! Embedding Processing Pipeline
//!
//! Note processing pipeline for generating and storing vector embeddings.
//! Supports bulk processing, incremental updates, and content chunking strategies.

use crucible_core::enrichment::*;
use crate::embedding_pool::EmbeddingThreadPool;
use crate::SurrealClient;
use anyhow::{anyhow, Result};
use crucible_core::types::ParsedNote;
use std::collections::HashMap;
use std::time::Instant;
use tracing::{debug, error, info, warn};

/// Default chunk size for note processing (characters)
const DEFAULT_CHUNK_SIZE: usize = 1000;

/// Default overlap between chunks (characters)
const DEFAULT_CHUNK_OVERLAP: usize = 200;

/// Maximum chunk size to prevent memory issues
const MAX_CHUNK_SIZE: usize = 8000;

/// Pipeline for processing documents with embeddings
pub struct EmbeddingPipeline {
    /// Thread pool for parallel processing
    thread_pool: EmbeddingThreadPool,

    /// Chunking configuration
    chunk_size: usize,
    chunk_overlap: usize,
}

impl EmbeddingPipeline {
    /// Create a new embedding pipeline with the given thread pool
    pub fn new(thread_pool: EmbeddingThreadPool) -> Self {
        Self {
            thread_pool,
            chunk_size: DEFAULT_CHUNK_SIZE,
            chunk_overlap: DEFAULT_CHUNK_OVERLAP,
        }
    }

    /// Create pipeline with custom chunking configuration
    pub fn with_chunking(
        thread_pool: EmbeddingThreadPool,
        chunk_size: usize,
        chunk_overlap: usize,
    ) -> Result<Self> {
        if chunk_size == 0 {
            return Err(anyhow::anyhow!("Chunk size must be greater than 0"));
        }

        if chunk_overlap >= chunk_size {
            return Err(anyhow::anyhow!(
                "Chunk overlap must be less than chunk size"
            ));
        }

        if chunk_size > MAX_CHUNK_SIZE {
            return Err(anyhow::anyhow!("Chunk size exceeds maximum allowed size"));
        }

        Ok(Self {
            thread_pool,
            chunk_size,
            chunk_overlap,
        })
    }

    /// Process multiple documents with embeddings (bulk processing)
    pub async fn process_documents_with_embeddings(
        &self,
        client: &SurrealClient,
        document_ids: &[String],
    ) -> Result<EmbeddingProcessingResult> {
        let start_time = Instant::now();
        let mut result = EmbeddingProcessingResult::new();

        info!(
            "Starting bulk embedding processing for {} documents",
            document_ids.len()
        );

        // Retrieve documents from database
        let documents = self.retrieve_documents(client, document_ids).await?;
        if documents.len() != document_ids.len() {
            warn!(
                "Only {} of {} documents found in database",
                documents.len(),
                document_ids.len()
            );
        }

        // Process documents in batches
        let batch_size = self.thread_pool.batch_size().await;
        let config = self.thread_pool.model_type().await;

        for chunk in document_ids.chunks(batch_size) {
            debug!("Processing batch of {} documents", chunk.len());

            // Prepare documents for processing
            let mut processing_tasks = Vec::new();

            for document_id in chunk {
                if let Some(note) = documents.get(document_id) {
                    // Chunk the note
                    let chunks = self.chunk_document(note, &config);

                    for (chunk_index, chunk_content) in chunks.iter().enumerate() {
                        let chunk_id = format!("{}_{}", document_id, chunk_index);
                        processing_tasks.push((chunk_id.clone(), chunk_content.clone()));
                    }
                } else {
                    warn!("Note {} not found in database", document_id);
                    result.failed_count += 1;

                    let error = EmbeddingError::new(
                        document_id.clone(),
                        EmbeddingErrorType::DatabaseError,
                        "Note not found in database".to_string(),
                    );
                    result.errors.push(error);
                }
            }

            // Process chunks through thread pool
            if !processing_tasks.is_empty() {
                let batch_result = self.thread_pool.process_batch(processing_tasks).await?;

                result.processed_count += batch_result.processed_count;
                result.failed_count += batch_result.failed_count;
                result.errors.extend(batch_result.errors);
                result.embeddings_generated += batch_result.embeddings_generated;

                if batch_result.circuit_breaker_triggered {
                    result.circuit_breaker_triggered = true;
                    break;
                }
            }
        }

        result.total_processing_time = start_time.elapsed();

        info!(
            "Bulk embedding processing complete: {} processed, {} failed, {} embeddings generated in {:?}",
            result.processed_count,
            result.failed_count,
            result.embeddings_generated,
            result.total_processing_time
        );

        Ok(result)
    }

    /// Process a single note incrementally (only if changed)
    pub async fn process_document_incremental(
        &self,
        client: &SurrealClient,
        document_id: &str,
    ) -> Result<IncrementalProcessingResult> {
        let start_time = Instant::now();

        info!("Starting incremental processing for note {}", document_id);

        // Retrieve note from database
        let note = self.retrieve_document(client, document_id).await?;
        let note = match note {
            Some(doc) => doc,
            None => {
                warn!("Note {} not found in database", document_id);
                return Err(anyhow!("Note not found: {}", document_id));
            }
        };

        // Chunk the note first
        let config = self.thread_pool.model_type().await;
        let chunks = self.chunk_document(&note, &config);

        debug!(
            "Note {} chunked into {} parts for processing",
            document_id,
            chunks.len()
        );

        // Get existing chunk hashes for incremental processing
        use crate::kiln_integration::get_document_chunk_hashes;
        let existing_chunk_hashes = get_document_chunk_hashes(client, document_id).await?;

        // Compute hashes for new chunks and determine which need re-embedding
        use sha2::{Digest, Sha256};
        let mut chunks_to_process = Vec::new();
        let mut chunks_to_delete = Vec::new();

        for (chunk_index, chunk_content) in chunks.iter().enumerate() {
            // Compute hash for this chunk
            let mut hasher = Sha256::new();
            hasher.update(chunk_content.as_bytes());
            let new_chunk_hash = format!("{:x}", hasher.finalize());

            // Check if chunk changed
            let chunk_changed = match existing_chunk_hashes.get(&chunk_index) {
                Some(existing_hash) => existing_hash != &new_chunk_hash,
                None => true, // New chunk
            };

            if chunk_changed {
                chunks_to_process.push(chunk_index);
                if existing_chunk_hashes.contains_key(&chunk_index) {
                    chunks_to_delete.push(chunk_index);
                }
            }
        }

        // Delete chunks that no longer exist (note got shorter)
        for (&existing_pos, _) in existing_chunk_hashes.iter() {
            if existing_pos >= chunks.len() {
                chunks_to_delete.push(existing_pos);
            }
        }

        // If no chunks changed, skip processing
        if chunks_to_process.is_empty() && chunks_to_delete.is_empty() {
            info!(
                "Note {} unchanged (all {} chunks match), skipping processing",
                document_id,
                chunks.len()
            );
            return Ok(IncrementalProcessingResult::skipped(
                note.content_hash.clone(),
            ));
        }

        info!(
            "Note {}: {} chunks to re-embed, {} to delete (out of {} total)",
            document_id,
            chunks_to_process.len(),
            chunks_to_delete.len(),
            chunks.len()
        );

        // Delete changed/removed chunks
        if !chunks_to_delete.is_empty() {
            use crate::kiln_integration::delete_document_chunks;
            delete_document_chunks(client, document_id, &chunks_to_delete).await?;
        }

        let mut embeddings_created = 0;
        let embeddings_updated = 0;

        // Process only changed chunks
        for &chunk_index in &chunks_to_process {
            let chunk_content = &chunks[chunk_index];
            let chunk_id = format!("{}_{}", document_id, chunk_index);

            match self
                .thread_pool
                .process_document_with_retry(&chunk_id, chunk_content)
                .await
            {
                Ok(retry_result) => {
                    if retry_result.succeeded {
                        // Store the embedding
                        let embedding = DocumentEmbedding::new(
                            document_id.to_string(),
                            // In real implementation, this would be the actual embedding vector
                            vec![0.1; config.dimensions()],
                            config.model_name().to_string(),
                        )
                        .with_chunk_info(
                            chunk_id.clone(),
                            chunk_content.len(),
                            chunk_index,
                        );

                        if let Err(e) = self.store_document_embedding(client, &embedding).await {
                            error!("Failed to store embedding for chunk {}: {}", chunk_id, e);

                            let _error = EmbeddingError::new(
                                chunk_id,
                                EmbeddingErrorType::DatabaseError,
                                format!("Failed to store embedding: {}", e),
                            );
                            // For incremental processing, we don't collect errors in the result
                            // but we could add error tracking if needed
                        } else {
                            embeddings_created += 1;
                        }
                    } else {
                        warn!(
                            "Failed to process chunk {} after {} attempts: {:?}",
                            chunk_id, retry_result.attempt_count, retry_result.final_error
                        );
                    }
                }
                Err(e) => {
                    error!("Failed to process chunk {}: {}", chunk_id, e);
                }
            }
        }

        // Update note's last processed timestamp
        self.update_document_processed_timestamp(client, document_id)
            .await?;

        let processing_time = start_time.elapsed();

        info!(
            "Incremental processing complete for note {}: {} embeddings created in {:?}",
            document_id, embeddings_created, processing_time
        );

        Ok(IncrementalProcessingResult::processed(
            embeddings_created,
            embeddings_updated,
            note.content_hash.clone(),
            processing_time,
        ))
    }

    /// Process multiple documents incrementally
    pub async fn process_documents_incremental(
        &self,
        client: &SurrealClient,
        document_ids: &[String],
    ) -> Result<BatchIncrementalResult> {
        let start_time = Instant::now();
        let mut result = BatchIncrementalResult::new();

        info!(
            "Starting batch incremental processing for {} documents",
            document_ids.len()
        );

        for document_id in document_ids {
            match self.process_document_incremental(client, document_id).await {
                Ok(processing_result) => {
                    if processing_result.processed {
                        result.processed_count += 1;
                        result.total_embeddings_created += processing_result.embeddings_created;
                        result.total_embeddings_updated += processing_result.embeddings_updated;
                    } else {
                        result.skipped_count += 1;
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to process note {} incrementally: {}",
                        document_id, e
                    );
                    // In batch processing, we might want to continue with other documents
                    // rather than failing the entire batch
                }
            }
        }

        result.total_processing_time = start_time.elapsed();

        info!(
            "Batch incremental processing complete: {} processed, {} skipped in {:?}",
            result.processed_count, result.skipped_count, result.total_processing_time
        );

        Ok(result)
    }

    /// Retrieve documents from database
    async fn retrieve_documents(
        &self,
        client: &SurrealClient,
        document_ids: &[String],
    ) -> Result<HashMap<String, ParsedNote>> {
        let mut documents = HashMap::new();

        for document_id in document_ids {
            if let Ok(Some(note)) = self.retrieve_document(client, document_id).await {
                documents.insert(document_id.clone(), note);
            }
        }

        Ok(documents)
    }

    /// Retrieve a single note from database
    async fn retrieve_document(
        &self,
        _client: &SurrealClient,
        _document_id: &str,
    ) -> Result<Option<ParsedNote>> {
        // This would be implemented in the kiln_integration module
        // For now, return None to indicate note not found
        Ok(None)
    }

    /// Store note embedding in database
    async fn store_document_embedding(
        &self,
        _client: &SurrealClient,
        embedding: &DocumentEmbedding,
    ) -> Result<()> {
        // This would store the embedding in the database
        debug!(
            "Storing embedding for note {}, chunk {} ({} dimensions)",
            embedding.document_id,
            embedding.chunk_id.as_deref().unwrap_or("main"),
            embedding.dimensions()
        );
        Ok(())
    }

    /// Update note's processed timestamp
    async fn update_document_processed_timestamp(
        &self,
        _client: &SurrealClient,
        _document_id: &str,
    ) -> Result<()> {
        // This would update the note's metadata
        debug!("Updating processed timestamp for note {}", _document_id);
        Ok(())
    }

    /// Chunk note content for processing
    fn chunk_document(&self, note: &ParsedNote, _model_type: &EmbeddingModel) -> Vec<String> {
        let content = &note.content.plain_text;

        // For empty content, return no chunks
        if content.is_empty() {
            return Vec::new();
        }

        // For short content, return as single chunk
        if content.len() <= self.chunk_size {
            return vec![content.clone()];
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < content.len() {
            let end = std::cmp::min(start + self.chunk_size, content.len());
            let chunk_end = if end < content.len() {
                // Try to break at word boundary
                if let Some(space_pos) = content[start..end].rfind(' ') {
                    start + space_pos
                } else {
                    end
                }
            } else {
                end
            };

            let chunk = content[start..chunk_end].trim().to_string();
            if !chunk.is_empty() {
                chunks.push(chunk);
            }

            start = if start + self.chunk_overlap < content.len() {
                start + self.chunk_size - self.chunk_overlap
            } else {
                chunk_end
            };

            if start >= content.len() {
                break;
            }
        }

        debug!(
            "Note {} chunked into {} parts (chunk size: {}, overlap: {})",
            note.path.display(),
            chunks.len(),
            self.chunk_size,
            self.chunk_overlap
        );

        chunks
    }

    /// Process a single note with retry logic
    pub async fn process_document_with_retry(
        &self,
        client: &SurrealClient,
        document_id: &str,
    ) -> Result<RetryProcessingResult> {
        let start_time = Instant::now();

        match self.process_document_incremental(client, document_id).await {
            Ok(result) => {
                if result.processed {
                    // Incremental processing doesn't return embeddings directly
                    Ok(RetryProcessingResult::success(
                        1,
                        start_time.elapsed(),
                        Vec::new(),
                    ))
                } else {
                    // Note was skipped, which is a form of success
                    Ok(RetryProcessingResult::success(
                        1,
                        start_time.elapsed(),
                        Vec::new(),
                    ))
                }
            }
            Err(e) => {
                let error = EmbeddingError::new(
                    document_id.to_string(),
                    EmbeddingErrorType::ProcessingError,
                    e.to_string(),
                );
                Ok(RetryProcessingResult::failure(
                    1,
                    start_time.elapsed(),
                    error,
                ))
            }
        }
    }
}

/// Process documents with embeddings using the given thread pool
pub async fn process_documents_with_embeddings(
    thread_pool: &EmbeddingThreadPool,
    client: &SurrealClient,
    document_ids: &[String],
) -> Result<EmbeddingProcessingResult> {
    let pipeline = EmbeddingPipeline::new(thread_pool.clone());
    pipeline
        .process_documents_with_embeddings(client, document_ids)
        .await
}

/// Process a single note incrementally
pub async fn process_document_incremental(
    thread_pool: &EmbeddingThreadPool,
    client: &SurrealClient,
    document_id: &str,
) -> Result<IncrementalProcessingResult> {
    let pipeline = EmbeddingPipeline::new(thread_pool.clone());
    pipeline
        .process_document_incremental(client, document_id)
        .await
}

/// Process multiple documents incrementally
pub async fn process_documents_incremental(
    thread_pool: &EmbeddingThreadPool,
    client: &SurrealClient,
    document_ids: &[String],
) -> Result<BatchIncrementalResult> {
    let pipeline = EmbeddingPipeline::new(thread_pool.clone());
    pipeline
        .process_documents_incremental(client, document_ids)
        .await
}

/// Process a note with retry logic
pub async fn process_document_with_retry(
    thread_pool: &EmbeddingThreadPool,
    client: &SurrealClient,
    document_id: &str,
) -> Result<RetryProcessingResult> {
    let pipeline = EmbeddingPipeline::new(thread_pool.clone());
    pipeline
        .process_document_with_retry(client, document_id)
        .await
}

/// Update note content in database
pub async fn update_document_content(
    _client: &SurrealClient,
    _document_id: &str,
    _document: &ParsedNote,
) -> Result<()> {
    // This would be implemented in the kiln_integration module
    info!("Updating content for note {}", _document_id);
    Ok(())
}

/// Get note embeddings from database
pub async fn get_document_embeddings(
    _client: &SurrealClient,
    _document_id: &str,
) -> Result<Vec<DocumentEmbedding>> {
    // This would query the database for note embeddings
    // For now, return empty vector
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding_config::EmbeddingConfig;

    #[tokio::test]
    async fn test_pipeline_creation() {
        let config = EmbeddingConfig::default();
        let thread_pool = EmbeddingThreadPool::new(config).await.unwrap();
        let pipeline = EmbeddingPipeline::new(thread_pool);

        // Pipeline should be created successfully
        // We can't easily test internal state without exposing it
    }

    #[tokio::test]
    async fn test_pipeline_custom_chunking() {
        let config = EmbeddingConfig::default();
        let thread_pool = EmbeddingThreadPool::new(config).await.unwrap();

        // Valid chunking configuration
        let pipeline = EmbeddingPipeline::with_chunking(thread_pool.clone(), 500, 100).unwrap();

        // Invalid chunking configurations
        assert!(EmbeddingPipeline::with_chunking(thread_pool.clone(), 0, 100,).is_err());

        assert!(EmbeddingPipeline::with_chunking(thread_pool.clone(), 500, 500,).is_err());

        assert!(EmbeddingPipeline::with_chunking(thread_pool, 10000, 100,).is_err());
    }

    #[tokio::test]
    async fn test_document_chunking() {
        let config = EmbeddingConfig::default();
        let thread_pool = EmbeddingThreadPool::new(config).await.unwrap();
        let pipeline = EmbeddingPipeline::new(thread_pool);

        let mut note = ParsedNote::new(std::path::PathBuf::from("/test/doc.md"));

        // Short content - should be single chunk
        note.content.plain_text = "Short content".to_string();
        let chunks = pipeline.chunk_document(&note, &EmbeddingModel::LocalStandard);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Short content");

        // Long content - should be multiple chunks
        let long_content = "This is a very long note that should be chunked. ".repeat(50);
        note.content.plain_text = long_content.clone();
        let chunks = pipeline.chunk_document(&note, &EmbeddingModel::LocalStandard);
        assert!(chunks.len() > 1);

        // Check that chunks overlap and cover the content
        let combined_length: usize = chunks.iter().map(|c| c.len()).sum();
        assert!(combined_length >= long_content.len()); // Due to overlap

        // Empty content - should return empty
        note.content.plain_text = String::new();
        let chunks = pipeline.chunk_document(&note, &EmbeddingModel::LocalStandard);
        assert_eq!(chunks.len(), 0);
    }

    #[tokio::test]
    async fn test_bulk_processing_structure() {
        let config = EmbeddingConfig {
            worker_count: 2,
            batch_size: 2,
            model_type: EmbeddingModel::LocalMini,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 100,
            timeout_ms: 10000,
            retry_attempts: 3,
            retry_delay_ms: 1000,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout_ms: 30000,
        };

        let thread_pool = EmbeddingThreadPool::new(config).await.unwrap();
        let pipeline = EmbeddingPipeline::new(thread_pool.clone());

        // Test with mock client (will fail but structure should work)
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        let document_ids = vec!["doc1".to_string(), "doc2".to_string()];

        let result = pipeline
            .process_documents_with_embeddings(&client, &document_ids)
            .await;

        // Should fail because documents don't exist, but structure should be correct
        assert!(result.is_ok());
        let processing_result = result.unwrap();
        assert_eq!(processing_result.processed_count, 0); // Documents not found
        assert_eq!(processing_result.failed_count, 2); // Both should fail
        assert_eq!(processing_result.errors.len(), 2);

        thread_pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_incremental_processing_structure() {
        let config = EmbeddingConfig::default();
        let thread_pool = EmbeddingThreadPool::new(config).await.unwrap();
        let pipeline = EmbeddingPipeline::new(thread_pool.clone());

        // Test with mock client
        let client = SurrealClient::new_isolated_memory().await.unwrap();

        let result = pipeline
            .process_document_incremental(&client, "nonexistent_doc")
            .await;

        // Should fail because note doesn't exist
        assert!(result.is_err());

        thread_pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_retry_processing_structure() {
        let config = EmbeddingConfig::default();
        let thread_pool = EmbeddingThreadPool::new(config).await.unwrap();
        let pipeline = EmbeddingPipeline::new(thread_pool.clone());

        // Test with mock client
        let client = SurrealClient::new_isolated_memory().await.unwrap();

        let result = pipeline
            .process_document_with_retry(&client, "nonexistent_doc")
            .await;

        // Should return a retry result with failure
        assert!(result.is_ok());
        let retry_result = result.unwrap();
        assert!(!retry_result.succeeded);
        assert!(retry_result.final_error.is_some());

        thread_pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_batch_incremental_structure() {
        let config = EmbeddingConfig::default();
        let thread_pool = EmbeddingThreadPool::new(config).await.unwrap();
        let pipeline = EmbeddingPipeline::new(thread_pool.clone());

        // Test with mock client
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        let document_ids = vec!["doc1".to_string(), "doc2".to_string(), "doc3".to_string()];

        let result = pipeline
            .process_documents_incremental(&client, &document_ids)
            .await;

        // Should succeed but with no processing (documents don't exist)
        assert!(result.is_ok());
        let batch_result = result.unwrap();
        assert_eq!(batch_result.processed_count, 0); // No documents processed
        assert_eq!(batch_result.skipped_count, 0); // No documents skipped (they weren't found)

        thread_pool.shutdown().await.unwrap();
    }
}
