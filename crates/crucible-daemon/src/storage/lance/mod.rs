//! LanceDB integration for Crucible lightweight storage mode
//!
//! Provides native vector search backed by LanceDB - a vector-native database
//! optimized for embedding search.

pub mod vector_index;

pub use vector_index::LanceVectorIndex;
