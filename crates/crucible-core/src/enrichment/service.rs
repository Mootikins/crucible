//! Enrichment Service Trait
//!
//! Defines the contract for enrichment operations following the Dependency
//! Inversion Principle. Core defines the abstraction, infrastructure provides
//! concrete implementations.

use super::types::{EnrichedNote, InferredRelation};
// use crucible_merkle::HybridMerkleTree;
use crate::parser::ParsedNote;
use anyhow::Result;

/// Trait for enrichment services
///
/// This trait defines the contract for enriching parsed notes with embeddings,
/// metadata, and inferred relations. Implementations are provided in the
/// infrastructure layer (crucible-enrichment crate).
///
/// ## Dependency Inversion
///
/// By defining this trait in the core domain layer, we ensure that:
/// - High-level modules don't depend on low-level modules
/// - Both depend on abstractions (this trait)
/// - Easy to swap implementations or add new enrichment strategies
///
#[async_trait::async_trait]
pub trait EnrichmentService: Send + Sync {
    /// Enrich a parsed note with embeddings, metadata, and relations
    ///
    /// # Arguments
    ///
    /// * `parsed` - The parsed note with AST
    /// * `changed_block_ids` - List of block IDs that have changed (only these get re-embedded)
    ///
    /// # Returns
    ///
    /// An `EnrichedNote` containing:
    /// - Original parsed AST
    /// - Merkle tree (for change detection)
    /// - Vector embeddings (for changed blocks)
    /// - Extracted metadata
    /// - Inferred relations
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Embedding generation fails
    /// - Merkle tree construction fails
    /// - Other enrichment operations fail
    ///
    /// The implementation should handle partial failures gracefully where possible.
    async fn enrich(
        &self,
        parsed: ParsedNote,
        changed_block_ids: Vec<String>,
    ) -> Result<EnrichedNote>;

    /// Enrich a parsed note with Merkle tree provided (optimization)
    ///
    /// This variant allows passing a pre-computed Merkle tree to avoid
    /// recomputation. Useful when the Merkle tree was already built during
    /// change detection.
    ///
    /// # Arguments
    ///
    /// * `parsed` - The parsed note with AST
    /// * `merkle_tree` - Pre-computed Merkle tree
    /// * `changed_block_ids` - List of block IDs that have changed
    ///
    /// # Returns
    ///
    /// An `EnrichedNote` with the provided Merkle tree and new enrichment data.
    async fn enrich_with_tree(
        &self,
        parsed: ParsedNote,
        // merkle_tree: HybridMerkleTree,
        changed_block_ids: Vec<String>,
    ) -> Result<EnrichedNote>;

    /// Infer relations between blocks based on embeddings and metadata
    ///
    /// This is separated out for flexibility - some implementations may want to
    /// run relation inference in a background job rather than inline.
    ///
    /// # Arguments
    ///
    /// * `enriched` - An already-enriched note with embeddings
    /// * `threshold` - Confidence threshold for relation inference (0.0 to 1.0)
    ///
    /// # Returns
    ///
    /// A list of inferred relations between blocks
    async fn infer_relations(
        &self,
        enriched: &EnrichedNote,
        threshold: f64,
    ) -> Result<Vec<InferredRelation>>;

    /// Get the minimum word count for embedding generation
    ///
    /// Blocks with fewer words than this threshold will not be embedded
    /// (to avoid noise from very short blocks).
    fn min_words_for_embedding(&self) -> usize;

    /// Get the maximum batch size for embedding operations
    ///
    /// Limits the number of blocks processed in a single batch to prevent
    /// memory issues and improve latency.
    fn max_batch_size(&self) -> usize;

    /// Check if embedding provider is configured
    ///
    /// Returns `true` if embeddings will be generated, `false` if
    /// only metadata and relations will be computed.
    fn has_embedding_provider(&self) -> bool;
}

/// Builder for enrichment service configuration
///
/// Implementations can use this pattern to configure their behavior
/// while maintaining the trait contract.
pub struct EnrichmentConfig {
    /// Minimum word count for embedding generation
    pub min_words_for_embedding: usize,

    /// Maximum batch size for embedding operations
    pub max_batch_size: usize,

    /// Whether to enable relation inference
    pub enable_relation_inference: bool,

    /// Default confidence threshold for relation inference
    pub relation_confidence_threshold: f64,
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            min_words_for_embedding: 5,
            max_batch_size: 10,
            enable_relation_inference: false,
            relation_confidence_threshold: 0.7,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enrichment_config_defaults() {
        let config = EnrichmentConfig::default();
        assert_eq!(config.min_words_for_embedding, 5);
        assert_eq!(config.max_batch_size, 10);
        assert!(!config.enable_relation_inference);
        assert_eq!(config.relation_confidence_threshold, 0.7);
    }
}
