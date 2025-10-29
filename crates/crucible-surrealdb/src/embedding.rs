//! Vector Embedding Module
//!
//! This module provides comprehensive vector embedding functionality for the Crucible
//! knowledge management system. It includes thread pool management, document processing
//! pipelines, and database integration for semantic search and retrieval.

// Re-export key types and functions from other embedding modules
pub use crate::embedding_config::{
    DocumentEmbedding, EmbeddingConfig, EmbeddingError, EmbeddingModel, EmbeddingProcessingResult,
    PrivacyMode, ThreadPoolMetrics,
};
pub use crate::embedding_pipeline::EmbeddingPipeline;
pub use crate::embedding_pool::EmbeddingThreadPool;

// Re-export from kiln_integration for embedding-specific functions
pub use crate::kiln_integration::{
    clear_document_embeddings, document_exists, get_database_stats, get_document_embeddings,
    semantic_search, store_document_embedding, update_document_content,
    update_document_processed_timestamp, DatabaseStats,
};
