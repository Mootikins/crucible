//! Semantic search operations
//!
//! This module re-exports search-related functions from kiln_integration.rs.
//! Future work: Move implementations here for better organization.

// Re-export from legacy kiln_integration module
pub use crate::kiln_integration::{semantic_search, semantic_search_with_reranking};
