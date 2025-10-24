//! Vector Embedding Module
//!
//! This module provides comprehensive vector embedding functionality for the Crucible
//! knowledge management system. It includes thread pool management, document processing
//! pipelines, and database integration for semantic search and retrieval.

// Re-export key types and functions from other embedding modules
pub use crate::embedding_config::{
    EmbeddingConfig, EmbeddingModel, PrivacyMode, EmbeddingProcessingResult,
    DocumentEmbedding, EmbeddingError, ThreadPoolMetrics
};
pub use crate::embedding_pool::EmbeddingThreadPool;
pub use crate::embedding_pipeline::EmbeddingPipeline;

// Re-export from vault_integration for embedding-specific functions
pub use crate::vault_integration::{
    store_document_embedding,
    get_document_embeddings,
    clear_document_embeddings,
    update_document_processed_timestamp,
    update_document_content,
    document_exists,
    get_database_stats,
    semantic_search,
    DatabaseStats,
};