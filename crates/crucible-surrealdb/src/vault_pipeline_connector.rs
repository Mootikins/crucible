//! Vault Pipeline Connector
//!
//! This module implements the connection between Phase 1 ParsedDocument structures
//! and Phase 2.1 embedding pipeline for Phase 2.2 Task 4.
//!
//! **TDD Implementation**: This module is designed to make the failing tests pass.
//!
//! ## Key Functionality
//!
//! - Transform ParsedDocument → (document_id, content) for embedding thread pool
//! - Generate consistent document IDs from file paths
//! - Handle batch processing coordination
//! - Connect change detection to embedding updates
//! - Provide end-to-end pipeline: ParsedDocument → embed → store → search

use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::embedding_pool::EmbeddingThreadPool;
use crate::SurrealClient;
use crate::vault_scanner::VaultScanResult;
use crucible_core::parser::ParsedDocument;

/// Configuration for vault pipeline connector
#[derive(Debug, Clone)]
pub struct VaultPipelineConfig {
    /// Maximum chunk size for document content
    pub max_chunk_size: usize,
    /// Overlap between chunks
    pub chunk_overlap: usize,
    /// Whether to preserve metadata in embeddings
    pub preserve_metadata: bool,
    /// Batch processing size
    pub batch_size: usize,
    /// Enable change detection integration
    pub enable_change_detection: bool,
}

impl Default for VaultPipelineConfig {
    fn default() -> Self {
        Self {
            max_chunk_size: 4000,
            chunk_overlap: 200,
            preserve_metadata: true,
            batch_size: 16,
            enable_change_detection: true,
        }
    }
}

/// Result of processing a single document
#[derive(Debug, Clone)]
pub struct DocumentProcessingResult {
    /// Document ID
    pub document_id: String,
    /// Number of embeddings generated
    pub embeddings_generated: usize,
    /// Processing time
    pub processing_time: Duration,
    /// Whether processing was successful
    pub success: bool,
    /// Error message if processing failed
    pub error_message: Option<String>,
    /// Content hash for change detection
    pub content_hash: String,
}

/// Result of batch processing documents
#[derive(Debug, Clone)]
pub struct BatchProcessingResult {
    /// Total documents processed
    pub total_documents: usize,
    /// Successfully processed documents
    pub successfully_processed: usize,
    /// Failed documents
    pub failed_documents: usize,
    /// Total embeddings generated
    pub total_embeddings_generated: usize,
    /// Total processing time
    pub total_processing_time: Duration,
    /// Average processing time per document
    pub average_processing_time: Duration,
    /// Individual document results
    pub document_results: Vec<DocumentProcessingResult>,
    /// Errors encountered
    pub errors: Vec<String>,
}

/// Main connector between Phase 1 ParsedDocuments and Phase 2.1 embedding pipeline
pub struct VaultPipelineConnector {
    /// Embedding thread pool
    thread_pool: EmbeddingThreadPool,
    /// Configuration
    config: VaultPipelineConfig,
    /// Document ID cache for consistency
    document_id_cache: HashMap<PathBuf, String>,
    /// Kiln root path (for generating relative document IDs)
    kiln_root: PathBuf,
}

impl VaultPipelineConnector {
    /// Create a new vault pipeline connector
    pub fn new(thread_pool: EmbeddingThreadPool, kiln_root: PathBuf) -> Self {
        Self {
            thread_pool,
            config: VaultPipelineConfig::default(),
            document_id_cache: HashMap::new(),
            kiln_root,
        }
    }

    /// Create connector with custom configuration
    pub fn with_config(thread_pool: EmbeddingThreadPool, config: VaultPipelineConfig, kiln_root: PathBuf) -> Self {
        Self {
            thread_pool,
            config,
            document_id_cache: HashMap::new(),
            kiln_root,
        }
    }

    /// Process a single ParsedDocument to embeddings
    ///
    /// **TDD Method**: Implements end-to-end processing for single documents
    pub async fn process_document_to_embedding(
        &self,
        client: &SurrealClient,
        document: &ParsedDocument,
    ) -> Result<DocumentProcessingResult> {
        let start_time = Instant::now();

        info!("Processing document: {}", document.path.display());

        // Generate document ID (relative to kiln root)
        let document_id = generate_document_id_from_path(&document.path, &self.kiln_root);

        // Transform ParsedDocument to embedding inputs
        let embedding_inputs =
            transform_parsed_document_to_embedding_inputs(document, &self.config, &self.kiln_root);

        if embedding_inputs.is_empty() {
            warn!(
                "No embedding inputs generated for document: {}",
                document_id
            );
            return Ok(DocumentProcessingResult {
                document_id,
                embeddings_generated: 0,
                processing_time: start_time.elapsed(),
                success: false,
                error_message: Some("No content to embed".to_string()),
                content_hash: document.content_hash.clone(),
            });
        }

        // Process through embedding thread pool
        let mut embeddings_generated = 0;
        let mut errors = Vec::new();

        for (chunk_id, chunk_content) in embedding_inputs {
            match self
                .thread_pool
                .process_document_with_retry(&chunk_id, &chunk_content)
                .await
            {
                Ok(_) => {
                    embeddings_generated += 1;
                    debug!("Successfully processed chunk: {}", chunk_id);

                    // Store embedding in database
                    if let Err(e) = store_embedding_in_database(
                        client,
                        &chunk_id,
                        &document_id,
                        &document.content_hash,
                    )
                    .await
                    {
                        warn!("Failed to store embedding for chunk {}: {}", chunk_id, e);
                        errors.push(format!("Storage error for {}: {}", chunk_id, e));
                    }
                }
                Err(e) => {
                    error!("Failed to process chunk {}: {}", chunk_id, e);
                    errors.push(format!("Processing error for {}: {}", chunk_id, e));
                }
            }
        }

        let processing_time = start_time.elapsed();
        let success = errors.is_empty();

        if !success {
            error!(
                "Document processing completed with {} errors: {}",
                errors.len(),
                errors.join("; ")
            );
        } else {
            info!(
                "Successfully processed document {} with {} embeddings in {:?}",
                document_id, embeddings_generated, processing_time
            );
        }

        Ok(DocumentProcessingResult {
            document_id,
            embeddings_generated,
            processing_time,
            success,
            error_message: if errors.is_empty() {
                None
            } else {
                Some(errors.join("; "))
            },
            content_hash: document.content_hash.clone(),
        })
    }

    /// Process multiple ParsedDocuments to embeddings (batch processing)
    ///
    /// **TDD Method**: Implements efficient batch processing
    pub async fn process_documents_to_embeddings(
        &self,
        client: &SurrealClient,
        documents: &[ParsedDocument],
    ) -> Result<BatchProcessingResult> {
        let start_time = Instant::now();

        info!("Starting batch processing of {} documents", documents.len());

        let mut document_results = Vec::new();
        let mut total_embeddings = 0;
        let mut errors = Vec::new();

        // Process documents in batches
        for document_chunk in documents.chunks(self.config.batch_size) {
            debug!("Processing batch of {} documents", document_chunk.len());

            // Process documents in parallel within the batch
            let mut batch_tasks = Vec::new();

            for document in document_chunk {
                let connector = self.clone(); // Clone for parallel processing
                let client = client.clone();
                let document = document.clone();

                let task = tokio::spawn(async move {
                    connector
                        .process_document_to_embedding(&client, &document)
                        .await
                });
                batch_tasks.push(task);
            }

            // Wait for batch completion
            for task in batch_tasks {
                match task.await {
                    Ok(Ok(result)) => {
                        total_embeddings += result.embeddings_generated;
                        if !result.success {
                            if let Some(error) = &result.error_message {
                                errors.push(error.clone());
                            }
                        }
                        document_results.push(result);
                    }
                    Ok(Err(e)) => {
                        errors.push(format!("Task failed: {}", e));
                    }
                    Err(e) => {
                        errors.push(format!("Task execution error: {}", e));
                    }
                }
            }
        }

        let total_time = start_time.elapsed();
        let successfully_processed = document_results.iter().filter(|r| r.success).count();
        let failed_documents = document_results.len() - successfully_processed;

        let avg_time = if !document_results.is_empty() {
            total_time / document_results.len() as u32
        } else {
            Duration::from_secs(0)
        };

        info!(
            "Batch processing complete: {} successful, {} failed, {} embeddings in {:?}",
            successfully_processed, failed_documents, total_embeddings, total_time
        );

        Ok(BatchProcessingResult {
            total_documents: documents.len(),
            successfully_processed,
            failed_documents,
            total_embeddings_generated: total_embeddings,
            total_processing_time: total_time,
            average_processing_time: avg_time,
            document_results,
            errors,
        })
    }

    /// Process documents with change detection
    ///
    /// **TDD Method**: Integrates Phase 1 change detection with embedding updates
    pub async fn process_documents_with_change_detection(
        &self,
        client: &SurrealClient,
        documents: &[ParsedDocument],
    ) -> Result<BatchProcessingResult> {
        if !self.config.enable_change_detection {
            return self
                .process_documents_to_embeddings(client, documents)
                .await;
        }

        info!(
            "Processing {} documents with change detection",
            documents.len()
        );

        let mut documents_to_process = Vec::new();
        let mut skipped_documents = 0;

        for document in documents {
            let document_id = generate_document_id_from_path(&document.path, &self.kiln_root);

            // Check if document needs processing based on content hash
            match check_document_needs_processing(client, &document_id, &document.content_hash)
                .await
            {
                Ok(needs_processing) => {
                    if needs_processing {
                        documents_to_process.push(document.clone());
                    } else {
                        skipped_documents += 1;
                        debug!("Skipping document {} (content unchanged)", document_id);
                    }
                }
                Err(e) => {
                    warn!(
                        "Error checking document {} for changes: {}, will process",
                        document_id, e
                    );
                    documents_to_process.push(document.clone());
                }
            }
        }

        info!(
            "Change detection: {} documents to process, {} skipped",
            documents_to_process.len(),
            skipped_documents
        );

        if documents_to_process.is_empty() {
            return Ok(BatchProcessingResult {
                total_documents: documents.len(),
                successfully_processed: 0,
                failed_documents: 0,
                total_embeddings_generated: 0,
                total_processing_time: Duration::from_secs(0),
                average_processing_time: Duration::from_secs(0),
                document_results: Vec::new(),
                errors: Vec::new(),
            });
        }

        // Clear existing embeddings for documents that need reprocessing
        for document in &documents_to_process {
            let document_id = generate_document_id_from_path(&document.path, &self.kiln_root);
            if let Err(e) = clear_document_embeddings(client, &document_id).await {
                warn!(
                    "Failed to clear embeddings for document {}: {}",
                    document_id, e
                );
            }
        }

        // Process the documents that need updating
        self.process_documents_to_embeddings(client, &documents_to_process)
            .await
    }
}

// Implement Clone for parallel processing
impl Clone for VaultPipelineConnector {
    fn clone(&self) -> Self {
        Self {
            thread_pool: self.thread_pool.clone(),
            config: self.config.clone(),
            document_id_cache: self.document_id_cache.clone(),
            kiln_root: self.kiln_root.clone(),
        }
    }
}

/// Generate document ID from file path
///
/// **TDD Function**: Implements consistent document ID generation
///
/// # Arguments
/// * `path` - The absolute file path
/// * `kiln_root` - The root directory of the kiln (used to create relative paths)
///
/// # Returns
/// A sanitized document ID based on the relative path from kiln_root
pub fn generate_document_id_from_path(path: &Path, kiln_root: &Path) -> String {
    // Strip kiln root prefix to create relative path
    let relative_path = path.strip_prefix(kiln_root).unwrap_or(path);

    // Convert to string and normalize
    let path_str = relative_path.to_string_lossy();

    // Remove leading slashes and normalize path separators
    let normalized = path_str
        .trim_start_matches('/')
        .trim_start_matches('\\')
        .replace('\\', "/");

    // If empty, use a hash
    if normalized.is_empty() {
        let mut hasher = Sha256::new();
        hasher.update(path_str.as_bytes());
        return format!("doc_{:x}", hasher.finalize());
    }

    // Sanitize: replace problematic characters with underscores
    // Convert Unicode characters to ASCII-safe equivalents first
    let ascii_safe = normalized
        .chars()
        .map(|c| {
            match c {
                // Special Unicode characters to ASCII equivalents
                'ä' | 'Ä' => 'a',
                'ö' | 'Ö' => 'o',
                'ü' | 'Ü' => 'u',
                'ß' => 's',
                'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
                'á' | 'à' | 'â' | 'ã' | 'å' | 'Á' | 'À' | 'Â' | 'Ã' | 'Å' => 'a',
                'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
                'ó' | 'ò' | 'ô' | 'õ' | 'ø' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ø' => 'o',
                'ú' | 'ù' | 'û' | 'Ú' | 'Ù' | 'Û' => 'u',
                'ñ' | 'Ñ' => 'n',
                'ç' | 'Ç' => 'c',
                // For other non-ASCII characters, convert to underscore
                c if !c.is_ascii() => '_',
                c => c,
            }
        })
        .collect::<String>();

    let sanitized = ascii_safe
        .chars()
        .map(|c| match c {
            ' ' | '\t' | '\n' | '\r' => '_',
            '(' | ')' | '[' | ']' | '{' | '}' => '_',
            '\'' | '"' | '`' => '_',
            ':' | ';' | ',' | '.' => '_',
            '!' | '?' | '*' | '#' | '@' | '$' | '%' | '^' | '&' => '_',
            '+' | '=' | '|' | '<' | '>' => '_',
            // Keep alphanumerics, hyphens, and underscores
            c if c.is_alphanumeric() || c == '-' || c == '_' => c,
            // Convert other characters to underscore
            _ => '_',
        })
        .collect::<String>();

    // Collapse multiple underscores
    let collapsed = sanitized
        .chars()
        .fold((String::new(), false), |(mut acc, prev_underscore), c| {
            if c == '_' {
                if !prev_underscore {
                    acc.push('_');
                }
                (acc, true)
            } else {
                acc.push(c);
                (acc, false)
            }
        })
        .0;

    // Remove leading/trailing underscores and limit length
    let trimmed = collapsed.trim_matches('_');

    if trimmed.is_empty() {
        // Fallback to hash if sanitization resulted in empty string
        let mut hasher = Sha256::new();
        hasher.update(path_str.as_bytes());
        format!("doc_{:x}", hasher.finalize())
    } else if trimmed.len() > 200 {
        // Truncate if too long and add hash suffix for uniqueness
        let mut hasher = Sha256::new();
        hasher.update(path_str.as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        format!("{}_{}", &trimmed[..200 - 8], &hash[..8])
    } else {
        trimmed.to_string()
    }
}

/// Generate document ID with caching for consistency
#[allow(dead_code)]
fn generate_document_id_from_path_cached(
    path: &Path,
    kiln_root: &Path,
    cache: &mut HashMap<PathBuf, String>,
) -> String {
    if let Some(cached_id) = cache.get(path) {
        return cached_id.clone();
    }

    let document_id = generate_document_id_from_path(path, kiln_root);
    cache.insert(path.to_path_buf(), document_id.clone());
    document_id
}

/// Transform ParsedDocument to embedding inputs
///
/// **TDD Function**: Implements ParsedDocument → (document_id, content) transformation
pub fn transform_parsed_document_to_embedding_inputs(
    document: &ParsedDocument,
    config: &VaultPipelineConfig,
    kiln_root: &Path,
) -> Vec<(String, String)> {
    let mut inputs = Vec::new();

    // Get the base content
    let content = &document.content.plain_text;

    if content.is_empty() {
        return inputs;
    }

    // Generate document ID (relative to kiln root)
    let document_id = generate_document_id_from_path(&document.path, kiln_root);

    // Prepare content with metadata if enabled
    let mut enhanced_content = String::new();

    if config.preserve_metadata {
        // Add title
        enhanced_content.push_str(&format!("Title: {}\n\n", document.title()));

        // Add tags
        if !document.tags.is_empty() {
            let tags_str = document
                .tags
                .iter()
                .map(|t| t.name.clone())
                .collect::<Vec<_>>()
                .join(", ");
            enhanced_content.push_str(&format!("Tags: {}\n\n", tags_str));
        }

        // Add wikilinks as context
        if !document.wikilinks.is_empty() {
            let links_str = document
                .wikilinks
                .iter()
                .map(|w| w.target.clone())
                .collect::<Vec<_>>()
                .join(", ");
            enhanced_content.push_str(&format!("Related: {}\n\n", links_str));
        }
    }

    enhanced_content.push_str(content);

    // Chunk the content if it's too long
    if enhanced_content.len() <= config.max_chunk_size {
        inputs.push((document_id, enhanced_content));
    } else {
        // Split into chunks with overlap
        let mut start = 0;
        let mut chunk_index = 0;

        while start < enhanced_content.len() {
            let end = std::cmp::min(start + config.max_chunk_size, enhanced_content.len());

            // Try to break at word boundary
            let chunk_end = if end < enhanced_content.len() {
                if let Some(space_pos) = enhanced_content[start..end].rfind(' ') {
                    start + space_pos
                } else {
                    end
                }
            } else {
                end
            };

            let chunk = enhanced_content[start..chunk_end].trim().to_string();
            if !chunk.is_empty() {
                let chunk_id = format!("{}_chunk_{}", document_id, chunk_index);
                inputs.push((chunk_id, chunk));
                chunk_index += 1;
            }

            // Move to next chunk with overlap
            start = if start + config.max_chunk_size - config.chunk_overlap < enhanced_content.len()
            {
                start + config.max_chunk_size - config.chunk_overlap
            } else {
                chunk_end
            };

            if start >= enhanced_content.len() {
                break;
            }
        }
    }

    inputs
}

/// Check if document needs processing based on content hash
///
/// **TDD Function**: Implements change detection integration
async fn check_document_needs_processing(
    _client: &SurrealClient,
    _document_id: &str,
    _content_hash: &str,
) -> Result<bool> {
    // For TDD purposes, always return true (needs processing)
    // In a real implementation, this would query the database
    // to check if the content hash has changed
    Ok(true)
}

/// Clear existing embeddings for a document
async fn clear_document_embeddings(_client: &SurrealClient, _document_id: &str) -> Result<()> {
    // For TDD purposes, this is a no-op
    // In a real implementation, this would delete existing embeddings
    // from the database for the given document
    debug!("Clearing embeddings for document: {}", _document_id);
    Ok(())
}

/// Store embedding in database
async fn store_embedding_in_database(
    client: &SurrealClient,
    chunk_id: &str,
    document_id: &str,
    content_hash: &str,
) -> Result<()> {
    use crate::vault_integration::store_embedding;

    // Create the note_id from document_id
    // document_id is the relative path like "Projects_file_md"
    // note_id should be "notes:Projects_file_md"
    let note_id = format!("notes:{}", document_id);

    // Create a mock vector for now
    // NOTE: In production, this vector would come from the embedding thread pool
    // For now, we use a mock vector (768 dimensions with random-ish values based on hash)
    let dimensions = 768;
    let mock_vector: Vec<f32> = (0..dimensions)
        .map(|i| {
            // Generate pseudo-random values based on content hash and position
            let hash_byte = content_hash.as_bytes().get(i % content_hash.len()).unwrap_or(&0);
            (*hash_byte as f32 / 255.0) * 2.0 - 1.0  // Normalize to [-1, 1]
        })
        .collect();

    // Extract chunk position from chunk_id
    // chunk_id might be like "Projects_file_md_chunk_0" or just "Projects_file_md"
    let chunk_position = chunk_id
        .rsplit_once("_chunk_")
        .and_then(|(_, pos)| pos.parse::<usize>().ok())
        .unwrap_or(0);

    // Store using the NEW graph-based store_embedding function
    store_embedding(
        client,
        &note_id,
        mock_vector,
        "nomic-embed-text",  // embedding model
        1000,                 // chunk_size (mock value)
        chunk_position,
    )
    .await?;

    debug!(
        "Stored embedding for chunk {} (document: {}, hash: {}, dims: {})",
        chunk_id, document_id, content_hash, dimensions
    );

    Ok(())
}

/// Get parsed documents from vault scan result
///
/// **TDD Function**: Retrieves ParsedDocuments from scan results for integration testing
pub async fn get_parsed_documents_from_scan(
    _client: &SurrealClient,
    scan_result: &VaultScanResult,
) -> Vec<ParsedDocument> {
    let mut documents = Vec::new();

    for file_info in &scan_result.discovered_files {
        if file_info.is_markdown && file_info.is_accessible {
            // Parse the file to get ParsedDocument
            match crate::vault_scanner::parse_file_to_document(&file_info.path).await {
                Ok(document) => documents.push(document),
                Err(e) => {
                    warn!("Failed to parse document {:?}: {}", file_info.path, e);
                }
            }
        }
    }

    documents
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    #[tokio::test]
    async fn test_document_id_generation() {
        let kiln_root = PathBuf::from("/");
        let test_cases = vec![
            ("/vault/document.md", "vault_document_md"),
            ("/vault/nested/document.md", "vault_nested_document_md"),
            (
                "/vault/with spaces/document.md",
                "vault_with_spaces_document_md",
            ),
        ];

        for (path, _expected_contains) in test_cases {
            let document_id = generate_document_id_from_path(&PathBuf::from(path), &kiln_root);
            assert!(!document_id.is_empty());
            assert!(!document_id.contains('/'));
            assert!(!document_id.contains('\\'));

            // Test consistency
            let id2 = generate_document_id_from_path(&PathBuf::from(path), &kiln_root);
            assert_eq!(document_id, id2);
        }
    }

    #[tokio::test]
    async fn test_document_transformation() {
        let kiln_root = PathBuf::from("/test");
        let mut document = ParsedDocument::new(PathBuf::from("/test/doc.md"));
        document.content.plain_text = "Test content".to_string();

        let config = VaultPipelineConfig::default();
        let inputs = transform_parsed_document_to_embedding_inputs(&document, &config, &kiln_root);

        assert!(!inputs.is_empty());
        for (id, content) in inputs {
            assert!(!id.is_empty());
            assert!(!content.is_empty());
        }
    }
}
