//! Test utilities for embedding generation and validation
//!
//! Provides helpers for:
//! - Loading pre-generated semantic corpus
//! - Creating mock and real embedding providers
//! - Batch embedding operations
//! - Test data builders

use crate::fixtures::semantic_corpus::{
    DocumentCategory, DocumentMetadata, SemanticTestCorpus, TestDocument,
};
use anyhow::Result;
use crucible_llm::embeddings::{
    EmbeddingConfig, EmbeddingProvider, EmbeddingResponse, OllamaProvider,
};
use std::sync::Arc;

// ============================================================================
// Corpus Loading
// ============================================================================

/// Load the pre-generated semantic corpus from JSON
///
/// This corpus contains 11 documents with real embeddings (768 dims) from Ollama.
/// Use this for deterministic tests without calling external APIs.
///
/// # Example
///
/// ```rust,ignore
/// let corpus = load_semantic_corpus().unwrap();
/// assert_eq!(corpus.documents.len(), 11);
/// assert_eq!(corpus.metadata.dimensions, 768);
/// ```
pub fn load_semantic_corpus() -> Result<SemanticTestCorpus> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let corpus_path = std::path::Path::new(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join("corpus_v1.json");

    let json = std::fs::read_to_string(&corpus_path)?;
    let corpus: SemanticTestCorpus = serde_json::from_str(&json)?;

    Ok(corpus)
}

/// Get a document from the corpus by ID
///
/// # Example
///
/// ```rust,ignore
/// let corpus = load_semantic_corpus().unwrap();
/// let doc = get_corpus_document(&corpus, "rust_fn_add").unwrap();
/// assert_eq!(doc.id, "rust_fn_add");
/// assert!(doc.embedding.is_some());
/// ```
pub fn get_corpus_document<'a>(
    corpus: &'a SemanticTestCorpus,
    id: &str,
) -> Option<&'a TestDocument> {
    corpus.documents.iter().find(|doc| doc.id == id)
}

/// Extract embeddings from corpus documents
///
/// Returns a vector of (id, embedding) pairs for documents with embeddings.
///
/// # Example
///
/// ```rust,ignore
/// let corpus = load_semantic_corpus().unwrap();
/// let embeddings = extract_corpus_embeddings(&corpus);
/// assert_eq!(embeddings.len(), 11);
/// ```
pub fn extract_corpus_embeddings(corpus: &SemanticTestCorpus) -> Vec<(String, Vec<f32>)> {
    corpus
        .documents
        .iter()
        .filter_map(|doc| {
            doc.embedding
                .as_ref()
                .map(|emb| (doc.id.clone(), emb.clone()))
        })
        .collect()
}

// ============================================================================
// Provider Creation
// ============================================================================

/// Create a mock embedding provider for testing
///
/// Returns deterministic embeddings without calling external APIs.
/// Use this for fast unit tests.
///
/// # Arguments
///
/// * `dimensions` - Embedding dimensions (default: 768)
///
/// # Example
///
/// ```rust,ignore
/// let provider = create_mock_provider(768);
/// let response = provider.embed("test").await.unwrap();
/// assert_eq!(response.dimensions, 768);
/// ```
pub fn create_mock_provider(dimensions: usize) -> Arc<dyn EmbeddingProvider> {
    use crucible_llm::embeddings::mock::MockEmbeddingProvider;
    Arc::new(MockEmbeddingProvider::with_dimensions(dimensions))
}

/// Create a real Ollama provider for integration tests
///
/// Uses environment variables for configuration:
/// - `EMBEDDING_ENDPOINT`: Default = "http://localhost:11434"
/// - `EMBEDDING_MODEL`: Default = "nomic-embed-text"
///
/// # Example
///
/// ```rust,ignore
/// let provider = create_ollama_provider().await.unwrap();
/// let response = provider.embed("test").await.unwrap();
/// println!("Generated {} dim embedding", response.dimensions);
/// ```
pub async fn create_ollama_provider() -> Result<Arc<dyn EmbeddingProvider>> {
    let endpoint = std::env::var("EMBEDDING_ENDPOINT").ok();
    let model = std::env::var("EMBEDDING_MODEL").ok();

    let config = EmbeddingConfig::ollama(endpoint, model);
    let provider = OllamaProvider::new(config)?;

    // Verify provider is healthy
    provider.health_check().await?;

    Ok(Arc::new(provider))
}

// ============================================================================
// Embedding Utilities
// ============================================================================

/// Batch embed multiple texts using a provider
///
/// Convenience wrapper around `embed_batch` with progress logging.
///
/// # Example
///
/// ```rust,ignore
/// let provider = create_mock_provider(768);
/// let texts = vec!["text1".to_string(), "text2".to_string()];
/// let embeddings = batch_embed(&provider, texts).await.unwrap();
/// assert_eq!(embeddings.len(), 2);
/// ```
pub async fn batch_embed(
    provider: &Arc<dyn EmbeddingProvider>,
    texts: Vec<String>,
) -> Result<Vec<EmbeddingResponse>> {
    println!("Embedding {} documents...", texts.len());
    let responses = provider.embed_batch(texts).await?;
    println!("✓ Generated {} embeddings", responses.len());
    Ok(responses)
}

// ============================================================================
// Test Data Builders
// ============================================================================

/// Builder for creating test documents with embeddings
///
/// # Example
///
/// ```rust,ignore
/// let provider = create_mock_provider(768);
/// let doc = TestDocumentBuilder::new()
///     .id("test_doc")
///     .content("Test content")
///     .with_embedding(&provider)
///     .await
///     .unwrap()
///     .build();
///
/// assert!(doc.embedding.is_some());
/// assert_eq!(doc.embedding.unwrap().len(), 768);
/// ```
pub struct TestDocumentBuilder {
    id: String,
    content: String,
    category: DocumentCategory,
    metadata: DocumentMetadata,
    embedding: Option<Vec<f32>>,
}

impl TestDocumentBuilder {
    pub fn new() -> Self {
        Self {
            id: String::new(),
            content: String::new(),
            category: DocumentCategory::Code,
            metadata: DocumentMetadata {
                language: None,
                token_count: 0,
                tags: vec![],
                description: String::new(),
            },
            embedding: None,
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    pub fn category(mut self, category: DocumentCategory) -> Self {
        self.category = category;
        self
    }

    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.metadata.tags = tags;
        self
    }

    /// Generate embedding using a provider
    pub async fn with_embedding(
        mut self,
        provider: &Arc<dyn EmbeddingProvider>,
    ) -> Result<Self> {
        let response = provider.embed(&self.content).await?;
        self.embedding = Some(response.embedding);
        self.metadata.token_count = response.tokens.unwrap_or(0);
        Ok(self)
    }

    /// Use a pre-computed embedding
    pub fn with_precomputed_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    pub fn build(self) -> TestDocument {
        TestDocument {
            id: self.id,
            content: self.content,
            category: self.category,
            metadata: self.metadata,
            embedding: self.embedding,
        }
    }
}

impl Default for TestDocumentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Provider Strategies
// ============================================================================

/// Strategy for selecting embedding provider in tests
#[derive(Debug, Clone, Copy)]
pub enum EmbeddingStrategy {
    /// Use mock provider (fast, deterministic)
    Mock,

    /// Use real Ollama provider (requires running server)
    Ollama,

    /// Auto-detect: use Ollama if available, fallback to mock
    Auto,
}

impl EmbeddingStrategy {
    /// Create a provider based on the strategy
    pub async fn create_provider(&self, dimensions: usize) -> Result<Arc<dyn EmbeddingProvider>> {
        match self {
            Self::Mock => Ok(create_mock_provider(dimensions)),
            Self::Ollama => create_ollama_provider().await,
            Self::Auto => {
                // Try Ollama first, fallback to mock
                match create_ollama_provider().await {
                    Ok(provider) => {
                        println!("✓ Using real Ollama provider");
                        Ok(provider)
                    }
                    Err(e) => {
                        println!("⚠ Ollama unavailable ({}), using mock provider", e);
                        Ok(create_mock_provider(dimensions))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_semantic_corpus() {
        let corpus = load_semantic_corpus().expect("Failed to load corpus");
        assert_eq!(corpus.documents.len(), 11);
        assert_eq!(corpus.metadata.dimensions, 768);
        assert_eq!(corpus.metadata.model, "nomic-embed-text-v1.5-q8_0");
    }

    #[test]
    fn test_get_corpus_document() {
        let corpus = load_semantic_corpus().unwrap();

        let doc = get_corpus_document(&corpus, "rust_fn_add");
        assert!(doc.is_some());
        assert_eq!(doc.unwrap().id, "rust_fn_add");

        let missing = get_corpus_document(&corpus, "nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_extract_corpus_embeddings() {
        let corpus = load_semantic_corpus().unwrap();
        let embeddings = extract_corpus_embeddings(&corpus);

        assert_eq!(embeddings.len(), 11);

        for (id, embedding) in embeddings {
            assert!(!id.is_empty());
            assert_eq!(embedding.len(), 768);
        }
    }

    #[tokio::test]
    async fn test_mock_provider() {
        let provider = create_mock_provider(768);

        let response = provider.embed("test content").await.unwrap();

        assert_eq!(response.dimensions, 768);
        assert_eq!(response.embedding.len(), 768);
        assert_eq!(response.model, "mock-test-model");
    }

    #[tokio::test]
    async fn test_batch_embed_mock() {
        let provider = create_mock_provider(768);
        let texts = vec![
            "text1".to_string(),
            "text2".to_string(),
            "text3".to_string(),
        ];

        let embeddings = batch_embed(&provider, texts).await.unwrap();

        assert_eq!(embeddings.len(), 3);
        for emb in embeddings {
            assert_eq!(emb.dimensions, 768);
        }
    }

    #[tokio::test]
    async fn test_document_builder() {
        let provider = create_mock_provider(768);

        let doc = TestDocumentBuilder::new()
            .id("test_doc")
            .content("Test content")
            .category(DocumentCategory::Code)
            .tags(vec!["test".to_string()])
            .with_embedding(&provider)
            .await
            .unwrap()
            .build();

        assert_eq!(doc.id, "test_doc");
        assert_eq!(doc.content, "Test content");
        assert!(doc.embedding.is_some());
        assert_eq!(doc.embedding.unwrap().len(), 768);
    }

    #[tokio::test]
    async fn test_document_builder_precomputed() {
        let embedding = vec![0.5; 768];

        let doc = TestDocumentBuilder::new()
            .id("test_doc")
            .content("Test content")
            .with_precomputed_embedding(embedding.clone())
            .build();

        assert!(doc.embedding.is_some());
        assert_eq!(doc.embedding.unwrap(), embedding);
    }

    #[tokio::test]
    async fn test_embedding_strategy_mock() {
        let strategy = EmbeddingStrategy::Mock;
        let provider = strategy.create_provider(768).await.unwrap();

        let response = provider.embed("test").await.unwrap();
        assert_eq!(response.dimensions, 768);
    }

    #[tokio::test]
    async fn test_embedding_strategy_auto_fallback() {
        // Auto should fallback to mock when Ollama unavailable
        let strategy = EmbeddingStrategy::Auto;
        let provider = strategy.create_provider(768).await.unwrap();

        // Try to embed - should work with either mock or Ollama
        let response = provider.embed("test").await;

        match response {
            Ok(resp) => {
                // Ollama is available and working
                assert!(resp.dimensions > 0);
            }
            Err(_) => {
                // Ollama failed at embed time, try mock directly
                let mock_provider = create_mock_provider(768);
                let mock_response = mock_provider.embed("test").await.unwrap();
                assert_eq!(mock_response.dimensions, 768);
            }
        }
    }
}
