//! Kiln Context
//!
//! Provides the runtime context for interacting with a Kiln (knowledge base).
//! This includes storage access, semantic search, and configuration.

use anyhow::{anyhow, Result};
#[cfg(feature = "storage-surrealdb")]
use crucible_surrealdb::adapters::SurrealClientHandle;
use std::path::Path;
use std::sync::Arc;

use crate::config::CliConfig;
use crate::factories::StorageHandle;

/// Runtime context for interacting with a Kiln
///
/// Provides access to:
/// - Storage (SurrealDB via opaque handle or daemon RPC)
/// - Semantic search capabilities
/// - Configuration
#[derive(Clone)]
pub struct KilnContext {
    /// Storage backend - either embedded SurrealDB or daemon client
    storage_handle: StorageHandle,
    config: Arc<CliConfig>,
}

impl KilnContext {
    /// Create a new facade from configuration (requires SurrealDB)
    #[cfg(feature = "storage-surrealdb")]
    pub async fn from_config(config: CliConfig) -> Result<Self> {
        // Initialize storage using factory function
        let storage_config = crucible_surrealdb::SurrealDbConfig {
            path: config.database_path_str()?,
            namespace: "crucible".to_string(),
            database: "kiln".to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };

        let storage = crucible_surrealdb::adapters::create_surreal_client(storage_config)
            .await
            .map_err(|e| anyhow!("Failed to create storage client: {}", e))?;

        // Initialize schema
        crucible_surrealdb::kiln_integration::initialize_kiln_schema(storage.inner()).await?;

        Ok(Self {
            storage_handle: StorageHandle::Embedded(storage),
            config: Arc::new(config),
        })
    }

    /// Create a facade from an existing storage client (requires SurrealDB)
    ///
    /// This constructor reuses an existing storage connection instead of creating a new one.
    /// Useful when the storage client is already initialized elsewhere (e.g., for pipeline processing).
    #[cfg(feature = "storage-surrealdb")]
    pub fn from_storage(storage: SurrealClientHandle, config: CliConfig) -> Self {
        Self {
            storage_handle: StorageHandle::Embedded(storage),
            config: Arc::new(config),
        }
    }

    /// Create a facade from a StorageHandle (supports both embedded and daemon)
    ///
    /// This is the preferred constructor for daemon mode where we don't have
    /// direct access to the embedded SurrealDB client.
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

    /// Get a reference to the embedded storage client handle (if available)
    ///
    /// Returns None if running in daemon or lightweight mode.
    #[cfg(feature = "storage-surrealdb")]
    pub fn storage(&self) -> Option<&SurrealClientHandle> {
        self.storage_handle.try_embedded()
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
        let embedding_config = self.config.embedding.to_provider_config();
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
        let query_response = provider.embed(query).await.map_err(|e| {
            tracing::error!("Failed to generate query embedding: {}", e);
            e
        })?;
        let query_embedding = query_response.embedding;
        tracing::debug!(
            "Query embedding generated, dimensions={}",
            query_embedding.len()
        );

        // Use KnowledgeRepository trait for search (works with both embedded and daemon)
        let knowledge_repo = self
            .storage_handle
            .as_knowledge_repository()
            .ok_or_else(|| {
                tracing::error!("Knowledge repository not available (lightweight mode)");
                anyhow!("Semantic search not supported in lightweight mode")
            })?;

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
        rerank_limit: usize,
    ) -> Result<Vec<SemanticSearchResult>> {
        // Reranking requires embedded mode (direct SurrealDB access)
        // In daemon mode, fall back to basic semantic search
        #[cfg(feature = "storage-surrealdb")]
        if let Some(storage) = self.storage_handle.try_embedded() {
            // Get embedding config from composite config and convert to provider config
            let embedding_config = self.config.embedding.to_provider_config();

            // Create embedding provider using factory function
            let provider = crucible_llm::embeddings::create_provider(embedding_config).await?;

            // Perform search with reranking
            let results = crucible_surrealdb::kiln_integration::semantic_search_with_reranking(
                storage.inner(),
                query,
                rerank_limit, // initial_limit (candidates to retrieve)
                None,         // reranker (None for now, TODO: add reranker support)
                limit,        // final_limit (results to return)
                provider,     // embedding_provider
            )
            .await?;

            // Convert to facade result type
            return Ok(results
                .into_iter()
                .map(|(doc_id, similarity)| {
                    let title = doc_id
                        .split('/')
                        .next_back()
                        .unwrap_or(&doc_id)
                        .trim_end_matches(".md")
                        .to_string();

                    SemanticSearchResult {
                        doc_id,
                        title,
                        snippet: String::new(), // TODO: Extract snippet from document content
                        similarity: similarity as f32,
                    }
                })
                .collect());
        }

        {
            // Fall back to basic semantic search (daemon, sqlite, lightweight modes)
            // Reranking is not supported via RPC yet
            tracing::debug!("Reranking not available in this storage mode, using basic semantic search");
            self.semantic_search(query, limit).await
        }
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
