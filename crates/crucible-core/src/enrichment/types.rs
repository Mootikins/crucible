//! Enrichment Domain Types
//!
//! Core types for the enrichment layer. These are domain models that should
//! be used across all layers of the application.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A fully enriched note ready for storage
///
/// Contains the original parsed AST plus enrichment data:
/// - Vector embeddings (for changed blocks only)
/// - Extracted metadata (word counts, language, etc.)
#[derive(Debug, Clone)]
pub struct EnrichedNote {
    /// Original parsed note with AST
    pub parsed: crate::parser::ParsedNote,

    /// Vector embeddings for blocks (only changed blocks)
    pub embeddings: Vec<BlockEmbedding>,

    /// Extracted and computed metadata
    pub metadata: EnrichmentMetadata,
}

impl EnrichedNote {
    /// Create a new enriched note
    pub fn new(
        parsed: crate::parser::ParsedNote,
        embeddings: Vec<BlockEmbedding>,
        metadata: EnrichmentMetadata,
    ) -> Self {
        Self {
            parsed,
            embeddings,
            metadata,
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

    /// Version of the model (e.g., "q8_0" for quantized, or API version)
    pub model_version: Option<String>,

    /// Dimensions of the vector
    pub dimensions: usize,

    /// BLAKE3 hash of the input text (for incremental embedding)
    pub content_hash: Option<String>,

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
            model_version: None,
            dimensions,
            content_hash: None,
            created_at: Utc::now(),
        }
    }

    /// Create a new block embedding with content hash for incremental embedding
    ///
    /// The content hash enables embedding reuse: if the same content hash
    /// is found for the same model+version, the cached embedding can be used.
    pub fn with_content_hash(
        block_id: String,
        vector: Vec<f32>,
        model: String,
        model_version: Option<String>,
        content_hash: String,
    ) -> Self {
        let dimensions = vector.len();
        Self {
            block_id,
            vector,
            model,
            model_version,
            dimensions,
            content_hash: Some(content_hash),
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

    /// Get the model version
    pub fn model_version(&self) -> Option<&str> {
        self.model_version.as_deref()
    }

    /// Get the content hash (BLAKE3 hash of input text)
    pub fn content_hash(&self) -> Option<&str> {
        self.content_hash.as_deref()
    }

    /// Get the dimensions
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }
}

/// Metadata computed during the enrichment phase
///
/// Contains only metadata computed during enrichment, not structural
/// metadata extracted during parsing (which lives in `ParsedNoteMetadata`).
///
/// This follows industry standard separation:
/// - Parser: Structural metrics (word count, element counts)
/// - Enrichment: Computed metrics (complexity, reading time, analysis)
///
/// For file-level metadata (name, path, tags), see `crucible_core::traits::NoteInfo`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnrichmentMetadata {
    /// Estimated reading time in minutes (computed from word count)
    pub reading_time_minutes: f32,

    /// Content complexity score (0.0-1.0) computed from AST structure
    pub complexity_score: f32,

    /// Detected language (if applicable, computed from content)
    pub language: Option<String>,

    /// When this metadata was computed
    pub computed_at: DateTime<Utc>,
}

impl Default for EnrichmentMetadata {
    fn default() -> Self {
        Self::new()
    }
}

impl EnrichmentMetadata {
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
}
