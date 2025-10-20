// crates/crucible-daemon/tests/fixtures/semantic_corpus.rs

//! Semantic test corpus for embedding and search testing

use serde::{Deserialize, Serialize};

/// A single test document with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestDocument {
    /// Unique identifier for the document
    pub id: String,

    /// The text content to be embedded
    pub content: String,

    /// Category for organizing test cases
    pub category: DocumentCategory,

    /// Optional metadata for test assertions
    pub metadata: DocumentMetadata,

    /// Pre-generated embedding (768 dimensions for nomic-embed-text-v1.5)
    /// Stored as Option to allow lazy loading
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

/// Categories for organizing test documents
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentCategory {
    /// Programming concepts and code snippets
    Code,

    /// Technical documentation
    Documentation,

    /// Natural language prose (non-technical)
    Prose,

    /// Mixed content (code + comments + prose)
    Mixed,

    /// Edge cases (empty, very long, special chars)
    EdgeCase,
}

/// Metadata for test assertions and analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Programming language (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Approximate token count
    pub token_count: usize,

    /// Domain/topic tags
    pub tags: Vec<String>,

    /// Human-readable description
    pub description: String,
}

/// Expected similarity relationship between two documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarityExpectation {
    /// ID of first document
    pub doc_a: String,

    /// ID of second document
    pub doc_b: String,

    /// Expected similarity range
    pub expected_range: SimilarityRange,

    /// Human-readable explanation of why this relationship exists
    pub rationale: String,
}

/// Similarity range definitions aligned with semantic search thresholds
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SimilarityRange {
    /// Cosine similarity > 0.7 (effectively same concept)
    High { min: f32, max: f32 },

    /// Cosine similarity 0.4-0.7 (related but distinct)
    Medium { min: f32, max: f32 },

    /// Cosine similarity < 0.3 (unrelated)
    Low { min: f32, max: f32 },
}

impl SimilarityRange {
    pub const HIGH: Self = Self::High { min: 0.7, max: 1.0 };
    pub const MEDIUM: Self = Self::Medium { min: 0.3, max: 0.7 };
    pub const LOW: Self = Self::Low { min: 0.0, max: 0.3 };

    /// Check if a cosine similarity value falls within this range
    pub fn contains(&self, similarity: f32) -> bool {
        let (min, max) = match self {
            Self::High { min, max } => (*min, *max),
            Self::Medium { min, max } => (*min, *max),
            Self::Low { min, max } => (*min, *max),
        };
        similarity >= min && similarity <= max
    }

    /// Get a descriptive name for this range
    pub fn name(&self) -> &'static str {
        match self {
            Self::High { .. } => "high",
            Self::Medium { .. } => "medium",
            Self::Low { .. } => "low",
        }
    }
}

/// The complete test corpus with documents and expected relationships
#[derive(Debug, Serialize, Deserialize)]
pub struct SemanticTestCorpus {
    /// All test documents
    pub documents: Vec<TestDocument>,

    /// Expected similarity relationships
    pub expectations: Vec<SimilarityExpectation>,

    /// Metadata about the corpus generation
    pub metadata: CorpusMetadata,
}

/// Metadata about when and how the corpus was generated
#[derive(Debug, Serialize, Deserialize)]
pub struct CorpusMetadata {
    /// Model used to generate embeddings
    pub model: String,

    /// Endpoint used
    pub endpoint: String,

    /// Generation timestamp
    pub generated_at: String,

    /// Version of the corpus schema
    pub schema_version: u32,

    /// Embedding dimensions
    pub dimensions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_similarity_range_contains() {
        let high = SimilarityRange::HIGH;
        assert!(high.contains(0.8));
        assert!(high.contains(0.7));
        assert!(!high.contains(0.69));

        let medium = SimilarityRange::MEDIUM;
        assert!(medium.contains(0.5));
        assert!(medium.contains(0.3)); // MEDIUM now starts at 0.3
        assert!(medium.contains(0.4));
        assert!(!medium.contains(0.29));

        let low = SimilarityRange::LOW;
        assert!(low.contains(0.2));
        assert!(low.contains(0.0));
        assert!(low.contains(0.3)); // 0.3 is in both LOW and MEDIUM
        assert!(!low.contains(0.31));
    }

    #[test]
    fn test_similarity_range_names() {
        assert_eq!(SimilarityRange::HIGH.name(), "high");
        assert_eq!(SimilarityRange::MEDIUM.name(), "medium");
        assert_eq!(SimilarityRange::LOW.name(), "low");
    }

    #[test]
    fn test_document_serialization() {
        let doc = TestDocument {
            id: "test_doc".to_string(),
            content: "test content".to_string(),
            category: DocumentCategory::Code,
            metadata: DocumentMetadata {
                language: Some("rust".to_string()),
                token_count: 10,
                tags: vec!["test".to_string()],
                description: "A test document".to_string(),
            },
            embedding: Some(vec![0.1, 0.2, 0.3]),
        };

        let json = serde_json::to_string(&doc).unwrap();
        let deserialized: TestDocument = serde_json::from_str(&json).unwrap();

        assert_eq!(doc.id, deserialized.id);
        assert_eq!(doc.content, deserialized.content);
        assert_eq!(doc.embedding, deserialized.embedding);
    }
}
