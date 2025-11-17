//! Core Facade Pattern
//!
//! Provides a clean, trait-based interface between the CLI and core functionality.
//! This facade pattern simplifies testing and maintains separation of concerns.

use anyhow::{anyhow, Result};
use crucible_surrealdb::{kiln_integration, SurrealClient};
use std::path::Path;
use std::sync::Arc;

use crate::config::CliConfig;

/// Main facade for accessing Crucible core functionality
///
/// This struct provides a clean interface to the underlying systems:
/// - Storage (SurrealDB)
/// - Semantic search
/// - Configuration
#[derive(Clone)]
pub struct CrucibleCoreFacade {
    storage: Arc<SurrealClient>,
    config: Arc<CliConfig>,
}

impl CrucibleCoreFacade {
    /// Create a new facade from configuration
    pub async fn from_config(config: CliConfig) -> Result<Self> {
        // Initialize storage
        let storage_config = crucible_surrealdb::SurrealDbConfig {
            path: config.database_path_str()?,
            namespace: "crucible".to_string(),
            database: "kiln".to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };

        let storage = SurrealClient::new(storage_config)
            .await
            .map_err(|e| anyhow!("Failed to create storage client: {}", e))?;

        // Initialize schema
        kiln_integration::initialize_kiln_schema(&storage).await?;

        Ok(Self {
            storage: Arc::new(storage),
            config: Arc::new(config),
        })
    }

    /// Get a reference to the storage client
    pub fn storage(&self) -> &SurrealClient {
        &self.storage
    }

    /// Get a reference to the configuration
    pub fn config(&self) -> &CliConfig {
        &self.config
    }

    /// Get the kiln root path
    pub fn kiln_root(&self) -> &Path {
        &self.config.kiln.path
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
        // Get embedding config
        let embedding_config = self.config.to_embedding_config()?;

        // Create embedding provider
        let provider = crucible_llm::embeddings::create_provider(embedding_config).await?;

        // Perform search using kiln_integration
        let results = kiln_integration::semantic_search(
            &self.storage,
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
        // Get embedding config
        let embedding_config = self.config.to_embedding_config()?;

        // Create embedding provider
        let provider = crucible_llm::embeddings::create_provider(embedding_config).await?;

        // Perform search with reranking
        let results = kiln_integration::semantic_search_with_reranking(
            &self.storage,
            query,
            rerank_limit,   // initial_limit (candidates to retrieve)
            None,           // reranker (None for now, TODO: add reranker support)
            limit,          // final_limit (results to return)
            provider,       // embedding_provider
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
