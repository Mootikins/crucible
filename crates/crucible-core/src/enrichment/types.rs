//! Enrichment Domain Types
//!
//! Core types for the enrichment layer. These are domain models that should
//! be used across all layers of the application.

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
    pub parsed: crate::parser::ParsedNote,

    /// Merkle tree computed from AST (for future change detection)
    // merkle_tree moved to infrastructure layer (crucible-enrichment) to avoid circular deps

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
        parsed: crate::parser::ParsedNote,
        // merkle_tree: moved to EnrichedNoteWithTree in crucible-enrichment
        embeddings: Vec<BlockEmbedding>,
        metadata: NoteMetadata,
        inferred_relations: Vec<InferredRelation>,
    ) -> Self {
        Self {
            parsed,
            // merkle_tree,
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

    /// Get total word count from parsed note metadata
    pub fn word_count(&self) -> usize {
        self.parsed.metadata.word_count
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

    /// Get the block ID
    pub fn block_id(&self) -> &str {
        &self.block_id
    }

    /// Get the vector
    pub fn vector(&self) -> &[f32] {
        &self.vector
    }

    /// Get the model name
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Get the dimensions
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }
}

/// Metadata extracted or computed from a note
///
/// Contains only metadata computed during enrichment phase, not structural
/// metadata extracted during parsing (which lives in `ParsedNoteMetadata`).
///
/// This follows industry standard separation:
/// - Parser: Structural metrics (word count, element counts)
/// - Enrichment: Computed metrics (complexity, reading time, analysis)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NoteMetadata {
    /// Estimated reading time in minutes (computed from word count)
    pub reading_time_minutes: f32,

    /// Content complexity score (0.0-1.0) computed from AST structure
    pub complexity_score: f32,

    /// Detected language (if applicable, computed from content)
    pub language: Option<String>,

    /// When this metadata was computed
    pub computed_at: DateTime<Utc>,
}

impl Default for NoteMetadata {
    fn default() -> Self {
        Self::new()
    }
}

impl NoteMetadata {
    /// Create new metadata with defaults
    pub fn new() -> Self {
        Self {
            reading_time_minutes: 0.0,
            complexity_score: 0.0,
            language: None,
            computed_at: Utc::now(),
        }
    }

    /// Compute reading time from word count (in minutes)
    ///
    /// Assumes average reading speed of 200 words per minute
    pub fn compute_reading_time(word_count: usize) -> f32 {
        const WORDS_PER_MINUTE: f32 = 200.0;
        word_count as f32 / WORDS_PER_MINUTE
    }

    /// Compute complexity score from AST element counts
    ///
    /// Returns value between 0.0 (simple) and 1.0 (complex)
    pub fn compute_complexity(
        heading_count: usize,
        code_block_count: usize,
        list_count: usize,
        latex_count: usize,
    ) -> f32 {
        // Simple heuristic: normalize by expected maximum counts
        let heading_score = (heading_count as f32 / 20.0).min(1.0);
        let code_score = (code_block_count as f32 / 10.0).min(1.0);
        let list_score = (list_count as f32 / 15.0).min(1.0);
        let latex_score = (latex_count as f32 / 5.0).min(1.0);

        // Weighted average
        (heading_score * 0.2 + code_score * 0.3 + list_score * 0.2 + latex_score * 0.3).min(1.0)
    }
}

/// An inferred relation between notes or blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferredRelation {
    /// Source block ID
    pub source_block_id: String,

    /// Target block ID
    pub target_block_id: String,

    /// Type of relation (similarity, clustering, etc.)
    pub relation_type: RelationType,

    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,

    /// When this relation was inferred
    pub inferred_at: DateTime<Utc>,
}

/// Type of inferred relation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RelationType {
    /// Semantic similarity based on embeddings
    SemanticSimilarity,

    /// Cluster membership (blocks in same semantic cluster)
    ClusterMembership,

    /// Topic relation (shared topics/concepts)
    TopicRelation,

    /// Custom relation type
    Custom(String),
}

impl InferredRelation {
    /// Create a new inferred relation
    pub fn new(
        source_block_id: String,
        target_block_id: String,
        relation_type: RelationType,
        confidence: f64,
    ) -> Self {
        Self {
            source_block_id,
            target_block_id,
            relation_type,
            confidence: confidence.clamp(0.0, 1.0),
            inferred_at: Utc::now(),
        }
    }

    /// Create a semantic similarity relation
    pub fn semantic_similarity(
        source_block_id: String,
        target_block_id: String,
        confidence: f64,
    ) -> Self {
        Self::new(
            source_block_id,
            target_block_id,
            RelationType::SemanticSimilarity,
            confidence,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_embedding_creation() {
        let embedding = BlockEmbedding::new(
            "block-1".to_string(),
            vec![0.1, 0.2, 0.3],
            "test-model".to_string(),
        );

        assert_eq!(embedding.block_id(), "block-1");
        assert_eq!(embedding.vector(), &[0.1, 0.2, 0.3]);
        assert_eq!(embedding.model(), "test-model");
        assert_eq!(embedding.dimensions(), 3);
    }

    #[test]
    fn test_inferred_relation_confidence_clamping() {
        let relation = InferredRelation::new(
            "a".to_string(),
            "b".to_string(),
            RelationType::SemanticSimilarity,
            1.5, // Over 1.0
        );

        assert_eq!(relation.confidence, 1.0);

        let relation2 = InferredRelation::new(
            "a".to_string(),
            "b".to_string(),
            RelationType::SemanticSimilarity,
            -0.5, // Under 0.0
        );

        assert_eq!(relation2.confidence, 0.0);
    }

    #[test]
    fn test_note_metadata_default() {
        let metadata = NoteMetadata::default();
        assert_eq!(metadata.reading_time_minutes, 0.0);
        assert_eq!(metadata.complexity_score, 0.0);
        assert_eq!(metadata.language, None);
    }
}
