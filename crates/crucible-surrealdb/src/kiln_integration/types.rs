//! Kiln integration types
//!
//! Type definitions for kiln integration layer.

/// Metadata describing the current embedding index.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingIndexMetadata {
    pub model: Option<String>,
    pub dimensions: Option<usize>,
}

/// Embed metadata for a note
#[derive(Debug, Clone, PartialEq)]
pub struct EmbedMetadata {
    pub target: String,
    pub is_embed: bool,
    pub heading_ref: Option<String>,
    pub block_ref: Option<String>,
    pub alias: Option<String>,
    pub position: usize,
}

/// Link relation information
#[derive(Debug, Clone, PartialEq)]
pub struct LinkRelation {
    pub relation_type: String,
    pub is_embed: bool,
    pub target: String,
}

/// Embed relation information
#[derive(Debug, Clone, PartialEq)]
pub struct EmbedRelation {
    pub relation_type: String,
    pub is_embed: bool,
    pub target: String,
    pub embed_type: String,
}

/// Embedding data for batch storage
pub struct EmbeddingData {
    pub vector: Vec<f32>,
    pub model: String,
    pub block_id: String,
    pub dimensions: usize,
}

/// Cached embedding result for incremental embedding lookups
#[derive(Debug, Clone)]
pub struct CachedEmbedding {
    /// The embedding vector
    pub vector: Vec<f32>,
    /// Model name used to generate this embedding
    pub model: String,
    /// Model version (e.g., "q8_0" for quantized models)
    pub model_version: String,
    /// Content hash (BLAKE3) of the input text
    pub content_hash: String,
    /// Vector dimensions
    pub dimensions: usize,
}
