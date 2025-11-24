//! Factory functions for creating infrastructure implementations
//!
//! This module is the composition root where concrete types are assembled
//! and returned as trait objects. This enforces dependency inversion at
//! the architectural level.

pub mod storage;
pub mod enrichment;
pub mod merkle;
pub mod pipeline;

pub use storage::{
    create_surrealdb_storage,
    initialize_surrealdb_schema,
    create_surrealdb_enriched_note_store
};
pub use enrichment::create_default_enrichment_service;
pub use merkle::create_surrealdb_merkle_store;
pub use pipeline::create_pipeline;
