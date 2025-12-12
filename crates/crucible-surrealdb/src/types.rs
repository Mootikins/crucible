//! Multi-Model Database Abstractions
//!
//! This module re-exports database abstractions from crucible-core and adds
//! SurrealDB-specific types for configuration and embeddings.
//!
//! ## Type Ownership
//!
//! - **Common types** (DbError, Record, Note, etc.): Canonical definitions in `crucible-core::database`
//! - **SurrealDB-specific types** (SurrealDbConfig, EmbeddingDocument, etc.): Defined here
//!
//! This follows the Dependency Inversion Principle - core defines abstractions,
//! infrastructure crates add implementation-specific types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ==============================================================================
// RE-EXPORTS FROM CRUCIBLE-CORE
// ==============================================================================
// These types are canonically defined in crucible-core::database.
// We re-export them here for convenience and backward compatibility.

pub use crucible_core::database::{
    // Relational types
    AggregateFunction,
    AggregateQuery,
    AggregateType,
    // Document types
    AggregationPipeline,
    AggregationResult,
    AggregationStage,
    // Graph types
    AnalyticsResult,
    BatchResult,
    ColumnDefinition,
    CommunityAlgorithm,
    DataType,
    // Error types
    DbError,
    DbResult,
    Direction,
    // Traits
    DocumentDB,
    DocumentFieldType,
    DocumentFilter,
    DocumentId,
    DocumentMetadata,
    DocumentQuery,
    DocumentSchema,
    DocumentSort,
    DocumentUpdates,
    Edge,
    EdgeFilter,
    EdgeId,
    EdgePattern,
    EdgeProperties,
    FieldDefinition,
    FilterClause,
    ForeignKey,
    GraphAnalysis,
    GraphDB,
    GroupOperation,
    IndexDefinition,
    IndexType,
    JoinClause,
    JoinQuery,
    JoinType,
    Node,
    NodeId,
    NodePattern,
    NodeProperties,
    Note,
    OrderClause,
    OrderDirection,
    Path,
    QueryResult,
    Record,
    RecordId,
    ReferentialAction,
    RelationalDB,
    SearchIndexOptions,
    SearchOptions,
    SearchResult,
    SelectQuery,
    Subgraph,
    SubgraphPattern,
    TableSchema,
    TransactionId,
    TraversalPattern,
    TraversalResult,
    TraversalStep,
    UpdateClause,
    ValidationRules,
};

// Re-export common types from crucible-core
pub use crucible_core::note::NoteNode;
pub use crucible_core::properties::{AttributeValue, PropertyMap};

// ==============================================================================
// SURREALDB-SPECIFIC CONFIGURATION
// ==============================================================================

/// Configuration for SurrealDB connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurrealDbConfig {
    /// Namespace to use
    pub namespace: String,
    /// Database name
    pub database: String,
    /// Path to database file (or ":memory:" for in-memory)
    pub path: String,
    /// Maximum number of connections in pool
    pub max_connections: Option<u32>,
    /// Timeout in seconds for operations
    pub timeout_seconds: Option<u32>,
}

impl Default for SurrealDbConfig {
    fn default() -> Self {
        Self {
            namespace: "crucible".to_string(),
            database: "kiln".to_string(),
            path: "./data/kiln.db".to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        }
    }
}

impl SurrealDbConfig {
    /// Create an in-memory database configuration (useful for testing)
    pub fn memory() -> Self {
        Self {
            path: ":memory:".to_string(),
            ..Default::default()
        }
    }
}

// ==============================================================================
// SURREALDB-SPECIFIC EMBEDDING TYPES
// ==============================================================================
// These types are specific to the SurrealDB embedding storage implementation.
// They support the SurrealEmbeddingDatabase and will be deprecated once we
// fully migrate to the new EAV+Graph schema.

/// Embedding metadata for documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingMetadata {
    pub file_path: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub folder: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Embedding document with all fields needed for vector operations.
/// This is the SurrealDB-specific document type that includes embeddings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingDocument {
    pub id: String,
    pub file_path: String,
    pub title: Option<String>,
    pub content: String,
    pub embedding: Vec<f32>,
    pub tags: Vec<String>,
    pub folder: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Embedding data with content and vector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingData {
    pub file_path: String,
    pub content: String,
    pub embedding: Vec<f32>,
    pub metadata: EmbeddingMetadata,
}

impl From<EmbeddingDocument> for EmbeddingData {
    fn from(doc: EmbeddingDocument) -> Self {
        EmbeddingData {
            file_path: doc.file_path.clone(),
            content: doc.content,
            embedding: doc.embedding,
            metadata: EmbeddingMetadata {
                file_path: doc.file_path,
                title: doc.title,
                tags: doc.tags,
                folder: doc.folder,
                properties: doc.properties,
                created_at: doc.created_at,
                updated_at: doc.updated_at,
            },
        }
    }
}

/// Document embedding with chunking metadata (legacy type for kiln_integration compatibility)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEmbedding {
    pub document_id: String,
    pub vector: Vec<f32>,
    pub embedding_model: String,
    pub chunk_id: Option<String>,
    pub chunk_size: usize,
    pub chunk_position: Option<usize>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl DocumentEmbedding {
    pub fn new(document_id: String, vector: Vec<f32>, embedding_model: String) -> Self {
        Self {
            document_id,
            vector,
            embedding_model,
            chunk_id: None,
            chunk_size: 0,
            chunk_position: None,
            created_at: chrono::Utc::now(),
        }
    }
}

/// Search query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub filters: Option<serde_json::Value>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Search result with similarity score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultWithScore {
    pub id: String,
    pub title: String,
    pub file_path: String,
    pub content: String,
    pub score: f64,
    pub metadata: EmbeddingMetadata,
}

/// Batch operation type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchOperationType {
    Create,
    Update,
    Delete,
}

/// Search filters for embedding queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFilters {
    pub tags: Option<Vec<String>>,
    pub folder: Option<String>,
    pub properties: Option<HashMap<String, serde_json::Value>>,
}

/// Batch operation with embedding documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperation {
    pub operation_type: BatchOperationType,
    pub documents: Vec<EmbeddingDocument>,
}

/// Database statistics
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseStats {
    pub total_documents: u64,
    pub total_embeddings: u64,
    pub storage_size_bytes: u64,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}
