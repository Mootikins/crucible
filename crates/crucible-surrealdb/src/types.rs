//! Type definitions for the SurrealDB backend implementation.
//! These types mirror the interface used by the DuckDB backend to ensure
//! compatibility while providing SurrealDB-specific optimizations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Metadata for embeddings stored in SurrealDB
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

/// Embedding data with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingData {
    pub file_path: String,
    pub content: String,
    pub embedding: Vec<f32>,
    pub metadata: EmbeddingMetadata,
}

/// Search result with similarity score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultWithScore {
    pub id: String,
    pub title: String,
    pub content: String,
    pub score: f64,
}

/// Document record stored in SurrealDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
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

impl From<Document> for EmbeddingData {
    fn from(doc: Document) -> Self {
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

/// Database statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    pub total_documents: i64,
    pub total_embeddings: i64,
    pub storage_size_bytes: Option<i64>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// Search query parameters for advanced searches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub filters: Option<SearchFilters>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Search filters for refining results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFilters {
    pub tags: Option<Vec<String>>,
    pub folder: Option<String>,
    pub properties: Option<HashMap<String, serde_json::Value>>,
    pub date_range: Option<DateRange>,
}

/// Date range filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub start: Option<chrono::DateTime<chrono::Utc>>,
    pub end: Option<chrono::DateTime<chrono::Utc>>,
}

/// Batch operation for processing multiple documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperation {
    pub operation_type: BatchOperationType,
    pub documents: Vec<Document>,
}

/// Types of batch operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BatchOperationType {
    Create,
    Update,
    Delete,
}

/// Result of batch operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    pub successful: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

/// Configuration for the SurrealDB backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurrealDbConfig {
    pub namespace: String,
    pub database: String,
    pub path: String,
    pub max_connections: Option<u32>,
    pub timeout_seconds: Option<u64>,
}

impl Default for SurrealDbConfig {
    fn default() -> Self {
        Self {
            namespace: "crucible".to_string(),
            database: "cache".to_string(),
            path: "./crucible.db".to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        }
    }
}