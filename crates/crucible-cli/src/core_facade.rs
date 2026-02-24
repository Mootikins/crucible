//! Kiln Context
//!
//! Provides the runtime context for interacting with a Kiln (knowledge base).
//! This includes storage access, semantic search, and configuration.

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::config::CliConfig;
use crate::factories::StorageHandle;

/// Runtime context for interacting with a Kiln
///
/// - Storage (daemon RPC)
/// - Semantic search
/// - Configuration
#[derive(Clone)]
pub struct KilnContext {
    /// Storage backend (daemon client via RPC)
    storage_handle: StorageHandle,
    config: Arc<CliConfig>,
}

impl KilnContext {
    /// Create a KilnContext from a StorageHandle.
    pub fn from_storage_handle(storage_handle: StorageHandle, config: CliConfig) -> Self {
        Self {
            storage_handle,
            config: Arc::new(config),
        }
    }

    /// Get a reference to the storage handle
    pub fn storage_handle(&self) -> &StorageHandle {
        &self.storage_handle
    }

    /// Get a reference to the configuration
    pub fn config(&self) -> &CliConfig {
        &self.config
    }

    /// Get the kiln root path
    pub fn kiln_root(&self) -> &Path {
        &self.config.kiln_path
    }

    /// Get the sessions folder path
    ///
    /// Returns `<kiln_root>/Sessions/` where chat sessions are stored.
    pub fn session_folder(&self) -> std::path::PathBuf {
        self.config.kiln_path.join("Sessions")
    }

    /// Perform semantic search
    ///
    /// # Arguments
    /// * `query` - The search query string
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    /// Vector of search results with document IDs and similarity scores
    pub async fn semantic_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SemanticSearchResult>> {
        use crucible_core::traits::KnowledgeRepository;

        tracing::debug!(
            "semantic_search called with query={:?}, limit={}",
            query,
            limit
        );

        // Get embedding config from composite config and convert to provider config
        let embedding_config =
            crate::factories::embedding_provider_config_from_cli(&self.config);
        tracing::debug!("embedding config: {:?}", embedding_config);

        // Create embedding provider using factory function
        let provider = crucible_llm::embeddings::create_provider(embedding_config)
            .await
            .map_err(|e| {
                tracing::error!("Failed to create embedding provider: {}", e);
                e
            })?;

        // Generate embedding for query
        tracing::debug!("Generating query embedding...");
        let query_embedding = provider.embed(query).await.map_err(|e| {
            tracing::error!("Failed to generate query embedding: {}", e);
            e
        })?;
        tracing::debug!(
            "Query embedding generated, dimensions={}",
            query_embedding.len()
        );

        let knowledge_repo = self.storage_handle.as_knowledge_repository();

        tracing::debug!("Searching vectors...");
        let results = knowledge_repo
            .search_vectors(query_embedding)
            .await
            .map_err(|e| {
                tracing::error!("Vector search failed: {}", e);
                e
            })?;
        tracing::debug!("Vector search returned {} raw results", results.len());

        // Convert to facade result type, respecting limit
        Ok(results
            .into_iter()
            .take(limit)
            .map(|result| {
                // Extract a title from the doc_id
                let title = result
                    .document_id
                    .0
                    .split('/')
                    .next_back()
                    .unwrap_or(&result.document_id.0)
                    .trim_end_matches(".md")
                    .to_string();

                SemanticSearchResult {
                    doc_id: result.document_id.0,
                    title,
                    snippet: result.snippet.unwrap_or_default(),
                    similarity: result.score as f32,
                }
            })
            .collect())
    }

    /// Perform semantic search with reranking for better results
    ///
    /// # Arguments
    /// * `query` - The search query string
    /// * `limit` - Maximum number of results to return
    /// * `rerank_limit` - Number of candidates to retrieve before reranking
    ///
    /// # Returns
    /// Vector of search results with document IDs and similarity scores
    pub async fn semantic_search_with_reranking(
        &self,
        query: &str,
        limit: usize,
        _rerank_limit: usize,
    ) -> Result<Vec<SemanticSearchResult>> {
        // Reranking not supported via RPC yet — fall back to basic search
        tracing::debug!(
            "Reranking not available in this storage mode, using basic semantic search"
        );
        self.semantic_search(query, limit).await
    }
}

/// Result from semantic search
#[derive(Debug, Clone)]
pub struct SemanticSearchResult {
    pub doc_id: String,
    pub title: String,
    pub snippet: String,
    pub similarity: f32,
}
