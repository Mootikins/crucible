//! LanceDB integration for Crucible lightweight storage mode
//!
//! Provides native vector search backed by LanceDB - a vector-native database
//! optimized for embedding search.

pub mod store;
pub mod vector_index;
pub mod vector_search;

pub use store::LanceStore;
pub use vector_index::LanceVectorIndex;
pub use vector_search::VectorSearchResult;
