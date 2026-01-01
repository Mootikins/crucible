//! LanceDB integration for Crucible lightweight storage mode
//!
//! Provides embedding cache and vector search backed by LanceDB.

pub mod store;

pub use store::LanceStore;
