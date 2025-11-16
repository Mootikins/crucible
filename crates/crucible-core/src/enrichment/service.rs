//! Enrichment Service
//!
//! Orchestrates all enrichment operations including embedding generation,
//! metadata extraction, and relation inference. Follows clean architecture
//! principles with dependency injection.

use crate::enrichment::{
    BlockEmbedding, EnrichedNote, InferredRelation, InferredRelationType, NoteMetadata,
};
use crate::merkle::HybridMerkleTree;
use crate::types::ParsedNote;
use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Trait for embedding providers (to be implemented by infrastructure)
///
/// This trait will be implemented by crucible-llm providers (Fastembed, OpenAI, etc.)
/// For now, we define a simplified version here for the core layer.
/// In production, this should reference the existing trait from crucible-llm.
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text string
    async fn embed_text(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed multiple texts in a batch (more efficient)
    async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>>;

    /// Get the model name
    fn model_name(&self) -> &str;

    /// Get the embedding dimensions
    fn dimensions(&self) -> usize;
}

/// Service that orchestrates all enrichment operations
///
/// Receives a ParsedNote and list of changed blocks, then coordinates:
/// - Embedding generation (only for changed blocks)
/// - Metadata extraction (word counts, language, etc.)
/// - Relation inference (semantic similarity, clustering)
///
/// Returns an EnrichedNote ready for storage.
pub struct EnrichmentService {
    /// Embedding provider (dependency injected)
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,

    /// Minimum word count for embedding generation
    min_words_for_embedding: usize,
}

impl EnrichmentService {
    /// Create a new enrichment service with an embedding provider
    pub fn new(embedding_provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            embedding_provider: Some(embedding_provider),
            min_words_for_embedding: 5,
        }
    }

    /// Create an enrichment service without embeddings (metadata/relations only)
    pub fn without_embeddings() -> Self {
        Self {
            embedding_provider: None,
            min_words_for_embedding: 5,
        }
    }

    /// Set the minimum word count for embedding generation
    pub fn with_min_words(mut self, min_words: usize) -> Self {
        self.min_words_for_embedding = min_words;
        self
    }

    /// Enrich a parsed note with all available enrichments
    ///
    /// # Arguments
    /// * `parsed` - The parsed note with AST
    /// * `merkle_tree` - The Merkle tree (already computed from AST)
    /// * `changed_blocks` - List of block IDs that changed (from Merkle diff)
    ///
    /// # Returns
    /// An EnrichedNote with embeddings, metadata, and inferred relations
    pub async fn enrich(
        &self,
        parsed: ParsedNote,
        merkle_tree: HybridMerkleTree,
        changed_blocks: Vec<String>,
    ) -> Result<EnrichedNote> {
        info!(
            "Enriching note: {} ({} changed blocks)",
            parsed.path.display(),
            changed_blocks.len()
        );

        // Run enrichment operations in parallel
        let (embeddings, metadata, relations) = tokio::join!(
            self.generate_embeddings(&parsed, &changed_blocks),
            self.extract_metadata(&parsed),
            self.infer_relations(&parsed),
        );

        Ok(EnrichedNote::new(
            parsed,
            merkle_tree,
            embeddings?,
            metadata?,
            relations?,
        ))
    }

    /// Generate embeddings for changed blocks only
    async fn generate_embeddings(
        &self,
        parsed: &ParsedNote,
        changed_blocks: &[String],
    ) -> Result<Vec<BlockEmbedding>> {
        // If no embedding provider, return empty
        let Some(provider) = &self.embedding_provider else {
            debug!("No embedding provider configured, skipping embeddings");
            return Ok(Vec::new());
        };

        // For now, we'll extract text from changed blocks
        // TODO: Integrate with actual block structure from ParsedNote
        let block_texts = self.extract_block_texts(parsed, changed_blocks);

        if block_texts.is_empty() {
            debug!("No blocks meet embedding criteria (min {} words)", self.min_words_for_embedding);
            return Ok(Vec::new());
        }

        info!("Generating embeddings for {} blocks", block_texts.len());

        // Prepare texts for batch embedding
        let texts: Vec<&str> = block_texts.iter().map(|(_, text)| text.as_str()).collect();

        // Batch embed
        let vectors = provider.embed_batch(texts).await?;

        // Package results as BlockEmbedding
        let embeddings: Vec<BlockEmbedding> = block_texts
            .iter()
            .zip(vectors)
            .map(|((block_id, _text), vector)| {
                BlockEmbedding::new(
                    block_id.clone(),
                    vector,
                    provider.model_name().to_string(),
                )
            })
            .collect();

        info!("Generated {} embeddings using {}", embeddings.len(), provider.model_name());

        Ok(embeddings)
    }

    /// Extract block texts that meet embedding criteria
    ///
    /// TODO: This is a placeholder. In production, this should traverse the
    /// ParsedNote's content structure and extract actual block content.
    fn extract_block_texts(
        &self,
        _parsed: &ParsedNote,
        changed_blocks: &[String],
    ) -> Vec<(String, String)> {
        // Placeholder implementation
        // In production: traverse parsed.content, extract text from each block
        // Filter by word count (>= min_words_for_embedding)
        // Return list of (block_id, text) tuples

        debug!(
            "Extracting texts from {} changed blocks (placeholder implementation)",
            changed_blocks.len()
        );

        // For now, return empty - will be implemented when integrating with parser
        Vec::new()
    }

    /// Extract metadata from the parsed note
    async fn extract_metadata(&self, parsed: &ParsedNote) -> Result<NoteMetadata> {
        debug!("Extracting metadata from {}", parsed.path.display());

        let mut metadata = NoteMetadata::new();

        // TODO: Implement actual metadata extraction
        // This should:
        // - Count words per block and total
        // - Detect language
        // - Estimate reading time
        // - Calculate complexity score

        // Placeholder values
        metadata.total_word_count = 0;
        metadata.language = Some("en".to_string());
        metadata.reading_time_minutes = 0.0;
        metadata.complexity_score = 0.5;

        debug!("Metadata extracted: {} words", metadata.total_word_count);

        Ok(metadata)
    }

    /// Infer relations based on content analysis
    async fn infer_relations(&self, parsed: &ParsedNote) -> Result<Vec<InferredRelation>> {
        debug!("Inferring relations for {}", parsed.path.display());

        // TODO: Implement relation inference
        // This could include:
        // - Semantic similarity to other notes (if we have embeddings stored)
        // - Topic clustering
        // - Temporal proximity
        // - Structural similarity

        // For now, return empty - will be implemented in later phases
        let relations = Vec::new();

        debug!("Inferred {} relations", relations.len());

        Ok(relations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Mock embedding provider for testing
    struct MockEmbeddingProvider {
        model: String,
        dimensions: usize,
    }

    impl MockEmbeddingProvider {
        fn new() -> Self {
            Self {
                model: "mock-model".to_string(),
                dimensions: 3,
            }
        }
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed_text(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3])
        }

        async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
        }

        fn model_name(&self) -> &str {
            &self.model
        }

        fn dimensions(&self) -> usize {
            self.dimensions
        }
    }

    #[tokio::test]
    async fn test_enrichment_service_with_provider() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = EnrichmentService::new(provider);

        assert!(service.embedding_provider.is_some());
        assert_eq!(service.min_words_for_embedding, 5);
    }

    #[tokio::test]
    async fn test_enrichment_service_without_provider() {
        let service = EnrichmentService::without_embeddings();

        assert!(service.embedding_provider.is_none());
    }

    #[tokio::test]
    async fn test_enrichment_service_with_custom_min_words() {
        let provider = Arc::new(MockEmbeddingProvider::new());
        let service = EnrichmentService::new(provider).with_min_words(10);

        assert_eq!(service.min_words_for_embedding, 10);
    }

    #[tokio::test]
    async fn test_generate_embeddings_without_provider() {
        let service = EnrichmentService::without_embeddings();

        // Create a minimal ParsedNote for testing
        let parsed = create_test_parsed_note();

        let embeddings = service
            .generate_embeddings(&parsed, &vec!["block_1".to_string()])
            .await
            .unwrap();

        // Should return empty when no provider
        assert_eq!(embeddings.len(), 0);
    }

    #[tokio::test]
    async fn test_extract_metadata() {
        let service = EnrichmentService::without_embeddings();
        let parsed = create_test_parsed_note();

        let metadata = service.extract_metadata(&parsed).await.unwrap();

        // Should create metadata with default values
        assert_eq!(metadata.language, Some("en".to_string()));
        assert_eq!(metadata.complexity_score, 0.5);
    }

    #[tokio::test]
    async fn test_infer_relations() {
        let service = EnrichmentService::without_embeddings();
        let parsed = create_test_parsed_note();

        let relations = service.infer_relations(&parsed).await.unwrap();

        // Should return empty for now (placeholder implementation)
        assert_eq!(relations.len(), 0);
    }

    /// Helper to create a minimal ParsedNote for testing
    fn create_test_parsed_note() -> ParsedNote {
        use crucible_parser::ParsedNoteBuilder;

        ParsedNoteBuilder::new(PathBuf::from("/test/note.md")).build()
    }
}
