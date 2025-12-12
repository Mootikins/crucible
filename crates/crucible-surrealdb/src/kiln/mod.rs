//! Kiln Integration Module
//!
//! This module provides the integration layer between the parser system and SurrealDB.
//! It implements the bridge between ParsedNote structures and the database schema.
//! Includes comprehensive vector embedding support for semantic search and processing.
//!
//! ## Module Structure
//!
//! - `document` - Document CRUD operations (store, retrieve, normalize IDs)
//! - `embedding` - Embedding storage and retrieval
//! - `relations` - Graph relations (wikilinks, embeds)
//! - `search` - Semantic search operations
//! - `stats` - Database statistics
//! - `utils` - Shared utility functions
//!
//! ## Usage
//!
//! All functions are re-exported at the module root for backwards compatibility:
//!
//! ```rust,ignore
//! use crucible_surrealdb::kiln::{
//!     initialize_kiln_schema,
//!     store_parsed_document,
//!     semantic_search,
//! };
//! ```
//!
//! ## Architecture Note
//!
//! This module structure wraps the existing `kiln_integration.rs` file through
//! re-exports. The submodules categorize functionality for easier discovery
//! and future incremental refactoring. The original monolithic file remains
//! as `kiln_integration.rs` for now to minimize disruption.

pub mod document;
pub mod embedding;
pub mod relations;
pub mod search;
pub mod stats;
pub mod utils;

// Re-export all public items for backwards compatibility
pub use document::*;
pub use embedding::*;
pub use relations::*;
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
