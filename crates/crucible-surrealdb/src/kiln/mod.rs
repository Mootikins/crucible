//! Kiln Integration Module
//!
//! This module provides the integration layer between the parser system and SurrealDB.
//! It implements embedding storage and semantic search support.
//!
//! ## Phase 4 Cleanup
//!
//! The document and relations modules have been removed.
//! Use NoteStore for note metadata storage instead.
//!
//! ## Module Structure
//!
//! - `embedding` - Embedding storage and retrieval
//! - `search` - Semantic search operations
//! - `stats` - Database statistics
//! - `utils` - Shared utility functions

pub mod embedding;
pub mod search;
pub mod stats;
pub mod utils;

// Re-export all public items for backwards compatibility
pub use embedding::*;
pub use search::*;
pub use stats::*;
pub use utils::{
    chunk_namespace, chunk_record_body, escape_record_id, generate_document_id, is_retryable_error,
    normalize_document_id, record_body, resolve_relative_path, INITIAL_BACKOFF_MS, MAX_RETRIES,
};

// Re-export initialize_kiln_schema from kiln_integration
pub use crate::kiln_integration::initialize_kiln_schema;

// Re-export types needed by external consumers
pub use crate::types::{DatabaseStats, DocumentEmbedding, Record};
