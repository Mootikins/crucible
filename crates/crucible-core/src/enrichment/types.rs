//! Enrichment Types
//!
//! Core types for the enrichment layer including EnrichedNote, BlockEmbedding,
//! and metadata structures.

use crate::types::ParsedNote;
use crate::merkle::HybridMerkleTree;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A fully enriched note ready for storage
///
/// Contains the original parsed AST plus all enrichment data:
/// - Vector embeddings (for changed blocks only)
/// - Merkle tree (for change detection)
/// - Extracted metadata (word counts, language, etc.)
/// - Inferred relations (similarity, clustering, etc.)
#[derive(Debug, Clone)]
pub struct EnrichedNote {
    /// Original parsed note with AST
    pub parsed: ParsedNote,

    /// Merkle tree computed from AST (for future change detection)
    pub merkle_tree: HybridMerkleTree,

    /// Vector embeddings for blocks (only changed blocks)
    pub embeddings: Vec<BlockEmbedding>,

    /// Extracted and computed metadata
    pub metadata: NoteMetadata,

    /// Inferred relations (semantic similarity, etc.)
    pub inferred_relations: Vec<InferredRelation>,
}

impl EnrichedNote {
    /// Create a new enriched note
    pub fn new(
        parsed: ParsedNote,
        merkle_tree: HybridMerkleTree,
        embeddings: Vec<BlockEmbedding>,
        metadata: NoteMetadata,
        inferred_relations: Vec<InferredRelation>,
    ) -> Self {
        Self {
            parsed,
            merkle_tree,
            embeddings,
            metadata,
            inferred_relations,
        }
    }

    /// Get the note path
    pub fn path(&self) -> &std::path::Path {
        &self.parsed.path
    }

    /// Get a note ID from the path (for convenience)
    pub fn id(&self) -> String {
        // Use the file stem as ID for now
        // In production, this might come from frontmatter or a hash
        self.parsed
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Get the number of embeddings
    pub fn embedding_count(&self) -> usize {
        self.embeddings.len()
    }

    /// Get total word count from metadata
    pub fn word_count(&self) -> usize {
        self.metadata.total_word_count
    }
}

/// Vector embedding for a single block
#[derive(Debug, Clone, PartialEq)]
pub struct BlockEmbedding {
    /// Block ID this embedding corresponds to
    pub block_id: String,

    /// The vector embedding values
    pub vector: Vec<f32>,

    /// Name of the model used to generate this embedding
    pub model: String,

    /// Dimensions of the vector
    pub dimensions: usize,

    /// When this embedding was generated
    pub created_at: DateTime<Utc>,
}

impl BlockEmbedding {
    /// Create a new block embedding
    pub fn new(block_id: String, vector: Vec<f32>, model: String) -> Self {
        let dimensions = vector.len();
        Self {
            block_id,
            vector,
            model,
            dimensions,
            created_at: Utc::now(),
        }
    }

    /// Calculate vector magnitude (L2 norm)
    pub fn magnitude(&self) -> f32 {
        self.vector.iter().map(|x| x * x).sum::<f32>().sqrt()
    }

    /// Normalize the vector to unit length
    pub fn normalize(&self) -> Vec<f32> {
        let mag = self.magnitude();
        if mag == 0.0 {
            self.vector.clone()
        } else {
            self.vector.iter().map(|x| x / mag).collect()
        }
    }

    /// Calculate cosine similarity with another embedding
    pub fn cosine_similarity(&self, other: &BlockEmbedding) -> Option<f32> {
        if self.dimensions != other.dimensions {
            return None;
        }

        let dot_product: f32 = self
            .vector
            .iter()
            .zip(&other.vector)
            .map(|(a, b)| a * b)
            .sum();

        let magnitude_product = self.magnitude() * other.magnitude();

        if magnitude_product == 0.0 {
            None
        } else {
            Some(dot_product / magnitude_product)
        }
    }
}

/// Metadata extracted and computed during enrichment
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteMetadata {
    /// Total word count across all blocks
    pub total_word_count: usize,

    /// Word count per block
    pub block_word_counts: Vec<(String, usize)>, // (block_id, count)

    /// Detected language (if applicable)
    pub language: Option<String>,

    /// Estimated reading time in minutes
    pub reading_time_minutes: f32,

    /// Content complexity score (0.0-1.0)
    pub complexity_score: f32,

    /// When this metadata was computed
    pub computed_at: DateTime<Utc>,
}

impl NoteMetadata {
    /// Create new metadata with defaults
    pub fn new() -> Self {
        Self {
            total_word_count: 0,
            block_word_counts: Vec::new(),
            language: None,
            reading_time_minutes: 0.0,
            complexity_score: 0.0,
            computed_at: Utc::now(),
        }
    }

    /// Calculate average words per block
    pub fn avg_words_per_block(&self) -> f32 {
        if self.block_word_counts.is_empty() {
            0.0
        } else {
            self.total_word_count as f32 / self.block_word_counts.len() as f32
        }
    }
}

impl Default for NoteMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// An inferred relation between notes or blocks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferredRelation {
    /// Source note or block ID
    pub source_id: String,

    /// Target note or block ID
    pub target_id: String,

    /// Type of inferred relation
    pub relation_type: InferredRelationType,

    /// Confidence score (0.0-1.0)
    pub confidence: f32,

    /// When this relation was inferred
    pub inferred_at: DateTime<Utc>,
}

impl InferredRelation {
    /// Create a new inferred relation
    pub fn new(
        source_id: String,
        target_id: String,
        relation_type: InferredRelationType,
        confidence: f32,
    ) -> Self {
        Self {
            source_id,
            target_id,
            relation_type,
            confidence: confidence.clamp(0.0, 1.0),
            inferred_at: Utc::now(),
        }
    }

    /// Check if this relation meets a minimum confidence threshold
    pub fn meets_threshold(&self, threshold: f32) -> bool {
        self.confidence >= threshold
    }
}

/// Types of inferred relations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InferredRelationType {
    /// Semantic similarity based on embeddings
    SemanticSimilarity,

    /// Topical clustering
    SameTopic,

    /// Temporal proximity (created/modified around same time)
    TemporalProximity,

    /// Structural similarity (similar heading structure, etc.)
    StructuralSimilarity,

    /// Other inferred relation type
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_embedding_creation() {
        let embedding = BlockEmbedding::new(
            "block_1".to_string(),
            vec![0.1, 0.2, 0.3],
            "test-model".to_string(),
        );

        assert_eq!(embedding.block_id, "block_1");
        assert_eq!(embedding.dimensions, 3);
        assert_eq!(embedding.model, "test-model");
        assert!(embedding.magnitude() > 0.0);
    }

    #[test]
    fn test_embedding_normalization() {
        let embedding = BlockEmbedding::new(
            "block_1".to_string(),
            vec![3.0, 4.0],
            "test-model".to_string(),
        );

        let normalized = embedding.normalize();
        let magnitude: f32 = normalized.iter().map(|x| x * x).sum::<f32>().sqrt();

        // Normalized vector should have magnitude ~1.0
        assert!((magnitude - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity() {
        let emb1 = BlockEmbedding::new(
            "block_1".to_string(),
            vec![1.0, 0.0, 0.0],
            "test-model".to_string(),
        );

        let emb2 = BlockEmbedding::new(
            "block_2".to_string(),
            vec![1.0, 0.0, 0.0],
            "test-model".to_string(),
        );

        let emb3 = BlockEmbedding::new(
            "block_3".to_string(),
            vec![0.0, 1.0, 0.0],
            "test-model".to_string(),
        );

        // Identical vectors should have similarity 1.0
        let sim1 = emb1.cosine_similarity(&emb2).unwrap();
        assert!((sim1 - 1.0).abs() < 0.001);

        // Orthogonal vectors should have similarity 0.0
        let sim2 = emb1.cosine_similarity(&emb3).unwrap();
        assert!(sim2.abs() < 0.001);
    }

    #[test]
    fn test_note_metadata() {
        let mut metadata = NoteMetadata::new();
        metadata.total_word_count = 100;
        metadata.block_word_counts = vec![
            ("block_1".to_string(), 50),
            ("block_2".to_string(), 50),
        ];

        assert_eq!(metadata.avg_words_per_block(), 50.0);
    }

    #[test]
    fn test_inferred_relation() {
        let relation = InferredRelation::new(
            "note_1".to_string(),
            "note_2".to_string(),
            InferredRelationType::SemanticSimilarity,
            0.85,
        );

        assert!(relation.meets_threshold(0.8));
        assert!(!relation.meets_threshold(0.9));
    }

    #[test]
    fn test_inferred_relation_confidence_clamping() {
        let relation = InferredRelation::new(
            "note_1".to_string(),
            "note_2".to_string(),
            InferredRelationType::SemanticSimilarity,
            1.5, // Should be clamped to 1.0
        );

        assert_eq!(relation.confidence, 1.0);

        let relation2 = InferredRelation::new(
            "note_1".to_string(),
            "note_2".to_string(),
            InferredRelationType::SemanticSimilarity,
            -0.5, // Should be clamped to 0.0
        );

        assert_eq!(relation2.confidence, 0.0);
    }
}
