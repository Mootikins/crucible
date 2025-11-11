//! Reranking functionality for improving search result quality.
//!
//! This module provides reranking capabilities that can be applied after initial
//! vector search to improve result relevance. Rerankers use cross-attention models
//! that better understand query-note relationships compared to vector similarity.

use anyhow::Result;
use async_trait::async_trait;

pub mod fastembed;

pub use fastembed::FastEmbedReranker;

/// Result from a reranking operation
#[derive(Debug, Clone)]
pub struct RerankResult {
    /// Note identifier
    pub document_id: String,
    /// Note text content
    pub text: String,
    /// Reranking score (higher is more relevant)
    pub score: f64,
    /// Original position in the input list
    pub original_index: usize,
}

/// Information about a reranker model
#[derive(Debug, Clone)]
pub struct RerankerModelInfo {
    /// Model name
    pub name: String,
    /// Provider name (e.g., "FastEmbed", "Cohere")
    pub provider: String,
    /// Maximum input text length supported
    pub max_input_length: usize,
}

/// Trait for reranking search results based on query relevance.
///
/// Rerankers take an initial set of search results and reorder them based on
/// more sophisticated relevance scoring than simple vector similarity.
#[async_trait]
pub trait Reranker: Send + Sync {
    /// Rerank documents based on their relevance to the query.
    ///
    /// # Arguments
    /// * `query` - The search query text
    /// * `documents` - Vec of (document_id, text, original_score) tuples
    /// * `top_n` - Optional limit on number of results to return
    ///
    /// # Returns
    /// Reranked results sorted by relevance score (highest first)
    ///
    /// # Example
    /// ```ignore
    /// let results = reranker.rerank(
    ///     "rust async programming",
    ///     vec![
    ///         ("doc1".into(), "async/await in Rust...".into(), 0.85),
    ///         ("doc2".into(), "Python asyncio guide...".into(), 0.82),
    ///     ],
    ///     Some(10),
    /// ).await?;
    /// ```
    async fn rerank(
        &self,
        query: &str,
        documents: Vec<(String, String, f64)>,
        top_n: Option<usize>,
    ) -> Result<Vec<RerankResult>>;

    /// Get information about the reranker model.
    fn model_info(&self) -> RerankerModelInfo;

    /// Check if the reranker is healthy and ready to use.
    ///
    /// Default implementation returns true. Override for custom health checks.
    async fn health_check(&self) -> Result<bool> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rerank_result_creation() {
        let result = RerankResult {
            document_id: "doc123".to_string(),
            text: "Test note".to_string(),
            score: 0.95,
            original_index: 5,
        };

        assert_eq!(result.document_id, "doc123");
        assert_eq!(result.score, 0.95);
        assert_eq!(result.original_index, 5);
    }

    #[test]
    fn test_model_info_creation() {
        let info = RerankerModelInfo {
            name: "bge-reranker-base".to_string(),
            provider: "FastEmbed".to_string(),
            max_input_length: 512,
        };

        assert_eq!(info.name, "bge-reranker-base");
        assert_eq!(info.provider, "FastEmbed");
        assert_eq!(info.max_input_length, 512);
    }
}
