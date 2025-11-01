//! Trait abstraction for kiln document storage operations.
//!
//! This module provides the `KilnStore` trait, which defines the minimal interface
//! needed for storing and retrieving embeddings with metadata. The trait is designed
//! for testability while supporting both in-memory and persistent implementations.
//!
//! # Design Philosophy
//!
//! The `KilnStore` trait follows these principles:
//! - **Minimal interface**: Only methods actually used by the codebase
//! - **Test-focused**: Enable fast, isolated unit tests with mock implementations
//! - **No leaky abstractions**: Don't expose SurrealDB-specific details
//! - **Async-first**: All operations return futures for flexibility
//!
//! # Usage
//!
//! Use `SurrealEmbeddingDatabase` for production and `InMemoryKilnStore` for tests.

use crate::types::{
    BatchOperation, BatchOperationType, BatchResult, DatabaseStats, EmbeddingData,
    EmbeddingMetadata, SearchQuery, SearchResultWithScore,
};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

/// Minimal trait for kiln document storage operations.
///
/// This trait provides the minimum interface needed for storing and
/// retrieving embeddings with metadata. It's designed for testability
/// while supporting both in-memory and persistent implementations.
///
/// # Method Categories
///
/// - **Storage**: `store_embedding`, `update_metadata`, `delete_file`
/// - **Retrieval**: `get_embedding`, `file_exists`, `list_files`
/// - **Search**: `search_similar`, `search_by_tags`, `search_by_properties`, `search`
/// - **Batch**: `batch_operation`
/// - **Metadata**: `get_stats`
/// - **Lifecycle**: `initialize`, `close`
#[async_trait]
pub trait KilnStore: Send + Sync {
    // === Core Storage Operations ===

    /// Store an embedding for a document.
    ///
    /// This is the primary write operation. If a document with the same `file_path`
    /// already exists, it will be replaced.
    ///
    /// # Arguments
    /// - `file_path`: Unique identifier for the document (typically relative path)
    /// - `content`: Full markdown content of the document
    /// - `embedding`: Vector embedding (typically 384 or 768 dimensions)
    /// - `metadata`: Document metadata (title, tags, properties, timestamps)
    async fn store_embedding(
        &self,
        file_path: &str,
        content: &str,
        embedding: &[f32],
        metadata: &EmbeddingMetadata,
    ) -> Result<()>;

    /// Update metadata without changing embedding.
    ///
    /// Use this when document metadata changes but content (and thus embedding) remains the same.
    /// More efficient than re-generating and storing the embedding.
    ///
    /// # Returns
    /// `Ok(())` if updated, `Err` if file doesn't exist
    async fn update_metadata(
        &self,
        file_path: &str,
        metadata: &EmbeddingMetadata,
    ) -> Result<()>;

    /// Update metadata properties using a HashMap.
    ///
    /// Merges properties into existing metadata without replacing it entirely.
    /// Used for edge cases where only specific frontmatter properties change.
    ///
    /// # Returns
    /// `Ok(true)` if updated, `Ok(false)` if file doesn't exist
    async fn update_metadata_hashmap(
        &self,
        file_path: &str,
        properties: HashMap<String, Value>,
    ) -> Result<bool>;

    /// Delete a document from the store.
    ///
    /// # Returns
    /// `Ok(true)` if deleted, `Ok(false)` if file didn't exist
    async fn delete_file(&self, file_path: &str) -> Result<bool>;

    // === Retrieval Operations ===

    /// Get embedding data for a document.
    ///
    /// # Returns
    /// `Ok(Some(data))` if found, `Ok(None)` if not found
    async fn get_embedding(&self, file_path: &str) -> Result<Option<EmbeddingData>>;

    /// Check if a document exists in the store.
    ///
    /// More efficient than `get_embedding` when you only need existence check.
    async fn file_exists(&self, file_path: &str) -> Result<bool>;

    /// List all document file paths in the store.
    ///
    /// Returns paths in arbitrary order (implementation-dependent).
    async fn list_files(&self) -> Result<Vec<String>>;

    // === Search Operations ===

    /// Semantic search by embedding similarity.
    ///
    /// Finds documents with embeddings most similar to the query embedding.
    /// Uses cosine similarity for comparison.
    ///
    /// # Arguments
    /// - `query`: Original query string (for logging/debugging, not used for search)
    /// - `query_embedding`: Vector embedding of the query
    /// - `top_k`: Maximum number of results to return
    ///
    /// # Returns
    /// Results sorted by similarity score (highest first)
    ///
    /// # Example
    /// ```rust,no_run
    /// # use crucible_surrealdb::*;
    /// # async fn example(store: &dyn KilnStore, query_embedding: &[f32]) -> anyhow::Result<()> {
    /// let results = store.search_similar(
    ///     "rust async programming",
    ///     query_embedding,
    ///     5
    /// ).await?;
    ///
    /// for result in results {
    ///     println!("{}: {:.3}", result.title, result.score);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn search_similar(
        &self,
        query: &str,
        query_embedding: &[f32],
        top_k: u32,
    ) -> Result<Vec<SearchResultWithScore>>;

    /// Search documents by tags.
    ///
    /// Returns documents that have ALL specified tags.
    ///
    /// # Example
    /// ```rust,no_run
    /// # use crucible_surrealdb::*;
    /// # async fn example(store: &dyn KilnStore) -> anyhow::Result<()> {
    /// // Find documents tagged with both "rust" AND "async"
    /// let files = store.search_by_tags(&[
    ///     "rust".to_string(),
    ///     "async".to_string()
    /// ]).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn search_by_tags(&self, tags: &[String]) -> Result<Vec<String>>;

    /// Search documents by frontmatter properties.
    ///
    /// Returns documents where ALL specified properties match exactly.
    ///
    /// # Example
    /// ```rust,no_run
    /// # use crucible_surrealdb::*;
    /// # use std::collections::HashMap;
    /// # use serde_json::json;
    /// # async fn example(store: &dyn KilnStore) -> anyhow::Result<()> {
    /// let mut props = HashMap::new();
    /// props.insert("status".to_string(), json!("published"));
    /// props.insert("author".to_string(), json!("alice"));
    ///
    /// let files = store.search_by_properties(&props).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn search_by_properties(
        &self,
        properties: &HashMap<String, Value>,
    ) -> Result<Vec<String>>;

    /// Advanced search with filters.
    ///
    /// Combines semantic search with tag/property filters.
    /// See `SearchQuery` type for filter options.
    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResultWithScore>>;

    // === Batch Operations ===

    /// Execute batch operations atomically.
    ///
    /// Allows multiple inserts/updates/deletes in a single transaction.
    /// Implementation may choose to execute sequentially if atomicity isn't critical.
    ///
    /// # Returns
    /// `BatchResult` with success count and any errors
    async fn batch_operation(&self, operation: &BatchOperation) -> Result<BatchResult>;

    // === Metadata & Stats ===

    /// Get database statistics.
    ///
    /// Useful for monitoring and debugging.
    async fn get_stats(&self) -> Result<DatabaseStats>;

    // === Lifecycle ===

    /// Initialize database schema and indexes.
    ///
    /// Must be idempotent - safe to call multiple times.
    /// Typically called once at application startup.
    async fn initialize(&self) -> Result<()>;

    /// Close database connection and clean up resources.
    ///
    /// Consumes self to prevent use-after-close.
    /// May be a no-op for in-memory implementations.
    async fn close(self: Box<Self>) -> Result<()>;
}

// === In-Memory Test Implementation ===

use std::sync::{Arc, RwLock};

/// Fast in-memory implementation of `KilnStore` for unit tests.
///
/// This implementation provides:
/// - **Fast**: No I/O, all operations in-memory
/// - **Deterministic**: No timing dependencies or external services
/// - **Isolated**: Each instance has independent state
/// - **Resettable**: Can clear state between tests
pub struct InMemoryKilnStore {
    storage: Arc<RwLock<HashMap<String, EmbeddingData>>>,
    stats: Arc<RwLock<DatabaseStats>>,
}

impl InMemoryKilnStore {
    /// Create a new empty in-memory store.
    pub fn new() -> Self {
        Self {
            storage: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(DatabaseStats {
                total_documents: 0,
                total_embeddings: 0,
                storage_size_bytes: Some(0),
                last_updated: chrono::Utc::now(),
            })),
        }
    }

    /// Pre-populate store with test data.
    ///
    /// Useful for setting up test fixtures.
    pub fn seed(&self, data: Vec<EmbeddingData>) -> Result<()> {
        let mut storage = self
            .storage
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        for item in data {
            storage.insert(item.file_path.clone(), item);
        }

        self.update_stats()?;
        Ok(())
    }

    /// Reset store to empty state.
    ///
    /// Useful for test isolation - call between test cases.
    pub fn reset(&self) -> Result<()> {
        let mut storage = self
            .storage
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        storage.clear();

        let mut stats = self
            .stats
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        *stats = DatabaseStats {
            total_documents: 0,
            total_embeddings: 0,
            storage_size_bytes: Some(0),
            last_updated: chrono::Utc::now(),
        };

        Ok(())
    }

    /// Get current document count (for test assertions).
    pub fn len(&self) -> usize {
        self.storage
            .read()
            .map(|s| s.len())
            .unwrap_or(0)
    }

    /// Check if store is empty (for test assertions).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Helper to update stats after modifications.
    fn update_stats(&self) -> Result<()> {
        let storage = self
            .storage
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let total_documents = storage.len() as i64;
        let total_embeddings = storage.values().filter(|d| !d.embedding.is_empty()).count() as i64;

        let mut stats = self
            .stats
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        stats.total_documents = total_documents;
        stats.total_embeddings = total_embeddings;
        stats.storage_size_bytes = Some(0); // In-memory doesn't track actual bytes
        stats.last_updated = chrono::Utc::now();

        Ok(())
    }
}

impl Default for InMemoryKilnStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl KilnStore for InMemoryKilnStore {
    async fn store_embedding(
        &self,
        file_path: &str,
        content: &str,
        embedding: &[f32],
        metadata: &EmbeddingMetadata,
    ) -> Result<()> {
        let data = EmbeddingData {
            file_path: file_path.to_string(),
            content: content.to_string(),
            embedding: embedding.to_vec(),
            metadata: metadata.clone(),
        };

        let mut storage = self
            .storage
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        storage.insert(file_path.to_string(), data);
        drop(storage); // Release lock before updating stats

        self.update_stats()?;
        Ok(())
    }

    async fn update_metadata(
        &self,
        file_path: &str,
        metadata: &EmbeddingMetadata,
    ) -> Result<()> {
        let mut storage = self
            .storage
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        if let Some(embedding_data) = storage.get_mut(file_path) {
            embedding_data.metadata = metadata.clone();
            Ok(())
        } else {
            anyhow::bail!("File not found: {}", file_path)
        }
    }

    async fn update_metadata_hashmap(
        &self,
        file_path: &str,
        properties: HashMap<String, Value>,
    ) -> Result<bool> {
        let mut storage = self
            .storage
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        if let Some(embedding_data) = storage.get_mut(file_path) {
            embedding_data.metadata.properties.extend(properties);
            embedding_data.metadata.updated_at = chrono::Utc::now();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn delete_file(&self, file_path: &str) -> Result<bool> {
        let mut storage = self
            .storage
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        let removed = storage.remove(file_path).is_some();
        drop(storage);

        if removed {
            self.update_stats()?;
        }

        Ok(removed)
    }

    async fn get_embedding(&self, file_path: &str) -> Result<Option<EmbeddingData>> {
        let storage = self
            .storage
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        Ok(storage.get(file_path).cloned())
    }

    async fn file_exists(&self, file_path: &str) -> Result<bool> {
        let storage = self
            .storage
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        Ok(storage.contains_key(file_path))
    }

    async fn list_files(&self) -> Result<Vec<String>> {
        let storage = self
            .storage
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        Ok(storage.keys().cloned().collect())
    }

    async fn search_similar(
        &self,
        _query: &str,
        query_embedding: &[f32],
        top_k: u32,
    ) -> Result<Vec<SearchResultWithScore>> {
        let storage = self
            .storage
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let mut results = Vec::new();

        for (file_path, embedding_data) in storage.iter() {
            if embedding_data.embedding.is_empty() {
                continue;
            }

            let similarity = cosine_similarity(query_embedding, &embedding_data.embedding);

            if similarity > 0.0 {
                results.push(SearchResultWithScore {
                    id: file_path.clone(),
                    title: embedding_data
                        .metadata
                        .title
                        .clone()
                        .unwrap_or_else(|| file_path.clone()),
                    content: embedding_data.content.clone(),
                    score: similarity,
                });
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });
        results.truncate(top_k as usize);

        Ok(results)
    }

    async fn search_by_tags(&self, tags: &[String]) -> Result<Vec<String>> {
        let storage = self
            .storage
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let results: Vec<String> = storage
            .iter()
            .filter(|(_, data)| {
                tags.iter()
                    .all(|required_tag| data.metadata.tags.contains(required_tag))
            })
            .map(|(path, _)| path.clone())
            .collect();

        Ok(results)
    }

    async fn search_by_properties(
        &self,
        properties: &HashMap<String, Value>,
    ) -> Result<Vec<String>> {
        let storage = self
            .storage
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let results: Vec<String> = storage
            .iter()
            .filter(|(_, data)| {
                properties.iter().all(|(key, expected_value)| {
                    data.metadata
                        .properties
                        .get(key)
                        .map(|v| v == expected_value)
                        .unwrap_or(false)
                })
            })
            .map(|(path, _)| path.clone())
            .collect();

        Ok(results)
    }

    async fn search(&self, _query: &SearchQuery) -> Result<Vec<SearchResultWithScore>> {
        // Simplified implementation - just return empty for now
        // Real implementation would parse query, generate embedding, apply filters
        // This is sufficient for most tests
        Ok(Vec::new())
    }

    async fn batch_operation(&self, operation: &BatchOperation) -> Result<BatchResult> {
        let mut successful = 0;
        let mut failed = 0;
        let mut errors = Vec::new();

        for document in &operation.documents {
            let embedding_data = EmbeddingData::from(document.clone());
            let result = match operation.operation_type {
                BatchOperationType::Create => {
                    self.store_embedding(
                        &document.file_path,
                        &document.content,
                        &document.embedding,
                        &embedding_data.metadata,
                    )
                    .await
                    .map(|_| ())
                }
                BatchOperationType::Update => {
                    self.update_metadata(&document.file_path, &embedding_data.metadata)
                        .await
                        .map(|_| ())
                }
                BatchOperationType::Delete => self.delete_file(&document.file_path).await.map(|_| ()),
            };

            match result {
                Ok(_) => successful += 1,
                Err(e) => {
                    failed += 1;
                    errors.push(format!("{}: {}", document.file_path, e));
                }
            }
        }

        Ok(BatchResult {
            successful,
            failed,
            errors,
        })
    }

    async fn get_stats(&self) -> Result<DatabaseStats> {
        let stats = self
            .stats
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        Ok(stats.clone())
    }

    async fn initialize(&self) -> Result<()> {
        // No initialization needed for in-memory
        Ok(())
    }

    async fn close(self: Box<Self>) -> Result<()> {
        // No cleanup needed for in-memory
        Ok(())
    }
}

/// Compute cosine similarity between two vectors.
///
/// Returns 0.0 if vectors have different lengths or if either is zero-length.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    (dot_product / (norm_a * norm_b)) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_store_basic_operations() {
        // Create store
        let store = InMemoryKilnStore::new();
        assert!(store.is_empty());

        // Create test data
        let metadata = EmbeddingMetadata {
            file_path: "test.md".to_string(),
            title: Some("Test Doc".to_string()),
            tags: vec!["test".to_string()],
            folder: "test".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Store embedding
        store
            .store_embedding("test.md", "# Test Content", &[0.1; 768], &metadata)
            .await
            .unwrap();

        // Verify storage
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);

        // Check file exists
        assert!(store.file_exists("test.md").await.unwrap());
        assert!(!store.file_exists("nonexistent.md").await.unwrap());

        // Retrieve embedding
        let result = store.get_embedding("test.md").await.unwrap();
        assert!(result.is_some());
        let data = result.unwrap();
        assert_eq!(data.file_path, "test.md");
        assert_eq!(data.content, "# Test Content");
        assert_eq!(data.embedding.len(), 768);

        // Delete file
        let deleted = store.delete_file("test.md").await.unwrap();
        assert!(deleted);
        assert!(store.is_empty());
    }

    #[tokio::test]
    async fn test_in_memory_store_trait_object() {
        // Verify we can use it as a trait object
        let store: Arc<dyn KilnStore> = Arc::new(InMemoryKilnStore::new());

        let metadata = EmbeddingMetadata {
            file_path: "doc.md".to_string(),
            title: Some("Doc".to_string()),
            tags: vec![],
            folder: "".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Use through trait object
        store
            .store_embedding("doc.md", "content", &[0.5; 384], &metadata)
            .await
            .unwrap();

        let result = store.get_embedding("doc.md").await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_in_memory_store_search() {
        let store = InMemoryKilnStore::new();

        // Store multiple documents
        let metadata1 = EmbeddingMetadata {
            file_path: "doc1.md".to_string(),
            title: Some("First".to_string()),
            tags: vec!["rust".to_string()],
            folder: "".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let metadata2 = EmbeddingMetadata {
            file_path: "doc2.md".to_string(),
            title: Some("Second".to_string()),
            tags: vec!["rust".to_string(), "async".to_string()],
            folder: "".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        store
            .store_embedding("doc1.md", "content1", &[1.0, 0.0, 0.0], &metadata1)
            .await
            .unwrap();

        store
            .store_embedding("doc2.md", "content2", &[0.9, 0.1, 0.0], &metadata2)
            .await
            .unwrap();

        // Search by tags
        let results = store
            .search_by_tags(&["rust".to_string()])
            .await
            .unwrap();
        assert_eq!(results.len(), 2);

        let results = store
            .search_by_tags(&["async".to_string()])
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "doc2.md");

        // Semantic search
        let query_embedding = vec![1.0, 0.0, 0.0];
        let results = store
            .search_similar("query", &query_embedding, 2)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        // First result should be doc1 (exact match)
        assert_eq!(results[0].id, "doc1.md");
        assert!(results[0].score > 0.9);
    }

    #[tokio::test]
    async fn test_in_memory_store_reset() {
        let store = InMemoryKilnStore::new();

        let metadata = EmbeddingMetadata {
            file_path: "test.md".to_string(),
            title: Some("Test".to_string()),
            tags: vec![],
            folder: "".to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        store
            .store_embedding("test.md", "content", &[0.1; 100], &metadata)
            .await
            .unwrap();

        assert_eq!(store.len(), 1);

        // Reset
        store.reset().unwrap();
        assert!(store.is_empty());
        assert!(!store.file_exists("test.md").await.unwrap());
    }
}
