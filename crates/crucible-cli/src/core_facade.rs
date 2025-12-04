//! Kiln Context
//!
//! Provides the runtime context for interacting with a Kiln (knowledge base).
//! This includes storage access, semantic search, and configuration.

use anyhow::{anyhow, Result};
use crucible_surrealdb::adapters::SurrealClientHandle;
use std::path::Path;
use std::sync::Arc;

use crate::config::CliConfig;

/// Runtime context for interacting with a Kiln
///
/// Provides access to:
/// - Storage (SurrealDB via opaque handle)
/// - Semantic search capabilities
/// - Configuration
#[derive(Clone)]
pub struct KilnContext {
    storage: SurrealClientHandle,
    config: Arc<CliConfig>,
}

impl KilnContext {
    /// Create a new facade from configuration
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
            storage,
            config: Arc::new(config),
        })
    }

    /// Create a facade from an existing storage client
    ///
    /// This constructor reuses an existing storage connection instead of creating a new one.
    /// Useful when the storage client is already initialized elsewhere (e.g., for pipeline processing).
    pub fn from_storage(storage: SurrealClientHandle, config: CliConfig) -> Self {
        Self {
            storage,
            config: Arc::new(config),
        }
    }

    /// Get a reference to the storage client handle
    pub fn storage(&self) -> &SurrealClientHandle {
        &self.storage
    }

    /// Get a reference to the configuration
    pub fn config(&self) -> &CliConfig {
        &self.config
    }

    /// Get the kiln root path
    pub fn kiln_root(&self) -> &Path {
        &self.config.kiln_path
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
        // Get embedding config from composite config and convert to provider config
        let embedding_config = self.config.embedding.to_provider_config();

        // Create embedding provider using factory function
        let provider = crucible_llm::embeddings::create_provider(embedding_config).await?;

        // Perform search using kiln_integration
        let results = crucible_surrealdb::kiln_integration::semantic_search(
            self.storage.inner(),
            query,
            limit,
            provider,
        )
        .await?;

        // Convert to facade result type
        // Note: kiln_integration returns (doc_id, similarity) tuples
        // We extract title from doc_id and leave snippet empty for now
        Ok(results
            .into_iter()
            .map(|(doc_id, similarity)| {
                // Extract a title from the doc_id
                let title = doc_id
                    .split('/')
                    .last()
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
        // Get embedding config from composite config and convert to provider config
        let embedding_config = self.config.embedding.to_provider_config();

        // Create embedding provider using factory function
        let provider = crucible_llm::embeddings::create_provider(embedding_config).await?;

        // Perform search with reranking
        let results = crucible_surrealdb::kiln_integration::semantic_search_with_reranking(
            self.storage.inner(),
            query,
            rerank_limit, // initial_limit (candidates to retrieve)
            None,         // reranker (None for now, TODO: add reranker support)
            limit,        // final_limit (results to return)
            provider,     // embedding_provider
        )
        .await?;

        // Convert to facade result type
        Ok(results
            .into_iter()
            .map(|(doc_id, similarity)| {
                let title = doc_id
                    .split('/')
                    .last()
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
            .collect())
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
