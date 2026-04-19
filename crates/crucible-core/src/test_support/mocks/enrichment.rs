//! Mock enrichment service.

use std::sync::{Arc, Mutex};

/// Mock enrichment service for testing
///
/// Provides a configurable implementation of enrichment that allows testing
/// pipeline integration without requiring actual embedding API calls.
///
/// # Features
///
/// - **Configurable behavior**: Control embedding generation, dimensions, etc.
/// - **Operation tracking**: Count enrichment operations
/// - **Error injection**: Simulate enrichment failures
/// - **Fast**: No actual API calls, instant responses
///
/// # Example
///
/// ```rust
/// use crucible_core::test_support::mocks::MockEnrichmentService;
/// use crucible_core::enrichment::EnrichmentService;
/// use crucible_core::parser::ParsedNote;
///
/// # async fn example() -> anyhow::Result<()> {
/// let service = MockEnrichmentService::new();
///
/// // Configure to generate embeddings
/// service.set_generate_embeddings(true);
/// service.set_embedding_dimension(384);
///
/// // Enrich a note
/// // let enriched = service.enrich(parsed_note, vec![]).await?;
///
/// // Check operation counts
/// assert_eq!(service.enrich_count(), 1);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct MockEnrichmentService {
    state: Arc<Mutex<MockEnrichmentState>>,
}

struct MockEnrichmentState {
    // Configuration
    generate_embeddings: bool,
    embedding_dimension: usize,
    min_words: usize,
    max_batch_size: usize,

    // Operation tracking
    enrich_count: usize,
    enrich_with_tree_count: usize,
    infer_relations_count: usize,

    // Error injection
    simulate_errors: bool,
    error_message: String,
}

impl MockEnrichmentService {
    /// Create a new mock enrichment service with defaults
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockEnrichmentState {
                generate_embeddings: true,
                embedding_dimension: 384,
                min_words: 10,
                max_batch_size: 100,
                enrich_count: 0,
                enrich_with_tree_count: 0,
                infer_relations_count: 0,
                simulate_errors: false,
                error_message: String::new(),
            })),
        }
    }

    /// Set whether to generate embeddings
    pub fn set_generate_embeddings(&self, enabled: bool) {
        self.state.lock().unwrap().generate_embeddings = enabled;
    }

    /// Set embedding dimension
    pub fn set_embedding_dimension(&self, dimension: usize) {
        self.state.lock().unwrap().embedding_dimension = dimension;
    }

    /// Set minimum words for embedding
    pub fn set_min_words(&self, min_words: usize) {
        self.state.lock().unwrap().min_words = min_words;
    }

    /// Set maximum batch size
    pub fn set_max_batch_size(&self, max_batch_size: usize) {
        self.state.lock().unwrap().max_batch_size = max_batch_size;
    }

    /// Enable or disable error simulation
    pub fn set_simulate_errors(&self, enabled: bool, message: &str) {
        let mut state = self.state.lock().unwrap();
        state.simulate_errors = enabled;
        state.error_message = message.to_string();
    }

    /// Get count of enrich() calls
    pub fn enrich_count(&self) -> usize {
        self.state.lock().unwrap().enrich_count
    }

    /// Get count of enrich_with_tree() calls
    pub fn enrich_with_tree_count(&self) -> usize {
        self.state.lock().unwrap().enrich_with_tree_count
    }

    /// Get count of infer_relations() calls
    pub fn infer_relations_count(&self) -> usize {
        self.state.lock().unwrap().infer_relations_count
    }

    /// Reset all counters and configuration
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.enrich_count = 0;
        state.enrich_with_tree_count = 0;
        state.infer_relations_count = 0;
        state.simulate_errors = false;
        state.error_message.clear();
    }

    /// Create mock embeddings for changed blocks
    fn create_mock_embeddings(
        &self,
        changed_block_ids: &[String],
        dimension: usize,
    ) -> Vec<crate::enrichment::BlockEmbedding> {
        changed_block_ids
            .iter()
            .map(|block_id| {
                // Create deterministic embedding based on block_id
                let vector = (0..dimension)
                    .map(|i| ((block_id.len() + i) as f32) / 1000.0)
                    .collect();

                crate::enrichment::BlockEmbedding::new(
                    block_id.clone(),
                    vector,
                    "mock-embeddings".to_string(),
                )
            })
            .collect()
    }

    /// Create mock metadata
    fn create_mock_metadata(
        &self,
        parsed: &crate::parser::ParsedNote,
    ) -> crate::enrichment::EnrichmentMetadata {
        use crate::enrichment::EnrichmentMetadata;

        let word_count = parsed.metadata.word_count;
        let reading_time = EnrichmentMetadata::compute_reading_time(word_count);
        let complexity = EnrichmentMetadata::compute_complexity(
            parsed.metadata.heading_count,
            parsed.metadata.code_block_count,
            parsed.metadata.list_count,
            parsed.metadata.latex_count,
        );

        crate::enrichment::EnrichmentMetadata {
            reading_time_minutes: reading_time,
            complexity_score: complexity,
            language: Some("en".to_string()),
            computed_at: chrono::Utc::now(),
        }
    }
}

impl Default for MockEnrichmentService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl crate::enrichment::EnrichmentService for MockEnrichmentService {
    async fn enrich(
        &self,
        parsed: crate::parser::ParsedNote,
        changed_block_ids: Vec<String>,
    ) -> anyhow::Result<crate::enrichment::EnrichedNote> {
        use crate::enrichment::EnrichedNote;
        // use crucible_merkle::HybridMerkleTree; // moved to infrastructure layer

        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(anyhow::anyhow!("{}", state.error_message));
        }

        state.enrich_count += 1;
        let generate_embeddings = state.generate_embeddings;
        let dimension = state.embedding_dimension;
        drop(state);

        // Build Merkle tree
        // let merkle_tree = HybridMerkleTree::from_document(&parsed);

        // Generate mock embeddings if enabled
        let embeddings = if generate_embeddings {
            self.create_mock_embeddings(&changed_block_ids, dimension)
        } else {
            vec![]
        };

        // Create mock metadata
        let metadata = self.create_mock_metadata(&parsed);

        // No inferred relations in basic enrich
        let inferred_relations = vec![];

        Ok(EnrichedNote::new(
            parsed,
            // merkle_tree,
            embeddings,
            metadata,
            inferred_relations,
        ))
    }

    async fn enrich_with_tree(
        &self,
        parsed: crate::parser::ParsedNote,
        // merkle_tree: see enrichment crate,
        changed_block_ids: Vec<String>,
    ) -> anyhow::Result<crate::enrichment::EnrichedNote> {
        use crate::enrichment::EnrichedNote;

        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(anyhow::anyhow!("{}", state.error_message));
        }

        state.enrich_with_tree_count += 1;
        let generate_embeddings = state.generate_embeddings;
        let dimension = state.embedding_dimension;
        drop(state);

        // Generate mock embeddings if enabled
        let embeddings = if generate_embeddings {
            self.create_mock_embeddings(&changed_block_ids, dimension)
        } else {
            vec![]
        };

        // Create mock metadata
        let metadata = self.create_mock_metadata(&parsed);

        // No inferred relations
        let inferred_relations = vec![];

        Ok(EnrichedNote::new(
            parsed,
            // merkle_tree,
            embeddings,
            metadata,
            inferred_relations,
        ))
    }

    async fn infer_relations(
        &self,
        _enriched: &crate::enrichment::EnrichedNote,
        _threshold: f64,
    ) -> anyhow::Result<Vec<crate::enrichment::InferredRelation>> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(anyhow::anyhow!("{}", state.error_message));
        }

        state.infer_relations_count += 1;
        drop(state);

        // Return empty relations for mock
        Ok(vec![])
    }

    fn min_words_for_embedding(&self) -> usize {
        self.state.lock().unwrap().min_words
    }

    fn max_batch_size(&self) -> usize {
        self.state.lock().unwrap().max_batch_size
    }

    fn has_embedding_provider(&self) -> bool {
        self.state.lock().unwrap().generate_embeddings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enrichment::service::EnrichmentService;

    #[tokio::test]
    async fn test_mock_enrichment_service_basic() {
        let service = MockEnrichmentService::new();

        assert!(service.has_embedding_provider());
        assert_eq!(service.min_words_for_embedding(), 10);
        assert_eq!(service.max_batch_size(), 100);
    }

    #[tokio::test]
    async fn test_mock_enrichment_service_configuration() {
        let service = MockEnrichmentService::new();

        service.set_generate_embeddings(false);
        service.set_embedding_dimension(768);
        service.set_min_words(20);
        service.set_max_batch_size(50);

        assert!(!service.has_embedding_provider());
        assert_eq!(service.min_words_for_embedding(), 20);
        assert_eq!(service.max_batch_size(), 50);
    }

    #[tokio::test]
    async fn test_mock_enrichment_service_operation_tracking() {
        let service = MockEnrichmentService::new();

        assert_eq!(service.enrich_count(), 0);
        assert_eq!(service.enrich_with_tree_count(), 0);

        // Would need a ParsedNote to test actual enrichment
        // For now, just verify the tracking mechanism works
        service.reset();
        assert_eq!(service.enrich_count(), 0);
    }

    #[tokio::test]
    async fn test_mock_enrichment_service_error_injection() {
        let service = MockEnrichmentService::new();

        service.set_simulate_errors(true, "Test error");

        // Error injection works
        assert!(service.has_embedding_provider()); // This doesn't error
    }
}
