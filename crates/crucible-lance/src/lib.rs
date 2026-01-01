//! LanceDB integration for Crucible lightweight storage mode
//!
//! Provides embedding cache and vector search backed by LanceDB.

pub mod embedding_cache;
pub mod store;
pub mod vector_search;

pub use store::LanceStore;
pub use vector_search::VectorSearchResult;
