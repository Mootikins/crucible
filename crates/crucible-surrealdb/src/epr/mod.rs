//! Entity/Property/Relation (EPR) data structures and helpers.
//!
//! This module defines strongly-typed structs that mirror the new schema in
//! `schema_epr.surql` and provides higher-level storage helpers used by the
//! ingestion pipeline.

pub mod ingest;
pub mod schema;
pub mod store;
pub mod types;

pub use ingest::DocumentIngestor;
pub use schema::apply_epr_schema;
pub use store::EprStore;
pub use types::*;
