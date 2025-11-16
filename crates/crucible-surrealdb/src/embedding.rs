//! Embedding Storage Module
//!
//! Pure storage operations for vector embeddings. This module is intentionally
//! minimal - all embedding generation and enrichment logic lives in the
//! crucible-enrichment crate.
//!
//! ## Architecture
//!
//! Following clean architecture principles:
//! - This module: Pure I/O (store, retrieve, delete, search)
//! - crucible-enrichment: Business logic (generation, orchestration)
//! - Clear separation of concerns

// Re-export storage functions from kiln_integration
pub use crate::kiln_integration::{
    clear_document_embeddings,
    get_database_stats,
    get_document_embeddings,
    semantic_search,
    store_document_embedding,
};
