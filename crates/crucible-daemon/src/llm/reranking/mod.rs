//! Reranking functionality for improving search result quality.
//!
//! This module provides reranking capabilities that can be applied after initial
//! vector search to improve result relevance. Rerankers use cross-attention models
//! that better understand query-document relationships compared to vector similarity.

#[cfg(feature = "fastembed")]
pub mod fastembed;

#[cfg(feature = "fastembed")]
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
